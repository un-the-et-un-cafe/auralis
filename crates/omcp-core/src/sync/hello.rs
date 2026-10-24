//! Hello cadence and digest advertisement (PROTOCOL §22.7(2)).
//!
//! - Hello: 1 s after a neighbourhood change, doubling to 60 s while stable
//!   (120 s with more than 32 neighbours, §22.7(11)).
//! - Digests: deltas on change; a full digest only to a new neighbour, on
//!   `DIGEST_REQ`, or as a 30-min safety refresh (§22.7(11)).
//! - `HELLO` / `DIGEST_REQ` are 6-byte link-control frames (§22.7(12),
//!   registry §21.9).

use super::cuckoo::{CuckooFilter, DigestDelta};

pub const HELLO_MIN_MS: u64 = 1_000;
pub const HELLO_MAX_MS: u64 = 60_000;
/// Hello ceiling above [`DENSE_NEIGHBOURS`].
pub const HELLO_MAX_DENSE_MS: u64 = 120_000;
/// Neighbour count above which the longer hello ceiling applies.
pub const DENSE_NEIGHBOURS: usize = 32;
pub const DIGEST_REFRESH_MS: u64 = 30 * 60_000;
/// `DIGEST_REQ` on the wire (same 6-byte link-control layout).
pub const DIGEST_REQ_BYTES: u64 = 6;

/// Hello ceiling for a neighbourhood of `neighbours`.
pub fn hello_max_ms(neighbours: usize) -> u64 {
    if neighbours > DENSE_NEIGHBOURS {
        HELLO_MAX_DENSE_MS
    } else {
        HELLO_MAX_MS
    }
}
/// Link-control frame size (§22.7(12), registry §21.9).
pub const HELLO_BYTES: u64 = 6;
/// §21.9 link-control frame type `HELLO`.
pub const TYPE_HELLO: u8 = 0x48;
/// §21.9 link-control frame type `DIGEST_REQ`.
pub const TYPE_DIGEST_REQ: u8 = 0x52;
/// `flags` bit0: a delta digest follows this frame. Bits 1–7 reserved (0).
pub const FLAG_HAS_DELTA: u8 = 0x01;
/// `DIGEST_REQ` digest_id meaning "send me your full digest".
pub const DIGEST_ID_FULL: u32 = 0;

/// `digest_id` = low 32 bits of BLAKE2b(-512) over the encoded want-digest,
/// i.e. the last 4 output bytes read as a big-endian u32.
pub fn digest_id32(f: &CuckooFilter) -> u32 {
    use blake2::{Blake2b512, Digest};
    let h = Blake2b512::digest(f.encode());
    u32::from_be_bytes([h[60], h[61], h[62], h[63]])
}

/// 6-byte link-control frame: `type u8 | flags u8 | digest_id u32-be`
/// (§22.7(12)). These are link-control frames (§21.9), not `header_aad` TLVs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkControl {
    /// Beacon advertising the current want-digest.
    Hello { has_delta: bool, digest_id: u32 },
    /// Request a digest (`digest_id` = [`DIGEST_ID_FULL`] for a full one).
    DigestReq { has_delta: bool, digest_id: u32 },
}

impl LinkControl {
    pub fn hello_for(f: &CuckooFilter, has_delta: bool) -> Self {
        Self::Hello {
            has_delta,
            digest_id: digest_id32(f),
        }
    }

    pub fn encode(&self) -> [u8; 6] {
        let (t, d, id) = match *self {
            Self::Hello {
                has_delta,
                digest_id,
            } => (TYPE_HELLO, has_delta, digest_id),
            Self::DigestReq {
                has_delta,
                digest_id,
            } => (TYPE_DIGEST_REQ, has_delta, digest_id),
        };
        let b = id.to_be_bytes();
        [t, u8::from(d), b[0], b[1], b[2], b[3]]
    }

    /// Decode; rejects wrong length, unregistered types and non-zero
    /// reserved flag bits.
    pub fn decode(b: &[u8]) -> crate::Result<Self> {
        if b.len() != HELLO_BYTES as usize {
            return Err(crate::OmcpError::Sync("link-control length"));
        }
        if b[1] & !FLAG_HAS_DELTA != 0 {
            return Err(crate::OmcpError::Sync("link-control reserved flags"));
        }
        let has_delta = b[1] & FLAG_HAS_DELTA != 0;
        let digest_id = u32::from_be_bytes([b[2], b[3], b[4], b[5]]);
        match b[0] {
            TYPE_HELLO => Ok(Self::Hello {
                has_delta,
                digest_id,
            }),
            TYPE_DIGEST_REQ => Ok(Self::DigestReq {
                has_delta,
                digest_id,
            }),
            _ => Err(crate::OmcpError::Sync("link-control type not in §21.9")),
        }
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

/// Should we send `DIGEST_REQ`? Yes if a neighbour's `HELLO` carries a
/// digest id we hold no digest (or delta chain) for.
pub fn needs_digest_req(cached: Option<&CuckooFilter>, hello_digest_id: u32) -> bool {
    cached.is_none_or(|f| digest_id32(f) != hello_digest_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_control_frames_exact_layout() {
        let mut rng = SplitMix64(4);
        let f = CuckooFilter::build_digest(&[[1; 8]], 16, &mut rng).unwrap();
        // digest_id = low 32 bits (last 4 bytes, BE) of BLAKE2b-512(digest).
        use blake2::{Blake2b512, Digest};
        let h = Blake2b512::digest(f.encode());
        let want = u32::from_be_bytes(h[60..64].try_into().unwrap());
        assert_eq!(digest_id32(&f), want);
        let hello = LinkControl::hello_for(&f, true);
        let b = hello.encode();
        assert_eq!(b[0], 0x48);
        assert_eq!(b[1], 0x01);
        assert_eq!(&b[2..], &want.to_be_bytes());
        assert_eq!(LinkControl::decode(&b).unwrap(), hello);
        let req = LinkControl::DigestReq {
            has_delta: false,
            digest_id: DIGEST_ID_FULL,
        };
        assert_eq!(req.encode(), [0x52, 0, 0, 0, 0, 0]);
        assert_eq!(LinkControl::decode(&req.encode()).unwrap(), req);
        // reserved flag bits must be zero
        for bit in 1..8 {
            let mut x = b;
            x[1] |= 1 << bit;
            assert!(LinkControl::decode(&x).is_err());
        }
        assert!(LinkControl::decode(&[0x49, 0, 0, 0, 0, 0]).is_err());
        assert!(LinkControl::decode(&b[..5]).is_err());
        assert!(LinkControl::decode(&[0x48, 0, 0, 0, 0, 0, 0]).is_err());
    }
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
        let id = digest_id32(&f);
        assert!(!needs_digest_req(Some(&f), id));
        assert!(needs_digest_req(None, id));
        let g = CuckooFilter::build_digest(&[[2; 8]], 16, &mut rng).unwrap();
        assert!(needs_digest_req(Some(&g), id));
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
