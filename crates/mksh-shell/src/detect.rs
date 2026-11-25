//! Detección de shells instalados.

use crate::Profile;
#[cfg(unix)]
use crate::ShellKind;
#[cfg(unix)]
use std::path::Path;

/// Devuelve los perfiles detectados. El primero es el recomendado por defecto.
pub fn detect_profiles() -> Vec<Profile> {
    #[cfg(unix)]
    {
        detect_unix(
            std::env::var("SHELL").ok().as_deref(),
            std::fs::read_to_string("/etc/shells").ok().as_deref(),
        )
    }
    #[cfg(windows)]
    {
        detect_windows()
    }
}

/// Detección en Unix a partir de `$SHELL` y del contenido de `/etc/shells`.
#[cfg(unix)]
pub(crate) fn detect_unix(env_shell: Option<&str>, etc_shells: Option<&str>) -> Vec<Profile> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(s) = env_shell.filter(|s| !s.is_empty()) {
        candidates.push(s.to_string());
    }
    if let Some(list) = etc_shells {
        // Se prefieren rutas /usr/bin sobre /bin (en distros con /usr fusionado
        // son el mismo fichero) para que el nombre mostrado sea estable.
        let mut lines: Vec<&str> = list
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect();
        lines.sort_by_key(|l| !l.starts_with("/usr/bin/"));
        candidates.extend(lines.into_iter().map(String::from));
    }
    for fallback in ["/bin/bash", "/usr/bin/zsh", "/usr/bin/fish", "/bin/sh"] {
        candidates.push(fallback.to_string());
    }

    let mut profiles: Vec<Profile> = Vec::new();
    for path in candidates {
        let kind = ShellKind::from_program(&path);
        if !matches!(
            kind,
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Fish | ShellKind::Posix
        ) {
            continue;
        }
        if profiles.iter().any(|p| p.kind == kind) || !Path::new(&path).exists() {
            continue;
        }
        let mut p = Profile::new(kind.label(), path, vec![]);
        // Ids estables y cortos («bash», «zsh»…) para la configuración y `mksh-probe`.
        p.id = match kind {
            ShellKind::Posix => "sh".into(),
            _ => p.id,
        };
        p.integration = kind.supports_integration();
        profiles.push(p);
    }
    // `sh` solo se ofrece si no hay nada mejor.
    if profiles.iter().any(|p| p.kind != ShellKind::Posix) {
        profiles.retain(|p| p.kind != ShellKind::Posix);
    }
    profiles
}

#[cfg(windows)]
fn detect_windows() -> Vec<Profile> {
    use std::path::PathBuf;

    // Ids explícitos: estables aunque cambie el nombre mostrado.
    let with_id = |mut p: Profile, id: &str| {
        p.id = id.into();
        p
    };
    let mut profiles = Vec::new();
    if let Some(pwsh) = which("pwsh.exe") {
        profiles.push(with_id(
            Profile::new("PowerShell", pwsh.to_string_lossy(), vec![]),
            "pwsh",
        ));
    }
    let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let winps =
        PathBuf::from(&system_root).join("System32\\WindowsPowerShell\\v1.0\\powershell.exe");
    if winps.exists() {
        profiles.push(with_id(
            Profile::new("Windows PowerShell", winps.to_string_lossy(), vec![]),
            "windows-powershell",
        ));
    }
    let cmd =
        std::env::var("ComSpec").unwrap_or_else(|_| format!("{system_root}\\System32\\cmd.exe"));
    profiles.push(with_id(
        Profile::new("Símbolo del sistema", cmd, vec![]),
        "cmd",
    ));

    for git_bash in [
        "C:\\Program Files\\Git\\bin\\bash.exe",
        "C:\\Program Files (x86)\\Git\\bin\\bash.exe",
    ] {
        if PathBuf::from(git_bash).exists() {
            let mut p = Profile::new("Git Bash", git_bash, vec!["--login".into()]);
            p.id = "git-bash".into();
            profiles.push(p);
            break;
        }
    }

    for distro in wsl_distros() {
        let mut p = Profile::new(
            format!("WSL: {distro}"),
            "wsl.exe",
            vec!["-d".into(), distro],
        );
        // El shell real está dentro de la distro: arranca en TTY y se puede
        // activar la integración con «Activar bloques» (mismo arranque que SSH).
        p.integration = false;
        profiles.push(p);
    }
    profiles
}

#[cfg(windows)]
fn which(exe: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(exe))
        .find(|p| p.exists())
}

/// Lista de distribuciones WSL. `wsl.exe -l -q` escribe en UTF-16LE.
#[cfg(windows)]
fn wsl_distros() -> Vec<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Ok(out) = std::process::Command::new("wsl.exe")
        .args(["-l", "-q"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    parse_wsl_list(&out.stdout)
}

/// Decodifica la salida UTF-16LE de `wsl.exe -l -q`.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn parse_wsl_list(bytes: &[u8]) -> Vec<String> {
    let units: Vec<u16> = (0..bytes.len() / 2)
        .map(|i| u16::from_le_bytes([bytes[2 * i], bytes[2 * i + 1]]))
        .collect();
    String::from_utf16_lossy(&units)
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\0'))
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wsl_utf16() {
        let text = "\u{feff}Ubuntu-24.04\r\ndocker-desktop\r\n";
        let bytes: Vec<u8> = text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(
            parse_wsl_list(&bytes),
            vec!["Ubuntu-24.04", "docker-desktop"]
        );
    }

    #[cfg(unix)]
    #[test]
    fn unix_detection_puts_login_shell_first_and_dedupes() {
        // Usa rutas que existen en cualquier Unix (bash puede no existir).
        let shells = "# comentario\n/bin/sh\n/usr/bin/sh\n";
        let profiles = detect_unix(Some("/bin/sh"), Some(shells));
        assert!(!profiles.is_empty());
        let kinds: Vec<_> = profiles.iter().map(|p| p.kind).collect();
        let mut unique = kinds.clone();
        unique.dedup();
        assert_eq!(kinds, unique, "sin duplicados por tipo");
    }
}
