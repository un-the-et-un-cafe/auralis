//! Delta ops and batched receipts (PROTOCOL §22.3).
//!
//! Edits, reactions and receipts are small ops (≤ [`MAX_OP_LEN`] bytes)
//! referencing a message id. Receipts and typing indicators are batched to at
//! most one flush per [`BATCH_INTERVAL_MS`] per chat and sent with
//! `ttl_hops ≤` [`RECEIPT_MAX_HOPS`].

use std::collections::BTreeMap;

use super::take;
use crate::error::{OmcpError, Result};

pub const MAX_OP_LEN: usize = 64;
pub const BATCH_INTERVAL_MS: u64 = 5_000;
pub const RECEIPT_MAX_HOPS: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum OpKind {
    Edit = 1,
    Reaction = 2,
    Delivered = 3,
    Read = 4,
    Typing = 5,
}

impl OpKind {
    /// Batched (receipt-like) ops.
    pub fn batched(self) -> bool {
        matches!(self, Self::Delivered | Self::Read | Self::Typing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeltaOp {
    pub kind: OpKind,
    pub target: [u8; 16],
    pub body: Vec<u8>,
}

impl DeltaOp {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let len = 2 + 16 + self.body.len();
        if len > MAX_OP_LEN {
            return Err(OmcpError::Sync("delta op > 64 bytes"));
        }
        let mut v = Vec::with_capacity(len);
        v.push(self.kind as u8);
        v.push(self.body.len() as u8);
        v.extend_from_slice(&self.target);
        v.extend_from_slice(&self.body);
        Ok(v)
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        if buf.len() > MAX_OP_LEN {
            return Err(OmcpError::Sync("delta op > 64 bytes"));
        }
        let kind = match take(&mut buf, 1)?[0] {
            1 => OpKind::Edit,
            2 => OpKind::Reaction,
            3 => OpKind::Delivered,
            4 => OpKind::Read,
            5 => OpKind::Typing,
            _ => return Err(OmcpError::Sync("op kind")),
        };
        let n = take(&mut buf, 1)?[0] as usize;
        let target = take(&mut buf, 16)?.try_into().expect("16");
        let body = take(&mut buf, n)?.to_vec();
        Ok(Self { kind, target, body })
    }
}

/// Per-chat receipt batcher. Later receipts of the same kind from the same
/// sender supersede earlier ones (a Read up to X implies everything before).
#[derive(Debug, Default)]
pub struct ReceiptBatcher {
    pending: BTreeMap<[u8; 8], (u64, BTreeMap<OpKind, DeltaOp>)>,
}

impl ReceiptBatcher {
    /// Queue a batched op for chat `tag`.
    pub fn push(&mut self, tag: [u8; 8], op: DeltaOp, now_ms: u64) -> Result<()> {
        if !op.kind.batched() {
            return Err(OmcpError::Sync("op is not batchable"));
        }
        op.encode()?;
        let e = self.pending.entry(tag).or_insert((now_ms, BTreeMap::new()));
        e.1.insert(op.kind, op);
        Ok(())
    }

    /// Chats whose batch window elapsed, with their ops (≤ 1 flush / 5 s).
    pub fn flush_due(&mut self, now_ms: u64) -> Vec<([u8; 8], Vec<DeltaOp>)> {
        let due: Vec<[u8; 8]> = self
            .pending
            .iter()
            .filter(|(_, (t0, _))| now_ms.saturating_sub(*t0) >= BATCH_INTERVAL_MS)
            .map(|(k, _)| *k)
            .collect();
        due.into_iter()
            .map(|k| {
                let (_, ops) = self.pending.remove(&k).expect("present");
                (k, ops.into_values().collect())
            })
            .collect()
    }
}

/// TTL to use for an op (receipts never travel more than 2 hops).
pub fn ttl_for(kind: OpKind, default_ttl: u8) -> u8 {
    if kind.batched() {
        default_ttl.min(RECEIPT_MAX_HOPS)
    } else {
        default_ttl
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kind: OpKind, t: u8) -> DeltaOp {
        DeltaOp {
            kind,
            target: [t; 16],
            body: vec![],
        }
    }

    #[test]
    fn op_size_limit_and_roundtrip() {
        let o = DeltaOp {
            kind: OpKind::Reaction,
            target: [1; 16],
            body: "👍".as_bytes().to_vec(),
        };
        assert_eq!(DeltaOp::decode(&o.encode().unwrap()).unwrap(), o);
        let big = DeltaOp {
            body: vec![0; 47],
            ..o
        };
        assert!(big.encode().is_err());
    }

    #[test]
    fn batching_one_per_5s() {
        let mut b = ReceiptBatcher::default();
        for i in 0..100u64 {
            b.push([1; 8], op(OpKind::Read, i as u8), i * 10).unwrap();
            b.push([1; 8], op(OpKind::Typing, 0), i * 10).unwrap();
        }
        assert!(b.flush_due(4_999).is_empty());
        let f = b.flush_due(5_000);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].1.len(), 2);
        assert_eq!(f[0].1[0].target, [99; 16]);
        assert!(b.push([1; 8], op(OpKind::Edit, 0), 0).is_err());
        assert_eq!(ttl_for(OpKind::Read, 9), 2);
        assert_eq!(ttl_for(OpKind::Edit, 9), 9);
    }
}
