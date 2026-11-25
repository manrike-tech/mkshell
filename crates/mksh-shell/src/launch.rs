//! Preparación del lanzamiento de un perfil con la integración activada.

use crate::{Profile, ShellKind};
use std::io;
use std::path::{Path, PathBuf};

const BASH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.bash");
const ZSH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.zsh");
const FISH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.fish");
const PWSH_SCRIPT: &str = include_str!("../../../shell-integration/mkshell.ps1");
const ZSH_ENV: &str = include_str!("../../../shell-integration/zsh/.zshenv");
const ZSH_PROFILE: &str = include_str!("../../../shell-integration/zsh/.zprofile");
const ZSH_RC: &str = include_str!("../../../shell-integration/zsh/.zshrc");
const ZSH_LOGIN: &str = include_str!("../../../shell-integration/zsh/.zlogin");

/// rcfile de Bash: reproduce el arranque normal y añade la integración.
const BASH_RCFILE: &str = r#"# MKShell: rcfile temporal. Reproduce el arranque de bash y carga la integración.
if [ -n "${MKSH_BASH_LOGIN-}" ]; then
    unset MKSH_BASH_LOGIN
    [ -r /etc/profile ] && . /etc/profile
    for __mksh_f in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
        if [ -r "$__mksh_f" ]; then . "$__mksh_f"; break; fi
    done
    unset __mksh_f
else
    [ -r "$HOME/.bashrc" ] && . "$HOME/.bashrc"
fi
. "${BASH_SOURCE[0]%/*}/mkshell.bash"
"#;

/// Scripts de integración instalados en un directorio propio de MKShell.
#[derive(Debug, Clone)]
pub struct IntegrationAssets {
    dir: PathBuf,
}

impl IntegrationAssets {
    /// Escribe los scripts en `base/<versión>/` (solo si cambiaron) y devuelve
    /// su ubicación. `base` suele ser el directorio de datos de la aplicación.
    pub fn install(base: &Path) -> io::Result<Self> {
        let dir = base.join(env!("CARGO_PKG_VERSION"));
        let zsh = dir.join("zsh");
        std::fs::create_dir_all(&zsh)?;
        let files: [(PathBuf, &str); 9] = [
            (dir.join("mkshell.bash"), BASH_SCRIPT),
            (dir.join("bashrc"), BASH_RCFILE),
            (dir.join("mkshell.zsh"), ZSH_SCRIPT),
            (dir.join("mkshell.fish"), FISH_SCRIPT),
            (dir.join("mkshell.ps1"), PWSH_SCRIPT),
            (zsh.join(".zshenv"), ZSH_ENV),
            (zsh.join(".zprofile"), ZSH_PROFILE),
            (zsh.join(".zshrc"), ZSH_RC),
            (zsh.join(".zlogin"), ZSH_LOGIN),
        ];
        for (path, content) in files {
            let current = std::fs::read_to_string(&path).ok();
            if current.as_deref() != Some(content) {
                std::fs::write(&path, content)?;
            }
        }
        Ok(Self { dir })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

/// Programa, argumentos y entorno listos para `mksh-pty`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub env_remove: Vec<String>,
    /// `true` si se inyectó la integración (se esperan marcas OSC 133).
    pub integrated: bool,
}

/// Construye el lanzamiento de `profile`. `inherited_env` permite inyectar el
/// entorno en pruebas; en producción se pasa `std::env::var`.
pub fn prepare_launch(
    profile: &Profile,
    assets: Option<&IntegrationAssets>,
    inherited_env: &dyn Fn(&str) -> Option<String>,
) -> LaunchPlan {
    let mut plan = LaunchPlan {
        program: profile.program.clone(),
        args: profile.args.clone(),
        env: profile
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        env_remove: Vec::new(),
        integrated: false,
    };
    if cfg!(unix) {
        plan.env.push(("TERM".into(), "xterm-256color".into()));
    }
    plan.env.push(("COLORTERM".into(), "truecolor".into()));
    plan.env.push(("TERM_PROGRAM".into(), "MKShell".into()));
    plan.env.push((
        "TERM_PROGRAM_VERSION".into(),
        env!("CARGO_PKG_VERSION").into(),
    ));
    // Evita heredar marcas de una integración anterior (MKShell dentro de MKShell).
    plan.env_remove.push("__MKSH_LOADED".into());

    let Some(assets) = assets.filter(|_| profile.integration) else {
        return plan;
    };
    let dir = assets.dir();
    match profile.kind {
        ShellKind::Bash => {
            let login = plan.args.iter().any(|a| a == "-l" || a == "--login");
            plan.args
                .retain(|a| a != "-l" && a != "--login" && a != "-i");
            if login {
                plan.env.push(("MKSH_BASH_LOGIN".into(), "1".into()));
            }
            plan.args.splice(
                0..0,
                [
                    "--rcfile".to_string(),
                    path_str(&dir.join("bashrc")),
                    "-i".to_string(),
                ],
            );
            plan.integrated = true;
        }
        ShellKind::Zsh => {
            let user_zdotdir = inherited_env("ZDOTDIR")
                .or_else(|| inherited_env("HOME"))
                .unwrap_or_default();
            plan.env.push(("MKSH_USER_ZDOTDIR".into(), user_zdotdir));
            plan.env
                .push(("ZDOTDIR".into(), path_str(&dir.join("zsh"))));
            if !plan.args.iter().any(|a| a == "-i") {
                plan.args.push("-i".into());
            }
            plan.integrated = true;
        }
        ShellKind::Fish => {
            let script = path_str(&dir.join("mkshell.fish"))
                .replace('\\', "\\\\")
                .replace('\'', "\\'");
            plan.args.push("--init-command".into());
            plan.args.push(format!("source '{script}'"));
            plan.integrated = true;
        }
        ShellKind::Pwsh | ShellKind::PowerShell => {
            if !plan
                .args
                .iter()
                .any(|a| a.eq_ignore_ascii_case("-command") || a.eq_ignore_ascii_case("-c"))
            {
                let script = path_str(&dir.join("mkshell.ps1")).replace('\'', "''");
                plan.args.extend([
                    "-NoLogo".to_string(),
                    "-NoExit".to_string(),
                    "-Command".to_string(),
                    format!(". '{script}'"),
                ]);
                plan.integrated = true;
            }
        }
        ShellKind::Cmd => {
            let current = inherited_env("PROMPT");
            plan.env
                .push(("PROMPT".into(), cmd_prompt(current.as_deref())));
            if plan.args.is_empty() {
                plan.args = vec!["/k".into(), "chcp 65001>nul".into()];
            }
            plan.integrated = true;
        }
        ShellKind::Wsl | ShellKind::Posix | ShellKind::Other => {}
    }
    plan
}

/// Prompt de CMD con marcas OSC 133 y OSC 9;9 (según la documentación de
/// Windows Terminal). CMD no expone el código de salida en el prompt, así que
/// D va sin código, y `mkc=0` avisa de que no hay marca C.
pub fn cmd_prompt(existing: Option<&str>) -> String {
    let base = existing.filter(|p| !p.is_empty()).unwrap_or("$P$G");
    format!("$e]133;D$e\\$e]133;A;mkc=0$e\\$e]9;9;$P$e\\{base}$e]133;B$e\\")
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_none(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn bash_login_becomes_rcfile_with_flag() {
        let tmp = std::env::temp_dir().join(format!("mksh-assets-{}", std::process::id()));
        let assets = IntegrationAssets::install(&tmp).unwrap();
        let p = Profile::new("Bash", "/bin/bash", vec!["-l".into()]);
        let plan = prepare_launch(&p, Some(&assets), &env_none);
        assert_eq!(plan.args[0], "--rcfile");
        assert!(plan.args[1].ends_with("bashrc"));
        assert!(!plan.args.contains(&"-l".to_string()));
        assert!(plan.env.contains(&("MKSH_BASH_LOGIN".into(), "1".into())));
        assert!(plan.integrated);
        assert!(assets.dir().join("zsh/.zshrc").exists());
        std::fs::remove_dir_all(tmp).ok();
    }

    #[test]
    fn zsh_uses_temporary_zdotdir() {
        let tmp = std::env::temp_dir().join(format!("mksh-assets-z-{}", std::process::id()));
        let assets = IntegrationAssets::install(&tmp).unwrap();
        let p = Profile::new("Zsh", "/usr/bin/zsh", vec![]);
        let home = |k: &str| (k == "HOME").then(|| "/home/u".to_string());
        let plan = prepare_launch(&p, Some(&assets), &home);
        assert!(
            plan.env
                .contains(&("MKSH_USER_ZDOTDIR".into(), "/home/u".into()))
        );
        assert!(
            plan.env
                .iter()
                .any(|(k, v)| k == "ZDOTDIR" && v.ends_with("zsh"))
        );
        std::fs::remove_dir_all(tmp).ok();
    }

    #[test]
    fn integration_can_be_disabled() {
        let tmp = std::env::temp_dir().join(format!("mksh-assets-d-{}", std::process::id()));
        let assets = IntegrationAssets::install(&tmp).unwrap();
        let mut p = Profile::new("Bash", "/bin/bash", vec![]);
        p.integration = false;
        let plan = prepare_launch(&p, Some(&assets), &env_none);
        assert!(!plan.integrated);
        assert!(plan.args.is_empty());
        std::fs::remove_dir_all(tmp).ok();
    }

    #[test]
    fn cmd_prompt_wraps_existing() {
        let p = cmd_prompt(Some("[$P]"));
        assert!(p.contains("[$P]"));
        assert!(p.starts_with("$e]133;D$e\\"));
        assert!(p.ends_with("$e]133;B$e\\"));
        assert!(cmd_prompt(None).contains("$P$G"));
    }
}
