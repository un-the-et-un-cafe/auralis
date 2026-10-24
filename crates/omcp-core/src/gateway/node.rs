//! Gateway state machine: opaque queue, data cap, outage/drain (PROTOCOL §9.1, §9.3).
//!
//! The gateway owns only its *own* Ed25519 key (to sign enqueue tokens) and
//! never any user content key. Local peers that opt in to Internet fetch hand
//! the gateway pre-signed, single-use, short-lived Fetch tokens; the gateway
//! cannot mint those itself.

use std::collections::VecDeque;

use super::control::OFFER_FLAG_RENDEZVOUS;
use super::control::{BusyReason, GatewayControl, GatewayOffering, OFFER_FLAG_METERED};
use super::policy::{GatewayPolicy, NetworkKind, DAY_MS};
use super::store::StoreAndForward;
use super::OpaqueEnvelope;
use crate::error::{OmcpError, Result};
use crate::rendezvous::{
    BlindedLabel, EnqueueRequest, FetchRequest, RendezvousIdentity, RendezvousToken,
    RendezvousTransport, TokenOp,
};

/// Result of [`Gateway::submit_outbound`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmitOutcome {
    /// Queued; will be bridged on the next eligible pump.
    Queued,
    /// Refused; send this control message back to the submitting peer.
    Busy(GatewayControl),
}

/// Result of one [`Gateway::pump`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PumpReport {
    /// Whether the gateway was bridging this round.
    pub bridged: bool,
    pub sent: usize,
    pub fetched: usize,
    /// UI should warn: bridging over a metered connection.
    pub metered_warning: bool,
    /// Busy signal to broadcast to the mesh, if any.
    pub busy: Option<GatewayControl>,
}

pub struct Gateway {
    identity: RendezvousIdentity,
    policy: GatewayPolicy,
    network: NetworkKind,
    outbound: VecDeque<OpaqueEnvelope>,
    fetch_tokens: Vec<RendezvousToken>,
    store: StoreAndForward,
    day: u64,
    bytes_today: u64,
}

impl Gateway {
    pub fn new(
        identity: RendezvousIdentity,
        policy: GatewayPolicy,
        store: StoreAndForward,
    ) -> Self {
        Self {
            identity,
            policy,
            network: NetworkKind::Offline,
            outbound: VecDeque::new(),
            fetch_tokens: Vec::new(),
            store,
            day: 0,
            bytes_today: 0,
        }
    }

    /// Gateway PeerID used in offerings / pinning (its Ed25519 public key).
    pub fn peer_id(&self) -> [u8; 32] {
        self.identity.verifying_key()
    }

    pub fn policy(&self) -> &GatewayPolicy {
        &self.policy
    }

    pub fn set_network(&mut self, net: NetworkKind) {
        self.network = net;
    }

    pub fn queue_len(&self) -> usize {
        self.outbound.len()
    }

    pub fn bytes_today(&self) -> u64 {
        self.bytes_today
    }

    /// Read-only view of queued opaque envelopes (for audits / tests).
    pub fn outbound_snapshot(&self) -> impl Iterator<Item = &OpaqueEnvelope> {
        self.outbound.iter()
    }

    fn roll_day(&mut self, now_ms: u64) {
        let d = now_ms / DAY_MS;
        if d != self.day {
            self.day = d;
            self.bytes_today = 0;
        }
    }

    fn remaining(&self) -> u64 {
        self.policy.daily_byte_cap.saturating_sub(self.bytes_today)
    }

    fn retry_after(&self, now_ms: u64) -> u64 {
        (self.day + 1) * DAY_MS - now_ms
    }

    /// Current `GATEWAY_OFFERING` advertisement.
    pub fn offering(&mut self, now_ms: u64) -> GatewayOffering {
        self.roll_day(now_ms);
        let d = self.policy.evaluate(self.network);
        let mut flags = OFFER_FLAG_RENDEZVOUS;
        if d.metered_warning {
            flags |= OFFER_FLAG_METERED;
        }
        GatewayOffering {
            gateway_peer_id: self.peer_id(),
            remaining_bytes_today: if d.bridge { self.remaining() } else { 0 },
            queue_free: u16::try_from(
                self.policy
                    .max_queue_len
                    .saturating_sub(self.outbound.len()),
            )
            .unwrap_or(u16::MAX),
            flags,
        }
    }

    /// Accept an opaque envelope from the local mesh for Internet delivery.
    /// Accepted even while offline (queued for drain on return, §9.3).
    pub fn submit_outbound(&mut self, env: OpaqueEnvelope, now_ms: u64) -> Result<SubmitOutcome> {
        self.roll_day(now_ms);
        if env.expiry_ms <= now_ms {
            return Err(OmcpError::Gateway("envelope expired"));
        }
        env.encode()?; // size check (§21.4)
        if self.outbound.len() >= self.policy.max_queue_len {
            return Ok(SubmitOutcome::Busy(GatewayControl::Busy {
                reason: BusyReason::QueueFull,
                retry_after_ms: 60_000,
            }));
        }
        if self.remaining() < env.wire_len() as u64 {
            return Ok(SubmitOutcome::Busy(GatewayControl::Busy {
                reason: BusyReason::DailyCapReached,
                retry_after_ms: self.retry_after(now_ms),
            }));
        }
        self.outbound.push_back(env);
        Ok(SubmitOutcome::Queued)
    }

    /// A local peer opts in to Internet fetch by handing a pre-signed token.
    pub fn register_fetch(&mut self, token: RendezvousToken) -> Result<()> {
        if token.op != TokenOp::Fetch {
            return Err(OmcpError::TokenRejected("not a fetch token"));
        }
        self.fetch_tokens.push(token);
        Ok(())
    }

    /// One bridging round. Offline / disallowed network ⇒ stop bridging,
    /// keep queue. Online ⇒ drain outbound queue (within cap), then run
    /// registered fetches into store-and-forward.
    pub fn pump<T: RendezvousTransport>(&mut self, rv: &mut T, now_ms: u64) -> Result<PumpReport> {
        self.roll_day(now_ms);
        self.store.purge(now_ms);
        self.outbound.retain(|e| e.expiry_ms > now_ms);
        let d = self.policy.evaluate(self.network);
        let mut rep = PumpReport {
            bridged: d.bridge,
            metered_warning: d.bridge && d.metered_warning,
            ..Default::default()
        };
        if !d.bridge {
            let reason = if self.network == NetworkKind::Offline {
                BusyReason::Offline
            } else {
                BusyReason::PolicyNetwork
            };
            rep.busy = Some(GatewayControl::Busy {
                reason,
                retry_after_ms: 60_000,
            });
            return Ok(rep);
        }

        while let Some(env) = self.outbound.front() {
            let cost = env.wire_len() as u64;
            if cost > self.remaining() {
                rep.busy = Some(GatewayControl::Busy {
                    reason: BusyReason::DailyCapReached,
                    retry_after_ms: self.retry_after(now_ms),
                });
                break;
            }
            let token = RendezvousToken::issue(
                &self.identity,
                TokenOp::Enqueue,
                env.dest_label,
                now_ms,
                60_000,
            )?;
            let req = EnqueueRequest {
                token,
                envelope: env.clone(),
            };
            match rv.enqueue(req, now_ms) {
                Ok(()) => {
                    self.bytes_today += cost;
                    self.outbound.pop_front();
                    rep.sent += 1;
                }
                // Transport failure mid-drain = outage: keep envelope, stop.
                Err(OmcpError::Rendezvous("unreachable")) => {
                    rep.bridged = false;
                    return Ok(rep);
                }
                // Permanently rejected by server: drop it.
                Err(_) => {
                    self.outbound.pop_front();
                }
            }
        }

        if rep.busy.is_none() {
            let tokens = std::mem::take(&mut self.fetch_tokens);
            for t in tokens {
                if t.expires_ms <= now_ms {
                    continue;
                }
                match rv.fetch(FetchRequest { token: t.clone() }, now_ms) {
                    Ok(envs) => {
                        for e in envs {
                            self.bytes_today += e.wire_len() as u64;
                            self.store.retain(e, now_ms);
                            rep.fetched += 1;
                        }
                    }
                    Err(OmcpError::Rendezvous("unreachable")) => {
                        self.fetch_tokens.push(t);
                        rep.bridged = false;
                    }
                    Err(_) => {}
                }
            }
        }
        Ok(rep)
    }

    /// Local peer (re)appears on the mesh: hand over its pending envelopes.
    pub fn deliver_local(&mut self, label: &BlindedLabel, now_ms: u64) -> Vec<OpaqueEnvelope> {
        self.store.take_for(label, now_ms)
    }

    pub fn pending_local(&self, label: &BlindedLabel) -> usize {
        self.store.have(label)
    }
}
