//! Shells: detección, perfiles e integración OSC 133.
//!
//! * [`ShellKind`] clasifica un ejecutable (bash, zsh, fish, pwsh, cmd…).
//! * [`detect_profiles`] encuentra los shells instalados en el sistema.
//! * [`IntegrationAssets`] instala los scripts de integración en un directorio
//!   de la aplicación (nunca en los dotfiles del usuario).
//! * [`prepare_launch`] construye programa, argumentos y entorno para lanzar un
//!   perfil con la integración activada.
//! * [`ssh`] analiza comandos `ssh` y genera el arranque remoto en memoria.

mod detect;
mod launch;
pub mod ssh;

pub use detect::detect_profiles;
pub use launch::{IntegrationAssets, LaunchPlan, cmd_prompt, prepare_launch};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Tipo de shell, deducido del nombre del ejecutable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    /// PowerShell 7+ (`pwsh`).
    Pwsh,
    /// Windows PowerShell 5.1 (`powershell.exe`).
    PowerShell,
    Cmd,
    /// Lanzador de WSL (`wsl.exe`): el shell real vive dentro de la distro.
    Wsl,
    /// `sh`, `dash`, `ash`, BusyBox…: sin hooks de prompt fiables.
    Posix,
    Other,
}

impl ShellKind {
    /// Clasifica por el nombre del ejecutable (sin distinguir mayúsculas ni `.exe`).
    pub fn from_program(program: &str) -> Self {
        // Se separa por `/` y `\` en cualquier plataforma: un perfil puede
        // describir rutas de Windows aunque se valide en Linux (y viceversa).
        let name = program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(program)
            .to_ascii_lowercase();
        let name = name.strip_suffix(".exe").unwrap_or(&name);
        match name {
            "bash" => Self::Bash,
            "zsh" => Self::Zsh,
            "fish" => Self::Fish,
            "pwsh" | "pwsh-preview" => Self::Pwsh,
            "powershell" => Self::PowerShell,
            "cmd" => Self::Cmd,
            "wsl" => Self::Wsl,
            "sh" | "dash" | "ash" | "ksh" | "mksh" | "busybox" => Self::Posix,
            _ => Self::Other,
        }
    }

    /// Indica si MKShell sabe inyectar integración al lanzar este shell.
    pub fn supports_integration(self) -> bool {
        matches!(
            self,
            Self::Bash | Self::Zsh | Self::Fish | Self::Pwsh | Self::PowerShell | Self::Cmd
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bash => "Bash",
            Self::Zsh => "Zsh",
            Self::Fish => "Fish",
            Self::Pwsh => "PowerShell",
            Self::PowerShell => "Windows PowerShell",
            Self::Cmd => "Símbolo del sistema",
            Self::Wsl => "WSL",
            Self::Posix => "sh",
            Self::Other => "Shell",
        }
    }
}

/// Perfil de terminal: qué shell lanzar y cómo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    /// Identificador estable (slug del nombre).
    pub id: String,
    pub name: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub kind: ShellKind,
    /// Si `false`, el panel arranca en modo TTY clásico.
    #[serde(default = "default_true")]
    pub integration: bool,
}

fn default_true() -> bool {
    true
}

impl Profile {
    pub fn new(name: impl Into<String>, program: impl Into<String>, args: Vec<String>) -> Self {
        let name = name.into();
        let program = program.into();
        Self {
            id: slug(&name),
            kind: ShellKind::from_program(&program),
            name,
            program,
            args,
            cwd: None,
            env: BTreeMap::new(),
            integration: true,
        }
    }
}

/// Convierte un nombre en identificador: minúsculas, alfanumérico y guiones.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in name.chars().flat_map(|c| c.to_lowercase()) {
        let c = match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { "shell".into() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_programs() {
        assert_eq!(ShellKind::from_program("/usr/bin/bash"), ShellKind::Bash);
        assert_eq!(
            ShellKind::from_program("C:\\Windows\\System32\\cmd.exe"),
            ShellKind::Cmd
        );
        assert_eq!(ShellKind::from_program("PWSH.EXE"), ShellKind::Pwsh);
        assert_eq!(ShellKind::from_program("/bin/dash"), ShellKind::Posix);
        assert_eq!(ShellKind::from_program("nu"), ShellKind::Other);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("WSL: Ubuntu 24.04"), "wsl-ubuntu-24-04");
        assert_eq!(slug("Símbolo del sistema"), "simbolo-del-sistema");
        assert_eq!(slug("  "), "shell");
    }
}
