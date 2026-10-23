//! Canonical envelope framing (PROTOCOL §10.1, §21.1–§21.4).
//!
//! All multi-byte integers are **big-endian** (PROTOCOL §21.2).

use crate::error::{OmcpError, Result};
use crate::suite::{is_supported_suite, ENVELOPE_VERSION_V1};

/// Magic bytes `b"OMCP"`.
pub const MAGIC: [u8; 4] = *b"OMCP";

/// Fixed header size before variable `header_aad` and ciphertext
/// (magic 4 + version 2 + suite 2 + envelope_id 16 + flags 2 + ttl 1 +
/// created_ms 8 + header_aad_len 2 + ciphertext_len 4 = 41).
pub const ENVELOPE_V1_FIXED_LEN: usize = 41;

/// Maximum accepted ciphertext length (bytes), inclusive of AEAD tag.
///
/// PROTOCOL §21.4: 1_048_576 bytes (1 MiB) maximum.
pub const MAX_CIPHERTEXT_LEN: u32 = 1_048_576;

/// Flag bits for [`EnvelopeV1::flags`] (PROTOCOL §21.3; bit 0 = LSB).
///
/// Bits 3–15 are reserved and MUST be 0 on send.
pub mod flags {
    /// Envelope may be relayed by mesh peers (bit 0).
    pub const RELAY_OK: u16 = 1 << 0;
    /// Envelope may be stored for later delivery (bit 1).
    pub const STORE_OK: u16 = 1 << 1;
    /// Preferential forwarding / scheduling hint (bit 2).
    pub const PRIORITY: u16 = 1 << 2;
}

/// Canonical `EnvelopeV1` (PROTOCOL §10.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvelopeV1 {
    /// Envelope format version (`1` for V1).
    pub version: u16,
    /// Cipher suite id (e.g. [`crate::suite::SUITE_OMCP_2026_A`]).
    pub suite_id: u16,
    /// Unique envelope id (replay detection; PROTOCOL §11).
    pub envelope_id: [u8; 16],
    /// Bit flags (`RELAY_OK`, `STORE_OK`, `PRIORITY`, …).
    pub flags: u16,
    /// Remaining hop budget for mesh relay.
    pub ttl_hops: u8,
    /// Sender-local creation time in milliseconds (ordering hint only).
    pub created_ms: u64,
    /// Cleartext associated data (no secrets).
    pub header_aad: Vec<u8>,
    /// AEAD ciphertext including authentication tag.
    pub ciphertext: Vec<u8>,
}

impl EnvelopeV1 {
    /// Encode to the canonical byte layout (§10.1).
    pub fn encode(&self) -> Result<Vec<u8>> {
        if self.version != ENVELOPE_VERSION_V1 {
            return Err(OmcpError::UnknownVersion(self.version));
        }
        if !is_supported_suite(self.suite_id) {
            return Err(OmcpError::UnsupportedSuite(self.suite_id));
        }
        if self.ciphertext.len() > MAX_CIPHERTEXT_LEN as usize {
            return Err(OmcpError::Oversized {
                len: self.ciphertext.len() as u32,
                max: MAX_CIPHERTEXT_LEN,
            });
        }
        if self.header_aad.len() > u16::MAX as usize {
            return Err(OmcpError::HeaderAadLength {
                declared: u16::MAX,
                remaining: self.header_aad.len(),
            });
        }

        let mut out = Vec::with_capacity(
            ENVELOPE_V1_FIXED_LEN + self.header_aad.len() + self.ciphertext.len(),
        );
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&self.version.to_be_bytes());
        out.extend_from_slice(&self.suite_id.to_be_bytes());
        out.extend_from_slice(&self.envelope_id);
        out.extend_from_slice(&self.flags.to_be_bytes());
        out.push(self.ttl_hops);
        out.extend_from_slice(&self.created_ms.to_be_bytes());
        out.extend_from_slice(&(self.header_aad.len() as u16).to_be_bytes());
        out.extend_from_slice(&self.header_aad);
        out.extend_from_slice(&(self.ciphertext.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.ciphertext);
        Ok(out)
    }

    /// Decode a full buffer; rejects truncated, oversized, bad magic,
    /// unknown version, and unsupported suite ids.
    pub fn decode(buf: &[u8]) -> Result<Self> {
        if buf.len() < ENVELOPE_V1_FIXED_LEN {
            return Err(OmcpError::Truncated {
                need: ENVELOPE_V1_FIXED_LEN,
                have: buf.len(),
            });
        }

        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        if magic != MAGIC {
            return Err(OmcpError::BadMagic(magic));
        }

        let version = u16::from_be_bytes([buf[4], buf[5]]);
        if version != ENVELOPE_VERSION_V1 {
            return Err(OmcpError::UnknownVersion(version));
        }

        let suite_id = u16::from_be_bytes([buf[6], buf[7]]);
        if !is_supported_suite(suite_id) {
            return Err(OmcpError::UnsupportedSuite(suite_id));
        }

        let mut envelope_id = [0u8; 16];
        envelope_id.copy_from_slice(&buf[8..24]);
        let flags = u16::from_be_bytes([buf[24], buf[25]]);
        let ttl_hops = buf[26];
        let created_ms = u64::from_be_bytes(buf[27..35].try_into().unwrap());
        let header_aad_len = u16::from_be_bytes([buf[35], buf[36]]) as usize;

        let aad_start: usize = 37;
        let aad_end = aad_start
            .checked_add(header_aad_len)
            .ok_or(OmcpError::HandshakeMalformed("aad overflow"))?;
        if buf.len() < aad_end + 4 {
            return Err(OmcpError::Truncated {
                need: aad_end + 4,
                have: buf.len(),
            });
        }
        let header_aad = buf[aad_start..aad_end].to_vec();

        let ct_len = u32::from_be_bytes(buf[aad_end..aad_end + 4].try_into().unwrap());
        if ct_len > MAX_CIPHERTEXT_LEN {
            return Err(OmcpError::Oversized {
                len: ct_len,
                max: MAX_CIPHERTEXT_LEN,
            });
        }
        let ct_start = aad_end + 4;
        let ct_end = ct_start
            .checked_add(ct_len as usize)
            .ok_or(OmcpError::HandshakeMalformed("ct overflow"))?;
        if buf.len() < ct_end {
            return Err(OmcpError::Truncated {
                need: ct_end,
                have: buf.len(),
            });
        }
        if buf.len() != ct_end {
            // Trailing bytes are not part of a single envelope; treat as truncate/malformed.
            return Err(OmcpError::Truncated {
                need: ct_end,
                have: buf.len(),
            });
        }

        Ok(Self {
            version,
            suite_id,
            envelope_id,
            flags,
            ttl_hops,
            created_ms,
            header_aad,
            ciphertext: buf[ct_start..ct_end].to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite::SUITE_OMCP_2026_A;

    fn sample() -> EnvelopeV1 {
        EnvelopeV1 {
            version: ENVELOPE_VERSION_V1,
            suite_id: SUITE_OMCP_2026_A,
            envelope_id: [0x11; 16],
            flags: flags::RELAY_OK | flags::STORE_OK,
            ttl_hops: 7,
            created_ms: 1_729_000_000_000,
            header_aad: b"hint".to_vec(),
            ciphertext: vec![0xAA; 32],
        }
    }

    #[test]
    fn round_trip() {
        let env = sample();
        let bytes = env.encode().unwrap();
        let decoded = EnvelopeV1::decode(&bytes).unwrap();
        assert_eq!(decoded, env);
    }

    #[test]
    fn reject_truncated() {
        let bytes = sample().encode().unwrap();
        let err = EnvelopeV1::decode(&bytes[..bytes.len() - 1]).unwrap_err();
        assert!(matches!(err, OmcpError::Truncated { .. }));
    }

    #[test]
    fn reject_truncated_header() {
        let err = EnvelopeV1::decode(&[0u8; 10]).unwrap_err();
        assert!(matches!(err, OmcpError::Truncated { .. }));
    }

    #[test]
    fn reject_oversized_ciphertext_len() {
        let mut env = sample();
        env.ciphertext = vec![0u8; 16];
        let mut bytes = env.encode().unwrap();
        // Overwrite ciphertext_len to claim MAX+1 while keeping short body.
        let aad_len = env.header_aad.len();
        let ct_len_at = 37 + aad_len;
        let bogus = (MAX_CIPHERTEXT_LEN + 1).to_be_bytes();
        bytes[ct_len_at..ct_len_at + 4].copy_from_slice(&bogus);
        // Body still short → either Oversized (checked before body) or Truncated.
        let err = EnvelopeV1::decode(&bytes).unwrap_err();
        assert!(
            matches!(err, OmcpError::Oversized { .. }),
            "expected Oversized, got {err:?}"
        );
    }

    #[test]
    fn reject_unknown_version() {
        let mut bytes = sample().encode().unwrap();
        bytes[4..6].copy_from_slice(&99u16.to_be_bytes());
        assert!(matches!(
            EnvelopeV1::decode(&bytes),
            Err(OmcpError::UnknownVersion(99))
        ));
    }

    #[test]
    fn reject_unsupported_suite() {
        let mut bytes = sample().encode().unwrap();
        bytes[6..8].copy_from_slice(&0xFFFFu16.to_be_bytes());
        assert!(matches!(
            EnvelopeV1::decode(&bytes),
            Err(OmcpError::UnsupportedSuite(0xFFFF))
        ));
    }

    #[test]
    fn reject_bad_magic() {
        let mut bytes = sample().encode().unwrap();
        bytes[0] = b'X';
        assert!(matches!(
            EnvelopeV1::decode(&bytes),
            Err(OmcpError::BadMagic(_))
        ));
    }
}
