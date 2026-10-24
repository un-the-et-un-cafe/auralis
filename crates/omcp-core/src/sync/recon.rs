//! Range-based set reconciliation, Negentropy-style (PROTOCOL §22.2).
//!
//! Items are `(timestamp, envelope_id)` restricted (by the caller) to the tags
//! both peers want. A message is a list of consecutive ranges, each closed by
//! an exclusive upper bound and carrying one of: `Skip`, a 16-byte
//! fingerprint, or an explicit id list. Mismatching fingerprints are split into
//! [`BRANCHING`] sub-ranges, so cost is O(d · log n). Two peers already in sync
//! exchange one fingerprint (~30 bytes) and an empty reply.
//!
//! Fingerprint: `SHA-256(XOR_i SHA-256(ts_i || id_i) || count)[..16]`.

use sha2::{Digest, Sha256};

use super::{get_varint, put_varint, take};
use crate::error::{OmcpError, Result};

/// Sub-ranges per split.
pub const BRANCHING: usize = 16;
/// Ranges with at most this many local items are sent as id lists.
pub const IDLIST_THRESHOLD: usize = 2 * BRANCHING;

/// One reconcilable item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Item {
    pub ts: u64,
    pub id: [u8; 16],
}

impl Item {
    pub const MIN: Item = Item { ts: 0, id: [0; 16] };
    pub const INF: Item = Item {
        ts: u64::MAX,
        id: [0xff; 16],
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Skip,
    Fingerprint([u8; 16]),
    IdList(Vec<Item>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub upper: Item,
    pub mode: Mode,
}

/// A reconciliation message.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Message(pub Vec<Range>);

fn put_item(out: &mut Vec<u8>, it: &Item) {
    if *it == Item::INF {
        out.push(0);
        return;
    }
    out.push(1);
    put_varint(out, it.ts);
    out.extend_from_slice(&it.id);
}

fn get_item(buf: &mut &[u8]) -> Result<Item> {
    match take(buf, 1)?[0] {
        0 => Ok(Item::INF),
        1 => {
            let ts = get_varint(buf)?;
            let id = take(buf, 16)?.try_into().expect("16");
            Ok(Item { ts, id })
        }
        _ => Err(OmcpError::Sync("recon bound")),
    }
}

impl Message {
    pub fn is_done(&self) -> bool {
        self.0.is_empty()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut v = Vec::new();
        put_varint(&mut v, self.0.len() as u64);
        for r in &self.0 {
            match &r.mode {
                Mode::Skip => v.push(0),
                Mode::Fingerprint(f) => {
                    v.push(1);
                    v.extend_from_slice(f);
                }
                Mode::IdList(ids) => {
                    v.push(2);
                    put_varint(&mut v, ids.len() as u64);
                    for i in ids {
                        put_varint(&mut v, i.ts);
                        v.extend_from_slice(&i.id);
                    }
                }
            }
            put_item(&mut v, &r.upper);
        }
        v
    }

    pub fn decode(mut buf: &[u8]) -> Result<Self> {
        let n = get_varint(&mut buf)?;
        if n > 1 << 16 {
            return Err(OmcpError::Sync("recon too many ranges"));
        }
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let mode = match take(&mut buf, 1)?[0] {
                0 => Mode::Skip,
                1 => Mode::Fingerprint(take(&mut buf, 16)?.try_into().expect("16")),
                2 => {
                    let k = get_varint(&mut buf)?;
                    if k > buf.len() as u64 / 17 {
                        return Err(OmcpError::Sync("recon id list"));
                    }
                    let mut ids = Vec::with_capacity(k as usize);
                    for _ in 0..k {
                        let ts = get_varint(&mut buf)?;
                        ids.push(Item {
                            ts,
                            id: take(&mut buf, 16)?.try_into().expect("16"),
                        });
                    }
                    Mode::IdList(ids)
                }
                _ => return Err(OmcpError::Sync("recon mode")),
            };
            out.push(Range {
                upper: get_item(&mut buf)?,
                mode,
            });
        }
        if !buf.is_empty() {
            return Err(OmcpError::Sync("recon trailing bytes"));
        }
        Ok(Message(out))
    }
}

/// Fingerprint of a slice of items.
pub fn fingerprint(items: &[Item]) -> [u8; 16] {
    let mut acc = [0u8; 32];
    for it in items {
        let mut h = Sha256::new();
        h.update(it.ts.to_be_bytes());
        h.update(it.id);
        for (a, b) in acc.iter_mut().zip(h.finalize().iter()) {
            *a ^= b;
        }
    }
    let mut h = Sha256::new();
    h.update(acc);
    h.update((items.len() as u64).to_be_bytes());
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.finalize()[..16]);
    out
}

/// Result of processing one incoming message.
#[derive(Debug, Default)]
pub struct Outcome {
    /// Reply to send (empty = done).
    pub reply: Message,
    /// Items we hold that the peer lacks (ship them).
    pub have: Vec<Item>,
    /// Items the peer holds that we lack (request them).
    pub need: Vec<Item>,
}

/// One side of a reconciliation session.
#[derive(Debug, Clone)]
pub struct Reconciler {
    items: Vec<Item>,
}

impl Reconciler {
    pub fn new(mut items: Vec<Item>) -> Self {
        items.sort_unstable();
        items.dedup();
        Self { items }
    }

    fn slice(&self, lo: &Item, hi: &Item) -> &[Item] {
        let a = self.items.partition_point(|x| x < lo);
        let b = self.items.partition_point(|x| x < hi);
        &self.items[a..b.max(a)]
    }

    /// First message: one fingerprint over everything.
    pub fn initiate(&self) -> Message {
        Message(vec![Range {
            upper: Item::INF,
            mode: Mode::Fingerprint(fingerprint(&self.items)),
        }])
    }

    pub fn process(&self, msg: &Message) -> Outcome {
        let mut out = Outcome::default();
        let mut reply: Vec<Range> = Vec::new();
        let mut lower = Item::MIN;
        for r in &msg.0 {
            let local = self.slice(&lower, &r.upper);
            match &r.mode {
                Mode::Skip => reply.push(Range {
                    upper: r.upper,
                    mode: Mode::Skip,
                }),
                Mode::Fingerprint(f) => {
                    if fingerprint(local) == *f {
                        reply.push(Range {
                            upper: r.upper,
                            mode: Mode::Skip,
                        });
                    } else if local.len() <= IDLIST_THRESHOLD {
                        reply.push(Range {
                            upper: r.upper,
                            mode: Mode::IdList(local.to_vec()),
                        });
                    } else {
                        let n = local.len();
                        for k in 0..BRANCHING {
                            let a = k * n / BRANCHING;
                            let b = (k + 1) * n / BRANCHING;
                            let upper = if k + 1 == BRANCHING {
                                r.upper
                            } else {
                                local[b]
                            };
                            reply.push(Range {
                                upper,
                                mode: Mode::Fingerprint(fingerprint(&local[a..b])),
                            });
                        }
                    }
                }
                Mode::IdList(theirs) => {
                    let mut t = theirs.clone();
                    t.sort_unstable();
                    out.need
                        .extend(t.iter().filter(|x| local.binary_search(x).is_err()));
                    out.have
                        .extend(local.iter().filter(|x| t.binary_search(x).is_err()));
                    reply.push(Range {
                        upper: r.upper,
                        mode: Mode::Skip,
                    });
                }
            }
            lower = r.upper;
        }
        // Coalesce skips; drop if everything is skipped.
        let mut merged: Vec<Range> = Vec::new();
        for r in reply {
            if r.mode == Mode::Skip {
                if let Some(last) = merged.last_mut() {
                    if last.mode == Mode::Skip {
                        last.upper = r.upper;
                        continue;
                    }
                }
            }
            merged.push(r);
        }
        if merged.iter().all(|r| r.mode == Mode::Skip) {
            merged.clear();
        }
        out.reply = Message(merged);
        out
    }
}

/// Run a full session in memory. Returns `(bytes_exchanged, round_trips,
/// a_has_b_lacks, b_has_a_lacks)`.
pub fn run_session(a: &Reconciler, b: &Reconciler) -> (usize, usize, Vec<Item>, Vec<Item>) {
    let mut bytes = 0;
    let mut msgs: usize = 0;
    let (mut a_extra, mut b_extra) = (Vec::new(), Vec::new());
    let mut msg = a.initiate();
    let mut turn_b = true;
    loop {
        let wire = msg.encode();
        bytes += wire.len();
        msgs += 1;
        let m = Message::decode(&wire).expect("self-encoded");
        let o = if turn_b { b.process(&m) } else { a.process(&m) };
        if turn_b {
            b_extra.extend(o.have);
            a_extra.extend(o.need);
        } else {
            a_extra.extend(o.have);
            b_extra.extend(o.need);
        }
        if o.reply.is_done() {
            bytes += 1; // terminal empty message
            break;
        }
        msg = o.reply;
        turn_b = !turn_b;
    }
    a_extra.sort_unstable();
    a_extra.dedup();
    b_extra.sort_unstable();
    b_extra.dedup();
    (bytes, msgs.div_ceil(2), a_extra, b_extra)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::SplitMix64;

    fn items(rng: &mut SplitMix64, n: usize) -> Vec<Item> {
        (0..n)
            .map(|_| {
                let mut id = [0u8; 16];
                id[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
                id[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
                Item {
                    ts: 1_700_000_000_000 + rng.below(86_400_000 * 30),
                    id,
                }
            })
            .collect()
    }

    #[test]
    fn in_sync_is_tiny() {
        let mut rng = SplitMix64(3);
        let s = items(&mut rng, 50_000);
        let (bytes, rt, x, y) = run_session(&Reconciler::new(s.clone()), &Reconciler::new(s));
        assert!(x.is_empty() && y.is_empty());
        assert_eq!(rt, 1);
        assert!(bytes <= 40, "{bytes}");
    }

    #[test]
    fn finds_exact_difference() {
        let mut rng = SplitMix64(4);
        let common = items(&mut rng, 20_000);
        let only_a = items(&mut rng, 7);
        let only_b = items(&mut rng, 13);
        let a = Reconciler::new([common.clone(), only_a.clone()].concat());
        let b = Reconciler::new([common, only_b.clone()].concat());
        let (bytes, _rt, mut ae, mut be) = run_session(&a, &b);
        let (mut oa, mut ob) = (only_a, only_b);
        oa.sort();
        ob.sort();
        ae.sort();
        be.sort();
        assert_eq!(ae, oa);
        assert_eq!(be, ob);
        // O(d log n): far below shipping 20k ids (~340 KB).
        assert!(bytes < 40_000, "{bytes}");
    }

    #[test]
    fn decode_rejects_garbage() {
        assert!(Message::decode(&[1, 9]).is_err());
        assert!(Message::decode(&[1, 0, 0, 5]).is_err());
        assert_eq!(Message::decode(&[0]).unwrap(), Message::default());
    }
}
