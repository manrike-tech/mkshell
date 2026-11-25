//! Eventos que el núcleo emite por panel.

use crate::{Origin, ids::BlockId};
use serde::{Deserialize, Serialize};

/// Región semántica de un fragmento de salida del PTY, según las marcas OSC 133.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[repr(u8)]
pub enum Region {
    /// Antes del primer prompt (mensajes de inicio, MOTD) o shell sin integración.
    Startup = 0,
    /// Texto del prompt (entre OSC 133;A y 133;B, o entre D y A).
    Prompt = 1,
    /// Eco de la línea de comandos (entre 133;B y 133;C).
    Echo = 2,
    /// Salida de un comando (entre 133;C y 133;D).
    Output = 3,
}

/// Evento de estado de un panel. Se serializa como JSON con campo `type`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PaneEvent {
    /// El shell empieza a dibujar el prompt (OSC 133;A).
    #[serde(rename_all = "camelCase")]
    PromptStart { depth: usize },
    /// El shell espera un comando (OSC 133;B).
    #[serde(rename_all = "camelCase")]
    Ready {
        depth: usize,
        cwd: Option<String>,
        host: Option<String>,
    },
    /// Comienza la salida de un comando (OSC 133;C).
    #[serde(rename_all = "camelCase")]
    BlockStarted {
        block: BlockId,
        depth: usize,
        command: Option<String>,
        cwd: Option<String>,
        host: Option<String>,
        origin: Origin,
        /// Destino `usuario@host` si el comando es un `ssh` interactivo.
        ssh: Option<String>,
    },
    /// Fin de un comando (OSC 133;D). `exit_code` es `None` si el shell no lo
    /// informa (CMD) o si el bloque se cerró de forma anómala.
    #[serde(rename_all = "camelCase")]
    BlockFinished {
        block: BlockId,
        exit_code: Option<i32>,
        duration_ms: u64,
    },
    /// Cambio de directorio (OSC 7 / OSC 9;9).
    #[serde(rename_all = "camelCase")]
    CwdChanged {
        depth: usize,
        cwd: String,
        host: Option<String>,
    },
    /// Entrada o salida de la pantalla alternativa (vim, htop, less…).
    #[serde(rename_all = "camelCase")]
    AltScreen { on: bool },
    /// Se detectó un shell anidado con integración (p. ej. tras `ssh`).
    #[serde(rename_all = "camelCase")]
    ShellEntered { depth: usize },
    /// Un shell anidado terminó.
    #[serde(rename_all = "camelCase")]
    ShellExited { depth: usize },
    /// El proceso del panel terminó.
    #[serde(rename_all = "camelCase")]
    Exited { code: Option<i32> },
}
