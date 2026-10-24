//! Signed per-member high-water marks (PROTOCOL §22.2).
//!
//! Each chat member signs `(chat_id, lamport)` so a returning device can ask
//! "everything after X" instead of re-listing history, and a relay cannot
//! forge a peer's progress. Signed message:
//! `"OMCP-WM-v1" || chat_id[32] || member_pk[32] || lamport_be`.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};

use super::take;
use crate::error::{OmcpError, Result};

const DOMAIN: &[u8] = b"OMCP-WM-v1";
/// Wire length of a [`SignedWatermark`].
pub const WATERMARK_LEN: usize = 32 + 32 + 8 + 64;

/// Lamport clock for one chat.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lamport(pub u64);

impl Lamport {
    /// Tick for a local event.
    pub fn tick(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }

    /// Observe a remote timestamp.
    pub fn observe(&mut self, remote: u64) {
        self.0 = self.0.max(remote) + 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedWatermark {
    pub chat_id: [u8; 32],
    pub member_pk: [u8; 32],
    pub lamport: u64,
    pub sig: [u8; 64],
}

fn signed_bytes(chat_id: &[u8; 32], member_pk: &[u8; 32], lamport: u64) -> Vec<u8> {
    let mut m = Vec::with_capacity(DOMAIN.len() + 72);
    m.extend_from_slice(DOMAIN);
    m.extend_from_slice(chat_id);
    m.extend_from_slice(member_pk);
    m.extend_from_slice(&lamport.to_be_bytes());
    m
}

impl SignedWatermark {
    pub fn sign(sk: &SigningKey, chat_id: [u8; 32], lamport: u64) -> Self {
        let member_pk = sk.verifying_key().to_bytes();
        let sig = sk.sign(&signed_bytes(&chat_id, &member_pk, lamport));
        Self {
            chat_id,
            member_pk,
            lamport,
            sig: sig.to_bytes(),
        }
    }

    pub fn verify(&self) -> Result<()> {
        let vk = VerifyingKey::from_bytes(&self.member_pk)
            .map_err(|_| OmcpError::Sync("watermark key"))?;
        vk.verify(
            &signed_bytes(&self.chat_id, &self.member_pk, self.lamport),
            &Signature::from_bytes(&self.sig),
        )
        .map_err(|_| OmcpError::Sync("watermark signature"))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(WATERMARK_LEN);
        v.extend_from_slice(&self.chat_id);
        v.extend_from_slice(&self.member_pk);
        v.extend_from_slice(&self.lamport.to_be_bytes());
        v.extend_from_slice(&self.sig);
        v
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let w = Self {
            chat_id: take(&mut buf, 32)?.try_into().expect("32"),
            member_pk: take(&mut buf, 32)?.try_into().expect("32"),
            lamport: u64::from_be_bytes(take(&mut buf, 8)?.try_into().expect("8")),
            sig: take(&mut buf, 64)?.try_into().expect("64"),
        };
        if !buf.is_empty() {
            return Err(OmcpError::Sync("watermark trailing bytes"));
        }
        Ok(w)
    }
}

/// Catch-up request: "send me everything in this chat after my watermark".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchUpRequest {
    pub tag: super::tag::MailboxTag,
    pub after: SignedWatermark,
}

impl CatchUpRequest {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = self.tag.to_vec();
        v.extend_from_slice(&self.after.encode());
        v
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let tag = take(&mut buf, 8)?.try_into().expect("8");
        Ok(Self {
            tag,
            after: SignedWatermark::decode(buf)?,
        })
    }

    /// Filter `(lamport, payload)` history to what the requester lacks.
    pub fn select<'a, T>(&self, history: &'a [(u64, T)]) -> impl Iterator<Item = &'a (u64, T)> {
        let after = self.after.lamport;
        history.iter().filter(move |(l, _)| *l > after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_verify_roundtrip_and_tamper() {
        let sk = SigningKey::from_bytes(&[5u8; 32]);
        let w = SignedWatermark::sign(&sk, [1; 32], 42);
        w.verify().unwrap();
        let d = SignedWatermark::decode(&w.encode()).unwrap();
        assert_eq!(d, w);
        let mut bad = w.clone();
        bad.lamport = 43;
        assert!(bad.verify().is_err());
        let req = CatchUpRequest {
            tag: [3; 8],
            after: w,
        };
        let r2 = CatchUpRequest::decode(&req.encode()).unwrap();
        let hist = [(40u64, 'a'), (42, 'b'), (43, 'c'), (50, 'd')];
        let got: Vec<char> = r2.select(&hist).map(|x| x.1).collect();
        assert_eq!(got, vec!['c', 'd']);
    }

    #[test]
    fn lamport() {
        let mut l = Lamport::default();
        assert_eq!(l.tick(), 1);
        l.observe(10);
        assert_eq!(l.0, 11);
        l.observe(3);
        assert_eq!(l.0, 12);
    }
}
