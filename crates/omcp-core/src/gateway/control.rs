//! `GATEWAY_BUSY` / `GATEWAY_OFFERING` control messages (PROTOCOL §9.1.4).
//!
//! Wire (big-endian):
//! ```text
//! BUSY     = 0x01 | reason u8 | retry_after_ms u64                          (10 B)
//! OFFERING = 0x02 | gateway_peer_id(32) | remaining_bytes_today u64 |
//!            queue_free u16 | flags u8 (bit0 RENDEZVOUS, bit1 METERED)      (44 B)
//! ```
//! These are local-mesh control messages; they carry no user content.

use crate::error::{OmcpError, Result};

pub const TYPE_BUSY: u8 = 0x01;
pub const TYPE_OFFERING: u8 = 0x02;
pub const OFFER_FLAG_RENDEZVOUS: u8 = 1 << 0;
pub const OFFER_FLAG_METERED: u8 = 1 << 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BusyReason {
    DailyCapReached = 1,
    QueueFull = 2,
    Offline = 3,
    PolicyNetwork = 4,
}

/// Gateway advertisement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayOffering {
    pub gateway_peer_id: [u8; 32],
    pub remaining_bytes_today: u64,
    pub queue_free: u16,
    pub flags: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayControl {
    Busy {
        reason: BusyReason,
        retry_after_ms: u64,
    },
    Offering(GatewayOffering),
}

impl GatewayControl {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Busy {
                reason,
                retry_after_ms,
            } => {
                let mut b = vec![TYPE_BUSY, *reason as u8];
                b.extend_from_slice(&retry_after_ms.to_be_bytes());
                b
            }
            Self::Offering(o) => {
                let mut b = vec![TYPE_OFFERING];
                b.extend_from_slice(&o.gateway_peer_id);
                b.extend_from_slice(&o.remaining_bytes_today.to_be_bytes());
                b.extend_from_slice(&o.queue_free.to_be_bytes());
                b.push(o.flags);
                b
            }
        }
    }

    pub fn decode(buf: &[u8]) -> Result<Self> {
        match buf.first() {
            Some(&TYPE_BUSY) if buf.len() == 10 => {
                let reason = match buf[1] {
                    1 => BusyReason::DailyCapReached,
                    2 => BusyReason::QueueFull,
                    3 => BusyReason::Offline,
                    4 => BusyReason::PolicyNetwork,
                    _ => return Err(OmcpError::Gateway("unknown busy reason")),
                };
                Ok(Self::Busy {
                    reason,
                    retry_after_ms: u64::from_be_bytes(buf[2..10].try_into().unwrap()),
                })
            }
            Some(&TYPE_OFFERING) if buf.len() == 44 => {
                let mut id = [0u8; 32];
                id.copy_from_slice(&buf[1..33]);
                Ok(Self::Offering(GatewayOffering {
                    gateway_peer_id: id,
                    remaining_bytes_today: u64::from_be_bytes(buf[33..41].try_into().unwrap()),
                    queue_free: u16::from_be_bytes([buf[41], buf[42]]),
                    flags: buf[43] & (OFFER_FLAG_RENDEZVOUS | OFFER_FLAG_METERED),
                }))
            }
            _ => Err(OmcpError::Gateway("malformed control message")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_roundtrip() {
        for m in [
            GatewayControl::Busy {
                reason: BusyReason::DailyCapReached,
                retry_after_ms: 99,
            },
            GatewayControl::Offering(GatewayOffering {
                gateway_peer_id: [5; 32],
                remaining_bytes_today: 1234,
                queue_free: 7,
                flags: OFFER_FLAG_RENDEZVOUS,
            }),
        ] {
            assert_eq!(GatewayControl::decode(&m.encode()).unwrap(), m);
        }
        assert!(GatewayControl::decode(&[0x09]).is_err());
    }
}
