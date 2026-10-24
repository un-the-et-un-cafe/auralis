//! Hello cadence and digest advertisement (PROTOCOL §22.7(2)).
//!
//! - Hello: 1 s after a neighbourhood change, doubling to 60 s while stable.
//! - Digests: deltas only on change, plus a full refresh every 5 min.

use super::cuckoo::{CuckooFilter, DigestDelta};

pub const HELLO_MIN_MS: u64 = 1_000;
pub const HELLO_MAX_MS: u64 = 60_000;
pub const DIGEST_REFRESH_MS: u64 = 300_000;
/// Hello on the wire: 4 B header + 8 B digest id.
pub const HELLO_BYTES: u64 = 12;

#[derive(Debug, Clone)]
pub struct HelloTimer {
    interval_ms: u64,
    next_ms: u64,
}

impl HelloTimer {
    pub fn new(now_ms: u64) -> Self {
        Self {
            interval_ms: HELLO_MIN_MS,
            next_ms: now_ms,
        }
    }

    /// A neighbour appeared or left: restart at 1 s.
    pub fn on_neighbourhood_change(&mut self, now_ms: u64) {
        self.interval_ms = HELLO_MIN_MS;
        self.next_ms = now_ms;
    }

    /// When the next hello is due.
    pub fn next_due(&self) -> u64 {
        self.next_ms
    }

    /// Returns `true` (and schedules the next one) if a hello is due.
    pub fn poll(&mut self, now_ms: u64) -> bool {
        if now_ms < self.next_ms {
            return false;
        }
        self.next_ms = now_ms + self.interval_ms;
        self.interval_ms = (self.interval_ms * 2).min(HELLO_MAX_MS);
        true
    }

    /// Count hellos in `[start, end)` given sorted neighbourhood-change times.
    pub fn count_hellos(start_ms: u64, end_ms: u64, changes: &[u64]) -> u64 {
        let mut t = HelloTimer::new(start_ms);
        let mut n = 0;
        let mut ci = 0;
        loop {
            let next_change = changes.get(ci).copied().unwrap_or(u64::MAX);
            let due = t.next_due();
            if next_change <= due && next_change < end_ms {
                t.on_neighbourhood_change(next_change);
                ci += 1;
                continue;
            }
            if due >= end_ms {
                return n;
            }
            t.poll(due);
            n += 1;
        }
    }
}

/// What to broadcast for the digest at a tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestMsg {
    Full(Vec<u8>),
    Delta(DigestDelta),
}

impl DigestMsg {
    pub fn wire_len(&self) -> usize {
        match self {
            Self::Full(b) => b.len(),
            Self::Delta(d) => d.encoded_len(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DigestAdvertiser {
    sent: CuckooFilter,
    last_full_ms: u64,
}

impl DigestAdvertiser {
    /// Starts by (conceptually) having sent `initial` in full at `now_ms`.
    pub fn new(initial: CuckooFilter, now_ms: u64) -> Self {
        Self {
            sent: initial,
            last_full_ms: now_ms,
        }
    }

    /// Given the node's current digest, what (if anything) to broadcast.
    pub fn tick(&mut self, current: &CuckooFilter, now_ms: u64) -> Option<DigestMsg> {
        if now_ms >= self.last_full_ms + DIGEST_REFRESH_MS {
            self.last_full_ms = now_ms;
            self.sent = current.clone();
            return Some(DigestMsg::Full(current.encode()));
        }
        if current.encode() == self.sent.encode() {
            return None;
        }
        let msg = match self.sent.delta_to(current) {
            Some(d) => DigestMsg::Delta(d),
            None => {
                self.last_full_ms = now_ms;
                DigestMsg::Full(current.encode())
            }
        };
        self.sent = current.clone();
        Some(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::SplitMix64;

    #[test]
    fn hello_backoff() {
        let mut t = HelloTimer::new(0);
        let mut sent = vec![];
        for ms in (0..200_000).step_by(100) {
            if t.poll(ms) {
                sent.push(ms);
            }
        }
        assert_eq!(
            &sent[..7],
            &[0, 1_000, 3_000, 7_000, 15_000, 31_000, 63_000]
        );
        assert_eq!(sent[7] - sent[6], 60_000);
        // A stable day: ~1440 hellos.
        let n = HelloTimer::count_hellos(0, 86_400_000, &[]);
        assert!((1_435..=1_445).contains(&n), "{n}");
        let m = HelloTimer::count_hellos(0, 86_400_000, &[3_600_000]);
        assert!(m > n && m < n + 8, "{m}");
    }

    #[test]
    fn digest_deltas_and_refresh() {
        let mut rng = SplitMix64(1);
        let tags: Vec<[u8; 8]> = (0..10u64).map(|i| i.to_be_bytes()).collect();
        let mut f = CuckooFilter::build_digest(&tags, 32, &mut rng).unwrap();
        let mut a = DigestAdvertiser::new(f.clone(), 0);
        assert_eq!(a.tick(&f, 1_000), None);
        f.insert_keep_padding(&[0xAA; 8], &mut rng).unwrap();
        match a.tick(&f, 2_000) {
            Some(DigestMsg::Delta(d)) => assert!(d.encoded_len() < 30),
            m => panic!("{m:?}"),
        }
        assert_eq!(a.tick(&f, 3_000), None);
        assert!(matches!(a.tick(&f, 300_000), Some(DigestMsg::Full(_))));
    }
}
