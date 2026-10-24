//! SY4: delivery over a 6-hop path at 30 % churn within 10 minutes, with
//! pure §22 / §22.7(1) behaviour (no pre-seeded routing state).
//!
//! Topology: A — 5 relay layers of `width` nodes — B (6 hops), adjacent
//! layers fully connected, plus `distractors` bystanders on every layer node.
//! Node count > 32, so interest routing is active. A and B are the two
//! members of a 1:1 chat (both *want* its tag); nobody else does.
//!
//! A trial is a conversation of `messages` messages alternating A→B, B→A,
//! sent one after another (each gets a 10-minute = 60-tick deadline; the
//! next starts when it is delivered or expires). Every 10 s tick each node is
//! independently offline with probability `churn`.
//!
//! Each node runs a real `CarryTable` (activity on deliver/forward, 1 h TTL,
//! ≤ 64 entries, distance vector `d = 1 + min neighbour d` from neighbours'
//! last digests) and advertises a real padded cuckoo digest of its wanted
//! tags + carry tags. Online holders forward with the real `plan_forward`
//! (want > lowest carry d > spray-and-wait). `Flood` is the §8.3 baseline.

use std::collections::HashMap;

use omcp_core::sync::cuckoo::CuckooFilter;
use omcp_core::sync::routing::{plan_forward, CarryTable, ForwardPlan, SPRAY_COPIES};
use omcp_core::sync::tag::MailboxTag;
use omcp_core::sync::SplitMix64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Flood,
    /// §22.7(1) as written: one chat tag, wanted by both members.
    Sync22,
    /// Proposed fix: route on a per-recipient sub-tag
    /// (`carry/want` keyed on `(chat tag, recipient member)`), so the
    /// distance vector points at the recipient, not the nearest member.
    Sync22PerMember,
    /// Proposed fix: per-recipient sub-tags + discovery — a holder with no
    /// wanter/carry neighbour and no carry state of its own floods (within
    /// TTL) instead of spraying; that flood creates the carry state.
    Sync22Discovery,
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
    pub median_latency_ticks: u64,
}

pub const TICK_MS: u64 = 10_000;
pub const DEADLINE_TICKS: u64 = 60;

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
    own: MailboxTag,
    wants: Vec<MailboxTag>,
    carry: CarryTable,
    advertised: Vec<MailboxTag>,
    digest: CuckooFilter,
}

fn digest(
    own: &MailboxTag,
    wants: &[MailboxTag],
    carry: &[MailboxTag],
    rng: &mut SplitMix64,
) -> CuckooFilter {
    let mut t = vec![*own];
    t.extend_from_slice(wants);
    t.extend_from_slice(carry);
    CuckooFilter::build_digest(&t, 16, rng).expect("digest")
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
    let chat_tag: MailboxTag = [0xD5; 8];
    let member_tag = |m: usize| -> MailboxTag {
        let mut t = chat_tag;
        t[7] ^= 1 + (m == b) as u8;
        t
    };
    let mut rng = SplitMix64(seed);
    let (mut delivered, mut cold, mut warm, mut warm_n, mut tx) =
        (0usize, 0usize, 0usize, 0usize, 0u64);
    let mut lat = Vec::new();
    for _ in 0..trials {
        let mut nodes: Vec<Node> = (0..n)
            .map(|i| {
                let own = rng.next_u64().to_be_bytes();
                let wants = match strategy {
                    _ if i != a && i != b => vec![],
                    Strategy::Sync22PerMember | Strategy::Sync22Discovery => vec![member_tag(i)],
                    _ => vec![chat_tag],
                };
                let digest = digest(&own, &wants, &[], &mut rng);
                Node {
                    own,
                    wants,
                    carry: CarryTable::default(),
                    advertised: Vec::new(),
                    digest,
                }
            })
            .collect();
        let mut now = 0u64;
        for m in 0..messages {
            let (src, dst) = if m % 2 == 0 { (a, b) } else { (b, a) };
            let tag = if matches!(
                strategy,
                Strategy::Sync22PerMember | Strategy::Sync22Discovery
            ) {
                member_tag(dst)
            } else {
                chat_tag
            };
            let mut copies: HashMap<usize, u8> = HashMap::from([(src, SPRAY_COPIES)]);
            let mut got = None;
            for t in 1..=DEADLINE_TICKS {
                now += TICK_MS;
                // Distance-vector refresh from neighbours' last digests.
                if strategy != Strategy::Flood {
                    for i in 0..n {
                        if nodes[i].carry.is_empty() {
                            continue;
                        }
                        let ds: Vec<CuckooFilter> =
                            adj[i].iter().map(|j| nodes[*j].digest.clone()).collect();
                        let refs: Vec<&CuckooFilter> = ds.iter().collect();
                        nodes[i].carry.update_from_neighbours(&refs, now);
                        let adv = nodes[i].carry.advertised();
                        if adv != nodes[i].advertised {
                            let own = nodes[i].own;
                            let d = digest(&own, &nodes[i].wants, &adv, &mut rng);
                            nodes[i].digest = d;
                            nodes[i].advertised = adv;
                        }
                    }
                }
                let online: Vec<bool> = (0..n).map(|_| !rng.chance(churn)).collect();
                let mut holders: Vec<(usize, u8)> = copies.iter().map(|(x, c)| (*x, *c)).collect();
                holders.sort_unstable();
                let mut new: Vec<(usize, u8, usize)> = Vec::new();
                for (h, c) in holders {
                    if !online[h] {
                        continue;
                    }
                    let cand: Vec<usize> = adj[h]
                        .iter()
                        .copied()
                        .filter(|x| online[*x] && !copies.contains_key(x))
                        .collect();
                    if cand.is_empty() {
                        continue;
                    }
                    match strategy {
                        Strategy::Flood => new.extend(cand.iter().map(|x| (*x, c, h))),
                        _ => {
                            let ds: Vec<Option<&CuckooFilter>> =
                                cand.iter().map(|x| Some(&nodes[*x].digest)).collect();
                            match plan_forward(&tag, c, &ds, None, n, &mut rng) {
                                ForwardPlan::Interest(v) | ForwardPlan::Flood(v) => {
                                    new.extend(v.into_iter().map(|k| (cand[k], c, h)))
                                }
                                ForwardPlan::Carry(k) => new.push((cand[k], c, h)),
                                ForwardPlan::Spray { .. } | ForwardPlan::Wait
                                    if strategy == Strategy::Sync22Discovery
                                        && nodes[h].carry.distance(&tag).is_none() =>
                                {
                                    new.extend(cand.iter().map(|x| (*x, c, h)))
                                }
                                ForwardPlan::Spray { targets, keep } => {
                                    new.extend(targets.into_iter().map(|(k, cc)| (cand[k], cc, h)));
                                    copies.insert(h, keep);
                                }
                                ForwardPlan::Wait => {}
                            }
                        }
                    }
                }
                for (x, c, from) in new {
                    tx += 1;
                    copies.entry(x).or_insert(c);
                    nodes[from].carry.note_activity(tag, now);
                    if x == dst {
                        nodes[x].carry.note_activity(tag, now);
                    }
                    if strategy == Strategy::Sync22Discovery {
                        // Relays learn the tag from the envelope they hold.
                        nodes[x].carry.note_activity(tag, now);
                    }
                }
                if copies.contains_key(&dst) {
                    got = Some(t);
                    break;
                }
            }
            if let Some(t) = got {
                delivered += 1;
                lat.push(t);
                if m == 0 {
                    cold += 1;
                }
                if m >= 2 {
                    warm += 1;
                }
            }
            if m >= 2 {
                warm_n += 1;
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
        median_latency_ticks: lat.get(lat.len() / 2).copied().unwrap_or(u64::MAX),
    }
}
