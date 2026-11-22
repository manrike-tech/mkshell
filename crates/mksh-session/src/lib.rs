//! Sesiones, paneles y segmentación de bloques.
//!
//! * [`tracker`]: convierte bytes del PTY en regiones y eventos de bloque.
//! * [`runtime`]: un panel vivo (PTY + tracker + canal hacia la UI).
//! * [`manager`]: sesiones con su árbol de paneles.
//! * [`frame`]: codificación binaria de los mensajes hacia la UI.
//! * [`flow`]: control de flujo por marcas de agua.

pub mod flow;
pub mod frame;
pub mod manager;
pub mod runtime;
pub mod tracker;

pub use frame::PaneFrame;
pub use manager::{ChangeListener, ManagerOptions, SessionInfo, SessionManager};
pub use runtime::{BlockHook, BlockRecord, FrameSink, PaneInfo, PaneOptions, PaneRuntime};
pub use tracker::{DataChunk, Tracker, TrackerOutput};
