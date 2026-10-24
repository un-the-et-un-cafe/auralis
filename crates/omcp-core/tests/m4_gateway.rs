//! M4 integration: two BLE meshes bridged by gateways via an in-memory rendezvous.

use omcp_core::error::{OmcpError, Result};
use omcp_core::framing::{flags, EnvelopeV1};
use omcp_core::gateway::{
    BusyReason, Gateway, GatewayControl, GatewayPinList, GatewayPolicy, NetworkKind,
    OpaqueEnvelope, StoreAndForward, SubmitOutcome,
};
use omcp_core::rendezvous::{
    blinded_label, epoch_for, sealed, EnqueueRequest, FetchRequest, InMemoryRendezvous,
    RendezvousIdentity, RendezvousToken, RendezvousTransport, TokenOp,
};
use omcp_core::suite::SUITE_OMCP_2026_A;

const NOW: u64 = 1_793_000_000_000;
const SECRET: &[u8] = b"top secret: meet at the old mill at 9";

/// Records every byte the gateway emits towards the Internet.
struct Recording<T> {
    inner: T,
    seen: Vec<Vec<u8>>,
    online: bool,
}

impl<T: RendezvousTransport> RendezvousTransport for Recording<T> {
    fn enqueue(&mut self, req: EnqueueRequest, now: u64) -> Result<()> {
        if !self.online {
            return Err(OmcpError::Rendezvous("unreachable"));
        }
        self.seen.push(req.encode()?);
        self.inner.enqueue(req, now)
    }
    fn fetch(&mut self, req: FetchRequest, now: u64) -> Result<Vec<OpaqueEnvelope>> {
        if !self.online {
            return Err(OmcpError::Rendezvous("unreachable"));
        }
        let out = self.inner.fetch(req, now)?;
        for e in &out {
            self.seen.push(e.encode()?);
        }
        Ok(out)
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// A builds an inner EnvelopeV1 (M1 framing) and seals it to B.
fn a_sends_to_b(b: &RendezvousIdentity, body: &[u8]) -> OpaqueEnvelope {
    let inner = EnvelopeV1 {
        version: 1,
        suite_id: SUITE_OMCP_2026_A,
        envelope_id: [0x11; 16],
        flags: flags::RELAY_OK | flags::STORE_OK,
        ttl_hops: 8,
        created_ms: NOW,
        header_aad: vec![],
        ciphertext: body.to_vec(), // stands in for M2/M3 app ciphertext
    };
    let label = blinded_label(&b.verifying_key(), epoch_for(NOW));
    OpaqueEnvelope {
        dest_label: label,
        suite_id: SUITE_OMCP_2026_A,
        expiry_ms: NOW + 3_600_000,
        ciphertext: sealed::seal(&b.sealed_public(), &label, &inner.encode().unwrap()).unwrap(),
    }
}

fn gw(policy: GatewayPolicy) -> Gateway {
    let mut g = Gateway::new(
        RendezvousIdentity::generate().unwrap(),
        policy,
        StoreAndForward::default(),
    );
    g.set_network(NetworkKind::Wifi { metered: false });
    g
}

#[test]
fn gateway_bridges_two_meshes_via_rendezvous() {
    let rv = InMemoryRendezvous::new();
    let mut t1 = Recording {
        inner: rv.clone(),
        seen: vec![],
        online: true,
    };
    let mut t2 = Recording {
        inner: rv.clone(),
        seen: vec![],
        online: true,
    };
    let b = RendezvousIdentity::generate().unwrap();
    let mut g1 = gw(GatewayPolicy::default());
    let mut g2 = gw(GatewayPolicy::default());

    // A picks a gateway honouring its pin list.
    let mut pins = GatewayPinList::new();
    pins.pin(g1.peer_id());
    let offers = [g2.offering(NOW), g1.offering(NOW)];
    assert_eq!(
        pins.select(&offers, &[]).unwrap().gateway_peer_id,
        g1.peer_id()
    );

    // Mesh 1: A -> G1
    let env = a_sends_to_b(&b, SECRET);
    assert_eq!(g1.submit_outbound(env, NOW).unwrap(), SubmitOutcome::Queued);
    for e in g1.outbound_snapshot() {
        assert!(!contains(&e.ciphertext, SECRET));
        assert!(!contains(&e.ciphertext, &b.verifying_key()));
    }
    let r1 = g1.pump(&mut t1, NOW).unwrap();
    assert_eq!((r1.bridged, r1.sent), (true, 1));
    assert_eq!(rv.stored_len(), 1);

    // Mesh 2: B opts in via G2 with a fetch token.
    let label = blinded_label(&b.verifying_key(), epoch_for(NOW));
    g2.register_fetch(RendezvousToken::issue(&b, TokenOp::Fetch, label, NOW, 60_000).unwrap())
        .unwrap();
    let r2 = g2.pump(&mut t2, NOW + 1).unwrap();
    assert_eq!(r2.fetched, 1);
    assert_eq!(g2.pending_local(&label), 1);
    let delivered = g2.deliver_local(&label, NOW + 2);

    // Gateways never saw plaintext nor B's identity key.
    for bytes in t1.seen.iter().chain(t2.seen.iter()) {
        assert!(!contains(bytes, SECRET));
        assert!(!contains(bytes, &b.verifying_key()));
        assert!(!contains(bytes, &b.sealed_public()));
    }

    // B opens.
    let pt = sealed::open(&b, &label, &delivered[0].ciphertext).unwrap();
    let inner = EnvelopeV1::decode(&pt).unwrap();
    assert_eq!(inner.ciphertext, SECRET);
    // Another identity cannot open it.
    let eve = RendezvousIdentity::generate().unwrap();
    assert!(sealed::open(&eve, &label, &delivered[0].ciphertext).is_err());
}

#[test]
fn gateway_daily_cap_emits_busy_and_resets_next_day() {
    let rv = InMemoryRendezvous::new();
    let b = RendezvousIdentity::generate().unwrap();
    let one = a_sends_to_b(&b, SECRET).wire_len() as u64;
    let mut g = gw(GatewayPolicy {
        daily_byte_cap: one * 2,
        ..Default::default()
    });
    assert_eq!(GatewayPolicy::default().daily_byte_cap, 500_000_000);
    for _ in 0..2 {
        assert_eq!(
            g.submit_outbound(a_sends_to_b(&b, SECRET), NOW).unwrap(),
            SubmitOutcome::Queued
        );
    }
    let mut t = rv.clone();
    assert_eq!(g.pump(&mut t, NOW).unwrap().sent, 2);
    match g.submit_outbound(a_sends_to_b(&b, SECRET), NOW).unwrap() {
        SubmitOutcome::Busy(GatewayControl::Busy { reason, .. }) => {
            assert_eq!(reason, BusyReason::DailyCapReached)
        }
        o => panic!("expected busy, got {o:?}"),
    }
    assert_eq!(g.offering(NOW).remaining_bytes_today, 0);
    let tomorrow = NOW + 86_400_000;
    let mut fresh = a_sends_to_b(&b, SECRET);
    fresh.expiry_ms = tomorrow + 3_600_000;
    assert_eq!(
        g.submit_outbound(fresh, tomorrow).unwrap(),
        SubmitOutcome::Queued
    );
}

#[test]
fn gateway_outage_stops_bridging_then_drains() {
    let rv = InMemoryRendezvous::new();
    let b = RendezvousIdentity::generate().unwrap();
    let mut g = gw(GatewayPolicy::default());
    let mut t = Recording {
        inner: rv.clone(),
        seen: vec![],
        online: true,
    };

    g.set_network(NetworkKind::Offline);
    for _ in 0..3 {
        g.submit_outbound(a_sends_to_b(&b, SECRET), NOW).unwrap();
    }
    let r = g.pump(&mut t, NOW).unwrap();
    assert!(!r.bridged);
    assert!(matches!(
        r.busy,
        Some(GatewayControl::Busy {
            reason: BusyReason::Offline,
            ..
        })
    ));
    assert_eq!(g.queue_len(), 3);

    // OS says online but transport is down: keep queue.
    g.set_network(NetworkKind::Wifi { metered: false });
    t.online = false;
    assert!(!g.pump(&mut t, NOW + 10).unwrap().bridged);
    assert_eq!(g.queue_len(), 3);

    // Internet returns: drain.
    t.online = true;
    let r = g.pump(&mut t, NOW + 20).unwrap();
    assert_eq!(r.sent, 3);
    assert_eq!(g.queue_len(), 0);
    assert_eq!(rv.stored_len(), 3);
}

#[test]
fn gateway_wifi_only_default_and_metered_warning() {
    let rv = InMemoryRendezvous::new();
    let b = RendezvousIdentity::generate().unwrap();
    let mut t = rv.clone();
    let mut g = gw(GatewayPolicy::default());
    g.set_network(NetworkKind::Cellular { metered: true });
    g.submit_outbound(a_sends_to_b(&b, SECRET), NOW).unwrap();
    let r = g.pump(&mut t, NOW).unwrap();
    assert!(!r.bridged && r.sent == 0);

    let mut g = gw(GatewayPolicy {
        wifi_only: false,
        ..Default::default()
    });
    g.set_network(NetworkKind::Cellular { metered: true });
    g.submit_outbound(a_sends_to_b(&b, SECRET), NOW).unwrap();
    let r = g.pump(&mut t, NOW).unwrap();
    assert!(r.bridged && r.metered_warning && r.sent == 1); // warn, not block
}
