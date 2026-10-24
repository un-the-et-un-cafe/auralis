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
    la.shutdown();
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

#[test]
fn backup_roundtrip_restores_identity_contacts_and_verification() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    a.set_verified(c.contact_id.clone(), true).unwrap();
    let pw = "correct horse battery staple 42".to_string();
    assert!(check_backup_password(pw.clone()).ok);
    assert!(!check_backup_password("password".into()).ok);
    let blob = a.create_backup(pw.clone(), "Pixel 8".into(), 42).unwrap();
    let phrase = a.recovery_phrase().unwrap();
    assert!(!blob.windows(phrase.len()).any(|w| w == phrase.as_bytes()));
    let info = backup_info(blob.clone()).unwrap();
    assert!(info.needs_password && info.created_at_ms == 42);
    assert!(matches!(
        open_backup_with_password(blob.clone(), "nope nope nope 1".into()),
        Err(FfiError::WrongBackupPassword)
    ));
    let r = open_backup_with_password(blob, pw).unwrap();
    assert_eq!(r.phrase, phrase);
    let a2 = OmcpNode::from_phrase(r.phrase, "Alice".into()).unwrap();
    assert_eq!(a2.identity().peer_id_hex, a.identity().peer_id_hex);
    for k in r.contacts {
        a2.restore_contact(k.contact_id, k.display_name, k.verified)
            .unwrap();
    }
    assert_eq!(a2.contacts(), a.contacts());
}

// ---- burn after read end to end (PROTOCOL §19.1) ----

fn pump_all(
    la: &MeshLink,
    lb: &MeshLink,
    out: LinkOutput,
) -> (Vec<ReceivedText>, Vec<OpenedEvent>) {
    // Like `pump`, but also collects OPENED events from both sides.
    let mut got = Vec::new();
    let mut opened = Vec::new();
    let mut queue = vec![(false, out.outgoing)];
    opened.extend(out.opened);
    while let Some((back, pdus)) = queue.pop() {
        for p in pdus {
            let dst = if back { la } else { lb };
            let o = dst.on_pdu(p).unwrap();
            got.extend(o.received);
            opened.extend(o.opened);
            if !o.outgoing.is_empty() {
                queue.push((!back, o.outgoing));
            }
        }
    }
    (got, opened)
}

fn spec(mode: BurnMode, secs: u32, ttl: u32) -> BurnSpec {
    BurnSpec {
        mode,
        secs,
        unread_ttl: ttl,
    }
}

#[test]
fn burn_timer_and_opened_shred_both_sides() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_message(
            bob_id,
            "secret".into(),
            Some(spec(BurnMode::Timer, 10, 604_800)),
        )
        .unwrap();
    assert!(a.burn_is_live(id.clone()));
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    let m = got.single_or_panic();
    assert_eq!(m.text, "", "burn content not in the received list");
    assert_eq!(m.burn, Some(spec(BurnMode::Timer, 10, 604_800)));
    assert_eq!(m.envelope_id_hex, id);

    let c = b.open_burn(id.clone(), 1_000).unwrap().unwrap();
    assert_eq!(c.text, "secret");
    assert_eq!(c.burn_at_ms, Some(11_000));
    assert!(
        b.open_burn(id.clone(), 1_001).unwrap().is_none(),
        "opens once"
    );
    let (_, opened) = pump_all(&lb, &la, lb.flush().unwrap());
    assert_eq!(opened.len(), 1);
    assert!(!opened[0].shredded);
    assert_eq!(opened[0].burn_at_ms, Some(11_000));
    assert!(b.tick_burn(10_999).is_empty());
    assert_eq!(b.tick_burn(11_000), vec![id.clone()]);
    assert_eq!(a.tick_burn(11_000), vec![id.clone()]);
    assert!(!a.burn_is_live(id.clone()) && !b.burn_is_live(id));
}

#[test]
fn burn_view_once_media_shreds_sender_on_opened() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_media(
            bob_id,
            "image/jpeg".into(),
            vec![0xFF, 0xD8, 1],
            Some(spec(BurnMode::OnOpen, 0, 86_400)),
        )
        .unwrap();
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    assert_eq!(got.single_or_panic().mime.as_deref(), Some("image/jpeg"));
    let c = b.open_burn(id.clone(), 5).unwrap().unwrap();
    assert_eq!(c.bytes, vec![0xFF, 0xD8, 1]);
    assert!(
        !b.burn_is_live(id.clone()),
        "view once: receiver copy shredded on view"
    );
    assert!(b.open_burn(id.clone(), 6).unwrap().is_none());
    let (_, opened) = pump_all(&lb, &la, lb.flush().unwrap());
    assert!(opened.single_or_panic().shredded);
    assert!(!a.burn_is_live(id));
}

#[test]
fn burn_unread_ttl_expires_both_sides_and_bad_specs_rejected() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let s = spec(BurnMode::OnOpen, 0, 3_600);
    let id = a
        .queue_message(bob_id.clone(), "never".into(), Some(s))
        .unwrap();
    let (got, _) = pump_all(&la, &lb, la.flush().unwrap());
    let sent = got.single_or_panic().created_ms;
    let exp = burn_expires_at_ms(sent, s).unwrap();
    assert_eq!(exp, sent + 3_600_000);
    assert!(b.tick_burn(exp - 1).is_empty());
    assert_eq!(b.tick_burn(exp), vec![id.clone()]);
    assert!(b.open_burn(id.clone(), exp).unwrap().is_none());
    assert_eq!(a.tick_burn(exp + 1_000), vec![id]); // sender copy expires too
    assert!(!burn_spec_valid(spec(BurnMode::Timer, 11, 3_600)));
    assert!(a
        .queue_message(bob_id, "x".into(), Some(spec(BurnMode::OnOpen, 0, 5)))
        .is_err());
}

#[test]
fn opened_from_wrong_peer_is_ignored_and_not_counted_as_flushed() {
    let (a, b, la, lb) = paired();
    let bob_id = a.contacts()[0].contact_id.clone();
    let _ = pump_all(&la, &lb, la.start().unwrap());
    let id = a
        .queue_message(bob_id, "t".into(), Some(spec(BurnMode::Timer, 60, 604_800)))
        .unwrap();
    let out = la.flush().unwrap();
    assert_eq!(out.flushed, 1);
    let _ = pump_all(&la, &lb, out);
    b.open_burn(id, 0).unwrap().unwrap();
    assert_eq!(
        b.queued_for(a.identity().peer_id_hex),
        0,
        "OPENED is control, not user-queued"
    );
    assert_eq!(lb.flush().unwrap().flushed, 0);
}

trait Single<T> {
    fn single_or_panic(self) -> T;
}
impl<T: std::fmt::Debug> Single<T> for Vec<T> {
    fn single_or_panic(mut self) -> T {
        assert_eq!(self.len(), 1, "{self:?}");
        self.pop().unwrap()
    }
}

#[test]
fn icloud_key_backup_roundtrip_and_no_plaintext() {
    let a = node("Alice");
    let b = node("Bob");
    let c = a.add_contact_from_qr(b.qr_payload()).unwrap();
    a.set_verified(c.contact_id.clone(), true).unwrap();
    let key = generate_backup_key().unwrap();
    assert_eq!(key.len(), 32);
    let blob = a
        .create_backup_with_key(key.clone(), "iPhone".into(), 7)
        .unwrap();
    let info = backup_info(blob.clone()).unwrap();
    assert!(!info.needs_password);
    // O2: neither the phrase words nor the key appear in the blob.
    let phrase = a.recovery_phrase().unwrap();
    for w in phrase.split(' ').filter(|w| w.len() >= 5) {
        assert!(!blob.windows(w.len()).any(|x| x == w.as_bytes()));
    }
    assert!(!blob.windows(32).any(|x| x == key.as_slice()));
    let r = open_backup_with_key(blob.clone(), key).unwrap();
    assert_eq!(r.phrase, phrase);
    assert!(r.contacts[0].verified);
    let wrong = generate_backup_key().unwrap();
    assert!(open_backup_with_key(blob.clone(), wrong).is_err());
    assert!(matches!(
        open_backup_with_key(blob, vec![0; 31]),
        Err(FfiError::Invalid { .. })
    ));
}

/// Like `pump`, but keeps calling `flush()` while the sender reports more file chunks.
fn pump_files(from: &MeshLink, to: &MeshLink) -> (Vec<ReceivedFile>, Vec<FileFailure>, u32) {
    let (mut files, mut fails, mut flushes) = (Vec::new(), Vec::new(), 0);
    loop {
        let out = from.flush().unwrap();
        flushes += 1;
        let more = out.more_pending;
        for p in out.outgoing {
            let o = to.on_pdu(p).unwrap();
            files.extend(o.files);
            fails.extend(o.file_failures);
        }
        if !more {
            return (files, fails, flushes);
        }
    }
}

#[test]
fn file_transfer_end_to_end_in_64k_chunks() {
    let (a, _b, la, lb) = paired();
    let start = la.start().unwrap();
    pump(&la, &lb, start);
    let bob = a.contacts()[0].contact_id.clone();
    let data: Vec<u8> = (0..(3 * FILE_CHUNK_BYTES + 123))
        .map(|i| (i * 7) as u8)
        .collect();
    let offer = a
        .queue_file(
            bob.clone(),
            "flyer.jpg".into(),
            "image/jpeg".into(),
            data.clone(),
        )
        .unwrap();
    assert_eq!(offer.num_chunks, 4);
    assert_eq!(a.files_pending_for(bob.clone()), 1);
    let (files, fails, flushes) = pump_files(&la, &lb);
    assert!(fails.is_empty());
    assert!(flushes >= 2, "chunks must be drained over several flushes");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].name, "flyer.jpg");
    assert_eq!(files[0].mime, "image/jpeg");
    assert_eq!(files[0].bytes, data);
    assert_eq!(a.files_pending_for(bob), 0);
}

#[test]
fn file_too_large_rejected_before_encryption_and_cancel() {
    let (a, _b, _la, _lb) = paired();
    let bob = a.contacts()[0].contact_id.clone();
    let big = vec![0u8; (max_file_bytes() + 1) as usize];
    assert!(matches!(
        a.queue_file(bob.clone(), "x.bin".into(), "".into(), big),
        Err(FfiError::FileTooLarge { .. })
    ));
    let o = a
        .queue_file(bob.clone(), "e.txt".into(), "text/plain".into(), vec![])
        .unwrap();
    assert_eq!(o.num_chunks, 1);
    assert!(a.cancel_file(o.transfer_id_hex.clone()));
    assert!(!a.cancel_file(o.transfer_id_hex));
    assert_eq!(a.files_pending_for(bob), 0);
}

#[test]
fn interrupted_file_transfer_completes_on_reconnect() {
    let (a, b, la, lb) = paired();
    let start = la.start().unwrap();
    pump(&la, &lb, start);
    let bob = a.contacts()[0].contact_id.clone();
    let data: Vec<u8> = (0..(5 * FILE_CHUNK_BYTES))
        .map(|i| (i % 251) as u8)
        .collect();
    a.queue_file(
        bob.clone(),
        "v.mp4".into(),
        "video/mp4".into(),
        data.clone(),
    )
    .unwrap();
    // First link: one flush delivered, then the link drops.
    let out = la.flush().unwrap();
    assert!(out.more_pending);
    for p in out.outgoing {
        assert!(lb.on_pdu(p).unwrap().files.is_empty());
    }
    la.shutdown();
    lb.shutdown();
    // Reconnect: offer re-sent, chunks replayed, receiver keeps partial state.
    let la2 = a.open_initiator_link(bob).unwrap();
    let lb2 = b.open_responder_link().unwrap();
    let start = la2.start().unwrap();
    pump(&la2, &lb2, start);
    let (files, fails, _) = pump_files(&la2, &lb2);
    assert!(fails.is_empty());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, data);
}

// ---- encrypted local store: survives a restart ----

const SKEY: [u8; 32] = [0x5c; 32];

fn open_node(phrase: &str, name: &str, dir: &std::path::Path) -> Arc<OmcpNode> {
    OmcpNode::open(
        phrase.into(),
        name.into(),
        dir.join("omcp.db").to_string_lossy().into_owned(),
        SKEY.to_vec(),
    )
    .unwrap()
}

fn disk_bytes(dir: &std::path::Path) -> Vec<u8> {
    let mut all = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        all.extend(std::fs::read(e.unwrap().path()).unwrap());
    }
    all
}

fn has(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn contacts_chats_messages_and_queue_survive_restart() {
    let dir = tempfile::tempdir().unwrap();
    let pa = generate_recovery_phrase().unwrap();
    let b = node("Bob");
    let bob_id;
    {
        let a = open_node(&pa, "Alice", dir.path());
        b.add_contact_from_qr(a.qr_payload()).unwrap();
        let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
        bob_id = cb.contact_id.clone();
        a.set_verified(bob_id.clone(), true).unwrap();
        // One message exchanged now, one left in the offline queue.
        let la = a.open_initiator_link(bob_id.clone()).unwrap();
        let lb = b.open_responder_link().unwrap();
        pump(&la, &lb, la.start().unwrap());
        let got = pump(
            &lb,
            &la,
            lb.send_text("hello Alice, from Bob".into()).unwrap(),
        );
        assert_eq!(got[0].text, "hello Alice, from Bob");
        la.shutdown();
        a.queue_text(bob_id.clone(), "queued while offline".into())
            .unwrap();
        assert_eq!(a.queued_for(bob_id.clone()), 1);
    }
    let raw = disk_bytes(dir.path());
    assert!(!has(&raw, b"queued while offline"));
    assert!(!has(&raw, b"hello Alice"));
    assert!(!has(&raw, b"Bob"));

    // "Restart": a fresh node over the same file and key.
    let a = open_node(&pa, "Alice", dir.path());
    let c = a.contacts();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].display_name, "Bob");
    assert!(c[0].verified);
    assert_eq!(a.chats().unwrap()[0].contact_id, bob_id);
    let ms = a.stored_messages(bob_id.clone()).unwrap();
    let texts: Vec<_> = ms
        .iter()
        .map(|m| (m.text.as_str(), m.mine, m.status))
        .collect();
    assert_eq!(
        texts,
        [
            ("hello Alice, from Bob", false, MessageStatus::Received),
            ("queued while offline", true, MessageStatus::Queued),
        ]
    );
    assert_eq!(a.queued_for(bob_id.clone()), 1);

    // The restored queue is delivered on the next link and marked sent.
    let la = a.open_initiator_link(bob_id.clone()).unwrap();
    let lb = b.open_responder_link().unwrap();
    let got = pump(&la, &lb, la.start().unwrap());
    assert_eq!(got[0].text, "queued while offline");
    assert_eq!(a.queued_for(bob_id.clone()), 0);
    assert_eq!(
        a.stored_messages(bob_id.clone()).unwrap()[1].status,
        MessageStatus::Sent
    );
    drop((la, lb, a));
    let a = open_node(&pa, "Alice", dir.path());
    assert_eq!(a.queued_for(bob_id), 0);
}

#[test]
fn wrong_storage_key_and_other_identity_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let pa = generate_recovery_phrase().unwrap();
    drop(open_node(&pa, "A", dir.path()));
    let path = dir.path().join("omcp.db").to_string_lossy().into_owned();
    assert!(matches!(
        OmcpNode::open(pa.clone(), "A".into(), path.clone(), vec![1; 32]),
        Err(FfiError::Storage { .. })
    ));
    assert!(matches!(
        OmcpNode::open(pa, "A".into(), path.clone(), vec![1; 31]),
        Err(FfiError::Invalid { .. })
    ));
    let other = generate_recovery_phrase().unwrap();
    assert!(matches!(
        OmcpNode::open(other, "B".into(), path, SKEY.to_vec()),
        Err(FfiError::Storage { .. })
    ));
    assert_eq!(storage_schema_version(), 1);
    assert_eq!(generate_storage_key().unwrap().len(), 32);
}

/// PROTOCOL §19.1 / REQUIREMENTS B1: an unopened burn message survives a
/// restart (and opens exactly once); once burned, neither it nor its key can be
/// recovered from disk, and it doesn't come back after another restart.
#[test]
fn burn_survives_restart_until_burned_then_is_unrecoverable() {
    const SECRET: &str = "burn me: the spare key is under the blue flowerpot";
    let dir = tempfile::tempdir().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let a = node("Alice");
    let (alice_id, msg_id);
    {
        let b = open_node(&pb, "Bob", dir.path());
        alice_id = b.add_contact_from_qr(a.qr_payload()).unwrap().contact_id;
        let cb = a.add_contact_from_qr(b.qr_payload()).unwrap();
        msg_id = a
            .queue_message(
                cb.contact_id.clone(),
                SECRET.into(),
                Some(spec(BurnMode::Timer, 10, core_burn::UNREAD_TTL_7D)),
            )
            .unwrap();
        let la = a.open_initiator_link(cb.contact_id).unwrap();
        let lb = b.open_responder_link().unwrap();
        let (got, _) = pump_all(&la, &lb, la.start().unwrap());
        assert_eq!(got[0].text, "");
        assert!(got[0].burn.is_some());
        lb.shutdown();
    }
    assert!(!has(&disk_bytes(dir.path()), SECRET.as_bytes()));

    let t0 = now_ms();
    {
        let b = open_node(&pb, "Bob", dir.path());
        assert!(b.burn_is_live(msg_id.clone()));
        let m = &b.stored_messages(alice_id.clone()).unwrap()[0];
        assert_eq!(
            (m.text.as_str(), m.burn_state),
            ("", StoredBurnState::Unopened)
        );
        let c = b.open_burn(msg_id.clone(), t0).unwrap().unwrap();
        assert_eq!(c.text, SECRET);
        assert_eq!(c.burn_at_ms, Some(t0 + 10_000));
    }
    // Restart while open: content is never handed out a second time.
    {
        let b = open_node(&pb, "Bob", dir.path());
        assert_eq!(b.open_burn(msg_id.clone(), t0 + 1).unwrap(), None);
        assert_eq!(
            b.stored_messages(alice_id.clone()).unwrap()[0].burn_state,
            StoredBurnState::Open
        );
        // Control messages (the OPENED receipt) aren't user-visible queue items.
        assert_eq!(b.path_status().queued_count, 0);
        assert_eq!(b.tick_burn(t0 + 10_000), vec![msg_id.clone()]);
        assert!(!b.burn_is_live(msg_id.clone()));
        assert!(b.stored_messages(alice_id.clone()).unwrap().is_empty());
    }
    let b = open_node(&pb, "Bob", dir.path());
    assert!(!b.burn_is_live(msg_id.clone()));
    assert_eq!(b.open_burn(msg_id.clone(), t0 + 20_000).unwrap(), None);
    assert!(b.stored_messages(alice_id).unwrap().is_empty());
    drop(b);
    assert!(!has(&disk_bytes(dir.path()), SECRET.as_bytes()));
}

#[test]
fn backup_carries_history_but_never_burn_content() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let bob = a.contacts()[0].contact_id.clone();
    pump(&la, &lb, la.send_text("kept in the backup".into()).unwrap());
    a.queue_message(
        bob.clone(),
        "burn never in backup".into(),
        Some(spec(BurnMode::OnOpen, 0, core_burn::UNREAD_TTL_1H)),
    )
    .unwrap();
    let key = generate_backup_key().unwrap();
    let blob = a
        .create_backup_with_key(key.clone(), "d".into(), 1)
        .unwrap();
    let r = open_backup_with_key(blob, key).unwrap();
    let h = r.history.clone().unwrap();
    assert!(!has(&h, b"burn never in backup"));

    let dir = tempfile::tempdir().unwrap();
    let a2 = open_node(&r.phrase, "Alice", dir.path());
    for k in &r.contacts {
        a2.restore_contact(k.contact_id.clone(), k.display_name.clone(), k.verified)
            .unwrap();
    }
    a2.restore_history(h).unwrap();
    let ms = a2.stored_messages(bob.clone()).unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].text, "kept in the backup");
    drop(a2);
    let a3 = open_node(&r.phrase, "Alice", dir.path());
    assert_eq!(a3.stored_messages(bob).unwrap().len(), 1);
    drop(b);
}

#[test]
fn outgoing_file_and_received_file_reference_survive_restart() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();
    let (pa, pb) = (
        generate_recovery_phrase().unwrap(),
        generate_recovery_phrase().unwrap(),
    );
    let data: Vec<u8> = (0..(2 * FILE_CHUNK_BYTES + 7)).map(|i| i as u8).collect();
    let (bob, alice, tid);
    {
        let a = open_node(&pa, "Alice", dir_a.path());
        let b = open_node(&pb, "Bob", dir_b.path());
        alice = b.add_contact_from_qr(a.qr_payload()).unwrap().contact_id;
        bob = a.add_contact_from_qr(b.qr_payload()).unwrap().contact_id;
        tid = a
            .queue_file(
                bob.clone(),
                "notes.txt".into(),
                "text/plain".into(),
                data.clone(),
            )
            .unwrap()
            .transfer_id_hex;
    }
    // Restart before anything was sent: the transfer is still queued.
    let a = open_node(&pa, "Alice", dir_a.path());
    let b = open_node(&pb, "Bob", dir_b.path());
    assert_eq!(a.files_pending_for(bob.clone()), 1);
    let m = &a.stored_messages(bob.clone()).unwrap()[0];
    assert_eq!(m.file.as_ref().unwrap().state, StoredFileState::Queued);
    let la = a.open_initiator_link(bob.clone()).unwrap();
    let lb = b.open_responder_link().unwrap();
    pump(&la, &lb, la.start().unwrap());
    let (files, fails, _) = pump_files(&la, &lb);
    assert!(fails.is_empty());
    assert_eq!(files[0].bytes, data);
    assert_eq!(a.files_pending_for(bob.clone()), 0);
    b.set_received_file_path(tid.clone(), Some("/app/files/notes.txt".into()))
        .unwrap();
    drop((la, lb, a, b));

    let a = open_node(&pa, "Alice", dir_a.path());
    let b = open_node(&pb, "Bob", dir_b.path());
    assert_eq!(a.files_pending_for(bob.clone()), 0);
    let sent = a.stored_messages(bob).unwrap()[0].file.clone().unwrap();
    assert_eq!(sent.state, StoredFileState::Sent);
    let got = b.stored_messages(alice).unwrap()[0].file.clone().unwrap();
    assert_eq!(got.state, StoredFileState::Received);
    assert_eq!(got.local_path.as_deref(), Some("/app/files/notes.txt"));
    assert_eq!(
        (got.name.as_str(), got.size),
        ("notes.txt", data.len() as u64)
    );
}
