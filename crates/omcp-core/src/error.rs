//! Error types for `omcp-core`.

use thiserror::Error;

/// Errors returned by framing, negotiation, handshake, and BLE transport APIs.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OmcpError {
    #[error("buffer truncated: need at least {need} bytes, have {have}")]
    Truncated { need: usize, have: usize },

    #[error("envelope too large: ciphertext_len {len} exceeds max {max}")]
    Oversized { len: u32, max: u32 },

    #[error("header_aad length mismatch: declared {declared}, remaining {remaining}")]
    HeaderAadLength { declared: u16, remaining: usize },

    #[error("bad magic: expected OMCP, got {0:?}")]
    BadMagic([u8; 4]),

    #[error("unsupported envelope version {0}")]
    UnknownVersion(u16),

    #[error("unsupported suite_id {0:#06x}")]
    UnsupportedSuite(u16),

    #[error("no overlapping protocol versions")]
    NoVersionOverlap,

    #[error("no overlapping cipher suites")]
    NoSuiteOverlap,

    #[error("handshake message malformed: {0}")]
    HandshakeMalformed(&'static str),

    #[error("handshake AEAD failure (tamper or wrong keys)")]
    HandshakeAead,

    #[error("handshake in unexpected state")]
    HandshakeState,

    #[error("cryptographic operation failed: {0}")]
    Crypto(&'static str),

    #[error("BLE fragment malformed: {0}")]
    FragmentMalformed(&'static str),

    #[error("BLE link AEAD failure (tamper or wrong keys)")]
    LinkAead,

    #[error("application AEAD failure (tamper, wrong keys, or bad UTF-8)")]
    AppAead,
}

/// Crate-wide result alias.
pub type Result<T> = core::result::Result<T, OmcpError>;
