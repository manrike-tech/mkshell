//! Pruebas de integración con shells reales (se omiten si no están instalados).
//!
//! Lanzan cada shell en un PTY con la integración de MKShell y comprueban que
//! los bloques, códigos de salida, cwd y shells anidados se detectan.

#![cfg(unix)]

use mksh_core::{BlockId, Origin, PaneEvent, PaneId, Region};
use mksh_session::{PaneFrame, PaneOptions, PaneRuntime};
use mksh_shell::{IntegrationAssets, Profile};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(15);

struct Harness {
    rt: Arc<PaneRuntime>,
    rx: Receiver<PaneFrame>,
    log: Vec<PaneFrame>,
    _tmp: TempDir,
}

struct TempDir(std::path::PathBuf);
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn which(name: &str) -> Option<String> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(name))
        .find(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
}

impl Harness {
    fn start(shell: &str, args: &[&str]) -> Option<Self> {
        let program = which(shell)?;
        let tmp = std::env::temp_dir().join(format!(
            "mksh-it-{shell}-{}-{}",
            std::process::id(),
            PaneId::new()
        ));
        std::fs::create_dir_all(&tmp).ok()?;
        let assets = IntegrationAssets::install(&tmp.join("assets")).ok()?;
        let mut profile =
            Profile::new(shell, program, args.iter().map(|s| s.to_string()).collect());
        // HOME vacío: las pruebas no dependen de la configuración de quien las ejecuta.
        let home = tmp.join("home");
        std::fs::create_dir_all(&home).ok()?;
        profile
            .env
            .insert("HOME".into(), home.to_string_lossy().into_owned());
        profile.env.insert(
            "XDG_CONFIG_HOME".into(),
            home.join(".config").to_string_lossy().into_owned(),
        );
        let opts = PaneOptions {
            cols: 100,
            rows: 30,
            cwd: Some(tmp.clone()),
            ..Default::default()
        };
        let rt = PaneRuntime::spawn(PaneId::new(), profile, Some(&assets), opts).ok()?;
        let (tx, rx) = channel();
        let ack_rt = Arc::downgrade(&rt);
        rt.attach(Arc::new(move |f: &PaneFrame| {
            if let Some(rt) = ack_rt.upgrade() {
                rt.ack(f.payload_len());
            }
            tx.send(f.clone()).is_ok()
        }));
        Some(Self {
            rt,
            rx,
            log: Vec::new(),
            _tmp: TempDir(tmp),
        })
    }

    fn wait_event(&mut self, what: &str, pred: impl Fn(&PaneEvent) -> bool) -> PaneEvent {
        // Primero en lo ya recibido y no consumido.
        let start = Instant::now();
        loop {
            while let Ok(f) = self.rx.try_recv() {
                self.log.push(f);
            }
            if let Some(pos) = self
                .log
                .iter()
                .position(|f| matches!(f, PaneFrame::Event(e) if pred(e)))
            {
                let PaneFrame::Event(e) = self.log[pos].clone() else {
                    unreachable!()
                };
                // Se marca como consumido sustituyéndolo por un evento neutro.
                self.log[pos] = PaneFrame::Event(PaneEvent::PromptStart { depth: usize::MAX });
                return e;
            }
            if start.elapsed() > TIMEOUT {
                panic!("timeout esperando {what}; registro:\n{}", self.dump());
            }
            if let Ok(f) = self.rx.recv_timeout(Duration::from_millis(50)) {
                self.log.push(f);
            }
        }
    }

    fn ready(&mut self, depth: usize) {
        self.wait_event(
            "Ready",
            |e| matches!(e, PaneEvent::Ready { depth: d, .. } if *d == depth),
        );
    }

    fn run(&mut self, cmd: &str) -> (BlockId, Option<i32>, String) {
        self.rt.run_command(cmd, Origin::User).unwrap();
        let PaneEvent::BlockStarted { block, command, .. } = self
            .wait_event(&format!("BlockStarted({cmd})"), |e| {
                matches!(e, PaneEvent::BlockStarted { .. })
            })
        else {
            unreachable!()
        };
        assert_eq!(command.as_deref(), Some(cmd));
        let PaneEvent::BlockFinished { exit_code, .. } = self.wait_event(
            &format!("BlockFinished({cmd})"),
            |e| matches!(e, PaneEvent::BlockFinished { block: b, .. } if *b == block),
        ) else {
            unreachable!()
        };
        (block, exit_code, self.output(block))
    }

    fn output(&self, block: BlockId) -> String {
        let bytes: Vec<u8> = self
            .log
            .iter()
            .filter_map(|f| match f {
                PaneFrame::Data(d) if d.region == Region::Output && d.block == Some(block) => {
                    Some(d.bytes.clone())
                }
                _ => None,
            })
            .flatten()
            .collect();
        strip(&bytes)
    }

    fn dump(&self) -> String {
        self.log
            .iter()
            .map(|f| match f {
                PaneFrame::Event(e) => format!("EV {e:?}"),
                PaneFrame::Data(d) => format!(
                    "{:?}/{:?} {:?}",
                    d.region,
                    d.block,
                    String::from_utf8_lossy(&d.bytes)
                ),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.rt.kill();
    }
}

/// Texto visible sin secuencias de escape (suficiente para las aserciones).
fn strip(bytes: &[u8]) -> String {
    struct P(String);
    impl vte::Perform for P {
        fn print(&mut self, c: char) {
            self.0.push(c);
        }
        fn execute(&mut self, b: u8) {
            if b == b'\n' {
                self.0.push('\n');
            }
        }
    }
    let mut p = P(String::new());
    vte::Parser::new().advance(&mut p, bytes);
    p.0
}

fn basic_flow(shell: &str, args: &[&str]) {
    let Some(mut h) = Harness::start(shell, args) else {
        eprintln!("{shell} no instalado: prueba omitida");
        return;
    };
    h.ready(0);

    let (_, code, out) = h.run("echo hola-mkshell");
    assert_eq!(code, Some(0));
    assert_eq!(out.trim(), "hola-mkshell");

    let (_, code, _) = h.run("false");
    assert_eq!(code, Some(1), "{}", h.dump());

    let (_, code, out) = h.run("ls /no-existe-mksh");
    assert_ne!(code, Some(0));
    assert!(
        out.contains("no-existe-mksh"),
        "stderr llega al bloque: {out:?}"
    );

    h.run("cd /tmp");
    let PaneEvent::CwdChanged { cwd, .. } = h.wait_event(
        "CwdChanged /tmp",
        |e| matches!(e, PaneEvent::CwdChanged { cwd, .. } if cwd == "/tmp"),
    ) else {
        unreachable!()
    };
    assert_eq!(cwd, "/tmp");

    // Comando multilínea (bracketed paste).
    let (_, code, out) = h.run("echo uno\necho dos");
    assert_eq!(code, Some(0));
    assert!(
        out.contains("uno") && out.contains("dos"),
        "{out:?}\n{}",
        h.dump()
    );
}

#[test]
fn bash_integration() {
    basic_flow("bash", &[]);
}

#[test]
fn zsh_integration() {
    basic_flow("zsh", &[]);
}

#[test]
fn fish_integration() {
    // Fish ejecuta las líneas de un pegado multilínea por separado: dos bloques.
    let Some(mut h) = Harness::start("fish", &[]) else {
        eprintln!("fish no instalado: prueba omitida");
        return;
    };
    h.ready(0);
    let (_, code, out) = h.run("echo hola-fish");
    assert_eq!((code, out.trim()), (Some(0), "hola-fish"));
    let (_, code, _) = h.run("false");
    assert_eq!(code, Some(1));
}

/// Simula `ssh`: un bash anidado sin integración al que se le inyecta el
/// arranque en memoria, igual que en un host remoto.
fn nested_injection(inner: &str, inner_args: &str) {
    let Some(mut h) = Harness::start("bash", &[]) else {
        return;
    };
    if which(inner).is_none() {
        eprintln!("{inner} no instalado: prueba omitida");
        return;
    }
    h.ready(0);
    let launch = format!("{inner} {inner_args}");
    h.rt.run_command(&launch, Origin::User).unwrap();
    let PaneEvent::BlockStarted { block: outer, .. } = h.wait_event("bloque externo", |e| {
        matches!(e, PaneEvent::BlockStarted { .. })
    }) else {
        unreachable!()
    };
    std::thread::sleep(Duration::from_millis(500));
    h.rt.enable_integration().unwrap();
    h.wait_event("ShellEntered", |e| {
        matches!(e, PaneEvent::ShellEntered { depth: 1 })
    });
    h.ready(1);

    let (block, code, out) = h.run("echo dentro");
    assert_eq!((code, out.trim()), (Some(0), "dentro"));
    assert_ne!(block, outer);

    // Al salir del shell anidado se cierra el bloque externo.
    h.rt.run_command("exit 3", Origin::User).unwrap();
    h.wait_event("ShellExited", |e| {
        matches!(e, PaneEvent::ShellExited { depth: 1 })
    });
    let PaneEvent::BlockFinished { exit_code, .. } = h.wait_event(
        "fin del bloque externo",
        |e| matches!(e, PaneEvent::BlockFinished { block, .. } if *block == outer),
    ) else {
        unreachable!()
    };
    assert_eq!(exit_code, Some(3));
}

#[test]
fn injection_into_nested_bash() {
    nested_injection("bash", "--norc --noprofile -i");
}

#[test]
fn injection_into_nested_zsh() {
    nested_injection("zsh", "-f -i");
}

#[test]
fn large_output_with_flow_control() {
    let Some(mut h) = Harness::start("bash", &[]) else {
        return;
    };
    h.ready(0);
    // ~6 MiB: supera la marca alta, así que el lector debe pausar y reanudar.
    let (block, code, _) = h.run("head -c 6000000 /dev/zero | tr '\\0' 'x' | fold -w 100");
    assert_eq!(code, Some(0));
    let total: usize = h
        .log
        .iter()
        .filter_map(|f| match f {
            PaneFrame::Data(d) if d.block == Some(block) => Some(d.bytes.len()),
            _ => None,
        })
        .sum();
    // 6 000 000 'x' + 60 000 saltos de línea (\r\n por el PTY)
    assert!(total >= 6_000_000, "bytes recibidos: {total}");
}

#[test]
fn exit_event_when_shell_exits() {
    let Some(mut h) = Harness::start("bash", &[]) else {
        return;
    };
    h.ready(0);
    h.rt.run_command("exit 7", Origin::User).unwrap();
    let PaneEvent::Exited { code } =
        h.wait_event("Exited", |e| matches!(e, PaneEvent::Exited { .. }))
    else {
        unreachable!()
    };
    assert_eq!(code, Some(7));
}
