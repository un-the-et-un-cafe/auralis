//! Allowed-gateway pinning (PROTOCOL §9.4).
//!
//! Empty pin list = any `GATEWAY` neighbour may carry transport, preferring
//! verified contacts. Non-empty = **only** pinned gateways are used.

use super::control::GatewayOffering;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayPinList {
    pinned: Vec<[u8; 32]>,
}

impl GatewayPinList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pin(&mut self, peer_id: [u8; 32]) {
        if !self.pinned.contains(&peer_id) {
            self.pinned.push(peer_id);
        }
    }

    pub fn unpin(&mut self, peer_id: &[u8; 32]) {
        self.pinned.retain(|p| p != peer_id);
    }

    pub fn is_pinning(&self) -> bool {
        !self.pinned.is_empty()
    }

    /// Whether `peer_id` may carry this user's traffic.
    pub fn allows(&self, peer_id: &[u8; 32]) -> bool {
        self.pinned.is_empty() || self.pinned.contains(peer_id)
    }

    /// Choose a gateway: allowed + has budget; prefer verified contacts, then
    /// most remaining bytes.
    pub fn select<'a>(
        &self,
        offers: &'a [GatewayOffering],
        verified_contacts: &[[u8; 32]],
    ) -> Option<&'a GatewayOffering> {
        offers
            .iter()
            .filter(|o| self.allows(&o.gateway_peer_id) && o.remaining_bytes_today > 0)
            .max_by_key(|o| {
                (
                    verified_contacts.contains(&o.gateway_peer_id),
                    o.remaining_bytes_today,
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(id: u8, rem: u64) -> GatewayOffering {
        GatewayOffering {
            gateway_peer_id: [id; 32],
            remaining_bytes_today: rem,
            queue_free: 1,
            flags: 0,
        }
    }

    #[test]
    fn pinning_and_preference() {
        let offers = [offer(1, 100), offer(2, 10), offer(3, 0)];
        let mut p = GatewayPinList::new();
        assert_eq!(p.select(&offers, &[]).unwrap().gateway_peer_id, [1; 32]);
        assert_eq!(
            p.select(&offers, &[[2; 32]]).unwrap().gateway_peer_id,
            [2; 32]
        );
        p.pin([3; 32]);
        assert!(p.select(&offers, &[]).is_none()); // pinned one has no budget
        p.pin([2; 32]);
        assert_eq!(p.select(&offers, &[]).unwrap().gateway_peer_id, [2; 32]);
        assert!(!p.allows(&[1; 32]));
    }
}
