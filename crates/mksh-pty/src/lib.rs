//! PTY multiplataforma para MKShell.
//!
//! Envuelve `portable-pty` (openpty en Unix, ConPTY en Windows) y añade lo que
//! un multiplexor necesita: terminar el árbol completo de procesos al cerrar un
//! panel (grupo de procesos en Unix, Job Object en Windows) y una API mínima.

use anyhow::{Context, Result};
use portable_pty::{Child, ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(windows)]
mod job;

/// Qué lanzar en el PTY.
#[derive(Debug, Clone, Default)]
pub struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
    /// Variables añadidas o sobrescritas sobre el entorno heredado.
    pub env: Vec<(String, String)>,
    /// Variables a eliminar del entorno heredado.
    pub env_remove: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// Proceso hijo vivo dentro de un PTY.
pub struct PtyProcess {
    /// `None` tras [`PtyProcess::close`]. En Windows, cerrar el pseudoconsole
    /// es lo que provoca el EOF del lector.
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    writer: Mutex<Box<dyn Write + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    /// Permite matar el proceso aunque otro hilo esté bloqueado en `wait`.
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    pid: Option<u32>,
    #[cfg(windows)]
    _job: Option<job::Job>,
}

impl std::fmt::Debug for PtyProcess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyProcess")
            .field("pid", &self.pid)
            .finish()
    }
}

impl PtyProcess {
    /// Lanza el programa y devuelve el proceso y el lector de su salida.
    ///
    /// El lector bloquea: debe consumirse desde un hilo dedicado.
    pub fn spawn(spec: &SpawnSpec) -> Result<(Self, Box<dyn Read + Send>)> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: spec.rows.max(1),
                cols: spec.cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("no se pudo abrir el PTY")?;

        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        for key in &spec.env_remove {
            cmd.env_remove(key);
        }
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if let Some(cwd) = &spec.cwd {
            cmd.cwd(cwd);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("no se pudo lanzar {}", spec.program))?;
        // El lado esclavo ya no hace falta en este proceso: cerrarlo permite
        // detectar EOF cuando el hijo termina.
        drop(pair.slave);

        let pid = child.process_id();
        let killer = child.clone_killer();
        #[cfg(windows)]
        let job = job::Job::for_child(child.as_ref()).ok();

        let reader = pair.master.try_clone_reader().context("lector del PTY")?;
        let writer = pair.master.take_writer().context("escritor del PTY")?;
        Ok((
            Self {
                master: Mutex::new(Some(pair.master)),
                writer: Mutex::new(writer),
                child: Mutex::new(child),
                killer: Mutex::new(killer),
                pid,
                #[cfg(windows)]
                _job: job,
            },
            reader,
        ))
    }

    /// PID del proceso principal (el shell).
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Escribe bytes en la entrada del PTY (teclas, comandos).
    pub fn write_all(&self, data: &[u8]) -> Result<()> {
        let mut w = self.writer.lock().expect("writer envenenado");
        w.write_all(data)?;
        w.flush()?;
        Ok(())
    }

    /// Informa al kernel/ConPTY del nuevo tamaño.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        match self.master.lock().expect("master envenenado").as_ref() {
            Some(m) => m.resize(PtySize {
                rows: rows.max(1),
                cols: cols.max(1),
                pixel_width: 0,
                pixel_height: 0,
            }),
            None => Ok(()),
        }
    }

    /// Indica si el lado esclavo tiene el eco desactivado (p. ej. al pedir una
    /// contraseña). Solo disponible en Unix; en Windows devuelve `None`.
    pub fn echo_disabled(&self) -> Option<bool> {
        #[cfg(unix)]
        {
            let master = self.master.lock().ok()?;
            let termios = master.as_ref()?.get_termios()?;
            Some(!termios.local_flags.contains(nix_echo()))
        }
        #[cfg(not(unix))]
        {
            None
        }
    }

    /// Estado de salida si el proceso ya terminó (no bloquea).
    pub fn try_exit_code(&self) -> Option<i32> {
        let mut child = self.child.lock().ok()?;
        match child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code() as i32),
            _ => None,
        }
    }

    /// Espera a que el proceso termine y devuelve su código.
    pub fn wait(&self) -> Option<i32> {
        let mut child = self.child.lock().ok()?;
        child.wait().ok().map(|s| s.exit_code() as i32)
    }

    /// Termina el proceso y todos sus descendientes.
    ///
    /// En Unix envía SIGHUP al grupo de procesos (el shell es líder de sesión) y,
    /// si sigue vivo, SIGKILL. En Windows cierra el Job Object (kill-on-close)
    /// y como respaldo termina el proceso principal.
    pub fn kill_tree(&self) {
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            unix_kill_group(pid as libc::pid_t);
        }
        if let Ok(mut killer) = self.killer.lock() {
            let _ = killer.kill();
        }
    }

    /// Cierra el lado maestro del PTY. El lector recibe EOF (en Windows es la
    /// única forma de desbloquearlo tras la salida del proceso).
    pub fn close(&self) {
        if let Ok(mut m) = self.master.lock() {
            m.take();
        }
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        let finished = self
            .child
            .try_lock()
            .ok()
            .and_then(|mut c| c.try_wait().ok().flatten())
            .is_some();
        if !finished {
            self.kill_tree();
        }
    }
}

#[cfg(unix)]
fn nix_echo() -> nix::sys::termios::LocalFlags {
    nix::sys::termios::LocalFlags::ECHO
}

#[cfg(unix)]
fn unix_kill_group(pid: libc::pid_t) {
    // SAFETY: llamadas POSIX sin punteros; un pid inexistente devuelve error.
    unsafe {
        libc::killpg(pid, libc::SIGHUP);
    }
    for _ in 0..20 {
        std::thread::sleep(std::time::Duration::from_millis(10));
        // kill(pid, 0) comprueba existencia sin enviar señal.
        if unsafe { libc::killpg(pid, 0) } != 0 {
            return;
        }
    }
    unsafe {
        libc::killpg(pid, libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn read_until(reader: &mut dyn Read, needle: &str, timeout: Duration) -> String {
        let start = Instant::now();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while start.elapsed() < timeout {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    out.extend_from_slice(&buf[..n]);
                    if String::from_utf8_lossy(&out).contains(needle) {
                        break;
                    }
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    #[cfg(unix)]
    #[test]
    fn spawns_and_echoes() {
        let spec = SpawnSpec {
            program: "/bin/sh".into(),
            args: vec![
                "-c".into(),
                "printf 'hola-%s' \"$MKSH_TEST\"; read x; printf 'leido-%s' \"$x\"".into(),
            ],
            env: vec![("MKSH_TEST".into(), "pty".into())],
            cols: 80,
            rows: 24,
            ..Default::default()
        };
        let (proc_, mut reader) = PtyProcess::spawn(&spec).unwrap();
        let out = read_until(reader.as_mut(), "hola-pty", Duration::from_secs(5));
        assert!(out.contains("hola-pty"), "salida: {out:?}");
        proc_.write_all(b"abc\r").unwrap();
        let out = read_until(reader.as_mut(), "leido-abc", Duration::from_secs(5));
        assert!(out.contains("leido-abc"), "salida: {out:?}");
        assert_eq!(proc_.wait(), Some(0));
    }

    #[cfg(unix)]
    #[test]
    fn kill_tree_terminates_children() {
        let spec = SpawnSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "sleep 300 & echo hijo=$!; wait".into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        };
        let (proc_, mut reader) = PtyProcess::spawn(&spec).unwrap();
        let out = read_until(reader.as_mut(), "\n", Duration::from_secs(5));
        let child_pid: i32 = out
            .split("hijo=")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .and_then(|s| s.parse().ok())
            .expect("pid del hijo");
        proc_.kill_tree();
        std::thread::sleep(Duration::from_millis(300));
        // Un proceso zombi (pendiente de que init lo recoja) cuenta como terminado.
        let state = std::fs::read_to_string(format!("/proc/{child_pid}/stat"))
            .ok()
            .and_then(|s| s.rsplit(')').next().map(|r| r.trim().chars().next()))
            .flatten();
        let alive = match state {
            Some('Z') | None => false,
            Some(_) => true,
        };
        assert!(!alive, "el nieto {child_pid} sigue vivo (estado {state:?})");
    }

    #[cfg(unix)]
    #[test]
    fn detects_echo_off() {
        let spec = SpawnSpec {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "stty -echo; echo listo; sleep 2".into()],
            cols: 80,
            rows: 24,
            ..Default::default()
        };
        let (proc_, mut reader) = PtyProcess::spawn(&spec).unwrap();
        let _ = read_until(reader.as_mut(), "listo", Duration::from_secs(5));
        assert_eq!(proc_.echo_disabled(), Some(true));
        proc_.kill_tree();
    }
}
