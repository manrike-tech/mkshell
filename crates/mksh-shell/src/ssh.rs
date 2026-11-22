//! Integración en sesiones SSH (y en shells sin integración, como WSL).
//!
//! MKShell no instala nada en el host remoto: envía una única línea que
//! decodifica el script de integración (bash o zsh) y lo evalúa en memoria.
//! Solo se envía con el consentimiento explícito del usuario para ese host.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};

const BASH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.bash");
const ZSH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.zsh");

/// Destino de una sesión SSH interactiva.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTarget {
    pub user: Option<String>,
    pub host: String,
    pub port: Option<u16>,
}

impl SshTarget {
    /// Clave estable para recordar el consentimiento: `usuario@host[:puerto]`.
    pub fn key(&self) -> String {
        let mut k = String::new();
        if let Some(u) = &self.user {
            k.push_str(u);
            k.push('@');
        }
        k.push_str(&self.host);
        if let Some(p) = self.port.filter(|p| *p != 22) {
            k.push(':');
            k.push_str(&p.to_string());
        }
        k
    }
}

/// Opciones de `ssh` que consumen un argumento (OpenSSH).
const OPTS_WITH_ARG: &str = "BbcDEeFIiJLlmOoPpRSWw";
/// Opciones que indican que no habrá shell interactivo.
const NON_INTERACTIVE: &str = "NGVQWOT";

/// Analiza una línea de comandos y devuelve el destino si es un `ssh`
/// interactivo (sin comando remoto ni modos como `-N`, `-W` o `-V`).
pub fn parse_ssh_command(line: &str) -> Option<SshTarget> {
    let tokens = split_words(line)?;
    let mut it = tokens.into_iter().peekable();
    let program = it.next()?;
    let name = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&program)
        .to_ascii_lowercase();
    if name != "ssh" && name != "ssh.exe" {
        return None;
    }
    let (mut user, mut port) = (None::<String>, None::<u16>);
    let mut destination = None;
    while let Some(tok) = it.next() {
        if destination.is_some() {
            // Hay comando remoto: no es una sesión interactiva.
            return None;
        }
        if tok == "--" {
            destination = it.next();
            continue;
        }
        if let Some(flags) = tok.strip_prefix('-').filter(|f| !f.is_empty()) {
            for (i, c) in flags.char_indices() {
                if NON_INTERACTIVE.contains(c) {
                    return None;
                }
                if OPTS_WITH_ARG.contains(c) {
                    let rest = &flags[i + c.len_utf8()..];
                    let value = if rest.is_empty() {
                        it.next()?
                    } else {
                        rest.to_string()
                    };
                    match c {
                        'l' => user = Some(value),
                        'p' => port = value.parse().ok(),
                        'o' => apply_option(&value, &mut user, &mut port),
                        _ => {}
                    }
                    break;
                }
            }
            continue;
        }
        destination = Some(tok);
    }
    let dest = destination?;
    let (dest_user, host, dest_port) = parse_destination(&dest)?;
    Some(SshTarget {
        user: dest_user.or(user),
        host,
        port: dest_port.or(port),
    })
}

fn apply_option(value: &str, user: &mut Option<String>, port: &mut Option<u16>) {
    let (key, val) = value
        .split_once('=')
        .or_else(|| value.split_once(' '))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim()))
        .unwrap_or_default();
    match key.as_str() {
        "user" => *user = Some(val.to_string()),
        "port" => *port = val.parse().ok(),
        "remotecommand" => {}
        _ => {}
    }
}

/// `[user@]host`, `ssh://[user@]host[:port]`.
fn parse_destination(dest: &str) -> Option<(Option<String>, String, Option<u16>)> {
    if let Some(rest) = dest.strip_prefix("ssh://") {
        let (user, hostport) = match rest.rsplit_once('@') {
            Some((u, h)) => (Some(u.to_string()), h),
            None => (None, rest),
        };
        let hostport = hostport.trim_end_matches('/');
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) if !h.contains(':') || h.starts_with('[') => (h, p.parse().ok()),
            _ => (hostport, None),
        };
        let host = host.trim_start_matches('[').trim_end_matches(']');
        return (!host.is_empty()).then(|| (user, host.to_string(), port));
    }
    let (user, host) = match dest.rsplit_once('@') {
        Some((u, h)) => (Some(u.to_string()), h.to_string()),
        None => (None, dest.to_string()),
    };
    (!host.is_empty()).then_some((user, host, None))
}

/// División en palabras al estilo POSIX (comillas simples, dobles y `\`).
/// Devuelve `None` si hay comillas sin cerrar.
fn split_words(line: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        ch => cur.push(ch),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => cur.push(chars.next()?),
                        ch => cur.push(ch),
                    }
                }
            }
            '\\' => {
                in_word = true;
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            // Operadores de shell: tras ellos ya no es un `ssh` simple.
            ';' | '|' | '&' | '<' | '>' => return Some(vec![]),
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Some(words)
}

/// Línea que activa la integración en el shell remoto actual (bash o zsh).
///
/// * Empieza con un espacio: no entra en el historial si `HISTCONTROL`
///   incluye `ignorespace` o zsh tiene `HIST_IGNORE_SPACE`.
/// * Solo necesita `base64` en el host; no escribe nada en disco.
/// * En shells no soportados (fish, sh) no hace nada.
pub fn bootstrap_line() -> String {
    let b = B64.encode(BASH_SCRIPT);
    let z = B64.encode(ZSH_SCRIPT);
    format!(
        " if [ -n \"${{ZSH_VERSION-}}\" ]; then eval \"$(printf '%s' '{z}' | base64 -d)\"; \
elif [ -n \"${{BASH_VERSION-}}\" ]; then eval \"$(printf '%s' '{b}' | base64 -d)\"; fi"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(user: Option<&str>, host: &str, port: Option<u16>) -> Option<SshTarget> {
        Some(SshTarget {
            user: user.map(String::from),
            host: host.into(),
            port,
        })
    }

    #[test]
    fn parses_common_forms() {
        assert_eq!(parse_ssh_command("ssh srv"), t(None, "srv", None));
        assert_eq!(
            parse_ssh_command("ssh root@pve01"),
            t(Some("root"), "pve01", None)
        );
        assert_eq!(
            parse_ssh_command("ssh -p 2222 admin@10.0.0.5"),
            t(Some("admin"), "10.0.0.5", Some(2222))
        );
        assert_eq!(
            parse_ssh_command("ssh -p2222 -l ops web"),
            t(Some("ops"), "web", Some(2222))
        );
        assert_eq!(
            parse_ssh_command("ssh -i ~/.ssh/id -J bastion db"),
            t(None, "db", None)
        );
        assert_eq!(
            parse_ssh_command("ssh -o User=deploy -o Port=22 app"),
            t(Some("deploy"), "app", Some(22))
        );
        assert_eq!(
            parse_ssh_command("ssh -vA ssh://u@h:2200"),
            t(Some("u"), "h", Some(2200))
        );
        assert_eq!(
            parse_ssh_command("/usr/bin/ssh -t host"),
            t(None, "host", None)
        );
    }

    #[test]
    fn rejects_non_interactive() {
        assert_eq!(parse_ssh_command("ssh host uptime"), None);
        assert_eq!(parse_ssh_command("ssh -N -L 8080:localhost:80 host"), None);
        assert_eq!(parse_ssh_command("ssh -V"), None);
        assert_eq!(parse_ssh_command("ssh host | tee log"), None);
        assert_eq!(parse_ssh_command("sshfs host:/ /mnt"), None);
        assert_eq!(parse_ssh_command("ssh 'unclosed"), None);
    }

    #[test]
    fn key_omits_default_port() {
        assert_eq!(t(Some("u"), "h", Some(22)).unwrap().key(), "u@h");
        assert_eq!(t(None, "h", Some(2200)).unwrap().key(), "h:2200");
    }

    #[test]
    fn bootstrap_is_single_line_and_decodes() {
        let line = bootstrap_line();
        assert!(line.starts_with(' '));
        assert!(!line.contains('\n'));
        let b64 = line.split('\'').nth(3).unwrap();
        let decoded = String::from_utf8(B64.decode(b64).unwrap()).unwrap();
        assert!(decoded.contains("133;A"));
    }
}
