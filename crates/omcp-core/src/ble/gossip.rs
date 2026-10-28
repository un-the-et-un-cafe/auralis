//! Epidemic / gossip seen-cache (PROTOCOL §8.3).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Default `envelope_id` seen-cache TTL (PROTOCOL §8.3): 10 minutes.
pub const SEEN_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

/// Loop-prevention cache: `seen` set per `envelope_id` with TTL expiry.
#[derive(Debug)]
pub struct SeenCache {
    entries: HashMap<[u8; 16], Instant>,
    ttl: Duration,
}

impl Default for SeenCache {
    fn default() -> Self {
        Self::new(SEEN_CACHE_TTL)
    }
}

impl SeenCache {
    /// Create a cache with the given TTL.
    pub fn new(ttl: Duration) -> Self {
        Self {
            entries: HashMap::new(),
            ttl,
        }
    }

    /// Returns `true` if this `envelope_id` was already seen within TTL (should not
    /// be gossiped again). Records the id when first seen.
    pub fn check_and_insert(&mut self, envelope_id: [u8; 16], now: Instant) -> bool {
        if self.contains(&envelope_id, now) {
            return true;
        }
        self.entries.insert(envelope_id, now);
        false
    }

    /// Query without consuming an ID, so fallible local processing can finish
    /// before the caller records success. Expired entries are evicted as usual.
    pub fn contains(&mut self, envelope_id: &[u8; 16], now: Instant) -> bool {
        self.evict_expired(now);
        self.entries.contains_key(envelope_id)
    }

    /// Number of live entries (after optional eviction).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn evict_expired(&mut self, now: Instant) {
        let ttl = self.ttl;
        self.entries
            .retain(|_, first| now.duration_since(*first) < ttl);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seen_cache_detects_duplicate() {
        let mut cache = SeenCache::new(Duration::from_secs(60));
        let now = Instant::now();
        let id = [1u8; 16];
        assert!(!cache.check_and_insert(id, now));
        assert!(cache.check_and_insert(id, now));
    }

    #[test]
    fn querying_does_not_consume_a_failed_processing_attempt() {
        let mut cache = SeenCache::new(Duration::from_secs(60));
        let now = Instant::now();
        let id = [7u8; 16];
        assert!(!cache.contains(&id, now));
        assert!(!cache.contains(&id, now));
        assert!(!cache.check_and_insert(id, now));
        assert!(cache.contains(&id, now));
        assert!(!cache.contains(&id, now + Duration::from_secs(61)));
    }

    #[test]
    fn seen_cache_expires() {
        let mut cache = SeenCache::new(Duration::from_millis(10));
        let id = [2u8; 16];
        let t0 = Instant::now();
        assert!(!cache.check_and_insert(id, t0));
        let t1 = t0 + Duration::from_millis(20);
        assert!(!cache.check_and_insert(id, t1));
    }
}
