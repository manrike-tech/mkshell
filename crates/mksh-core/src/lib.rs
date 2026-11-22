//! Tipos de dominio compartidos por todos los crates de MKShell.
//!
//! Este crate no depende de Tauri ni de ningún runtime: define identificadores,
//! el árbol de paneles, los comandos del bus y los eventos que emite el núcleo.

pub mod command;
pub mod event;
pub mod ids;
pub mod layout;

pub use command::{CommandId, Origin, SplitDirection};
pub use event::{PaneEvent, Region};
pub use ids::{BlockId, PaneId, SessionId, SplitId};
pub use layout::{Direction, LayoutNode, LayoutTree, Rect};

/// Error genérico del dominio.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("panel no encontrado: {0}")]
    PaneNotFound(PaneId),
    #[error("sesión no encontrada: {0}")]
    SessionNotFound(SessionId),
    #[error("división no encontrada: {0}")]
    SplitNotFound(SplitId),
    #[error("no se puede cerrar el último panel de la sesión")]
    LastPane,
}
