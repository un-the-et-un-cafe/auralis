//! Deterministic simulation harness for OMCP scalable sync (PROTOCOL §22.6).
//!
//! Every scenario takes an explicit `seed` and uses only
//! [`omcp_core::sync::SplitMix64`], so runs are bit-for-bit reproducible.
//! Scenarios use the real `omcp_core::sync` data structures (tags, cuckoo
//! digests, reconciliation messages, watermarks, relay store) and count the
//! bytes their encodings actually take; what is *modelled* rather than
//! executed is documented on each scenario.

#![forbid(unsafe_code)]

pub mod sy1;
pub mod sy23;
pub mod sy4;
pub mod sy5;
pub mod sy6;

/// Encoded size of a typical text envelope: `EnvelopeV1` fixed header (41) +
/// tag TLV (10) + one 256 B padded ciphertext bucket.
pub const TEXT_ENVELOPE_BYTES: u64 = 41 + 10 + 256;

/// Median of a slice (sorts in place).
pub fn median(v: &mut [u64]) -> u64 {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Percentile (0..=100) of a sorted slice.
pub fn percentile(sorted: &[u64], p: usize) -> u64 {
    sorted[(sorted.len() - 1) * p / 100]
}
