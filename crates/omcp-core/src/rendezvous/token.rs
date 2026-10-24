//! Short-lived signed rendezvous tokens (PROTOCOL §9.2).
//!
//! Wire layout (big-endian, 166 bytes):
//! ```text
//! magic "OMRT"(4) | version u8 = 1 | op u8 | label(32) | issued_ms u64 |
//! expires_ms u64 | nonce(16) | signer_pk(32) | sig(64)
//! ```
//! `sig = Ed25519(signer, "OMCP-RV-Token-v1" || bytes[0..102])`.
//! Lifetime is capped at [`MAX_TOKEN_LIFETIME_MS`] (5 min); servers reject
//! replays by nonce.

use super::identity::{random_bytes, verify, RendezvousIdentity};
use super::label::BlindedLabel;
use crate::error::{OmcpError, Result};

/// Maximum token lifetime (5 minutes).
pub const MAX_TOKEN_LIFETIME_MS: u64 = 5 * 60 * 1000;
/// Allowed clock skew for `issued_ms` in the future.
pub const MAX_CLOCK_SKEW_MS: u64 = 30 * 1000;
/// Encoded token length.
pub const TOKEN_LEN: usize = 4 + 1 + 1 + 32 + 8 + 8 + 16 + 32 + 64;

const MAGIC: [u8; 4] = *b"OMRT";
const DOMAIN: &[u8] = b"OMCP-RV-Token-v1";
const SIGNED_LEN: usize = TOKEN_LEN - 64;

/// Which endpoint the token authorises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TokenOp {
    /// `POST /v1/enqueue`
    Enqueue = 1,
    /// `POST /v1/fetch`
    Fetch = 2,
}

/// A signed, short-lived token bound to one op and one blinded label.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RendezvousToken {
    pub op: TokenOp,
    pub label: BlindedLabel,
    pub issued_ms: u64,
    pub expires_ms: u64,
    pub nonce: [u8; 16],
    pub signer_pk: [u8; 32],
    pub sig: [u8; 64],
}

impl RendezvousToken {
    /// Issue a token signed by `signer`; `lifetime_ms` is clamped to the max.
    pub fn issue(
        signer: &RendezvousIdentity,
        op: TokenOp,
        label: BlindedLabel,
        now_ms: u64,
        lifetime_ms: u64,
    ) -> Result<Self> {
        let mut t = Self {
            op,
            label,
            issued_ms: now_ms,
            expires_ms: now_ms + lifetime_ms.min(MAX_TOKEN_LIFETIME_MS),
            nonce: random_bytes::<16>()?,
            signer_pk: signer.verifying_key(),
            sig: [0u8; 64],
        };
        let mut msg = DOMAIN.to_vec();
        msg.extend_from_slice(&t.signed_bytes());
        t.sig = signer.sign(&msg);
        Ok(t)
    }

    fn signed_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(SIGNED_LEN);
        b.extend_from_slice(&MAGIC);
        b.push(1);
        b.push(self.op as u8);
        b.extend_from_slice(&self.label);
        b.extend_from_slice(&self.issued_ms.to_be_bytes());
        b.extend_from_slice(&self.expires_ms.to_be_bytes());
        b.extend_from_slice(&self.nonce);
        b.extend_from_slice(&self.signer_pk);
        b
    }

    /// Encode to the 166-byte wire form.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = self.signed_bytes();
        b.extend_from_slice(&self.sig);
        b
    }

    /// Decode (does not verify; call [`Self::verify`]).
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() != TOKEN_LEN {
            return Err(OmcpError::TokenRejected("bad length"));
        }
        if buf[..4] != MAGIC || buf[4] != 1 {
            return Err(OmcpError::TokenRejected("bad magic/version"));
        }
        let op = match buf[5] {
            1 => TokenOp::Enqueue,
            2 => TokenOp::Fetch,
            _ => return Err(OmcpError::TokenRejected("bad op")),
        };
        let a = |r: core::ops::Range<usize>| buf[r].to_vec();
        Ok(Self {
            op,
            label: a(6..38).try_into().unwrap(),
            issued_ms: u64::from_be_bytes(a(38..46).try_into().unwrap()),
            expires_ms: u64::from_be_bytes(a(46..54).try_into().unwrap()),
            nonce: a(54..70).try_into().unwrap(),
            signer_pk: a(70..102).try_into().unwrap(),
            sig: a(102..166).try_into().unwrap(),
        })
    }

    /// Verify signature, op, and freshness at `now_ms`.
    pub fn verify(&self, expected_op: TokenOp, now_ms: u64) -> Result<()> {
        if self.op != expected_op {
            return Err(OmcpError::TokenRejected("wrong op"));
        }
        if self.expires_ms <= self.issued_ms
            || self.expires_ms - self.issued_ms > MAX_TOKEN_LIFETIME_MS
        {
            return Err(OmcpError::TokenRejected("lifetime too long"));
        }
        if self.issued_ms > now_ms + MAX_CLOCK_SKEW_MS {
            return Err(OmcpError::TokenRejected("issued in future"));
        }
        if now_ms >= self.expires_ms {
            return Err(OmcpError::TokenRejected("expired"));
        }
        let mut msg = DOMAIN.to_vec();
        msg.extend_from_slice(&self.signed_bytes());
        verify(&self.signer_pk, &msg, &self.sig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_roundtrip_verify_expiry_tamper() {
        let id = RendezvousIdentity::generate().unwrap();
        let t = RendezvousToken::issue(&id, TokenOp::Fetch, [3; 32], 1_000, 60_000).unwrap();
        let enc = t.encode();
        assert_eq!(enc.len(), TOKEN_LEN);
        let d = RendezvousToken::decode(&enc).unwrap();
        assert_eq!(d, t);
        d.verify(TokenOp::Fetch, 2_000).unwrap();
        assert!(d.verify(TokenOp::Enqueue, 2_000).is_err());
        assert!(d.verify(TokenOp::Fetch, 61_000).is_err());
        let mut bad = enc.clone();
        bad[10] ^= 1;
        assert!(RendezvousToken::decode(&bad)
            .unwrap()
            .verify(TokenOp::Fetch, 2_000)
            .is_err());
        let long = RendezvousToken::issue(&id, TokenOp::Fetch, [3; 32], 0, u64::MAX / 2).unwrap();
        assert_eq!(long.expires_ms, MAX_TOKEN_LIFETIME_MS);
    }
}
