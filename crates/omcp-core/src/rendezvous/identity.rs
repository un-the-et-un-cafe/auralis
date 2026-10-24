//! Rendezvous identity keys.
//!
//! **Choice (documented):** token signatures use **Ed25519** (`ed25519-dalek`,
//! already in the dependency graph via OpenMLS). The M1 Noise static identity is
//! X25519-only and cannot sign, so the Peer Identity Key role for §9.2 tokens is
//! an Ed25519 key. Sealed-sender uses a separate X25519 key so signing and
//! key-agreement keys are never shared.

use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
use x25519_dalek::{PublicKey, StaticSecret};

use crate::error::{OmcpError, Result};

pub(crate) fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|_| OmcpError::Crypto("getrandom"))?;
    Ok(b)
}

/// Ed25519 signing key + X25519 sealed-sender key for one peer.
pub struct RendezvousIdentity {
    signing: SigningKey,
    sealed: StaticSecret,
}

impl RendezvousIdentity {
    /// Generate fresh keys from the OS RNG.
    pub fn generate() -> Result<Self> {
        Ok(Self {
            signing: SigningKey::from_bytes(&random_bytes::<32>()?),
            sealed: StaticSecret::from(random_bytes::<32>()?),
        })
    }

    /// Ed25519 public key (32 bytes); used for token verification and label blinding.
    pub fn verifying_key(&self) -> [u8; 32] {
        self.signing.verifying_key().to_bytes()
    }

    /// X25519 public key that senders seal to.
    pub fn sealed_public(&self) -> [u8; 32] {
        PublicKey::from(&self.sealed).to_bytes()
    }

    pub(crate) fn sign(&self, msg: &[u8]) -> [u8; 64] {
        self.signing.sign(msg).to_bytes()
    }

    pub(crate) fn sealed_secret(&self) -> &StaticSecret {
        &self.sealed
    }
}

/// Verify an Ed25519 signature (strict verification).
pub(crate) fn verify(pk: &[u8; 32], msg: &[u8], sig: &[u8; 64]) -> Result<()> {
    let vk = VerifyingKey::from_bytes(pk).map_err(|_| OmcpError::TokenRejected("bad key"))?;
    let sig = ed25519_dalek::Signature::from_bytes(sig);
    vk.verify_strict(msg, &sig)
        .map_err(|_| OmcpError::TokenRejected("bad signature"))
}
