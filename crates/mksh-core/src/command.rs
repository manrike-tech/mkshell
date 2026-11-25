//! Bus de comandos.
//!
//! Toda acción de la aplicación tiene un identificador estable (`pane.split_vertical`,
//! `session.new`…). Los atajos de teclado, la paleta de comandos, los menús y, en
//! fases posteriores, los agentes de IA y los plugins emiten estos mismos
//! identificadores, de modo que la política de permisos y la auditoría se aplican
//! en un único punto.

use serde::{Deserialize, Serialize};

/// Quién origina una acción. Los agentes nunca comparten origen con el usuario.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Origin {
    /// Acción iniciada por la persona usuaria desde la UI.
    #[default]
    User,
    /// Acción propuesta por un agente (fase 4). Requiere aprobación explícita.
    Agent {
        /// Proveedor y modelo, p. ej. `ollama/qwen` o `anthropic/claude`.
        model: String,
    },
}

impl Origin {
    pub fn is_user(&self) -> bool {
        matches!(self, Origin::User)
    }
}

/// Dirección de una división de panel.
///
/// * `Vertical`: divisor vertical, paneles lado a lado (equivale a `tmux split-window -h`).
/// * `Horizontal`: divisor horizontal, paneles apilados (equivale a `tmux split-window -v`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitDirection {
    Vertical,
    Horizontal,
}

/// Descriptor de un comando registrado.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandId {
    /// Identificador estable usado en atajos y configuración.
    pub id: &'static str,
    /// Descripción legible (para la paleta de comandos).
    pub title: &'static str,
    /// Fase en la que el comando pasa a estar operativo.
    pub phase: u8,
}

/// Catálogo de comandos conocidos. La UI valida los atajos contra esta lista.
pub const COMMANDS: &[CommandId] = &[
    CommandId {
        id: "session.new",
        title: "Nueva sesión",
        phase: 2,
    },
    CommandId {
        id: "session.close",
        title: "Cerrar sesión",
        phase: 2,
    },
    CommandId {
        id: "session.rename",
        title: "Renombrar sesión",
        phase: 2,
    },
    CommandId {
        id: "session.next",
        title: "Sesión siguiente",
        phase: 2,
    },
    CommandId {
        id: "session.prev",
        title: "Sesión anterior",
        phase: 2,
    },
    CommandId {
        id: "pane.split_vertical",
        title: "Dividir panel (lado a lado)",
        phase: 2,
    },
    CommandId {
        id: "pane.split_horizontal",
        title: "Dividir panel (arriba/abajo)",
        phase: 2,
    },
    CommandId {
        id: "pane.close",
        title: "Cerrar panel",
        phase: 2,
    },
    CommandId {
        id: "pane.focus_left",
        title: "Foco al panel izquierdo",
        phase: 2,
    },
    CommandId {
        id: "pane.focus_right",
        title: "Foco al panel derecho",
        phase: 2,
    },
    CommandId {
        id: "pane.focus_up",
        title: "Foco al panel superior",
        phase: 2,
    },
    CommandId {
        id: "pane.focus_down",
        title: "Foco al panel inferior",
        phase: 2,
    },
    CommandId {
        id: "pane.search",
        title: "Buscar en la salida",
        phase: 2,
    },
    CommandId {
        id: "pane.clear",
        title: "Limpiar salida",
        phase: 2,
    },
    CommandId {
        id: "pane.toggle_tty",
        title: "Alternar modo TTY clásico",
        phase: 2,
    },
    CommandId {
        id: "pane.interrupt",
        title: "Interrumpir comando (Ctrl+C)",
        phase: 2,
    },
    CommandId {
        id: "input.focus",
        title: "Foco en Command Input",
        phase: 2,
    },
    CommandId {
        id: "clipboard.copy",
        title: "Copiar",
        phase: 2,
    },
    CommandId {
        id: "clipboard.paste",
        title: "Pegar",
        phase: 2,
    },
    CommandId {
        id: "palette.open",
        title: "Paleta de comandos",
        phase: 3,
    },
    CommandId {
        id: "ai.toggle_panel",
        title: "Mostrar/ocultar panel de IA",
        phase: 4,
    },
    CommandId {
        id: "mode.toggle",
        title: "Alternar Terminal Mode / IDE Mode",
        phase: 5,
    },
];

/// Devuelve el descriptor de un comando por su identificador.
pub fn find_command(id: &str) -> Option<&'static CommandId> {
    COMMANDS.iter().find(|c| c.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_ids_are_unique() {
        let mut ids: Vec<_> = COMMANDS.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let len = ids.len();
        ids.dedup();
        assert_eq!(len, ids.len());
        assert!(find_command("pane.split_vertical").is_some());
        assert!(find_command("nope").is_none());
    }

    #[test]
    fn origin_serializes_with_kind_tag() {
        let json = serde_json::to_string(&Origin::Agent { model: "m".into() }).unwrap();
        assert_eq!(json, r#"{"kind":"agent","model":"m"}"#);
    }
}
