//! Tiny M2 demo: two in-process peers complete Noise hybrid + BLE loopback text chat.
//!
//! Run: `cargo run --example m2_two_peer_chat -p omcp-core`
//! (example lives at workspace root — use `cargo run --example m2_two_peer_chat`)

use std::time::Instant;

use omcp_core::ble::fragment::{fragment_and_seal, random_frag_id, FragmentReassembler};
use omcp_core::ble::link::LinkSession;
use omcp_core::ble::loopback::BleLoopback;
use omcp_core::ble::message::{open_text_envelope, seal_text_envelope};
use omcp_core::ble::{capabilities, DiscoveryAdvertisement};
use omcp_core::{EnvelopeV1, HybridInitiator, HybridResponder, StaticIdentity, SUITE_OMCP_2026_A};

fn main() {
    let id_a = StaticIdentity::generate().expect("id A");
    let id_b = StaticIdentity::generate().expect("id B");
    let peer_a = id_a.dh_public();
    let peer_b = id_b.dh_public();

    let _ad = DiscoveryAdvertisement::new(1, &peer_a, capabilities::PQ);
    let (ep_a, ep_b) = BleLoopback::pair();

    let mut init = HybridInitiator::new(id_a, &peer_b).expect("initiator");
    let mut resp = HybridResponder::new(id_b);

    let msg1 = init.write_message_1(b"m2").expect("msg1");
    ep_a.send(msg1).unwrap();
    resp.read_message_1(&ep_b.recv().unwrap().unwrap()).unwrap();
    let (msg2, keys_b) = resp.write_message_2(b"m2").unwrap();
    ep_b.send(msg2).unwrap();
    let (_, keys_a) = init.read_message_2(&ep_a.recv().unwrap().unwrap()).unwrap();
    assert_eq!(keys_a.handshake_hash, keys_b.handshake_hash);

    let mut link_a = LinkSession::initiator(&keys_a);
    let mut link_b = LinkSession::responder(&keys_b);
    let mut reasm_b = FragmentReassembler::default();
    let mut send_ctr = 0u64;
    let mut recv_ctr = 0u64;

    let env = seal_text_envelope(&link_a.send_key(), &peer_a, "ping", 1, &mut send_ctr, 4).unwrap();
    assert_eq!(env.suite_id, SUITE_OMCP_2026_A);
    let frames = fragment_and_seal(
        &mut link_a,
        &env.encode().unwrap(),
        random_frag_id().unwrap(),
        512,
    )
    .unwrap();
    for f in frames {
        ep_a.send(f).unwrap();
    }
    let now = Instant::now();
    let mut decoded = None;
    while let Some(frame) = ep_b.recv().unwrap() {
        if let Some(bytes) = reasm_b.push_frame(&mut link_b, &frame, now).unwrap() {
            decoded = Some(EnvelopeV1::decode(&bytes).unwrap());
            break;
        }
    }
    let env = decoded.expect("reassembled");
    let text = open_text_envelope(&keys_a.k_i2r, &peer_a, &env, &mut recv_ctr).unwrap();
    assert_eq!(text, "ping");

    println!("M2 BLE loopback two-peer text chat: OK");
}
