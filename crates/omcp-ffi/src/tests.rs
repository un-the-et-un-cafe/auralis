use super::*;

fn node(name: &str) -> Arc<OmcpNode> {
    OmcpNode::from_phrase(generate_recovery_phrase().unwrap(), name.into()).unwrap()
}

/// Deliver every PDU in `out` to `to`, ping-ponging until quiet.
fn pump(from: &MeshLink, to: &MeshLink, out: LinkOutput) -> Vec<ReceivedText> {
    let mut got = Vec::new();
    let mut queue = vec![(false, out.outgoing)];
    while let Some((dir_back, pdus)) = queue.pop() {
        for p in pdus {
            let (dst, back) = if dir_back { (from, true) } else { (to, false) };
            let o = dst.on_pdu(p).unwrap();
            got.extend(o.received);
            if !o.outgoing.is_empty() {
                queue.push((!back, o.outgoing));
            }
        }
    }
    got
}

fn paired() -> (Arc<OmcpNode>, Arc<OmcpNode>, Arc<MeshLink>, Arc<MeshLink>) {
    let a = node("Alice");
    let b = node("Bob");
    let ca = b.add_contact_from_qr(a.qr_payload()).unwrap();
    let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert_eq!(ca.safety_number, cb.safety_number);
    let la = a.open_initiator_link(cb.contact_id).unwrap();
    let lb = b.open_responder_link().unwrap();
    (a, b, la, lb)
}

#[test]
fn phrase_roundtrip_restores_identity() {
    let p = generate_recovery_phrase().unwrap();
    assert_eq!(p.split(' ').count(), 24);
    assert!(validate_recovery_phrase(p.clone()));
    assert!(!validate_recovery_phrase("hello world".into()));
    let n1 = OmcpNode::from_phrase(p.clone(), "x".into()).unwrap();
    let n2 = OmcpNode::from_phrase(p, "x".into()).unwrap();
    assert_eq!(n1.identity(), n2.identity());
}

#[test]
fn qr_contact_add_and_errors() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert_eq!(c.display_name, "Bob");
    assert_eq!(c.safety_number.len(), 12);
    assert!(!c.verified);
    a.set_verified(c.contact_id.clone(), true).unwrap();
    assert!(a.contacts()[0].verified);
    assert!(a.add_contact_from_qr("https://evil".into()).is_err());
    assert!(a.add_contact_from_qr(a.qr_payload()).is_err());
    let prefix = parse_advertisement(b.advertisement()).unwrap();
    assert_eq!(a.contact_for_prefix(prefix), Some(c.contact_id));
}

#[test]
fn end_to_end_text_both_directions() {
    let (a, b, la, lb) = paired();
    let out = la.start().unwrap();
    assert!(out.outgoing.len() > 1, "handshake must be chunked");
    assert!(out.outgoing.iter().all(|p| p.len() <= 512));
    pump(&la, &lb, out);
    assert!(la.is_established() && lb.is_established());
    assert_eq!(a.path_status().path, PathKind::Mesh);
    assert_eq!(b.path_status().nearby_peers, 1);

    let long = "é".repeat(1500); // multi-fragment
    let o = la.send_text(long.clone()).unwrap();
    assert!(o.outgoing.len() > 1);
    assert!(o.outgoing.iter().all(|p| p.len() <= 512));
    let got = pump(&la, &lb, o);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, long);
    assert_eq!(got[0].contact_id, a.identity().peer_id_hex);

    let got = pump(&lb, &la, lb.send_text("hi back".into()).unwrap());
    assert_eq!(got[0].text, "hi back");
    la.close();
    assert_eq!(a.path_status().nearby_peers, 0);
}

#[test]
fn offline_queue_flushes_on_connect() {
    let (a, _b, la, lb) = paired();
    let bid = a.contacts()[0].contact_id.clone();
    a.queue_text(bid.clone(), "one".into()).unwrap();
    la.send_text("two".into()).unwrap(); // not up yet → queued
    assert_eq!(a.queued_for(bid), 2);
    assert_eq!(a.path_status().path, PathKind::Queued);
    let got = pump(&la, &lb, la.start().unwrap());
    let texts: Vec<_> = got.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, ["one", "two"]);
    assert_eq!(a.path_status().queued_count, 0);
}

#[test]
fn responder_rejects_unknown_initiator() {
    let a = node("Alice");
    let b = node("Bob");
    let cb = a.add_contact_from_qr(b.qr_payload()).unwrap(); // b never adds a
    let la = a.open_initiator_link(cb.contact_id).unwrap();
    let lb = b.open_responder_link().unwrap();
    let mut err = None;
    for p in la.start().unwrap().outgoing {
        if let Err(e) = lb.on_pdu(p) {
            err = Some(e);
        }
    }
    assert!(matches!(err, Some(FfiError::UnknownContact)));
    assert!(!lb.is_established());
}

#[test]
fn link_pdu_before_handshake_is_error() {
    let b = node("Bob");
    let lb = b.open_responder_link().unwrap();
    assert!(lb.on_pdu(vec![PDU_LINK, 0, 1]).is_err());
    assert!(lb.on_pdu(vec![]).is_err());
    assert!(lb.on_pdu(vec![9]).is_err());
}

#[test]
fn gateway_policy_defaults() {
    assert_eq!(default_gateway_daily_cap_bytes(), 500_000_000);
    let wifi = gateway_decision(true, Network::Wifi { metered: false });
    assert!(wifi.bridge && !wifi.metered_warning);
    assert!(!gateway_decision(true, Network::Cellular { metered: true }).bridge);
    let cell = gateway_decision(false, Network::Cellular { metered: true });
    assert!(cell.bridge && cell.metered_warning);
    assert!(!gateway_decision(false, Network::Offline).bridge);
}

#[test]
fn file_size_limit_exposed() {
    assert_eq!(max_file_bytes(), 50_000_000);
    assert!(check_file_size(50_000_000).is_ok());
    assert!(matches!(
        check_file_size(50_000_001),
        Err(FfiError::FileTooLarge {
            size: 50_000_001,
            max: 50_000_000
        })
    ));
}

#[test]
fn recovery_word_helpers() {
    assert!(is_recovery_word("fossil".into()));
    assert_eq!(suggest_recovery_words("foss".into()), vec!["fossil"]);
}

#[test]
fn preview_detects_known_and_mismatch() {
    let a = node("Alice");
    let b = node("Bob");
    let p = a.preview_contact_from_qr(b.qr_payload()).unwrap();
    assert!(!p.already_known && p.mismatch_with.is_none());
    assert!(a.contacts().is_empty(), "preview must not save");
    a.add_contact_from_qr(b.qr_payload()).unwrap();
    assert!(
        a.preview_contact_from_qr(b.qr_payload())
            .unwrap()
            .already_known
    );
    let impostor = node("Bob");
    let p = a.preview_contact_from_qr(impostor.qr_payload()).unwrap();
    let old = p.mismatch_with.unwrap();
    assert_ne!(old.safety_number, p.contact.safety_number);
}
