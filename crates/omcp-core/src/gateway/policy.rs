//! Local gateway policy (PROTOCOL §9.1.4 — "rate limits and battery policies are local").
//!
//! Product defaults: 500 MB/day cap (decimal MB = 500 000 000 bytes, counted on
//! encoded opaque envelopes up + down), Wi-Fi-only on. If the user disables
//! Wi-Fi-only, metered cellular is **allowed with a warning**, never hard-blocked.

/// Default daily byte cap: 500 MB (decimal).
pub const DEFAULT_DAILY_BYTE_CAP: u64 = 500 * 1000 * 1000;
/// Length of an accounting day (UTC days since epoch).
pub const DAY_MS: u64 = 86_400_000;

/// Current uplink as reported by the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkKind {
    /// No Internet — gateway stops bridging (§9.3).
    Offline,
    /// Wi-Fi / Ethernet (unmetered unless the OS says otherwise).
    Wifi { metered: bool },
    /// Cellular data.
    Cellular { metered: bool },
}

/// Result of evaluating the policy against the current network.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkDecision {
    /// Whether bridging may happen now.
    pub bridge: bool,
    /// Show a "metered connection" warning to the user.
    pub metered_warning: bool,
}

/// Gateway policy knobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayPolicy {
    pub daily_byte_cap: u64,
    pub wifi_only: bool,
    /// Bound on queued outbound envelopes.
    pub max_queue_len: usize,
}

impl Default for GatewayPolicy {
    fn default() -> Self {
        Self {
            daily_byte_cap: DEFAULT_DAILY_BYTE_CAP,
            wifi_only: true,
            max_queue_len: 1024,
        }
    }
}

impl GatewayPolicy {
    /// Evaluate whether to bridge on `net`.
    pub fn evaluate(&self, net: NetworkKind) -> NetworkDecision {
        match net {
            NetworkKind::Offline => NetworkDecision {
                bridge: false,
                metered_warning: false,
            },
            NetworkKind::Wifi { metered } => NetworkDecision {
                bridge: true,
                metered_warning: metered,
            },
            NetworkKind::Cellular { metered } => NetworkDecision {
                bridge: !self.wifi_only,
                metered_warning: metered,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_metered_warns_not_blocks() {
        let p = GatewayPolicy::default();
        assert_eq!(p.daily_byte_cap, 500_000_000);
        assert!(p.wifi_only);
        assert!(!p.evaluate(NetworkKind::Cellular { metered: true }).bridge);
        assert!(!p.evaluate(NetworkKind::Offline).bridge);
        let p2 = GatewayPolicy {
            wifi_only: false,
            ..p
        };
        let d = p2.evaluate(NetworkKind::Cellular { metered: true });
        assert!(d.bridge && d.metered_warning);
        let d = p2.evaluate(NetworkKind::Wifi { metered: false });
        assert!(d.bridge && !d.metered_warning);
    }
}
