//! Codificación binaria de los mensajes de un panel hacia la UI.
//!
//! Un único canal ordenado transporta datos y eventos:
//!
//! ```text
//! Datos:  [0][región u8][flags u8][bloque u64 LE][bytes…]   (cabecera de 11 bytes)
//!         flags bit 0 = pantalla alternativa; bloque 0 = sin bloque
//! Evento: [1][JSON de PaneEvent]
//! ```
//!
//! Evita base64 y JSON para los bytes del PTY, que son la mayor parte del tráfico.

use crate::tracker::{DataChunk, TrackerOutput};
use mksh_core::PaneEvent;

pub const KIND_DATA: u8 = 0;
pub const KIND_EVENT: u8 = 1;
pub const DATA_HEADER: usize = 11;

/// Mensaje de un panel.
#[derive(Debug, Clone, PartialEq)]
pub enum PaneFrame {
    Data(DataChunk),
    Event(PaneEvent),
}

impl From<TrackerOutput> for PaneFrame {
    fn from(o: TrackerOutput) -> Self {
        match o {
            TrackerOutput::Data(d) => PaneFrame::Data(d),
            TrackerOutput::Event(e) => PaneFrame::Event(e),
        }
    }
}

impl PaneFrame {
    /// Bytes de datos del PTY que transporta (para el control de flujo).
    pub fn payload_len(&self) -> usize {
        match self {
            PaneFrame::Data(d) => d.bytes.len(),
            PaneFrame::Event(_) => 0,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            PaneFrame::Data(d) => {
                let mut v = Vec::with_capacity(DATA_HEADER + d.bytes.len());
                v.push(KIND_DATA);
                v.push(d.region as u8);
                v.push(u8::from(d.alt));
                v.extend_from_slice(&d.block.map(|b| b.0).unwrap_or(0).to_le_bytes());
                v.extend_from_slice(&d.bytes);
                v
            }
            PaneFrame::Event(e) => {
                let mut v = vec![KIND_EVENT];
                // Serializar un enum propio no puede fallar.
                v.extend_from_slice(&serde_json::to_vec(e).expect("evento serializable"));
                v
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mksh_core::{BlockId, Region};

    #[test]
    fn data_header_layout() {
        let f = PaneFrame::Data(DataChunk {
            region: Region::Output,
            block: Some(BlockId(258)),
            alt: true,
            bytes: b"hi".to_vec(),
        });
        assert_eq!(
            f.encode(),
            vec![0, 3, 1, 2, 1, 0, 0, 0, 0, 0, 0, b'h', b'i']
        );
    }

    #[test]
    fn event_is_json() {
        let f = PaneFrame::Event(PaneEvent::AltScreen { on: true });
        let enc = f.encode();
        assert_eq!(enc[0], KIND_EVENT);
        assert_eq!(&enc[1..], br#"{"type":"altScreen","on":true}"#);
    }
}
