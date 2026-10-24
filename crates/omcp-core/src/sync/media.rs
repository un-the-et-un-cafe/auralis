//! Lazy, content-addressed media (PROTOCOL §22.3), built on the §7 `files`
//! module (BLAKE3 content hash + per-chunk ChaCha20-Poly1305).
//!
//! A message carries only a [`MediaRef`]: 32-byte BLAKE3 content hash, the
//! 32-byte content key and a ≤ 40 B blurhash. Blobs are chunked at
//! [`MEDIA_CHUNK_LEN`] (64 KiB, same as §7 files) and fetched on demand, resumably, from a
//! holder. Holders index blobs by an opaque *locator* (§22.7(6):
//! `BLAKE3-keyed(media_key, "OMCP-loc" || content_hash)`) so a relay
//! never learns the plaintext hash (no confirmation-of-file attack), and
//! chunks are stored de-duplicated by `BLAKE3(chunk)`. Relays have no
//! auto-fetch path: chunks move only in response to [`MediaHolder::serve`].

use std::collections::{BTreeSet, HashMap};

use super::take;
use crate::error::{OmcpError, Result};
use crate::files::{decrypt_and_reassemble, encrypt_file, ContentKey, FileChunk, FileMeta};

/// Media chunk plaintext size (64 KiB).
pub const MEDIA_CHUNK_LEN: usize = crate::files::DEFAULT_CHUNK_PLAINTEXT_LEN;
/// Max blurhash bytes.
pub const MAX_BLURHASH: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRef {
    pub content_hash: [u8; 32],
    pub key: [u8; 32],
    pub blurhash: Vec<u8>,
}

impl MediaRef {
    pub fn locator(&self) -> [u8; 32] {
        locator(&self.key, &self.content_hash)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.blurhash.len() > MAX_BLURHASH {
            return Err(OmcpError::Sync("blurhash too long"));
        }
        let mut v = Vec::with_capacity(65 + self.blurhash.len());
        v.extend_from_slice(&self.content_hash);
        v.extend_from_slice(&self.key);
        v.push(self.blurhash.len() as u8);
        v.extend_from_slice(&self.blurhash);
        Ok(v)
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let content_hash = take(&mut buf, 32)?.try_into().expect("32");
        let key = take(&mut buf, 32)?.try_into().expect("32");
        let n = take(&mut buf, 1)?[0] as usize;
        if n > MAX_BLURHASH {
            return Err(OmcpError::Sync("blurhash too long"));
        }
        let blurhash = take(&mut buf, n)?.to_vec();
        if !buf.is_empty() {
            return Err(OmcpError::Sync("media ref trailing bytes"));
        }
        Ok(Self {
            content_hash,
            key,
            blurhash,
        })
    }
}

/// `locator = BLAKE3-keyed(media_key, "OMCP-loc" || content_hash)` (§22.7(6)).
pub fn locator(media_key: &[u8; 32], content_hash: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new_keyed(media_key);
    h.update(b"OMCP-loc");
    h.update(content_hash);
    *h.finalize().as_bytes()
}

/// Encrypt a blob for lazy sharing. Returns the ref (for the message) and
/// the public, key-less meta + chunks (for holders).
pub fn prepare_media(
    plaintext: &[u8],
    blurhash: &[u8],
) -> Result<(MediaRef, FileMeta, Vec<FileChunk>)> {
    if blurhash.len() > MAX_BLURHASH {
        return Err(OmcpError::Sync("blurhash too long"));
    }
    // No per-recipient wraps: the key rides inside the E2E message.
    let (mut meta, key, chunks) = encrypt_file(plaintext, "", "", &[], MEDIA_CHUNK_LEN)?;
    let r = MediaRef {
        content_hash: meta.content_hash,
        key: key.0,
        blurhash: blurhash.to_vec(),
    };
    meta.wrapped_keys.clear();
    Ok((r, meta, chunks))
}

/// A device or relay that holds media it was asked to keep.
#[derive(Debug, Default)]
pub struct MediaHolder {
    chunks: HashMap<[u8; 32], FileChunk>,
    blobs: HashMap<[u8; 32], (FileMeta, Vec<[u8; 32]>)>,
}

impl MediaHolder {
    /// Store a blob under its locator; chunks are de-duplicated.
    pub fn put(&mut self, locator: [u8; 32], meta: FileMeta, chunks: Vec<FileChunk>) {
        let mut ids = Vec::with_capacity(chunks.len());
        for c in chunks {
            let id = *blake3::hash(&c.encode()).as_bytes();
            self.chunks.entry(id).or_insert(c);
            ids.push(id);
        }
        self.blobs.insert(locator, (meta, ids));
    }

    pub fn meta(&self, locator: &[u8; 32]) -> Option<&FileMeta> {
        self.blobs.get(locator).map(|b| &b.0)
    }

    /// Serve one chunk on request.
    pub fn serve(&self, locator: &[u8; 32], index: u32) -> Option<&FileChunk> {
        let (_, ids) = self.blobs.get(locator)?;
        self.chunks.get(ids.get(index as usize)?)
    }

    pub fn stored_chunks(&self) -> usize {
        self.chunks.len()
    }
}

/// Resumable on-demand fetch state.
#[derive(Debug, Clone)]
pub struct LazyFetch {
    pub r: MediaRef,
    meta: FileMeta,
    got: HashMap<u32, FileChunk>,
}

impl LazyFetch {
    pub fn new(r: MediaRef, meta: FileMeta) -> Result<Self> {
        if meta.content_hash != r.content_hash {
            return Err(OmcpError::Sync("media meta mismatch"));
        }
        Ok(Self {
            r,
            meta,
            got: HashMap::new(),
        })
    }

    /// Indices still missing (resume point).
    pub fn missing(&self) -> BTreeSet<u32> {
        (0..self.meta.num_chunks)
            .filter(|i| !self.got.contains_key(i))
            .collect()
    }

    /// Accept a chunk (duplicates ignored). Returns `true` if new.
    pub fn accept(&mut self, c: FileChunk) -> Result<bool> {
        if c.chunk_index >= self.meta.num_chunks {
            return Err(OmcpError::Sync("chunk index"));
        }
        Ok(self.got.insert(c.chunk_index, c).is_none())
    }

    /// Decrypt + verify once complete.
    pub fn finish(&self) -> Result<Vec<u8>> {
        let chunks: Vec<FileChunk> = self.got.values().cloned().collect();
        decrypt_and_reassemble(&self.meta, &ContentKey(self.r.key), &chunks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lazy_resume_dedupe() {
        let blob: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let (r, meta, chunks) = prepare_media(&blob, b"LEHV6nWB2yk8").unwrap();
        assert_eq!(meta.num_chunks, 4);
        let wire = r.encode().unwrap();
        assert!(wire.len() <= 32 + 32 + 1 + MAX_BLURHASH);
        let r = MediaRef::decode(&wire).unwrap();
        let mut h = MediaHolder::default();
        h.put(r.locator(), meta.clone(), chunks.clone());
        h.put(r.locator(), meta.clone(), chunks);
        assert_eq!(h.stored_chunks(), 4);
        assert_ne!(r.locator(), r.content_hash);
        let mut kh = blake3::Hasher::new_keyed(&r.key);
        kh.update(b"OMCP-loc");
        kh.update(&r.content_hash);
        assert_eq!(r.locator(), *kh.finalize().as_bytes());

        let mut f = LazyFetch::new(r.clone(), h.meta(&r.locator()).unwrap().clone()).unwrap();
        // fetch two, "disconnect", resume.
        for i in [0u32, 2] {
            assert!(f.accept(h.serve(&r.locator(), i).unwrap().clone()).unwrap());
        }
        assert!(!f.accept(h.serve(&r.locator(), 0).unwrap().clone()).unwrap());
        assert!(f.finish().is_err());
        assert_eq!(f.missing().into_iter().collect::<Vec<_>>(), vec![1, 3]);
        for i in f.missing() {
            f.accept(h.serve(&r.locator(), i).unwrap().clone()).unwrap();
        }
        assert_eq!(f.finish().unwrap(), blob);
        assert!(h.serve(&[0; 32], 0).is_none());
        assert!(prepare_media(b"x", &[0; 41]).is_err());
    }
}
