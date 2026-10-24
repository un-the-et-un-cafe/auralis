//! SY4: delivery over a 6-hop path at 30 % churn within 10 minutes, with the
//! §22.7(9) forwarding rules (no pre-seeded routing state).
//!
//! Topology: A — 5 relay layers of `width` nodes — B (6 hops), adjacent
//! layers fully connected, plus `distractors` bystanders on every layer node.
//! Node count > 32, so interest routing is active. A and B form a 1:1 chat;
//! each wants only its own per-recipient sub-tag.
//!
//! A trial is a conversation of `messages` messages alternating A→B, B→A,
//! sent one after another (each gets a 10-minute = 60-tick deadline). Every
//! 10 s tick each node is independently offline with probability `churn`.
//! Envelopes start with the real adaptive TTL and lose one hop per relay.
//!
//! `Sync22`: each node runs a real `CarryTable` (activity on forward /
//! delivery, 1 h TTL, ≤ 64 entries, distance vector from neighbours' last
//! digests) and keeps a real padded cuckoo digest of its own sub-tag + carry
//! tags, updated incrementally; every change is broadcast as a real
//! `DigestDelta`. Online holders forward with the real `plan_forward`
//! (want > lowest carry > scoped flood within TTL). `Flood` is the §8.3
//! epidemic baseline with the same TTL.
//!
//! Per-message metadata (§22.7(10)): per delivered envelope, the
//! `EnvelopeV1` header (41 B) + routing TLVs (chat tag 10 B + one sub-tag
//! 10 B) + the digest-delta bytes broadcast because of carry-state changes
//! the conversation caused. Per-hop retransmissions are reported separately
//! as transmissions.

use std::collections::{BTreeSet, HashMap};

use omcp_core::sync::cuckoo::CuckooFilter;
use omcp_core::sync::routing::{adaptive_ttl, plan_forward, CarryTable, ForwardPlan};
use omcp_core::sync::tag::{subtag, MailboxTag};
use omcp_core::sync::SplitMix64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Flood,
    Sync22,
}

#[derive(Debug, Clone)]
pub struct Sy4Report {
    pub strategy: Strategy,
    pub trials: usize,
    pub messages: usize,
    pub delivery_rate: f64,
    /// First message of each conversation (no carry state yet).
    pub cold_delivery_rate: f64,
    /// Messages 3.. of each conversation.
    pub warm_delivery_rate: f64,
    pub mean_transmissions_per_msg: f64,
    pub warm_transmissions_per_msg: f64,
    pub median_latency_ticks: u64,
    /// Per delivered envelope: header + TLVs + caused digest deltas (bytes).
    pub metadata_per_delivered: f64,
}

pub const TICK_MS: u64 = 10_000;
pub const DEADLINE_TICKS: u64 = 60;
/// `EnvelopeV1` fixed header + tag TLV + one sub-tag TLV.
pub const ENVELOPE_META_BYTES: u64 = 41 + 10 + 10;

fn build(width: usize, distractors: usize) -> (Vec<Vec<usize>>, usize, usize) {
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let mut n = 0;
    for l in 0..7 {
        let w = if l == 0 || l == 6 { 1 } else { width };
        layers.push((n..n + w).collect());
        n += w;
    }
    let mut adj = vec![Vec::new(); n];
    for l in 0..6 {
        for &a in &layers[l] {
            for &b in &layers[l + 1] {
                adj[a].push(b);
                adj[b].push(a);
            }
        }
    }
    for a in 0..n {
        for _ in 0..distractors {
            adj.push(vec![a]);
            let d = adj.len() - 1;
            adj[a].push(d);
        }
    }
    (adj, layers[0][0], layers[6][0])
}

struct Node {
    carry: CarryTable,
    advertised: BTreeSet<MailboxTag>,
    digest: CuckooFilter,
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    strategy: Strategy,
    width: usize,
    distractors: usize,
    churn: f64,
    trials: usize,
    messages: usize,
    seed: u64,
) -> Sy4Report {
    let (adj, a, b) = build(width, distractors);
    let n = adj.len();
    let ttl0 = adaptive_ttl(n as f64);
    let mut rng = SplitMix64(seed);
    let secret = [0x5Au8; 32];
    let sub = |m: usize| subtag(&secret, u32::from(m == b), 0).expect("subtag");
    let (mut delivered, mut cold, mut warm, mut warm_n) = (0usize, 0usize, 0usize, 0usize);
    let (mut tx, mut warm_tx, mut delta_bytes) = (0u64, 0u64, 0u64);
    let mut lat = Vec::new();
    for _ in 0..trials {
        let mut nodes: Vec<Node> = (0..n)
            .map(|i| {
                let mut tags = vec![rng.next_u64().to_be_bytes()];
                if i == a || i == b {
                    tags.push(sub(i));
                }
                Node {
                    carry: CarryTable::default(),
                    advertised: BTreeSet::new(),
                    digest: CuckooFilter::build_digest(&tags, 16, &mut rng).expect("digest"),
                }
            })
            .collect();
        let mut now = 0u64;
        for m in 0..messages {
            let (src, dst) = if m % 2 == 0 { (a, b) } else { (b, a) };
            let route = [sub(dst)];
            let mut held: HashMap<usize, u8> = HashMap::from([(src, ttl0)]);
            let mut got = None;
            let mut msg_tx = 0u64;
            for t in 1..=DEADLINE_TICKS {
                now += TICK_MS;
                if strategy == Strategy::Sync22 {
                    for i in 0..n {
                        if nodes[i].carry.is_empty() && nodes[i].advertised.is_empty() {
                            continue;
                        }
                        let ds: Vec<CuckooFilter> =
                            adj[i].iter().map(|j| nodes[*j].digest.clone()).collect();
                        let refs: Vec<&CuckooFilter> = ds.iter().collect();
                        nodes[i].carry.update_from_neighbours(&refs, now);
                        let adv: BTreeSet<MailboxTag> =
                            nodes[i].carry.advertised().into_iter().collect();
                        if adv != nodes[i].advertised {
                            let old = nodes[i].digest.clone();
                            let mut f = old.clone();
                            let mut ok = true;
                            for r in nodes[i].advertised.difference(&adv) {
                                ok &= f.remove_keep_padding(r, &mut rng).is_ok();
                            }
                            for x in adv.difference(&nodes[i].advertised) {
                                ok &= f.insert_keep_padding(x, &mut rng).is_ok();
                            }
                            delta_bytes += match (ok, old.delta_to(&f)) {
                                (true, Some(d)) => d.encoded_len() as u64,
                                _ => f.encoded_len() as u64,
                            };
                            nodes[i].digest = f;
                            nodes[i].advertised = adv;
                        }
                    }
                }
                let online: Vec<bool> = (0..n).map(|_| !rng.chance(churn)).collect();
                let mut holders: Vec<(usize, u8)> = held.iter().map(|(x, c)| (*x, *c)).collect();
                holders.sort_unstable();
                let mut new: Vec<(usize, u8, usize)> = Vec::new();
                for (h, ttl) in holders {
                    if !online[h] || ttl == 0 {
                        continue;
                    }
                    let cand: Vec<usize> = adj[h]
                        .iter()
                        .copied()
                        .filter(|x| online[*x] && !held.contains_key(x))
                        .collect();
                    if cand.is_empty() {
                        continue;
                    }
                    let targets: Vec<usize> = match strategy {
                        Strategy::Flood => cand.clone(),
                        Strategy::Sync22 => {
                            let ds: Vec<Option<&CuckooFilter>> =
                                cand.iter().map(|x| Some(&nodes[*x].digest)).collect();
                            match plan_forward(&route, ttl, &ds, None, n) {
                                ForwardPlan::Interest(v)
                                | ForwardPlan::Flood(v)
                                | ForwardPlan::ScopedFlood(v) => v,
                                ForwardPlan::Carry(k) => vec![k],
                                ForwardPlan::Hold => vec![],
                            }
                            .into_iter()
                            .map(|k| cand[k])
                            .collect()
                        }
                    };
                    new.extend(targets.into_iter().map(|x| (x, ttl - 1, h)));
                }
                for (x, ttl, from) in new {
                    if held.contains_key(&x) {
                        continue;
                    }
                    msg_tx += 1;
                    held.insert(x, ttl);
                    nodes[from].carry.note_activity(route[0], now);
                    if x == dst {
                        nodes[x].carry.note_activity(route[0], now);
                    }
                }
                if held.contains_key(&dst) {
                    got = Some(t);
                    break;
                }
            }
            tx += msg_tx;
            if m >= 2 {
                warm_n += 1;
                warm_tx += msg_tx;
            }
            if let Some(t) = got {
                delivered += 1;
                lat.push(t);
                cold += usize::from(m == 0);
                warm += usize::from(m >= 2);
            }
        }
    }
    lat.sort_unstable();
    let total = trials * messages;
    Sy4Report {
        strategy,
        trials,
        messages: total,
        delivery_rate: delivered as f64 / total as f64,
        cold_delivery_rate: cold as f64 / trials as f64,
        warm_delivery_rate: warm as f64 / warm_n.max(1) as f64,
        mean_transmissions_per_msg: tx as f64 / total as f64,
        warm_transmissions_per_msg: warm_tx as f64 / warm_n.max(1) as f64,
        median_latency_ticks: lat.get(lat.len() / 2).copied().unwrap_or(u64::MAX),
        metadata_per_delivered: (ENVELOPE_META_BYTES * delivered as u64 + delta_bytes) as f64
            / delivered.max(1) as f64,
    }
}
