//! Persistence glue: every state change on [`crate::OmcpNode`] is written
//! through to the encrypted [`omcp_core::store::Store`], and a node opened with
//! [`crate::OmcpNode::open`] reloads contacts, queue, burn state and outgoing
//! transfers from it. The storage key comes from the platform keystore.

use omcp_core::burn::BurnHeader;
use omcp_core::store::{
    self as st, BurnDir, ContactRec, FileRec, MessageRec, OutFileRec, OutboxRec, Store,
};
use zeroize::Zeroizing;

use crate::files::OutgoingFile;
use crate::{BurnMeta, BurnSpec, Contact, FfiError, LinkOutput, NodeState, Queued};

/// An incoming transfer may fill a same-peer non-burn file record, but never
/// replace an immutable completed file or active outgoing transfer. Restored
/// own-sent media is also re-fetchable from its hash-bound placeholder.
pub(crate) fn accepts_incoming_file(m: &MessageRec, peer: &[u8; 32]) -> bool {
    let Some(f) = m.file.as_ref() else {
        return false;
    };
    m.peer == *peer
        && m.burn.is_none()
        && matches!(
            f.state,
            st::FILE_RECEIVING
                | st::FILE_PAUSED
                | st::FILE_MISSING
                | st::FILE_REQUESTED
                | st::FILE_UNAVAILABLE
        )
        && (!m.mine
            || (f.hash.as_ref().is_some_and(|h| h.len() == 32)
                && matches!(
                    f.state,
                    st::FILE_MISSING
                        | st::FILE_REQUESTED
                        | st::FILE_UNAVAILABLE
                        | st::FILE_RECEIVING
                        | st::FILE_PAUSED
                )))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MessageStatus {
    Queued,
    Sent,
    Received,
    /// In-chat system notice (restore notice, RS1); `text` = `restored:<ms>`.
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StoredBurnState {
    None,
    Unopened,
    Open,
    /// View-once media placeholder ("🔥 Photo opened").
    Opened,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum StoredFileState {
    Queued,
    Sending,
    Receiving,
    Paused,
    Sent,
    Received,
    Failed,
    IntegrityFailed,
    /// Restored without bytes: missing-media placeholder (CB3).
    Missing,
    /// Placeholder re-requested from the partner.
    Requested,
    /// The partner no longer has it ("Ask to resend").
    Unavailable,
    /// Verified sealed bytes await durable native file finalization.
    Finalizing,
}

/// File-transfer metadata; `local_path` = where the app saved a received file.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoredFile {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub state: StoredFileState,
    pub done_bytes: u64,
    pub local_path: Option<String>,
}

/// A message as persisted. Burn messages never carry content here: it is only
/// available once, via `open_burn`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct StoredMessage {
    pub id_hex: String,
    pub contact_id: String,
    pub text: String,
    pub mine: bool,
    pub time_ms: u64,
    pub status: MessageStatus,
    pub mime: Option<String>,
    pub burn: Option<BurnSpec>,
    pub burn_state: StoredBurnState,
    pub burn_at_ms: Option<u64>,
    pub file: Option<StoredFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChatSummary {
    pub contact_id: String,
    pub last_ms: u64,
    pub last_read_ms: u64,
}

pub(crate) fn file_state(s: u8) -> StoredFileState {
    match s {
        st::FILE_QUEUED => StoredFileState::Queued,
        st::FILE_SENDING => StoredFileState::Sending,
        st::FILE_RECEIVING => StoredFileState::Receiving,
        st::FILE_PAUSED => StoredFileState::Paused,
        st::FILE_SENT => StoredFileState::Sent,
        st::FILE_RECEIVED => StoredFileState::Received,
        st::FILE_INTEGRITY_FAILED => StoredFileState::IntegrityFailed,
        st::FILE_MISSING => StoredFileState::Missing,
        st::FILE_REQUESTED => StoredFileState::Requested,
        st::FILE_UNAVAILABLE => StoredFileState::Unavailable,
        st::FILE_FINALIZING => StoredFileState::Finalizing,
        _ => StoredFileState::Failed,
    }
}

pub(crate) fn to_stored(m: &MessageRec) -> StoredMessage {
    StoredMessage {
        id_hex: hex::encode(m.id),
        contact_id: hex::encode(m.peer),
        text: m.text.clone(),
        mine: m.mine,
        time_ms: m.time_ms,
        status: match m.status {
            st::STATUS_QUEUED => MessageStatus::Queued,
            st::STATUS_SENT => MessageStatus::Sent,
            st::STATUS_SYSTEM => MessageStatus::System,
            _ => MessageStatus::Received,
        },
        mime: m.mime.clone(),
        burn: m
            .burn
            .as_ref()
            .and_then(|b| BurnHeader::decode(b).ok())
            .map(|h| BurnSpec::from_header(&h)),
        burn_state: match m.burn_state {
            st::BURN_UNOPENED => StoredBurnState::Unopened,
            st::BURN_OPEN => StoredBurnState::Open,
            st::BURN_OPENED => StoredBurnState::Opened,
            _ => StoredBurnState::None,
        },
        burn_at_ms: m.burn_at_ms,
        file: m.file.as_ref().map(|f| StoredFile {
            name: f.name.clone(),
            mime: f.mime.clone(),
            size: f.size,
            state: file_state(f.state),
            done_bytes: f.done_bytes,
            local_path: f.local_path.clone(),
        }),
    }
}

pub(crate) fn is_view_once_media(mime: Option<&str>) -> bool {
    mime.is_some_and(|m| m.starts_with("image/") || m.starts_with("video/"))
}

impl NodeState {
    /// Drop queued plaintext as soon as burn content is shredded, including an
    /// unacknowledged send. Pending batch metadata can no longer resurrect it.
    pub(crate) fn discard_queued(&mut self, id: &[u8; 16]) {
        for queue in self.queues.values_mut() {
            queue.retain(|q| q.envelope_id != *id);
        }
    }

    pub(crate) fn db(&self) -> &Store {
        &self.db
    }

    pub(crate) fn persist_contact(&self, id: &[u8; 32]) -> Result<(), FfiError> {
        if let Some(c) = self.contacts.get(id) {
            self.db.put_contact(&ContactRec {
                peer: *id,
                name: c.name.clone(),
                verified: c.verified,
                device: Some(c.device.clone()),
                dh: Some(c.dh.to_vec()),
            })?;
        }
        Ok(())
    }

    /// Write the live burn entry for `id` (sent or received), or shred it from disk.
    pub(crate) fn persist_burn(&self, id: &[u8; 16]) -> Result<(), FfiError> {
        let meta = self.burn_meta.get(id);
        let (snap, dir) = match (self.burn_in.snapshot(id), self.burn_out.snapshot(id)) {
            (Some(s), _) => (s, BurnDir::In),
            (None, Some(s)) => (s, BurnDir::Out),
            (None, None) => return Ok(self.db.delete_burn(id)?),
        };
        let Some(meta) = meta else {
            return Ok(self.db.delete_burn(id)?);
        };
        Ok(self
            .db
            .put_burn(&snap, &meta.peer, dir, meta.mime.as_deref())?)
    }

    pub(crate) fn persist_queued(&self, peer: &[u8; 32], q: &Queued) -> Result<(), FfiError> {
        let burn_key = self.burn_out.snapshot(&q.envelope_id).map(|s| s.key);
        let rec = OutboxRec {
            id: q.envelope_id,
            peer: *peer,
            created_ms: q.created_ms,
            inner: q.inner.to_vec(),
            control: q.control,
            burn: false,
        };
        Ok(self.db.put_outbox(&rec, burn_key.as_deref())?)
    }

    pub(crate) fn update_message(
        &self,
        id: &[u8; 16],
        f: impl FnOnce(&mut MessageRec),
    ) -> Result<(), FfiError> {
        if let Some(mut m) = self.db.message(id)? {
            f(&mut m);
            self.db.put_message(&m)?;
        }
        Ok(())
    }

    /// The burn copy is gone: drop a text bubble, keep a view-once media placeholder.
    pub(crate) fn burn_message_now(&self, id: &[u8; 16]) -> Result<(), FfiError> {
        let Some(m) = self.db.message(id)? else {
            return Ok(());
        };
        if is_view_once_media(m.mime.as_deref()) {
            self.update_message(id, |m| {
                m.burn_state = st::BURN_OPENED;
                m.burn_at_ms = None;
            })
        } else {
            Ok(self.db.delete_message(id)?)
        }
    }

    pub(crate) fn persist_out_file(
        &self,
        peer: &[u8; 32],
        f: &OutgoingFile,
        verified_bytes: Option<&[u8]>,
    ) -> Result<(), FfiError> {
        let record = OutFileRec {
            id: f.id,
            peer: *peer,
            name: f.name.clone(),
            size: f.size,
            total: f.total,
            offer: f.offer.to_vec(),
        };
        if let Some(bytes) = verified_bytes {
            self.db
                .put_received_file_resend(&record, &f.chunks, bytes)?;
        } else {
            self.db.put_out_file(&record, &f.chunks)?;
        }
        Ok(())
    }

    pub(crate) fn refresh_receive_budget(&mut self) -> Result<(), FfiError> {
        self.files.durable_pending = self.db.pending_received_files()?;
        self.files.durable_bytes = usize::try_from(self.db.received_spool_bytes()?)
            .map_err(|_| crate::invalid("receipt budget overflow"))?;
        Ok(())
    }
    /// Persist offer/progress before staging. Existing hash-bound placeholders
    /// cannot be replaced by same-ID/same-peer but different file content.
    pub(crate) fn persist_file_events(
        &mut self,
        out: &mut LinkOutput,
        from: (usize, usize, usize),
        now: u64,
    ) -> Result<(), FfiError> {
        for p in &out.file_progress[from.0..] {
            let id = crate::parse_msg_id(&p.transfer_id_hex)?;
            let peer = crate::parse_id(&p.contact_id)?;
            let done = if p.total_chunks == 0 {
                p.size
            } else {
                p.size * u64::from(p.done_chunks) / u64::from(p.total_chunks)
            };
            if p.incoming {
                let meta = self
                    .files
                    .incoming
                    .get(&id)
                    .map(|f| &f.meta)
                    .or_else(|| self.files.verified.get(&id).map(|f| &f.meta));
                let Some(meta) = meta else {
                    continue;
                };
                self.db.transaction_with::<_, FfiError>(|db| {
                    let mut m = match db.message(&id)? {
                        Some(m) if !accepts_incoming_file(&m, &peer) => return Ok(()),
                        Some(m) => m,
                        None => MessageRec {
                            id,
                            peer,
                            mine: false,
                            time_ms: now,
                            status: st::STATUS_RECEIVED,
                            text: String::new(),
                            mime: None,
                            burn: None,
                            burn_state: st::BURN_NONE,
                            burn_at_ms: None,
                            file: None,
                        },
                    };
                    if m.file
                        .as_ref()
                        .and_then(|f| f.hash.as_deref())
                        .is_some_and(|h| h != meta.content_hash)
                    {
                        return Err(crate::invalid("refetched file hash"));
                    }
                    let old = m.file.as_ref().filter(|f| f.hash.is_some());
                    if old.is_some_and(|f| f.size != meta.size) {
                        return Err(crate::invalid("refetched file size"));
                    }
                    m.file = Some(FileRec {
                        name: old.map_or_else(|| meta.name.clone(), |f| f.name.clone()),
                        mime: old.map_or_else(|| meta.mime.clone(), |f| f.mime.clone()),
                        size: meta.size,
                        state: st::FILE_RECEIVING,
                        done_bytes: done,
                        local_path: None,
                        hash: Some(meta.content_hash.to_vec()),
                        locator: old.and_then(|f| f.locator.clone()).or(Some(id.to_vec())),
                    });
                    db.put_message(&m)?;
                    Ok(())
                })?;
            } else {
                self.update_message(&id, |m| {
                    if m.peer != peer || !m.mine {
                        return;
                    }
                    let Some(f) = m.file.as_mut() else {
                        return;
                    };
                    if matches!(
                        f.state,
                        st::FILE_RECEIVED
                            | st::FILE_FINALIZING
                            | st::FILE_INTEGRITY_FAILED
                            | st::FILE_FAILED
                    ) {
                        return;
                    }
                    f.done_bytes = done;
                    f.state = if p.done_chunks >= p.total_chunks {
                        m.status = st::STATUS_SENT;
                        st::FILE_SENT
                    } else {
                        st::FILE_SENDING
                    };
                })?;
            }
        }
        for e in &out.file_failures[from.2..] {
            let id = crate::parse_msg_id(&e.transfer_id_hex)?;
            if self.files.verified.contains_key(&id) {
                continue;
            }
            let peer = crate::parse_id(&e.contact_id)?;
            self.update_message(&id, |m| {
                if !accepts_incoming_file(m, &peer) {
                    return;
                }
                if let Some(f) = m.file.as_mut() {
                    f.state = if e.reason == "integrity" {
                        st::FILE_INTEGRITY_FAILED
                    } else {
                        st::FILE_FAILED
                    };
                }
            })?;
        }
        out.files.extend(self.stage_verified_files(now, false)?);
        Ok(())
    }
    /// A storage failure leaves the bounded zeroizing candidate in NodeState.
    /// Remove it only after the whole metadata+spool transaction commits.
    pub(crate) fn stage_verified_files(
        &mut self,
        _now: u64,
        retry: bool,
    ) -> Result<Vec<crate::ReceivedFile>, FfiError> {
        let ids: Vec<_> = self.files.verified.keys().copied().collect();
        let mut results = Vec::new();
        let mut first_error = None;
        for id in ids {
            let candidate = self
                .files
                .verified
                .get_mut(&id)
                .expect("candidate snapshot");
            if !retry && !candidate.needs_attempt {
                continue;
            }
            candidate.needs_attempt = false;
            let candidate = &self.files.verified[&id];
            let meta = &candidate.meta;
            let receipt = self.db.transaction_with::<_, FfiError>(|db| {
                let mut m = match db.message(&id)? {
                    Some(m) if !accepts_incoming_file(&m, &candidate.peer) => {
                        return Err(crate::invalid("receive candidate no longer owns message"))
                    }
                    Some(m) => m,
                    None => return Err(crate::invalid("receive candidate message deleted")),
                };
                if m.file
                    .as_ref()
                    .and_then(|f| f.hash.as_deref())
                    .is_some_and(|h| h != meta.content_hash)
                {
                    return Err(crate::invalid("refetched file hash"));
                }
                let old = m.file.as_ref().filter(|f| f.hash.is_some());
                if old.is_some_and(|f| f.size != meta.size) {
                    return Err(crate::invalid("refetched file size"));
                }
                m.file = Some(FileRec {
                    name: old.map_or_else(|| meta.name.clone(), |f| f.name.clone()),
                    mime: old.map_or_else(|| meta.mime.clone(), |f| f.mime.clone()),
                    size: meta.size,
                    state: st::FILE_RECEIVING,
                    done_bytes: meta.size,
                    local_path: None,
                    hash: Some(meta.content_hash.to_vec()),
                    locator: old.and_then(|f| f.locator.clone()).or(Some(id.to_vec())),
                });
                db.put_message(&m)?;
                Ok(db.stage_received_file(&candidate.peer, &id, &candidate.bytes)?)
            });
            let receipt = match receipt {
                Ok(receipt) => receipt,
                Err(error) => {
                    // Continue the bounded pass so one failed candidate cannot
                    // starve unrelated durable receipts. Confirmed deletion or
                    // explicit terminal state revokes this volatile generation.
                    if self.db.message(&id).ok().is_some_and(|m| {
                        m.is_none_or(|m| {
                            m.file.as_ref().is_none_or(|f| {
                                matches!(f.state, st::FILE_FAILED | st::FILE_INTEGRITY_FAILED)
                            })
                        })
                    }) {
                        self.files.verified.remove(&id);
                    }
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    continue;
                }
            };
            self.files.verified.remove(&id);
            results.push(crate::receipt_to_ffi(receipt));
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        self.refresh_receive_budget()?;
        Ok(results)
    }

    /// Link to `peer` dropped: running transfers wait for the next link.
    pub(crate) fn pause_transfers(&self, peer: &[u8; 32]) -> Result<(), FfiError> {
        for m in self.db.messages(peer)? {
            if m.file
                .as_ref()
                .is_some_and(|f| matches!(f.state, st::FILE_SENDING | st::FILE_RECEIVING))
            {
                self.update_message(&m.id, |m| {
                    if let Some(f) = m.file.as_mut() {
                        // A re-fetch that didn't finish goes back to the placeholder.
                        f.state = if f.state == st::FILE_RECEIVING
                            && f.hash.is_some()
                            && !self.files.incoming.contains_key(&m.id)
                            && !self.files.verified.contains_key(&m.id)
                        {
                            st::FILE_MISSING
                        } else {
                            st::FILE_PAUSED
                        };
                    }
                })?;
            } else if m
                .file
                .as_ref()
                .is_some_and(|f| f.state == st::FILE_REQUESTED)
            {
                self.update_message(&m.id, |m| {
                    if let Some(f) = m.file.as_mut() {
                        f.state = st::FILE_MISSING;
                    }
                })?;
            }
        }
        Ok(())
    }

    /// Rebuild the in-memory state from the store (node start, after a restore).
    pub(crate) fn load(&mut self, now_ms: u64) -> Result<(), FfiError> {
        for c in self.db.contacts()? {
            self.contacts.insert(
                c.peer,
                Contact {
                    name: c.name.clone(),
                    verified: c.verified,
                    device: c.device.clone().unwrap_or_default(),
                    dh: c
                        .dh
                        .as_deref()
                        .and_then(|d| d.try_into().ok())
                        .unwrap_or(c.peer),
                },
            );
        }
        for b in self.db.burns()? {
            let id = b.snapshot.msg_id;
            self.burn_meta.insert(
                id,
                BurnMeta {
                    peer: b.peer,
                    mime: b.mime,
                },
            );
            match b.dir {
                BurnDir::In => self.burn_in.restore(b.snapshot),
                BurnDir::Out => self.burn_out.restore(b.snapshot),
            }
        }
        self.queues.clear();
        for q in self.db.outbox()? {
            self.queues.entry(q.peer).or_default().push_back(Queued {
                envelope_id: q.id,
                created_ms: q.created_ms,
                inner: Zeroizing::new(q.inner.clone()),
                control: q.control,
            });
        }
        self.files.out.clear();
        for (f, chunks) in self.db.out_files()? {
            self.files
                .out
                .entry(f.peer)
                .or_default()
                .push_back(OutgoingFile {
                    id: f.id,
                    name: f.name.clone(),
                    size: f.size,
                    total: f.total,
                    cursor: 0,
                    link: None,
                    offer: Zeroizing::new(f.offer.clone()),
                    chunks,
                });
        }
        // Timers that ran out while the app was closed, then any message whose
        // burn copy no longer exists; interrupted transfers resume on the next link.
        let mut due = self.burn_in.tick(now_ms);
        due.extend(self.burn_out.tick(now_ms));
        for id in &due {
            self.discard_queued(id);
            self.db.delete_burn(id)?;
        }
        for m in self.db.all_messages()? {
            if m.burn.is_some()
                && m.burn_state != st::BURN_OPENED
                && !self.burn_in.is_live(&m.id)
                && !self.burn_out.is_live(&m.id)
            {
                self.burn_message_now(&m.id)?;
            } else if m
                .file
                .as_ref()
                .is_some_and(|f| matches!(f.state, st::FILE_SENDING | st::FILE_RECEIVING))
            {
                self.pause_transfers(&m.peer)?;
            }
        }
        for mut m in self.db.all_messages()? {
            if let Some(f) = m.file.as_mut() {
                if f.state == st::FILE_RECEIVED && f.local_path.is_none() {
                    f.state = st::FILE_MISSING;
                    f.done_bytes = 0;
                    self.db.put_message(&m)?;
                }
            }
        }
        self.refresh_receive_budget()?;
        Ok(())
    }
}
