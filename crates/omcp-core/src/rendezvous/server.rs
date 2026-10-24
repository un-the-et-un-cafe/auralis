//! In-memory reference rendezvous server + client transport (PROTOCOL §9.2).
//!
//! Endpoints (an HTTP binding would POST these bodies over TLS 1.3/QUIC):
//! - `POST /v1/enqueue` — [`EnqueueRequest`]: Enqueue token + one opaque envelope.
//!   Token label MUST equal the envelope's destination label.
//! - `POST /v1/fetch` — [`FetchRequest`]: Fetch token whose label MUST equal
//!   `blinded_label(signer_pk, epoch)` for the current or previous epoch, i.e.
//!   only the holder of the recipient's Ed25519 key can drain that mailbox.
//!
//! The server stores envelopes opaquely, never decrypts, enforces
//! expiry + max retention (168 h, §8.4), and rejects token replays by nonce.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use super::label::{blinded_label, epoch_for, BlindedLabel};
use super::token::{RendezvousToken, TokenOp};
use crate::error::{OmcpError, Result};
use crate::gateway::{OpaqueEnvelope, MAX_RETENTION_HOURS};

/// HTTP path for enqueue.
pub const PATH_ENQUEUE: &str = "/v1/enqueue";
/// HTTP path for fetch.
pub const PATH_FETCH: &str = "/v1/fetch";
/// Per-mailbox queue bound (DoS guard).
pub const MAX_MAILBOX_LEN: usize = 4096;

/// Body of `POST /v1/enqueue`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueRequest {
    pub token: RendezvousToken,
    pub envelope: OpaqueEnvelope,
}

/// Body of `POST /v1/fetch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchRequest {
    pub token: RendezvousToken,
}

impl EnqueueRequest {
    /// `token(166) || opaque_envelope`.
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut b = self.token.encode();
        b.extend_from_slice(&self.envelope.encode()?);
        Ok(b)
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        let n = super::token::TOKEN_LEN;
        if buf.len() < n {
            return Err(OmcpError::Truncated {
                need: n,
                have: buf.len(),
            });
        }
        Ok(Self {
            token: RendezvousToken::decode(&buf[..n])?,
            envelope: OpaqueEnvelope::decode(&buf[n..])?,
        })
    }
}

impl FetchRequest {
    pub fn encode(&self) -> Vec<u8> {
        self.token.encode()
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        Ok(Self {
            token: RendezvousToken::decode(buf)?,
        })
    }
}

/// Reference server state.
#[derive(Default)]
pub struct RendezvousServer {
    mailboxes: HashMap<BlindedLabel, VecDeque<(u64, OpaqueEnvelope)>>,
    seen_nonces: HashMap<[u8; 16], u64>,
}

impl RendezvousServer {
    pub fn new() -> Self {
        Self::default()
    }

    fn check_replay(&mut self, t: &RendezvousToken, now_ms: u64) -> Result<()> {
        self.seen_nonces.retain(|_, exp| *exp > now_ms);
        if self.seen_nonces.insert(t.nonce, t.expires_ms).is_some() {
            return Err(OmcpError::TokenRejected("replayed nonce"));
        }
        Ok(())
    }

    fn purge(&mut self, now_ms: u64) {
        let max = MAX_RETENTION_HOURS * 3_600_000;
        for q in self.mailboxes.values_mut() {
            q.retain(|(stored, e)| e.expiry_ms > now_ms && now_ms.saturating_sub(*stored) < max);
        }
        self.mailboxes.retain(|_, q| !q.is_empty());
    }

    /// Handle `POST /v1/enqueue`.
    pub fn enqueue(&mut self, req: EnqueueRequest, now_ms: u64) -> Result<()> {
        req.token.verify(TokenOp::Enqueue, now_ms)?;
        if req.token.label != req.envelope.dest_label {
            return Err(OmcpError::TokenRejected("label mismatch"));
        }
        if req.envelope.expiry_ms <= now_ms {
            return Err(OmcpError::Rendezvous("envelope expired"));
        }
        self.check_replay(&req.token, now_ms)?;
        self.purge(now_ms);
        let q = self.mailboxes.entry(req.envelope.dest_label).or_default();
        if q.len() >= MAX_MAILBOX_LEN {
            return Err(OmcpError::Rendezvous("mailbox full"));
        }
        q.push_back((now_ms, req.envelope));
        Ok(())
    }

    /// Handle `POST /v1/fetch`: drains the mailbox for the token's label.
    pub fn fetch(&mut self, req: FetchRequest, now_ms: u64) -> Result<Vec<OpaqueEnvelope>> {
        let t = &req.token;
        t.verify(TokenOp::Fetch, now_ms)?;
        let e = epoch_for(now_ms);
        let ok = [e, e.saturating_sub(1)]
            .iter()
            .any(|ep| blinded_label(&t.signer_pk, *ep) == t.label);
        if !ok {
            return Err(OmcpError::TokenRejected("signer does not own label"));
        }
        self.check_replay(t, now_ms)?;
        self.purge(now_ms);
        Ok(self
            .mailboxes
            .remove(&t.label)
            .map(|q| q.into_iter().map(|(_, e)| e).collect())
            .unwrap_or_default())
    }

    /// Number of stored envelopes (all mailboxes).
    pub fn stored_len(&self) -> usize {
        self.mailboxes.values().map(VecDeque::len).sum()
    }
}

/// Transport used by gateways to reach a rendezvous (real impl: HTTPS/QUIC).
pub trait RendezvousTransport {
    fn enqueue(&mut self, req: EnqueueRequest, now_ms: u64) -> Result<()>;
    fn fetch(&mut self, req: FetchRequest, now_ms: u64) -> Result<Vec<OpaqueEnvelope>>;
}

/// Shared handle to an in-memory server; round-trips requests through their
/// byte encodings to exercise the wire format.
#[derive(Clone, Default)]
pub struct InMemoryRendezvous(pub Arc<Mutex<RendezvousServer>>);

impl InMemoryRendezvous {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, RendezvousServer>> {
        self.0
            .lock()
            .map_err(|_| OmcpError::Rendezvous("server poisoned"))
    }

    pub fn stored_len(&self) -> usize {
        self.lock().map(|s| s.stored_len()).unwrap_or(0)
    }
}

impl RendezvousTransport for InMemoryRendezvous {
    fn enqueue(&mut self, req: EnqueueRequest, now_ms: u64) -> Result<()> {
        let req = EnqueueRequest::decode(&req.encode()?)?;
        self.lock()?.enqueue(req, now_ms)
    }

    fn fetch(&mut self, req: FetchRequest, now_ms: u64) -> Result<Vec<OpaqueEnvelope>> {
        let req = FetchRequest::decode(&req.encode())?;
        self.lock()?.fetch(req, now_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rendezvous::RendezvousIdentity;
    use crate::suite::SUITE_OMCP_2026_A;

    fn env(label: BlindedLabel, exp: u64) -> OpaqueEnvelope {
        OpaqueEnvelope {
            dest_label: label,
            suite_id: SUITE_OMCP_2026_A,
            expiry_ms: exp,
            ciphertext: vec![0xAA; 40],
        }
    }

    #[test]
    fn only_label_owner_can_fetch_and_replays_rejected() {
        let now = 10 * crate::rendezvous::LABEL_EPOCH_MS + 5;
        let bob = RendezvousIdentity::generate().unwrap();
        let eve = RendezvousIdentity::generate().unwrap();
        let gw = RendezvousIdentity::generate().unwrap();
        let label = blinded_label(&bob.verifying_key(), epoch_for(now));
        let mut s = RendezvousServer::new();
        let t = RendezvousToken::issue(&gw, TokenOp::Enqueue, label, now, 60_000).unwrap();
        s.enqueue(
            EnqueueRequest {
                token: t.clone(),
                envelope: env(label, now + 1000),
            },
            now,
        )
        .unwrap();
        // replay of same token
        assert!(s
            .enqueue(
                EnqueueRequest {
                    token: t,
                    envelope: env(label, now + 1000)
                },
                now
            )
            .is_err());
        // Eve signs a fetch for Bob's label: rejected
        let te = RendezvousToken::issue(&eve, TokenOp::Fetch, label, now, 60_000).unwrap();
        assert!(s.fetch(FetchRequest { token: te }, now).is_err());
        // Enqueue token can't be used to fetch
        let tb = RendezvousToken::issue(&bob, TokenOp::Fetch, label, now, 60_000).unwrap();
        let got = s.fetch(FetchRequest { token: tb.clone() }, now).unwrap();
        assert_eq!(got.len(), 1);
        assert!(s.fetch(FetchRequest { token: tb }, now).is_err()); // replay
        assert_eq!(s.stored_len(), 0);
    }

    #[test]
    fn expired_envelopes_purged() {
        let now = 1_000_000;
        let bob = RendezvousIdentity::generate().unwrap();
        let gw = RendezvousIdentity::generate().unwrap();
        let label = blinded_label(&bob.verifying_key(), epoch_for(now));
        let mut s = RendezvousServer::new();
        let t = RendezvousToken::issue(&gw, TokenOp::Enqueue, label, now, 60_000).unwrap();
        s.enqueue(
            EnqueueRequest {
                token: t,
                envelope: env(label, now + 10),
            },
            now,
        )
        .unwrap();
        let tb = RendezvousToken::issue(&bob, TokenOp::Fetch, label, now + 20, 60_000).unwrap();
        assert!(s
            .fetch(FetchRequest { token: tb }, now + 20)
            .unwrap()
            .is_empty());
    }
}
