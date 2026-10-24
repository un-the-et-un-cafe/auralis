//! File metadata advertisement (PROTOCOL §6.3 `0x20` / §7).

use crate::error::{OmcpError, Result};
use crate::files::wrap::WrappedContentKey;

/// Content type `0x20` — `application/omcp-file-meta`.
pub const CONTENT_TYPE_FILE_META: u8 = 0x20;
/// Content type `0x21` — chunked encrypted blob.
pub const CONTENT_TYPE_FILE_CHUNK: u8 = 0x21;

/// Metadata published before chunk serve (PROTOCOL §7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
    pub mime: String,
    /// BLAKE3(plaintext), 32 bytes.
    pub content_hash: [u8; 32],
    /// Plaintext bytes per chunk (default 16 KiB).
    pub chunk_plaintext_len: u32,
    pub num_chunks: u32,
    /// Content Key wrapped to each intended recipient.
    pub wrapped_keys: Vec<WrappedContentKey>,
}

impl FileMeta {
    /// Encode to a compact binary blob (M3 provisional).
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.name.len() > u16::MAX as usize || self.mime.len() > u16::MAX as usize {
            return Err(OmcpError::File("name/mime too long"));
        }
        if self.wrapped_keys.len() > u16::MAX as usize {
            return Err(OmcpError::File("too many wrapped keys"));
        }
        let mut v = Vec::new();
        v.extend_from_slice(&(self.name.len() as u16).to_be_bytes());
        v.extend_from_slice(self.name.as_bytes());
        v.extend_from_slice(&self.size.to_be_bytes());
        v.extend_from_slice(&(self.mime.len() as u16).to_be_bytes());
        v.extend_from_slice(self.mime.as_bytes());
        v.extend_from_slice(&self.content_hash);
        v.extend_from_slice(&self.chunk_plaintext_len.to_be_bytes());
        v.extend_from_slice(&self.num_chunks.to_be_bytes());
        v.extend_from_slice(&(self.wrapped_keys.len() as u16).to_be_bytes());
        for w in &self.wrapped_keys {
            let enc = w.encode();
            v.extend_from_slice(&(enc.len() as u32).to_be_bytes());
            v.extend_from_slice(&enc);
        }
        Ok(v)
    }

    /// Decode from [`Self::encode`].
    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let take_u16 = |buf: &mut &[u8]| -> Result<u16> {
            if buf.len() < 2 {
                return Err(OmcpError::Truncated {
                    need: 2,
                    have: buf.len(),
                });
            }
            let n = u16::from_be_bytes([buf[0], buf[1]]);
            *buf = &buf[2..];
            Ok(n)
        };
        let name_len = take_u16(&mut buf)? as usize;
        if buf.len() < name_len {
            return Err(OmcpError::Truncated {
                need: name_len,
                have: buf.len(),
            });
        }
        let name = std::str::from_utf8(&buf[..name_len])
            .map_err(|_| OmcpError::File("name utf8"))?
            .to_string();
        buf = &buf[name_len..];
        if buf.len() < 8 {
            return Err(OmcpError::Truncated {
                need: 8,
                have: buf.len(),
            });
        }
        let mut size_b = [0u8; 8];
        size_b.copy_from_slice(&buf[..8]);
        let size = u64::from_be_bytes(size_b);
        buf = &buf[8..];
        let mime_len = take_u16(&mut buf)? as usize;
        if buf.len() < mime_len {
            return Err(OmcpError::Truncated {
                need: mime_len,
                have: buf.len(),
            });
        }
        let mime = std::str::from_utf8(&buf[..mime_len])
            .map_err(|_| OmcpError::File("mime utf8"))?
            .to_string();
        buf = &buf[mime_len..];
        if buf.len() < 32 + 4 + 4 + 2 {
            return Err(OmcpError::Truncated {
                need: 32 + 4 + 4 + 2,
                have: buf.len(),
            });
        }
        let mut content_hash = [0u8; 32];
        content_hash.copy_from_slice(&buf[..32]);
        buf = &buf[32..];
        let chunk_plaintext_len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        buf = &buf[4..];
        let num_chunks = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        buf = &buf[4..];
        let n_keys = take_u16(&mut buf)? as usize;
        let mut wrapped_keys = Vec::with_capacity(n_keys);
        for _ in 0..n_keys {
            if buf.len() < 4 {
                return Err(OmcpError::Truncated {
                    need: 4,
                    have: buf.len(),
                });
            }
            let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
            buf = &buf[4..];
            if buf.len() < len {
                return Err(OmcpError::Truncated {
                    need: len,
                    have: buf.len(),
                });
            }
            wrapped_keys.push(WrappedContentKey::decode(&buf[..len])?);
            buf = &buf[len..];
        }
        Ok(Self {
            name,
            size,
            mime,
            content_hash,
            chunk_plaintext_len,
            num_chunks,
            wrapped_keys,
        })
    }
}
