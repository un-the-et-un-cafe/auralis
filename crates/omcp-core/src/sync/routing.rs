//! Interest-based forwarding (PROTOCOL §22.1, §22.7(1)) and per-neighbour rate
//! limiting (§22.4).
//!
//! Forwarding order above 32 peers: (a) every neighbour that *wants* the tag;
//! else (b) the neighbour advertising the lowest *carry distance* for it;
//! else (c) binary spray-and-wait (k = 2, L = 8).

use std::collections::HashMap;

use super::cuckoo::CuckooFilter;
use super::tag::{carry_tag, MailboxTag, MAX_CARRY_DISTANCE};
use super::SplitMix64;

/// Carry tags live at most this long after the last delivery/forward.
pub const CARRY_TTL_MS: u64 = 3_600_000;
/// At most this many carry tags per node.
pub const MAX_CARRY_ENTRIES: usize = 64;

/// Distance a neighbour's digest advertises for `tag`: 0 if it wants it,
/// `d` for the lowest carry entry, `None` otherwise.
pub fn neighbour_distance(f: &CuckooFilter, tag: &MailboxTag) -> Option<u8> {
    if f.contains(tag) {
        return Some(0);
    }
    (1..=MAX_CARRY_DISTANCE).find(|d| f.contains(&carry_tag(tag, *d)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CarryEntry {
    last_active_ms: u64,
    distance: Option<u8>,
}

/// Distance-vector carry table (§22.7(1)).
#[derive(Debug, Clone, Default)]
pub struct CarryTable {
    entries: HashMap<MailboxTag, CarryEntry>,
}

impl CarryTable {
    /// This node delivered or forwarded an envelope for `tag` at `now_ms`.
    pub fn note_activity(&mut self, tag: MailboxTag, now_ms: u64) {
        self.expire(now_ms);
        let e = self.entries.entry(tag).or_insert(CarryEntry {
            last_active_ms: now_ms,
            distance: None,
        });
        e.last_active_ms = e.last_active_ms.max(now_ms);
        while self.entries.len() > MAX_CARRY_ENTRIES {
            let oldest = *self
                .entries
                .iter()
                .min_by_key(|(t, e)| (e.last_active_ms, **t))
                .expect("nonempty")
                .0;
            self.entries.remove(&oldest);
        }
    }

    /// Recompute `d = 1 + min(neighbour distance)` for every active tag.
    pub fn update_from_neighbours(&mut self, neighbours: &[&CuckooFilter], now_ms: u64) {
        self.expire(now_ms);
        for (tag, e) in self.entries.iter_mut() {
            e.distance = neighbours
                .iter()
                .filter_map(|f| neighbour_distance(f, tag))
                .min()
                .map(|d| d + 1)
                .filter(|d| *d <= MAX_CARRY_DISTANCE);
        }
    }

    pub fn expire(&mut self, now_ms: u64) {
        self.entries
            .retain(|_, e| now_ms < e.last_active_ms + CARRY_TTL_MS);
    }

    pub fn distance(&self, tag: &MailboxTag) -> Option<u8> {
        self.entries.get(tag).and_then(|e| e.distance)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Digest entries to advertise (go into the same padded cuckoo digest).
    pub fn advertised(&self) -> Vec<MailboxTag> {
        let mut v: Vec<MailboxTag> = self
            .entries
            .iter()
            .filter_map(|(t, e)| e.distance.map(|d| carry_tag(t, d)))
            .collect();
        v.sort_unstable();
        v
    }
}

/// Above this many visible peers, epidemic flooding (§8.3) is replaced.
pub const FLOOD_MAX_PEERS: usize = 32;
/// Spray-and-wait: total copies L.
pub const SPRAY_COPIES: u8 = 8;
/// Spray-and-wait: at most k neighbours per hop when nothing matches.
pub const SPRAY_FANOUT: usize = 2;
/// Default per-neighbour token bucket rate (envelopes/s).
pub const DEFAULT_NEIGHBOUR_RATE: u32 = 64;

/// HyperLogLog (p = 8, 256 registers) over peer ids; ~6.5 % std error.
#[derive(Debug, Clone)]
pub struct HyperLogLog {
    regs: [u8; 256],
}

impl Default for HyperLogLog {
    fn default() -> Self {
        Self { regs: [0; 256] }
    }
}

impl HyperLogLog {
    pub fn insert(&mut self, peer_id: &[u8]) {
        let h = blake3::hash(peer_id);
        let x = u64::from_le_bytes(h.as_bytes()[..8].try_into().expect("8"));
        let idx = (x & 0xff) as usize;
        let w = x >> 8;
        let rank = (w.trailing_zeros().min(56) + 1) as u8;
        if rank > self.regs[idx] {
            self.regs[idx] = rank;
        }
    }

    /// Merge a neighbour's sketch (register-wise max).
    pub fn merge(&mut self, other: &HyperLogLog) {
        for (a, b) in self.regs.iter_mut().zip(other.regs.iter()) {
            *a = (*a).max(*b);
        }
    }

    pub fn estimate(&self) -> f64 {
        let m = 256.0f64;
        let alpha = 0.7213 / (1.0 + 1.079 / m);
        let sum: f64 = self.regs.iter().map(|r| 2f64.powi(-(*r as i32))).sum();
        let raw = alpha * m * m / sum;
        let zeros = self.regs.iter().filter(|r| **r == 0).count();
        if raw <= 2.5 * m && zeros > 0 {
            m * (m / zeros as f64).ln()
        } else {
            raw
        }
    }
}

/// `ttl_hops = clamp(ceil(log2(n)) + 2, 3, 12)`.
pub fn adaptive_ttl(estimated_mesh_size: f64) -> u8 {
    let n = estimated_mesh_size.max(1.0);
    let l = n.log2().ceil() as i64 + 2;
    l.clamp(3, 12) as u8
}

/// What a node does with one envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardPlan {
    /// Small mesh: flood to everyone except the sender.
    Flood(Vec<usize>),
    /// Interest match: hand a copy to each matching neighbour.
    Interest(Vec<usize>),
    /// No wanter: hand a copy to the neighbour with the lowest carry distance.
    Carry(usize),
    /// No match: spray `(neighbour, copies)`; caller keeps the remainder.
    Spray { targets: Vec<(usize, u8)>, keep: u8 },
    /// Wait phase (1 copy left, no match): hold until a matching neighbour appears.
    Wait,
}

/// Decide forwarding for an envelope with `tag`, holding `copies` spray
/// tokens, given neighbour digests (`None` = digest unknown). `visible_peers`
/// selects flood vs interest mode; `from` is excluded.
pub fn plan_forward(
    tag: &MailboxTag,
    copies: u8,
    neighbours: &[Option<&CuckooFilter>],
    from: Option<usize>,
    visible_peers: usize,
    rng: &mut SplitMix64,
) -> ForwardPlan {
    let cands: Vec<usize> = (0..neighbours.len()).filter(|i| Some(*i) != from).collect();
    if visible_peers <= FLOOD_MAX_PEERS {
        return ForwardPlan::Flood(cands);
    }
    let matching: Vec<usize> = cands
        .iter()
        .copied()
        .filter(|i| neighbours[*i].is_some_and(|f| f.contains(tag)))
        .collect();
    if !matching.is_empty() {
        return ForwardPlan::Interest(matching);
    }
    let best = cands
        .iter()
        .filter_map(|i| {
            neighbours[*i]
                .and_then(|f| neighbour_distance(f, tag))
                .map(|d| (d, *i))
        })
        .min();
    if let Some((_, i)) = best {
        return ForwardPlan::Carry(i);
    }
    if copies <= 1 || cands.is_empty() {
        return ForwardPlan::Wait;
    }
    // Binary spray: give away half, split over at most k = 2 neighbours.
    let give = copies / 2;
    let mut pool = cands;
    let k = SPRAY_FANOUT.min(pool.len()).min(give as usize);
    let mut targets = Vec::with_capacity(k);
    for j in 0..k {
        let pick = rng.below(pool.len() as u64) as usize;
        let n = pool.swap_remove(pick);
        let share = give / k as u8 + u8::from((j as u8) < give % k as u8);
        targets.push((n, share));
    }
    ForwardPlan::Spray {
        targets,
        keep: copies - give,
    }
}

/// Per-neighbour token bucket (§22.4).
#[derive(Debug, Clone)]
pub struct TokenBucket {
    rate_per_s: u32,
    burst: u32,
    tokens_milli: u64,
    last_ms: u64,
}

impl TokenBucket {
    pub fn new(rate_per_s: u32, burst: u32, now_ms: u64) -> Self {
        Self {
            rate_per_s,
            burst,
            tokens_milli: burst as u64 * 1000,
            last_ms: now_ms,
        }
    }

    /// Default: 64 envelopes/s, burst 64.
    pub fn default_neighbour(now_ms: u64) -> Self {
        Self::new(DEFAULT_NEIGHBOUR_RATE, DEFAULT_NEIGHBOUR_RATE, now_ms)
    }

    /// Try to admit one envelope.
    pub fn admit(&mut self, now_ms: u64) -> bool {
        let dt = now_ms.saturating_sub(self.last_ms);
        self.last_ms = self.last_ms.max(now_ms);
        self.tokens_milli =
            (self.tokens_milli + dt * self.rate_per_s as u64).min(self.burst as u64 * 1000);
        if self.tokens_milli >= 1000 {
            self.tokens_milli -= 1000;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_formula() {
        assert_eq!(adaptive_ttl(1.0), 3);
        assert_eq!(adaptive_ttl(32.0), 7);
        assert_eq!(adaptive_ttl(1000.0), 12);
        assert_eq!(adaptive_ttl(1e9), 12);
    }

    #[test]
    fn hll_estimates_within_15_percent() {
        for n in [100u32, 1000, 10_000] {
            let mut h = HyperLogLog::default();
            for i in 0..n {
                h.insert(&i.to_be_bytes());
            }
            let e = h.estimate();
            assert!((e - n as f64).abs() / (n as f64) < 0.15, "n={n} e={e}");
        }
    }

    #[test]
    fn plan_modes() {
        let mut rng = SplitMix64(9);
        let tag = [1u8; 8];
        let f = CuckooFilter::build_digest(&[tag], 16, &mut rng).unwrap();
        let g = CuckooFilter::build_digest(&[[2u8; 8]], 16, &mut rng).unwrap();
        let ns = [Some(&g), Some(&f), None, Some(&g)];
        assert_eq!(
            plan_forward(&tag, 8, &ns, Some(0), 10, &mut rng),
            ForwardPlan::Flood(vec![1, 2, 3])
        );
        assert_eq!(
            plan_forward(&tag, 8, &ns, None, 100, &mut rng),
            ForwardPlan::Interest(vec![1])
        );
        let ns2 = [Some(&g), None, Some(&g)];
        match plan_forward(&tag, 8, &ns2, None, 100, &mut rng) {
            ForwardPlan::Spray { targets, keep } => {
                assert_eq!(targets.len(), 2);
                assert_eq!(targets.iter().map(|t| t.1).sum::<u8>() + keep, 8);
                assert_eq!(keep, 4);
            }
            p => panic!("{p:?}"),
        }
        assert_eq!(
            plan_forward(&tag, 1, &ns2, None, 100, &mut rng),
            ForwardPlan::Wait
        );
        match plan_forward(&tag, 2, &ns2, None, 100, &mut rng) {
            ForwardPlan::Spray { targets, keep } => {
                assert_eq!(targets.len(), 1);
                assert_eq!(keep, 1);
            }
            p => panic!("{p:?}"),
        }
    }

    #[test]
    fn carry_order_and_table() {
        let mut rng = SplitMix64(5);
        let tag = [7u8; 8];
        let far = CuckooFilter::build_digest(&[carry_tag(&tag, 3)], 16, &mut rng).unwrap();
        let near = CuckooFilter::build_digest(&[carry_tag(&tag, 1)], 16, &mut rng).unwrap();
        let want = CuckooFilter::build_digest(&[tag], 16, &mut rng).unwrap();
        let none = CuckooFilter::build_digest(&[[1; 8]], 16, &mut rng).unwrap();
        assert_eq!(
            plan_forward(
                &tag,
                8,
                &[Some(&far), Some(&near), Some(&none)],
                None,
                99,
                &mut rng
            ),
            ForwardPlan::Carry(1)
        );
        assert_eq!(
            plan_forward(&tag, 8, &[Some(&far), Some(&want)], None, 99, &mut rng),
            ForwardPlan::Interest(vec![1])
        );
        let mut t = CarryTable::default();
        t.update_from_neighbours(&[&near], 0);
        assert!(t.advertised().is_empty()); // no activity, no carry
        t.note_activity(tag, 0);
        t.update_from_neighbours(&[&far, &near], 0);
        assert_eq!(t.distance(&tag), Some(2));
        assert_eq!(t.advertised(), vec![carry_tag(&tag, 2)]);
        t.update_from_neighbours(&[&want], 10);
        assert_eq!(t.distance(&tag), Some(1));
        t.expire(CARRY_TTL_MS);
        assert!(t.is_empty());
        for i in 0..100u64 {
            t.note_activity(i.to_be_bytes(), i);
        }
        assert_eq!(t.len(), MAX_CARRY_ENTRIES);
        assert_eq!(t.distance(&0u64.to_be_bytes()), None);
    }

    #[test]
    fn token_bucket_limits() {
        let mut b = TokenBucket::default_neighbour(0);
        let ok = (0..200).filter(|_| b.admit(0)).count();
        assert_eq!(ok, 64);
        assert!(b.admit(20)); // 20 ms * 64/s = 1.28 tokens
        assert!(!b.admit(20));
    }
}
