//! Rotating mailbox tags (PROTOCOL §22.1).
//!
//! `tag = BLAKE2b-64(key = exporter_secret, "OMCP-tag" || epoch_be)` with a 6 h
//! epoch. Only chat members (who hold the MLS / session exporter secret) can
//! compute it; the secret is `MLS-Exporter("OMCP-tag", epoch, 32)` for every
//! chat, 1:1 chats being 2-member MLS groups (§22.7(5)). Tags ride in
//! `EnvelopeV1::header_aad` as TLV `0x54` (len 8) per the §21.8 registry.

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
/// MLS exporter label for tag secrets (§22.7(5)).
pub const TAG_EXPORTER_LABEL: &str = "OMCP-tag";

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

/// `header_aad` TLV registry class (PROTOCOL §21.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlvClass {
    /// `0x01`–`0x3F`: reserved for core.
    Core,
    /// `0x54`: mailbox tag (len 8).
    MailboxTag,
    /// `0x55`–`0x7F`: unassigned (extension process §16).
    Unassigned,
    /// `0x80`–`0xFF`: experimental / private use.
    Experimental,
    /// `0x00`, `0x40`–`0x53`: not in the registry.
    Invalid,
}

pub fn tlv_class(t: u8) -> TlvClass {
    match t {
        0x01..=0x3F => TlvClass::Core,
        AAD_TLV_TAG => TlvClass::MailboxTag,
        0x55..=0x7F => TlvClass::Unassigned,
        0x80..=0xFF => TlvClass::Experimental,
        _ => TlvClass::Invalid,
    }
}

/// Append the tag TLV to a `header_aad` buffer.
pub fn put_tag_in_aad(aad: &mut Vec<u8>, tag: &MailboxTag) {
    aad.push(AAD_TLV_TAG);
    aad.push(8);
    aad.extend_from_slice(tag);
}

/// Parse a `header_aad` of `type u8 | len u8 | value` items per the §21.8
/// registry and return the mailbox tag, if any. Rejects: truncated items,
/// types outside the registry, a `0x54` item whose length is not 8, and
/// duplicate `0x54` items. Core, unassigned and experimental types are
/// skipped (forward compatibility).
pub fn tag_from_aad(aad: &[u8]) -> Result<Option<MailboxTag>> {
    let mut i = 0;
    let mut found = None;
    while i < aad.len() {
        if i + 2 > aad.len() {
            return Err(OmcpError::Sync("aad tlv truncated"));
        }
        let (t, l) = (aad[i], aad[i + 1] as usize);
        let v = aad
            .get(i + 2..i + 2 + l)
            .ok_or(OmcpError::Sync("aad tlv truncated"))?;
        match tlv_class(t) {
            TlvClass::Invalid => return Err(OmcpError::Sync("aad tlv type not in registry")),
            TlvClass::MailboxTag => {
                if l != 8 {
                    return Err(OmcpError::Sync("tag tlv length"));
                }
                if found.is_some() {
                    return Err(OmcpError::Sync("duplicate tag tlv"));
                }
                let mut tag = [0u8; 8];
                tag.copy_from_slice(v);
                found = Some(tag);
            }
            _ => {}
        }
        i += 2 + l;
    }
    Ok(found)
}

/// 32-byte gateway rendezvous label (§22.7(3)):
/// `HKDF-SHA-384(ikm = tag_secret, info = "OMCP-rdv" || epoch_be, L = 32)`.
/// The existing `OpaqueEnvelope` / rendezvous server carry it unchanged.
pub fn rendezvous_label(tag_secret: &[u8], epoch: u64) -> Result<[u8; 32]> {
    let hk = hkdf::Hkdf::<sha2::Sha384>::new(None, tag_secret);
    let mut info = Vec::with_capacity(16);
    info.extend_from_slice(b"OMCP-rdv");
    info.extend_from_slice(&epoch.to_be_bytes());
    let mut out = [0u8; 32];
    hk.expand(&info, &mut out)
        .map_err(|_| OmcpError::Sync("hkdf expand"))?;
    Ok(out)
}

/// Maximum carry distance encoded in digests (= max adaptive TTL).
pub const MAX_CARRY_DISTANCE: u8 = 12;

/// Digest entry advertising "I can reach wanters of `tag` in `d` hops"
/// (§22.7(1)). Only nodes that saw `tag` can compute or test it.
pub fn carry_tag(tag: &MailboxTag, d: u8) -> MailboxTag {
    let mut m = [0u8; 9];
    m[..8].copy_from_slice(tag);
    m[8] = d;
    let h = blake3::derive_key("OMCP-carry-v1", &m);
    let mut t = [0u8; 8];
    t.copy_from_slice(&h[..8]);
    t
}

/// Source of per-epoch tag secrets (§22.7(5)): every chat — 1:1 included —
/// is an MLS group and the secret is `MLS-Exporter("OMCP-tag", epoch, 32)`.
/// Implemented by [`crate::mls::OmcpMlsMember`].
pub trait TagSecretSource {
    fn tag_secret(&self, tag_epoch: u64) -> Result<[u8; 32]>;
}

/// Tag for a chat at `tag_epoch` from its secret source.
pub fn chat_tag<S: TagSecretSource + ?Sized>(src: &S, tag_epoch: u64) -> Result<MailboxTag> {
    mailbox_tag(&src.tag_secret(tag_epoch)?, tag_epoch)
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
    fn rendezvous_label_is_hkdf_sha384() {
        let a = rendezvous_label(&[1; 32], 1).unwrap();
        assert_ne!(a, rendezvous_label(&[1; 32], 2).unwrap());
        assert_ne!(a, rendezvous_label(&[2; 32], 1).unwrap());
        let hk = hkdf::Hkdf::<sha2::Sha384>::new(None, &[1; 32]);
        let mut want = [0u8; 32];
        hk.expand(b"OMCP-rdv\0\0\0\0\0\0\0\x01", &mut want).unwrap();
        assert_eq!(a, want);
    }

    #[test]
    fn tlv_registry() {
        assert_eq!(tlv_class(0x01), TlvClass::Core);
        assert_eq!(tlv_class(0x54), TlvClass::MailboxTag);
        assert_eq!(tlv_class(0x60), TlvClass::Unassigned);
        assert_eq!(tlv_class(0x90), TlvClass::Experimental);
        assert_eq!(tlv_class(0x00), TlvClass::Invalid);
        assert_eq!(tlv_class(0x40), TlvClass::Invalid);
        // unassigned / experimental skipped, invalid rejected, dup rejected
        let mut aad = vec![0x60, 1, 0, 0x90, 0];
        put_tag_in_aad(&mut aad, &[4; 8]);
        assert_eq!(tag_from_aad(&aad).unwrap(), Some([4; 8]));
        put_tag_in_aad(&mut aad, &[4; 8]);
        assert!(tag_from_aad(&aad).is_err());
        assert!(tag_from_aad(&[0x45, 0]).is_err());
    }

    #[test]
    fn carry_tags_distinct_per_distance() {
        let t = [9u8; 8];
        assert_ne!(carry_tag(&t, 1), carry_tag(&t, 2));
        assert_ne!(carry_tag(&t, 1), t);
    }
}
