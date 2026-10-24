//! SY2 (in-sync reconnect ≤ 256 B) and SY3 (7-day catch-up ≤ 1.2× payload).

use ed25519_dalek::SigningKey;
use omcp_core::sync::recon::{run_session, Item, Reconciler};
use omcp_core::sync::watermark::{CatchUpRequest, SignedWatermark};
use omcp_core::sync::SplitMix64;

use crate::TEXT_ENVELOPE_BYTES;

#[derive(Debug, Clone)]
pub struct Sy2Report {
    pub items: usize,
    pub hello_bytes: usize,
    pub recon_bytes: usize,
    pub total_bytes: usize,
    pub round_trips: usize,
}

fn rand_items(rng: &mut SplitMix64, n: usize) -> Vec<Item> {
    (0..n)
        .map(|_| {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&rng.next_u64().to_le_bytes());
            id[8..].copy_from_slice(&rng.next_u64().to_le_bytes());
            Item {
                ts: rng.below(1 << 44),
                id,
            }
        })
        .collect()
}

/// Two peers sharing `items` envelopes across their common tags reconnect
/// with cached digests: one sync hello each way plus one reconciliation
/// session.
pub fn sy2(items: usize, seed: u64) -> Sy2Report {
    let mut rng = SplitMix64(seed);
    let s = rand_items(&mut rng, items);
    let (recon, rt, a, b) = run_session(&Reconciler::new(s.clone()), &Reconciler::new(s));
    assert!(a.is_empty() && b.is_empty());
    let hello = 2 * omcp_core::sync::hello::HELLO_BYTES as usize;
    Sy2Report {
        items,
        hello_bytes: hello,
        recon_bytes: recon,
        total_bytes: hello + recon,
        round_trips: rt,
    }
}

#[derive(Debug, Clone)]
pub struct Sy3Report {
    pub own_chats: usize,
    pub missed_envelopes: usize,
    pub missed_payload_bytes: u64,
    pub request_bytes: u64,
    pub transfer_bytes: u64,
    pub ratio: f64,
    pub foreign_bytes: u64,
}

/// A device offline for 7 days returns to a peer that holds its 55 chats
/// plus `foreign_chats` it is not a member of. It sends one signed-watermark
/// catch-up request per own chat; the peer returns only envelopes after the
/// watermark, each with a 4-byte length prefix.
pub fn sy3(own_chats: usize, foreign_chats: usize, msgs_per_chat_day: u64, seed: u64) -> Sy3Report {
    let mut rng = SplitMix64(seed);
    let sk = SigningKey::from_bytes(&[9u8; 32]);
    let days = 7u64;
    let mut request = 0u64;
    let mut transfer = 0u64;
    let mut missed = 0usize;
    let foreign_bytes = 0u64;
    for c in 0..own_chats + foreign_chats {
        let mine = c < own_chats;
        // History: lamport 1.. ; device saw up to `seen`.
        let seen = 1 + rng.below(50);
        let new = days * msgs_per_chat_day + rng.below(msgs_per_chat_day + 1);
        let hist: Vec<(u64, u64)> = (1..=seen + new).map(|l| (l, TEXT_ENVELOPE_BYTES)).collect();
        if !mine {
            // Not a member: no tag, no request, nothing transferred.
            continue;
        }
        let mut chat_id = [0u8; 32];
        chat_id[..8].copy_from_slice(&(c as u64).to_be_bytes());
        let req = CatchUpRequest {
            tag: (c as u64).to_be_bytes(),
            after: SignedWatermark::sign(&sk, chat_id, seen),
        };
        let wire = req.encode();
        let back = CatchUpRequest::decode(&wire).expect("roundtrip");
        back.after.verify().expect("sig");
        request += wire.len() as u64;
        for (_, sz) in back.select(&hist) {
            transfer += 4 + sz;
            missed += 1;
        }
    }
    let payload = missed as u64 * TEXT_ENVELOPE_BYTES;
    Sy3Report {
        own_chats,
        missed_envelopes: missed,
        missed_payload_bytes: payload,
        request_bytes: request,
        transfer_bytes: transfer,
        ratio: (request + transfer) as f64 / payload as f64,
        foreign_bytes,
    }
}
