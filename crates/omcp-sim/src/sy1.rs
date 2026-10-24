//! SY1: per-node sync traffic in a large mesh with 1 % active nodes.
//!
//! Model (one simulated day, 96 contact windows of 15 min):
//! - Nodes sit on a torus grid; radio neighbours are within Chebyshev
//!   distance 2 (24 neighbours). Each window every node opens
//!   `encounters_per_window` links to random neighbours.
//! - Each node has ~50 1:1 contacts (half within distance 5, half anywhere)
//!   and 5 groups of 8 (local members) — ~55 rotating tags.
//! - 1 % of nodes are active and send `msgs_per_active` text messages a day.
//!
//! Executed per encounter, with real encodings: a hello carrying the 8-byte
//! digest id each way; a full padded cuckoo digest each way when the peer's
//! cached one is from an older 6 h tag epoch; and range reconciliation over
//! the chats both peers are members of (real `recon::run_session`; sessions
//! between peers whose shared chats are identical are charged the measured
//! cost of an in-sync session, which is what running them would give).
//!
//! Modelled per message: binary spray-and-wait (L = 8, k = 2) charged as 7
//! envelope transmissions among the sender's radio neighbours, then one
//! delivery transmission per recipient member. Both ends of every
//! transmission are charged (bytes sent + received).

use omcp_core::sync::cuckoo::CuckooFilter;
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
}

const HELLO_BYTES: u64 = 4 + 8;
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
    let radio: Vec<Vec<u32>> = (0..n).map(|i| neighbours(i, side, n, 2)).collect();
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

    // Digests per epoch (real padded cuckoo filters) — size + id.
    let windows_per_epoch = p.windows.div_ceil(TAG_EPOCHS_PER_DAY);
    let mut digest_len = vec![0u64; n];
    let mut seen_epoch: Vec<Vec<u8>> = radio.iter().map(|r| vec![u8::MAX; r.len()]).collect();
    let mut recon_sessions = 0u64;
    let mut digests_sent = 0u64;
    let mut mi = 0;
    for w in 0..p.windows {
        let epoch = w / windows_per_epoch;
        if w % windows_per_epoch == 0 {
            for i in 0..n {
                let tags: Vec<MailboxTag> = member_of[i]
                    .iter()
                    .map(|c| {
                        let mut s = [0u8; 32];
                        s[..4].copy_from_slice(&c.to_be_bytes());
                        s[4..12].copy_from_slice(&p.seed.to_be_bytes());
                        mailbox_tag(&s, epoch as u64).expect("tag")
                    })
                    .collect();
                let f = CuckooFilter::build_digest(&tags, 64, &mut rng).expect("digest");
                digest_len[i] = f.encoded_len() as u64;
            }
        }
        // New messages: spray + delivery to members (logs are then in sync).
        while mi < msgs.len() && msgs[mi].0 == w {
            let (_, c, s) = msgs[mi];
            mi += 1;
            let s = s as usize;
            let ts = (w as u64) << 32 | mi as u64;
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
            log[c as usize].push(Item { ts, id });
            let sprays = SPRAY_COPIES as usize - 1;
            for _ in 0..sprays {
                let a = s;
                let b = radio[s][rng.below(radio[s].len() as u64) as usize] as usize;
                data[a] += TEXT_ENVELOPE_BYTES;
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
            }
        }
        // Encounters.
        for a in 0..n {
            for _ in 0..p.encounters_per_window {
                let k = rng.below(radio[a].len() as u64) as usize;
                let b = radio[a][k] as usize;
                let mut cost = 2 * HELLO_BYTES;
                if seen_epoch[a][k] != epoch as u8 {
                    seen_epoch[a][k] = epoch as u8;
                    cost += digest_len[a] + digest_len[b];
                    digests_sent += 2;
                }
                // Shared chats (both members): logs are in sync after delivery.
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
                    cost += in_sync_cost;
                }
                meta[a] += cost;
                meta[b] += cost;
            }
        }
    }

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
    }
}
