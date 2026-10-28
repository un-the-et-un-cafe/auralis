//! Chunked file transfer over a [`MeshLink`] (PROTOCOL §7, beta wiring).
//!
//! The sender encrypts the whole file once with [`omcp_core::files::encrypt_file`]
//! (64 KiB plaintext chunks, per-chunk ChaCha20-Poly1305, BLAKE3 content hash).
//! The offer (`transfer_id ‖ content_key ‖ FileMeta`) and every chunk
//! (`transfer_id ‖ FileChunk`) then travel as ordinary end-to-end encrypted
//! `InnerV1` messages (inner content types `0x20` / `0x21`).
//!
//! Beta note: contacts don't yet carry ML-KEM file keys, so the Content Key is
//! sent inside the E2E-encrypted offer instead of an HPKE `wrapped_keys` entry
//! (`wrapped_keys` is empty). 1:1 only.
//!
//! Chunks are drained a few at a time per [`MeshLink::flush`] so a 50 MB file never
//! turns into one giant PDU batch; `LinkOutput::more_pending` tells the platform
//! to call `flush()` again once its GATT write/notify queue is empty.

use std::collections::{HashMap, VecDeque};

use omcp_core::burn::AppContent;
use omcp_core::files::{
    check_file_size, encrypt_file, ContentKey, FileChunk, FileMeta, FileReassembler,
};
use zeroize::Zeroizing;

use crate::{invalid, FfiError, LinkOutput};

/// Plaintext bytes per file chunk.
pub const FILE_CHUNK_BYTES: usize = 64 * 1024;
/// Chunks sealed per `flush()` call.
pub(crate) const FILE_CHUNKS_PER_FLUSH: usize = 2;
/// Concurrent incoming transfers per peer (bounds receiver memory).
const MAX_INCOMING_PER_PEER: usize = 2;
/// Node-wide budgets include paused/reconnecting transfers; local admission
/// policy, not smaller OMCP file/chunk limits. See docs/RESOURCE_LIMITS.md.
const MAX_INCOMING_TRANSFERS: usize = 8;
const MAX_INCOMING_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileOffer {
    pub transfer_id_hex: String,
    pub contact_id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub num_chunks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileProgress {
    pub transfer_id_hex: String,
    pub contact_id: String,
    pub incoming: bool,
    pub name: String,
    pub size: u64,
    pub done_chunks: u32,
    pub total_chunks: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ReceivedFile {
    pub transfer_id_hex: String,
    pub contact_id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FileFailure {
    pub transfer_id_hex: String,
    pub contact_id: String,
    /// Stable reason code: `integrity` (hash/AEAD failed), `invalid`, `busy`.
    pub reason: String,
}

pub(crate) struct OutgoingFile {
    pub id: [u8; 16],
    pub name: String,
    pub size: u64,
    pub total: u32,
    /// Next chunk to seal on the current link.
    pub cursor: usize,
    /// Link handle the transfer is running on. A new link (reconnect) re-sends the
    /// offer and replays from chunk 0; the receiver keeps what it already has and
    /// ignores duplicates, so an interrupted transfer completes without user action.
    pub link: Option<u64>,
    pub offer: Zeroizing<Vec<u8>>,
    pub chunks: Vec<Zeroizing<Vec<u8>>>,
}

pub(crate) struct IncomingFile {
    peer: [u8; 32],
    meta: FileMeta,
    key: ContentKey,
    receiver: FileReassembler,
    reserved_bytes: usize,
}

#[derive(Default)]
pub(crate) struct FileState {
    pub out: HashMap<[u8; 32], VecDeque<OutgoingFile>>,
    pub incoming: HashMap<[u8; 16], IncomingFile>,
}

pub(crate) fn build_outgoing(
    id: [u8; 16],
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<OutgoingFile, FfiError> {
    check_file_size(bytes.len() as u64)?;
    if name.is_empty() || name.len() > 255 || mime.len() > 255 {
        return Err(invalid("file name/mime"));
    }
    let (meta, key, chunks) = encrypt_file(bytes, name, mime, &[], FILE_CHUNK_BYTES)?;
    let mut offer = Vec::with_capacity(16 + 32 + 128);
    offer.extend_from_slice(&id);
    offer.extend_from_slice(key.as_bytes());
    offer.extend_from_slice(&meta.encode()?);
    let offer = encode(AppContent::FileMeta(offer))?;
    let chunks = chunks
        .iter()
        .map(|c| {
            let mut b = Vec::with_capacity(16 + 20 + c.ciphertext.len());
            b.extend_from_slice(&id);
            b.extend_from_slice(&c.encode());
            encode(AppContent::FileChunk(b))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OutgoingFile {
        id,
        name: name.into(),
        size: meta.size,
        total: meta.num_chunks,
        cursor: 0,
        link: None,
        offer,
        chunks,
    })
}

fn encode(c: AppContent) -> Result<Zeroizing<Vec<u8>>, FfiError> {
    Ok(Zeroizing::new(omcp_core::burn::encode_app(None, &c)?))
}

impl OutgoingFile {
    pub fn progress(&self, peer: &[u8; 32]) -> FileProgress {
        FileProgress {
            transfer_id_hex: hex::encode(self.id),
            contact_id: hex::encode(peer),
            incoming: false,
            name: self.name.clone(),
            size: self.size,
            done_chunks: self.cursor as u32,
            total_chunks: self.total,
        }
    }
}

fn split_id(b: &[u8]) -> Result<([u8; 16], &[u8]), FfiError> {
    if b.len() < 16 {
        return Err(invalid("file transfer id"));
    }
    let mut id = [0u8; 16];
    id.copy_from_slice(&b[..16]);
    Ok((id, &b[16..]))
}

fn fail(out: &mut LinkOutput, id: &[u8; 16], peer: &[u8; 32], reason: &str) {
    out.file_failures.push(FileFailure {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode(peer),
        reason: reason.into(),
    });
}

fn incoming_budget_allows(
    count: usize,
    peer_count: usize,
    reserved: usize,
    required: usize,
) -> bool {
    count < MAX_INCOMING_TRANSFERS
        && peer_count < MAX_INCOMING_PER_PEER
        && reserved
            .checked_add(required)
            .is_some_and(|n| n <= MAX_INCOMING_BYTES)
}

/// Receiver: a file offer arrived.
pub(crate) fn on_offer(st: &mut FileState, peer: [u8; 32], body: &[u8], out: &mut LinkOutput) {
    let Ok((id, rest)) = split_id(body) else {
        return;
    };
    if st.incoming.get(&id).is_some_and(|f| f.peer != peer) {
        // An ID collision from a different authenticated contact must not erase
        // the original contact's partial transfer.
        return fail(out, &id, &peer, "invalid");
    }
    if rest.len() < 32 {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&rest[..32]);
    let key = ContentKey(k);
    let Ok(meta) = FileMeta::decode(&rest[32..]) else {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    };
    let Ok(buffer_bytes) = FileReassembler::required_bytes(&meta) else {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    };
    if meta.chunk_plaintext_len as usize > FILE_CHUNK_BYTES {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    }
    if let Some(f) = st.incoming.get(&id) {
        if f.meta == meta && f.key == key {
            // Reconnect replays the same offer. Retain authenticated partial data
            // and do not reserve a second buffer or reset receipt progress.
            out.file_progress.push(FileProgress {
                transfer_id_hex: hex::encode(id),
                contact_id: hex::encode(peer),
                incoming: true,
                name: meta.name,
                size: meta.size,
                done_chunks: f.receiver.received_chunks(),
                total_chunks: meta.num_chunks,
            });
            return;
        }
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    }
    // Metadata is bounded to an envelope before decode. Twice its wire size
    // covers its strings, wrapped-key structs/ciphertexts and allocation slack.
    let reserved_bytes = buffer_bytes + 2 * rest.len() + std::mem::size_of::<IncomingFile>() + 256;
    let reserved: usize = st.incoming.values().map(|f| f.reserved_bytes).sum();
    if !incoming_budget_allows(
        st.incoming.len(),
        st.incoming.values().filter(|f| f.peer == peer).count(),
        reserved,
        reserved_bytes,
    ) {
        return fail(out, &id, &peer, "busy");
    }
    let Ok(receiver) = FileReassembler::new(&meta) else {
        return fail(out, &id, &peer, "busy");
    };
    out.file_progress.push(FileProgress {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode(peer),
        incoming: true,
        name: meta.name.clone(),
        size: meta.size,
        done_chunks: 0,
        total_chunks: meta.num_chunks,
    });
    st.incoming.insert(
        id,
        IncomingFile {
            peer,
            meta,
            key,
            receiver,
            reserved_bytes,
        },
    );
}

/// Receiver: one chunk arrived. Reassembles and verifies when complete.
pub(crate) fn on_chunk(st: &mut FileState, peer: [u8; 32], body: &[u8], out: &mut LinkOutput) {
    let Ok((id, rest)) = split_id(body) else {
        return;
    };
    let Some(f) = st.incoming.get_mut(&id) else {
        return;
    };
    if f.peer != peer {
        return;
    }
    // Reject oversized encoded chunks before FileChunk::decode copies any bytes.
    if rest.len() > 20 + FILE_CHUNK_BYTES + 16 {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    }
    let Ok(c) = FileChunk::decode(rest) else {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    };
    if let Err(error) = f.receiver.push(&f.key, &c) {
        st.incoming.remove(&id);
        let reason = if matches!(error, omcp_core::error::OmcpError::File("chunk open")) {
            "integrity"
        } else {
            "invalid"
        };
        return fail(out, &id, &peer, reason);
    }
    out.file_progress.push(FileProgress {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode(peer),
        incoming: true,
        name: f.meta.name.clone(),
        size: f.meta.size,
        done_chunks: f.receiver.received_chunks(),
        total_chunks: f.meta.num_chunks,
    });
    if f.receiver.received_chunks() < f.meta.num_chunks {
        return;
    }
    let Some(f) = st.incoming.remove(&id) else {
        return;
    };
    match f.receiver.finish() {
        Ok(bytes) => out.files.push(ReceivedFile {
            transfer_id_hex: hex::encode(id),
            contact_id: hex::encode(peer),
            name: f.meta.name,
            mime: f.meta.mime,
            size: f.meta.size,
            bytes,
        }),
        Err(_) => fail(out, &id, &peer, "integrity"),
    }
}

impl FileState {
    /// v0.1.4a migration: a legacy contact id became a PeerID.
    pub(crate) fn rekey(&mut self, old: &[u8; 32], new: &[u8; 32]) {
        if let Some(q) = self.out.remove(old) {
            self.out.entry(*new).or_default().extend(q);
        }
        for f in self.incoming.values_mut() {
            if &f.peer == old {
                f.peer = *new;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(id: [u8; 16], meta: &FileMeta, key: &ContentKey) -> Vec<u8> {
        let mut body = id.to_vec();
        body.extend_from_slice(key.as_bytes());
        body.extend_from_slice(&meta.encode().unwrap());
        body
    }

    fn chunk(id: [u8; 16], chunk: &FileChunk) -> Vec<u8> {
        let mut body = id.to_vec();
        body.extend_from_slice(&chunk.encode());
        body
    }

    #[test]
    fn node_wide_file_budget_boundaries_do_not_require_large_allocations() {
        let max_file = 56_250_000 + 4096;
        assert!(incoming_budget_allows(0, 0, 0, max_file));
        assert!(incoming_budget_allows(1, 1, max_file, max_file));
        assert!(!incoming_budget_allows(2, 0, 2 * max_file, max_file));
        assert!(incoming_budget_allows(0, 0, 0, MAX_INCOMING_BYTES));
        assert!(!incoming_budget_allows(0, 0, 0, MAX_INCOMING_BYTES + 1));
        assert!(!incoming_budget_allows(0, 0, usize::MAX, 1));
        assert!(!incoming_budget_allows(MAX_INCOMING_TRANSFERS, 0, 0, 1));
        assert!(!incoming_budget_allows(0, MAX_INCOMING_PER_PEER, 0, 1));
    }

    #[test]
    fn incoming_ids_are_bounded_across_peers_and_invalid_chunk_releases_admission() {
        let (meta, key, _) = encrypt_file(b"xy", "x", "binary", &[], 1).unwrap();
        let mut st = FileState::default();
        let mut out = LinkOutput::default();
        for i in 0..MAX_INCOMING_TRANSFERS as u8 {
            on_offer(&mut st, [i; 32], &offer([i; 16], &meta, &key), &mut out);
        }
        assert_eq!(st.incoming.len(), MAX_INCOMING_TRANSFERS);
        on_offer(&mut st, [99; 32], &offer([99; 16], &meta, &key), &mut out);
        assert_eq!(out.file_failures.last().unwrap().reason, "busy");
        assert!(!st.incoming.contains_key(&[99; 16]));
        // Malformed body for a transfer owned by this peer frees its whole budget.
        on_chunk(&mut st, [0; 32], &[0; 16], &mut out);
        assert_eq!(st.incoming.len(), MAX_INCOMING_TRANSFERS - 1);
        on_offer(&mut st, [99; 32], &offer([99; 16], &meta, &key), &mut out);
        assert!(st.incoming.contains_key(&[99; 16]));
    }

    #[test]
    fn reconnect_offer_is_idempotent_but_conflicting_or_cross_peer_ids_cannot_overwrite() {
        let (meta, key, chunks) = encrypt_file(b"xy", "x", "binary", &[], 1).unwrap();
        let mut st = FileState::default();
        let mut out = LinkOutput::default();
        let id = [1; 16];
        let body = offer(id, &meta, &key);
        on_offer(&mut st, [1; 32], &body, &mut out);
        on_chunk(&mut st, [1; 32], &chunk(id, &chunks[0]), &mut out);
        let reserved = st.incoming[&id].reserved_bytes;
        on_offer(&mut st, [1; 32], &body, &mut out);
        assert_eq!(st.incoming[&id].receiver.received_chunks(), 1);
        assert_eq!(st.incoming[&id].reserved_bytes, reserved);
        on_offer(&mut st, [2; 32], &body, &mut out);
        assert_eq!(out.file_failures.last().unwrap().reason, "invalid");
        assert_eq!(st.incoming[&id].receiver.received_chunks(), 1);
        on_chunk(&mut st, [2; 32], &[1; 16], &mut out);
        assert!(st.incoming.contains_key(&id));
        on_chunk(&mut st, [1; 32], &chunk(id, &chunks[0]), &mut out);
        assert_eq!(st.incoming[&id].receiver.received_chunks(), 1);
        on_chunk(&mut st, [1; 32], &chunk(id, &chunks[1]), &mut out);
        assert_eq!(out.files.last().unwrap().bytes, b"xy");
        assert!(st.incoming.is_empty());
        on_offer(&mut st, [1; 32], &body, &mut out);
        on_offer(
            &mut st,
            [1; 32],
            &offer(id, &meta, &ContentKey([99; 32])),
            &mut out,
        );
        assert!(st.incoming.is_empty());
        assert_eq!(out.file_failures.last().unwrap().reason, "invalid");
    }

    #[test]
    fn invalid_layout_and_oversized_chunk_are_rejected_before_chunk_allocation() {
        let (mut meta, key, _) = encrypt_file(b"x", "x", "binary", &[], 1).unwrap();
        let mut st = FileState::default();
        let mut out = LinkOutput::default();
        meta.num_chunks = 50_000_000;
        on_offer(&mut st, [1; 32], &offer([1; 16], &meta, &key), &mut out);
        assert!(st.incoming.is_empty());
        assert_eq!(out.file_failures.last().unwrap().reason, "invalid");
        meta.num_chunks = 1;
        on_offer(&mut st, [1; 32], &offer([1; 16], &meta, &key), &mut out);
        let mut body = vec![0; 16 + 20 + FILE_CHUNK_BYTES + 17];
        body[..16].copy_from_slice(&[1; 16]);
        on_chunk(&mut st, [1; 32], &body, &mut out);
        assert!(st.incoming.is_empty());
        assert_eq!(out.file_failures.last().unwrap().reason, "invalid");
    }
}
