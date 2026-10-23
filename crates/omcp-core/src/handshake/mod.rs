//! Handshake constructions for suite `OMCP-2026-A`.

mod hybrid;
mod symmetric;

pub use hybrid::{
    HybridInitiator, HybridResponder, HybridSessionKeys, StaticIdentity, HYBRID_HKDF_INFO,
    HYBRID_PROTOCOL_NAME, MLKEM768_CT_LEN, MLKEM768_EK_LEN, MLKEM768_SS_LEN,
};
