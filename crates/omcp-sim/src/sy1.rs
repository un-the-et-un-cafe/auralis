//! SY1: per-node sync traffic in a large mesh with 1 % active nodes, plus
//! the §22.7(2) metadata budget (≤ 2 % of payload when active, ≤ 1 MB/day
//! overhead when idle).
//!
//! Model (one simulated day):
//! - Nodes sit on a torus grid; radio neighbours are the
//!   `radio_neighbours` nearest grid nodes (8, 24, 32, …). Each node's
//!   neighbourhood changes `neighbourhood_changes_per_hour` times (Poisson).
//! - Each node has ~50 1:1 contacts (half within distance 5, half anywhere)
//!   and 5 groups of 8 (local) — ~55 rotating tags.
//! - 1 % of nodes are active and send `msgs_per_active` messages a day.
//!
//! Executed with real encodings / state machines:
//! - Hellos (12 B broadcast) on the real `HelloTimer` schedule (1 s after a
//!   change, doubling to 60 s; 120 s above 32 neighbours).
//! - Digests (§22.7(11)): a full padded cuckoo digest broadcast as the 30-min
//!   safety refresh, a unicast full digest exchanged with each new neighbour,
//!   and real slot deltas at each 6 h tag rotation (decoy padding constant).
//! - Broadcasts are charged to the sender (tx) and to every radio neighbour
//!   (rx).
//! - Per 15-min window each node opens `encounters_per_window` links; peers
//!   sharing a chat run range reconciliation (charged the measured in-sync
//!   session cost, which is what running it gives after delivery).
//!
//! Modelled per message: `RELAY_TX_PER_MSG` relay transmissions among the
//! sender's radio neighbours (≈ SY4 warm path cost), then one delivery
//! transmission per recipient. Both ends of every transmission are charged.
//!
//! Per-message metadata (§22.7(10)) per delivered envelope: `EnvelopeV1`
//! header (41 B) + chat-tag TLV (10 B) + sub-tag TLV (2 + 8·recipients B,
//! chats ≤ 8 members) + reconciliation bytes caused by the message (0 here:
//! recipients are already in sync when they reconcile). Carry-state digest
//! deltas caused by traffic are measured in SY4.
//!
//! Metrics: *payload* of a node = envelopes it authored or that were
//! delivered to it as a member. *Overhead* = everything else it sent or
//! received (metadata + relaying for others).

use omcp_core::sync::cuckoo::CuckooFilter;
use omcp_core::sync::hello::{HelloTimer, DIGEST_REFRESH_MS, HELLO_BYTES};
use omcp_core::sync::recon::{run_session, Item, Reconciler};
use omcp_core::sync::tag::SUBTAG_MAX_MEMBERS;

/// Relay transmissions charged per message (≈ SY4 warm path).
pub const RELAY_TX_PER_MSG: usize = 7;
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
    pub radio_neighbours: usize,
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
            radio_neighbours: 8,
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
    /// §22.7(10): mean per-message metadata per delivered envelope (bytes).
    pub per_message_metadata: f64,
    /// §22.7(10) limit: max(256 B, 10 % of the ciphertext).
    pub per_message_limit: f64,
}

const TAG_EPOCHS_PER_DAY: usize = 4;

/// The `k` nearest grid nodes (Euclidean on the torus, ties by index).
fn nearest(i: usize, side: usize, n: usize, k: usize) -> Vec<u32> {
    let r = ((k as f64).sqrt() as i64) + 2;
    let (x, y) = ((i % side) as i64, (i / side) as i64);
    let mut v: Vec<(i64, u32)> = Vec::new();
    for dy in -r..=r {
        for dx in -r..=r {
            let nx = (x + dx).rem_euclid(side as i64) as usize;
            let ny = (y + dy).rem_euclid(side as i64) as usize;
            let j = ny * side + nx;
            if j < n && j != i {
                v.push((dx * dx + dy * dy, j as u32));
            }
        }
    }
    v.sort_unstable();
    v.dedup_by_key(|e| e.1);
    let mut out: Vec<u32> = v.into_iter().take(k).map(|e| e.1).collect();
    out.sort_unstable();
    out
}

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
        .map(|i| nearest(i, side, n, p.radio_neighbours))
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
        bcast[i] += HelloTimer::count_hellos(0, day_ms, &ch, radio[i].len()) * HELLO_BYTES;
        // Digests: 30-min safety refresh + new-neighbour unicast + deltas.
        let tags0: Vec<MailboxTag> = member_of[i].iter().map(|c| tag_of(*c, 0)).collect();
        let mut f = CuckooFilter::build_digest(&tags0, 64, &mut rng).expect("digest");
        let refreshes = day_ms / DIGEST_REFRESH_MS;
        bcast[i] += refreshes * f.encoded_len() as u64;
        // Each change brings one new neighbour: full digest each way (unicast).
        meta[i] += ch.len() as u64 * 2 * f.encoded_len() as u64;
        digests_sent += refreshes + 2 * ch.len() as u64;
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
    let (mut per_msg_meta, mut delivered_envs) = (0u64, 0u64);
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
            let recips = chats[c as usize].len() as u64 - 1;
            let sub_tlv = if chats[c as usize].len() <= SUBTAG_MAX_MEMBERS {
                2 + 8 * recips
            } else {
                0
            };
            per_msg_meta += recips * (41 + 10 + sub_tlv);
            delivered_envs += recips;
            for _ in 0..RELAY_TX_PER_MSG {
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
        per_message_metadata: per_msg_meta as f64 / delivered_envs.max(1) as f64,
        per_message_limit: 256f64.max(0.1 * 256.0),
    }
}
