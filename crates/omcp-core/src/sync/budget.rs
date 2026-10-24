//! Relay budgets, eviction, expiry, metered-network rules (PROTOCOL §22.4)
//! and gateway rendezvous offload (§22.5).
//!
//! These budgets are **per-device mesh relay** budgets. They are separate
//! from, and in addition to, the gateway's Internet byte cap
//! ([`crate::gateway::policy::DEFAULT_DAILY_BYTE_CAP`], 500 MB/day).

use std::collections::HashMap;

use super::tag::{tag_rendezvous_label, MailboxTag};
use crate::error::{OmcpError, Result};
use crate::gateway::policy::{NetworkKind, DAY_MS};

pub const MB: u64 = 1_000_000;
pub const DEFAULT_EXPIRY_MS: u64 = 72 * 3_600_000;
pub const MAX_EXPIRY_MS: u64 = 7 * DAY_MS;

/// Storage / forwarding limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelayBudget {
    pub storage_bytes: u64,
    /// `None` = unlimited.
    pub daily_forward_bytes: Option<u64>,
}

impl RelayBudget {
    /// Defaults: 50 MB / 20 MB-per-day on battery or cellular; 500 MB /
    /// unlimited on Wi-Fi while charging.
    pub fn for_conditions(net: NetworkKind, charging: bool) -> Self {
        let generous = charging && matches!(net, NetworkKind::Wifi { metered: false });
        if generous {
            Self {
                storage_bytes: 500 * MB,
                daily_forward_bytes: None,
            }
        } else {
            Self {
                storage_bytes: 50 * MB,
                daily_forward_bytes: Some(20 * MB),
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEnvelope {
    pub id: [u8; 16],
    pub size: u64,
    pub priority: bool,
    pub expires_at: u64,
    /// Hops already travelled.
    pub hop_count: u8,
}

impl StoredEnvelope {
    /// Eviction key: smallest is evicted first — non-priority before
    /// priority, then soonest expiry, then most hops travelled.
    pub fn evict_key(&self) -> (bool, u64, std::cmp::Reverse<u8>) {
        (
            self.priority,
            self.expires_at,
            std::cmp::Reverse(self.hop_count),
        )
    }
}

/// Clamp an expiry: default 72 h, max 7 d. Burn envelopes keep their own
/// (earlier) `expires_at`.
pub fn clamp_expiry(now_ms: u64, requested: Option<u64>, burn: bool) -> u64 {
    let max = now_ms + MAX_EXPIRY_MS;
    match requested {
        // Burn envelopes always carry their own expiry; others may too.
        Some(t) => t.min(max),
        None if burn => now_ms,
        None => now_ms + DEFAULT_EXPIRY_MS,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    Stored {
        evicted: Vec<[u8; 16]>,
    },
    /// Would have been the first victim itself, or larger than the budget.
    Rejected,
}

/// Store-and-forward buffer that never exceeds its budget.
#[derive(Debug)]
pub struct RelayStore {
    budget: RelayBudget,
    items: HashMap<[u8; 16], StoredEnvelope>,
    used: u64,
    day: u64,
    forwarded_today: u64,
}

impl RelayStore {
    pub fn new(budget: RelayBudget) -> Self {
        Self {
            budget,
            items: HashMap::new(),
            used: 0,
            day: 0,
            forwarded_today: 0,
        }
    }

    pub fn used(&self) -> u64 {
        self.used
    }

    pub fn forwarded_today(&self) -> u64 {
        self.forwarded_today
    }

    pub fn budget(&self) -> RelayBudget {
        self.budget
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Change budget (e.g. unplugged); evicts down to the new cap.
    pub fn set_budget(&mut self, b: RelayBudget) -> Vec<[u8; 16]> {
        self.budget = b;
        let mut ev = Vec::new();
        while self.used > b.storage_bytes {
            ev.push(self.evict_one().expect("used > 0 implies items"));
        }
        ev
    }

    fn evict_one(&mut self) -> Option<[u8; 16]> {
        let id = self.items.values().min_by_key(|e| e.evict_key())?.id;
        let e = self.items.remove(&id)?;
        self.used -= e.size;
        Some(id)
    }

    pub fn purge_expired(&mut self, now_ms: u64) -> usize {
        let before = self.items.len();
        self.items.retain(|_, e| e.expires_at > now_ms);
        self.used = self.items.values().map(|e| e.size).sum();
        before - self.items.len()
    }

    pub fn insert(&mut self, env: StoredEnvelope, now_ms: u64) -> InsertOutcome {
        self.purge_expired(now_ms);
        if env.size > self.budget.storage_bytes
            || env.expires_at <= now_ms
            || self.items.contains_key(&env.id)
        {
            return InsertOutcome::Rejected;
        }
        // Would the newcomer be evicted before everything needed to make room?
        if self.used + env.size > self.budget.storage_bytes {
            let mut sorted: Vec<&StoredEnvelope> = self.items.values().collect();
            sorted.sort_by_key(|e| e.evict_key());
            let mut free = self.budget.storage_bytes - self.used;
            for e in sorted {
                if free >= env.size {
                    break;
                }
                if env.evict_key() <= e.evict_key() {
                    return InsertOutcome::Rejected;
                }
                free += e.size;
            }
        }
        let mut evicted = Vec::new();
        while self.used + env.size > self.budget.storage_bytes {
            evicted.push(self.evict_one().expect("room computed above"));
        }
        self.used += env.size;
        self.items.insert(env.id, env);
        InsertOutcome::Stored { evicted }
    }

    pub fn take(&mut self, id: &[u8; 16]) -> Option<StoredEnvelope> {
        let e = self.items.remove(id)?;
        self.used -= e.size;
        Some(e)
    }

    /// Account forwarding `bytes`; refuses when the daily cap would be exceeded.
    pub fn try_forward(&mut self, bytes: u64, now_ms: u64) -> Result<()> {
        let day = now_ms / DAY_MS;
        if day != self.day {
            self.day = day;
            self.forwarded_today = 0;
        }
        if let Some(cap) = self.budget.daily_forward_bytes {
            if self.forwarded_today + bytes > cap {
                return Err(OmcpError::Sync("daily forward budget exhausted"));
            }
        }
        self.forwarded_today += bytes;
        Ok(())
    }
}

/// How much a device may sync over the Internet on the current link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncScope {
    /// Unmetered: everything (subject to budgets).
    Full,
    /// Cellular with mobile data allowed: only pending outbound and the
    /// device's own chats.
    OwnOnly,
    None,
}

pub fn internet_sync_scope(net: NetworkKind, allow_mobile_data: bool) -> SyncScope {
    match net {
        NetworkKind::Offline => SyncScope::None,
        NetworkKind::Wifi { metered: false } => SyncScope::Full,
        // Cellular (metered or not) and metered Wi-Fi count as mobile data.
        _ if allow_mobile_data => SyncScope::OwnOnly,
        _ => SyncScope::None,
    }
}

/// May this envelope be synced over the Internet under `scope`?
pub fn may_sync(scope: SyncScope, pending_outbound: bool, is_member: bool) -> bool {
    match scope {
        SyncScope::Full => true,
        SyncScope::OwnOnly => pending_outbound || is_member,
        SyncScope::None => false,
    }
}

/// Where to send an envelope (§22.5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    /// One upload to the gateway rendezvous mailbox for this label.
    Rendezvous { label: [u8; 32] },
    /// BLE gossip fallback.
    BleGossip,
}

pub fn choose_route(gateway_reachable: bool, tag: &MailboxTag, tag_epoch: u64) -> Route {
    if gateway_reachable {
        Route::Rendezvous {
            label: tag_rendezvous_label(tag, tag_epoch),
        }
    } else {
        Route::BleGossip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(i: u8, size: u64, prio: bool, exp: u64, hops: u8) -> StoredEnvelope {
        StoredEnvelope {
            id: [i; 16],
            size,
            priority: prio,
            expires_at: exp,
            hop_count: hops,
        }
    }

    #[test]
    fn defaults() {
        let b = RelayBudget::for_conditions(NetworkKind::Cellular { metered: true }, true);
        assert_eq!(b.storage_bytes, 50 * MB);
        assert_eq!(b.daily_forward_bytes, Some(20 * MB));
        let b = RelayBudget::for_conditions(NetworkKind::Wifi { metered: false }, true);
        assert_eq!(b.storage_bytes, 500 * MB);
        assert_eq!(b.daily_forward_bytes, None);
        let b = RelayBudget::for_conditions(NetworkKind::Wifi { metered: false }, false);
        assert_eq!(b.storage_bytes, 50 * MB);
    }

    #[test]
    fn eviction_order() {
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 300,
            daily_forward_bytes: Some(1000),
        });
        assert!(matches!(
            s.insert(env(1, 100, true, 50, 0), 0),
            InsertOutcome::Stored { .. }
        ));
        assert!(matches!(
            s.insert(env(2, 100, false, 90, 1), 0),
            InsertOutcome::Stored { .. }
        ));
        assert!(matches!(
            s.insert(env(3, 100, false, 90, 5), 0),
            InsertOutcome::Stored { .. }
        ));
        // Non-priority, same expiry: more hops goes first.
        assert_eq!(
            s.insert(env(4, 100, false, 95, 0), 0),
            InsertOutcome::Stored {
                evicted: vec![[3; 16]]
            }
        );
        // A newcomer that would itself be first victim is rejected.
        assert_eq!(
            s.insert(env(5, 100, false, 10, 0), 0),
            InsertOutcome::Rejected
        );
        assert_eq!(s.used(), 300);
        assert_eq!(
            s.insert(env(6, 1000, true, 99, 0), 0),
            InsertOutcome::Rejected
        );
        assert_eq!(s.purge_expired(60), 1);
        assert_eq!(s.used(), 200);
        assert_eq!(
            s.set_budget(RelayBudget {
                storage_bytes: 100,
                daily_forward_bytes: None
            }),
            vec![[2; 16]]
        );
    }

    #[test]
    fn forward_cap_and_expiry() {
        let mut s = RelayStore::new(RelayBudget {
            storage_bytes: 1,
            daily_forward_bytes: Some(100),
        });
        s.try_forward(60, 0).unwrap();
        assert!(s.try_forward(60, 1).is_err());
        s.try_forward(60, DAY_MS).unwrap();
        assert_eq!(clamp_expiry(0, None, false), DEFAULT_EXPIRY_MS);
        assert_eq!(clamp_expiry(0, Some(u64::MAX), false), MAX_EXPIRY_MS);
        assert_eq!(clamp_expiry(0, Some(5), true), 5);
    }

    #[test]
    fn metered_rules_and_offload() {
        use NetworkKind::*;
        assert_eq!(
            internet_sync_scope(Wifi { metered: false }, false),
            SyncScope::Full
        );
        assert_eq!(
            internet_sync_scope(Cellular { metered: true }, false),
            SyncScope::None
        );
        assert_eq!(
            internet_sync_scope(Cellular { metered: false }, true),
            SyncScope::OwnOnly
        );
        assert_eq!(
            internet_sync_scope(Wifi { metered: true }, true),
            SyncScope::OwnOnly
        );
        assert_eq!(internet_sync_scope(Offline, true), SyncScope::None);
        assert!(may_sync(SyncScope::OwnOnly, true, false));
        assert!(!may_sync(SyncScope::OwnOnly, false, false));
        assert!(matches!(
            choose_route(true, &[1; 8], 3),
            Route::Rendezvous { .. }
        ));
        assert_eq!(choose_route(false, &[1; 8], 3), Route::BleGossip);
    }
}
