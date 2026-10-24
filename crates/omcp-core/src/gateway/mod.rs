//! Internet gateway role (PROTOCOL §9, §8.4) — **experimental (M4)**.
//!
//! A gateway bridges a local BLE mesh to the Internet by forwarding
//! [`OpaqueEnvelope`]s only. It holds **no user content keys** and exposes no
//! decrypt API: the type system makes "never decrypts" structural (§9.1.3).
//!
//! - [`policy`]: local data / network policy (500 MB/day cap, Wi-Fi-only
//!   default, warn-not-block on metered cellular when Wi-Fi-only is off).
//! - [`control`]: `GATEWAY_BUSY` / `GATEWAY_OFFERING` control messages (§9.1.4).
//! - [`pin`]: client-side allowed-gateway pin list + selection (§9.4).
//! - [`store`]: store-and-forward retention (default 72 h, max 168 h; §8.4).
//! - [`node`]: the gateway state machine: queue, outage/drain (§9.3).

pub mod control;
pub mod node;
pub mod pin;
pub mod policy;
pub mod store;

pub use control::{BusyReason, GatewayControl, GatewayOffering};
pub use node::{Gateway, PumpReport, SubmitOutcome};
pub use pin::GatewayPinList;
pub use policy::{GatewayPolicy, NetworkDecision, NetworkKind, DEFAULT_DAILY_BYTE_CAP};
pub use store::{StoreAndForward, DEFAULT_RETENTION_HOURS, MAX_RETENTION_HOURS};

use crate::error::{OmcpError, Result};
use crate::framing::MAX_CIPHERTEXT_LEN;
use crate::rendezvous::BlindedLabel;

/// Magic for opaque gateway envelopes.
pub const OPAQUE_MAGIC: [u8; 4] = *b"OMGE";
/// Fixed header: magic 4 + version 1 + label 32 + suite 2 + expiry 8 + len 4.
pub const OPAQUE_FIXED_LEN: usize = 51;
/// Max opaque payload: one max-size `EnvelopeV1` (§21.4) plus framing/seal slack.
pub const MAX_OPAQUE_CIPHERTEXT_LEN: u32 = MAX_CIPHERTEXT_LEN + 64 * 1024;

/// What a gateway forwards (§9.1.2): `{ sealed_address, suite_id, ciphertext, expiry }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpaqueEnvelope {
    /// Blinded destination label (sealed address); never the PeerID.
    pub dest_label: BlindedLabel,
    /// Suite of the inner content (informational for the recipient).
    pub suite_id: u16,
    /// Absolute expiry (ms since Unix epoch).
    pub expiry_ms: u64,
    /// Sealed-sender ciphertext; opaque to gateways.
    pub ciphertext: Vec<u8>,
}

impl OpaqueEnvelope {
    /// Encoded length in bytes (what counts against the data cap).
    pub fn wire_len(&self) -> usize {
        OPAQUE_FIXED_LEN + self.ciphertext.len()
    }

    /// Big-endian encoding (§21.2).
    pub fn encode(&self) -> Result<Vec<u8>> {
        let len = u32::try_from(self.ciphertext.len()).unwrap_or(u32::MAX);
        if len > MAX_OPAQUE_CIPHERTEXT_LEN {
            return Err(OmcpError::Oversized {
                len,
                max: MAX_OPAQUE_CIPHERTEXT_LEN,
            });
        }
        let mut b = Vec::with_capacity(self.wire_len());
        b.extend_from_slice(&OPAQUE_MAGIC);
        b.push(1);
        b.extend_from_slice(&self.dest_label);
        b.extend_from_slice(&self.suite_id.to_be_bytes());
        b.extend_from_slice(&self.expiry_ms.to_be_bytes());
        b.extend_from_slice(&len.to_be_bytes());
        b.extend_from_slice(&self.ciphertext);
        Ok(b)
    }

    /// Decode; rejects oversize before allocating (§21.4).
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < OPAQUE_FIXED_LEN {
            return Err(OmcpError::Truncated {
                need: OPAQUE_FIXED_LEN,
                have: buf.len(),
            });
        }
        if buf[..4] != OPAQUE_MAGIC {
            return Err(OmcpError::BadMagic([buf[0], buf[1], buf[2], buf[3]]));
        }
        if buf[4] != 1 {
            return Err(OmcpError::UnknownVersion(buf[4] as u16));
        }
        let mut dest_label = [0u8; 32];
        dest_label.copy_from_slice(&buf[5..37]);
        let suite_id = u16::from_be_bytes([buf[37], buf[38]]);
        let expiry_ms = u64::from_be_bytes(buf[39..47].try_into().unwrap());
        let len = u32::from_be_bytes(buf[47..51].try_into().unwrap());
        if len > MAX_OPAQUE_CIPHERTEXT_LEN {
            return Err(OmcpError::Oversized {
                len,
                max: MAX_OPAQUE_CIPHERTEXT_LEN,
            });
        }
        let end = OPAQUE_FIXED_LEN + len as usize;
        if buf.len() != end {
            return Err(OmcpError::Gateway("opaque envelope length mismatch"));
        }
        Ok(Self {
            dest_label,
            suite_id,
            expiry_ms,
            ciphertext: buf[OPAQUE_FIXED_LEN..end].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_roundtrip_and_rejects() {
        let e = OpaqueEnvelope {
            dest_label: [7; 32],
            suite_id: 1,
            expiry_ms: 42,
            ciphertext: vec![1, 2, 3],
        };
        let b = e.encode().unwrap();
        assert_eq!(b.len(), e.wire_len());
        assert_eq!(OpaqueEnvelope::decode(&b).unwrap(), e);
        assert!(OpaqueEnvelope::decode(&b[..b.len() - 1]).is_err());
        let mut big = b.clone();
        big[47..51].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(matches!(
            OpaqueEnvelope::decode(&big),
            Err(OmcpError::Oversized { .. })
        ));
    }
}
