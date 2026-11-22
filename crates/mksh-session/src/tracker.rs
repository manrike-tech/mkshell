//! Segmentación de la salida del PTY en bloques.
//!
//! El [`Tracker`] recibe bytes crudos del PTY y devuelve fragmentos etiquetados
//! con su región (prompt, eco, salida, arranque) y eventos (bloque iniciado,
//! terminado, cambio de cwd, pantalla alternativa…). Los bytes nunca se
//! modifican: la concatenación de todos los fragmentos es idéntica a la
//! entrada, de modo que el modo TTY clásico puede mostrar el flujo completo.
//!
//! # Shells anidados
//!
//! Cada shell con integración marca sus prompts con `aid=<id>`. Una pila de
//! *frames* sigue qué shell está activo: si llega un prompt de un `aid`
//! desconocido mientras un comando está en ejecución (p. ej. `ssh`), se apila
//! un frame nuevo; un `D` con el `aid` de un frame inferior desapila los que
//! estén encima (el shell remoto terminó, aunque la conexión se cortara).

use mksh_core::{BlockId, Origin, PaneEvent, Region};
use std::collections::VecDeque;
use std::time::Instant;

const ECHO_CAP: usize = 8 * 1024;

/// Fragmento contiguo de salida con su contexto.
#[derive(Debug, Clone, PartialEq)]
pub struct DataChunk {
    pub region: Region,
    pub block: Option<BlockId>,
    /// `true` mientras una aplicación usa la pantalla alternativa.
    pub alt: bool,
    pub bytes: Vec<u8>,
}

/// Resultado de procesar bytes: datos y eventos, en orden.
#[derive(Debug, Clone, PartialEq)]
pub enum TrackerOutput {
    Data(DataChunk),
    Event(PaneEvent),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameState {
    /// Antes del primer prompt (solo el frame raíz).
    Startup,
    Prompt,
    /// Esperando o editando un comando (tras B).
    Input,
    /// Comando en ejecución (tras C).
    Output,
    /// Tras D, antes del siguiente A.
    Idle,
}

#[derive(Debug)]
struct OpenBlock {
    id: BlockId,
    started: Instant,
}

#[derive(Debug)]
struct Frame {
    aid: Option<String>,
    state: FrameState,
    block: Option<OpenBlock>,
    host: Option<String>,
    cwd: Option<String>,
    /// `false` si el shell no emite la marca C (CMD, PowerShell sin PSReadLine).
    supports_c: bool,
    /// `Some(true)` si el shell activó *bracketed paste* en su prompt alguna vez.
    /// Entre comandos lo desactiva (y zsh lo reactiva después de B), así que el
    /// estado instantáneo no sirve para decidir cómo enviar un comando.
    paste: Option<bool>,
    echo: Vec<u8>,
}

impl Frame {
    fn new(state: FrameState) -> Self {
        Self {
            aid: None,
            state,
            block: None,
            host: None,
            cwd: None,
            supports_c: true,
            paste: None,
            echo: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct PendingCommand {
    text: String,
    origin: Origin,
}

#[derive(Debug, Clone, PartialEq)]
enum Marker {
    PromptStart {
        aid: Option<String>,
        no_c: bool,
    },
    CommandStart,
    OutputStart {
        aid: Option<String>,
    },
    CommandDone {
        code: Option<i32>,
        aid: Option<String>,
    },
    Cwd {
        host: Option<String>,
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
enum Hit {
    Osc { marker: Marker, bell: bool },
    AltScreen(bool),
    BracketedPaste(bool),
}

/// Recolector de secuencias relevantes para `vte`.
#[derive(Default)]
struct Collector {
    hits: Vec<Hit>,
}

impl vte::Perform for Collector {
    fn osc_dispatch(&mut self, params: &[&[u8]], bell: bool) {
        if let Some(marker) = parse_osc(params) {
            self.hits.push(Hit::Osc { marker, bell });
        }
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        _ignore: bool,
        action: char,
    ) {
        if intermediates != b"?" || !(action == 'h' || action == 'l') {
            return;
        }
        let on = action == 'h';
        for p in params.iter() {
            match p.first().copied() {
                Some(1049) | Some(1047) | Some(47) => self.hits.push(Hit::AltScreen(on)),
                Some(2004) => self.hits.push(Hit::BracketedPaste(on)),
                _ => {}
            }
        }
    }
}

fn parse_osc(params: &[&[u8]]) -> Option<Marker> {
    let text = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let aid_of = |ps: &[&[u8]]| {
        ps.iter().find_map(|p| {
            p.strip_prefix(b"aid=")
                .map(|v| String::from_utf8_lossy(v).into_owned())
        })
    };
    match *params.first()? {
        b"133" => {
            let kind = params.get(1)?;
            let rest = &params[2..];
            match *kind {
                b"A" => Some(Marker::PromptStart {
                    aid: aid_of(rest),
                    no_c: rest.iter().any(|p| *p == b"mkc=0"),
                }),
                b"B" => Some(Marker::CommandStart),
                b"C" => Some(Marker::OutputStart { aid: aid_of(rest) }),
                b"D" => {
                    let code = rest
                        .first()
                        .and_then(|c| std::str::from_utf8(c).ok()?.parse().ok());
                    Some(Marker::CommandDone {
                        code,
                        aid: aid_of(rest),
                    })
                }
                _ => None,
            }
        }
        b"7" => {
            // OSC 7 ; file://host/path  (vte separa por ';', se vuelve a unir)
            let joined = params[1..]
                .iter()
                .map(|p| text(p))
                .collect::<Vec<_>>()
                .join(";");
            let rest = joined.strip_prefix("file://")?;
            let (host, path) = match rest.find('/') {
                Some(i) => (&rest[..i], &rest[i..]),
                None => (rest, "/"),
            };
            Some(Marker::Cwd {
                host: (!host.is_empty()).then(|| host.to_string()),
                path: percent_decode(path),
            })
        }
        b"9" if params.get(1).copied() == Some(b"9") => {
            let joined = params[2..]
                .iter()
                .map(|p| text(p))
                .collect::<Vec<_>>()
                .join(";");
            let path = joined.trim_matches('"').to_string();
            (!path.is_empty()).then_some(Marker::Cwd { host: None, path })
        }
        _ => None,
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Segmentador de bloques de un panel.
pub struct Tracker {
    parser: vte::Parser,
    collector: Collector,
    frames: Vec<Frame>,
    alt: bool,
    bracketed_paste: bool,
    pending: VecDeque<PendingCommand>,
    /// Shell sin marca C: el bloque empieza en el primer salto de línea tras
    /// enviar un comando.
    infer_c: bool,
    /// OSC 7 recibido durante un comando: probablemente de un shell anidado
    /// que aún no ha dibujado su primer prompt.
    stash_cwd: Option<(Option<String>, String)>,
    /// OSC terminado en ESC cuyo `\` final puede llegar en el siguiente trozo.
    carry_st: Option<Marker>,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new()
    }
}

impl Tracker {
    pub fn new() -> Self {
        Self {
            parser: vte::Parser::new(),
            collector: Collector::default(),
            frames: vec![Frame::new(FrameState::Startup)],
            alt: false,
            bracketed_paste: false,
            pending: VecDeque::new(),
            infer_c: false,
            stash_cwd: None,
            carry_st: None,
        }
    }

    /// Profundidad del shell activo (0 = shell del panel).
    pub fn depth(&self) -> usize {
        self.frames.len() - 1
    }

    /// `true` si el shell activo espera un comando.
    pub fn is_ready(&self) -> bool {
        self.top().state == FrameState::Input
    }

    /// `true` si hay un comando en ejecución en el shell activo.
    pub fn is_running(&self) -> bool {
        self.top().state == FrameState::Output
    }

    /// `true` si el shell activo acepta *bracketed paste* en su prompt.
    pub fn bracketed_paste(&self) -> bool {
        self.top().paste.unwrap_or(self.bracketed_paste)
    }

    /// `true` cuando el shell raíz ya dibujó su primer prompt con marcas.
    pub fn has_prompted(&self) -> bool {
        self.frames[0].state != FrameState::Startup
    }

    /// Directorio del shell raíz (para abrir paneles nuevos en el mismo sitio).
    pub fn root_cwd(&self) -> Option<&str> {
        self.frames[0].cwd.as_deref()
    }

    /// Registra el texto de un comando que MKShell va a enviar al shell activo.
    pub fn expect_command(&mut self, text: String, origin: Origin) {
        self.pending.push_back(PendingCommand { text, origin });
        if !self.top().supports_c {
            self.infer_c = true;
        }
    }

    fn top(&self) -> &Frame {
        self.frames.last().expect("siempre hay un frame")
    }

    fn top_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("siempre hay un frame")
    }

    fn current_region(&self) -> (Region, Option<BlockId>) {
        let top = self.top();
        match top.state {
            FrameState::Startup => (Region::Startup, None),
            FrameState::Prompt | FrameState::Idle => (Region::Prompt, None),
            FrameState::Input => (Region::Echo, None),
            FrameState::Output => (Region::Output, top.block.as_ref().map(|b| b.id)),
        }
    }

    fn emit(&mut self, data: &[u8], out: &mut Vec<TrackerOutput>) {
        if data.is_empty() {
            return;
        }
        let (region, block) = self.current_region();
        if region == Region::Echo {
            let echo = &mut self.top_mut().echo;
            let room = ECHO_CAP.saturating_sub(echo.len());
            echo.extend_from_slice(&data[..data.len().min(room)]);
        }
        // Se fusiona con el fragmento anterior si comparte contexto.
        let alt = self.alt;
        let same = |o: &&mut TrackerOutput| matches!(o, TrackerOutput::Data(p) if p.region == region && p.block == block && p.alt == alt);
        if let Some(TrackerOutput::Data(prev)) = out.last_mut().filter(same) {
            prev.bytes.extend_from_slice(data);
            return;
        }
        out.push(TrackerOutput::Data(DataChunk {
            region,
            block,
            alt: self.alt,
            bytes: data.to_vec(),
        }));
    }

    /// Emite bytes de control (las propias marcas OSC) como región oculta,
    /// para que no aparezcan dentro de la salida de un bloque.
    fn emit_hidden(&mut self, data: &[u8], out: &mut Vec<TrackerOutput>) {
        if data.is_empty() {
            return;
        }
        let alt = self.alt;
        let same = |o: &&mut TrackerOutput| matches!(o, TrackerOutput::Data(p) if p.region == Region::Prompt && p.block.is_none() && p.alt == alt);
        if let Some(TrackerOutput::Data(prev)) = out.last_mut().filter(same) {
            prev.bytes.extend_from_slice(data);
            return;
        }
        out.push(TrackerOutput::Data(DataChunk {
            region: Region::Prompt,
            block: None,
            alt: self.alt,
            bytes: data.to_vec(),
        }));
    }

    /// Procesa bytes del PTY.
    pub fn feed(&mut self, data: &[u8]) -> Vec<TrackerOutput> {
        let mut out = Vec::new();
        let mut seg = 0usize;
        let mut last_esc: Option<usize> = None;
        // Inicio (ESC) del OSC en curso dentro de este trozo.
        let mut osc_begin: Option<usize> = None;
        // OSC terminado por ESC: (marca, inicio del OSC, índice del ESC final).
        let mut pending_st: Option<(Marker, usize, usize)> = None;

        // Un OSC terminado en ESC al final del trozo anterior.
        if let Some(marker) = self.carry_st.take() {
            if data.first() == Some(&b'\\') {
                self.emit_hidden(&data[..1], &mut out);
                seg = 1;
            }
            self.apply(marker, &mut out);
        }

        let first = seg;
        for i in first..data.len() {
            let b = data[i];
            if let Some((marker, begin, esc_idx)) = pending_st.take() {
                let end = if b == b'\\' { i + 1 } else { esc_idx };
                self.emit(&data[seg..begin], &mut out);
                self.emit_hidden(&data[begin..end], &mut out);
                seg = end;
                self.apply(marker, &mut out);
            }
            if b == b']' && last_esc == Some(i.wrapping_sub(1)) {
                osc_begin = Some(i - 1);
            }
            if b == 0x1b {
                last_esc = Some(i);
            }
            self.parser.advance(&mut self.collector, &[b]);
            let hits = std::mem::take(&mut self.collector.hits);
            for hit in hits {
                match hit {
                    Hit::Osc { marker, bell } => {
                        let begin = osc_begin.take().filter(|o| *o >= seg).unwrap_or(seg);
                        if bell {
                            self.emit(&data[seg..begin], &mut out);
                            self.emit_hidden(&data[begin..=i], &mut out);
                            seg = i + 1;
                            self.apply(marker, &mut out);
                        } else {
                            pending_st = Some((marker, begin, i));
                        }
                    }
                    Hit::AltScreen(true) if !self.alt => {
                        let start = last_esc.filter(|e| *e >= seg).unwrap_or(seg);
                        self.emit(&data[seg..start], &mut out);
                        seg = start;
                        self.alt = true;
                        out.push(TrackerOutput::Event(PaneEvent::AltScreen { on: true }));
                    }
                    Hit::AltScreen(false) if self.alt => {
                        self.emit(&data[seg..=i], &mut out);
                        seg = i + 1;
                        self.alt = false;
                        out.push(TrackerOutput::Event(PaneEvent::AltScreen { on: false }));
                    }
                    Hit::AltScreen(_) => {}
                    Hit::BracketedPaste(on) => {
                        self.bracketed_paste = on;
                        let top = self.top_mut();
                        if on
                            && matches!(
                                top.state,
                                FrameState::Prompt | FrameState::Input | FrameState::Idle
                            )
                        {
                            top.paste = Some(true);
                        }
                    }
                }
            }
            if self.infer_c
                && b == b'\n'
                && self.top().state == FrameState::Input
                && pending_st.is_none()
            {
                self.emit(&data[seg..=i], &mut out);
                seg = i + 1;
                self.infer_c = false;
                self.start_block(&mut out);
            }
        }

        if let Some((marker, begin, esc_idx)) = pending_st {
            self.emit(&data[seg..begin], &mut out);
            if esc_idx + 1 == data.len() {
                // El `\` del ST llegará en el siguiente trozo.
                self.emit_hidden(&data[begin..], &mut out);
                self.carry_st = Some(marker);
                return out;
            }
            self.emit_hidden(&data[begin..esc_idx], &mut out);
            seg = esc_idx;
            self.apply(marker, &mut out);
        }
        self.emit(&data[seg..], &mut out);
        out
    }

    fn apply(&mut self, marker: Marker, out: &mut Vec<TrackerOutput>) {
        match marker {
            Marker::PromptStart { aid, no_c } => self.on_prompt_start(aid, no_c, out),
            Marker::CommandStart => {
                let depth = self.depth();
                let top = self.top_mut();
                if top.state != FrameState::Output {
                    top.state = FrameState::Input;
                    top.echo.clear();
                    let (cwd, host) = (top.cwd.clone(), top.host.clone());
                    out.push(TrackerOutput::Event(PaneEvent::Ready { depth, cwd, host }));
                }
            }
            Marker::OutputStart { aid } => {
                if let Some(k) = self.find_frame(aid.as_deref()) {
                    self.pop_above(k, out);
                }
                if self.top().state != FrameState::Output {
                    self.infer_c = false;
                    self.start_block(out);
                }
            }
            Marker::CommandDone { code, aid } => self.on_command_done(code, aid, out),
            Marker::Cwd { host, path } => {
                if self.top().state == FrameState::Output {
                    self.stash_cwd = Some((host, path));
                } else {
                    let depth = self.depth();
                    let top = self.top_mut();
                    top.cwd = Some(path.clone());
                    top.host = host.clone();
                    out.push(TrackerOutput::Event(PaneEvent::CwdChanged {
                        depth,
                        cwd: path,
                        host,
                    }));
                }
            }
        }
    }

    fn find_frame(&self, aid: Option<&str>) -> Option<usize> {
        let aid = aid?;
        self.frames
            .iter()
            .rposition(|f| f.aid.as_deref() == Some(aid))
    }

    fn on_prompt_start(&mut self, aid: Option<String>, no_c: bool, out: &mut Vec<TrackerOutput>) {
        if let Some(k) = self.find_frame(aid.as_deref()) {
            self.pop_above(k, out);
            // A sin D previo: el comando terminó sin informar (se cierra sin código).
            self.finish_top(None, out);
        } else if self.top().state == FrameState::Output {
            // Prompt de un shell desconocido durante un comando: shell anidado.
            let mut frame = Frame::new(FrameState::Prompt);
            if let Some((host, path)) = self.stash_cwd.take() {
                frame.host = host;
                frame.cwd = Some(path);
            }
            self.frames.push(frame);
            out.push(TrackerOutput::Event(PaneEvent::ShellEntered {
                depth: self.depth(),
            }));
        }
        let depth = self.depth();
        let top = self.top_mut();
        if aid.is_some() {
            top.aid = aid;
        }
        top.supports_c = !no_c;
        top.state = FrameState::Prompt;
        out.push(TrackerOutput::Event(PaneEvent::PromptStart { depth }));
    }

    fn on_command_done(
        &mut self,
        code: Option<i32>,
        aid: Option<String>,
        out: &mut Vec<TrackerOutput>,
    ) {
        match aid {
            Some(aid) => {
                // Un D de un shell desconocido (p. ej. el primer prompt de un
                // shell recién inyectado) se ignora.
                if let Some(k) = self.find_frame(Some(&aid)) {
                    self.pop_above(k, out);
                    self.finish_top(code, out);
                }
            }
            None => {
                if self.top().state == FrameState::Output {
                    self.finish_top(code, out);
                } else if self.frames.len() > 1 {
                    self.pop_above(self.frames.len() - 2, out);
                    self.finish_top(code, out);
                }
            }
        }
    }

    /// Desapila los frames por encima de `k`, cerrando sus bloques abiertos.
    fn pop_above(&mut self, k: usize, out: &mut Vec<TrackerOutput>) {
        while self.frames.len() > k + 1 {
            self.finish_top(None, out);
            self.frames.pop();
            out.push(TrackerOutput::Event(PaneEvent::ShellExited {
                depth: self.frames.len(),
            }));
        }
    }

    fn finish_top(&mut self, code: Option<i32>, out: &mut Vec<TrackerOutput>) {
        let top = self.top_mut();
        if let Some(block) = top.block.take() {
            top.state = FrameState::Idle;
            out.push(TrackerOutput::Event(PaneEvent::BlockFinished {
                block: block.id,
                exit_code: code,
                duration_ms: block.started.elapsed().as_millis() as u64,
            }));
            self.stash_cwd = None;
        } else if top.state == FrameState::Output {
            top.state = FrameState::Idle;
        }
    }

    fn start_block(&mut self, out: &mut Vec<TrackerOutput>) {
        let depth = self.depth();
        let pending = self.pending.pop_front();
        let top = self.top_mut();
        let echo_text = echo_to_text(&top.echo);
        top.echo.clear();
        let (command, origin) = match pending {
            Some(p) => (Some(p.text), p.origin),
            None => ((!echo_text.is_empty()).then_some(echo_text), Origin::User),
        };
        let id = BlockId::new();
        top.state = FrameState::Output;
        top.block = Some(OpenBlock {
            id,
            started: Instant::now(),
        });
        let ssh = command
            .as_deref()
            .and_then(mksh_shell::ssh::parse_ssh_command)
            .map(|t| t.key());
        out.push(TrackerOutput::Event(PaneEvent::BlockStarted {
            block: id,
            depth,
            command,
            cwd: top.cwd.clone(),
            host: top.host.clone(),
            origin,
            ssh,
        }));
    }
}

/// Reconstruye el texto tecleado a partir del eco del shell, emulando una
/// línea simple (`\r`, `\b`, CSI C/D/K). Basta para recuperar comandos
/// escritos directamente en la terminal.
pub(crate) fn echo_to_text(bytes: &[u8]) -> String {
    #[derive(Default)]
    struct Line {
        lines: Vec<String>,
        cur: Vec<char>,
        col: usize,
    }
    impl Line {
        fn put(&mut self, c: char) {
            if self.col >= self.cur.len() {
                self.cur.resize(self.col, ' ');
                self.cur.push(c);
            } else {
                self.cur[self.col] = c;
            }
            self.col += 1;
        }
    }
    impl vte::Perform for Line {
        fn print(&mut self, c: char) {
            self.put(c);
        }
        fn execute(&mut self, byte: u8) {
            match byte {
                b'\r' => self.col = 0,
                b'\n' => {
                    let line: String = self.cur.drain(..).collect();
                    self.lines.push(line);
                    self.col = 0;
                }
                0x08 => self.col = self.col.saturating_sub(1),
                _ => {}
            }
        }
        fn csi_dispatch(&mut self, params: &vte::Params, inter: &[u8], _ig: bool, action: char) {
            if !inter.is_empty() {
                return;
            }
            let n = params
                .iter()
                .next()
                .and_then(|p| p.first().copied())
                .unwrap_or(1)
                .max(1) as usize;
            match action {
                'C' => self.col += n,
                'D' => self.col = self.col.saturating_sub(n),
                'K' => self.cur.truncate(self.col),
                _ => {}
            }
        }
    }
    let mut line = Line::default();
    let mut parser = vte::Parser::new();
    parser.advance(&mut line, bytes);
    if !line.cur.is_empty() {
        let last: String = line.cur.drain(..).collect();
        line.lines.push(last);
    }
    line.lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests;
