//! Identificadores numéricos únicos por proceso.
//!
//! Se usan `u64` crecientes en lugar de UUID: son baratos, ordenables y se
//! serializan como números en el IPC. Cuando exista el daemon `mkshd`, el
//! contador vivirá en él y seguirá siendo único para todas las UIs conectadas.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// Genera un identificador nuevo, único en el proceso.
            pub fn new() -> Self {
                Self(next())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(
    /// Sesión: conjunto de paneles con nombre (equivalente a una sesión de tmux).
    SessionId
);
id_type!(
    /// Panel: hoja del árbol de layout con su propio PTY.
    PaneId
);
id_type!(
    /// Nodo de división del árbol de layout.
    SplitId
);
id_type!(
    /// Bloque: un comando con su salida, código de salida y duración.
    BlockId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_increasing() {
        let a = PaneId::new();
        let b = PaneId::new();
        assert!(b.0 > a.0);
        assert_eq!(serde_json::to_string(&a).unwrap(), a.0.to_string());
    }
}
