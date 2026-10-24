//! Cuckoo-filter interest digests (PROTOCOL §22.1).
//!
//! 16-bit fingerprints, 4 slots per bucket, power-of-two bucket count. Each
//! advertisement carries a fresh 8-byte salt so fingerprints are not linkable
//! across adverts, and is padded with random decoy entries up to a fixed
//! occupancy class so neighbours learn neither the real count nor the tags.
//! Wire: `salt[8] | log2_buckets u8 | slots[buckets*4] u16-be` (0 = empty).

use super::tag::MailboxTag;
use super::{take, SplitMix64};
use crate::error::{OmcpError, Result};

/// Slots per bucket.
pub const SLOTS: usize = 4;
/// Max kicks before an insert fails.
const MAX_KICKS: usize = 500;
/// Largest accepted filter on the wire (2^12 buckets ≈ 32 KiB).
pub const MAX_LOG2_BUCKETS: u8 = 12;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CuckooFilter {
    salt: u64,
    log2: u8,
    slots: Vec<u16>,
    count: usize,
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl CuckooFilter {
    /// Empty filter with `2^log2` buckets.
    pub fn new(log2: u8, salt: u64) -> Result<Self> {
        if log2 == 0 || log2 > MAX_LOG2_BUCKETS {
            return Err(OmcpError::Sync("cuckoo size"));
        }
        Ok(Self {
            salt,
            log2,
            slots: vec![0; (1usize << log2) * SLOTS],
            count: 0,
        })
    }

    /// Smallest filter for `n` entries at ≤ 85 % load.
    pub fn for_capacity(n: usize, salt: u64) -> Result<Self> {
        let mut log2 = 1u8;
        while (((1usize << log2) * SLOTS) as f64) * 0.85 < n as f64 {
            log2 += 1;
        }
        Self::new(log2, salt)
    }

    fn mask(&self) -> usize {
        (1usize << self.log2) - 1
    }

    fn fp_and_index(&self, tag: &MailboxTag) -> (u16, usize) {
        let h = mix(u64::from_be_bytes(*tag) ^ self.salt);
        let mut fp = (h >> 48) as u16;
        if fp == 0 {
            fp = 1;
        }
        (fp, (h as usize) & self.mask())
    }

    fn alt(&self, i: usize, fp: u16) -> usize {
        (i ^ (mix(fp as u64) as usize)) & self.mask()
    }

    fn bucket(&mut self, i: usize) -> &mut [u16] {
        &mut self.slots[i * SLOTS..(i + 1) * SLOTS]
    }

    fn put(&mut self, i: usize, fp: u16) -> bool {
        if let Some(s) = self.bucket(i).iter_mut().find(|s| **s == 0) {
            *s = fp;
            true
        } else {
            false
        }
    }

    fn insert_fp(&mut self, fp: u16, i1: usize, rng: &mut SplitMix64) -> Result<()> {
        let i2 = self.alt(i1, fp);
        if self.put(i1, fp) || self.put(i2, fp) {
            self.count += 1;
            return Ok(());
        }
        let mut i = if rng.chance(0.5) { i1 } else { i2 };
        let mut fp = fp;
        for _ in 0..MAX_KICKS {
            let j = rng.below(SLOTS as u64) as usize;
            std::mem::swap(&mut fp, &mut self.bucket(i)[j]);
            i = self.alt(i, fp);
            if self.put(i, fp) {
                self.count += 1;
                return Ok(());
            }
        }
        Err(OmcpError::Sync("cuckoo full"))
    }

    /// Insert a wanted tag.
    pub fn insert(&mut self, tag: &MailboxTag, rng: &mut SplitMix64) -> Result<()> {
        let (fp, i) = self.fp_and_index(tag);
        self.insert_fp(fp, i, rng)
    }

    /// Membership test (false positives possible, no false negatives).
    pub fn contains(&self, tag: &MailboxTag) -> bool {
        let (fp, i1) = self.fp_and_index(tag);
        let i2 = self.alt(i1, fp);
        let b = |i: usize| self.slots[i * SLOTS..(i + 1) * SLOTS].contains(&fp);
        b(i1) || b(i2)
    }

    /// Pad with random decoy fingerprints until occupancy reaches `target`
    /// (callers use a fixed class, e.g. 80 % of slots).
    pub fn pad_with_decoys(&mut self, target: usize, rng: &mut SplitMix64) -> Result<()> {
        while self.count < target {
            let fp = (rng.next_u64() as u16).max(1);
            let i = rng.below(1 << self.log2) as usize;
            self.insert_fp(fp, i, rng)?;
        }
        Ok(())
    }

    /// Build a padded digest: `tags` padded with decoys to 80 % of the
    /// smallest class (≥ 16 buckets) holding `max(tags, min_entries)`. A
    /// failed insert can drop a kicked entry, so on failure the whole filter
    /// is rebuilt one size class up — never a false negative.
    pub fn build_digest(
        tags: &[MailboxTag],
        min_entries: usize,
        rng: &mut SplitMix64,
    ) -> Result<Self> {
        let mut log2 = Self::for_capacity(tags.len().max(min_entries), 0)?
            .log2
            .max(4);
        loop {
            let mut f = Self::new(log2, rng.next_u64())?;
            let target = f.slots.len() * 4 / 5;
            let ok = tags.iter().try_for_each(|t| f.insert(t, rng)).is_ok()
                && f.pad_with_decoys(target, rng).is_ok();
            if ok {
                return Ok(f);
            }
            if log2 >= MAX_LOG2_BUCKETS {
                return Err(OmcpError::Sync("cuckoo full"));
            }
            log2 += 1;
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn encoded_len(&self) -> usize {
        9 + self.slots.len() * 2
    }

    /// Short id so a reconnecting neighbour can say "I still have digest X".
    pub fn digest_id(&self) -> [u8; 8] {
        let h = blake3::hash(&self.encode());
        let mut id = [0u8; 8];
        id.copy_from_slice(&h.as_bytes()[..8]);
        id
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.encoded_len());
        v.extend_from_slice(&self.salt.to_be_bytes());
        v.push(self.log2);
        for s in &self.slots {
            v.extend_from_slice(&s.to_be_bytes());
        }
        v
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let salt = u64::from_be_bytes(take(&mut buf, 8)?.try_into().expect("8"));
        let log2 = take(&mut buf, 1)?[0];
        let mut f = Self::new(log2, salt)?;
        let raw = take(&mut buf, f.slots.len() * 2)?;
        if !buf.is_empty() {
            return Err(OmcpError::Sync("cuckoo trailing bytes"));
        }
        for (k, c) in raw.as_chunks::<2>().0.iter().enumerate() {
            f.slots[k] = u16::from_be_bytes([c[0], c[1]]);
        }
        f.count = f.slots.iter().filter(|s| **s != 0).count();
        Ok(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_false_negatives_low_fpr_and_padding() {
        let mut rng = SplitMix64(1);
        let tags: Vec<MailboxTag> = (0..60u64).map(|i| (i * 7919 + 3).to_be_bytes()).collect();
        let f = CuckooFilter::build_digest(&tags, 64, &mut rng).unwrap();
        assert!(tags.iter().all(|t| f.contains(t)));
        // Padded to the 80 % class regardless of the real count.
        let g = CuckooFilter::build_digest(&tags[..3], 64, &mut rng).unwrap();
        assert_eq!(f.len(), g.len());
        assert_eq!(f.encoded_len(), g.encoded_len());
        let fp = (1_000_000u64..1_100_000)
            .filter(|i| f.contains(&i.to_be_bytes()))
            .count();
        assert!(fp < 1000, "fpr too high: {fp}/100000");
        let d = CuckooFilter::decode(&f.encode()).unwrap();
        assert_eq!(d, f);
        assert!(CuckooFilter::decode(&f.encode()[..20]).is_err());
        assert!(CuckooFilter::new(0, 0).is_err());
    }
}
