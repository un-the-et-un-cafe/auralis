//! Hello cadence and digest advertisement (PROTOCOL §22.7(2)).
//!
//! - Hello: 1 s after a neighbourhood change, doubling to 60 s while stable
//!   (120 s with more than 32 neighbours, §22.7(11)).
//! - Digests: deltas on change; a full digest only to a new neighbour, on
//!   `DIGEST_REQ`, or as a 30-min safety refresh (§22.7(11)).

use super::cuckoo::{CuckooFilter, DigestDelta};

pub const HELLO_MIN_MS: u64 = 1_000;
pub const HELLO_MAX_MS: u64 = 60_000;
/// Hello ceiling above [`DENSE_NEIGHBOURS`].
pub const HELLO_MAX_DENSE_MS: u64 = 120_000;
/// Neighbour count above which the longer hello ceiling applies.
pub const DENSE_NEIGHBOURS: usize = 32;
pub const DIGEST_REFRESH_MS: u64 = 30 * 60_000;
/// `DIGEST_REQ` on the wire: `type u8 | flags u8 | digest_id[4]`.
pub const DIGEST_REQ_BYTES: u64 = 6;

/// Hello ceiling for a neighbourhood of `neighbours`.
pub fn hello_max_ms(neighbours: usize) -> u64 {
    if neighbours > DENSE_NEIGHBOURS {
        HELLO_MAX_DENSE_MS
    } else {
        HELLO_MAX_MS
    }
}
/// Sync beacon on the wire: `type u8 | flags u8 | digest_id[4]` (6 B).
/// (Proposed pin; §22.7 leaves the beacon format open.)
pub const HELLO_BYTES: u64 = 6;
/// Beacon type byte.
pub const TYPE_SYNC_HELLO: u8 = 0x48;
/// `DIGEST_REQ` type byte.
pub const TYPE_DIGEST_REQ: u8 = 0x52;

/// Periodic sync beacon (§22.7(2)/(11)): advertises the current digest id
/// (first 4 bytes of [`CuckooFilter::digest_id`]) so neighbours know whether
/// their cached digest is current or they must send `DIGEST_REQ`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncHello {
    pub flags: u8,
    pub digest_id: [u8; 4],
}

impl SyncHello {
    pub fn for_digest(f: &CuckooFilter, flags: u8) -> Self {
        let id = f.digest_id();
        Self {
            flags,
            digest_id: [id[0], id[1], id[2], id[3]],
        }
    }

    pub fn encode(&self) -> [u8; 6] {
        let d = self.digest_id;
        [TYPE_SYNC_HELLO, self.flags, d[0], d[1], d[2], d[3]]
    }

    pub fn decode(b: &[u8]) -> crate::Result<Self> {
        if b.len() != 6 || b[0] != TYPE_SYNC_HELLO {
            return Err(crate::OmcpError::Sync("sync hello"));
        }
        Ok(Self {
            flags: b[1],
            digest_id: [b[2], b[3], b[4], b[5]],
        })
    }
}

#[derive(Debug, Clone)]
pub struct HelloTimer {
    interval_ms: u64,
    next_ms: u64,
    max_ms: u64,
}

impl HelloTimer {
    pub fn new(now_ms: u64) -> Self {
        Self {
            interval_ms: HELLO_MIN_MS,
            next_ms: now_ms,
            max_ms: HELLO_MAX_MS,
        }
    }

    /// Adapt the ceiling to the current neighbour count.
    pub fn set_neighbours(&mut self, neighbours: usize) {
        self.max_ms = hello_max_ms(neighbours);
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
        self.interval_ms = (self.interval_ms * 2).min(self.max_ms);
        true
    }

    /// Count hellos in `[start, end)` given sorted neighbourhood-change times.
    pub fn count_hellos(start_ms: u64, end_ms: u64, changes: &[u64], neighbours: usize) -> u64 {
        let mut t = HelloTimer::new(start_ms);
        t.set_neighbours(neighbours);
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

/// A digest transmission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DigestMsg {
    /// Full digest (broadcast refresh, or unicast to a new neighbour /
    /// in answer to `DIGEST_REQ`).
    Full(Vec<u8>),
    /// Broadcast delta on change.
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

    /// Periodic tick: a delta if the digest changed, a full digest only as
    /// the 30-min safety refresh (or if a delta is impossible).
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

    /// Unicast full digest for a new neighbour or a `DIGEST_REQ`.
    pub fn full_for_peer(&self, current: &CuckooFilter) -> DigestMsg {
        DigestMsg::Full(current.encode())
    }
}

/// Should we send `DIGEST_REQ`? Yes if a neighbour's hello carries a digest
/// id we hold no digest (or delta chain) for.
pub fn needs_digest_req(cached: Option<&CuckooFilter>, hello: &SyncHello) -> bool {
    cached.is_none_or(|f| SyncHello::for_digest(f, 0).digest_id != hello.digest_id)
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
        let n = HelloTimer::count_hellos(0, 86_400_000, &[], 8);
        assert!((1_435..=1_445).contains(&n), "{n}");
        let m = HelloTimer::count_hellos(0, 86_400_000, &[3_600_000], 8);
        assert!(m > n && m < n + 8, "{m}");
        let dense = HelloTimer::count_hellos(0, 86_400_000, &[], 33);
        assert!((715..=730).contains(&dense), "{dense}");
        let mut rng = SplitMix64(3);
        let f = CuckooFilter::build_digest(&[[1; 8]], 16, &mut rng).unwrap();
        let h = SyncHello::for_digest(&f, 0);
        assert_eq!(SyncHello::decode(&h.encode()).unwrap(), h);
        assert_eq!(h.encode().len() as u64, HELLO_BYTES);
        assert!(needs_digest_req(None, &h));
        assert!(!needs_digest_req(Some(&f), &h));
        let g = CuckooFilter::build_digest(&[[2; 8]], 16, &mut rng).unwrap();
        assert!(needs_digest_req(Some(&g), &h));
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
        assert_eq!(a.tick(&f, 300_000), None);
        assert!(matches!(
            a.tick(&f, DIGEST_REFRESH_MS),
            Some(DigestMsg::Full(_))
        ));
        assert!(matches!(a.full_for_peer(&f), DigestMsg::Full(_)));
    }
}
