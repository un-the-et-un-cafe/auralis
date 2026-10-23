//! Per-link session AEAD from Noise hybrid transport keys (PROTOCOL §10.2).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{OmcpError, Result};
use crate::handshake::HybridSessionKeys;

/// Direction of traffic relative to the Noise initiator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Initiator → responder (`k_i2r`).
    InitiatorToResponder,
    /// Responder → initiator (`k_r2i`).
    ResponderToInitiator,
}

/// One half of a duplex link cipher (Noise-style counter nonces).
#[derive(Zeroize, ZeroizeOnDrop)]
struct Half {
    key: [u8; 32],
    nonce: u64,
}

impl Half {
    fn new(key: [u8; 32]) -> Self {
        Self { key, nonce: 0 }
    }

    fn nonce_bytes(&self) -> Nonce {
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&self.nonce.to_le_bytes());
        Nonce::from(n)
    }

    fn seal(&mut self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let out = cipher
            .encrypt(
                &self.nonce_bytes(),
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| OmcpError::LinkAead)?;
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(OmcpError::Crypto("nonce overflow"))?;
        Ok(out)
    }

    fn open(&mut self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let out = cipher
            .decrypt(
                &self.nonce_bytes(),
                Payload {
                    msg: ciphertext,
                    aad,
                },
            )
            .map_err(|_| OmcpError::LinkAead)?;
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(OmcpError::Crypto("nonce overflow"))?;
        Ok(out)
    }
}

/// Bidirectional link session derived from [`HybridSessionKeys`].
///
/// Each peer keeps send/recv halves matching its role (initiator or responder).
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct LinkSession {
    send: Half,
    recv: Half,
}

impl LinkSession {
    /// Build a session for the Noise **initiator** (sends with `k_i2r`, receives `k_r2i`).
    pub fn initiator(keys: &HybridSessionKeys) -> Self {
        Self {
            send: Half::new(keys.k_i2r),
            recv: Half::new(keys.k_r2i),
        }
    }

    /// Build a session for the Noise **responder** (sends with `k_r2i`, receives `k_i2r`).
    pub fn responder(keys: &HybridSessionKeys) -> Self {
        Self {
            send: Half::new(keys.k_r2i),
            recv: Half::new(keys.k_i2r),
        }
    }

    /// Encrypt a fragment payload; AAD should be the fragment header bytes.
    pub fn seal_fragment(&mut self, aad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        self.send.seal(aad, plaintext)
    }

    /// Decrypt a fragment payload.
    pub fn open_fragment(&mut self, aad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.recv.open(aad, ciphertext)
    }

    /// Directional transport key used for **sending** from this peer.
    pub fn send_key(&self) -> [u8; 32] {
        self.send.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handshake::{HybridInitiator, HybridResponder, StaticIdentity};

    fn paired_sessions() -> (LinkSession, LinkSession) {
        let responder_id = StaticIdentity::generate().unwrap();
        let initiator_id = StaticIdentity::generate().unwrap();
        let mut init = HybridInitiator::new(initiator_id, &responder_id.dh_public()).unwrap();
        let mut resp = HybridResponder::new(responder_id);
        let msg1 = init.write_message_1(b"").unwrap();
        resp.read_message_1(&msg1).unwrap();
        let (msg2, keys_r) = resp.write_message_2(b"").unwrap();
        let (_, keys_i) = init.read_message_2(&msg2).unwrap();
        (
            LinkSession::initiator(&keys_i),
            LinkSession::responder(&keys_r),
        )
    }

    #[test]
    fn link_aead_round_trip() {
        let (mut a, mut b) = paired_sessions();
        let aad = b"frag-hdr";
        let ct = a.seal_fragment(aad, b"hello-link").unwrap();
        let pt = b.open_fragment(aad, &ct).unwrap();
        assert_eq!(pt, b"hello-link");
    }

    #[test]
    fn link_aead_tamper_fails() {
        let (mut a, mut b) = paired_sessions();
        let mut ct = a.seal_fragment(b"aad", b"x").unwrap();
        ct[0] ^= 1;
        assert!(matches!(
            b.open_fragment(b"aad", &ct),
            Err(OmcpError::LinkAead)
        ));
    }
}
