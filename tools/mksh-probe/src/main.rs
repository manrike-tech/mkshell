//! `mksh-probe`: prueba de concepto de la integración OSC 133 sin interfaz.
//!
//! Lanza un shell en un PTY (openpty o ConPTY) con la integración de MKShell,
//! ejecuta unos comandos y comprueba que los bloques, códigos de salida y
//! directorios se detectan. Sirve para validar la fase 1 en Windows:
//!
//! ```text
//! cargo run -p mksh-probe -- --shell pwsh
//! cargo run -p mksh-probe -- --shell cmd
//! cargo run -p mksh-probe -- --shell bash --cmd "ls -la" --dump
//! ```

use mksh_core::{BlockId, Origin, PaneEvent, PaneId, Region};
use mksh_session::{PaneFrame, PaneOptions, PaneRuntime};
use mksh_shell::{IntegrationAssets, Profile, ShellKind, detect_profiles};
use serde::Serialize;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
struct Args {
    shell: Option<String>,
    commands: Vec<String>,
    integration: bool,
    json: bool,
    dump: bool,
    timeout: Duration,
}

fn parse_args() -> Result<Args, String> {
    let mut a = Args {
        integration: true,
        timeout: Duration::from_secs(10),
        ..Default::default()
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shell" | "-s" => a.shell = Some(it.next().ok_or("--shell requiere un valor")?),
            "--cmd" | "-c" => a.commands.push(it.next().ok_or("--cmd requiere un valor")?),
            "--no-integration" => a.integration = false,
            "--json" => a.json = true,
            "--dump" => a.dump = true,
            "--timeout" => {
                let s: u64 = it
                    .next()
                    .ok_or("--timeout requiere segundos")?
                    .parse()
                    .map_err(|_| "segundos inválidos")?;
                a.timeout = Duration::from_secs(s);
            }
            "--list" => {
                for p in detect_profiles() {
                    println!("{:<24} {:<10} {}", p.id, format!("{:?}", p.kind), p.program);
                }
                std::process::exit(0);
            }
            "-h" | "--help" => {
                println!(
                    "mksh-probe [--shell <id|nombre|ruta>] [--cmd <comando>]... [--no-integration] \
                     [--json] [--dump] [--timeout <s>] [--list]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("argumento desconocido: {other}")),
        }
    }
    Ok(a)
}

fn pick_profile(wanted: Option<&str>) -> Result<Profile, String> {
    let profiles = detect_profiles();
    let Some(w) = wanted else {
        return profiles
            .into_iter()
            .next()
            .ok_or_else(|| "no se detectó ningún shell".to_string());
    };
    let wl = w.to_ascii_lowercase();
    if let Some(p) = profiles
        .iter()
        .find(|p| p.id == wl || p.name.to_ascii_lowercase() == wl)
    {
        return Ok(p.clone());
    }
    let kind = ShellKind::from_program(&wl);
    if let Some(p) = profiles
        .iter()
        .find(|p| p.kind == kind && kind != ShellKind::Other)
    {
        return Ok(p.clone());
    }
    // Ruta o ejecutable arbitrario.
    Ok(Profile::new(w, w, vec![]))
}

fn default_commands(kind: ShellKind) -> Vec<(String, Option<i32>)> {
    let fail = match kind {
        ShellKind::Pwsh | ShellKind::PowerShell | ShellKind::Cmd => {
            ("cmd /c exit 3".to_string(), Some(3))
        }
        _ => ("false".to_string(), Some(1)),
    };
    let (fail_cmd, mut fail_code) = fail;
    if kind == ShellKind::Cmd {
        // CMD no informa del código de salida en el prompt.
        fail_code = None;
    }
    let cd = if cfg!(windows) { "cd \\" } else { "cd /" };
    let ok_code = if kind == ShellKind::Cmd {
        None
    } else {
        Some(0)
    };
    vec![
        ("echo mkshell-probe".into(), ok_code),
        (fail_cmd, fail_code),
        (cd.into(), None),
    ]
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BlockReport {
    command: String,
    exit_code: Option<i32>,
    expected_exit_code: Option<i32>,
    duration_ms: u64,
    output: String,
    ok: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    shell: String,
    program: String,
    os: &'static str,
    integration_requested: bool,
    integration_injected: bool,
    prompt_detected_ms: Option<u128>,
    cwd: Option<String>,
    bracketed_paste: bool,
    blocks: Vec<BlockReport>,
    ok: bool,
    notes: Vec<String>,
}

struct Probe {
    rx: Receiver<PaneFrame>,
    log: Vec<PaneFrame>,
    cursor: usize,
    timeout: Duration,
    dump: bool,
}

impl Probe {
    fn pump(&mut self, wait: Duration) {
        if let Ok(f) = self.rx.recv_timeout(wait) {
            self.record(f);
        }
        while let Ok(f) = self.rx.try_recv() {
            self.record(f);
        }
    }

    fn record(&mut self, f: PaneFrame) {
        if self.dump {
            match &f {
                PaneFrame::Event(e) => eprintln!("  ev   {e:?}"),
                PaneFrame::Data(d) => eprintln!(
                    "  data {:?}{} {:?}",
                    d.region,
                    if d.alt { "/alt" } else { "" },
                    String::from_utf8_lossy(&d.bytes)
                ),
            }
        }
        self.log.push(f);
    }

    /// Espera el siguiente evento que cumpla `pred` (posterior al cursor).
    fn next_event(&mut self, pred: impl Fn(&PaneEvent) -> bool) -> Option<PaneEvent> {
        let start = Instant::now();
        loop {
            while self.cursor < self.log.len() {
                let i = self.cursor;
                self.cursor += 1;
                match &self.log[i] {
                    PaneFrame::Event(e) if pred(e) => return Some(e.clone()),
                    _ => {}
                }
            }
            if start.elapsed() > self.timeout {
                return None;
            }
            self.pump(Duration::from_millis(50));
        }
    }

    fn output_of(&self, block: BlockId) -> String {
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
        visible_text(&bytes)
    }
}

fn visible_text(bytes: &[u8]) -> String {
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
    p.0.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn run() -> Result<bool, String> {
    let args = parse_args()?;
    let mut profile = pick_profile(args.shell.as_deref())?;
    profile.integration = args.integration;
    let tmp = std::env::temp_dir().join(format!("mksh-probe-{}", std::process::id()));
    let assets =
        IntegrationAssets::install(&tmp).map_err(|e| format!("instalando scripts: {e}"))?;

    let started = Instant::now();
    let rt = PaneRuntime::spawn(
        PaneId::new(),
        profile.clone(),
        Some(&assets),
        PaneOptions::default(),
    )
    .map_err(|e| format!("no se pudo lanzar {}: {e:#}", profile.program))?;
    let (tx, rx) = channel();
    let weak = Arc::downgrade(&rt);
    rt.attach(Arc::new(move |f: &PaneFrame| {
        if let Some(rt) = weak.upgrade() {
            rt.ack(f.payload_len());
        }
        tx.send(f.clone()).is_ok()
    }));
    let mut probe = Probe {
        rx,
        log: Vec::new(),
        cursor: 0,
        timeout: args.timeout,
        dump: args.dump,
    };
    let mut report = Report {
        shell: profile.name.clone(),
        program: profile.program.clone(),
        os: std::env::consts::OS,
        integration_requested: args.integration,
        integration_injected: rt.info().integrated,
        prompt_detected_ms: None,
        cwd: None,
        bracketed_paste: false,
        blocks: Vec::new(),
        ok: false,
        notes: Vec::new(),
    };

    if probe
        .next_event(|e| matches!(e, PaneEvent::Ready { depth: 0, .. }))
        .is_none()
    {
        report.notes.push(format!(
            "No llegó ningún prompt con marcas OSC 133 en {:?}. Si es Windows, ConPTY puede estar \
             filtrando las secuencias: prueba con un conpty.dll reciente junto al ejecutable.",
            args.timeout
        ));
        // Lo que escribió el shell al arrancar suele explicar el bloqueo
        // (una pregunta de compinit, un error en el rc…).
        let startup: Vec<u8> = probe
            .log
            .iter()
            .filter_map(|f| match f {
                PaneFrame::Data(d) if d.region == Region::Startup => Some(d.bytes.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let text = visible_text(&startup);
        if !text.is_empty() {
            report.notes.push(format!(
                "salida de arranque: {}",
                text.chars().take(400).collect::<String>()
            ));
        }
        return finish(report, &args, &rt);
    }
    report.prompt_detected_ms = Some(started.elapsed().as_millis());

    let commands: Vec<(String, Option<i32>)> = if args.commands.is_empty() {
        default_commands(profile.kind)
    } else {
        args.commands.iter().map(|c| (c.clone(), None)).collect()
    };
    let check_codes = args.commands.is_empty();

    for (cmd, expected) in commands {
        // Se espera a que el shell esté listo antes de cada comando.
        if !rt_ready(&mut probe) {
            report.notes.push(format!(
                "El shell no volvió a estar listo antes de «{cmd}»."
            ));
            break;
        }
        rt.run_command(&cmd, Origin::User)
            .map_err(|e| e.to_string())?;
        let Some(PaneEvent::BlockStarted { block, .. }) =
            probe.next_event(|e| matches!(e, PaneEvent::BlockStarted { .. }))
        else {
            report.notes.push(format!(
                "«{cmd}» no generó un bloque (falta la marca C o el eco)."
            ));
            break;
        };
        let Some(PaneEvent::BlockFinished {
            exit_code,
            duration_ms,
            ..
        }) = probe
            .next_event(|e| matches!(e, PaneEvent::BlockFinished { block: b, .. } if *b == block))
        else {
            report
                .notes
                .push(format!("«{cmd}» no terminó (falta la marca D)."));
            break;
        };
        let ok = !check_codes || expected.is_none() || exit_code == expected;
        report.blocks.push(BlockReport {
            command: cmd,
            exit_code,
            expected_exit_code: expected,
            duration_ms,
            output: probe.output_of(block),
            ok,
        });
    }
    // Se espera al prompt final para recoger el último cwd.
    let _ = probe.next_event(|e| matches!(e, PaneEvent::Ready { depth: 0, .. }));
    report.bracketed_paste = rt.info().integrated
        && probe.log.iter().any(|f| {
            matches!(f,
        PaneFrame::Data(d) if d.bytes.windows(8).any(|w| w == b"\x1b[?2004h"))
        });
    report.cwd = probe.log.iter().rev().find_map(|f| match f {
        PaneFrame::Event(PaneEvent::CwdChanged { cwd, .. }) => Some(cwd.clone()),
        _ => None,
    });
    finish(report, &args, &rt)
}

fn rt_ready(probe: &mut Probe) -> bool {
    probe
        .next_event(|e| matches!(e, PaneEvent::Ready { depth: 0, .. }))
        .is_some()
        || {
            // El Ready pudo llegar antes del BlockFinished ya consumido.
            probe
                .log
                .iter()
                .rev()
                .take(20)
                .any(|f| matches!(f, PaneFrame::Event(PaneEvent::Ready { .. })))
        }
}

fn finish(mut report: Report, args: &Args, rt: &Arc<PaneRuntime>) -> Result<bool, String> {
    report.ok = report.prompt_detected_ms.is_some()
        && !report.blocks.is_empty()
        && report.blocks.iter().all(|b| b.ok)
        && report.notes.is_empty();
    rt.kill();
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
        return Ok(report.ok);
    }
    println!(
        "MKShell probe · {} ({}) · {}",
        report.shell, report.program, report.os
    );
    println!(
        "  integración: {}",
        if report.integration_injected {
            "inyectada"
        } else {
            "no inyectada (TTY clásico)"
        }
    );
    match report.prompt_detected_ms {
        Some(ms) => println!("  [ok] prompt OSC 133 detectado en {ms} ms"),
        None => println!("  [!!] prompt OSC 133 no detectado"),
    }
    for b in &report.blocks {
        let mark = if b.ok { "ok" } else { "!!" };
        let code = b
            .exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "desconocido".into());
        let first = b.output.lines().next().unwrap_or("");
        println!(
            "  [{mark}] «{}» → código {code}, {} ms, salida: {first:?}",
            b.command, b.duration_ms
        );
    }
    if let Some(cwd) = &report.cwd {
        println!("  cwd: {cwd}");
    }
    for n in &report.notes {
        println!("  nota: {n}");
    }
    println!(
        "  resultado: {}",
        if report.ok {
            "integración operativa"
        } else {
            "FALLO"
        }
    );
    Ok(report.ok)
}

fn main() {
    match run() {
        Ok(true) => std::process::exit(0),
        Ok(false) => std::process::exit(1),
        Err(e) => {
            eprintln!("mksh-probe: {e}");
            std::process::exit(2);
        }
    }
}
