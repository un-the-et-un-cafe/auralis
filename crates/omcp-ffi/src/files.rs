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
    check_file_size, decrypt_and_reassemble, encrypt_file, ContentKey, FileChunk, FileMeta,
};
use zeroize::Zeroizing;

use crate::{invalid, FfiError, LinkOutput};

/// Plaintext bytes per file chunk.
pub const FILE_CHUNK_BYTES: usize = 64 * 1024;
/// Chunks sealed per `flush()` call.
pub(crate) const FILE_CHUNKS_PER_FLUSH: usize = 2;
/// Concurrent incoming transfers per peer (bounds receiver memory).
const MAX_INCOMING_PER_PEER: usize = 2;

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
    chunks: Vec<Option<FileChunk>>,
    have: u32,
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

/// Receiver: a file offer arrived.
pub(crate) fn on_offer(st: &mut FileState, peer: [u8; 32], body: &[u8], out: &mut LinkOutput) {
    let Ok((id, rest)) = split_id(body) else {
        return;
    };
    if rest.len() < 32 || st.incoming.contains_key(&id) {
        return;
    }
    let mut k = [0u8; 32];
    k.copy_from_slice(&rest[..32]);
    let key = ContentKey(k);
    let Ok(meta) = FileMeta::decode(&rest[32..]) else {
        return fail(out, &id, &peer, "invalid");
    };
    let len = meta.chunk_plaintext_len as u64;
    let expect = if meta.size == 0 {
        1
    } else if len == 0 {
        0
    } else {
        meta.size.div_ceil(len)
    };
    if check_file_size(meta.size).is_err()
        || len == 0
        || len > FILE_CHUNK_BYTES as u64
        || u64::from(meta.num_chunks) != expect
    {
        return fail(out, &id, &peer, "invalid");
    }
    if st.incoming.values().filter(|f| f.peer == peer).count() >= MAX_INCOMING_PER_PEER {
        return fail(out, &id, &peer, "busy");
    }
    out.file_progress.push(FileProgress {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode(peer),
        incoming: true,
        name: meta.name.clone(),
        size: meta.size,
        done_chunks: 0,
        total_chunks: meta.num_chunks,
    });
    let n = meta.num_chunks as usize;
    st.incoming.insert(
        id,
        IncomingFile {
            peer,
            meta,
            key,
            chunks: vec![None; n],
            have: 0,
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
    let Ok(c) = FileChunk::decode(rest) else {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    };
    let i = c.chunk_index as usize;
    if i >= f.chunks.len() {
        st.incoming.remove(&id);
        return fail(out, &id, &peer, "invalid");
    }
    if f.chunks[i].is_none() {
        f.chunks[i] = Some(c);
        f.have += 1;
    }
    out.file_progress.push(FileProgress {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode(peer),
        incoming: true,
        name: f.meta.name.clone(),
        size: f.meta.size,
        done_chunks: f.have,
        total_chunks: f.meta.num_chunks,
    });
    if f.have < f.meta.num_chunks {
        return;
    }
    let Some(f) = st.incoming.remove(&id) else {
        return;
    };
    let chunks: Vec<FileChunk> = f.chunks.into_iter().flatten().collect();
    match decrypt_and_reassemble(&f.meta, &f.key, &chunks) {
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
