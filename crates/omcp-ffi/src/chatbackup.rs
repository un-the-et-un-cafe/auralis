//! Encrypted chat backup, missing-media re-fetch and the restore indicator
//! (REQUIREMENTS CB1–CB5, RS1–RS2). Core logic lives in
//! [`omcp_core::chat_backup`] and [`omcp_core::device`]; this module wires it to
//! the node, its store and its mesh links.
//!
//! Platform flow (Drive `appDataFolder` / iCloud private database):
//! 1. When `chat_backup_due` → `chat_backup_next` → upload `blob` under `name`
//!    → `chat_backup_commit(seq)`; after a snapshot, delete blobs whose `seq`
//!    is below `delete_below_seq`.
//! 2. Media pass (CB5): `media_backup_due` → `media_backup_todo` → read each
//!    file → `seal_media_backup` → upload → `media_backup_commit`.
//! 3. Restore: download all `hopla-chat-*` blobs → `chat_backup_restore`; with
//!    media backup, `media_placeholders` → download `blob_name` →
//!    `restore_media_from_backup` → save → `set_received_file_path`. Anything
//!    left stays a placeholder and is re-requested from partners on the next link.

use omcp_core::burn::{
    AppContent, MediaControl, CONTROL_MEDIA_REQUEST, CONTROL_MEDIA_RESEND_ASK,
    CONTROL_MEDIA_UNAVAILABLE,
};
use omcp_core::chat_backup as cb;
use omcp_core::device::{self as core_device, OwnDevice};
use omcp_core::store::{self as st, MessageRec, Store};
use zeroize::Zeroizing;

use crate::{
    files, invalid, now_ms, parse_id, parse_msg_id, random_id16, FfiError, FileFailure, LinkOutput,
    NodeState, OmcpNode, Queued, StoredFileState,
};

const META_DEVICE: &[u8] = b"own_device";
const META_RESTORED_AT: &[u8] = b"own_restored_at";

/// An encrypted, padded blob to upload.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChatBackupBlob {
    /// Provider file / record name.
    pub name: String,
    /// 0 = snapshot, 1 = log, 2 = media.
    pub kind: u8,
    pub seq: u64,
    pub blob: Vec<u8>,
    /// After this snapshot uploads, blobs with a lower `seq` may be deleted.
    pub delete_below_seq: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChatBackupSummary {
    pub messages: u32,
    pub media_placeholders: u32,
}

/// Settings > Backup > "Also back up photos, videos and files" (CB5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct MediaBackupSettings {
    pub enabled: bool,
    pub photos: bool,
    pub videos: bool,
    pub voice: bool,
    pub files: bool,
    pub mobile_data: bool,
}

impl From<cb::MediaSettings> for MediaBackupSettings {
    fn from(s: cb::MediaSettings) -> Self {
        Self {
            enabled: s.enabled,
            photos: s.photos,
            videos: s.videos,
            voice: s.voice,
            files: s.files,
            mobile_data: s.mobile_data,
        }
    }
}

impl From<MediaBackupSettings> for cb::MediaSettings {
    fn from(s: MediaBackupSettings) -> Self {
        Self {
            enabled: s.enabled,
            photos: s.photos,
            videos: s.videos,
            voice: s.voice,
            files: s.files,
            mobile_data: s.mobile_data,
        }
    }
}

/// A media item for the media backup or a placeholder to fill from it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaBackupItem {
    pub msg_id_hex: String,
    pub hash_hex: String,
    pub mime: String,
    /// Where the app keeps the file (upload); None for placeholders.
    pub local_path: Option<String>,
    /// Name of the media blob in the backup.
    pub blob_name: String,
}

/// A partner asks for a media item again (or asks us to resend it).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaRequestEvent {
    pub contact_id: String,
    pub msg_id_hex: String,
    pub hash_hex: String,
    pub name: String,
    pub mime: String,
    /// Our copy, if the app saved one.
    pub local_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MediaUpdate {
    pub contact_id: String,
    pub msg_id_hex: String,
    pub state: StoredFileState,
}

/// RS1: "<name> restored Hopla on a new device · <date>".
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RestoreNotice {
    pub contact_id: String,
    /// From the contact's device certificate; None = new device without a restore stamp.
    pub restored_at_ms: Option<u64>,
    /// Id of the system message inserted in the chat.
    pub msg_id_hex: String,
}

/// This installation's device key.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DeviceInfo {
    pub device_key_hex: String,
    pub fingerprint: String,
    /// Set after `mark_restored`: show "You restored this account…" in chats.
    pub restored_at_ms: Option<u64>,
}

pub(crate) fn load_or_create_device(
    db: &Store,
    seed: &[u8; 32],
    peer_id: [u8; 32],
    now: u64,
) -> Result<OwnDevice, FfiError> {
    if let Some(d) = db.get_meta::<OwnDevice>(META_DEVICE)? {
        if d.cert().is_ok_and(|c| c.peer_id == peer_id) {
            return Ok(d);
        }
    }
    let d = OwnDevice::generate(seed, peer_id, now, None)?;
    db.put_meta(META_DEVICE, &d)?;
    Ok(d)
}

fn hash32(h: &str) -> Result<[u8; 32], FfiError> {
    hex::decode(h)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| invalid("hash hex"))
}

fn file_hash(m: &MessageRec) -> Option<[u8; 32]> {
    m.file.as_ref()?.hash.as_deref()?.try_into().ok()
}

impl NodeState {
    fn queue_control(&mut self, peer: &[u8; 32], c: AppContent) -> Result<(), FfiError> {
        let q = Queued {
            envelope_id: random_id16()?,
            created_ms: now_ms(),
            inner: Zeroizing::new(omcp_core::burn::encode_app(None, &c)?),
            control: true,
        };
        self.persist_queued(peer, &q)?;
        self.queues.entry(*peer).or_default().push_back(q);
        Ok(())
    }

    fn set_file_state(&self, id: &[u8; 16], state: u8) -> Result<(), FfiError> {
        self.update_message(id, |m| {
            if let Some(f) = m.file.as_mut() {
                f.state = state;
            }
        })
    }

    /// RS1/RS2: apply the peer's device certificate from the handshake.
    pub(crate) fn apply_peer_cert(
        &mut self,
        peer: [u8; 32],
        payload: &[u8],
        now: u64,
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let cert = core_device::parse_handshake_payload(payload)?;
        let Some(c) = self.contacts.get_mut(&peer) else {
            return Ok(());
        };
        let before = c.device.clone();
        let ev = core_device::on_peer_cert(&mut c.device, &peer, cert.as_ref(), now)?;
        if ev.is_notice() {
            c.verified = false;
            let restored_at_ms = c.device.restored_at_ms;
            let id = random_id16()?;
            self.db.put_message(&MessageRec {
                id,
                peer,
                mine: false,
                time_ms: now,
                status: st::STATUS_SYSTEM,
                text: format!("restored:{}", restored_at_ms.unwrap_or(0)),
                mime: None,
                burn: None,
                burn_state: st::BURN_NONE,
                burn_at_ms: None,
                file: None,
            })?;
            out.restore_notices.push(RestoreNotice {
                contact_id: hex::encode(peer),
                restored_at_ms,
                msg_id_hex: hex::encode(id),
            });
        }
        if self.contacts.get(&peer).map(|c| &c.device) != Some(&before) || ev.is_notice() {
            self.persist_contact(&peer)?;
        }
        Ok(())
    }

    /// CB3: queue `MEDIA_REQUEST` for every placeholder in the chat with `peer`.
    pub(crate) fn request_missing_media(
        &mut self,
        peer: &[u8; 32],
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        for m in self.db.messages(peer)? {
            let Some(f) = m.file.as_ref() else { continue };
            if f.state != st::FILE_MISSING {
                continue;
            }
            let (Some(hash), Some(loc)) = (
                file_hash(&m),
                f.locator
                    .as_deref()
                    .and_then(|l| <[u8; 16]>::try_from(l).ok()),
            ) else {
                continue;
            };
            self.queue_control(
                peer,
                AppContent::MediaControl(MediaControl {
                    kind: CONTROL_MEDIA_REQUEST,
                    locator: loc,
                    hash,
                }),
            )?;
            self.set_file_state(&m.id, st::FILE_REQUESTED)?;
            out.media_updates.push(MediaUpdate {
                contact_id: hex::encode(peer),
                msg_id_hex: hex::encode(m.id),
                state: StoredFileState::Requested,
            });
        }
        Ok(())
    }

    fn find_media(
        &self,
        peer: &[u8; 32],
        c: &MediaControl,
    ) -> Result<Option<MessageRec>, FfiError> {
        Ok(self
            .db
            .message(&c.locator)?
            .filter(|m| m.peer == *peer && m.burn.is_none() && file_hash(m) == Some(c.hash)))
    }

    pub(crate) fn on_media_control(
        &mut self,
        peer: [u8; 32],
        c: MediaControl,
        out: &mut LinkOutput,
    ) -> Result<(), FfiError> {
        let m = self.find_media(&peer, &c)?;
        let event = |m: &MessageRec| {
            let f = m.file.as_ref().expect("file");
            MediaRequestEvent {
                contact_id: hex::encode(peer),
                msg_id_hex: hex::encode(m.id),
                hash_hex: hex::encode(c.hash),
                name: f.name.clone(),
                mime: f.mime.clone(),
                local_path: f.local_path.clone(),
            }
        };
        match c.kind {
            CONTROL_MEDIA_REQUEST => match m {
                // Only items we actually hold; placeholders can't serve.
                Some(m)
                    if m.file.as_ref().is_some_and(|f| {
                        !matches!(
                            f.state,
                            st::FILE_MISSING | st::FILE_REQUESTED | st::FILE_UNAVAILABLE
                        )
                    }) =>
                {
                    out.media_requests.push(event(&m))
                }
                _ => self.queue_control(
                    &peer,
                    AppContent::MediaControl(MediaControl {
                        kind: CONTROL_MEDIA_UNAVAILABLE,
                        ..c
                    }),
                )?,
            },
            CONTROL_MEDIA_UNAVAILABLE => {
                if let Some(m) = m.filter(|m| {
                    m.file
                        .as_ref()
                        .is_some_and(|f| matches!(f.state, st::FILE_MISSING | st::FILE_REQUESTED))
                }) {
                    self.set_file_state(&m.id, st::FILE_UNAVAILABLE)?;
                    out.media_updates.push(MediaUpdate {
                        contact_id: hex::encode(peer),
                        msg_id_hex: hex::encode(m.id),
                        state: StoredFileState::Unavailable,
                    });
                }
            }
            CONTROL_MEDIA_RESEND_ASK => {
                if let Some(m) = m {
                    out.resend_asks.push(event(&m));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// A re-fetched file must match the placeholder's hash before it replaces it.
    pub(crate) fn check_refetched(&self, out: &mut LinkOutput, from: usize) {
        let mut i = from;
        while i < out.files.len() {
            let r = &out.files[i];
            let expected = parse_msg_id(&r.transfer_id_hex)
                .ok()
                .and_then(|id| self.db.message(&id).ok().flatten())
                .and_then(|m| file_hash(&m));
            match expected {
                Some(h) if blake3::hash(&r.bytes).as_bytes() != &h => {
                    let r = out.files.remove(i);
                    out.file_failures.push(FileFailure {
                        transfer_id_hex: r.transfer_id_hex,
                        contact_id: r.contact_id,
                        reason: "integrity".into(),
                    });
                }
                _ => i += 1,
            }
        }
    }
}

fn blob(u: cb::Upload) -> ChatBackupBlob {
    ChatBackupBlob {
        name: u.name,
        kind: u.kind,
        seq: u.seq,
        blob: u.blob,
        delete_below_seq: u.delete_below_seq,
    }
}

impl OmcpNode {
    fn backup_key(&self) -> Result<Zeroizing<[u8; 32]>, FfiError> {
        Ok(cb::chat_backup_key(&self.inner.seed)?)
    }
}

#[uniffi::export]
impl OmcpNode {
    // ---- restore indicator (RS1 / RS2) ----

    /// After restoring this identity on a new install (24 words or cloud
    /// backup): mint a new device key stamped `restored_at` (RS1). Contacts see
    /// the notice on their next link and must re-verify us (RS2). Our own
    /// verifications of others are kept (O3, design §5.98 "Still verified on
    /// your side"): their keys didn't change.
    pub fn mark_restored(&self, now_ms: u64) -> Result<DeviceInfo, FfiError> {
        let d = OwnDevice::generate(&self.inner.seed, self.inner.peer_id, now_ms, Some(now_ms))?;
        let st = self.inner.lock();
        st.db().put_meta(META_DEVICE, &d)?;
        st.db().put_meta(META_RESTORED_AT, &now_ms)?;
        *self.inner.device() = d;
        drop(st);
        self.device_info()
    }

    pub fn device_info(&self) -> Result<DeviceInfo, FfiError> {
        let key = self.inner.device_public();
        let restored_at_ms = self.inner.lock().db().get_meta::<u64>(META_RESTORED_AT)?;
        Ok(DeviceInfo {
            device_key_hex: hex::encode(key),
            fingerprint: core_device::device_fingerprint(&key),
            restored_at_ms,
        })
    }

    /// Verify a contact by scanning their QR in person (RS2). Fails if the code
    /// doesn't carry their current device key (e.g. a QR from before a restore).
    pub fn verify_contact_from_qr(&self, payload: String) -> Result<crate::ContactInfo, FfiError> {
        let (id, _name, qr_dev) = self.parse_qr(&payload)?;
        let mut st = self.inner.lock();
        let c = st.contacts.get_mut(&id).ok_or(FfiError::UnknownContact)?;
        let ok = match (c.device.current_key(), qr_dev) {
            (Some(cur), Some(q)) => cur == q,
            (None, Some(q)) => !c.device.is_superseded(&q) && c.device.superseded.is_empty(),
            (None, None) => c.device.superseded.is_empty(),
            (Some(_), None) => false,
        };
        if !ok {
            return Err(FfiError::VerificationFailed);
        }
        if c.device.current.is_none() {
            c.device.current = qr_dev.map(|d| d.to_vec());
        }
        c.verified = true;
        c.device.restored_unverified = false;
        let info = crate::contact_info(&self.inner, &id, c);
        st.persist_contact(&id)?;
        Ok(info)
    }

    // ---- text backup (CB1, CB2, CB4) ----

    pub fn chat_backup_due(
        &self,
        now_ms: u64,
        on_wifi: bool,
        allow_mobile: bool,
    ) -> Result<bool, FfiError> {
        let st = self.inner.lock();
        Ok(cb::text_backup_due(
            &cb::load_state(st.db())?,
            now_ms,
            on_wifi,
            allow_mobile,
        ))
    }

    /// Next encrypted upload, or None when nothing changed. Call
    /// [`Self::chat_backup_commit`] once it uploaded.
    pub fn chat_backup_next(
        &self,
        now_ms: u64,
        force_snapshot: bool,
    ) -> Result<Option<ChatBackupBlob>, FfiError> {
        let key = self.backup_key()?;
        let mut st = self.inner.lock();
        let prev = cb::load_state(st.db())?;
        let mut names: Vec<cb::BackupName> = st
            .contacts
            .iter()
            .map(|(id, c)| cb::BackupName {
                peer: id.to_vec(),
                name: c.name.clone(),
            })
            .collect();
        names.sort_by(|a, b| a.peer.cmp(&b.peer));
        let Some((u, next)) = cb::plan(st.db(), &prev, &names, now_ms, force_snapshot)? else {
            return Ok(None);
        };
        st.pending_backup = Some(next);
        Ok(Some(blob(cb::seal_upload(&key, u)?)))
    }

    pub fn chat_backup_commit(&self, seq: u64) -> Result<(), FfiError> {
        let mut st = self.inner.lock();
        match st.pending_backup.take() {
            Some(next) if next.seq == seq => Ok(cb::save_state(st.db(), &next)?),
            other => {
                st.pending_backup = other;
                Err(FfiError::State {
                    msg: "no pending chat backup with this seq".into(),
                })
            }
        }
    }

    /// Restore message text from every downloaded chat-backup blob (any order).
    pub fn chat_backup_restore(&self, blobs: Vec<Vec<u8>>) -> Result<ChatBackupSummary, FfiError> {
        let key = self.backup_key()?;
        let mut st = self.inner.lock();
        let r = cb::restore(&key, &blobs, st.db())?;
        for n in &r.names {
            if let Ok(id) = <[u8; 32]>::try_from(n.peer.as_slice()) {
                if let Some(c) = st.contacts.get_mut(&id) {
                    if c.name.is_empty() {
                        c.name = n.name.clone();
                    }
                }
            }
        }
        let mut state = r.state.clone();
        state.media_uploaded = cb::load_state(st.db())?.media_uploaded;
        cb::save_state(st.db(), &state)?;
        st.load(now_ms())?;
        Ok(ChatBackupSummary {
            messages: r.messages as u32,
            media_placeholders: r.media_placeholders as u32,
        })
    }

    // ---- media backup (CB5) ----

    pub fn media_backup_settings(&self) -> Result<MediaBackupSettings, FfiError> {
        Ok(cb::load_media_settings(self.inner.lock().db())?.into())
    }

    /// Save the settings. Turning media backup off returns the media blob names
    /// to delete from the provider (call only after the user confirmed).
    pub fn set_media_backup_settings(
        &self,
        s: MediaBackupSettings,
    ) -> Result<Vec<String>, FfiError> {
        let key = self.backup_key()?;
        let st = self.inner.lock();
        cb::save_media_settings(st.db(), &s.into())?;
        if s.enabled {
            return Ok(vec![]);
        }
        let mut state = cb::load_state(st.db())?;
        let names = std::mem::take(&mut state.media_uploaded)
            .into_iter()
            .filter_map(|h| <[u8; 32]>::try_from(h.as_slice()).ok())
            .map(|h| cb::media_blob_name(&key, &h))
            .collect();
        cb::save_state(st.db(), &state)?;
        Ok(names)
    }

    pub fn media_backup_due(&self, now_ms: u64, on_wifi: bool) -> Result<bool, FfiError> {
        let st = self.inner.lock();
        Ok(cb::media_backup_due(
            &cb::load_media_settings(st.db())?,
            &cb::load_state(st.db())?,
            now_ms,
            on_wifi,
        ))
    }

    /// Media items to read and upload in this pass (none when media backup is off).
    pub fn media_backup_todo(&self) -> Result<Vec<MediaBackupItem>, FfiError> {
        let key = self.backup_key()?;
        let st = self.inner.lock();
        let s = cb::load_media_settings(st.db())?;
        Ok(cb::plan_media(st.db(), &s, &cb::load_state(st.db())?)?
            .into_iter()
            .map(|t| MediaBackupItem {
                msg_id_hex: hex::encode(t.msg_id),
                hash_hex: hex::encode(t.hash),
                mime: t.mime,
                local_path: Some(t.local_path),
                blob_name: cb::media_blob_name(&key, &t.hash),
            })
            .collect())
    }

    /// Encrypt one media file for upload; fails if `bytes` don't match `hash_hex`.
    pub fn seal_media_backup(
        &self,
        hash_hex: String,
        bytes: Vec<u8>,
    ) -> Result<ChatBackupBlob, FfiError> {
        let key = self.backup_key()?;
        if !cb::load_media_settings(self.inner.lock().db())?.enabled {
            return Err(FfiError::State {
                msg: "media backup is off".into(),
            });
        }
        Ok(blob(cb::seal_media(&key, &hash32(&hash_hex)?, &bytes)?))
    }

    pub fn media_backup_commit(&self, hash_hex: String, now_ms: u64) -> Result<(), FfiError> {
        let h = hash32(&hash_hex)?;
        let st = self.inner.lock();
        let mut state = cb::load_state(st.db())?;
        state.media_uploaded.insert(h.to_vec());
        state.last_media_ms = now_ms;
        Ok(cb::save_state(st.db(), &state)?)
    }

    /// Restored placeholders that may be in the media backup.
    pub fn media_placeholders(&self) -> Result<Vec<MediaBackupItem>, FfiError> {
        let key = self.backup_key()?;
        let st = self.inner.lock();
        Ok(st
            .db()
            .all_messages()?
            .into_iter()
            .filter_map(|m| {
                let f = m.file.as_ref()?;
                if !matches!(
                    f.state,
                    st::FILE_MISSING | st::FILE_REQUESTED | st::FILE_UNAVAILABLE
                ) {
                    return None;
                }
                let h = file_hash(&m)?;
                Some(MediaBackupItem {
                    msg_id_hex: hex::encode(m.id),
                    hash_hex: hex::encode(h),
                    mime: f.mime.clone(),
                    local_path: None,
                    blob_name: cb::media_blob_name(&key, &h),
                })
            })
            .collect())
    }

    /// Decrypt a media blob for a placeholder and verify its hash. The app saves
    /// the bytes and calls `set_received_file_path`.
    pub fn restore_media_from_backup(
        &self,
        msg_id_hex: String,
        blob: Vec<u8>,
    ) -> Result<Vec<u8>, FfiError> {
        let key = self.backup_key()?;
        let id = parse_msg_id(&msg_id_hex)?;
        let m = self
            .inner
            .lock()
            .db()
            .message(&id)?
            .ok_or_else(|| invalid("unknown message"))?;
        let h = file_hash(&m).ok_or_else(|| invalid("no media hash"))?;
        Ok(cb::open_media(&key, &h, &blob)?.to_vec())
    }

    // ---- missing-media re-fetch (CB3) ----

    /// Tap on a placeholder: ask the partner again (sent on the next flush/link).
    pub fn request_media(&self, msg_id_hex: String) -> Result<(), FfiError> {
        let id = parse_msg_id(&msg_id_hex)?;
        let mut st = self.inner.lock();
        let m = st
            .db()
            .message(&id)?
            .ok_or_else(|| invalid("unknown message"))?;
        if m.file
            .as_ref()
            .is_some_and(|f| f.state == st::FILE_REQUESTED)
        {
            st.set_file_state(&id, st::FILE_MISSING)?;
        }
        let peer = m.peer;
        let mut out = LinkOutput::default();
        st.request_missing_media(&peer, &mut out)
    }

    /// "Ask to resend" (after the user confirmed): the partner sees a prompt.
    pub fn ask_to_resend(&self, msg_id_hex: String) -> Result<(), FfiError> {
        let id = parse_msg_id(&msg_id_hex)?;
        let mut st = self.inner.lock();
        let m = st
            .db()
            .message(&id)?
            .ok_or_else(|| invalid("unknown message"))?;
        let hash = file_hash(&m).ok_or_else(|| invalid("no media hash"))?;
        st.queue_control(
            &m.peer,
            AppContent::MediaControl(MediaControl {
                kind: CONTROL_MEDIA_RESEND_ASK,
                locator: id,
                hash,
            }),
        )
    }

    /// Answer a [`MediaRequestEvent`]: `bytes` = our copy (re-sent as a file
    /// transfer under the same locator), or None = "no longer have it".
    pub fn answer_media_request(
        &self,
        contact_id: String,
        msg_id_hex: String,
        bytes: Option<Vec<u8>>,
    ) -> Result<(), FfiError> {
        let peer = parse_id(&contact_id)?;
        let id = parse_msg_id(&msg_id_hex)?;
        let mut st = self.inner.lock();
        let m = st
            .db()
            .message(&id)?
            .filter(|m| m.peer == peer && m.burn.is_none())
            .ok_or_else(|| invalid("unknown message"))?;
        let f = m.file.clone().ok_or_else(|| invalid("not a file"))?;
        let hash = file_hash(&m).ok_or_else(|| invalid("no media hash"))?;
        match bytes.filter(|b| blake3::hash(b).as_bytes() == &hash) {
            Some(b) => {
                let out = files::build_outgoing(id, &f.name, &f.mime, &b)?;
                st.persist_out_file(&peer, &out)?;
                st.files.out.entry(peer).or_default().push_back(out);
                Ok(())
            }
            None => st.queue_control(
                &peer,
                AppContent::MediaControl(MediaControl {
                    kind: CONTROL_MEDIA_UNAVAILABLE,
                    locator: id,
                    hash,
                }),
            ),
        }
    }
}
