//! Open Mesh Chat Protocol (`omcp-core`) — M1 + M2 core library.
//!
//! Provides:
//! - Canonical [`framing::EnvelopeV1`] encode/decode (PROTOCOL §10.1, §21.1–§21.4)
//! - Version / suite negotiation hooks (PROTOCOL §5.2, §10.3)
//! - Noise_IK + ML-KEM-768 hybrid (`OMCP-Noise-Hybrid-v1`, PROTOCOL §21.5)
//! - BLE loopback transport: fragment AEAD, discovery, gossip seen-cache, and
//!   two-peer encrypted text chat over an in-memory GATT pipe (PROTOCOL §8, §10.2)
//!
//! Not in scope yet: real phone BLE bindings, MLS groups, gateway, production hardware.

#![forbid(unsafe_code)]
#![allow(missing_docs)] // field-level docs filled as API stabilizes

pub mod ble;
pub mod error;
pub mod framing;
pub mod handshake;
pub mod negotiate;
pub mod suite;

pub use error::{OmcpError, Result};
pub use framing::EnvelopeV1;
pub use handshake::{
    HybridInitiator, HybridResponder, HybridSessionKeys, StaticIdentity, HYBRID_PROTOCOL_NAME,
};
pub use negotiate::{negotiate, Hello, Negotiated};
pub use suite::{SUITE_OMCP_2026_A, SUITE_OMCP_2026_A_NAME};

#[cfg(test)]
mod m2_integration {
    //! End-to-end M2: Noise hybrid + BLE framing + 1:1 text over loopback.

    use std::time::Instant;

    use crate::ble::fragment::{fragment_and_seal, random_frag_id, FragmentReassembler};
    use crate::ble::gossip::SeenCache;
    use crate::ble::link::LinkSession;
    use crate::ble::loopback::BleLoopback;
    use crate::ble::message::{open_text_envelope, seal_text_envelope, CONTENT_TYPE_TEXT_PLAIN};
    use crate::ble::{capabilities, DiscoveryAdvertisement};
    use crate::framing::EnvelopeV1;
    use crate::handshake::{HybridInitiator, HybridResponder, StaticIdentity};
    use crate::negotiate::{negotiate, Hello};

    /// Send an encoded envelope as BLE fragments over the loopback endpoint.
    fn send_envelope(link: &mut LinkSession, ep: &crate::ble::BleEndpoint, envelope_bytes: &[u8]) {
        let frag_id = random_frag_id().unwrap();
        let frames = fragment_and_seal(link, envelope_bytes, frag_id, 512).unwrap();
        for f in frames {
            ep.send(f).unwrap();
        }
    }

    /// Receive and reassemble one full envelope from the loopback endpoint.
    fn recv_envelope(
        link: &mut LinkSession,
        ep: &crate::ble::BleEndpoint,
        reasm: &mut FragmentReassembler,
    ) -> EnvelopeV1 {
        let now = Instant::now();
        loop {
            let frame = ep
                .recv()
                .unwrap()
                .expect("expected BLE frame for envelope reassembly");
            if let Some(bytes) = reasm.push_frame(link, &frame, now).unwrap() {
                return EnvelopeV1::decode(&bytes).unwrap();
            }
        }
    }

    #[test]
    fn two_peer_ble_loopback_text_chat() {
        // --- identities & discovery ads ---
        let id_a = StaticIdentity::generate().unwrap();
        let id_b = StaticIdentity::generate().unwrap();
        let peer_a = id_a.dh_public();
        let peer_b = id_b.dh_public();

        let ad_a = DiscoveryAdvertisement::new(1, &peer_a, capabilities::PQ);
        let ad_b = DiscoveryAdvertisement::new(1, &peer_b, capabilities::PQ);
        assert_eq!(
            DiscoveryAdvertisement::decode(&ad_a.encode())
                .unwrap()
                .peer_id_trunc,
            peer_a[..6]
        );
        assert_eq!(ad_b.magic, *b"OMCP");

        // --- loopback GATT pipe ---
        let (ep_a, ep_b) = BleLoopback::pair();

        // --- OMCP-Noise-Hybrid-v1 over the pipe (A initiator, B responder) ---
        // Handshake messages ride as single opaque frames (pre-link-AEAD).
        let mut init = HybridInitiator::new(id_a, &peer_b).unwrap();
        let mut resp = HybridResponder::new(id_b);

        let hello_a = Hello::local(["PQ", "BLE"]);
        let hello_b = Hello::local(["PQ", "BLE"]);
        let negotiated = negotiate(&hello_a, &hello_b).unwrap();
        assert_eq!(negotiated.suite_id, crate::SUITE_OMCP_2026_A);

        let msg1 = init.write_message_1(b"omcp-m2-hello-b").unwrap();
        ep_a.send(msg1).unwrap();
        let raw1 = ep_b.recv().unwrap().unwrap();
        let p1 = resp.read_message_1(&raw1).unwrap();
        assert_eq!(p1, b"omcp-m2-hello-b");

        let (msg2, keys_b) = resp.write_message_2(b"omcp-m2-hello-a").unwrap();
        ep_b.send(msg2).unwrap();
        let raw2 = ep_a.recv().unwrap().unwrap();
        let (p2, keys_a) = init.read_message_2(&raw2).unwrap();
        assert_eq!(p2, b"omcp-m2-hello-a");
        assert_eq!(keys_a.k_i2r, keys_b.k_i2r);

        let mut link_a = LinkSession::initiator(&keys_a);
        let mut link_b = LinkSession::responder(&keys_b);
        let mut reasm_a = FragmentReassembler::default();
        let mut reasm_b = FragmentReassembler::default();
        let mut seen_a = SeenCache::default();
        let mut seen_b = SeenCache::default();
        let mut app_send_a = 0u64;
        let mut app_recv_b = 0u64;
        let mut app_send_b = 0u64;
        let mut app_recv_a = 0u64;

        // --- A → B text ---
        let env_ab = seal_text_envelope(
            &link_a.send_key(),
            &peer_a,
            "hello from A over BLE loopback",
            1_729_000_000_001,
            &mut app_send_a,
            8,
        )
        .unwrap();
        assert_eq!(env_ab.header_aad, vec![CONTENT_TYPE_TEXT_PLAIN]);
        assert!(!seen_a.check_and_insert(env_ab.envelope_id, Instant::now()));
        let bytes_ab = env_ab.encode().unwrap();
        send_envelope(&mut link_a, &ep_a, &bytes_ab);

        let got_ab = recv_envelope(&mut link_b, &ep_b, &mut reasm_b);
        assert!(!seen_b.check_and_insert(got_ab.envelope_id, Instant::now()));
        assert!(seen_b.check_and_insert(got_ab.envelope_id, Instant::now())); // gossip dup
        let text_ab =
            open_text_envelope(&link_a.send_key(), &peer_a, &got_ab, &mut app_recv_b).unwrap();
        assert_eq!(text_ab, "hello from A over BLE loopback");

        // --- B → A text ---
        let env_ba = seal_text_envelope(
            &link_b.send_key(),
            &peer_b,
            "hello from B — M2 complete",
            1_729_000_000_002,
            &mut app_send_b,
            8,
        )
        .unwrap();
        let bytes_ba = env_ba.encode().unwrap();
        send_envelope(&mut link_b, &ep_b, &bytes_ba);

        let got_ba = recv_envelope(&mut link_a, &ep_a, &mut reasm_a);
        let text_ba =
            open_text_envelope(&link_b.send_key(), &peer_b, &got_ba, &mut app_recv_a).unwrap();
        assert_eq!(text_ba, "hello from B — M2 complete");
    }
}
