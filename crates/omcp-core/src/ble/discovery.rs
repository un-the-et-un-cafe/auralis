//! BLE discovery advertisement (PROTOCOL §8.2).

/// Placeholder GATT Service UUID (PROTOCOL §8.1). Finalize before ship.
pub const SERVICE_UUID_STR: &str = "a0mc0001-0mcp-4c01-9e00-000000000001";

/// Capability flag bits in the advertisement `capabilities` byte (PROTOCOL §8.2).
pub mod capabilities {
    /// Peer may bridge opaque envelopes to the Internet.
    pub const GATEWAY: u8 = 1 << 0;
    /// Peer retains envelopes for store-and-forward.
    pub const STORE_FORWARD: u8 = 1 << 1;
    /// Peer supports MLS group messaging.
    pub const MLS: u8 = 1 << 2;
    /// Peer advertises post-quantum hybrid readiness.
    pub const PQ: u8 = 1 << 3;
}

/// Minimal discovery advertisement payload (PROTOCOL §8.2).
///
/// BLE advertisements include: magic `OMCP`, version, truncated PeerID (first 6
/// bytes), capability flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryAdvertisement {
    /// Protocol magic (`b"OMCP"`).
    pub magic: [u8; 4],
    /// Advertised protocol / envelope version.
    pub version: u16,
    /// First 6 bytes of PeerID.
    pub peer_id_trunc: [u8; 6],
    /// Capability flags ([`capabilities`]).
    pub capabilities: u8,
}

impl DiscoveryAdvertisement {
    /// Build an advertisement from a full 32-byte PeerID.
    pub fn new(version: u16, peer_id: &[u8; 32], capabilities: u8) -> Self {
        let mut peer_id_trunc = [0u8; 6];
        peer_id_trunc.copy_from_slice(&peer_id[..6]);
        Self {
            magic: *b"OMCP",
            version,
            peer_id_trunc,
            capabilities,
        }
    }

    /// Encode to the compact advertisement byte layout used by M2 loopback.
    ///
    /// Layout: `magic(4) ‖ version(u16 BE) ‖ peer_id_trunc(6) ‖ capabilities(1)` = 13 bytes.
    pub fn encode(&self) -> [u8; 13] {
        let mut out = [0u8; 13];
        out[0..4].copy_from_slice(&self.magic);
        out[4..6].copy_from_slice(&self.version.to_be_bytes());
        out[6..12].copy_from_slice(&self.peer_id_trunc);
        out[12] = self.capabilities;
        out
    }

    /// Decode a 13-byte advertisement; rejects bad magic / wrong length.
    pub fn decode(buf: &[u8]) -> crate::Result<Self> {
        if buf.len() != 13 {
            return Err(crate::OmcpError::Truncated {
                need: 13,
                have: buf.len(),
            });
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&buf[0..4]);
        if magic != *b"OMCP" {
            return Err(crate::OmcpError::BadMagic(magic));
        }
        let version = u16::from_be_bytes([buf[4], buf[5]]);
        let mut peer_id_trunc = [0u8; 6];
        peer_id_trunc.copy_from_slice(&buf[6..12]);
        Ok(Self {
            magic,
            version,
            peer_id_trunc,
            capabilities: buf[12],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_round_trip() {
        let peer = [0xABu8; 32];
        let ad = DiscoveryAdvertisement::new(1, &peer, capabilities::PQ | capabilities::GATEWAY);
        let bytes = ad.encode();
        let decoded = DiscoveryAdvertisement::decode(&bytes).unwrap();
        assert_eq!(decoded, ad);
        assert_eq!(&decoded.peer_id_trunc, &peer[..6]);
        assert_eq!(SERVICE_UUID_STR, "a0mc0001-0mcp-4c01-9e00-000000000001");
    }
}
