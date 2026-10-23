//! Open Mesh Chat Protocol (`omcp-core`) — M1 core library.
//!
//! Provides:
//! - Canonical [`framing::EnvelopeV1`] encode/decode (PROTOCOL §10.1)
//! - Version / suite negotiation hooks (PROTOCOL §5.2, §10.3)
//! - Noise_IK + ML-KEM-768 hybrid handshake for suite `OMCP-2026-A` (PROTOCOL §5.1)
//!
//! Not in scope for M1: BLE transport, MLS groups, apps, gateway.

#![forbid(unsafe_code)]
#![allow(missing_docs)] // field-level docs filled as API stabilizes post-M1

pub mod error;
pub mod framing;
pub mod handshake;
pub mod negotiate;
pub mod suite;

pub use error::{OmcpError, Result};
pub use framing::EnvelopeV1;
pub use handshake::{
    HybridInitiator, HybridResponder, HybridSessionKeys, StaticIdentity, HYBRID_PROTOCOL_NAME,
};
pub use negotiate::{negotiate, Hello, Negotiated};
pub use suite::{SUITE_OMCP_2026_A, SUITE_OMCP_2026_A_NAME};
