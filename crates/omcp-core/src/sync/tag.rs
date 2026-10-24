//! Rotating mailbox tags (PROTOCOL §22.1).
//!
//! `tag = BLAKE2b-64(key = exporter_secret, "OMCP-tag" || epoch_be)` with a 6 h
//! epoch. Only chat members (who hold the MLS / session exporter secret) can
//! compute it. Tags ride in `EnvelopeV1::header_aad` as a TLV
//! (type [`AAD_TLV_TAG`], length 8) — the canonical framing is unchanged.

use blake2::digest::consts::U8;
use blake2::digest::Mac;
use blake2::Blake2bMac;

use crate::error::{OmcpError, Result};

/// 8-byte routing tag.
pub type MailboxTag = [u8; 8];

/// Tag rotation period: 6 h.
pub const TAG_EPOCH_MS: u64 = 6 * 60 * 60 * 1000;

/// `header_aad` TLV type for the routing tag (ASCII 'T').
pub const AAD_TLV_TAG: u8 = 0x54;

const LABEL: &[u8] = b"OMCP-tag";

/// Tag epoch for wall-clock `now_ms`.
pub fn tag_epoch(now_ms: u64) -> u64 {
    now_ms / TAG_EPOCH_MS
}

/// Derive the tag for `exporter_secret` (16..=64 bytes) in `epoch`.
pub fn mailbox_tag(exporter_secret: &[u8], epoch: u64) -> Result<MailboxTag> {
    if exporter_secret.len() < 16 {
        return Err(OmcpError::Sync("exporter secret too short"));
    }
    let mut mac = <Blake2bMac<U8> as Mac>::new_from_slice(exporter_secret)
        .map_err(|_| OmcpError::Sync("exporter secret too long"))?;
    mac.update(LABEL);
    mac.update(&epoch.to_be_bytes());
    let out = mac.finalize().into_bytes();
    let mut t = [0u8; 8];
    t.copy_from_slice(&out);
    Ok(t)
}

/// Tags a receiver should accept at `now_ms`: current and previous epoch
/// (clock skew / in-flight envelopes).
pub fn accepted_tags(exporter_secret: &[u8], now_ms: u64) -> Result<[MailboxTag; 2]> {
    let e = tag_epoch(now_ms);
    Ok([
        mailbox_tag(exporter_secret, e)?,
        mailbox_tag(exporter_secret, e.saturating_sub(1))?,
    ])
}

/// Append the tag TLV to a `header_aad` buffer.
pub fn put_tag_in_aad(aad: &mut Vec<u8>, tag: &MailboxTag) {
    aad.push(AAD_TLV_TAG);
    aad.push(8);
    aad.extend_from_slice(tag);
}

/// Find the tag TLV in a `header_aad` buffer of `type u8 | len u8 | value` items.
pub fn tag_from_aad(aad: &[u8]) -> Result<Option<MailboxTag>> {
    let mut i = 0;
    while i < aad.len() {
        if i + 2 > aad.len() {
            return Err(OmcpError::Sync("aad tlv truncated"));
        }
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad
            .get(i + 2..i + 2 + l)
            .ok_or(OmcpError::Sync("aad tlv truncated"))?;
        if t == AAD_TLV_TAG {
            if l != 8 {
                return Err(OmcpError::Sync("tag tlv length"));
            }
            let mut tag = [0u8; 8];
            tag.copy_from_slice(v);
            return Ok(Some(tag));
        }
        i += 2 + l;
    }
    Ok(None)
}

/// Gateway rendezvous mailbox for a tag (§22.5 ↔ §9.2 bridge): a 32-byte
/// label so the existing `OpaqueEnvelope` / rendezvous server stay unchanged
/// and the gateway still sees only an opaque label.
pub fn tag_rendezvous_label(tag: &MailboxTag, epoch: u64) -> [u8; 32] {
    let key = blake3::derive_key("OMCP-TagMailbox-v1", tag);
    *blake3::keyed_hash(&key, &epoch.to_be_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_rotates_and_is_keyed() {
        let s = [7u8; 32];
        let t = [8u8; 32];
        assert_eq!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&s, 1).unwrap());
        assert_ne!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&s, 2).unwrap());
        assert_ne!(mailbox_tag(&s, 1).unwrap(), mailbox_tag(&t, 1).unwrap());
        assert!(mailbox_tag(&[1u8; 8], 1).is_err());
        assert!(mailbox_tag(&[1u8; 65], 1).is_err());
        assert_eq!(tag_epoch(TAG_EPOCH_MS * 3 + 5), 3);
        let acc = accepted_tags(&s, TAG_EPOCH_MS * 3).unwrap();
        assert_eq!(acc[1], mailbox_tag(&s, 2).unwrap());
    }

    #[test]
    fn aad_roundtrip() {
        let mut aad = vec![0x01, 2, 9, 9];
        put_tag_in_aad(&mut aad, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(tag_from_aad(&aad).unwrap(), Some([1, 2, 3, 4, 5, 6, 7, 8]));
        assert_eq!(tag_from_aad(&[1, 1, 0]).unwrap(), None);
        assert!(tag_from_aad(&[0x54, 3, 1, 2, 3]).is_err());
        assert!(tag_from_aad(&[0x54]).is_err());
    }

    #[test]
    fn rendezvous_label_differs_from_tag_input() {
        let a = tag_rendezvous_label(&[1; 8], 1);
        assert_ne!(a, tag_rendezvous_label(&[1; 8], 2));
        assert_ne!(a, tag_rendezvous_label(&[2; 8], 1));
    }
}
