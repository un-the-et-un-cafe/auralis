//! Cipher suite identifiers for OMCP (PROTOCOL §5, §21.1).

/// Suite id for mandatory suite `OMCP-2026-A` (PROTOCOL §21.1).
pub const SUITE_OMCP_2026_A: u16 = 0x0001;

/// Human-readable name for [`SUITE_OMCP_2026_A`].
pub const SUITE_OMCP_2026_A_NAME: &str = "OMCP-2026-A";

/// Envelope format version for [`crate::framing::EnvelopeV1`] (PROTOCOL §10.1).
pub const ENVELOPE_VERSION_V1: u16 = 1;

/// Returns true if `suite_id` is known to this implementation.
pub fn is_supported_suite(suite_id: u16) -> bool {
    suite_id == SUITE_OMCP_2026_A
}

/// Preference-ordered list of suites this build supports (initiator picks
/// intersection; PROTOCOL §10.3).
pub fn supported_suites() -> &'static [u16] {
    &[SUITE_OMCP_2026_A]
}

/// Preference-ordered list of envelope/protocol versions this build supports.
pub fn supported_versions() -> &'static [u16] {
    &[ENVELOPE_VERSION_V1]
}
