//! Per-chunk AEAD encrypt / decrypt and reassembly (PROTOCOL §7).

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};

use crate::error::{OmcpError, Result};
use crate::files::meta::FileMeta;
use crate::files::wrap::{
    generate_content_key, seal_content_key, ContentKey, RecipientFileKeys, WrappedContentKey,
};

/// Default plaintext chunk size: 16 KiB (PROTOCOL §7).
pub const DEFAULT_CHUNK_PLAINTEXT_LEN: usize = 16 * 1024;

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
pub fn encrypt_file(
    plaintext: &[u8],
    name: &str,
    mime: &str,
    recipients: &[RecipientFileKeys],
    chunk_plaintext_len: usize,
) -> Result<(FileMeta, ContentKey, Vec<FileChunk>)> {
    if chunk_plaintext_len == 0 {
        return Err(OmcpError::File("chunk size zero"));
    }
    let content_hash = *blake3::hash(plaintext).as_bytes();
    let content_key = generate_content_key()?;
    let cipher = ChaCha20Poly1305::new_from_slice(content_key.as_bytes())
        .map_err(|_| OmcpError::Crypto("chacha key"))?;

    let mut chunks = Vec::new();
    let mut offset = 0usize;
    let mut index = 0u32;
    while offset < plaintext.len() || (plaintext.is_empty() && index == 0) {
        let end = (offset + chunk_plaintext_len).min(plaintext.len());
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
        index = index.checked_add(1).ok_or(OmcpError::File("too many chunks"))?;
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

/// Decrypt chunks with `content_key` and verify BLAKE3 against `meta.content_hash`.
pub fn decrypt_and_reassemble(
    meta: &FileMeta,
    content_key: &ContentKey,
    chunks: &[FileChunk],
) -> Result<Vec<u8>> {
    if chunks.len() as u32 != meta.num_chunks {
        return Err(OmcpError::File("chunk count mismatch"));
    }
    let cipher = ChaCha20Poly1305::new_from_slice(content_key.as_bytes())
        .map_err(|_| OmcpError::Crypto("chacha key"))?;

    let mut ordered: Vec<Option<&FileChunk>> = vec![None; meta.num_chunks as usize];
    for c in chunks {
        let i = c.chunk_index as usize;
        if i >= ordered.len() {
            return Err(OmcpError::File("chunk index oob"));
        }
        if ordered[i].is_some() {
            return Err(OmcpError::File("duplicate chunk"));
        }
        ordered[i] = Some(c);
    }

    let mut plaintext = Vec::with_capacity(meta.size as usize);
    for (i, slot) in ordered.iter().enumerate() {
        let c = slot.ok_or(OmcpError::File("missing chunk"))?;
        let nonce = Nonce::from(c.nonce);
        let aad = chunk_aad(&meta.content_hash, i as u32);
        let piece = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &c.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| OmcpError::File("chunk open"))?;
        plaintext.extend_from_slice(&piece);
    }

    if plaintext.len() as u64 != meta.size {
        return Err(OmcpError::File("size mismatch"));
    }
    let hash = *blake3::hash(&plaintext).as_bytes();
    if hash != meta.content_hash {
        return Err(OmcpError::FileHashMismatch);
    }
    Ok(plaintext)
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
}
