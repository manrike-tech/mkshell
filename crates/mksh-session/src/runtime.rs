//! Panel en ejecución: PTY + segmentador + canal hacia la UI.

use crate::flow::FlowGate;
use crate::frame::PaneFrame;
use crate::tracker::{Tracker, TrackerOutput};
use anyhow::Result;
use mksh_core::{BlockId, Origin, PaneEvent, PaneId};
use mksh_pty::{PtyProcess, SpawnSpec};
use mksh_shell::{IntegrationAssets, Profile, ShellKind, prepare_launch};
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

/// Bytes máximos retenidos mientras no hay UI conectada.
const BACKLOG_CAP: usize = 8 * 1024 * 1024;

/// Destino de los mensajes de un panel. Devuelve `false` si se desconectó.
pub trait FrameSink: Send + Sync + 'static {
    fn send(&self, frame: &PaneFrame) -> bool;
}

impl<F> FrameSink for F
where
    F: Fn(&PaneFrame) -> bool + Send + Sync + 'static,
{
    fn send(&self, frame: &PaneFrame) -> bool {
        self(frame)
    }
}

/// Comando terminado, para el historial y la auditoría.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockRecord {
    pub pane: PaneId,
    pub profile_id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub host: Option<String>,
    pub depth: usize,
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub origin: Origin,
}

pub type BlockHook = Arc<dyn Fn(BlockRecord) + Send + Sync>;
pub type ExitHook = Box<dyn FnOnce(PaneId, Option<i32>) + Send>;

/// Opciones de creación de un panel.
pub struct PaneOptions {
    pub cols: u16,
    pub rows: u16,
    pub cwd: Option<PathBuf>,
    pub on_exit: Option<ExitHook>,
    pub on_block: Option<BlockHook>,
}

impl Default for PaneOptions {
    fn default() -> Self {
        Self {
            cols: 120,
            rows: 32,
            cwd: None,
            on_exit: None,
            on_block: None,
        }
    }
}

#[derive(Default)]
struct Slot {
    sink: Option<Arc<dyn FrameSink>>,
    backlog: VecDeque<PaneFrame>,
    backlog_bytes: usize,
}

impl Slot {
    fn deliver(&mut self, frame: PaneFrame) {
        if let Some(sink) = &self.sink {
            if sink.send(&frame) {
                return;
            }
            self.sink = None;
        }
        self.backlog_bytes += frame.payload_len();
        self.backlog.push_back(frame);
        while self.backlog_bytes > BACKLOG_CAP {
            match self.backlog.pop_front() {
                Some(f) => self.backlog_bytes -= f.payload_len(),
                None => break,
            }
        }
    }
}

/// Información pública de un panel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaneInfo {
    pub id: PaneId,
    pub profile_id: String,
    pub profile_name: String,
    pub kind: ShellKind,
    /// `true` si se lanzó con integración (se esperan bloques).
    pub integrated: bool,
    pub cwd: Option<String>,
    pub pid: Option<u32>,
}

/// Panel vivo.
pub struct PaneRuntime {
    pub id: PaneId,
    pub profile: Profile,
    integrated: bool,
    pty: Arc<PtyProcess>,
    tracker: Arc<Mutex<Tracker>>,
    gate: Arc<FlowGate>,
    slot: Arc<Mutex<Slot>>,
}

impl std::fmt::Debug for PaneRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PaneRuntime")
            .field("id", &self.id)
            .field("profile", &self.profile.id)
            .finish()
    }
}

impl PaneRuntime {
    /// Lanza el shell del perfil y arranca los hilos de lectura y espera.
    pub fn spawn(
        id: PaneId,
        profile: Profile,
        assets: Option<&IntegrationAssets>,
        mut opts: PaneOptions,
    ) -> Result<Arc<Self>> {
        let plan = prepare_launch(&profile, assets, &|k| std::env::var(k).ok());
        let cwd = opts
            .cwd
            .take()
            .filter(|p| p.is_dir())
            .or_else(|| {
                profile
                    .cwd
                    .as_deref()
                    .map(expand_home)
                    .filter(|p| p.is_dir())
            })
            .or_else(dirs_home);
        let spec = SpawnSpec {
            program: plan.program.clone(),
            args: plan.args.clone(),
            env: plan.env.clone(),
            env_remove: plan.env_remove.clone(),
            cwd,
            cols: opts.cols,
            rows: opts.rows,
        };
        let (pty, reader) = PtyProcess::spawn(&spec)?;
        let rt = Arc::new(Self {
            id,
            integrated: plan.integrated,
            profile,
            pty: Arc::new(pty),
            tracker: Arc::new(Mutex::new(Tracker::new())),
            gate: Arc::new(FlowGate::new()),
            slot: Arc::new(Mutex::new(Slot::default())),
        });
        rt.start_threads(reader, opts.on_exit.take(), opts.on_block.take());
        Ok(rt)
    }

    fn start_threads(
        self: &Arc<Self>,
        mut reader: Box<dyn Read + Send>,
        on_exit: Option<ExitHook>,
        on_block: Option<BlockHook>,
    ) {
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let tracker = self.tracker.clone();
        let slot = self.slot.clone();
        let gate = self.gate.clone();
        let pane = self.id;
        let profile_id = self.profile.id.clone();

        std::thread::Builder::new()
            .name(format!("mksh-pty-read-{pane}"))
            .spawn(move || {
                // Metadatos de bloques abiertos para el historial.
                let mut open: HashMap<BlockId, OpenMeta> = HashMap::new();
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => n,
                    };
                    let outputs = tracker.lock().feed(&buf[..n]);
                    let mut bytes = 0usize;
                    {
                        let mut slot = slot.lock();
                        for o in outputs {
                            if let Some(hook) = &on_block {
                                track_block(&o, &mut open, hook, pane, &profile_id);
                            }
                            let frame = PaneFrame::from(o);
                            bytes += frame.payload_len();
                            slot.deliver(frame);
                        }
                    }
                    gate.add(bytes);
                    gate.wait_for_room();
                }
                let _ = done_tx.send(());
            })
            .expect("hilo lector");

        let pty = self.pty.clone();
        let slot = self.slot.clone();
        let gate = self.gate.clone();
        std::thread::Builder::new()
            .name(format!("mksh-pty-wait-{pane}"))
            .spawn(move || {
                let code = pty.wait();
                // Se da tiempo a leer la salida pendiente. Si el lector sigue
                // bloqueado (nietos con el PTY abierto, o ConPTY), se fuerza.
                if done_rx.recv_timeout(Duration::from_millis(500)).is_err() {
                    pty.kill_tree();
                    pty.close();
                    let _ = done_rx.recv_timeout(Duration::from_secs(2));
                }
                slot.lock()
                    .deliver(PaneFrame::Event(PaneEvent::Exited { code }));
                gate.close();
                if let Some(hook) = on_exit {
                    hook(pane, code);
                }
            })
            .expect("hilo de espera");
    }

    /// Conecta la UI. Se le envía primero lo acumulado mientras no había UI.
    pub fn attach(&self, sink: Arc<dyn FrameSink>) {
        let mut slot = self.slot.lock();
        let backlog: Vec<PaneFrame> = slot.backlog.drain(..).collect();
        slot.backlog_bytes = 0;
        // Lo enviado a una UI anterior nunca se confirmará.
        self.gate.reset();
        let mut pending = 0;
        let mut ok = true;
        for f in &backlog {
            pending += f.payload_len();
            if ok && !sink.send(f) {
                ok = false;
            }
        }
        self.gate.add(pending);
        slot.sink = ok.then_some(sink);
    }

    /// Desconecta la UI; la salida se acumula hasta la siguiente conexión.
    pub fn detach(&self) {
        self.slot.lock().sink = None;
    }

    /// Envía un comando al shell activo como si se tecleara y se pulsara Enter.
    ///
    /// Si ya hay un comando en ejecución, el texto se entrega como entrada
    /// estándar de ese programa (p. ej. responder a una confirmación).
    /// Devuelve `true` si se registró como comando nuevo.
    pub fn run_command(&self, text: &str, origin: Origin) -> Result<bool> {
        let text = text.trim_end_matches(['\r', '\n']);
        let mut tracker = self.tracker.lock();
        if tracker.is_running() {
            drop(tracker);
            self.pty.write_all(format!("{text}\r").as_bytes())?;
            return Ok(false);
        }
        let paste = tracker.bracketed_paste();
        tracker.expect_command(text.to_string(), origin);
        drop(tracker);
        self.pty.write_all(&encode_line(text, paste))?;
        Ok(true)
    }

    /// Activa la integración en el shell en primer plano (sesión SSH, WSL o
    /// shell lanzado sin integración). Requiere consentimiento previo en la UI.
    pub fn enable_integration(&self) -> Result<()> {
        let paste = self.tracker.lock().bracketed_paste();
        self.pty
            .write_all(&encode_line(&mksh_shell::ssh::bootstrap_line(), paste))
    }

    /// Bytes crudos de teclado (desde xterm.js).
    pub fn write_input(&self, data: &[u8]) -> Result<()> {
        self.pty.write_all(data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.pty.resize(cols, rows)
    }

    /// La UI confirma que pintó `n` bytes.
    pub fn ack(&self, n: usize) {
        self.gate.ack(n);
    }

    /// `Some(true)` si el programa en primer plano desactivó el eco (contraseña).
    pub fn echo_disabled(&self) -> Option<bool> {
        self.pty.echo_disabled()
    }

    pub fn info(&self) -> PaneInfo {
        PaneInfo {
            id: self.id,
            profile_id: self.profile.id.clone(),
            profile_name: self.profile.name.clone(),
            kind: self.profile.kind,
            integrated: self.integrated,
            cwd: self.tracker.lock().root_cwd().map(String::from),
            pid: self.pty.pid(),
        }
    }

    /// Directorio actual del shell raíz, si se conoce y existe localmente.
    pub fn local_cwd(&self) -> Option<PathBuf> {
        self.tracker
            .lock()
            .root_cwd()
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
    }

    /// Termina el shell y sus descendientes.
    pub fn kill(&self) {
        self.gate.close();
        self.pty.kill_tree();
    }
}

/// Datos de un bloque abierto: comando, cwd, host, profundidad y origen.
type OpenMeta = (
    Option<String>,
    Option<String>,
    Option<String>,
    usize,
    Origin,
);

fn track_block(
    o: &TrackerOutput,
    open: &mut HashMap<BlockId, OpenMeta>,
    hook: &BlockHook,
    pane: PaneId,
    profile_id: &str,
) {
    match o {
        TrackerOutput::Event(PaneEvent::BlockStarted {
            block,
            command,
            cwd,
            host,
            depth,
            origin,
            ..
        }) => {
            open.insert(
                *block,
                (
                    command.clone(),
                    cwd.clone(),
                    host.clone(),
                    *depth,
                    origin.clone(),
                ),
            );
        }
        TrackerOutput::Event(PaneEvent::BlockFinished {
            block,
            exit_code,
            duration_ms,
        }) => {
            if let Some((Some(command), cwd, host, depth, origin)) = open.remove(block) {
                hook(BlockRecord {
                    pane,
                    profile_id: profile_id.to_string(),
                    command,
                    cwd,
                    host,
                    depth,
                    exit_code: *exit_code,
                    duration_ms: *duration_ms,
                    origin,
                });
            }
        }
        _ => {}
    }
}

/// Codifica una línea para el shell: *bracketed paste* si el shell lo activó
/// (el texto multilínea se inserta completo) y Enter (`\r`).
fn encode_line(text: &str, bracketed_paste: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(text.len() + 16);
    if bracketed_paste {
        v.extend_from_slice(b"\x1b[200~");
        v.extend_from_slice(text.as_bytes());
        v.extend_from_slice(b"\x1b[201~");
    } else {
        v.extend_from_slice(text.replace('\n', "\r").as_bytes());
    }
    v.push(b'\r');
    v
}

fn expand_home(p: &str) -> PathBuf {
    match (p.strip_prefix("~"), dirs_home()) {
        (Some(rest), Some(home)) => home.join(rest.trim_start_matches(['/', '\\'])),
        _ => PathBuf::from(p),
    }
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_lines() {
        assert_eq!(encode_line("ls", false), b"ls\r");
        assert_eq!(encode_line("a\nb", false), b"a\rb\r");
        assert_eq!(encode_line("a\nb", true), b"\x1b[200~a\nb\x1b[201~\r");
    }
}
