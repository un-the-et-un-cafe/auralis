//! Version / suite negotiation hooks (PROTOCOL §5.2, §10.3).

use crate::error::{OmcpError, Result};
use crate::suite::{is_supported_suite, supported_suites, supported_versions, ENVELOPE_VERSION_V1};

/// `Hello` control payload after handshake (PROTOCOL §10.3).
///
/// Wire encoding of this struct is deferred; M1 only provides selection logic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    /// Advertised protocol / envelope versions.
    pub versions: Vec<u16>,
    /// Advertised suite ids.
    pub suites: Vec<u16>,
    /// Feature bits / names; opaque for M1 (e.g. `"PQ"`, `"MLS"`, `"GATEWAY"`).
    pub features: Vec<String>,
}

impl Hello {
    /// Local hello advertising this build's capabilities.
    pub fn local(features: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            versions: supported_versions().to_vec(),
            suites: supported_suites().to_vec(),
            features: features.into_iter().map(Into::into).collect(),
        }
    }
}

/// Result of initiator-side negotiation (PROTOCOL §10.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Negotiated {
    /// Chosen protocol version.
    pub version: u16,
    /// Chosen suite id.
    pub suite_id: u16,
}

/// Initiator chooses the first locally preferred version that the peer also
/// offers, then the first locally preferred suite the peer also offers.
pub fn negotiate(initiator: &Hello, peer: &Hello) -> Result<Negotiated> {
    let version = initiator
        .versions
        .iter()
        .find(|v| peer.versions.contains(v))
        .copied()
        .ok_or(OmcpError::NoVersionOverlap)?;

    let suite_id = initiator
        .suites
        .iter()
        .find(|s| peer.suites.contains(s) && is_supported_suite(**s))
        .copied()
        .ok_or(OmcpError::NoSuiteOverlap)?;

    Ok(Negotiated { version, suite_id })
}

/// Validate a peer hello has at least one usable version (envelope v1).
pub fn peer_supports_v1(peer: &Hello) -> bool {
    peer.versions.contains(&ENVELOPE_VERSION_V1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::suite::SUITE_OMCP_2026_A;

    #[test]
    fn negotiate_picks_overlap() {
        let init = Hello::local(["PQ"]);
        let peer = Hello {
            versions: vec![1, 2],
            suites: vec![0x0099, SUITE_OMCP_2026_A],
            features: vec!["PQ".into()],
        };
        let n = negotiate(&init, &peer).unwrap();
        assert_eq!(n.version, 1);
        assert_eq!(n.suite_id, SUITE_OMCP_2026_A);
    }

    #[test]
    fn negotiate_rejects_empty_overlap() {
        let init = Hello::local([] as [&str; 0]);
        let peer = Hello {
            versions: vec![99],
            suites: vec![SUITE_OMCP_2026_A],
            features: vec![],
        };
        assert!(matches!(
            negotiate(&init, &peer),
            Err(OmcpError::NoVersionOverlap)
        ));
    }
}
