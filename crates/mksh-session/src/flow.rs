//! Control de flujo por marcas de agua.
//!
//! El hilo lector del PTY suma los bytes que envía a la UI y la UI confirma
//! (`ack`) los que ya ha pintado. Si la diferencia supera `HIGH`, el lector
//! deja de leer: el proceso se bloquea al escribir en el PTY en lugar de
//! congelar la interfaz (mismo enfoque que la guía de flow control de xterm.js).

use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub const HIGH_WATERMARK: usize = 2 * 1024 * 1024;
pub const LOW_WATERMARK: usize = 512 * 1024;

#[derive(Debug, Default)]
struct State {
    inflight: usize,
    closed: bool,
}

#[derive(Debug, Default)]
pub struct FlowGate {
    state: Mutex<State>,
    cv: Condvar,
}

impl FlowGate {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, n: usize) {
        self.state.lock().expect("flow").inflight += n;
    }

    pub fn ack(&self, n: usize) {
        let mut s = self.state.lock().expect("flow");
        s.inflight = s.inflight.saturating_sub(n);
        if s.inflight <= LOW_WATERMARK {
            self.cv.notify_all();
        }
    }

    pub fn inflight(&self) -> usize {
        self.state.lock().expect("flow").inflight
    }

    /// Libera al lector para siempre (panel cerrado o UI desconectada).
    pub fn close(&self) {
        self.state.lock().expect("flow").closed = true;
        self.cv.notify_all();
    }

    /// Reinicia el contador (p. ej. al reconectar la UI).
    pub fn reset(&self) {
        self.state.lock().expect("flow").inflight = 0;
        self.cv.notify_all();
    }

    /// Bloquea mientras haya más de `HIGH` bytes sin confirmar, hasta bajar de
    /// `LOW`. Devuelve `false` si el gate se cerró.
    pub fn wait_for_room(&self) -> bool {
        let mut s = self.state.lock().expect("flow");
        if s.inflight <= HIGH_WATERMARK {
            return !s.closed;
        }
        while s.inflight > LOW_WATERMARK && !s.closed {
            s = self
                .cv
                .wait_timeout(s, Duration::from_millis(250))
                .expect("flow")
                .0;
        }
        !s.closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn blocks_until_acked() {
        let gate = Arc::new(FlowGate::new());
        gate.add(HIGH_WATERMARK + 1);
        let g = gate.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            g.ack(HIGH_WATERMARK);
        });
        let start = Instant::now();
        assert!(gate.wait_for_room());
        assert!(start.elapsed() >= Duration::from_millis(90));
        t.join().unwrap();
    }

    #[test]
    fn close_releases_waiter() {
        let gate = Arc::new(FlowGate::new());
        gate.add(HIGH_WATERMARK * 2);
        let g = gate.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            g.close();
        });
        assert!(!gate.wait_for_room());
    }
}
