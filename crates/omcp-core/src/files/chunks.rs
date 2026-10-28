//! Per-chunk AEAD encrypt / decrypt and reassembly (PROTOCOL §7).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::Zeroizing;

use crate::error::{OmcpError, Result};
use crate::files::meta::FileMeta;
use crate::files::wrap::{
    generate_content_key, seal_content_key, ContentKey, RecipientFileKeys, WrappedContentKey,
};

/// Default plaintext chunk size: 64 KiB (PROTOCOL §7, §22.7(7)) for all blobs.
pub const DEFAULT_CHUNK_PLAINTEXT_LEN: usize = 64 * 1024;

/// One encrypted file chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChunk {
    pub chunk_index: u32,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

impl FileChunk {
    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(4 + 12 + 4 + self.ciphertext.len());
        v.extend_from_slice(&self.chunk_index.to_be_bytes());
        v.extend_from_slice(&self.nonce);
        v.extend_from_slice(&(self.ciphertext.len() as u32).to_be_bytes());
        v.extend_from_slice(&self.ciphertext);
        v
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        if buf.len() < 4 + 12 + 4 {
            return Err(OmcpError::Truncated {
                need: 4 + 12 + 4,
                have: buf.len(),
            });
        }
        let chunk_index = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        buf = &buf[4..];
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&buf[..12]);
        buf = &buf[12..];
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
        buf = &buf[4..];
        if buf.len() < len {
            return Err(OmcpError::Truncated {
                need: len,
                have: buf.len(),
            });
        }
        if buf.len() != len {
            return Err(OmcpError::File("trailing chunk bytes"));
        }
        Ok(Self {
            chunk_index,
            nonce,
            ciphertext: buf[..len].to_vec(),
        })
    }
}

fn chunk_aad(content_hash: &[u8; 32], chunk_index: u32) -> [u8; 36] {
    let mut aad = [0u8; 36];
    aad[..32].copy_from_slice(content_hash);
    aad[32..].copy_from_slice(&chunk_index.to_be_bytes());
    aad
}

fn chunk_nonce(chunk_index: u32) -> [u8; 12] {
    // counter‖zeros (§5.3 style); Content Key is unique per file so nonce reuse across files is fine.
    let mut n = [0u8; 12];
    n[8..].copy_from_slice(&chunk_index.to_be_bytes());
    n
}

/// Encrypt `plaintext` into [`FileMeta`] + chunks; wrap Content Key to `recipients`.
/// Maximum plaintext file size: 50 MB, decimal (50 000 000 bytes) —
/// PROTOCOL §13.2 default; the app's "too large" copy says "50 MB".
pub const MAX_FILE_BYTES: u64 = 50_000_000;

/// Local budget for the materialized encrypted chunk API (not a wire limit).
/// Incremental receiving does not pay a Vec/nonce/tag allocation per chunk.
const MAX_ENCRYPTED_CHUNK_MEMORY: u64 = 128 * 1024 * 1024;

fn encrypted_chunk_count(size: u64, chunk_len: usize) -> Result<usize> {
    check_file_size(size)?;
    if chunk_len == 0 || chunk_len > u32::MAX as usize {
        return Err(OmcpError::File("chunk size outside u32 range"));
    }
    let count = size.div_ceil(chunk_len as u64).max(1);
    // Include each Vec/nonce/index, AEAD tag and a conservative allocation allowance.
    let overhead = std::mem::size_of::<FileChunk>() as u64 + 16 + 32;
    if size + count * overhead > MAX_ENCRYPTED_CHUNK_MEMORY {
        return Err(OmcpError::File(
            "encrypted chunk memory budget exhausted; use larger chunks",
        ));
    }
    Ok(count as usize)
}

/// Fail fast with [`OmcpError::FileTooLarge`] before any encryption/chunking.
pub fn check_file_size(size: u64) -> Result<()> {
    if size > MAX_FILE_BYTES {
        return Err(OmcpError::FileTooLarge {
            size,
            max: MAX_FILE_BYTES,
        });
    }
    Ok(())
}

pub fn encrypt_file(
    plaintext: &[u8],
    name: &str,
    mime: &str,
    recipients: &[RecipientFileKeys],
    chunk_plaintext_len: usize,
) -> Result<(FileMeta, ContentKey, Vec<FileChunk>)> {
    let count = encrypted_chunk_count(plaintext.len() as u64, chunk_plaintext_len)?;
    if name.len() > u16::MAX as usize
        || mime.len() > u16::MAX as usize
        || recipients.len() > u16::MAX as usize
    {
        return Err(OmcpError::File("metadata field outside u16 range"));
    }
    let content_hash = *blake3::hash(plaintext).as_bytes();
    let content_key = generate_content_key()?;
    let cipher = ChaCha20Poly1305::new_from_slice(content_key.as_bytes())
        .map_err(|_| OmcpError::Crypto("chacha key"))?;

    let mut chunks = Vec::new();
    chunks
        .try_reserve_exact(count)
        .map_err(|_| OmcpError::File("encrypted chunk allocation failed"))?;
    let mut offset = 0usize;
    let mut index = 0u32;
    while offset < plaintext.len() || (plaintext.is_empty() && index == 0) {
        let end = offset
            .saturating_add(chunk_plaintext_len)
            .min(plaintext.len());
        let piece = &plaintext[offset..end];
        let nonce_bytes = chunk_nonce(index);
        let nonce = Nonce::from(nonce_bytes);
        let aad = chunk_aad(&content_hash, index);
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: piece,
                    aad: &aad,
                },
            )
            .map_err(|_| OmcpError::Crypto("chunk seal"))?;
        chunks.push(FileChunk {
            chunk_index: index,
            nonce: nonce_bytes,
            ciphertext,
        });
        offset = end;
        index = index
            .checked_add(1)
            .ok_or(OmcpError::File("too many chunks"))?;
        if plaintext.is_empty() {
            break;
        }
    }

    let mut wrapped_keys = Vec::with_capacity(recipients.len());
    for r in recipients {
        wrapped_keys.push(seal_content_key(&content_key, r)?);
    }

    let meta = FileMeta {
        name: name.to_string(),
        size: plaintext.len() as u64,
        mime: mime.to_string(),
        content_hash,
        chunk_plaintext_len: chunk_plaintext_len as u32,
        num_chunks: chunks.len() as u32,
        wrapped_keys,
    };
    Ok((meta, content_key, chunks))
}

/// Bounded incremental receiver: one file buffer and one bit per chunk, even for
/// valid one-byte chunk layouts. Authenticates before retaining plaintext;
/// unfinished plaintext is zeroized on cancellation/error/drop.
pub struct FileReassembler {
    content_hash: [u8; 32],
    chunk_len: usize,
    num_chunks: u32,
    plaintext: Zeroizing<Vec<u8>>,
    seen: Vec<u8>,
    have: u32,
}

impl FileReassembler {
    /// Exact buffer bytes required, validated without allocating. At the default
    /// 50 MB file limit this is at most 56,250,000 bytes (one-byte chunks).
    pub fn required_bytes(meta: &FileMeta) -> Result<usize> {
        check_file_size(meta.size)?;
        let len = u64::from(meta.chunk_plaintext_len);
        if len == 0 || u64::from(meta.num_chunks) != meta.size.div_ceil(len).max(1) {
            return Err(OmcpError::File("invalid file chunk layout"));
        }
        Ok(meta.size as usize + (meta.num_chunks as usize).div_ceil(8))
    }

    /// Call `required_bytes` to check an aggregate admission budget first.
    pub fn new(meta: &FileMeta) -> Result<Self> {
        Self::required_bytes(meta)?;
        let mut plaintext = Zeroizing::new(Vec::new());
        plaintext
            .try_reserve_exact(meta.size as usize)
            .map_err(|_| OmcpError::File("file buffer allocation failed"))?;
        plaintext.resize(meta.size as usize, 0);
        let mut seen = Vec::new();
        let bitmap_len = (meta.num_chunks as usize).div_ceil(8);
        seen.try_reserve_exact(bitmap_len)
            .map_err(|_| OmcpError::File("file bitmap allocation failed"))?;
        seen.resize(bitmap_len, 0);
        Ok(Self {
            content_hash: meta.content_hash,
            chunk_len: meta.chunk_plaintext_len as usize,
            num_chunks: meta.num_chunks,
            plaintext,
            seen,
            have: 0,
        })
    }

    /// Retain one authenticated chunk. Exact duplicates are idempotent; a
    /// conflicting duplicate, invalid index or mismatched length is an error.
    /// Returns true only for a newly received chunk.
    pub fn push(&mut self, key: &ContentKey, chunk: &FileChunk) -> Result<bool> {
        if chunk.chunk_index >= self.num_chunks {
            return Err(OmcpError::File("chunk index oob"));
        }
        let index = chunk.chunk_index as usize;
        // Layout validation guarantees the multiplication is at most file size.
        let start = index * self.chunk_len;
        let end = start
            .saturating_add(self.chunk_len)
            .min(self.plaintext.len());
        if chunk.ciphertext.len() != end - start + 16 {
            return Err(OmcpError::File("chunk length mismatch"));
        }
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_bytes())
            .map_err(|_| OmcpError::Crypto("chacha key"))?;
        let piece = Zeroizing::new(
            cipher
                .decrypt(
                    &Nonce::from(chunk.nonce),
                    Payload {
                        msg: &chunk.ciphertext,
                        aad: &chunk_aad(&self.content_hash, chunk.chunk_index),
                    },
                )
                .map_err(|_| OmcpError::File("chunk open"))?,
        );
        let mask = 1 << (index % 8);
        if self.seen[index / 8] & mask != 0 {
            if self.plaintext[start..end] != piece[..] {
                return Err(OmcpError::File("conflicting duplicate chunk"));
            }
            return Ok(false);
        }
        self.plaintext[start..end].copy_from_slice(&piece);
        self.seen[index / 8] |= mask;
        self.have += 1;
        Ok(true)
    }

    pub fn received_chunks(&self) -> u32 {
        self.have
    }

    /// Verify the complete content hash and move the buffer without a second
    /// whole-file copy. Any error zeroizes the unfinished plaintext.
    pub fn finish(mut self) -> Result<Vec<u8>> {
        if self.have != self.num_chunks {
            return Err(OmcpError::File("missing chunk"));
        }
        if blake3::hash(&self.plaintext).as_bytes() != &self.content_hash {
            return Err(OmcpError::FileHashMismatch);
        }
        Ok(std::mem::take(&mut *self.plaintext))
    }
}

/// Decrypt chunks with `content_key` and verify BLAKE3 against `meta.content_hash`.
pub fn decrypt_and_reassemble(
    meta: &FileMeta,
    content_key: &ContentKey,
    chunks: &[FileChunk],
) -> Result<Vec<u8>> {
    FileReassembler::required_bytes(meta)?;
    if chunks.len() != meta.num_chunks as usize {
        return Err(OmcpError::File("chunk count mismatch"));
    }
    let mut receiver = FileReassembler::new(meta)?;
    for chunk in chunks {
        if !receiver.push(content_key, chunk)? {
            return Err(OmcpError::File("duplicate chunk"));
        }
    }
    receiver.finish()
}

/// Helper: find the wrapped key for `recipient_id` in meta.
pub fn wrapped_key_for<'a>(
    meta: &'a FileMeta,
    recipient_id: &[u8; 32],
) -> Option<&'a WrappedContentKey> {
    meta.wrapped_keys
        .iter()
        .find(|w| &w.recipient_id == recipient_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_files_over_50_mb_before_encrypting() {
        assert_eq!(MAX_FILE_BYTES, 50_000_000);
        assert!(check_file_size(MAX_FILE_BYTES).is_ok());
        assert_eq!(
            check_file_size(MAX_FILE_BYTES + 1),
            Err(OmcpError::FileTooLarge {
                size: MAX_FILE_BYTES + 1,
                max: MAX_FILE_BYTES
            })
        );
        let big = vec![0u8; (MAX_FILE_BYTES + 1) as usize];
        let err = encrypt_file(&big, "big.bin", "application/octet-stream", &[], 16 * 1024)
            .err()
            .unwrap();
        assert!(matches!(err, OmcpError::FileTooLarge { .. }));
    }
    use crate::files::wrap::FileRecipientSecrets;

    #[test]
    fn small_file_roundtrip() {
        let alice = FileRecipientSecrets::generate([1u8; 32]).unwrap();
        let bob = FileRecipientSecrets::generate([2u8; 32]).unwrap();
        let plain = b"hello OMCP file chunk M3";
        let (meta, _ck, chunks) = encrypt_file(
            plain,
            "hello.txt",
            "text/plain",
            &[alice.public_keys(), bob.public_keys()],
            DEFAULT_CHUNK_PLAINTEXT_LEN,
        )
        .unwrap();
        assert_eq!(meta.num_chunks, 1);
        assert_eq!(chunks.len(), 1);
        let enc = meta.encode().unwrap();
        let meta2 = FileMeta::decode(&enc).unwrap();
        assert_eq!(meta2.name, "hello.txt");

        let w = wrapped_key_for(&meta2, &bob.recipient_id).unwrap();
        let ck = bob.open_content_key(w).unwrap();
        let out = decrypt_and_reassemble(&meta2, &ck, &chunks).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn multi_chunk_file_roundtrip() {
        let bob = FileRecipientSecrets::generate([9u8; 32]).unwrap();
        // 3 chunks of 1000 bytes + remainder
        let mut plain = vec![0u8; 2500];
        for (i, b) in plain.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let (meta, ck, chunks) = encrypt_file(
            &plain,
            "blob.bin",
            "application/octet-stream",
            &[bob.public_keys()],
            1000,
        )
        .unwrap();
        assert_eq!(meta.num_chunks, 3);
        assert_eq!(chunks.len(), 3);
        // Shuffle receive order
        let shuffled = vec![chunks[2].clone(), chunks[0].clone(), chunks[1].clone()];
        let out = decrypt_and_reassemble(&meta, &ck, &shuffled).unwrap();
        assert_eq!(out, plain);
    }

    fn layout(size: u64, chunk_plaintext_len: u32, num_chunks: u32) -> FileMeta {
        FileMeta {
            name: "test".into(),
            mime: "application/octet-stream".into(),
            size,
            chunk_plaintext_len,
            num_chunks,
            content_hash: [0; 32],
            wrapped_keys: vec![],
        }
    }

    #[test]
    fn maximum_file_and_one_byte_chunks_have_bounded_layout_without_allocating() {
        let meta = layout(MAX_FILE_BYTES, 1, MAX_FILE_BYTES as u32);
        assert_eq!(FileReassembler::required_bytes(&meta).unwrap(), 56_250_000);
        let meta = layout(MAX_FILE_BYTES, 65536, MAX_FILE_BYTES.div_ceil(65536) as u32);
        assert_eq!(FileReassembler::required_bytes(&meta).unwrap(), 50_000_096);
        assert_eq!(
            FileReassembler::required_bytes(&layout(0, 1, 1)).unwrap(),
            1
        );
        for meta in [
            layout(u64::MAX, 1, 1),
            layout(1, 0, 1),
            layout(10, 1, u32::MAX),
            layout(0, 1, 0),
        ] {
            assert!(FileReassembler::new(&meta).is_err());
            assert!(decrypt_and_reassemble(&meta, &ContentKey([0; 32]), &[]).is_err());
        }
        assert!(encrypted_chunk_count(MAX_FILE_BYTES, 1).is_err());
        assert_eq!(encrypted_chunk_count(MAX_FILE_BYTES, 65536).unwrap(), 763);
    }

    #[test]
    fn incremental_one_byte_chunks_allow_reordering_and_idempotent_reconnect() {
        let bytes: Vec<u8> = (0..=255).collect();
        let (meta, key, chunks) = encrypt_file(&bytes, "bytes", "binary", &[], 1).unwrap();
        let mut receiver = FileReassembler::new(&meta).unwrap();
        for chunk in chunks.iter().rev() {
            assert!(receiver.push(&key, chunk).unwrap());
            assert!(!receiver.push(&key, chunk).unwrap());
        }
        assert_eq!(receiver.received_chunks(), 256);
        assert_eq!(receiver.finish().unwrap(), bytes);
        assert!(
            decrypt_and_reassemble(&meta, &key, &vec![chunks[0].clone(); chunks.len()]).is_err()
        );
    }

    #[test]
    fn incremental_chunks_reject_invalid_length_index_authentication_and_conflicts() {
        let (meta, key, chunks) = encrypt_file(b"xy", "x", "binary", &[], 1).unwrap();
        let mut receiver = FileReassembler::new(&meta).unwrap();
        let mut bad = chunks[0].clone();
        bad.chunk_index = u32::MAX;
        assert!(receiver.push(&key, &bad).is_err());
        bad = chunks[0].clone();
        bad.ciphertext.push(0);
        assert!(receiver.push(&key, &bad).is_err());
        bad = chunks[0].clone();
        bad.ciphertext[0] ^= 1;
        assert!(receiver.push(&key, &bad).is_err());
        assert_eq!(receiver.received_chunks(), 0);
        receiver.push(&key, &chunks[0]).unwrap();
        // A malicious authenticated sender knows the content key, but cannot
        // rewrite an already accepted slot with a different valid plaintext.
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_bytes()).unwrap();
        bad.ciphertext = cipher
            .encrypt(
                &Nonce::from(bad.nonce),
                Payload {
                    msg: b"z",
                    aad: &chunk_aad(&meta.content_hash, 0),
                },
            )
            .unwrap();
        assert_eq!(
            receiver.push(&key, &bad),
            Err(OmcpError::File("conflicting duplicate chunk"))
        );
        assert_eq!(receiver.received_chunks(), 1);
        assert!(receiver.finish().is_err());
        let mut encoded = chunks[0].encode();
        encoded.push(0);
        assert!(FileChunk::decode(&encoded).is_err());
    }

    #[test]
    fn forged_maximum_ciphertext_length_is_truncated_without_addition_or_allocation() {
        let mut bytes = [0; 20];
        bytes[16..].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(
            FileChunk::decode(&bytes),
            Err(OmcpError::Truncated {
                need: u32::MAX as usize,
                have: 0,
            })
        );
    }

    #[test]
    fn incremental_receiver_still_checks_whole_file_hash() {
        let (mut meta, key, _) = encrypt_file(b"x", "x", "binary", &[], 1).unwrap();
        meta.content_hash = [5; 32];
        let cipher = ChaCha20Poly1305::new_from_slice(key.as_bytes()).unwrap();
        let chunk = FileChunk {
            chunk_index: 0,
            nonce: [0; 12],
            ciphertext: cipher
                .encrypt(
                    &Nonce::from([0; 12]),
                    Payload {
                        msg: b"x",
                        aad: &chunk_aad(&meta.content_hash, 0),
                    },
                )
                .unwrap(),
        };
        let mut receiver = FileReassembler::new(&meta).unwrap();
        receiver.push(&key, &chunk).unwrap();
        assert_eq!(receiver.finish(), Err(OmcpError::FileHashMismatch));
    }
}
