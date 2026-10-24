//! Blinded destination labels (PROTOCOL §9.2).
//!
//! `label = BLAKE3-keyed(key = BLAKE3-derive("OMCP-BlindLabel-v1", recipient_ed25519_pk),
//!                       msg = epoch_u64_be)`
//!
//! Labels rotate every [`LABEL_EPOCH_MS`] (24 h) so long-term observers cannot
//! link a mailbox across days without the recipient's public key. Senders who
//! know the recipient's key can compute it; gateways never learn the PeerID.

/// 32-byte blinded mailbox label.
pub type BlindedLabel = [u8; 32];

/// Label rotation period (24 h).
pub const LABEL_EPOCH_MS: u64 = 24 * 60 * 60 * 1000;

const CONTEXT: &str = "OMCP-BlindLabel-v1";

/// Epoch index for a wall-clock time in ms.
pub fn epoch_for(now_ms: u64) -> u64 {
    now_ms / LABEL_EPOCH_MS
}

/// Compute the blinded label for `recipient_pk` in `epoch`.
pub fn blinded_label(recipient_pk: &[u8; 32], epoch: u64) -> BlindedLabel {
    let key = blake3::derive_key(CONTEXT, recipient_pk);
    *blake3::keyed_hash(&key, &epoch.to_be_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_rotate_and_differ_per_key() {
        let a = [1u8; 32];
        let b = [2u8; 32];
        assert_eq!(blinded_label(&a, 5), blinded_label(&a, 5));
        assert_ne!(blinded_label(&a, 5), blinded_label(&a, 6));
        assert_ne!(blinded_label(&a, 5), blinded_label(&b, 5));
        assert_ne!(&blinded_label(&a, 5), &a);
    }
}
