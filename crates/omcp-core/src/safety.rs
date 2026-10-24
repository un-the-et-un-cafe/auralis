//! Safety numbers for out-of-band contact verification (M5, experimental).
//!
//! 60 decimal digits (12 groups of 5) derived from both peers' X25519 static
//! public keys, order-independent so both sides display the same number:
//! `BLAKE2b-512("OMCP-Safety-v1" || min(a,b) || max(a,b))`, iterated, each
//! group = big-endian u40 from 5 bytes mod 100000.

use blake2::{Blake2b512, Digest};

/// Number of 5-digit groups.
pub const SAFETY_GROUPS: usize = 12;

/// Compute the safety number groups for two peers.
pub fn safety_number(a: &[u8; 32], b: &[u8; 32]) -> Vec<String> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut h = Blake2b512::new();
    h.update(b"OMCP-Safety-v1");
    h.update(lo);
    h.update(hi);
    let first = h.finalize();
    let mut second = Blake2b512::new();
    second.update(first);
    let second = second.finalize();
    let mut bytes = first.to_vec();
    bytes.extend_from_slice(&second);
    (0..SAFETY_GROUPS)
        .map(|i| {
            let c = &bytes[i * 5..i * 5 + 5];
            let v = c.iter().fold(0u64, |acc, x| (acc << 8) | u64::from(*x));
            format!("{:05}", v % 100_000)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_and_distinct() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        let n = safety_number(&a, &b);
        assert_eq!(n, safety_number(&b, &a));
        assert_eq!(n.len(), 12);
        assert!(n.iter().all(|g| g.len() == 5));
        assert_ne!(n, safety_number(&a, &[3u8; 32]));
    }
}
