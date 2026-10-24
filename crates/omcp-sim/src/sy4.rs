//! SY4: delivery over a 6-hop path at 30 % churn within 10 minutes.
//!
//! Topology: source → 5 relay layers of `width` nodes → destination (6 hops),
//! adjacent layers fully connected, plus `distractors` bystanders attached to
//! every layer node (they never want the tag). Total nodes > 32, so §22.1
//! interest routing (not flooding) is active.
//!
//! Each 10 s tick every node is independently offline with probability
//! `churn`. Holders store-and-forward. Three strategies, same seeds:
//! - `Flood`: §8.3 epidemic baseline — send to every online neighbour.
//! - `Interest`: real `plan_forward` over real cuckoo digests where relays
//!   advertise the destination's tag as a *carry* tag (§22.1 "carry tags for
//!   nearby friends", radius 6); spray-and-wait when nothing matches.
//! - `SprayOnly`: the same without carry tags (pure spray-and-wait).

use std::collections::HashMap;

use omcp_core::sync::cuckoo::CuckooFilter;
use omcp_core::sync::routing::{plan_forward, ForwardPlan, SPRAY_COPIES};
use omcp_core::sync::SplitMix64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    Flood,
    Interest,
    SprayOnly,
}

#[derive(Debug, Clone)]
pub struct Sy4Report {
    pub strategy: Strategy,
    pub trials: usize,
    pub delivered: usize,
    pub delivery_rate: f64,
    pub mean_transmissions: f64,
    pub median_latency_ticks: u64,
}

pub const TICK_S: u64 = 10;
pub const DEADLINE_TICKS: u64 = 60;

struct Topo {
    adj: Vec<Vec<usize>>,
    dst: usize,
    want_dst: Vec<bool>,
}

fn build(width: usize, distractors: usize) -> Topo {
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
    let mut want = vec![false; n];
    for layer in &layers[1..6] {
        for &x in layer {
            want[x] = true; // carry tag
        }
    }
    let layer_nodes: Vec<usize> = (0..n).collect();
    for a in layer_nodes {
        for _ in 0..distractors {
            adj.push(vec![a]);
            want.push(false);
            let d = adj.len() - 1;
            adj[a].push(d);
        }
    }
    let dst = layers[6][0];
    want[dst] = true;
    Topo {
        adj,
        dst,
        want_dst: want,
    }
}

pub fn run(
    strategy: Strategy,
    width: usize,
    distractors: usize,
    churn: f64,
    trials: usize,
    seed: u64,
) -> Sy4Report {
    let topo = build(width, distractors);
    let n = topo.adj.len();
    let tag = [0xD5u8; 8];
    let mut rng = SplitMix64(seed);
    let mut delivered = 0;
    let mut tx_total = 0u64;
    let mut lat = Vec::new();
    for _ in 0..trials {
        // Fresh digests each trial (fresh salts / decoys).
        let digests: Vec<CuckooFilter> = (0..n)
            .map(|i| {
                let carry = strategy == Strategy::Interest && topo.want_dst[i];
                let own = i == topo.dst;
                let mut tags = vec![rng.next_u64().to_be_bytes()];
                if carry || own {
                    tags.push(tag);
                }
                CuckooFilter::build_digest(&tags, 8, &mut rng).expect("digest")
            })
            .collect();
        let mut copies: HashMap<usize, u8> = HashMap::from([(0usize, SPRAY_COPIES)]);
        let mut done = None;
        for t in 1..=DEADLINE_TICKS {
            let online: Vec<bool> = (0..n).map(|_| !rng.chance(churn)).collect();
            let holders: Vec<(usize, u8)> = copies.iter().map(|(a, b)| (*a, *b)).collect();
            let mut new: Vec<(usize, u8, usize)> = Vec::new();
            for (h, c) in holders {
                if !online[h] {
                    continue;
                }
                let cand: Vec<usize> = topo.adj[h]
                    .iter()
                    .copied()
                    .filter(|x| online[*x] && !copies.contains_key(x))
                    .collect();
                if cand.is_empty() {
                    continue;
                }
                match strategy {
                    Strategy::Flood => {
                        for x in cand {
                            new.push((x, SPRAY_COPIES, h));
                        }
                    }
                    _ => {
                        let ds: Vec<Option<&CuckooFilter>> =
                            cand.iter().map(|x| Some(&digests[*x])).collect();
                        match plan_forward(&tag, c, &ds, None, n, &mut rng) {
                            ForwardPlan::Interest(v) | ForwardPlan::Flood(v) => {
                                for k in v {
                                    new.push((cand[k], SPRAY_COPIES, h));
                                }
                            }
                            ForwardPlan::Spray { targets, keep } => {
                                for (k, cc) in targets {
                                    new.push((cand[k], cc, h));
                                }
                                copies.insert(h, keep);
                            }
                            ForwardPlan::Wait => {}
                        }
                    }
                }
            }
            for (x, c, _) in new {
                tx_total += 1;
                copies.entry(x).or_insert(c);
            }
            if copies.contains_key(&topo.dst) {
                done = Some(t);
                break;
            }
        }
        if let Some(t) = done {
            delivered += 1;
            lat.push(t);
        }
    }
    lat.sort_unstable();
    Sy4Report {
        strategy,
        trials,
        delivered,
        delivery_rate: delivered as f64 / trials as f64,
        mean_transmissions: tx_total as f64 / trials as f64,
        median_latency_ticks: lat.get(lat.len() / 2).copied().unwrap_or(u64::MAX),
    }
}
