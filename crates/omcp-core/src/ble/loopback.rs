//! In-memory duplex pipe simulating GATT Inbox write + Outbox notify.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use crate::error::{OmcpError, Result};

/// One end of a loopback BLE link (no real BlueZ).
///
/// Semantics map to PROTOCOL §8.1:
/// - [`BleEndpoint::send`] ≈ write to the peer's **Inbox** characteristic, or
///   notify on our **Outbox** toward the peer (same byte path in loopback).
/// - [`BleEndpoint::recv`] ≈ read a pending notification / inbox write from the peer.
#[derive(Clone)]
pub struct BleEndpoint {
    tx: Arc<Mutex<VecDeque<Vec<u8>>>>,
    rx: Arc<Mutex<VecDeque<Vec<u8>>>>,
}

impl BleEndpoint {
    /// Deliver a GATT frame to the peer.
    pub fn send(&self, frame: Vec<u8>) -> Result<()> {
        self.tx
            .lock()
            .map_err(|_| OmcpError::Crypto("loopback lock"))?
            .push_back(frame);
        Ok(())
    }

    /// Pop the next frame from the peer, if any.
    pub fn recv(&self) -> Result<Option<Vec<u8>>> {
        Ok(self
            .rx
            .lock()
            .map_err(|_| OmcpError::Crypto("loopback lock"))?
            .pop_front())
    }

    /// Number of queued inbound frames (test helper).
    pub fn pending(&self) -> Result<usize> {
        Ok(self
            .rx
            .lock()
            .map_err(|_| OmcpError::Crypto("loopback lock"))?
            .len())
    }
}

/// Factory for a paired in-process BLE link.
pub struct BleLoopback;

impl BleLoopback {
    /// Create two cross-linked endpoints (peer A ↔ peer B).
    pub fn pair() -> (BleEndpoint, BleEndpoint) {
        let a_inbox = Arc::new(Mutex::new(VecDeque::new()));
        let b_inbox = Arc::new(Mutex::new(VecDeque::new()));
        let a = BleEndpoint {
            tx: Arc::clone(&b_inbox),
            rx: Arc::clone(&a_inbox),
        };
        let b = BleEndpoint {
            tx: Arc::clone(&a_inbox),
            rx: Arc::clone(&b_inbox),
        };
        (a, b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplex_delivers_both_ways() {
        let (a, b) = BleLoopback::pair();
        a.send(b"ping".to_vec()).unwrap();
        b.send(b"pong".to_vec()).unwrap();
        assert_eq!(b.recv().unwrap().unwrap(), b"ping");
        assert_eq!(a.recv().unwrap().unwrap(), b"pong");
        assert!(a.recv().unwrap().is_none());
    }
}
