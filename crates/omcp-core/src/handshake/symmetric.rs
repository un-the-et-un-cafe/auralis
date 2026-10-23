//! Minimal Noise symmetric-state primitives for BLAKE2b + ChaCha20-Poly1305.
//!
//! Implements the constructions from the Noise Protocol Framework (revision 34)
//! needed for Noise_IK, not a full Noise library.

use blake2::{Blake2b512, Digest};
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{OmcpError, Result};

pub(crate) const HASHLEN: usize = 64;
pub(crate) const DHLEN: usize = 32;
const TAGLEN: usize = 16;

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct CipherState {
    key: [u8; 32],
    nonce: u64,
}

impl CipherState {
    fn new(key: [u8; 32]) -> Self {
        Self { key, nonce: 0 }
    }

    fn nonce_bytes(&self) -> Nonce {
        let mut n = [0u8; 12];
        n[4..].copy_from_slice(&self.nonce.to_le_bytes());
        Nonce::from(n)
    }

    pub(crate) fn encrypt_with_ad(&mut self, ad: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let out = cipher
            .encrypt(
                &self.nonce_bytes(),
                Payload {
                    msg: plaintext,
                    aad: ad,
                },
            )
            .map_err(|_| OmcpError::HandshakeAead)?;
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(OmcpError::Crypto("nonce overflow"))?;
        Ok(out)
    }

    pub(crate) fn decrypt_with_ad(&mut self, ad: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
        let cipher = ChaCha20Poly1305::new_from_slice(&self.key)
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let out = cipher
            .decrypt(
                &self.nonce_bytes(),
                Payload {
                    msg: ciphertext,
                    aad: ad,
                },
            )
            .map_err(|_| OmcpError::HandshakeAead)?;
        self.nonce = self
            .nonce
            .checked_add(1)
            .ok_or(OmcpError::Crypto("nonce overflow"))?;
        Ok(out)
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub(crate) struct SymmetricState {
    ck: [u8; HASHLEN],
    h: [u8; HASHLEN],
    #[zeroize(skip)]
    cipher: Option<CipherState>,
}

impl SymmetricState {
    /// Initialize from protocol name (Noise §5.2).
    pub(crate) fn initialize(protocol_name: &[u8]) -> Self {
        let h = if protocol_name.len() <= HASHLEN {
            let mut h = [0u8; HASHLEN];
            h[..protocol_name.len()].copy_from_slice(protocol_name);
            h
        } else {
            hash(protocol_name)
        };
        Self {
            ck: h,
            h,
            cipher: None,
        }
    }

    pub(crate) fn mix_hash(&mut self, data: &[u8]) {
        let mut d = Blake2b512::new();
        d.update(self.h);
        d.update(data);
        self.h = d.finalize().into();
    }

    pub(crate) fn mix_key(&mut self, input_key_material: &[u8]) {
        let (ck, temp_k) = hkdf_noise(&self.ck, input_key_material, 2);
        self.ck = ck;
        let mut key = [0u8; 32];
        key.copy_from_slice(&temp_k[..32]);
        self.cipher = Some(CipherState::new(key));
    }

    pub(crate) fn encrypt_and_hash(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let ciphertext = match self.cipher.as_mut() {
            Some(c) => c.encrypt_with_ad(&self.h, plaintext)?,
            None => plaintext.to_vec(),
        };
        self.mix_hash(&ciphertext);
        Ok(ciphertext)
    }

    pub(crate) fn decrypt_and_hash(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let plaintext = match self.cipher.as_mut() {
            Some(c) => c.decrypt_with_ad(&self.h, ciphertext)?,
            None => ciphertext.to_vec(),
        };
        self.mix_hash(ciphertext);
        Ok(plaintext)
    }

    pub(crate) fn handshake_hash(&self) -> [u8; HASHLEN] {
        self.h
    }

    /// Split into two 32-byte transport keys (Noise §5.2), returning `(k1, k2, ck)`.
    pub(crate) fn split(self) -> ([u8; 32], [u8; 32], [u8; HASHLEN]) {
        let (k1_full, k2_full) = hkdf_noise(&self.ck, &[], 2);
        let mut k1 = [0u8; 32];
        let mut k2 = [0u8; 32];
        k1.copy_from_slice(&k1_full[..32]);
        k2.copy_from_slice(&k2_full[..32]);
        (k1, k2, self.ck)
    }
}

fn hash(data: &[u8]) -> [u8; HASHLEN] {
    let mut d = Blake2b512::new();
    d.update(data);
    d.finalize().into()
}

/// Noise HKDF using BLAKE2b (HMAC-HASH).
fn hkdf_noise(chaining_key: &[u8], ikm: &[u8], n: usize) -> ([u8; HASHLEN], [u8; HASHLEN]) {
    debug_assert!(n == 2 || n == 3);
    let temp_key = hmac(chaining_key, ikm);
    let out1 = hmac(&temp_key, &[0x01]);
    let mut data = Vec::with_capacity(HASHLEN + 1);
    data.extend_from_slice(&out1);
    data.push(0x02);
    let out2 = hmac(&temp_key, &data);
    (out1, out2)
}

fn hmac(key: &[u8], data: &[u8]) -> [u8; HASHLEN] {
    // HMAC per Noise: B = HASHLEN; ipad/opad over block size 128 for BLAKE2b.
    const BLOCK: usize = 128;
    let mut k_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        k_block[..HASHLEN].copy_from_slice(&hash(key));
    } else {
        k_block[..key.len()].copy_from_slice(key);
    }
    let mut ikey = [0u8; BLOCK];
    let mut okey = [0u8; BLOCK];
    for i in 0..BLOCK {
        ikey[i] = k_block[i] ^ 0x36;
        okey[i] = k_block[i] ^ 0x5c;
    }
    let mut inner = Blake2b512::new();
    inner.update(ikey);
    inner.update(data);
    let inner_hash: [u8; HASHLEN] = inner.finalize().into();
    let mut outer = Blake2b512::new();
    outer.update(okey);
    outer.update(inner_hash);
    outer.finalize().into()
}

/// Encrypt length used when sealing a DH public key (32 + tag).
pub(crate) fn sealed_dh_len() -> usize {
    DHLEN + TAGLEN
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_hash_changes_h() {
        let mut s = SymmetricState::initialize(b"test");
        let h0 = s.h;
        s.mix_hash(b"abc");
        assert_ne!(s.h, h0);
    }
}
