//! Burn after read (apps/REQUIREMENTS.md "Burn after read (v0.1.3)", PROTOCOL §19.1).
//!
//! - [`BurnHeader`]: the encrypted inner `burn` header, carried as a prefix of the
//!   AEAD plaintext ([`encode_inner`] / [`decode_inner`]); relays never see it.
//! - [`expires_at_ms`]: outer envelope expiry = `sent + unread_ttl`
//!   ([`crate::gateway::OpaqueEnvelope::expiry_ms`]).
//! - [`SingleUseMediaKey`]: a media Content Key that can be taken exactly once.
//! - [`OpenedReceipt`]: encrypted control message telling the sender to shred.
//! - [`BurnStore`]: receiver-side in-memory store; every item is sealed under
//!   its own random key, and burning zeroizes that key (crypto-shredding).

use std::collections::HashMap;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{OmcpError, Result};
use crate::files::ContentKey;

/// Allowed sender-chosen unread TTLs: 1 h, 24 h, 7 days.
pub const UNREAD_TTL_1H: u32 = 3_600;
pub const UNREAD_TTL_24H: u32 = 86_400;
pub const UNREAD_TTL_7D: u32 = 604_800;
/// Allowed timer values (seconds) for [`BurnMode::Timer`]: 10 s, 1 min, 1 h.
pub const TIMER_SECS: [u32; 3] = [10, 60, 3_600];
/// Burn is unavailable in channels with more than this many members.
pub const MAX_BURN_GROUP_MEMBERS: usize = 256;

/// Inner header tag: no burn header.
pub const INNER_TAG_NONE: u8 = 0x00;
/// Inner header tag: `burn` header follows (9 bytes).
pub const INNER_TAG_BURN: u8 = 0x01;
/// Control message type for `OPENED` (PROTOCOL §19.1).
pub const CONTROL_OPENED: u8 = 0x30;
/// Control: re-request a media item by locator + hash (PROTOCOL §7.1, v0.1.4).
pub const CONTROL_MEDIA_REQUEST: u8 = 0x31;
/// Control: "I no longer have this media item".
pub const CONTROL_MEDIA_UNAVAILABLE: u8 = 0x32;
/// Control: user-confirmed "Ask to resend" for a media item the partner lacks.
pub const CONTROL_MEDIA_RESEND_ASK: u8 = 0x33;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BurnMode {
    /// Deleted as soon as it's closed (view once). `secs` = 0.
    OnOpen = 0,
    /// Deleted `secs` after it's opened.
    Timer = 1,
}

/// `burn: {mode, secs, unread_ttl}`. Wire: `mode u8 | secs u32 BE | unread_ttl u32 BE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BurnHeader {
    pub mode: BurnMode,
    pub secs: u32,
    pub unread_ttl: u32,
}

impl BurnHeader {
    pub const WIRE_LEN: usize = 9;

    pub fn on_open(unread_ttl: u32) -> Result<Self> {
        Self::new(BurnMode::OnOpen, 0, unread_ttl)
    }

    pub fn timer(secs: u32, unread_ttl: u32) -> Result<Self> {
        Self::new(BurnMode::Timer, secs, unread_ttl)
    }

    pub fn new(mode: BurnMode, secs: u32, unread_ttl: u32) -> Result<Self> {
        let h = Self {
            mode,
            secs,
            unread_ttl,
        };
        h.validate()?;
        Ok(h)
    }

    fn validate(&self) -> Result<()> {
        if ![UNREAD_TTL_1H, UNREAD_TTL_24H, UNREAD_TTL_7D].contains(&self.unread_ttl) {
            return Err(OmcpError::Burn("unread_ttl"));
        }
        match self.mode {
            BurnMode::OnOpen if self.secs != 0 => Err(OmcpError::Burn("on_open secs")),
            BurnMode::Timer if !TIMER_SECS.contains(&self.secs) => {
                Err(OmcpError::Burn("timer secs"))
            }
            _ => Ok(()),
        }
    }

    pub fn encode(&self) -> [u8; Self::WIRE_LEN] {
        let mut b = [0u8; Self::WIRE_LEN];
        b[0] = self.mode as u8;
        b[1..5].copy_from_slice(&self.secs.to_be_bytes());
        b[5..9].copy_from_slice(&self.unread_ttl.to_be_bytes());
        b
    }

    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != Self::WIRE_LEN {
            return Err(OmcpError::Burn("header length"));
        }
        let mode = match b[0] {
            0 => BurnMode::OnOpen,
            1 => BurnMode::Timer,
            _ => return Err(OmcpError::Burn("mode")),
        };
        let secs = u32::from_be_bytes(b[1..5].try_into().unwrap());
        let unread_ttl = u32::from_be_bytes(b[5..9].try_into().unwrap());
        Self::new(mode, secs, unread_ttl)
    }
}

/// Outer envelope `expires_at` (ms): `sent_ms + unread_ttl`.
pub fn expires_at_ms(sent_ms: u64, h: &BurnHeader) -> u64 {
    sent_ms.saturating_add(u64::from(h.unread_ttl) * 1000)
}

/// Inner (encrypted) plaintext = `tag u8 [| burn(9)] | body`.
pub fn encode_inner(burn: Option<&BurnHeader>, body: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(1 + BurnHeader::WIRE_LEN + body.len());
    match burn {
        None => v.push(INNER_TAG_NONE),
        Some(h) => {
            v.push(INNER_TAG_BURN);
            v.extend_from_slice(&h.encode());
        }
    }
    v.extend_from_slice(body);
    v
}

pub fn decode_inner(p: &[u8]) -> Result<(Option<BurnHeader>, &[u8])> {
    match p.first() {
        Some(&INNER_TAG_NONE) => Ok((None, &p[1..])),
        Some(&INNER_TAG_BURN) if p.len() > BurnHeader::WIRE_LEN => {
            let h = BurnHeader::decode(&p[1..1 + BurnHeader::WIRE_LEN])?;
            Ok((Some(h), &p[1 + BurnHeader::WIRE_LEN..]))
        }
        _ => Err(OmcpError::Burn("inner header")),
    }
}

/// Inner content type: UTF-8 text.
pub const INNER_CT_TEXT: u8 = 0x01;
/// Inner content type: view-once media descriptor / bytes (beta: raw bytes).
pub const INNER_CT_MEDIA: u8 = 0x02;
/// Inner content type: file offer (`transfer_id ‖ content_key ‖ FileMeta`), PROTOCOL §7.
pub const INNER_CT_FILE_META: u8 = crate::files::CONTENT_TYPE_FILE_META;
/// Inner content type: one encrypted file chunk (`transfer_id ‖ FileChunk`).
pub const INNER_CT_FILE_CHUNK: u8 = crate::files::CONTENT_TYPE_FILE_CHUNK;

/// Decoded application content inside `InnerV1` (after the optional burn header).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppContent {
    Text(String),
    Media {
        mime: String,
        bytes: Vec<u8>,
    },
    Opened(OpenedReceipt),
    /// File offer body (opaque to this layer; parsed by the file transfer code).
    FileMeta(Vec<u8>),
    /// File chunk body (opaque to this layer).
    FileChunk(Vec<u8>),
    /// Media re-fetch controls (`0x31`/`0x32`/`0x33`), never with a burn header.
    MediaControl(MediaControl),
}

/// Media re-fetch control. Wire: `type u8 | locator 16 | content_hash 32`, and for
/// `0x33` (Ask to resend, v0.1.4b) an optional trailing UTF-8 note of at most
/// [`MediaControl::MAX_NOTE`] bytes, so the request and its note are ONE message.
/// Carries nothing else beyond what the original file offer already told the partner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaControl {
    pub kind: u8,
    pub locator: [u8; 16],
    pub hash: [u8; 32],
    /// Only on `CONTROL_MEDIA_RESEND_ASK`.
    pub note: Option<String>,
}

impl MediaControl {
    pub const WIRE_LEN: usize = 49;
    pub const MAX_NOTE: usize = 280;
    pub fn new(kind: u8, locator: [u8; 16], hash: [u8; 32]) -> Self {
        Self {
            kind,
            locator,
            hash,
            note: None,
        }
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::WIRE_LEN);
        b.push(self.kind);
        b.extend_from_slice(&self.locator);
        b.extend_from_slice(&self.hash);
        if self.kind == CONTROL_MEDIA_RESEND_ASK {
            if let Some(n) = self.note.as_deref().filter(|n| !n.is_empty()) {
                b.extend_from_slice(n.as_bytes());
            }
        }
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        let ok_len = match b.first() {
            Some(&CONTROL_MEDIA_RESEND_ASK) => {
                (Self::WIRE_LEN..=Self::WIRE_LEN + Self::MAX_NOTE).contains(&b.len())
            }
            Some(&(CONTROL_MEDIA_REQUEST | CONTROL_MEDIA_UNAVAILABLE)) => b.len() == Self::WIRE_LEN,
            _ => false,
        };
        if !ok_len {
            return Err(OmcpError::Burn("media control"));
        }
        let note = if b.len() > Self::WIRE_LEN {
            Some(
                String::from_utf8(b[Self::WIRE_LEN..].to_vec())
                    .map_err(|_| OmcpError::Burn("note utf8"))?,
            )
        } else {
            None
        };
        Ok(Self {
            kind: b[0],
            locator: b[1..17].try_into().expect("len"),
            hash: b[17..49].try_into().expect("len"),
            note,
        })
    }
}

/// `InnerV1` plaintext for an application message: `tag [| burn] | inner_ct u8 | body`.
/// Media body = `mime_len u8 | mime | bytes`. `OPENED` body = the 24-byte receipt tail.
pub fn encode_app(burn: Option<&BurnHeader>, content: &AppContent) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    match content {
        AppContent::Text(t) => {
            body.push(INNER_CT_TEXT);
            body.extend_from_slice(t.as_bytes());
        }
        AppContent::Media { mime, bytes } => {
            let m = u8::try_from(mime.len()).map_err(|_| OmcpError::Burn("mime"))?;
            body.push(INNER_CT_MEDIA);
            body.push(m);
            body.extend_from_slice(mime.as_bytes());
            body.extend_from_slice(bytes);
        }
        AppContent::Opened(r) => {
            if burn.is_some() {
                return Err(OmcpError::Burn("control messages can't burn"));
            }
            body.extend_from_slice(&r.encode());
        }
        AppContent::MediaControl(c) => {
            if burn.is_some() {
                return Err(OmcpError::Burn("control messages can't burn"));
            }
            body.extend_from_slice(&c.encode());
        }
        AppContent::FileMeta(b) | AppContent::FileChunk(b) => {
            if burn.is_some() {
                return Err(OmcpError::Burn("files can't burn in the beta"));
            }
            body.push(if matches!(content, AppContent::FileMeta(_)) {
                INNER_CT_FILE_META
            } else {
                INNER_CT_FILE_CHUNK
            });
            body.extend_from_slice(b);
        }
    }
    Ok(encode_inner(burn, &body))
}

pub fn decode_app(p: &[u8]) -> Result<(Option<BurnHeader>, AppContent)> {
    let (burn, body) = decode_inner(p)?;
    let (&ct, rest) = body.split_first().ok_or(OmcpError::Burn("empty body"))?;
    let c = match ct {
        INNER_CT_TEXT => {
            AppContent::Text(String::from_utf8(rest.to_vec()).map_err(|_| OmcpError::Burn("utf8"))?)
        }
        INNER_CT_MEDIA => {
            let (&n, rest) = rest.split_first().ok_or(OmcpError::Burn("mime"))?;
            let n = n as usize;
            if rest.len() < n {
                return Err(OmcpError::Burn("mime"));
            }
            AppContent::Media {
                mime: String::from_utf8(rest[..n].to_vec()).map_err(|_| OmcpError::Burn("mime"))?,
                bytes: rest[n..].to_vec(),
            }
        }
        INNER_CT_FILE_META if burn.is_none() => AppContent::FileMeta(rest.to_vec()),
        INNER_CT_FILE_CHUNK if burn.is_none() => AppContent::FileChunk(rest.to_vec()),
        CONTROL_OPENED if burn.is_none() => AppContent::Opened(OpenedReceipt::decode(body)?),
        CONTROL_MEDIA_REQUEST | CONTROL_MEDIA_UNAVAILABLE | CONTROL_MEDIA_RESEND_ASK
            if burn.is_none() =>
        {
            AppContent::MediaControl(MediaControl::decode(body)?)
        }
        _ => return Err(OmcpError::Burn("inner content type")),
    };
    Ok((burn, c))
}

/// A media Content Key usable exactly once. [`Self::take`] hands it out and
/// leaves nothing behind; the key bytes zeroize on drop ([`ContentKey`]).
pub struct SingleUseMediaKey(Option<ContentKey>);

impl SingleUseMediaKey {
    pub fn new(k: ContentKey) -> Self {
        Self(Some(k))
    }
    pub fn take(&mut self) -> Result<ContentKey> {
        self.0
            .take()
            .ok_or(OmcpError::Burn("media key already used"))
    }
    pub fn is_spent(&self) -> bool {
        self.0.is_none()
    }
}

/// `OPENED` receipt (sent inside an encrypted control message).
/// Wire: `0x30 | msg_id(16) | opened_ms u64 BE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenedReceipt {
    pub msg_id: [u8; 16],
    pub opened_ms: u64,
}

impl OpenedReceipt {
    pub fn encode(&self) -> [u8; 25] {
        let mut b = [0u8; 25];
        b[0] = CONTROL_OPENED;
        b[1..17].copy_from_slice(&self.msg_id);
        b[17..].copy_from_slice(&self.opened_ms.to_be_bytes());
        b
    }
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() != 25 || b[0] != CONTROL_OPENED {
            return Err(OmcpError::Burn("opened receipt"));
        }
        Ok(Self {
            msg_id: b[1..17].try_into().unwrap(),
            opened_ms: u64::from_be_bytes(b[17..].try_into().unwrap()),
        })
    }
}

/// Sender side: media keys kept only until the `OPENED` receipt arrives.
#[derive(Default)]
pub struct SenderMediaKeys(HashMap<[u8; 16], ContentKey>);

impl SenderMediaKeys {
    pub fn insert(&mut self, msg_id: [u8; 16], k: ContentKey) {
        self.0.insert(msg_id, k);
    }
    /// Shred the key for this message (dropping zeroizes it). Returns true if it existed.
    pub fn on_opened(&mut self, r: &OpenedReceipt) -> bool {
        self.0.remove(&r.msg_id).is_some()
    }
    pub fn contains(&self, msg_id: &[u8; 16]) -> bool {
        self.0.contains_key(msg_id)
    }
}

/// What [`BurnStore::open`] returns.
#[derive(Debug, PartialEq, Eq)]
pub enum Opened {
    /// Plaintext + receipt to send. `burn_at_ms` = None means burn on close.
    Content {
        body: Zeroizing<Vec<u8>>,
        receipt: OpenedReceipt,
        burn_at_ms: Option<u64>,
    },
    /// Already opened or burned: show the "🔥 opened" placeholder.
    Placeholder,
}

/// Persistable copy of one live [`BurnStore`] entry (see [`crate::store`]).
/// `key` is the per-message key; dropping the snapshot zeroizes it.
#[derive(Clone, PartialEq, Eq)]
pub struct BurnSnapshot {
    pub msg_id: [u8; 16],
    pub header: BurnHeader,
    pub key: Zeroizing<[u8; 32]>,
    pub sealed: Vec<u8>,
    pub expires_at_ms: u64,
    pub burn_at_ms: Option<u64>,
    pub opened: bool,
}

impl std::fmt::Debug for BurnSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BurnSnapshot")
            .field("msg_id", &self.msg_id)
            .field("opened", &self.opened)
            .finish_non_exhaustive()
    }
}

struct Entry {
    header: BurnHeader,
    key: Zeroizing<[u8; 32]>,
    sealed: Vec<u8>,
    expires_at_ms: u64,
    burn_at_ms: Option<u64>,
    opened: bool,
}

/// Receiver-side store. Plaintext is never held at rest: each entry is sealed
/// under its own random key, and burning zeroizes the key and the ciphertext.
#[derive(Default)]
pub struct BurnStore {
    live: HashMap<[u8; 16], Entry>,
    burned: std::collections::HashSet<[u8; 16]>,
}

fn nonce0() -> Nonce {
    Nonce::from([0u8; 12]) // each key is used for exactly one encryption
}

impl BurnStore {
    pub fn insert(
        &mut self,
        msg_id: [u8; 16],
        header: BurnHeader,
        body: &[u8],
        sent_ms: u64,
    ) -> Result<()> {
        let mut key = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(key.as_mut()).map_err(|_| OmcpError::Crypto("getrandom"))?;
        let c = ChaCha20Poly1305::new_from_slice(key.as_ref())
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let sealed = c
            .encrypt(&nonce0(), body)
            .map_err(|_| OmcpError::Crypto("burn seal"))?;
        self.live.insert(
            msg_id,
            Entry {
                expires_at_ms: expires_at_ms(sent_ms, &header),
                header,
                key,
                sealed,
                burn_at_ms: None,
                opened: false,
            },
        );
        Ok(())
    }

    /// Open once. A second open (or after burn/expiry) returns [`Opened::Placeholder`].
    pub fn open(&mut self, msg_id: &[u8; 16], now_ms: u64) -> Result<Opened> {
        self.tick(now_ms);
        let Some(e) = self.live.get_mut(msg_id) else {
            return Ok(Opened::Placeholder);
        };
        if e.opened {
            return Ok(Opened::Placeholder);
        }
        let c = ChaCha20Poly1305::new_from_slice(e.key.as_ref())
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let body = Zeroizing::new(
            c.decrypt(&nonce0(), e.sealed.as_slice())
                .map_err(|_| OmcpError::Crypto("burn open"))?,
        );
        e.opened = true;
        let burn_at_ms = match e.header.mode {
            BurnMode::OnOpen => None,
            BurnMode::Timer => Some(now_ms.saturating_add(u64::from(e.header.secs) * 1000)),
        };
        e.burn_at_ms = burn_at_ms;
        if burn_at_ms.is_none() {
            // View once: the store copy is shredded right away; only the caller's
            // in-memory plaintext remains until the viewer closes.
            self.shred(msg_id);
        }
        Ok(Opened::Content {
            body,
            receipt: OpenedReceipt {
                msg_id: *msg_id,
                opened_ms: now_ms,
            },
            burn_at_ms,
        })
    }

    /// Sender copy: the peer sent `OPENED`. View-once (or any media) is shredded
    /// at once; a timer copy burns `secs` after `opened_ms`. Returns true if shredded now.
    pub fn on_remote_opened(&mut self, msg_id: &[u8; 16], opened_ms: u64, media: bool) -> bool {
        let Some(e) = self.live.get_mut(msg_id) else {
            return false;
        };
        if media || e.header.mode == BurnMode::OnOpen {
            self.shred(msg_id);
            return true;
        }
        e.opened = true;
        e.burn_at_ms = Some(opened_ms.saturating_add(u64::from(e.header.secs) * 1000));
        false
    }

    /// Burn deadline once opened (timer mode), if any.
    pub fn burn_at(&self, msg_id: &[u8; 16]) -> Option<u64> {
        self.live.get(msg_id).and_then(|e| e.burn_at_ms)
    }

    /// Viewer closed: burn now (any mode).
    pub fn close(&mut self, msg_id: &[u8; 16]) {
        if self.live.get(msg_id).is_some_and(|e| e.opened) {
            self.shred(msg_id);
        }
    }

    /// Burn timers that ran out and unread messages past `expires_at`. Returns burned ids.
    pub fn tick(&mut self, now_ms: u64) -> Vec<[u8; 16]> {
        let due: Vec<[u8; 16]> = self
            .live
            .iter()
            .filter(|(_, e)| {
                e.burn_at_ms.is_some_and(|t| now_ms >= t)
                    || (!e.opened && now_ms >= e.expires_at_ms)
            })
            .map(|(k, _)| *k)
            .collect();
        for id in &due {
            self.shred(id);
        }
        due
    }

    fn shred(&mut self, msg_id: &[u8; 16]) {
        if let Some(mut e) = self.live.remove(msg_id) {
            e.key.zeroize();
            e.sealed.zeroize();
            self.burned.insert(*msg_id);
        }
    }

    /// Copy of a live entry for the encrypted local store (None once burned).
    pub fn snapshot(&self, msg_id: &[u8; 16]) -> Option<BurnSnapshot> {
        self.live.get(msg_id).map(|e| BurnSnapshot {
            msg_id: *msg_id,
            header: e.header,
            key: e.key.clone(),
            sealed: e.sealed.clone(),
            expires_at_ms: e.expires_at_ms,
            burn_at_ms: e.burn_at_ms,
            opened: e.opened,
        })
    }

    /// Re-insert an entry loaded from the local store (after an app restart).
    pub fn restore(&mut self, s: BurnSnapshot) {
        self.burned.remove(&s.msg_id);
        self.live.insert(
            s.msg_id,
            Entry {
                header: s.header,
                key: s.key,
                sealed: s.sealed,
                expires_at_ms: s.expires_at_ms,
                burn_at_ms: s.burn_at_ms,
                opened: s.opened,
            },
        );
    }

    /// Ids of all live entries.
    pub fn live_ids(&self) -> Vec<[u8; 16]> {
        self.live.keys().copied().collect()
    }

    pub fn is_burned(&self, msg_id: &[u8; 16]) -> bool {
        self.burned.contains(msg_id)
    }
    pub fn is_live(&self, msg_id: &[u8; 16]) -> bool {
        self.live.contains_key(msg_id)
    }
    pub fn len(&self) -> usize {
        self.live.len()
    }
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::generate_content_key;
    use crate::gateway::store::StoreAndForward;
    use crate::gateway::OpaqueEnvelope;

    const ID: [u8; 16] = [7; 16];

    #[test]
    fn header_roundtrip_and_validation() {
        let h = BurnHeader::timer(10, UNREAD_TTL_7D).unwrap();
        assert_eq!(BurnHeader::decode(&h.encode()).unwrap(), h);
        assert!(BurnHeader::timer(11, UNREAD_TTL_7D).is_err());
        assert!(BurnHeader::on_open(1234).is_err());
        assert!(BurnHeader::new(BurnMode::OnOpen, 5, UNREAD_TTL_1H).is_err());
        assert!(BurnHeader::decode(&[9, 0, 0, 0, 0, 0, 0, 14, 16]).is_err());
    }

    #[test]
    fn inner_header_roundtrip() {
        let h = BurnHeader::on_open(UNREAD_TTL_24H).unwrap();
        let p = encode_inner(Some(&h), b"hi");
        assert_eq!(decode_inner(&p).unwrap(), (Some(h), &b"hi"[..]));
        let p = encode_inner(None, b"x");
        assert_eq!(decode_inner(&p).unwrap(), (None, &b"x"[..]));
        assert!(decode_inner(&[2, 1]).is_err());
    }

    #[test]
    fn b1_timer_burn_shreds() {
        let mut s = BurnStore::default();
        let h = BurnHeader::timer(10, UNREAD_TTL_7D).unwrap();
        s.insert(ID, h, b"secret text", 0).unwrap();
        let Opened::Content {
            body, burn_at_ms, ..
        } = s.open(&ID, 1_000).unwrap()
        else {
            panic!()
        };
        assert_eq!(&body[..], b"secret text");
        assert_eq!(burn_at_ms, Some(11_000));
        assert!(s.tick(10_999).is_empty());
        assert_eq!(s.tick(11_000), vec![ID]);
        assert!(s.is_burned(&ID) && !s.is_live(&ID));
        assert_eq!(s.open(&ID, 12_000).unwrap(), Opened::Placeholder);
    }

    #[test]
    fn b2_view_once_opens_exactly_once() {
        let mut s = BurnStore::default();
        s.insert(ID, BurnHeader::on_open(UNREAD_TTL_7D).unwrap(), b"jpeg", 0)
            .unwrap();
        assert!(matches!(
            s.open(&ID, 5).unwrap(),
            Opened::Content {
                burn_at_ms: None,
                ..
            }
        ));
        assert!(s.is_empty());
        assert_eq!(s.open(&ID, 6).unwrap(), Opened::Placeholder);
    }

    #[test]
    fn b4_unread_expiry_receiver_and_relay() {
        let h = BurnHeader::on_open(UNREAD_TTL_1H).unwrap();
        let mut s = BurnStore::default();
        s.insert(ID, h, b"never read", 1_000).unwrap();
        let exp = expires_at_ms(1_000, &h);
        assert_eq!(exp, 3_601_000);
        assert!(s.tick(exp - 1).is_empty());
        assert_eq!(s.tick(exp), vec![ID]);

        let mut relay = StoreAndForward::default();
        let env = OpaqueEnvelope {
            dest_label: [1; 32],
            suite_id: 1,
            expiry_ms: exp,
            ciphertext: vec![0xAA; 8],
        };
        relay.retain(env.clone(), 1_000);
        assert_eq!(relay.have(&[1; 32]), 1);
        relay.purge(exp);
        assert_eq!(relay.have(&[1; 32]), 0);
        relay.retain(env, exp); // already expired: not stored
        assert!(relay.is_empty());
    }

    #[test]
    fn single_use_media_key_and_opened_receipt() {
        let k = generate_content_key().unwrap();
        let mut sender = SenderMediaKeys::default();
        sender.insert(ID, k.clone());
        let mut rk = SingleUseMediaKey::new(k);
        assert!(rk.take().is_ok());
        assert!(rk.is_spent() && rk.take().is_err());

        let r = OpenedReceipt {
            msg_id: ID,
            opened_ms: 42,
        };
        let r2 = OpenedReceipt::decode(&r.encode()).unwrap();
        assert_eq!(r, r2);
        assert!(sender.on_opened(&r2));
        assert!(!sender.contains(&ID));
        assert!(!sender.on_opened(&r2));
        assert!(OpenedReceipt::decode(&[0; 25]).is_err());
    }

    #[test]
    fn app_content_roundtrip() {
        let h = BurnHeader::timer(60, UNREAD_TTL_24H).unwrap();
        for (b, c) in [
            (Some(h), AppContent::Text("hé".into())),
            (None, AppContent::Text(String::new())),
            (
                Some(BurnHeader::on_open(UNREAD_TTL_7D).unwrap()),
                AppContent::Media {
                    mime: "image/jpeg".into(),
                    bytes: vec![1, 2, 3],
                },
            ),
            (
                None,
                AppContent::Opened(OpenedReceipt {
                    msg_id: ID,
                    opened_ms: 9,
                }),
            ),
        ] {
            let p = encode_app(b.as_ref(), &c).unwrap();
            assert_eq!(decode_app(&p).unwrap(), (b, c));
        }
        let r = AppContent::Opened(OpenedReceipt {
            msg_id: ID,
            opened_ms: 1,
        });
        assert!(encode_app(Some(&h), &r).is_err());
        for c in [
            AppContent::FileMeta(vec![7; 40]),
            AppContent::FileChunk(vec![]),
        ] {
            let p = encode_app(None, &c).unwrap();
            assert_eq!(decode_app(&p).unwrap(), (None, c.clone()));
            assert!(encode_app(Some(&h), &c).is_err());
        }
    }

    #[test]
    fn sender_copy_on_remote_opened() {
        let mut s = BurnStore::default();
        s.insert(ID, BurnHeader::timer(10, UNREAD_TTL_7D).unwrap(), b"t", 0)
            .unwrap();
        assert!(!s.on_remote_opened(&ID, 5_000, false));
        assert_eq!(s.burn_at(&ID), Some(15_000));
        assert_eq!(s.tick(15_000), vec![ID]);
        s.insert(ID, BurnHeader::on_open(UNREAD_TTL_7D).unwrap(), b"t", 0)
            .unwrap();
        assert!(s.on_remote_opened(&ID, 1, false));
        assert!(s.is_empty());
    }

    #[test]
    fn close_burns_timer_message_early() {
        let mut s = BurnStore::default();
        s.insert(
            ID,
            BurnHeader::timer(3_600, UNREAD_TTL_7D).unwrap(),
            b"t",
            0,
        )
        .unwrap();
        s.close(&ID); // unopened close is a no-op
        assert!(s.is_live(&ID));
        let _ = s.open(&ID, 0).unwrap();
        s.close(&ID);
        assert!(s.is_burned(&ID));
    }
}
