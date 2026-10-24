//! Store-and-forward retention (PROTOCOL §8.4): default 72 h, max 168 h.

use super::OpaqueEnvelope;
use crate::rendezvous::BlindedLabel;

pub const DEFAULT_RETENTION_HOURS: u64 = 72;
pub const MAX_RETENTION_HOURS: u64 = 168;
const HOUR_MS: u64 = 3_600_000;

/// Opaque envelopes held for local peers until they reappear.
#[derive(Debug, Clone)]
pub struct StoreAndForward {
    retention_hours: u64,
    entries: Vec<(u64, OpaqueEnvelope)>,
}

impl Default for StoreAndForward {
    fn default() -> Self {
        Self::new(DEFAULT_RETENTION_HOURS)
    }
}

impl StoreAndForward {
    /// `retention_hours` is clamped to `1..=168`.
    pub fn new(retention_hours: u64) -> Self {
        Self {
            retention_hours: retention_hours.clamp(1, MAX_RETENTION_HOURS),
            entries: Vec::new(),
        }
    }

    pub fn retention_hours(&self) -> u64 {
        self.retention_hours
    }

    pub fn retain(&mut self, env: OpaqueEnvelope, now_ms: u64) {
        if env.expiry_ms > now_ms {
            self.entries.push((now_ms, env));
        }
    }

    /// Drop envelopes past their expiry or the retention window.
    pub fn purge(&mut self, now_ms: u64) {
        let window = self.retention_hours * HOUR_MS;
        self.entries
            .retain(|(t, e)| e.expiry_ms > now_ms && now_ms.saturating_sub(*t) < window);
    }

    /// Number pending for `label` (what a `HAVE` offer would advertise).
    pub fn have(&self, label: &BlindedLabel) -> usize {
        self.entries
            .iter()
            .filter(|(_, e)| &e.dest_label == label)
            .count()
    }

    /// Hand over everything pending for `label` (contact reappeared).
    pub fn take_for(&mut self, label: &BlindedLabel, now_ms: u64) -> Vec<OpaqueEnvelope> {
        self.purge(now_ms);
        let (mine, rest) = std::mem::take(&mut self.entries)
            .into_iter()
            .partition(|(_, e)| &e.dest_label == label);
        self.entries = rest;
        mine.into_iter().map(|(_, e)| e).collect::<Vec<_>>()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_default_clamp_and_purge() {
        assert_eq!(StoreAndForward::default().retention_hours(), 72);
        assert_eq!(StoreAndForward::new(1000).retention_hours(), 168);
        let mut s = StoreAndForward::new(1);
        let e = |l: u8| OpaqueEnvelope {
            dest_label: [l; 32],
            suite_id: 1,
            expiry_ms: u64::MAX,
            ciphertext: vec![],
        };
        s.retain(e(1), 0);
        s.retain(e(2), 0);
        assert_eq!(s.have(&[1; 32]), 1);
        assert_eq!(s.take_for(&[1; 32], 10).len(), 1);
        s.purge(HOUR_MS);
        assert!(s.is_empty());
    }
}
