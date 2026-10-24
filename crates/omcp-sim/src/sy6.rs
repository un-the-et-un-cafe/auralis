//! SY6: a non-member cannot link one chat's tags across epochs.
//!
//! Generates `pairs` chats with random 32-byte exporter secrets and computes
//! each chat's tags for epochs `e` and `e+1` (2·pairs tags; 10⁶ for the
//! acceptance run). Statistics a non-member can compute from tags alone:
//! - per-bit balance of all tags (max |z| over 64 bits),
//! - per-bit balance of `tag_e XOR tag_{e+1}` for linked pairs (max |z|),
//! - mean Hamming distance of linked pairs vs. unrelated pairs (z-score),
//! - the best Hamming-threshold linking distinguisher's advantage,
//! - 64-bit collisions.

use std::collections::HashSet;

use omcp_core::sync::tag::mailbox_tag;
use omcp_core::sync::SplitMix64;

#[derive(Debug, Clone)]
pub struct Sy6Report {
    pub tags: usize,
    pub max_bit_z: f64,
    pub max_xor_bit_z: f64,
    pub hamming_linked_mean: f64,
    pub hamming_unlinked_mean: f64,
    pub hamming_z: f64,
    pub best_distinguisher_advantage: f64,
    pub collisions: usize,
}

pub fn run(pairs: usize, seed: u64) -> Sy6Report {
    let mut rng = SplitMix64(seed);
    let epoch = 20_000 + rng.below(1000);
    let mut a = Vec::with_capacity(pairs);
    let mut b = Vec::with_capacity(pairs);
    for _ in 0..pairs {
        let mut s = [0u8; 32];
        for c in s.chunks_mut(8) {
            c.copy_from_slice(&rng.next_u64().to_le_bytes());
        }
        a.push(u64::from_be_bytes(mailbox_tag(&s, epoch).expect("tag")));
        b.push(u64::from_be_bytes(mailbox_tag(&s, epoch + 1).expect("tag")));
    }
    let n = pairs as f64;
    let z = |ones: u64, total: f64| ((ones as f64) - total / 2.0) / (total / 4.0).sqrt();
    let mut bits = [0u64; 64];
    let mut xbits = [0u64; 64];
    let mut hist_l = [0u64; 65];
    let mut hist_u = [0u64; 65];
    for i in 0..pairs {
        let x = a[i] ^ b[i];
        let u = a[i] ^ b[(i + 1) % pairs];
        hist_l[x.count_ones() as usize] += 1;
        hist_u[u.count_ones() as usize] += 1;
        for k in 0..64 {
            bits[k] += (a[i] >> k) & 1;
            bits[k] += (b[i] >> k) & 1;
            xbits[k] += (x >> k) & 1;
        }
    }
    let max_bit_z = bits
        .iter()
        .map(|o| z(*o, 2.0 * n).abs())
        .fold(0.0, f64::max);
    let max_xor_bit_z = xbits.iter().map(|o| z(*o, n).abs()).fold(0.0, f64::max);
    let mean = |h: &[u64; 65]| {
        h.iter()
            .enumerate()
            .map(|(k, c)| k as f64 * *c as f64)
            .sum::<f64>()
            / n
    };
    let (ml, mu) = (mean(&hist_l), mean(&hist_u));
    // Var of Hamming(64, 1/2) = 16; difference of two means.
    let hz = (ml - mu) / (2.0 * 16.0 / n).sqrt();
    // Best threshold rule "linked iff hamming ≤ t" (or ≥ t).
    let mut best = 0.0f64;
    let (mut cl, mut cu) = (0u64, 0u64);
    for t in 0..=64 {
        cl += hist_l[t];
        cu += hist_u[t];
        let adv = ((cl as f64 - cu as f64) / n).abs() / 2.0;
        best = best.max(adv);
    }
    let mut set = HashSet::with_capacity(2 * pairs);
    let mut collisions = 0;
    for t in a.iter().chain(b.iter()) {
        if !set.insert(*t) {
            collisions += 1;
        }
    }
    Sy6Report {
        tags: 2 * pairs,
        max_bit_z,
        max_xor_bit_z,
        hamming_linked_mean: ml,
        hamming_unlinked_mean: mu,
        hamming_z: hz,
        best_distinguisher_advantage: best,
        collisions,
    }
}
