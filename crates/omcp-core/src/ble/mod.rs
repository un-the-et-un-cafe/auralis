//! BLE mesh transport helpers (PROTOCOL §8, §10.2) — M2 loopback.
//!
//! This module provides a **faithful in-process / loopback** BLE link layer that
//! exercises OMCP framing without real BlueZ / phone hardware:
//!
//! - GATT role simulation: Inbox write + Outbox notify over an in-memory duplex
//! - Fragment codec + 30 s reassembly timeout (§10.2)
//! - Per-link session AEAD from completed `OMCP-Noise-Hybrid-v1` keys (§10.2)
//! - Discovery advertisement struct (§8.2)
//! - Gossip / seen-cache for `envelope_id` (§8.3; default TTL 10 minutes)
//!
//! **Not production BLE hardware readiness.** Real Android/iOS bindings are out of
//! scope for M2 (see roadmap M5/M6).
//!
//! # Wire choices pinned for M2 (PROTOCOL ambiguities)
//!
//! PROTOCOL §10.2 names fields but not a byte layout. M2 pins:
//!
//! ```text
//! BleFrame {
//!   frame_len:   u16 BE   // bytes that follow (header + ciphertext)
//!   frag_id:     u64 BE   // random per logical message
//!   frag_index:  u16 BE   // 0-based
//!   frag_count:  u16 BE   // >= 1
//!   ciphertext:  [..]     // ChaCha20-Poly1305(chunk) + 16-byte tag
//! }
//! ```
//!
//! - Default cleartext chunk size before link AEAD: **512** bytes (§8.1).
//! - Link AEAD key: directional Noise transport key (`k_i2r` / `k_r2i`).
//! - Link nonce: Noise `CipherState` layout (4 zero bytes ‖ u64 LE counter).
//! - AAD for link AEAD: `frag_id ‖ frag_index ‖ frag_count` (big-endian fields).
//! - Application AEAD (EnvelopeV1 ciphertext) uses keys derived via
//!   `HKDF-SHA-384(salt = directional_transport_key, ikm = [], info =
//!   "OMCP-App-AEAD-v1", L = 32)` so link and app nonce spaces never collide.
//! - Application AAD (§6.1): `suite_id(u16 BE) ‖ sender_peer_id(32) ‖
//!   msg_id(16) ‖ timestamp_ms(u64 BE) ‖ content_type(u8)`.
//! - Content type for UTF-8 text chat: **`0x01`** (`text/plain; charset=utf-8`,
//!   PROTOCOL §6.3).
//! - Provisional PeerID for M2: the peer's static X25519 public key (32 bytes).
//!   Full `multihash(Ed25519 pk)` PeerID lands when identity signing keys do.
//! - Service UUID (placeholder §8.1): `a0mc0001-0mcp-4c01-9e00-000000000001`.

pub mod discovery;
pub mod fragment;
pub mod gossip;
pub mod link;
pub mod loopback;
pub mod message;

pub use discovery::{capabilities, DiscoveryAdvertisement, SERVICE_UUID_STR};
pub use fragment::{
    FragmentHeader, FragmentReassembler, DEFAULT_FRAGMENT_PAYLOAD, FRAGMENT_HEADER_LEN,
    REASSEMBLY_TIMEOUT,
};
pub use gossip::{SeenCache, SEEN_CACHE_TTL};
pub use link::{Direction, LinkSession};
pub use loopback::{BleEndpoint, BleLoopback};
pub use message::{
    open_text_envelope, seal_text_envelope, APP_AEAD_HKDF_INFO, CONTENT_TYPE_TEXT_PLAIN,
};
