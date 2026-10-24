//! Chunked encrypted file sharing (PROTOCOL §7, content types §6.3).
//!
//! Flow:
//! 1. `content_hash = BLAKE3(plaintext)`
//! 2. Random 256-bit Content Key; wrap to each recipient (HPKE-style hybrid)
//! 3. Split into default 64 KiB (§22.7(7)) plaintext chunks; per-chunk ChaCha20-Poly1305
//! 4. Advertise [`FileMeta`] then serve [`FileChunk`]s; receiver reassembles + verifies hash

mod chunks;
mod meta;
mod wrap;

pub use chunks::{
    check_file_size, decrypt_and_reassemble, encrypt_file, wrapped_key_for, FileChunk,
    DEFAULT_CHUNK_PLAINTEXT_LEN, MAX_FILE_BYTES,
};
pub use meta::{FileMeta, CONTENT_TYPE_FILE_CHUNK, CONTENT_TYPE_FILE_META};
pub use wrap::{
    generate_content_key, seal_content_key, ContentKey, FileRecipientSecrets, RecipientFileKeys,
    WrappedContentKey, FILE_HPKE_HKDF_INFO,
};
