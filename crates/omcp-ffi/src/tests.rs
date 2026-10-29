use super::*;

fn node(name: &str) -> Arc<OmcpNode> {
    OmcpNode::from_phrase(generate_recovery_phrase().unwrap(), name.into()).unwrap()
}

/// Simulate OS queue acceptance, not a remote protocol receipt.
fn accept(link: &MeshLink, mut out: LinkOutput) -> LinkOutput {
    if let Some(token) = out.batch_token.clone() {
        let committed = link.commit_outgoing(token).unwrap();
        out.flushed += committed.flushed;
        out.file_progress.extend(committed.file_progress);
        out.more_pending = committed.more_pending;
    }
    out
}

/// Deliver every PDU in `out` to `to`, ping-ponging until quiet.
fn pump(from: &MeshLink, to: &MeshLink, out: LinkOutput) -> Vec<ReceivedText> {
    let out = accept(from, out);
    let mut got = Vec::new();
    let mut queue = vec![(false, out.outgoing)];
    while let Some((dir_back, pdus)) = queue.pop() {
        for p in pdus {
            let (dst, back) = if dir_back { (from, true) } else { (to, false) };
            let o = accept(dst, dst.on_pdu(p).unwrap());
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
fn outgoing_text_does_not_populate_inbound_control_replay_cache() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    assert!(a.inner.lock().seen.is_empty());
    assert!(b.inner.lock().seen.is_empty());
    let out = la
        .send_text("durable text, not a control-cache entry".into())
        .unwrap();
    assert!(!out.outgoing.is_empty());
    assert!(a.inner.lock().seen.is_empty());
    assert_eq!(pump(&la, &lb, out).len(), 1);
    // User messages are protected by durable authenticated-peer/message IDs,
    // not raw outgoing envelope IDs in this transient inbound control cache.
    assert!(a.inner.lock().seen.is_empty());
    assert!(b.inner.lock().seen.is_empty());
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
    assert_eq!(r.display_name.as_deref(), Some("Alice"));
    let a2 = OmcpNode::from_phrase(r.phrase, "Alice".into()).unwrap();
    assert_eq!(a2.identity().peer_id_hex, a.identity().peer_id_hex);
    for k in r.contacts {
        a2.restore_contact(k.contact_id, k.display_name, k.verified)
            .unwrap();
    }
    let key = |v: Vec<ContactInfo>| {
        v.into_iter()
            .map(|c| (c.contact_id, c.display_name, c.verified))
            .collect::<Vec<_>>()
    };
    assert_eq!(key(a2.contacts()), key(a.contacts()));
}

// ---- burn after read end to end (PROTOCOL §19.1) ----

fn pump_all(
    la: &MeshLink,
    lb: &MeshLink,
    out: LinkOutput,
) -> (Vec<ReceivedText>, Vec<OpenedEvent>) {
    let out = accept(la, out);
    // Like `pump`, but also collects OPENED events from both sides.
    let mut got = Vec::new();
    let mut opened = Vec::new();
    let mut queue = vec![(false, out.outgoing)];
    opened.extend(out.opened);
    while let Some((back, pdus)) = queue.pop() {
        for p in pdus {
            let dst = if back { la } else { lb };
            let o = accept(dst, dst.on_pdu(p).unwrap());
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
    assert_eq!(out.flushed, 0); // only commit reports transport acceptance
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
    // Synthetic public test fixture, not a user's recovery material. The first
    // 11 entropy bits encode BIP39 word 485 ("device"). The checksum is generated
    // by the real encoder rather than constructing an invalid word sequence.
    let mut entropy = [0xa5; 32];
    entropy[0] = 0x3c;
    let phrase = recovery::seed_to_phrase(&entropy).unwrap().to_string();
    assert_eq!(phrase.split(' ').next(), Some("device"));
    let a = OmcpNode::from_phrase(phrase.clone(), "Alice".into()).unwrap();
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
    // Individual BIP39 words are not valid confidentiality sentinels: the public
    // CBOR field "device_hint" contains "device" even when all secrets are sealed.
    // This fixture deterministically demonstrates why the old word scan failed.
    assert!(blob
        .windows(b"device_hint".len())
        .any(|x| x == b"device_hint"));
    assert!(blob.windows(b"device".len()).any(|x| x == b"device"));
    assert_eq!(a.recovery_phrase().unwrap(), phrase);
    // Check complete secret representations, including the raw entropy that the
    // format actually encrypts (the mnemonic text is reconstructed on restore).
    for secret in [phrase.as_bytes(), entropy.as_slice(), key.as_slice()] {
        assert!(!blob.windows(secret.len()).any(|x| x == secret));
    }
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
        let out = accept(from, from.flush().unwrap());
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
    assert_eq!(storage_schema_version(), 3);
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
fn identity_backup_no_longer_carries_history() {
    // v0.1.4: history moved to the chat backup (see `chatbackup` tests); the
    // Argon2id identity backup is identity + contacts + name only.
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    pump(
        &la,
        &lb,
        la.send_text("not in the identity backup".into()).unwrap(),
    );
    let key = generate_backup_key().unwrap();
    let blob = a
        .create_backup_with_key(key.clone(), "d".into(), 1)
        .unwrap();
    let r = open_backup_with_key(blob, key).unwrap();
    assert!(r.history.is_none());
    // Old blobs that still carry `history` keep restoring through `restore_history`.
    let legacy = a.inner.lock().db().export_history().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a2 = open_node(&r.phrase, "Alice", dir.path());
    for k in &r.contacts {
        a2.restore_contact(k.contact_id.clone(), k.display_name.clone(), k.verified)
            .unwrap();
    }
    a2.restore_history(legacy.to_vec()).unwrap();
    let bob = a.contacts()[0].contact_id.clone();
    assert_eq!(
        a2.stored_messages(bob).unwrap()[0].text,
        "not in the identity backup"
    );
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

// ---- chat backup, media re-fetch, restore indicator (CB1–CB5, RS1–RS2) ----

/// Pump in both directions until quiet, collecting every output.
fn pump_out(la: &MeshLink, lb: &MeshLink, out: LinkOutput) -> Vec<LinkOutput> {
    let out = accept(la, out);
    let mut outs = Vec::new();
    let mut queue = vec![(false, out.outgoing.clone())];
    outs.push(out);
    while let Some((back, pdus)) = queue.pop() {
        for p in pdus {
            let dst = if back { la } else { lb };
            let o = accept(dst, dst.on_pdu(p).unwrap());
            if !o.outgoing.is_empty() {
                queue.push((!back, o.outgoing.clone()));
            }
            outs.push(o);
        }
    }
    outs
}

/// Drain pending file chunks from `from` to `to` (and replies back).
fn drain(from: &MeshLink, to: &MeshLink) -> Vec<LinkOutput> {
    let mut outs = Vec::new();
    loop {
        let o = from.flush().unwrap();
        let more = o.more_pending;
        outs.extend(pump_out(from, to, o));
        if !more {
            return outs;
        }
    }
}

fn link(a: &Arc<OmcpNode>, b: &Arc<OmcpNode>) -> (Arc<MeshLink>, Arc<MeshLink>, Vec<LinkOutput>) {
    let bid = b.identity().peer_id_hex;
    let la = a.open_initiator_link(bid).unwrap();
    let lb = b.open_responder_link().unwrap();
    let outs = pump_out(&la, &lb, la.start().unwrap());
    assert!(la.is_established() && lb.is_established());
    (la, lb, outs)
}

/// Back up all chat blobs of `n` (snapshot + logs so far).
fn backup_all(n: &OmcpNode, now: u64, blobs: &mut Vec<Vec<u8>>) {
    if let Some(u) = n.chat_backup_next(now, false).unwrap() {
        assert_eq!(
            u.blob.len() % omcp_core::chat_backup::BUCKET,
            0,
            "CB2 padded"
        );
        n.chat_backup_commit(u.seq).unwrap();
        blobs.push(u.blob);
    }
}

/// Restore `a` onto a new device from its 24 words and the chat backup.
fn restore_device(a: &OmcpNode, blobs: &[Vec<u8>], dir: &std::path::Path) -> Arc<OmcpNode> {
    // Identity backup (contacts with their X25519 keys) → new install.
    let key = generate_backup_key().unwrap();
    let r = open_backup_with_key(
        a.create_backup_with_key(key.clone(), "t".into(), 1)
            .unwrap(),
        key,
    )
    .unwrap();
    let a2 = open_node(&r.phrase, "Alice", dir);
    for c in r.contacts {
        assert!(c.dh_hex.is_some(), "v0.1.4a backups carry the X25519 key");
        a2.restore_contact_with_key(c.contact_id, c.display_name, c.verified, c.dh_hex)
            .unwrap();
    }
    a2.mark_restored(now_ms()).unwrap();
    a2.chat_backup_restore(blobs.to_vec()).unwrap();
    a2
}

const PIC: &[u8] = b"pretend these are the bytes of a holiday photo";

#[test]
fn cb1_cb4_restore_brings_back_all_text_no_media_bytes_no_burn() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let bob = b.identity().peer_id_hex;
    let mut sent = Vec::new();
    for i in 0..30 {
        let t = format!("text {i} – ünïcödé");
        pump(&la, &lb, la.send_text(t.clone()).unwrap());
        sent.push(t);
    }
    let got = pump(&lb, &la, lb.send_text("reply from Bob".into()).unwrap());
    assert_eq!(got.len(), 1);
    a.queue_message(
        bob.clone(),
        "burn never in backup".into(),
        Some(spec(BurnMode::OnOpen, 0, core_burn::UNREAD_TTL_1H)),
    )
    .unwrap();
    a.queue_file(
        bob.clone(),
        "pic.jpg".into(),
        "image/jpeg".into(),
        PIC.to_vec(),
    )
    .unwrap();
    pump(&la, &lb, la.flush().unwrap());
    drain(&la, &lb);
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    pump(
        &la,
        &lb,
        la.send_text("after the first upload".into()).unwrap(),
    );
    sent.push("after the first upload".into());
    backup_all(&a, 2, &mut blobs);
    assert_eq!(blobs.len(), 2, "snapshot + incremental log");
    for blob in &blobs {
        assert!(!has(blob, PIC), "no media bytes");
        assert!(!has(blob, b"burn never in backup"), "CB4");
        assert!(!has(blob, b"text 1"), "encrypted");
    }
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let msgs = a2.stored_messages(bob.clone()).unwrap();
    let texts: Vec<&str> = msgs.iter().map(|m| m.text.as_str()).collect();
    for t in &sent {
        assert!(texts.contains(&t.as_str()), "missing {t}");
    }
    assert!(texts.contains(&"reply from Bob"));
    assert!(msgs.iter().all(|m| m.burn.is_none()));
    let pic = msgs.iter().find(|m| m.file.is_some()).unwrap();
    let f = pic.file.as_ref().unwrap();
    assert_eq!(f.state, StoredFileState::Missing);
    assert_eq!((f.mime.as_str(), f.size), ("image/jpeg", PIC.len() as u64));
    assert!(f.local_path.is_none());
    // Text count is exact: everything that wasn't burn content.
    let before = a.stored_messages(bob).unwrap();
    assert_eq!(
        msgs.len(),
        before.iter().filter(|m| m.burn.is_none()).count()
    );
}

#[test]
fn cb2_wrong_words_cannot_decrypt() {
    let (a, _b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    pump(&la, &lb, la.send_text("private".into()).unwrap());
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let other = node("Mallory");
    assert!(matches!(
        other.chat_backup_restore(blobs.clone()),
        Err(FfiError::WrongBackupPassword)
    ));
    assert!(
        a.chat_backup_next(2, false).unwrap().is_none(),
        "nothing new"
    );
    assert!(a.chat_backup_due(1 + 3_600_000, true, false).unwrap());
    assert!(
        !a.chat_backup_due(1 + 3_600_000, false, false).unwrap(),
        "Wi-Fi only"
    );
}

#[test]
fn cb3_placeholder_refills_from_partner_and_unavailable_path() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let bob = b.identity().peer_id_hex;
    let alice = a.identity().peer_id_hex;
    // Bob sends Alice a photo; Alice saves it.
    lb.flush().unwrap();
    let offer = b
        .queue_file(
            alice.clone(),
            "pic.jpg".into(),
            "image/jpeg".into(),
            PIC.to_vec(),
        )
        .unwrap();
    let outs = drain(&lb, &la);
    let rf: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    assert_eq!(rf.len(), 1);
    a.set_received_file_path(offer.transfer_id_hex.clone(), Some("/a/pic.jpg".into()))
        .unwrap();
    b.set_received_file_path(offer.transfer_id_hex.clone(), Some("/b/pic.jpg".into()))
        .ok();
    // A second file Bob will no longer have.
    let gone = b
        .queue_file(
            alice.clone(),
            "doc.pdf".into(),
            "application/pdf".into(),
            b"pdf".to_vec(),
        )
        .unwrap();
    drain(&lb, &la);
    // Bob's kept copies: the photo answers from the app below; the pdf is gone.
    {
        let st = b.inner.lock();
        st.db()
            .delete_kept_media(&parse_msg_id(&offer.transfer_id_hex).unwrap())
            .unwrap();
        st.db()
            .delete_kept_media(&parse_msg_id(&gone.transfer_id_hex).unwrap())
            .unwrap();
    }
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    la.shutdown();
    lb.shutdown();

    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let st = |id: &str| {
        a2.stored_messages(bob.clone())
            .unwrap()
            .into_iter()
            .find(|m| m.id_hex == id)
            .unwrap()
            .file
            .unwrap()
            .state
    };
    assert_eq!(st(&offer.transfer_id_hex), StoredFileState::Missing);
    // Bob comes into range: requests go out on link-up, Bob's app answers.
    let t0 = Instant::now();
    let (l2, lb2, outs) = link(&a2, &b);
    assert!(outs.iter().any(|o| o
        .media_updates
        .iter()
        .any(|u| u.state == StoredFileState::Requested)));
    let reqs: Vec<_> = outs.iter().flat_map(|o| o.media_requests.clone()).collect();
    assert_eq!(reqs.len(), 2);
    for r in &reqs {
        let bytes = (r.msg_id_hex == offer.transfer_id_hex).then(|| PIC.to_vec());
        b.answer_media_request(r.contact_id.clone(), r.msg_id_hex.clone(), bytes)
            .unwrap();
    }
    let outs = drain(&lb2, &l2);
    let files: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, PIC);
    assert!(t0.elapsed().as_secs() < 10, "CB3 within 10 s");
    a2.set_received_file_path(files[0].transfer_id_hex.clone(), Some("/a2/pic.jpg".into()))
        .unwrap();
    assert_eq!(st(&offer.transfer_id_hex), StoredFileState::Received);
    let unavailable: Vec<_> = outs.iter().flat_map(|o| o.media_updates.clone()).collect();
    assert!(unavailable
        .iter()
        .any(|u| u.msg_id_hex == gone.transfer_id_hex && u.state == StoredFileState::Unavailable));
    assert_eq!(st(&gone.transfer_id_hex), StoredFileState::Unavailable);
    // "Ask to resend" reaches Bob as a prompt.
    // (e) "Ask to resend" is ONE message carrying the optional note.
    a2.ask_to_resend(
        gone.transfer_id_hex.clone(),
        Some("  lost it, pls resend ".into()),
    )
    .unwrap();
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let asks: Vec<_> = outs.iter().flat_map(|o| o.resend_asks.clone()).collect();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].note.as_deref(), Some("lost it, pls resend"));
    assert!(
        outs.iter().all(|o| o.received.is_empty()),
        "no separate text message"
    );
    // Without a note, and a too-long note is refused.
    a2.ask_to_resend(gone.transfer_id_hex.clone(), None)
        .unwrap();
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let asks: Vec<_> = outs.iter().flat_map(|o| o.resend_asks.clone()).collect();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].note, None);
    assert!(a2
        .ask_to_resend(gone.transfer_id_hex.clone(), Some("x".repeat(281)))
        .is_err());
}

#[test]
fn e_resend_ask_wire_carries_note_and_old_form_still_decodes() {
    use omcp_core::burn::{decode_app, encode_app, MediaControl, CONTROL_MEDIA_RESEND_ASK};
    let c = MediaControl {
        note: Some("merci".into()),
        ..MediaControl::new(CONTROL_MEDIA_RESEND_ASK, [1; 16], [2; 32])
    };
    let wire = encode_app(None, &AppContent::MediaControl(c.clone())).unwrap();
    assert_eq!(decode_app(&wire).unwrap().1, AppContent::MediaControl(c));
    let bare = MediaControl::new(CONTROL_MEDIA_RESEND_ASK, [1; 16], [2; 32]);
    assert_eq!(bare.encode().len(), MediaControl::WIRE_LEN);
    assert_eq!(MediaControl::decode(&bare.encode()).unwrap(), bare);
}

#[test]
fn rs3_restored_contact_refetches_file_sent_30_days_ago_burn_never_served() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let bob = b.identity().peer_id_hex;
    // Bob sends Alice a photo, and a view-once photo.
    lb.flush().unwrap();
    let offer = b
        .queue_file(
            alice.clone(),
            "pic.jpg".into(),
            "image/jpeg".into(),
            PIC.to_vec(),
        )
        .unwrap();
    let once = b
        .queue_media(
            alice.clone(),
            "image/jpeg".into(),
            b"view-once".to_vec(),
            Some(spec(BurnMode::OnOpen, 0, 86_400)),
        )
        .unwrap();
    drain(&lb, &la);
    let u = b.kept_media_usage().unwrap();
    assert_eq!(
        (u.count, u.bytes),
        (1, PIC.len() as u64),
        "burn media never kept"
    );
    assert_eq!(
        b.kept_media_settings().unwrap(),
        KeptMediaSettings {
            max_days: 90,
            max_bytes: 2_000_000_000
        }
    );
    // 30 days later the copy is still kept.
    let day = 86_400_000u64;
    assert_eq!(b.evict_kept_media(now_ms() + 30 * day).unwrap(), 0);
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    la.shutdown();
    lb.shutdown();

    // Alice restores; the photo is a placeholder. Bob's app answers nothing itself.
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let (l2, lb2, outs) = link(&a2, &b);
    assert!(
        outs.iter().all(|o| o.media_requests.is_empty()),
        "served from the kept copy, no app round trip"
    );
    let mut files: Vec<_> = outs.iter().flat_map(|o| o.files.clone()).collect();
    files.extend(drain(&lb2, &l2).iter().flat_map(|o| o.files.clone()));
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].transfer_id_hex, offer.transfer_id_hex);
    assert_eq!(files[0].bytes, PIC);
    assert_eq!(
        blake3::hash(&files[0].bytes),
        blake3::hash(PIC),
        "hash verified"
    );

    // A (forged) request for the burn item is never served.
    {
        let mut st = a2.inner.lock();
        let peer = parse_id(&bob).unwrap();
        st.queue_control(
            &peer,
            AppContent::MediaControl(omcp_core::burn::MediaControl::new(
                omcp_core::burn::CONTROL_MEDIA_REQUEST,
                parse_msg_id(&once).unwrap(),
                *blake3::hash(b"view-once").as_bytes(),
            )),
        )
        .unwrap();
    }
    let outs = pump_out(&l2, &lb2, l2.flush().unwrap());
    let outs2 = drain(&lb2, &l2);
    assert!(outs
        .iter()
        .chain(outs2.iter())
        .all(|o| o.files.is_empty() && o.media_requests.is_empty()));

    // Limits are configurable; shrinking them evicts oldest first.
    assert_eq!(
        b.set_kept_media_settings(
            KeptMediaSettings {
                max_days: 90,
                max_bytes: 10
            },
            now_ms()
        )
        .unwrap(),
        1
    );
    assert_eq!(b.kept_media_usage().unwrap().count, 0);
}

#[test]
fn cb3_refetch_with_wrong_bytes_is_rejected() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let offer = b
        .queue_file(alice, "pic.jpg".into(), "image/jpeg".into(), PIC.to_vec())
        .unwrap();
    drain(&lb, &la);
    // No kept copy: the misbehaving partner below answers by hand.
    b.inner
        .lock()
        .db()
        .delete_kept_media(&parse_msg_id(&offer.transfer_id_hex).unwrap())
        .unwrap();
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let (l2, lb2, _) = link(&a2, &b);
    // A misbehaving partner sends other bytes under the same locator.
    let mut st = b.inner.lock();
    let peer = st.contacts.keys().next().copied().unwrap();
    let f = files::build_outgoing(
        parse_msg_id(&offer.transfer_id_hex).unwrap(),
        "pic.jpg",
        "image/jpeg",
        b"evil",
    )
    .unwrap();
    st.files.out.entry(peer).or_default().push_back(f);
    drop(st);
    let outs = drain(&lb2, &l2);
    assert!(outs.iter().all(|o| o.files.is_empty()));
    assert!(outs
        .iter()
        .any(|o| o.file_failures.iter().any(|e| e.reason == "integrity")));
}

#[test]
fn cb5_media_backup_on_restores_hash_verified_off_uploads_nothing() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let offer = b
        .queue_file(alice, "pic.jpg".into(), "image/jpeg".into(), PIC.to_vec())
        .unwrap();
    drain(&lb, &la);
    a.set_received_file_path(offer.transfer_id_hex.clone(), Some("/a/pic.jpg".into()))
        .unwrap();
    // Off by default: nothing to upload, sealing refused.
    let s = a.media_backup_settings().unwrap();
    assert!(!s.enabled && s.photos && s.voice && s.files && !s.videos && !s.mobile_data);
    assert!(a.media_backup_todo().unwrap().is_empty());
    assert!(!a.media_backup_due(100_000_000, true).unwrap());
    let item_hash = hex::encode(blake3::hash(PIC).as_bytes());
    assert!(a
        .seal_media_backup(item_hash.clone(), PIC.to_vec())
        .is_err());
    // On.
    a.set_media_backup_settings(MediaBackupSettings { enabled: true, ..s })
        .unwrap();
    assert!(a.media_backup_due(100_000_000, true).unwrap());
    let todo = a.media_backup_todo().unwrap();
    assert_eq!(todo.len(), 1);
    assert_eq!(todo[0].hash_hex, item_hash);
    let mb = a
        .seal_media_backup(todo[0].hash_hex.clone(), PIC.to_vec())
        .unwrap();
    assert!(!has(&mb.blob, PIC));
    a.media_backup_commit(todo[0].hash_hex.clone(), 5).unwrap();
    assert!(a.media_backup_todo().unwrap().is_empty());
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &blobs, dir.path());
    let ph = a2.media_placeholders().unwrap();
    assert_eq!(ph.len(), 1);
    assert_eq!(ph[0].blob_name, mb.name);
    let bytes = a2
        .restore_media_from_backup(ph[0].msg_id_hex.clone(), mb.blob.clone())
        .unwrap();
    assert_eq!(bytes, PIC, "hash verified");
    assert!(a2
        .restore_media_from_backup(ph[0].msg_id_hex.clone(), blobs[0].clone())
        .is_err());
    // Turning it off hands back the blobs to delete.
    let del = a
        .set_media_backup_settings(MediaBackupSettings {
            enabled: false,
            ..s
        })
        .unwrap();
    assert_eq!(del, vec![mb.name]);
}

#[test]
fn rs1_contact_sees_unsuppressible_notice_and_old_key_superseded() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let old_dev = a.device_info().unwrap().device_key_hex;
    assert!(b.contacts()[0].restored_at_ms.is_none());
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    let info = a2.device_info().unwrap();
    assert_ne!(info.device_key_hex, old_dev);
    assert!(
        info.restored_at_ms.is_some(),
        "banner on the restored phone"
    );
    let t0 = Instant::now();
    let (l2, lb2, outs) = link(&a2, &b);
    let notices: Vec<_> = outs
        .iter()
        .flat_map(|o| o.restore_notices.clone())
        .collect();
    assert_eq!(notices.len(), 1);
    assert!(t0.elapsed().as_secs() < 5, "RS1 within 5 s");
    assert_eq!(notices[0].contact_id, alice);
    assert_eq!(notices[0].restored_at_ms, info.restored_at_ms);
    let c = b
        .contacts()
        .into_iter()
        .find(|c| c.contact_id == alice)
        .unwrap();
    assert!(c.restored_badge && c.restored_unverified && !c.verified);
    let sys: Vec<_> = b
        .stored_messages(alice.clone())
        .unwrap()
        .into_iter()
        .filter(|m| m.status == MessageStatus::System)
        .collect();
    assert_eq!(sys.len(), 1);
    assert!(sys[0].text.starts_with("restored:"));
    // Old device key superseded: the old install can no longer connect.
    {
        let st = b.inner.lock();
        let d = &st.contacts.values().next().unwrap().device;
        assert!(d.is_superseded(&hex::decode(&old_dev).unwrap().try_into().unwrap()));
    }
    let lo = a.open_initiator_link(b.identity().peer_id_hex).unwrap();
    let lbo = b.open_responder_link().unwrap();
    let mut failed = false;
    for p in lo.start().unwrap().outgoing {
        failed |= lbo.on_pdu(p).is_err();
    }
    assert!(failed && !lbo.is_established());
    // Reconnecting with the same new device doesn't repeat the notice.
    l2.shutdown();
    lb2.shutdown();
    let (_l3, _lb3, outs) = link(&a2, &b);
    assert!(outs.iter().all(|o| o.restore_notices.is_empty()));
}

#[test]
fn rs2_safety_number_changes_verified_cleared_old_qr_fails() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let alice = a.identity().peer_id_hex;
    let old_qr = a.qr_payload();
    b.verify_contact_from_qr(old_qr.clone()).unwrap();
    a.set_verified(b.identity().peer_id_hex, true).unwrap();
    let before = b.contacts()[0].safety_number.clone();
    assert_eq!(before, a.contacts()[0].safety_number, "both sides agree");
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    // O3: our own verification of Bob carries over ("Still verified on your side").
    assert!(a2.contacts().iter().all(|c| c.verified));
    link(&a2, &b);
    let c = b
        .contacts()
        .into_iter()
        .find(|c| c.contact_id == alice)
        .unwrap();
    assert!(!c.verified, "contact side cleared");
    assert_ne!(c.safety_number, before);
    assert_eq!(c.safety_number, a2.contacts()[0].safety_number);
    assert!(matches!(
        b.verify_contact_from_qr(old_qr),
        Err(FfiError::VerificationFailed)
    ));
    let v = b.verify_contact_from_qr(a2.qr_payload()).unwrap();
    assert!(v.verified && !v.restored_unverified);
}

// ---- v0.1.4a: device binding (c) and PeerID / migration (d) ----

/// Send a hand-built message 1 (initiator static `id`, payload from `payload(h)`)
/// to a fresh responder link of `b`. Returns whether `b` refused it.
fn raw_msg1_refused(
    id: StaticIdentity,
    b: &OmcpNode,
    b_dh: [u8; 32],
    payload: impl FnOnce(&[u8]) -> omcp_core::error::Result<Vec<u8>>,
) -> bool {
    let mut init = HybridInitiator::new(id, &b_dh).unwrap();
    let m1 = init.write_message_1_with(payload).unwrap();
    let mut pdus = Vec::new();
    chunk_handshake(&m1, &mut pdus).unwrap();
    let lb = b.open_responder_link().unwrap();
    let mut refused = false;
    for p in pdus {
        refused |= lb.on_pdu(p).is_err();
    }
    refused && !lb.is_established()
}

#[test]
fn c_missing_certificate_or_binding_fails_the_handshake() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let b_dh = b.inner.dh_public();
    // No certificate at all (a pre-v0.1.4a or modified client).
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |_| { Ok(b"omcp-ffi-v1".to_vec()) }
    ));
    // Certificate without the device-key signature over the handshake hash.
    let cert = a.inner.device().cert().unwrap().encode();
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |_| {
            let mut p = b"omcp-ffi-v1\x01".to_vec();
            p.extend_from_slice(&cert);
            Ok(p)
        }
    ));
    // The genuine payload still works.
    assert!(!raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        |h| { a.inner.handshake_payload(h) }
    ));
}

#[test]
fn c_replayed_old_certificate_is_rejected_and_notice_still_shows() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    // An attacker captured a payload of the old device from an earlier session.
    let old_payload = a
        .inner
        .handshake_payload(b"an earlier session hash")
        .unwrap();
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let a2 = restore_device(&a, &[], dir.path());
    let b_dh = b.inner.dh_public();
    // A modified restored client replays it (with its own Noise key): rejected,
    // the binding signature is over another handshake hash and the cert's
    // X25519 key isn't this session's.
    let replay = old_payload.clone();
    assert!(raw_msg1_refused(
        a2.inner.identity().unwrap(),
        &b,
        b_dh,
        move |_| Ok(replay)
    ));
    // Even with the old Noise key it can't sign the fresh hash with the old device key.
    let replay = old_payload;
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b_dh,
        move |_| Ok(replay)
    ));
    // Nothing was applied: the genuine restored device still raises the notice.
    let (_l, _lb, outs) = link(&a2, &b);
    assert_eq!(
        outs.iter().map(|o| o.restore_notices.len()).sum::<usize>(),
        1
    );
    // And the old device key, now superseded, can't connect even signing a fresh hash.
    assert!(raw_msg1_refused(
        a.inner.identity().unwrap(),
        &b,
        b.inner.dh_public(),
        |h| { a.inner.handshake_payload(h) }
    ));
}

#[test]
fn d_peer_id_is_blake2b_of_identity_key_and_contacts_key_by_it() {
    use blake2::digest::consts::U32;
    use blake2::{Blake2b, Digest};
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let ik = omcp_core::device::identity_signing_key(&a.inner.seed)
        .unwrap()
        .verifying_key()
        .to_bytes();
    let mut h = Blake2b::<U32>::new();
    h.update(ik);
    let want = hex::encode::<[u8; 32]>(h.finalize().into());
    assert_eq!(a.identity().peer_id_hex, want);
    assert_eq!(b.contacts()[0].contact_id, want);
    // X25519 is per device and not the id.
    assert_ne!(hex::encode(a.inner.dh_public()), want);
    let qr = a.qr_payload();
    assert!(qr.starts_with(&format!("omcp:3:{want}:{}:", hex::encode(ik))));
    // A QR whose identity key doesn't hash to the PeerID is refused.
    let bad = qr.replacen(&want, &"00".repeat(32), 1);
    assert!(b.preview_contact_from_qr(bad).is_err());
    // The advertisement carries the PeerID prefix and maps back to the contact.
    let adv = a.advertisement();
    let prefix = parse_advertisement(adv).unwrap();
    assert_eq!(b.contact_for_prefix(prefix), Some(want));
}

/// A store as written before v0.1.4a: owner and contact ids are X25519 keys.
fn legacy_store(dir: &std::path::Path, own_phrase: &str, peer_phrase: &str, peer_name: &str) {
    let own = recovery::phrase_to_seed(own_phrase).unwrap();
    let peer = recovery::phrase_to_seed(peer_phrase).unwrap();
    let db = Store::open(&dir.join("omcp.db"), &SKEY).unwrap();
    db.bind_owner(&omcp_core::device::legacy_dh_public(&own).unwrap())
        .unwrap();
    let pid = omcp_core::device::legacy_dh_public(&peer).unwrap();
    db.put_contact(&st::ContactRec {
        peer: pid,
        name: peer_name.into(),
        verified: true,
        device: None,
        dh: None,
    })
    .unwrap();
    db.put_message(&st::MessageRec {
        id: [7; 16],
        peer: pid,
        mine: true,
        time_ms: 10,
        status: st::STATUS_SENT,
        text: format!("hi {peer_name}, from before the upgrade"),
        mime: None,
        burn: None,
        burn_state: st::BURN_NONE,
        burn_at_ms: None,
        file: None,
    })
    .unwrap();
}

#[test]
fn d_legacy_store_migrates_to_peer_ids_on_first_handshake() {
    let pa = generate_recovery_phrase().unwrap();
    let pb = generate_recovery_phrase().unwrap();
    let (da, db) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    legacy_store(da.path(), &pa, &pb, "Bob");
    legacy_store(db.path(), &pb, &pa, "Alice");
    let a = open_node(&pa, "Alice", da.path());
    let b = open_node(&pb, "Bob", db.path());
    // Migrated installs keep their X25519 key, so legacy contacts can still dial.
    let legacy_b = hex::encode(
        omcp_core::device::legacy_dh_public(&recovery::phrase_to_seed(&pb).unwrap()).unwrap(),
    );
    assert_eq!(hex::encode(b.inner.dh_public()), legacy_b);
    assert_eq!(a.contacts()[0].contact_id, legacy_b);
    let sn_before = a.contacts()[0].safety_number.clone();
    assert_eq!(sn_before, b.contacts()[0].safety_number);
    // Legacy contacts still find a migrated install by its advertisement.
    assert_eq!(
        a.contact_for_prefix(parse_advertisement(b.advertisement()).unwrap()),
        Some(legacy_b.clone())
    );
    let la = a.open_initiator_link(legacy_b).unwrap();
    let lb = b.open_responder_link().unwrap();
    pump_out(&la, &lb, la.start().unwrap());
    assert!(la.is_established() && lb.is_established());
    let (pid_a, pid_b) = (a.identity().peer_id_hex, b.identity().peer_id_hex);
    assert_eq!(a.contacts()[0].contact_id, pid_b);
    assert_eq!(b.contacts()[0].contact_id, pid_a);
    assert!(
        a.contacts()[0].verified && b.contacts()[0].verified,
        "verification kept"
    );
    assert!(
        b.contacts()[0].restored_at_ms.is_none(),
        "an upgrade is not a restore"
    );
    assert_eq!(a.contacts()[0].safety_number, b.contacts()[0].safety_number);
    let ms = a.stored_messages(pid_b.clone()).unwrap();
    assert!(ms[0].text.contains("before the upgrade"));
    // Persisted: survives a restart under the new ids.
    drop((la, lb, a));
    let a = open_node(&pa, "Alice", da.path());
    assert_eq!(a.contacts()[0].contact_id, pid_b);
    assert_eq!(a.stored_messages(pid_b).unwrap().len(), 1);
}

#[test]
fn d_old_identity_backup_restores_legacy_contacts() {
    let r = restored(backup::BackupContents {
        entropy: Zeroizing::new([5; 32]),
        contacts: vec![backup::BackupContact {
            peer_id: [9; 32],
            name: "Old".into(),
            verified: true,
            dh: None,
        }],
        display_name: None,
        history: None,
    })
    .unwrap();
    assert_eq!(r.contacts[0].dh_hex, None);
    let n = node("Me");
    let c = &r.contacts[0];
    n.restore_contact_with_key(
        c.contact_id.clone(),
        c.display_name.clone(),
        c.verified,
        None,
    )
    .unwrap();
    // A legacy contact: dialled by its id (the X25519 key) until re-keyed.
    assert_eq!(n.inner.lock().contacts[&[9; 32]].dh, [9; 32]);
}

// Resource bounds and fail-closed transport cleanup (no external devices).
#[test]
fn handshake_rejects_malformed_order_counts_and_oversized_chunks_and_releases_buffers() {
    let b = node("Bob");
    for bad in [
        vec![PDU_HANDSHAKE, 0, 0],
        vec![PDU_HANDSHAKE, 255, 255],
        vec![PDU_HANDSHAKE, 1, 2],
        vec![PDU_HANDSHAKE, 0],
        {
            let mut p = vec![PDU_HANDSHAKE, 0, 2];
            p.extend(vec![0; HANDSHAKE_CHUNK + 1]);
            p
        },
    ] {
        let link = b.open_responder_link().unwrap();
        assert!(link.on_pdu(bad).is_err());
        let st = link.lock();
        assert!(matches!(st.role, Role::Closed));
        assert_eq!(st.hs_buf.capacity(), 0);
        assert_eq!(st.hs_next, 0);
        assert!(b.inner.lock().active_links.is_empty());
    }
    for bad in [
        vec![PDU_HANDSHAKE, 0, 3, 1],
        vec![PDU_HANDSHAKE, 2, 3, 1],
        vec![PDU_HANDSHAKE, 1, 2, 1],
    ] {
        let link = b.open_responder_link().unwrap();
        link.on_pdu(vec![PDU_HANDSHAKE, 0, 3, 1]).unwrap();
        assert!(link.on_pdu(bad).is_err());
        assert_eq!(link.lock().hs_buf.capacity(), 0);
        assert!(b.inner.lock().active_links.is_empty());
    }
}

#[test]
fn full_handshake_chunk_range_is_bounded_and_shutdown_drops_reservation() {
    let b = node("Bob");
    let link = b.open_responder_link().unwrap();
    for index in 0..254 {
        let mut pdu = vec![PDU_HANDSHAKE, index, 255];
        pdu.extend(vec![0; HANDSHAKE_CHUNK]);
        link.on_pdu(pdu).unwrap();
    }
    assert_eq!(link.lock().hs_buf.len(), 254 * HANDSHAKE_CHUNK);
    assert_eq!(link.lock().hs_buf.capacity(), MAX_HANDSHAKE_BYTES);
    let mut last = vec![PDU_HANDSHAKE, 254, 255];
    last.extend(vec![0; HANDSHAKE_CHUNK]);
    // The entire legal framing range reaches Noise, which rejects these fake
    // cryptographic bytes. The transport must not overflow/reject index 254.
    assert!(matches!(link.on_pdu(last), Err(FfiError::Protocol { .. })));
    assert_eq!(link.lock().hs_buf.capacity(), 0);
    let next = b.open_responder_link().unwrap();
    next.on_pdu(vec![PDU_HANDSHAKE, 0, 255, 0]).unwrap();
    next.shutdown();
    assert_eq!(next.lock().hs_buf.capacity(), 0);
    assert!(b.inner.lock().active_links.is_empty());
}

#[test]
fn active_links_are_bounded_and_drop_shutdown_and_input_errors_release_slots() {
    let b = node("Bob");
    let mut links = Vec::new();
    for _ in 0..MAX_ACTIVE_LINKS {
        links.push(b.open_responder_link().unwrap());
    }
    assert!(matches!(
        b.open_responder_link(),
        Err(FfiError::State { .. })
    ));
    links.pop();
    let replacement = b.open_responder_link().unwrap();
    assert_eq!(b.inner.lock().active_links.len(), MAX_ACTIVE_LINKS);
    replacement.shutdown();
    assert_eq!(b.inner.lock().active_links.len(), MAX_ACTIVE_LINKS - 1);
    assert!(links[0].on_pdu(vec![]).is_err());
    assert_eq!(b.inner.lock().active_links.len(), MAX_ACTIVE_LINKS - 2);
    links.clear();
    assert!(b.inner.lock().active_links.is_empty());
}

#[test]
fn established_link_error_marks_peer_offline_and_new_connection_still_works() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    assert_eq!(b.path_status().nearby_peers, 1);
    let out = la.send_text("tampered".into()).unwrap();
    let mut pdu = out.outgoing[0].clone();
    *pdu.last_mut().unwrap() ^= 1;
    assert!(lb.on_pdu(pdu).is_err());
    assert!(!lb.is_established());
    assert_eq!(b.path_status().nearby_peers, 0);
    assert!(b.inner.lock().active_links.is_empty());
    la.shutdown();
    let la2 = a
        .open_initiator_link(a.contacts()[0].contact_id.clone())
        .unwrap();
    let lb2 = b.open_responder_link().unwrap();
    pump(&la2, &lb2, la2.start().unwrap());
    let got = pump(
        &la2,
        &lb2,
        la2.send_text("fresh nonce space".into()).unwrap(),
    );
    assert_eq!(got[0].text, "fresh nonce space");
}

#[test]
fn cancel_incoming_file_releases_peer_admission_and_can_be_repeated_safely() {
    use omcp_core::files::encrypt_file;
    let b = node("Bob");
    let (meta, key, _) = encrypt_file(b"small", "x", "binary", &[], 1).unwrap();
    let offer = |id: [u8; 16]| {
        let mut body = id.to_vec();
        body.extend_from_slice(key.as_bytes());
        body.extend_from_slice(&meta.encode().unwrap());
        body
    };
    let mut out = LinkOutput::default();
    {
        let mut st = b.inner.lock();
        files::on_offer(&mut st.files, [7; 32], &offer([1; 16]), &mut out);
        files::on_offer(&mut st.files, [7; 32], &offer([2; 16]), &mut out);
        files::on_offer(&mut st.files, [7; 32], &offer([3; 16]), &mut out);
        assert_eq!(out.file_failures.last().unwrap().reason, "busy");
        assert_eq!(st.files.incoming.len(), 2);
    }
    assert!(b.cancel_file(hex::encode([1; 16])));
    assert!(!b.cancel_file(hex::encode([1; 16])));
    let mut st = b.inner.lock();
    files::on_offer(&mut st.files, [7; 32], &offer([3; 16]), &mut out);
    assert_eq!(st.files.incoming.len(), 2);
    assert!(st.files.incoming.contains_key(&[3; 16]));
}

#[test]
fn file_events_and_offers_cannot_overwrite_durable_transfer_ownership() {
    use omcp_core::files::encrypt_file;
    let (a, _b, _la, _lb) = paired();
    let peer = parse_id(&a.contacts()[0].contact_id).unwrap();
    let offer = a
        .queue_file(
            hex::encode(peer),
            "original".into(),
            "binary".into(),
            b"original".to_vec(),
        )
        .unwrap();
    let id = parse_msg_id(&offer.transfer_id_hex).unwrap();
    let original = a.inner.lock().db().message(&id).unwrap().unwrap();
    let (meta, key, _) = encrypt_file(b"replacement", "foreign", "binary", &[], 1).unwrap();
    let mut body = id.to_vec();
    body.extend_from_slice(key.as_bytes());
    body.extend_from_slice(&meta.encode().unwrap());
    let inner = core_burn::encode_app(None, &AppContent::FileMeta(body)).unwrap();
    let mut send_counter = 0;
    let env =
        seal_inner_envelope(&[0; 32], &peer, [99; 16], &inner, 0, &mut send_counter, 8).unwrap();
    for claimant in [peer, [99; 32]] {
        let mut out = LinkOutput::default();
        let mut st = a.inner.lock();
        MeshLink::deliver(
            &mut st,
            claimant,
            &env,
            omcp_core::ble::message::CONTENT_TYPE_INNER_V1,
            &inner,
            &mut out,
        )
        .unwrap();
        assert!(st.files.incoming.is_empty());
        assert_eq!(out.file_failures.len(), 1);
        // Defense in depth: even synthesized progress/completion events cannot
        // cross the durable peer/direction ownership boundary.
        out.file_progress.push(FileProgress {
            transfer_id_hex: hex::encode(id),
            contact_id: hex::encode(claimant),
            incoming: true,
            name: "foreign".into(),
            size: 11,
            done_chunks: 1,
            total_chunks: 1,
        });
        out.files.push(ReceivedFile {
            transfer_id_hex: hex::encode(id),
            contact_id: hex::encode(claimant),
            name: "foreign".into(),
            mime: "binary".into(),
            size: 11,
            bytes: b"replacement".to_vec(),
        });
        st.persist_file_events(&out, (0, 0, 0), 1).unwrap();
        assert_eq!(st.db().message(&id).unwrap().unwrap(), original);
    }
    // A foreign peer also cannot alter an existing incoming transfer record.
    let mut incoming = original.clone();
    incoming.mine = false;
    a.inner.lock().db().put_message(&incoming).unwrap();
    let mut out = LinkOutput::default();
    out.file_failures.push(FileFailure {
        transfer_id_hex: hex::encode(id),
        contact_id: hex::encode([99; 32]),
        reason: "invalid".into(),
    });
    a.inner
        .lock()
        .persist_file_events(&out, (0, 0, 0), 2)
        .unwrap();
    assert_eq!(a.inner.lock().db().message(&id).unwrap().unwrap(), incoming);
}

#[test]
fn completed_file_id_is_immutable_under_same_peer_replay_conflicts_and_failure_events() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let peer = parse_id(&b.identity().peer_id_hex).unwrap();
    let offer = a
        .queue_file(
            hex::encode(peer),
            "original".into(),
            "binary".into(),
            PIC.to_vec(),
        )
        .unwrap();
    let id = parse_msg_id(&offer.transfer_id_hex).unwrap();
    let (encoded_offer, encoded_chunk) = {
        let st = a.inner.lock();
        let f = st.files.out[&peer].front().unwrap();
        (f.offer.clone(), f.chunks[0].clone())
    };
    assert_eq!(drain(&la, &lb).iter().flat_map(|o| &o.files).count(), 1);
    b.set_received_file_path(
        offer.transfer_id_hex.clone(),
        Some("/saved/original".into()),
    )
    .unwrap();
    let original = b.inner.lock().db().message(&id).unwrap().unwrap();
    let AppContent::FileMeta(mut conflict) = core_burn::decode_app(&encoded_offer).unwrap().1
    else {
        panic!("offer")
    };
    conflict[16] ^= 1; // Same transfer ID, conflicting content key.
    let repeated = [
        core_burn::decode_app(&encoded_offer).unwrap().1,
        core_burn::decode_app(&encoded_chunk).unwrap().1,
        AppContent::FileMeta(conflict),
        AppContent::FileChunk(id.to_vec()), // Malformed chunk after completed offer.
    ];
    for content in repeated {
        a.inner.lock().queue_control(&peer, content).unwrap();
        let outputs = drain(&la, &lb);
        assert!(outputs.iter().all(|o| o.files.is_empty()
            && o.file_progress.is_empty()
            && o.file_failures.is_empty()));
        assert!(b.inner.lock().files.incoming.is_empty());
        assert_eq!(b.inner.lock().db().message(&id).unwrap().unwrap(), original);
    }
    // Defense in depth at persistence: late/synthetic completion or error events
    // from the correct peer must not reset a completed file's saved path/hash.
    let mut out = LinkOutput::default();
    out.files.push(ReceivedFile {
        transfer_id_hex: hex::encode(id),
        contact_id: a.identity().peer_id_hex.clone(),
        name: "replacement".into(),
        mime: "binary".into(),
        size: 3,
        bytes: b"bad".to_vec(),
    });
    out.file_failures.push(FileFailure {
        transfer_id_hex: hex::encode(id),
        contact_id: a.identity().peer_id_hex,
        reason: "integrity".into(),
    });
    b.inner
        .lock()
        .persist_file_events(&out, (0, 0, 0), 99)
        .unwrap();
    assert_eq!(b.inner.lock().db().message(&id).unwrap().unwrap(), original);
}

#[test]
fn restored_own_sent_file_placeholder_remains_hash_verified_and_refetchable() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let offer = a
        .queue_file(
            b.identity().peer_id_hex,
            "sent.jpg".into(),
            "image/jpeg".into(),
            PIC.to_vec(),
        )
        .unwrap();
    drain(&la, &lb);
    b.set_received_file_path(offer.transfer_id_hex.clone(), Some("/b/sent.jpg".into()))
        .unwrap();
    let mut blobs = Vec::new();
    backup_all(&a, 1, &mut blobs);
    la.shutdown();
    lb.shutdown();
    let dir = tempfile::tempdir().unwrap();
    let restored = restore_device(&a, &blobs, dir.path());
    let id = parse_msg_id(&offer.transfer_id_hex).unwrap();
    let before = restored.inner.lock().db().message(&id).unwrap().unwrap();
    assert!(before.mine);
    assert_eq!(before.file.as_ref().unwrap().state, st::FILE_MISSING);
    let (lr, lb2, outputs) = link(&restored, &b);
    let requests: Vec<_> = outputs.iter().flat_map(|o| &o.media_requests).collect();
    assert_eq!(requests.len(), 1);
    b.answer_media_request(
        requests[0].contact_id.clone(),
        requests[0].msg_id_hex.clone(),
        Some(PIC.to_vec()),
    )
    .unwrap();
    let outputs = drain(&lb2, &lr);
    assert!(outputs.iter().all(|o| o.file_failures.is_empty()));
    let files: Vec<_> = outputs.iter().flat_map(|o| &o.files).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, PIC);
    let after = restored.inner.lock().db().message(&id).unwrap().unwrap();
    assert!(
        after.mine,
        "refetch does not change original message direction"
    );
    assert_eq!(after.file.as_ref().unwrap().state, st::FILE_RECEIVED);
    assert_eq!(after.file.as_ref().unwrap().hash, before.file.unwrap().hash);
}

// ---- durable local transport acceptance (not peer delivery) ----

#[test]
fn pending_batches_keep_work_and_nonces_until_acceptance() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let peer = b.identity().peer_id_hex;
    let out = la.send_text("durable before radio".into()).unwrap();
    let token = out.batch_token.clone().unwrap();
    assert_eq!(out.flushed, 0);
    assert_eq!(a.queued_for(peer.clone()), 1);
    assert_eq!(
        a.stored_messages(peer.clone()).unwrap()[0].status,
        MessageStatus::Queued
    );
    let nonce = match &la.lock().role {
        Role::Up(up) => up.app_send,
        _ => panic!(),
    };
    for _ in 0..3 {
        let pending = la.flush().unwrap();
        assert!(pending.outgoing.is_empty() && pending.batch_token.is_none());
    }
    assert_eq!(
        match &la.lock().role {
            Role::Up(up) => up.app_send,
            _ => panic!(),
        },
        nonce
    );
    // Local acceptance does not require, or pretend to prove, peer receipt.
    let committed = la.commit_outgoing(token.clone()).unwrap();
    assert_eq!(committed.flushed, 1);
    assert_eq!(a.queued_for(peer.clone()), 0);
    assert!(b
        .stored_messages(a.identity().peer_id_hex)
        .unwrap()
        .is_empty());
    assert_eq!(la.commit_outgoing(token.clone()).unwrap().flushed, 0);
    let next = la.send_text("next batch".into()).unwrap();
    assert_eq!(la.commit_outgoing(token.clone()).unwrap().flushed, 0);
    assert_eq!(a.queued_for(peer.clone()), 1);
    assert!(la.abort_outgoing(token).is_err());
    la.commit_outgoing(next.batch_token.unwrap()).unwrap();
    assert_eq!(a.queued_for(peer), 0);
}

#[test]
fn first_middle_final_transport_failure_replays_logical_text_on_new_session() {
    for fraction in [0, 1, 2] {
        let (a, b, la, lb) = paired();
        pump(&la, &lb, la.start().unwrap());
        let text = "x".repeat(1600);
        let out = la.send_text(text.clone()).unwrap();
        let n = match fraction {
            0 => 0,
            1 => out.outgoing.len() / 2,
            _ => out.outgoing.len() - 1,
        };
        for p in out.outgoing.iter().take(n) {
            assert!(lb.on_pdu(p.clone()).unwrap().received.is_empty());
        }
        la.abort_outgoing(out.batch_token.clone().unwrap()).unwrap();
        assert!(!la.is_established());
        assert_eq!(a.queued_for(b.identity().peer_id_hex), 1);
        assert!(la.commit_outgoing(out.batch_token.unwrap()).is_err());
        lb.shutdown();
        let la2 = a.open_initiator_link(b.identity().peer_id_hex).unwrap();
        let lb2 = b.open_responder_link().unwrap();
        let got = pump(&la2, &lb2, la2.start().unwrap());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, text);
        assert_eq!(a.queued_for(b.identity().peer_id_hex), 0);
    }
}

#[test]
fn handshake_and_incoming_data_before_os_callback_do_not_drop_replies() {
    let (a, b, la, lb) = paired();
    a.queue_text(b.identity().peer_id_hex, "queued Alice".into())
        .unwrap();
    b.queue_text(a.identity().peer_id_hex, "queued Bob".into())
        .unwrap();
    let first = la.start().unwrap();
    let mut reply = LinkOutput::default();
    for p in first.outgoing {
        let o = lb.on_pdu(p).unwrap();
        if !o.outgoing.is_empty() {
            reply = o;
        }
    }
    assert!(reply.batch_token.is_some());
    let mut got = Vec::new();
    for p in reply.outgoing {
        let o = la.on_pdu(p).unwrap();
        assert!(o.outgoing.is_empty());
        got.extend(o.received);
    }
    assert_eq!(got[0].text, "queued Bob");
    assert!(
        la.commit_outgoing(first.batch_token.unwrap())
            .unwrap()
            .more_pending
    );
    lb.commit_outgoing(reply.batch_token.unwrap()).unwrap();
    let got = pump(&la, &lb, la.flush().unwrap());
    assert_eq!(got[0].text, "queued Alice");
    assert_eq!(
        a.path_status().queued_count + b.path_status().queued_count,
        0
    );
}

#[test]
fn pending_incoming_text_does_not_hide_a_queued_reply() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let first = la.send_text("first".into()).unwrap();
    b.queue_text(a.identity().peer_id_hex, "inbound while waiting".into())
        .unwrap();
    let incoming = accept(&lb, lb.flush().unwrap());
    for p in incoming.outgoing {
        assert!(la.on_pdu(p).unwrap().outgoing.is_empty());
    }
    a.queue_text(b.identity().peer_id_hex, "later durable reply".into())
        .unwrap();
    assert!(
        la.commit_outgoing(first.batch_token.unwrap())
            .unwrap()
            .more_pending
    );
    // The first batch must be accepted by the peer in sequence before batch 2.
    for p in first.outgoing {
        lb.on_pdu(p).unwrap();
    }
    let got = pump(&la, &lb, la.flush().unwrap());
    assert_eq!(got[0].text, "later durable reply");
}

#[test]
fn one_peer_backlog_owner_and_wrong_link_tokens_cannot_steal_work() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let la2 = a.open_initiator_link(b.identity().peer_id_hex).unwrap();
    let lb2 = b.open_responder_link().unwrap();
    pump(&la2, &lb2, la2.start().unwrap());
    a.queue_text(b.identity().peer_id_hex, "one owner".into())
        .unwrap();
    let first = la.flush().unwrap();
    assert!(la2.flush().unwrap().outgoing.is_empty());
    assert!(la2
        .commit_outgoing(first.batch_token.clone().unwrap())
        .is_err());
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 1);
    la.abort_outgoing(first.batch_token.unwrap()).unwrap();
    let got = pump(&la2, &lb2, la2.flush().unwrap());
    assert_eq!(got.len(), 1);
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 0);
}

#[test]
fn cancelled_file_and_burned_message_are_not_resurrected_by_late_commit() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let peer = b.identity().peer_id_hex;
    let id = a
        .queue_message(
            peer.clone(),
            "ephemeral".into(),
            Some(spec(BurnMode::OnOpen, 0, 604_800)),
        )
        .unwrap();
    let offer = a
        .queue_file(
            peer.clone(),
            "cancel.txt".into(),
            "text/plain".into(),
            vec![42; 4],
        )
        .unwrap();
    let out = la.flush().unwrap();
    assert!(out.file_progress.is_empty());
    assert!(a.cancel_file(offer.transfer_id_hex));
    assert!(a.tick_burn(u64::MAX).contains(&id));
    assert_eq!(
        a.queued_for(peer.clone()),
        0,
        "shredded plaintext leaves RAM before ACK"
    );
    let committed = la.commit_outgoing(out.batch_token.unwrap()).unwrap();
    assert_eq!(committed.flushed, 0);
    assert!(committed.file_progress.is_empty());
    assert!(a.stored_messages(peer.clone()).unwrap().is_empty());
    assert_eq!(a.queued_for(peer.clone()), 0);
    assert_eq!(a.files_pending_for(peer), 0);
}

fn persisted_pair(dir: &std::path::Path) -> (String, Vec<u8>, Arc<OmcpNode>, Arc<OmcpNode>) {
    let phrase = generate_recovery_phrase().unwrap();
    let key = generate_storage_key().unwrap();
    let a = OmcpNode::open(
        phrase.clone(),
        "Alice".into(),
        dir.join("a.db").to_string_lossy().into(),
        key.clone(),
    )
    .unwrap();
    let b = node("Bob");
    a.add_contact_from_qr(b.qr_payload()).unwrap();
    b.add_contact_from_qr(a.qr_payload()).unwrap();
    (phrase, key, a, b)
}

#[test]
fn database_failure_rolls_back_acceptance_and_closes_session_without_loss() {
    let dir = tempfile::tempdir().unwrap();
    let (_phrase, _key, a, b) = persisted_pair(dir.path());
    let (la, lb, _) = link(&a, &b);
    a.queue_text(b.identity().peer_id_hex, "atomic one".into())
        .unwrap();
    a.queue_text(b.identity().peer_id_hex, "atomic two".into())
        .unwrap();
    let out = la.flush().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("a.db")).unwrap();
    db.execute_batch(
        "CREATE TRIGGER reject_ack BEFORE DELETE ON outbox BEGIN SELECT RAISE(ABORT,'test'); END;",
    )
    .unwrap();
    assert!(matches!(
        la.commit_outgoing(out.batch_token.clone().unwrap()),
        Err(FfiError::Storage { .. })
    ));
    assert!(!la.is_established());
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 2);
    assert!(a
        .stored_messages(b.identity().peer_id_hex)
        .unwrap()
        .iter()
        .all(|m| m.status == MessageStatus::Queued));
    assert!(la.commit_outgoing(out.batch_token.unwrap()).is_err());
    db.execute_batch("DROP TRIGGER reject_ack").unwrap();
    lb.shutdown();
    let (la2, lb2, outs) = link(&a, &b);
    assert_eq!(outs.iter().map(|o| o.received.len()).sum::<usize>(), 2);
    assert!(la2.is_established() && lb2.is_established());
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 0);
}

#[test]
fn crash_before_commit_replays_same_id_after_restart_but_after_commit_does_not() {
    for commit in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (phrase, key, a, b) = persisted_pair(dir.path());
        let (la, lb, _) = link(&a, &b);
        let out = la.send_text("crash boundary".into()).unwrap();
        let id = a.stored_messages(b.identity().peer_id_hex).unwrap()[0]
            .id_hex
            .clone();
        if commit {
            la.commit_outgoing(out.batch_token.unwrap()).unwrap();
        }
        drop(la);
        drop(lb);
        drop(a);
        let a = OmcpNode::open(
            phrase,
            "Alice".into(),
            dir.path().join("a.db").to_string_lossy().into(),
            key,
        )
        .unwrap();
        assert_eq!(
            a.queued_for(b.identity().peer_id_hex),
            if commit { 0 } else { 1 }
        );
        let (_, _, outs) = link(&a, &b);
        let got: Vec<_> = outs.into_iter().flat_map(|o| o.received).collect();
        assert_eq!(got.len(), if commit { 0 } else { 1 });
        if !commit {
            assert_eq!(got[0].envelope_id_hex, id);
        }
    }
}

#[test]
fn final_file_batch_retains_durable_work_until_commit_and_replay_is_not_redelivered() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let peer = b.identity().peer_id_hex;
    let offer = a
        .queue_file(
            peer.clone(),
            "final.txt".into(),
            "text/plain".into(),
            vec![7; 33],
        )
        .unwrap();
    let out = la.flush().unwrap();
    assert_eq!(a.files_pending_for(peer.clone()), 1);
    assert!(out.file_progress.is_empty());
    let mut received = Vec::new();
    for p in out.outgoing {
        received.extend(lb.on_pdu(p).unwrap().files);
    }
    assert_eq!(received.len(), 1);
    b.set_received_file_path(
        offer.transfer_id_hex.clone(),
        Some("private/final.txt".into()),
    )
    .unwrap();
    la.abort_outgoing(out.batch_token.unwrap()).unwrap();
    lb.shutdown();
    let (la2, lb2, outs) = link(&a, &b);
    assert!(outs.iter().all(|o| o.files.is_empty()));
    assert_eq!(a.files_pending_for(peer), 0);
    assert_eq!(
        b.stored_messages(a.identity().peer_id_hex).unwrap()[0]
            .file
            .as_ref()
            .unwrap()
            .local_path
            .as_deref(),
        Some("private/final.txt")
    );
    assert!(la2.is_established() && lb2.is_established());
}

#[test]
fn text_batch_bound_drains_backlog_without_unbounded_pending_state() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    for i in 0..(TEXTS_PER_FLUSH + 3) {
        a.queue_text(b.identity().peer_id_hex, format!("{i}"))
            .unwrap();
    }
    let first = la.flush().unwrap();
    assert_eq!(
        la.lock().pending.as_ref().unwrap().queued.len(),
        TEXTS_PER_FLUSH
    );
    assert_eq!(pump(&la, &lb, first).len(), TEXTS_PER_FLUSH);
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 3);
    assert_eq!(pump(&la, &lb, la.flush().unwrap()).len(), 3);
}

#[test]
fn authenticated_user_replay_after_receiver_restart_and_deletion_is_suppressed() {
    for burn in [None, Some(spec(BurnMode::OnOpen, 0, 604_800))] {
        let dir = tempfile::tempdir().unwrap();
        let phrase = generate_recovery_phrase().unwrap();
        let a = node("Alice");
        let b = open_node(&phrase, "Bob", dir.path());
        a.add_contact_from_qr(b.qr_payload()).unwrap();
        b.add_contact_from_qr(a.qr_payload()).unwrap();
        let (la, lb, _) = link(&a, &b);
        let id = a
            .queue_message(
                b.identity().peer_id_hex,
                "only once in retained window".into(),
                burn,
            )
            .unwrap();
        let out = la.flush().unwrap();
        let mut received = Vec::new();
        for p in out.outgoing {
            received.extend(lb.on_pdu(p).unwrap().received);
        }
        assert_eq!(received.len(), 1);
        if burn.is_some() {
            assert!(b.tick_burn(u64::MAX).contains(&id));
        } else {
            b.inner
                .lock()
                .db()
                .delete_message(&parse_msg_id(&id).unwrap())
                .unwrap();
        }
        la.abort_outgoing(out.batch_token.unwrap()).unwrap();
        drop(lb);
        drop(b);
        let b = open_node(&phrase, "Bob", dir.path());
        let (_, _, outs) = link(&a, &b);
        assert!(outs.iter().all(|o| o.received.is_empty()));
        assert!(b
            .stored_messages(a.identity().peer_id_hex)
            .unwrap()
            .is_empty());
        assert!(!b.burn_is_live(id));
    }
}

#[test]
fn received_message_and_tombstone_failure_roll_back_together_and_allow_retry() {
    let dir = tempfile::tempdir().unwrap();
    let (phrase, key, b, a) = persisted_pair(dir.path());
    let (la, lb, _) = link(&a, &b);
    let id = a
        .queue_message(
            b.identity().peer_id_hex,
            "receive transaction".into(),
            Some(spec(BurnMode::OnOpen, 0, 604_800)),
        )
        .unwrap();
    let out = la.flush().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("a.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_receipt BEFORE INSERT ON received BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    let mut failed = false;
    for p in out.outgoing {
        failed |= lb.on_pdu(p).is_err();
    }
    assert!(failed && !lb.is_established());
    assert!(b
        .stored_messages(a.identity().peer_id_hex)
        .unwrap()
        .is_empty());
    assert!(!b.burn_is_live(id.clone()));
    assert!(!b
        .inner
        .lock()
        .db()
        .received_before(
            &parse_id(&a.identity().peer_id_hex).unwrap(),
            &parse_msg_id(&id).unwrap()
        )
        .unwrap());
    la.abort_outgoing(out.batch_token.unwrap()).unwrap();
    db.execute_batch("DROP TRIGGER reject_receipt").unwrap();
    drop(lb);
    drop(b);
    let b = OmcpNode::open(
        phrase,
        "Alice".into(),
        dir.path().join("a.db").to_string_lossy().into(),
        key,
    )
    .unwrap();
    let (_, _, outs) = link(&a, &b);
    assert_eq!(outs.iter().map(|o| o.received.len()).sum::<usize>(), 1);
    assert!(b.burn_is_live(id));
}

#[test]
fn send_text_queue_failure_cannot_leave_an_unqueued_visible_message() {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, a, b) = persisted_pair(dir.path());
    let (la, _lb, _) = link(&a, &b);
    let db = rusqlite::Connection::open(dir.path().join("a.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_queue BEFORE INSERT ON outbox BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    assert!(matches!(
        la.send_text("must persist first".into()),
        Err(FfiError::Storage { .. })
    ));
    assert!(a
        .stored_messages(b.identity().peer_id_hex)
        .unwrap()
        .is_empty());
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 0);
    assert!(la.is_established()); // no nonce was consumed
    db.execute_batch("DROP TRIGGER reject_queue").unwrap();
    assert!(la.send_text("retry".into()).unwrap().batch_token.is_some());
}

#[test]
fn first_middle_final_file_pdu_failure_keeps_final_batch_durable() {
    for fraction in [0, 1, 2] {
        let (a, b, la, lb) = paired();
        pump(&la, &lb, la.start().unwrap());
        let peer = b.identity().peer_id_hex;
        let bytes = vec![9; FILE_CHUNK_BYTES + 7];
        a.queue_file(
            peer.clone(),
            "failed-final.bin".into(),
            "binary".into(),
            bytes.clone(),
        )
        .unwrap();
        let out = la.flush().unwrap();
        let n = match fraction {
            0 => 0,
            1 => out.outgoing.len() / 2,
            _ => out.outgoing.len() - 1,
        };
        for p in out.outgoing.iter().take(n) {
            assert!(lb.on_pdu(p.clone()).unwrap().files.is_empty());
        }
        la.abort_outgoing(out.batch_token.unwrap()).unwrap();
        lb.shutdown();
        assert_eq!(a.files_pending_for(peer.clone()), 1);
        let (_, _, outs) = link(&a, &b);
        let received: Vec<_> = outs.into_iter().flat_map(|o| o.files).collect();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].bytes, bytes);
        assert_eq!(a.files_pending_for(peer), 0);
    }
}

#[test]
fn opened_while_batch_pending_immediately_shreds_queued_plaintext_without_ack() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let id = a
        .queue_message(
            b.identity().peer_id_hex,
            "do not retain after opened".into(),
            Some(spec(BurnMode::OnOpen, 0, 604_800)),
        )
        .unwrap();
    let pending = la.flush().unwrap();
    for p in pending.outgoing {
        lb.on_pdu(p).unwrap();
    }
    b.open_burn(id.clone(), now_ms()).unwrap().unwrap();
    let receipt = accept(&lb, lb.flush().unwrap());
    let mut opened = Vec::new();
    for p in receipt.outgoing {
        let out = la.on_pdu(p).unwrap();
        assert!(out.outgoing.is_empty());
        opened.extend(out.opened);
    }
    assert_eq!(opened.len(), 1);
    assert!(opened[0].shredded);
    assert_eq!(a.queued_for(b.identity().peer_id_hex), 0);
    assert!(!a.burn_is_live(id));
    la.abort_outgoing(pending.batch_token.unwrap()).unwrap();
    assert!(a.inner.lock().queues.values().all(|q| q.is_empty()));
}

#[test]
fn failed_opened_persistence_does_not_consume_control_replay_id() {
    let dir = tempfile::tempdir().unwrap();
    let (_, _, a, b) = persisted_pair(dir.path());
    let (la, lb, _) = link(&a, &b);
    let id = a
        .queue_message(
            b.identity().peer_id_hex,
            "retry OPENED storage".into(),
            Some(spec(BurnMode::OnOpen, 0, 604_800)),
        )
        .unwrap();
    let pending = la.flush().unwrap();
    for p in pending.outgoing {
        lb.on_pdu(p).unwrap();
    }
    b.open_burn(id.clone(), now_ms()).unwrap().unwrap();
    let receipt = lb.flush().unwrap();
    let db = rusqlite::Connection::open(dir.path().join("a.db")).unwrap();
    db.execute_batch("CREATE TRIGGER reject_shred BEFORE DELETE ON burn_keys BEGIN SELECT RAISE(ABORT,'test'); END;").unwrap();
    let mut failed = false;
    for p in receipt.outgoing {
        failed |= la.on_pdu(p).is_err();
    }
    assert!(failed && !la.is_established());
    assert!(a.inner.lock().queues.values().all(|q| q.is_empty()));
    lb.abort_outgoing(receipt.batch_token.unwrap()).unwrap();
    db.execute_batch("DROP TRIGGER reject_shred").unwrap();
    let (_, _, outs) = link(&a, &b);
    assert_eq!(outs.iter().map(|o| o.opened.len()).sum::<usize>(), 1);
    assert!(!a.burn_is_live(id));
    assert!(a
        .stored_messages(b.identity().peer_id_hex)
        .unwrap()
        .is_empty());
}

#[test]
fn opened_cannot_shred_a_burn_message_sent_by_the_receipt_sender() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let id = b
        .queue_message(
            a.identity().peer_id_hex,
            "Bob cannot remotely erase Alice's receipt".into(),
            Some(spec(BurnMode::OnOpen, 0, 604_800)),
        )
        .unwrap();
    pump(&lb, &la, lb.flush().unwrap());
    let peer = parse_id(&a.identity().peer_id_hex).unwrap();
    let fake = Queued {
        envelope_id: random_id16().unwrap(),
        created_ms: now_ms(),
        control: true,
        inner: Zeroizing::new(
            core_burn::encode_app(
                None,
                &AppContent::Opened(core_burn::OpenedReceipt {
                    msg_id: parse_msg_id(&id).unwrap(),
                    opened_ms: now_ms(),
                }),
            )
            .unwrap(),
        ),
    };
    {
        let mut node = b.inner.lock();
        node.persist_queued(&peer, &fake).unwrap();
        node.queues.entry(peer).or_default().push_back(fake);
    }
    let outs = pump_out(&lb, &la, lb.flush().unwrap());
    assert!(outs.iter().all(|o| o.opened.is_empty()));
    assert!(a.burn_is_live(id.clone()));
    let records = a.stored_messages(b.identity().peer_id_hex).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id_hex, id);
    assert!(!records[0].mine);
}

#[test]
fn idle_media_request_starts_kept_file_reply_without_an_external_kick() {
    let (a, b, la, lb) = paired();
    pump(&la, &lb, la.start().unwrap());
    let offer = b
        .queue_file(
            a.identity().peer_id_hex,
            "kept.txt".into(),
            "text/plain".into(),
            b"kept bytes".to_vec(),
        )
        .unwrap();
    let original = pump_out(&lb, &la, lb.flush().unwrap());
    assert_eq!(original.iter().map(|o| o.files.len()).sum::<usize>(), 1);
    // Model the supported hash-bound missing-media placeholder without changing
    // the live transport. No external kick/flush of Bob is permitted below.
    {
        let node = a.inner.lock();
        node.update_message(&parse_msg_id(&offer.transfer_id_hex).unwrap(), |m| {
            m.file.as_mut().unwrap().state = st::FILE_MISSING
        })
        .unwrap();
    }
    a.request_media(offer.transfer_id_hex).unwrap();
    let outs = pump_out(&la, &lb, la.flush().unwrap());
    let files: Vec<_> = outs.into_iter().flat_map(|o| o.files).collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].bytes, b"kept bytes");
}
