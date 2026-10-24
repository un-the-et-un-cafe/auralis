use super::*;
use crate::burn::BurnHeader;
use crate::store::{FileRec, BURN_UNOPENED, FILE_RECEIVED, STATUS_RECEIVED, STATUS_SENT};

const SEED: [u8; 32] = [0x42; 32];
const ALEX: [u8; 32] = [7; 32];

fn store() -> Store {
    Store::open_in_memory(&[1; 32]).unwrap()
}

fn text(id: u8, t: &str, ms: u64) -> MessageRec {
    MessageRec {
        id: [id; 16],
        peer: ALEX,
        mine: id.is_multiple_of(2),
        time_ms: ms,
        status: if id.is_multiple_of(2) {
            STATUS_SENT
        } else {
            STATUS_RECEIVED
        },
        text: t.into(),
        mime: None,
        burn: None,
        burn_state: 0,
        burn_at_ms: None,
        file: None,
    }
}

const PHOTO: &[u8] = b"\x89PNG not really a photo but some bytes";

fn photo(id: u8, ms: u64) -> MessageRec {
    MessageRec {
        text: "caption: sunset".into(),
        mime: Some("image/png".into()),
        file: Some(FileRec {
            name: "sunset.png".into(),
            mime: "image/png".into(),
            size: PHOTO.len() as u64,
            state: FILE_RECEIVED,
            done_bytes: PHOTO.len() as u64,
            local_path: Some("/data/files/sunset.png".into()),
            hash: Some(blake3::hash(PHOTO).as_bytes().to_vec()),
            locator: Some(vec![id; 16]),
        }),
        ..text(id, "", ms)
    }
}

fn burn(id: u8) -> MessageRec {
    let h = BurnHeader::on_open(3600).unwrap();
    MessageRec {
        burn: Some(h.encode().to_vec()),
        burn_state: BURN_UNOPENED,
        ..text(id, "", 9)
    }
}

fn names() -> Vec<BackupName> {
    vec![BackupName {
        peer: ALEX.to_vec(),
        name: "Alex".into(),
    }]
}

fn run(s: &Store, st: &BackupState, now: u64, snap: bool) -> Option<(Upload, BackupState)> {
    let key = chat_backup_key(&SEED).unwrap();
    plan(s, st, &names(), now, snap)
        .unwrap()
        .map(|(u, n)| (seal_upload(&key, u).unwrap(), n))
}

#[test]
fn key_is_derived_from_the_24_words_with_the_label() {
    let a = chat_backup_key(&SEED).unwrap();
    assert_eq!(*a, *chat_backup_key(&SEED).unwrap());
    assert_ne!(*a, *chat_backup_key(&[0x43; 32]).unwrap());
    let mut k = [0u8; 32];
    Hkdf::<Sha256>::new(None, &SEED)
        .expand(b"hopla/chat-backup/v1", &mut k)
        .unwrap();
    assert_eq!(*a, k);
}

#[test]
fn cb1_restore_brings_back_all_text_and_no_media_bytes() {
    let s = store();
    for i in 1..=40u8 {
        s.put_message(&text(i, &format!("message {i} ✓"), u64::from(i)))
            .unwrap();
    }
    s.put_message(&photo(41, 41)).unwrap();
    let (u1, st1) = run(&s, &BackupState::default(), 1_000, false).unwrap();
    assert_eq!(u1.kind, KIND_SNAPSHOT);
    // An edit and a new message go into an incremental log.
    s.put_message(&text(3, "message 3 (edited)", 3)).unwrap();
    s.put_message(&text(42, "after the snapshot", 42)).unwrap();
    let (u2, st2) = run(&s, &st1, 1_000 + TEXT_INTERVAL_MS, false).unwrap();
    assert_eq!(u2.kind, KIND_LOG);
    assert!(run(&s, &st2, 2_000 + TEXT_INTERVAL_MS, false).is_none());
    for u in [&u1, &u2] {
        assert!(!u.blob.windows(PHOTO.len()).any(|w| w == PHOTO));
    }

    let fresh = store();
    let key = chat_backup_key(&SEED).unwrap();
    let r = restore(&key, &[u2.blob.clone(), u1.blob.clone()], &fresh).unwrap();
    let before: Vec<_> = s.all_messages().unwrap();
    let after = fresh.all_messages().unwrap();
    assert_eq!(after.len(), before.len());
    assert_eq!(r.messages, 42);
    for (b, a) in before.iter().zip(&after) {
        assert_eq!(a.text, b.text, "100% of message text");
        assert_eq!((a.time_ms, a.mine, a.peer), (b.time_ms, b.mine, b.peer));
    }
    assert_eq!(r.names, names());
    // The photo is a placeholder: type, size, caption, hash and locator, no bytes or path.
    let p = after.iter().find(|m| m.id == [41; 16]).unwrap();
    let f = p.file.as_ref().unwrap();
    assert_eq!(f.state, FILE_MISSING);
    assert_eq!((f.mime.as_str(), f.size), ("image/png", PHOTO.len() as u64));
    assert_eq!(p.text, "caption: sunset");
    assert!(f.local_path.is_none() && f.hash.is_some() && f.locator.is_some());
    assert_eq!(r.media_placeholders, 1);
    assert_eq!(r.state.seq, 2);
}

#[test]
fn cb2_needs_the_24_words_and_sizes_are_padded() {
    let s = store();
    s.put_message(&text(1, "secret plan", 1)).unwrap();
    let (u, _) = run(&s, &BackupState::default(), 1, false).unwrap();
    assert_eq!(u.blob.len(), BUCKET);
    assert!(!u.blob.windows(11).any(|w| w == b"secret plan"));
    let wrong = chat_backup_key(&[0x41; 32]).unwrap();
    assert_eq!(
        restore(&wrong, std::slice::from_ref(&u.blob), &store()),
        Err(OmcpError::BackupAuth)
    );
    let mut t = u.blob.clone();
    t[100] ^= 1;
    let key = chat_backup_key(&SEED).unwrap();
    assert!(restore(&key, &[t], &store()).is_err());
    // A large history still lands on a bucket boundary.
    let big = store();
    for i in 0..200u8 {
        big.put_message(&text(i, &"x".repeat(900), u64::from(i)))
            .unwrap();
    }
    let (u, _) = run(&big, &BackupState::default(), 1, false).unwrap();
    assert!(u.blob.len() > BUCKET && u.blob.len() % BUCKET == 0);
}

#[test]
fn cb4_burn_content_is_never_in_the_backup() {
    let s = store();
    s.put_message(&text(1, "normal", 1)).unwrap();
    s.put_message(&burn(2)).unwrap();
    let (u, st) = run(&s, &BackupState::default(), 1, false).unwrap();
    let fresh = store();
    let key = chat_backup_key(&SEED).unwrap();
    restore(&key, &[u.blob], &fresh).unwrap();
    let got = fresh.all_messages().unwrap();
    assert_eq!(got.len(), 1);
    assert!(got.iter().all(|m| m.burn.is_none()));
    assert!(!st.digests.contains_key([2u8; 16].as_slice()));
}

#[test]
fn deletions_and_weekly_snapshot() {
    let s = store();
    s.put_message(&text(1, "a", 1)).unwrap();
    s.put_message(&text(2, "b", 2)).unwrap();
    let (u1, st1) = run(&s, &BackupState::default(), 10, false).unwrap();
    s.delete_message(&[2; 16]).unwrap();
    let (u2, st2) = run(&s, &st1, 20, false).unwrap();
    assert_eq!(u2.kind, KIND_LOG);
    let key = chat_backup_key(&SEED).unwrap();
    let fresh = store();
    restore(&key, &[u1.blob, u2.blob], &fresh).unwrap();
    assert_eq!(fresh.all_messages().unwrap().len(), 1);
    s.put_message(&text(3, "c", 3)).unwrap();
    let (u3, _) = run(&s, &st2, 10 + SNAPSHOT_INTERVAL_MS, false).unwrap();
    assert_eq!((u3.kind, u3.delete_below_seq), (KIND_SNAPSHOT, Some(3)));
    assert!(!text_backup_due(&st2, 30, true, false));
    assert!(text_backup_due(&st2, 20 + TEXT_INTERVAL_MS, true, false));
    assert!(!text_backup_due(&st2, 20 + TEXT_INTERVAL_MS, false, false));
    assert!(text_backup_due(&st2, 20 + TEXT_INTERVAL_MS, false, true));
}

#[test]
fn cb5_media_backup_roundtrip_and_off_uploads_nothing() {
    let s = store();
    s.put_message(&photo(5, 5)).unwrap();
    let mut video = photo(6, 6);
    video.file.as_mut().unwrap().mime = "video/mp4".into();
    s.put_message(&video).unwrap();
    let st = BackupState::default();
    // Off (default): no media planned.
    let off = MediaSettings::default();
    assert!(!off.enabled && off.photos && off.voice && off.files && !off.videos);
    assert!(plan_media(&s, &off, &st).unwrap().is_empty());
    assert!(!media_backup_due(&off, &st, MEDIA_INTERVAL_MS, true));
    // On: photo (pre-selected) but not video.
    let on = MediaSettings {
        enabled: true,
        ..off
    };
    let todo = plan_media(&s, &on, &st).unwrap();
    assert_eq!(todo.len(), 1);
    assert!(media_backup_due(&on, &st, MEDIA_INTERVAL_MS, true));
    assert!(!media_backup_due(&on, &st, MEDIA_INTERVAL_MS, false));
    let key = chat_backup_key(&SEED).unwrap();
    let u = seal_media(&key, &todo[0].hash, PHOTO).unwrap();
    assert_eq!(u.blob.len() % BUCKET, 0);
    assert!(!u.name.contains(&hex::encode(todo[0].hash)));
    let back = open_media(&key, &todo[0].hash, &u.blob).unwrap();
    assert_eq!(back.as_slice(), PHOTO, "hash-verified media restore");
    assert_eq!(
        seal_media(&key, &todo[0].hash, b"other").unwrap_err(),
        OmcpError::FileHashMismatch
    );
    assert!(open_media(&key, &[0; 32], &u.blob).is_err());
    // Text blobs never carry the media bytes even with media backup on.
    let (t, _) = run(&s, &st, 1, false).unwrap();
    assert!(!t.blob.windows(PHOTO.len()).any(|w| w == PHOTO));
    let mut done = st.clone();
    done.media_uploaded.insert(todo[0].hash.to_vec());
    assert!(plan_media(&s, &on, &done).unwrap().is_empty());
    assert_eq!(media_class("audio/ogg"), MediaClass::Voice);
    assert_eq!(media_class("application/pdf"), MediaClass::File);
}

#[test]
fn state_and_settings_persist_in_the_store() {
    let s = store();
    let st = BackupState {
        seq: 7,
        ..BackupState::default()
    };
    save_state(&s, &st).unwrap();
    assert_eq!(load_state(&s).unwrap(), st);
    assert_eq!(load_media_settings(&s).unwrap(), MediaSettings::default());
    let m = MediaSettings {
        enabled: true,
        ..MediaSettings::default()
    };
    save_media_settings(&s, &m).unwrap();
    assert_eq!(load_media_settings(&s).unwrap(), m);
}
