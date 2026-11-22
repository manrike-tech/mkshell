use super::*;

const BEL: &str = "\x07";

fn a(aid: &str) -> String {
    format!("\x1b]133;A;aid={aid}{BEL}")
}
fn b() -> String {
    format!("\x1b]133;B{BEL}")
}
fn c(aid: &str) -> String {
    format!("\x1b]133;C;aid={aid}{BEL}")
}
fn d(code: i32, aid: &str) -> String {
    format!("\x1b]133;D;{code};aid={aid}{BEL}")
}

fn events(out: &[TrackerOutput]) -> Vec<PaneEvent> {
    out.iter()
        .filter_map(|o| match o {
            TrackerOutput::Event(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}

fn concat(out: &[TrackerOutput]) -> Vec<u8> {
    out.iter()
        .flat_map(|o| match o {
            TrackerOutput::Data(d) => d.bytes.clone(),
            _ => vec![],
        })
        .collect()
}

fn output_text(out: &[TrackerOutput], block: BlockId) -> String {
    let bytes: Vec<u8> = out
        .iter()
        .flat_map(|o| match o {
            TrackerOutput::Data(d) if d.region == Region::Output && d.block == Some(block) => {
                d.bytes.clone()
            }
            _ => vec![],
        })
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn started(ev: &[PaneEvent]) -> Vec<(BlockId, usize, Option<String>)> {
    ev.iter()
        .filter_map(|e| match e {
            PaneEvent::BlockStarted {
                block,
                depth,
                command,
                ..
            } => Some((*block, *depth, command.clone())),
            _ => None,
        })
        .collect()
}

fn finished(ev: &[PaneEvent]) -> Vec<(BlockId, Option<i32>)> {
    ev.iter()
        .filter_map(|e| match e {
            PaneEvent::BlockFinished {
                block, exit_code, ..
            } => Some((*block, *exit_code)),
            _ => None,
        })
        .collect()
}

/// Secuencia real capturada de bash 5.2 con mkshell.bash (abreviada).
fn bash_session() -> String {
    let id = "100.1";
    format!(
        "motd\r\n\x1b]7;file://vm/tmp{BEL}\x1b[?2004h{}$ {}echo hi\r\n\x1b[?2004l\r{}hi\r\n{}\
         \x1b]7;file://vm/tmp{BEL}\x1b[?2004h{}$ {}\r\n\x1b[?2004l\r\x1b]7;file://vm/tmp{BEL}{}$ {}\
         false\r\n{}{}\x1b[?2004h{}$ {}",
        a(id),
        b(),
        c(id),
        d(0, id),
        a(id),
        b(),
        a(id),
        b(),
        c(id),
        d(1, id),
        a(id),
        b()
    )
}

#[test]
fn bash_blocks_and_regions() {
    let input = bash_session();
    let mut t = Tracker::new();
    t.expect_command("echo hi".into(), Origin::User);
    let out = t.feed(input.as_bytes());
    assert_eq!(concat(&out), input.as_bytes(), "los bytes no se alteran");

    let ev = events(&out);
    let st = started(&ev);
    assert_eq!(st.len(), 2, "Enter en vacío no crea bloque: {ev:#?}");
    assert_eq!(st[0].2.as_deref(), Some("echo hi"));
    // Sin comando pendiente, se recupera del eco.
    assert_eq!(st[1].2.as_deref(), Some("false"));
    assert_eq!(finished(&ev), vec![(st[0].0, Some(0)), (st[1].0, Some(1))]);
    assert_eq!(output_text(&out, st[0].0), "hi\r\n");

    // El texto de arranque queda en la región Startup.
    match &out[0] {
        TrackerOutput::Data(d) => {
            assert_eq!(d.region, Region::Startup);
            assert!(String::from_utf8_lossy(&d.bytes).starts_with("motd"));
        }
        other => panic!("{other:?}"),
    }
    assert!(ev.contains(&PaneEvent::CwdChanged {
        depth: 0,
        cwd: "/tmp".into(),
        host: Some("vm".into())
    }));
    assert!(t.is_ready());
    assert!(t.bracketed_paste());
}

#[test]
fn byte_by_byte_feeding_gives_same_events() {
    let input = bash_session();
    let mut whole = Tracker::new();
    let ev_whole = events(&whole.feed(input.as_bytes()));
    let mut t = Tracker::new();
    let mut out = Vec::new();
    for byte in input.as_bytes() {
        out.extend(t.feed(std::slice::from_ref(byte)));
    }
    assert_eq!(concat(&out), input.as_bytes());
    let strip = |ev: Vec<PaneEvent>| -> Vec<String> {
        ev.into_iter()
            .map(|e| format!("{e:?}").split(" {").next().unwrap().to_string())
            .collect()
    };
    assert_eq!(strip(events(&out)), strip(ev_whole));
}

#[test]
fn osc_with_esc_backslash_terminator_split_across_chunks() {
    // CMD usa ESC \ como terminador. Se parte justo tras el ESC.
    let mut t = Tracker::new();
    let first = b"x\x1b]133;A;mkc=0\x1b";
    let second = b"\\C:\\> ";
    let mut out = t.feed(first);
    out.extend(t.feed(second));
    assert_eq!(concat(&out), [&first[..], &second[..]].concat());
    // El `\` del terminador pertenece a la región previa (Startup), no al prompt.
    let regions: Vec<(Region, Vec<u8>)> = out
        .iter()
        .filter_map(|o| match o {
            TrackerOutput::Data(d) => Some((d.region, d.bytes.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(regions[0], (Region::Startup, b"x".to_vec()));
    assert!(regions[1..].iter().all(|(r, _)| *r == Region::Prompt));
    assert!(events(&out).contains(&PaneEvent::PromptStart { depth: 0 }));
}

#[test]
fn cmd_without_c_marker_infers_output_start() {
    let st = "\x1b\\";
    let mut t = Tracker::new();
    let prompt = format!(
        "\x1b]133;D{st}\x1b]133;A;mkc=0{st}\x1b]9;9;C:\\Users\\u{st}C:\\Users\\u>\x1b]133;B{st}"
    );
    t.feed(prompt.as_bytes());
    t.expect_command("dir".into(), Origin::User);
    let out = t.feed(format!("dir\r\nfile.txt\r\n\r\n{prompt}").as_bytes());
    let ev = events(&out);
    let st_ = started(&ev);
    assert_eq!(st_.len(), 1);
    assert_eq!(output_text(&out, st_[0].0), "file.txt\r\n\r\n");
    // D sin código: bloque cerrado con código desconocido.
    assert_eq!(finished(&ev), vec![(st_[0].0, None)]);
    assert!(
        ev.iter()
            .any(|e| matches!(e, PaneEvent::CwdChanged { cwd, .. } if cwd == "C:\\Users\\u"))
    );
}

#[test]
fn alt_screen_is_split_at_the_escape_sequence() {
    let id = "1.1";
    let mut t = Tracker::new();
    t.feed(format!("{}$ {}", a(id), b()).as_bytes());
    t.expect_command("vim".into(), Origin::User);
    let out = t.feed(format!("{}pre\x1b[?1049hFULL\x1b[?1049lpost{}", c(id), d(0, id)).as_bytes());
    let chunks: Vec<(bool, String)> = out
        .iter()
        .filter_map(|o| match o {
            TrackerOutput::Data(d) if d.region == Region::Output => {
                Some((d.alt, String::from_utf8_lossy(&d.bytes).into_owned()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        chunks,
        vec![
            (false, "pre".to_string()),
            (true, "\x1b[?1049hFULL\x1b[?1049l".to_string()),
            (false, "post".to_string()),
        ]
    );
    let ev = events(&out);
    assert!(ev.contains(&PaneEvent::AltScreen { on: true }));
    assert!(ev.contains(&PaneEvent::AltScreen { on: false }));
}

#[test]
fn nested_ssh_shell_gets_its_own_blocks() {
    let (local, remote) = ("10.1", "20.2");
    let mut t = Tracker::new();
    t.feed(format!("{}$ {}", a(local), b()).as_bytes());
    t.expect_command("ssh root@pve".into(), Origin::User);
    // Se conecta, el shell remoto (inyectado) emite un D espurio, su cwd y su prompt.
    let out = t.feed(
        format!(
            "{}Welcome\r\n{}\x1b]7;file://pve/root{BEL}{}root@pve:~# {}",
            c(local),
            d(0, remote),
            a(remote),
            b()
        )
        .as_bytes(),
    );
    let ev = events(&out);
    let ssh_block = started(&ev)[0].0;
    assert!(ev.iter().any(|e| matches!(e,
        PaneEvent::BlockStarted { ssh: Some(s), .. } if s == "root@pve")));
    assert!(ev.contains(&PaneEvent::ShellEntered { depth: 1 }));
    assert!(ev.contains(&PaneEvent::Ready {
        depth: 1,
        cwd: Some("/root".into()),
        host: Some("pve".into())
    }));
    assert!(
        finished(&ev).is_empty(),
        "el D del shell remoto recién cargado se ignora"
    );
    assert_eq!(t.depth(), 1);

    // Comando remoto.
    t.expect_command("uptime".into(), Origin::User);
    let out = t.feed(
        format!(
            "{}up 3 days\r\n{}{}# {}",
            c(remote),
            d(0, remote),
            a(remote),
            b()
        )
        .as_bytes(),
    );
    let ev = events(&out);
    let remote_block = started(&ev)[0].clone();
    assert_eq!(remote_block.1, 1);
    assert_eq!(output_text(&out, remote_block.0), "up 3 days\r\n");
    assert_eq!(finished(&ev), vec![(remote_block.0, Some(0))]);

    // La conexión se corta: llega directamente el D del shell local.
    let out = t.feed(
        format!(
            "Connection closed\r\n{}{}$ {}",
            d(255, local),
            a(local),
            b()
        )
        .as_bytes(),
    );
    let ev = events(&out);
    assert!(ev.contains(&PaneEvent::ShellExited { depth: 1 }));
    assert!(finished(&ev).contains(&(ssh_block, Some(255))));
    assert_eq!(t.depth(), 0);
}

#[test]
fn ctrl_c_at_prompt_does_not_create_blocks() {
    let id = "5.5";
    let mut t = Tracker::new();
    let out = t.feed(format!("{}$ {}^C\r\n{}$ {}", a(id), b(), a(id), b()).as_bytes());
    assert!(started(&events(&out)).is_empty());
    assert!(t.is_ready());
}

#[test]
fn echo_reconstruction_handles_line_editors() {
    // zsh: «e», retroceso, «echo hi»
    assert_eq!(
        echo_to_text(b"\x1b[K\x1b[?2004he\x08echo hi\x1b[?2004l\r\r\n"),
        "echo hi"
    );
    // fish: redibujado con movimientos de cursor
    assert_eq!(
        echo_to_text(b"\x1b[K\r\x1b[16Cecho \r\x1b[21Chi\r\x1b[23C\x08\x08\x08\x08\x08\x08\x08\x1b[34mecho\x1b[30m \x1b[36mhi\r\n"),
        "echo hi"
    );
}

#[test]
fn percent_decoding() {
    assert_eq!(
        percent_decode("/home/u/mis%20docs/%C3%B1"),
        "/home/u/mis docs/ñ"
    );
    assert_eq!(percent_decode("/bad%2"), "/bad%2");
    assert_eq!(percent_decode("/x%ZZ"), "/x%ZZ");
}

#[test]
fn cmd_via_conpty_line_break_as_cursor_move() {
    // Secuencia real de CMD bajo ConPTY (CI windows-2025): el salto de línea
    // tras el eco llega como `CSI 7;1 H`.
    let st = "\x1b\\";
    let prompt = format!(
        "\x1b]133;D{st}\x1b]133;A;mkc=0{st}\x1b]9;9;C:\\Users\\r{st}C:\\Users\\r>\x1b]133;B{st}"
    );
    let mut t = Tracker::new();
    t.feed(prompt.as_bytes());
    t.expect_command("cmd /c exit 3".into(), Origin::User);
    let out = t.feed(format!("\x1b[?25lcmd /c exit 3\x1b[7;1H\x1b[?25h{prompt}").as_bytes());
    let ev = events(&out);
    let st_ = started(&ev);
    assert_eq!(st_.len(), 1, "{ev:#?}");
    assert_eq!(st_[0].2.as_deref(), Some("cmd /c exit 3"));
    assert_eq!(finished(&ev), vec![(st_[0].0, None)]);
    // El movimiento de cursor de la propia línea de comandos (columna > 1) no cuenta.
    let mut t = Tracker::new();
    t.feed(prompt.as_bytes());
    t.expect_command("dir".into(), Origin::User);
    let out = t.feed(b"\x1b[3;20Hdir\r\nfile\r\n");
    let ev = events(&out);
    let st_ = started(&ev);
    assert_eq!(output_text(&out, st_[0].0), "file\r\n");
}
