//! SY1: per-node sync traffic in a large mesh with 1 % active nodes, plus
//! the §22.7(2) metadata budget (≤ 2 % of payload when active, ≤ 1 MB/day
//! overhead when idle).
//!
//! Model (one simulated day):
//! - Nodes sit on a torus grid; radio neighbours are within Chebyshev
//!   distance `radio_radius` (1 → 8 neighbours, 2 → 24). Each node's
//!   neighbourhood changes `neighbourhood_changes_per_hour` times (Poisson).
//! - Each node has ~50 1:1 contacts (half within distance 5, half anywhere)
//!   and 5 groups of 8 (local) — ~55 rotating tags.
//! - 1 % of nodes are active and send `msgs_per_active` messages a day.
//!
//! Executed with real encodings / state machines:
//! - Hellos (12 B broadcast) on the real `HelloTimer` schedule (1 s after a
//!   change, doubling to 60 s).
//! - Digests: a full padded cuckoo digest broadcast every 5 min, and real
//!   slot deltas at each 6 h tag rotation (old tags removed, new inserted,
//!   decoy padding kept constant).
//! - Broadcasts are charged to the sender (tx) and to every radio neighbour
//!   (rx).
//! - Per 15-min window each node opens `encounters_per_window` links; peers
//!   sharing a chat run range reconciliation (charged the measured in-sync
//!   session cost, which is what running it gives after delivery).
//!
//! Modelled per message: binary spray-and-wait (L = 8) charged as 7 envelope
//! transmissions among the sender's radio neighbours, then one delivery
//! transmission per recipient. Both ends of every transmission are charged.
//!
//! Metrics: *payload* of a node = envelopes it authored or that were
//! delivered to it as a member. *Overhead* = everything else it sent or
//! received (metadata + relaying for others).

use omcp_core::sync::cuckoo::CuckooFilter;
use omcp_core::sync::hello::{HelloTimer, DIGEST_REFRESH_MS, HELLO_BYTES};
use omcp_core::sync::recon::{run_session, Item, Reconciler};
use omcp_core::sync::routing::SPRAY_COPIES;
use omcp_core::sync::tag::{mailbox_tag, MailboxTag};
use omcp_core::sync::SplitMix64;

use crate::{median, percentile, TEXT_ENVELOPE_BYTES};

#[derive(Debug, Clone)]
pub struct Sy1Params {
    pub nodes: usize,
    pub active_frac: f64,
    pub msgs_per_active: usize,
    pub contacts_each_side: usize,
    pub groups_per_node: usize,
    pub group_size: usize,
    pub windows: usize,
    pub encounters_per_window: usize,
    pub radio_radius: i64,
    pub neighbourhood_changes_per_hour: f64,
    pub seed: u64,
}

impl Sy1Params {
    pub fn new(nodes: usize, seed: u64) -> Self {
        Self {
            nodes,
            active_frac: 0.01,
            msgs_per_active: 200,
            contacts_each_side: 25,
            groups_per_node: 5,
            group_size: 8,
            windows: 96,
            encounters_per_window: 4,
            radio_radius: 1,
            neighbourhood_changes_per_hour: 2.0,
            seed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sy1Report {
    pub nodes: usize,
    pub messages: usize,
    pub median_bytes: u64,
    pub p95_bytes: u64,
    pub max_bytes: u64,
    pub metadata_fraction: f64,
    pub flood_median_bytes: u64,
    pub recon_sessions: u64,
    pub digests_sent: u64,
    pub radio_neighbours: usize,
    /// Median idle node: overhead (metadata + relaying) bytes/day.
    pub idle_overhead_median: u64,
    pub idle_overhead_p95: u64,
    /// Median idle node: metadata only (tx + rx), bytes/day.
    pub idle_metadata_median: u64,
    /// Active nodes, median of metadata / own payload (all metadata).
    pub active_meta_ratio_raw_median: f64,
    /// Active nodes, median of (metadata − idle median metadata) / payload.
    pub active_meta_ratio_incremental_median: f64,
}

const TAG_EPOCHS_PER_DAY: usize = 4;

fn neighbours(i: usize, side: usize, n: usize, r: i64) -> Vec<u32> {
    let (x, y) = ((i % side) as i64, (i / side) as i64);
    let mut v = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = (x + dx).rem_euclid(side as i64) as usize;
            let ny = (y + dy).rem_euclid(side as i64) as usize;
            let j = ny * side + nx;
            if j < n && j != i {
                v.push(j as u32);
            }
        }
    }
    v.sort_unstable();
    v.dedup();
    v
}

pub fn run(p: &Sy1Params) -> Sy1Report {
    let n = p.nodes;
    let mut rng = SplitMix64(p.seed);
    let side = (n as f64).sqrt().ceil() as usize;
    let radio: Vec<Vec<u32>> = (0..n)
        .map(|i| neighbours(i, side, n, p.radio_radius))
        .collect();
    let near: Vec<Vec<u32>> = (0..n).map(|i| neighbours(i, side, n, 5)).collect();

    // Chats: (members). 1:1 then groups.
    let mut chats: Vec<Vec<u32>> = Vec::new();
    let mut member_of: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        for c in 0..p.contacts_each_side {
            let j = if c % 2 == 0 {
                near[i][rng.below(near[i].len() as u64) as usize] as usize
            } else {
                rng.below(n as u64) as usize
            };
            if j == i {
                continue;
            }
            let id = chats.len() as u32;
            chats.push(vec![i as u32, j as u32]);
            member_of[i].push(id);
            member_of[j].push(id);
        }
    }
    let n_groups = n * p.groups_per_node / p.group_size;
    for _ in 0..n_groups {
        let seed = rng.below(n as u64) as usize;
        let mut m = vec![seed as u32];
        while m.len() < p.group_size {
            let j = near[seed][rng.below(near[seed].len() as u64) as usize];
            if !m.contains(&j) {
                m.push(j);
            }
        }
        let id = chats.len() as u32;
        for x in &m {
            member_of[*x as usize].push(id);
        }
        chats.push(m);
    }
    for m in member_of.iter_mut() {
        m.sort_unstable();
        m.dedup();
    }

    // Messages: (window, chat, sender).
    let mut msgs: Vec<(usize, u32, u32)> = Vec::new();
    for i in 0..n {
        if !rng.chance(p.active_frac) || member_of[i].is_empty() {
            continue;
        }
        for _ in 0..p.msgs_per_active {
            let c = member_of[i][rng.below(member_of[i].len() as u64) as usize];
            msgs.push((rng.below(p.windows as u64) as usize, c, i as u32));
        }
    }
    msgs.sort_unstable();

    let mut data = vec![0u64; n];
    let mut meta = vec![0u64; n];
    let mut log: Vec<Vec<Item>> = vec![Vec::new(); chats.len()];

    // Cost of an in-sync reconciliation, measured once.
    let in_sync_cost = {
        let s: Vec<Item> = (0..100u64)
            .map(|t| Item {
                ts: t,
                id: [t as u8; 16],
            })
            .collect();
        run_session(&Reconciler::new(s.clone()), &Reconciler::new(s)).0 as u64
    };

    let mut payload = vec![0u64; n];
    let mut bcast = vec![0u64; n]; // broadcast tx bytes
    let mut digests_sent = 0u64;
    let day_ms = 86_400_000u64;
    let tag_of = |c: u32, epoch: u64| {
        let mut s = [0u8; 32];
        s[..4].copy_from_slice(&c.to_be_bytes());
        s[4..12].copy_from_slice(&p.seed.to_be_bytes());
        mailbox_tag(&s, epoch).expect("tag")
    };
    for i in 0..n {
        // Hellos.
        // Poisson process: exponential gaps with mean 1/λ.
        let mean_gap = 3_600_000.0 / p.neighbourhood_changes_per_hour;
        let mut ch: Vec<u64> = Vec::new();
        let mut t = 0f64;
        loop {
            let u = (rng.below(1 << 30) as f64 + 1.0) / (1u64 << 30) as f64;
            t += -mean_gap * u.ln();
            if t >= day_ms as f64 {
                break;
            }
            ch.push(t as u64);
        }
        ch.sort_unstable();
        bcast[i] += HelloTimer::count_hellos(0, day_ms, &ch) * HELLO_BYTES;
        // Digests: full refresh every 5 min + real deltas at each tag rotation.
        let tags0: Vec<MailboxTag> = member_of[i].iter().map(|c| tag_of(*c, 0)).collect();
        let mut f = CuckooFilter::build_digest(&tags0, 64, &mut rng).expect("digest");
        let refreshes = day_ms / DIGEST_REFRESH_MS;
        bcast[i] += refreshes * f.encoded_len() as u64;
        digests_sent += refreshes;
        for e in 1..TAG_EPOCHS_PER_DAY as u64 {
            let old = f.clone();
            let mut ok = true;
            for c in &member_of[i] {
                ok &= f.remove_keep_padding(&tag_of(*c, e - 1), &mut rng).is_ok()
                    && f.insert_keep_padding(&tag_of(*c, e), &mut rng).is_ok();
            }
            if !ok {
                // Incremental update hit the load limit: rebuild (full send).
                let t: Vec<MailboxTag> = member_of[i].iter().map(|c| tag_of(*c, e)).collect();
                f = CuckooFilter::build_digest(&t, 64, &mut rng).expect("digest");
            }
            bcast[i] += match old.delta_to(&f) {
                Some(d) if ok => d.encoded_len() as u64,
                _ => f.encoded_len() as u64,
            };
        }
    }
    for i in 0..n {
        let rx: u64 = radio[i].iter().map(|j| bcast[*j as usize]).sum();
        meta[i] += bcast[i] + rx;
    }

    let mut recon_sessions = 0u64;
    let mut mi = 0;
    for w in 0..p.windows {
        // New messages: spray + delivery to members (logs are then in sync).
        while mi < msgs.len() && msgs[mi].0 == w {
            let (_, c, s) = msgs[mi];
            mi += 1;
            let s = s as usize;
            let ts = (w as u64) << 32 | mi as u64;
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
            log[c as usize].push(Item { ts, id });
            payload[s] += TEXT_ENVELOPE_BYTES;
            for _ in 0..SPRAY_COPIES as usize - 1 {
                let b = radio[s][rng.below(radio[s].len() as u64) as usize] as usize;
                data[s] += TEXT_ENVELOPE_BYTES;
                data[b] += TEXT_ENVELOPE_BYTES;
            }
            for m in &chats[c as usize] {
                let m = *m as usize;
                if m == s {
                    continue;
                }
                let carrier = radio[m][rng.below(radio[m].len() as u64) as usize] as usize;
                data[m] += TEXT_ENVELOPE_BYTES;
                data[carrier] += TEXT_ENVELOPE_BYTES;
                payload[m] += TEXT_ENVELOPE_BYTES;
            }
        }
        // Link encounters: reconcile shared chats.
        for a in 0..n {
            for _ in 0..p.encounters_per_window {
                let k = rng.below(radio[a].len() as u64) as usize;
                let b = radio[a][k] as usize;
                let (ma, mb) = (&member_of[a], &member_of[b]);
                let (mut x, mut y, mut shared) = (0, 0, false);
                while x < ma.len() && y < mb.len() {
                    if ma[x] == mb[y] {
                        if !log[ma[x] as usize].is_empty() {
                            shared = true;
                            break;
                        }
                        x += 1;
                        y += 1;
                    } else if ma[x] < mb[y] {
                        x += 1;
                    } else {
                        y += 1;
                    }
                }
                if shared {
                    recon_sessions += 1;
                    meta[a] += in_sync_cost;
                    meta[b] += in_sync_cost;
                }
            }
        }
    }

    // Metadata budget metrics.
    let mut active = vec![false; n];
    for m in &msgs {
        active[m.2 as usize] = true;
    }
    let mut idle_over: Vec<u64> = (0..n)
        .filter(|i| !active[*i])
        .map(|i| meta[i] + data[i] - payload[i])
        .collect();
    let mut idle_meta: Vec<u64> = (0..n).filter(|i| !active[*i]).map(|i| meta[i]).collect();
    let idle_meta_med = median(&mut idle_meta);
    let idle_over_med = median(&mut idle_over);
    let idle_over_p95 = percentile(&idle_over, 95);
    let mut raw: Vec<f64> = Vec::new();
    let mut inc: Vec<f64> = Vec::new();
    for i in (0..n).filter(|i| active[*i]) {
        raw.push(meta[i] as f64 / payload[i] as f64);
        inc.push((meta[i] as f64 - idle_meta_med as f64).max(0.0) / payload[i] as f64);
    }
    let fmed = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let (raw_med, inc_med) = (fmed(&mut raw), fmed(&mut inc));

    let mut total: Vec<u64> = (0..n).map(|i| data[i] + meta[i]).collect();
    let med = median(&mut total);
    let sum_meta: u64 = meta.iter().sum();
    let sum_all: u64 = total.iter().sum();
    // Flooding baseline: every node receives and re-broadcasts every message.
    let flood = msgs.len() as u64 * TEXT_ENVELOPE_BYTES * 2;
    Sy1Report {
        nodes: n,
        messages: msgs.len(),
        median_bytes: med,
        p95_bytes: percentile(&total, 95),
        max_bytes: *total.last().expect("nodes"),
        metadata_fraction: sum_meta as f64 / sum_all as f64,
        flood_median_bytes: flood,
        recon_sessions,
        digests_sent,
        radio_neighbours: radio[0].len(),
        idle_overhead_median: idle_over_med,
        idle_overhead_p95: idle_over_p95,
        idle_metadata_median: idle_meta_med,
        active_meta_ratio_raw_median: raw_med,
        active_meta_ratio_incremental_median: inc_med,
    }
}
