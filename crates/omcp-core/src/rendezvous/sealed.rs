//! Sealed-sender box (PROTOCOL §9.2) — **HPKE-style, not RFC 9180 wire-compatible**.
//!
//! ```text
//! eph            = X25519 ephemeral
//! ss             = X25519(eph, recipient_sealed_pk)
//! key            = HKDF-SHA-256(salt = eph_pk || recipient_pk, ikm = ss,
//!                               info = "OMCP-SealedSender-v1", L = 32)
//! sealed         = eph_pk(32) || ChaCha20-Poly1305(key, nonce = 0^12, aad = label, pt)
//! ```
//! Each key is single-use (fresh ephemeral), so the fixed nonce is safe. The
//! sender identity lives *inside* the plaintext; gateways see only `label`.
//! A follow-up may swap this for RFC 9180 HPKE (DHKEM-X25519/HKDF-SHA256/ChaCha).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::Zeroize;

use super::identity::{random_bytes, RendezvousIdentity};
use super::label::BlindedLabel;
use crate::error::{OmcpError, Result};

const INFO: &[u8] = b"OMCP-SealedSender-v1";

fn derive(ss: &[u8; 32], eph: &[u8; 32], recip: &[u8; 32]) -> Result<[u8; 32]> {
    let mut salt = [0u8; 64];
    salt[..32].copy_from_slice(eph);
    salt[32..].copy_from_slice(recip);
    let mut out = [0u8; 32];
    Hkdf::<Sha256>::new(Some(&salt), ss)
        .expand(INFO, &mut out)
        .map_err(|_| OmcpError::Crypto("hkdf"))?;
    Ok(out)
}

/// Seal `plaintext` to `recipient_sealed_pk`, binding the blinded `label`.
pub fn seal(
    recipient_sealed_pk: &[u8; 32],
    label: &BlindedLabel,
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let eph = StaticSecret::from(random_bytes::<32>()?);
    let eph_pk = PublicKey::from(&eph).to_bytes();
    let ss = eph.diffie_hellman(&PublicKey::from(*recipient_sealed_pk));
    let mut key = derive(ss.as_bytes(), &eph_pk, recipient_sealed_pk)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    key.zeroize();
    let ct = cipher
        .encrypt(
            Nonce::from_slice(&[0u8; 12]),
            Payload {
                msg: plaintext,
                aad: label,
            },
        )
        .map_err(|_| OmcpError::Crypto("sealed-sender seal"))?;
    let mut out = Vec::with_capacity(32 + ct.len());
    out.extend_from_slice(&eph_pk);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Open a sealed box addressed to `me` under `label`.
pub fn open(me: &RendezvousIdentity, label: &BlindedLabel, sealed: &[u8]) -> Result<Vec<u8>> {
    if sealed.len() < 32 + 16 {
        return Err(OmcpError::Truncated {
            need: 48,
            have: sealed.len(),
        });
    }
    let mut eph_pk = [0u8; 32];
    eph_pk.copy_from_slice(&sealed[..32]);
    let ss = me.sealed_secret().diffie_hellman(&PublicKey::from(eph_pk));
    let mut key = derive(ss.as_bytes(), &eph_pk, &me.sealed_public())?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    key.zeroize();
    cipher
        .decrypt(
            Nonce::from_slice(&[0u8; 12]),
            Payload {
                msg: &sealed[32..],
                aad: label,
            },
        )
        .map_err(|_| OmcpError::AppAead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip_and_wrong_key_fails() {
        let bob = RendezvousIdentity::generate().unwrap();
        let eve = RendezvousIdentity::generate().unwrap();
        let label = [9u8; 32];
        let boxed = seal(&bob.sealed_public(), &label, b"hi bob").unwrap();
        assert_eq!(open(&bob, &label, &boxed).unwrap(), b"hi bob");
        assert!(open(&eve, &label, &boxed).is_err());
        assert!(open(&bob, &[8u8; 32], &boxed).is_err());
    }
}
