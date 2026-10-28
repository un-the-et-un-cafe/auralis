use super::*;
use crate::burn::{BurnHeader, BurnStore, Opened, UNREAD_TTL_7D};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};

const KEY: [u8; 32] = [0x11; 32];
const PEER: [u8; 32] = [0x22; 32];

fn deny_commit_and_rollback(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        // rusqlite 0.32 maps COMMIT to Unknown; allow only BEGIN here.
        AuthAction::Transaction { operation } if operation != TransactionOperation::Begin => {
            Authorization::Deny
        }
        _ => Authorization::Allow,
    }
}

fn assert_quarantined(s: &Store) {
    assert!(s.poisoned.get());
    assert_eq!(s.transaction_depth.get(), 0);
    let called = Cell::new(false);
    assert!(s
        .transaction(|_| {
            called.set(true);
            Ok(())
        })
        .is_err());
    assert!(
        !called.get(),
        "quarantined transactions must not enter the body"
    );
    assert!(s.put_message(&msg(99, "must not write", 99)).is_err());
    assert!(s.delete_message(&[1; 16]).is_err());
    assert!(s.put_meta(b"quarantined", &true).is_err());
    assert!(s.message(&[1; 16]).is_err());
    assert!(s.all_messages().is_err());
    assert!(s.schema_version().is_err());
    assert!(s.export_history().is_err());
    assert!(s.commit_outgoing(&PEER, &[], None).is_err());
}

#[test]
fn failed_commit_and_rollback_quarantines_until_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollback.db");
    let s = Store::open(&path, &KEY).unwrap();
    s.put_message(&msg(1, "already durable", 1)).unwrap();
    s.conn.authorizer(Some(deny_commit_and_rollback));
    assert!(s
        .transaction(|s| {
            s.delete_message(&[1; 16])?;
            s.put_message(&msg(2, "not committed", 2))
        })
        .is_err());
    assert!(!s.conn.is_autocommit());
    assert_quarantined(&s);
    // Clearing the temporary fault is not permission to trust the connection.
    s.conn
        .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
    assert_quarantined(&s);
    assert!(!s.conn.is_autocommit());
    drop(s);
    let reopened = Store::open(&path, &KEY).unwrap();
    assert_eq!(
        reopened.message(&[1; 16]).unwrap().unwrap().text,
        "already durable"
    );
    assert!(reopened.message(&[2; 16]).unwrap().is_none());
    assert!(reopened.message(&[99; 16]).unwrap().is_none());
    reopened
        .transaction(|s| s.put_message(&msg(3, "durable after reopen", 3)))
        .unwrap();
    drop(reopened);
    assert!(Store::open(&path, &KEY)
        .unwrap()
        .message(&[3; 16])
        .unwrap()
        .is_some());
}

#[test]
fn failed_body_and_rollback_quarantines_store() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.conn.authorizer(Some(deny_commit_and_rollback));
    let result: Result<()> = s.transaction(|s| {
        s.put_message(&msg(1, "not committed", 1))?;
        Err(OmcpError::Store("body failed"))
    });
    assert!(result.is_err());
    assert!(!s.conn.is_autocommit());
    assert_quarantined(&s);
}

#[test]
fn caught_nested_rollback_failure_cannot_commit_outer_transaction() {
    for body_fails in [false, true] {
        let s = Store::open_in_memory(&KEY).unwrap();
        s.conn
            .authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
                AuthAction::Savepoint { operation, .. }
                    if operation != TransactionOperation::Begin =>
                {
                    Authorization::Deny
                }
                _ => Authorization::Allow,
            }));
        let result = s.transaction(|s| {
            s.put_message(&msg(1, "outer", 1))?;
            let inner = s.transaction(|s| {
                s.put_message(&msg(2, "inner", 2))?;
                if body_fails {
                    return Err(OmcpError::Store("inner failed"));
                }
                Ok(())
            });
            assert!(inner.is_err());
            assert!(s.put_message(&msg(3, "after caught error", 3)).is_err());
            Ok(()) // Even catching the inner error cannot authorize COMMIT.
        });
        assert!(result.is_err());
        assert!(s.conn.is_autocommit(), "outer rollback is still attempted");
        assert_eq!(
            s.conn
                .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert_quarantined(&s); // Sticky even when outer rollback succeeded.
    }
}

#[test]
fn panic_with_failed_rollback_quarantines_store() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.conn.authorizer(Some(deny_commit_and_rollback));
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<()> = s.transaction(|s| {
            s.put_message(&msg(1, "panic", 1))?;
            panic!("test failed rollback during unwinding")
        });
    }));
    assert!(panic.is_err());
    assert!(!s.conn.is_autocommit());
    assert_quarantined(&s);
}

#[test]
fn unowned_transaction_is_never_adopted_as_nested() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    assert!(s.transaction(|_| Ok(())).is_err());
    assert_quarantined(&s);
    s.conn.execute_batch("ROLLBACK").unwrap();
    assert_quarantined(&s);
}

#[test]
fn unexpected_transaction_end_blocks_further_writes_and_outer_success() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let result = s.transaction(|s| {
        s.put_message(&msg(1, "rolled back", 1))?;
        // Simulate the state after SQLite automatically rolls back a transaction
        // (e.g. an I/O error): the live guard must not allow new autocommit writes.
        s.conn.execute_batch("ROLLBACK").unwrap();
        assert!(s.put_message(&msg(2, "must not autocommit", 2)).is_err());
        Ok(())
    });
    assert!(result.is_err());
    assert!(s.conn.is_autocommit());
    assert_eq!(
        s.conn
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_quarantined(&s);
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Every byte of the store on disk (DB file plus any journal left behind).
fn disk(dir: &Path) -> Vec<u8> {
    let mut all = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        all.extend(std::fs::read(e.unwrap().path()).unwrap());
    }
    all
}

fn msg(id: u8, text: &str, t: u64) -> MessageRec {
    MessageRec {
        id: [id; 16],
        peer: PEER,
        mine: id.is_multiple_of(2),
        time_ms: t,
        status: STATUS_RECEIVED,
        text: text.into(),
        mime: None,
        burn: None,
        burn_state: BURN_NONE,
        burn_at_ms: None,
        file: None,
    }
}

#[test]
fn survives_reopen_and_is_encrypted_at_rest() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.bind_owner(&[1; 32]).unwrap();
        s.put_contact(&ContactRec {
            peer: PEER,
            name: "Zanzibar Quokka".into(),
            verified: true,
            device: None,
            dh: None,
        })
        .unwrap();
        s.put_message(&msg(1, "meet at the old lighthouse", 10))
            .unwrap();
        let mut m2 = msg(2, "", 20);
        m2.file = Some(FileRec {
            name: "holiday-plan.pdf".into(),
            mime: "application/pdf".into(),
            size: 1234,
            state: FILE_RECEIVED,
            done_bytes: 1234,
            local_path: Some("/data/files/holiday-plan.pdf".into()),
            hash: None,
            locator: None,
        });
        s.put_message(&m2).unwrap();
        s.put_outbox(
            &OutboxRec {
                id: [9; 16],
                peer: PEER,
                created_ms: 30,
                inner: b"queued secret".to_vec(),
                control: false,
                burn: false,
            },
            None,
        )
        .unwrap();
        s.put_out_file(
            &OutFileRec {
                id: [5; 16],
                peer: PEER,
                name: "big.bin".into(),
                size: 3,
                total: 2,
                offer: b"offer".to_vec(),
            },
            &[b"chunk-zero".to_vec(), b"chunk-one".to_vec()],
        )
        .unwrap();
    }
    let raw = disk(dir.path());
    for needle in [
        &b"Zanzibar Quokka"[..],
        b"meet at the old lighthouse",
        b"holiday-plan",
        b"queued secret",
        b"chunk-zero",
        &PEER,
        &[1u8; 16],
    ] {
        assert!(!contains(&raw, needle), "plaintext leaked: {needle:?}");
    }

    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    s.bind_owner(&[1; 32]).unwrap();
    assert!(s.bind_owner(&[2; 32]).is_err());
    let c = s.contacts().unwrap();
    assert_eq!(c.len(), 1);
    assert!(c[0].verified);
    let ms = s.messages(&PEER).unwrap();
    assert_eq!(ms.len(), 2);
    assert_eq!(ms[0].text, "meet at the old lighthouse");
    assert_eq!(
        ms[1].file.as_ref().unwrap().local_path.as_deref(),
        Some("/data/files/holiday-plan.pdf")
    );
    assert_eq!(s.chats().unwrap()[0].last_ms, 20);
    assert_eq!(s.outbox().unwrap()[0].inner, b"queued secret");
    let files = s.out_files().unwrap();
    assert_eq!(files[0].1.len(), 2);
    assert_eq!(&files[0].1[1][..], b"chunk-one");
    s.delete_out_file(&[5; 16]).unwrap();
    assert!(s.out_files().unwrap().is_empty());
}

#[test]
fn update_keeps_order_and_wrong_key_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.put_message(&msg(1, "a", 1)).unwrap();
        s.put_message(&msg(2, "b", 2)).unwrap();
        let mut m = msg(1, "a", 1);
        m.status = STATUS_SENT;
        s.put_message(&m).unwrap();
        let ms = s.messages(&PEER).unwrap();
        assert_eq!((ms[0].id, ms[0].status), ([1; 16], STATUS_SENT));
    }
    assert_eq!(
        Store::open(&path, &[0x12; 32]).err(),
        Some(OmcpError::StoreAuth)
    );
}

#[test]
fn rows_are_bound_to_their_id() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.put_message(&msg(1, "one", 1)).unwrap();
    s.put_message(&msg(2, "two", 2)).unwrap();
    // Swap the ciphertexts of the two rows: both must fail to open.
    s.conn
        .execute_batch(
            "UPDATE messages SET ct = (SELECT ct FROM messages m2 WHERE m2.id <> messages.id)",
        )
        .unwrap();
    assert_eq!(s.messages(&PEER).err(), Some(OmcpError::StoreAuth));
}

#[test]
fn migrations_are_versioned_and_newer_schema_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    }
    // Re-opening is idempotent.
    assert_eq!(
        Store::open(&path, &KEY).unwrap().schema_version().unwrap(),
        SCHEMA_VERSION
    );
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 1))
            .unwrap();
    }
    assert_eq!(
        Store::open(&path, &KEY).err(),
        Some(OmcpError::Store("store schema is newer than this app"))
    );
}

/// REQUIREMENTS B1 / PROTOCOL §19.1: a burned message, its key and its unsent
/// copy are gone from disk, and nothing about it comes back after a restart.
#[test]
fn b1_burned_message_is_not_recoverable_after_restart() {
    const ID: [u8; 16] = [0xB1; 16];
    const ID2: [u8; 16] = [0xB2; 16];
    const SECRET: &[u8] = b"self-destructing rendezvous coordinates";
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    let h = BurnHeader::timer(10, UNREAD_TTL_7D).unwrap();

    let (key, sealed) = {
        let s = Store::open(&path, &KEY).unwrap();
        let mut bs = BurnStore::default();
        bs.insert(ID, h, SECRET, 0).unwrap();
        bs.insert(ID2, h, b"another one", 0).unwrap();
        let snap = bs.snapshot(&ID).unwrap();
        s.put_burn(&snap, &PEER, BurnDir::In, None).unwrap();
        s.put_burn(&bs.snapshot(&ID2).unwrap(), &PEER, BurnDir::Out, None)
            .unwrap();
        s.put_outbox(
            &OutboxRec {
                id: ID2,
                peer: PEER,
                created_ms: 0,
                inner: b"unsent burn payload".to_vec(),
                control: false,
                burn: false,
            },
            Some(&bs.snapshot(&ID2).unwrap().key),
        )
        .unwrap();
        let mut m = msg(0xB1, "", 0);
        m.burn = Some(h.encode().to_vec());
        m.burn_state = BURN_UNOPENED;
        m.text = "must never be stored".into();
        s.put_message(&m).unwrap();
        (snap.key.clone(), snap.sealed.clone())
    };
    assert!(!contains(&disk(dir.path()), SECRET));
    assert!(!contains(&disk(dir.path()), b"must never be stored"));

    // Restart #1: the message is still there and opens exactly once.
    {
        let s = Store::open(&path, &KEY).unwrap();
        let mut bs = BurnStore::default();
        for b in s.burns().unwrap() {
            bs.restore(b.snapshot);
        }
        assert_eq!(s.outbox().unwrap()[0].inner, b"unsent burn payload");
        let Opened::Content { body, .. } = bs.open(&ID, 1_000).unwrap() else {
            panic!("expected content");
        };
        assert_eq!(&body[..], SECRET);
        s.put_burn(&bs.snapshot(&ID).unwrap(), &PEER, BurnDir::In, None)
            .unwrap();
        // The read timer runs out (ID) and the unread sent copy expires (ID2):
        // shred them the way the node does.
        assert_eq!(bs.tick(11_000), vec![ID]);
        s.delete_burn(&ID).unwrap();
        for id in bs.tick(u64::from(UNREAD_TTL_7D) * 1000) {
            s.delete_burn(&id).unwrap();
        }
        s.delete_message(&ID).unwrap();
        assert!(bs.is_empty());
    }

    // Restart #2: nothing to restore, and no trace of content, key or ciphertext.
    let s = Store::open(&path, &KEY).unwrap();
    assert!(s.burns().unwrap().is_empty());
    assert!(s.outbox().unwrap().is_empty());
    assert!(s.message(&ID).unwrap().is_none());
    drop(s);
    let raw = disk(dir.path());
    assert!(!contains(&raw, SECRET));
    assert!(!contains(&raw, b"unsent burn payload"));
    assert!(!contains(&raw, &key[..]), "per-message key left on disk");
    assert!(!contains(&raw, &sealed), "burn ciphertext left on disk");
    // Even with the storage key, no burn row remains to decrypt.
    let c = Connection::open(&path).unwrap();
    for t in ["burn", "burn_keys", "outbox"] {
        let n: i64 = c
            .query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{t} not empty");
    }
}

#[test]
fn history_export_excludes_burn_and_imports() {
    let s = Store::open_in_memory(&KEY).unwrap();
    s.put_message(&msg(1, "keep me", 1)).unwrap();
    let mut b = msg(3, "", 2);
    b.burn = Some(
        BurnHeader::on_open(UNREAD_TTL_7D)
            .unwrap()
            .encode()
            .to_vec(),
    );
    s.put_message(&b).unwrap();
    let mut f = msg(5, "", 3);
    f.file = Some(FileRec {
        name: "x".into(),
        mime: "text/plain".into(),
        size: 1,
        state: FILE_RECEIVING,
        done_bytes: 0,
        local_path: Some("/old/device".into()),
        hash: None,
        locator: None,
    });
    s.put_message(&f).unwrap();
    s.put_outbox(
        &OutboxRec {
            id: [7; 16],
            peer: PEER,
            created_ms: 1,
            inner: b"pending".to_vec(),
            control: false,
            burn: false,
        },
        None,
    )
    .unwrap();
    let h = s.export_history().unwrap();

    let t = Store::open_in_memory(&[0x33; 32]).unwrap();
    t.import_history(&h).unwrap();
    let ms = t.messages(&PEER).unwrap();
    assert_eq!(ms.len(), 2);
    assert_eq!(ms[0].text, "keep me");
    let file = ms[1].file.as_ref().unwrap();
    assert_eq!(
        (file.state, file.local_path.as_deref()),
        (FILE_FAILED, None)
    );
    assert_eq!(t.outbox().unwrap().len(), 1);
    assert!(t.import_history(b"junk").is_err());
}

#[test]
fn v014a_rekey_legacy_peer_and_owner() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let legacy_owner = [1; 32];
    let owner = [2; 32];
    s.bind_owner(&legacy_owner).unwrap();
    assert!(s.rebind_owner(&legacy_owner, &owner).unwrap());
    assert!(!s.rebind_owner(&legacy_owner, &owner).unwrap());
    assert!(s.bind_owner(&legacy_owner).is_err());
    s.put_contact(&ContactRec {
        peer: PEER,
        name: "Legacy".into(),
        verified: true,
        device: None,
        dh: None,
    })
    .unwrap();
    s.put_message(&msg(1, "before the upgrade", 10)).unwrap();
    let new = [0x42; 32];
    s.rekey_peer(&PEER, &new).unwrap();
    let cs = s.contacts().unwrap();
    assert_eq!(cs.len(), 1);
    assert_eq!(cs[0].peer, new);
    assert!(cs[0].verified);
    assert!(s.messages(&PEER).unwrap().is_empty());
    let ms = s.messages(&new).unwrap();
    assert_eq!(ms.len(), 1);
    assert_eq!(ms[0].peer, new);
    assert!(s.chat(&new).unwrap().is_some());
    assert!(s.chat(&PEER).unwrap().is_none());
}

#[test]
fn v1_store_migrates_to_kept_media_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("omcp.db");
    {
        let c = Connection::open(&path).unwrap();
        c.execute_batch(MIGRATIONS[0]).unwrap();
        c.execute_batch("PRAGMA user_version = 1").unwrap();
    }
    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(s.kept_media_index().unwrap().is_empty());
}

#[test]
fn kept_media_sealed_evicts_oldest_and_by_age() {
    let dir = tempfile::tempdir().unwrap();
    let s = Store::open(&dir.path().join("omcp.db"), &KEY).unwrap();
    let day = 86_400_000u64;
    let rec = |n: u8, kept_ms: u64| KeptMediaRec {
        id: [n; 16],
        peer: PEER,
        hash: [n; 32],
        kept_ms,
        size: 1000,
    };
    let lim = KeptMediaLimits {
        max_days: 90,
        max_bytes: 2500,
    };
    let secret = b"KEPT-MEDIA-PLAINTEXT-MARKER".repeat(37);
    let mut r1 = rec(1, day);
    r1.size = secret.len() as u64;
    assert!(s
        .put_kept_media(&r1, &secret, KeptMediaLimits::default(), day)
        .unwrap()
        .is_empty());
    assert!(
        !contains(&disk(dir.path()), b"KEPT-MEDIA-PLAINTEXT-MARKER"),
        "sealed at rest"
    );
    assert_eq!(
        s.kept_media(&[1; 16]).unwrap().unwrap().as_slice(),
        &secret[..]
    );
    s.delete_kept_media(&[1; 16]).unwrap();
    assert!(s.kept_media(&[1; 16]).unwrap().is_none());
    // Size limit: oldest first.
    for n in 1..=3 {
        s.put_kept_media(&rec(n, day * u64::from(n)), &[n; 1000], lim, day * 3)
            .unwrap();
    }
    assert!(s.kept_media(&[1; 16]).unwrap().is_none());
    assert!(s.kept_media(&[2; 16]).unwrap().is_some());
    // Age limit: 30 days later both are kept, after 90 days the older one goes.
    assert!(s.evict_kept_media(lim, day * 32).unwrap().is_empty());
    assert_eq!(
        s.evict_kept_media(lim, day * 92 + 1).unwrap(),
        vec![[2; 16]]
    );
    // Defaults are 90 days / 2 GB.
    assert_eq!(
        KeptMediaLimits::default(),
        KeptMediaLimits {
            max_days: 90,
            max_bytes: 2_000_000_000
        }
    );
    // Re-keyed with the contact.
    s.rekey_peer(&PEER, &[0x33; 32]).unwrap();
    assert_eq!(s.kept_media_index().unwrap()[0].peer, [0x33; 32]);
}

fn queued_message(s: &Store, n: u8) -> [u8; 16] {
    let mut m = msg(n, "queued text", n as u64);
    m.mine = true;
    m.status = STATUS_QUEUED;
    s.put_message(&m).unwrap();
    s.put_outbox(
        &OutboxRec {
            id: m.id,
            peer: PEER,
            created_ms: m.time_ms,
            inner: b"encoded queued text".to_vec(),
            control: false,
            burn: false,
        },
        None,
    )
    .unwrap();
    m.id
}

fn queued_file(s: &Store, n: u8) -> [u8; 16] {
    let mut m = msg(n, "", n as u64);
    m.mine = true;
    m.status = STATUS_QUEUED;
    m.file = Some(FileRec {
        name: "outgoing.bin".into(),
        mime: "application/octet-stream".into(),
        size: 10,
        state: FILE_QUEUED,
        done_bytes: 0,
        local_path: None,
        hash: None,
        locator: None,
    });
    s.put_message(&m).unwrap();
    s.put_out_file(
        &OutFileRec {
            id: m.id,
            peer: PEER,
            name: "outgoing.bin".into(),
            size: 10,
            total: 2,
            offer: b"file offer".to_vec(),
        },
        &[b"first".to_vec(), b"second".to_vec()],
    )
    .unwrap();
    m.id
}

#[test]
fn outgoing_commit_updates_progress_and_final_batch_atomically_across_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ack.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        let text = queued_message(&s, 2);
        let file = queued_file(&s, 4);
        s.commit_outgoing(
            &PEER,
            &[(text, false)],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 5,
                complete: false,
            }),
        )
        .unwrap();
        assert!(s.outbox().unwrap().is_empty());
        assert_eq!(s.message(&text).unwrap().unwrap().status, STATUS_SENT);
        let partial = s.message(&file).unwrap().unwrap();
        assert_eq!(partial.status, STATUS_QUEUED);
        assert_eq!(partial.file.as_ref().unwrap().state, FILE_SENDING);
        assert_eq!(partial.file.as_ref().unwrap().done_bytes, 5);
        assert_eq!(s.out_files().unwrap()[0].1.len(), 2);
    }
    {
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.out_files().unwrap().len(), 1);
        s.commit_outgoing(
            &PEER,
            &[],
            Some(OutgoingFileCommit {
                id: [4; 16],
                done_bytes: 10,
                complete: true,
            }),
        )
        .unwrap();
    }
    let s = Store::open(&path, &KEY).unwrap();
    assert!(s.out_files().unwrap().is_empty());
    assert!(s.list_raw(Table::FileChunks, None).unwrap().is_empty());
    let m = s.message(&[4; 16]).unwrap().unwrap();
    assert_eq!(m.status, STATUS_SENT);
    assert_eq!(m.file.as_ref().unwrap().state, FILE_SENT);
    assert_eq!(m.file.as_ref().unwrap().done_bytes, 10);
}

#[test]
fn outgoing_commit_rolls_back_first_middle_and_final_store_write_failures() {
    for table in ["messages", "outbox", "file_chunks", "out_files"] {
        let s = Store::open_in_memory(&KEY).unwrap();
        let text = queued_message(&s, 2);
        let file = queued_file(&s, 4);
        let event = if table == "messages" {
            "UPDATE"
        } else {
            "DELETE"
        };
        s.conn.execute_batch(&format!(
            "CREATE TRIGGER fail_ack BEFORE {event} ON {table} BEGIN SELECT RAISE(FAIL, 'injected'); END;"
        )).unwrap();
        let result = s.commit_outgoing(
            &PEER,
            &[(text, false)],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 10,
                complete: true,
            }),
        );
        assert!(result.is_err(), "{table}");
        assert!(s.conn.is_autocommit(), "{table}");
        assert_eq!(s.outbox().unwrap().len(), 1, "{table}");
        assert_eq!(
            s.message(&text).unwrap().unwrap().status,
            STATUS_QUEUED,
            "{table}"
        );
        assert_eq!(
            s.message(&file).unwrap().unwrap().file.unwrap().state,
            FILE_QUEUED,
            "{table}"
        );
        assert_eq!(s.out_files().unwrap()[0].1.len(), 2, "{table}");
        s.conn.execute_batch("DROP TRIGGER fail_ack").unwrap();
        s.commit_outgoing(
            &PEER,
            &[(text, false)],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 10,
                complete: true,
            }),
        )
        .unwrap();
    }
}

#[test]
fn outgoing_commit_failure_at_sql_commit_rolls_back_and_releases_transaction() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let text = queued_message(&s, 2);
    s.conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         CREATE TABLE ack_parent(id INTEGER PRIMARY KEY);
         CREATE TABLE ack_child(parent_id INTEGER REFERENCES ack_parent(id) DEFERRABLE INITIALLY DEFERRED);
         CREATE TRIGGER fail_commit AFTER DELETE ON outbox BEGIN INSERT INTO ack_child VALUES (42); END;"
    ).unwrap();
    assert!(s.commit_outgoing(&PEER, &[(text, false)], None).is_err());
    assert!(s.conn.is_autocommit());
    assert_eq!(s.outbox().unwrap().len(), 1);
    assert_eq!(s.message(&text).unwrap().unwrap().status, STATUS_QUEUED);
    let children: i64 = s
        .conn
        .query_row("SELECT COUNT(*) FROM ack_child", [], |r| r.get(0))
        .unwrap();
    assert_eq!(children, 0);
    s.conn.execute_batch("DROP TRIGGER fail_commit").unwrap();
    s.commit_outgoing(&PEER, &[(text, false)], None).unwrap();
}

#[test]
fn outgoing_commit_missing_cancelled_rows_never_resurrects_messages() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let text = queued_message(&s, 2);
    let file = queued_file(&s, 4);
    s.delete_outbox(&text).unwrap();
    s.delete_message(&text).unwrap();
    s.delete_out_file(&file).unwrap();
    s.delete_message(&file).unwrap();
    for _ in 0..2 {
        s.commit_outgoing(
            &PEER,
            &[(text, false)],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 10,
                complete: true,
            }),
        )
        .unwrap();
    }
    assert!(s.all_messages().unwrap().is_empty());
    assert!(s.outbox().unwrap().is_empty());
    assert!(s.out_files().unwrap().is_empty());
}

#[test]
fn outgoing_commit_wrong_owner_direction_control_and_progress_fail_closed() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let text = queued_message(&s, 2);
    let file = queued_file(&s, 4);
    assert!(s.commit_outgoing(&[7; 32], &[(text, false)], None).is_err());
    assert!(s.commit_outgoing(&PEER, &[(text, true)], None).is_err());
    assert!(s
        .commit_outgoing(
            &[7; 32],
            &[],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 10,
                complete: true
            })
        )
        .is_err());
    for (done_bytes, complete) in [(11, true), (9, true), (11, false)] {
        assert!(s
            .commit_outgoing(
                &PEER,
                &[],
                Some(OutgoingFileCommit {
                    id: file,
                    done_bytes,
                    complete
                })
            )
            .is_err());
    }
    let mut m = s.message(&text).unwrap().unwrap();
    m.mine = false;
    s.put_message(&m).unwrap();
    assert!(s.commit_outgoing(&PEER, &[(text, false)], None).is_err());
    let mut f = s.message(&file).unwrap().unwrap();
    f.peer = [7; 32];
    s.put_message(&f).unwrap();
    assert!(s
        .commit_outgoing(
            &PEER,
            &[],
            Some(OutgoingFileCommit {
                id: file,
                done_bytes: 10,
                complete: true
            })
        )
        .is_err());
    assert_eq!(s.outbox().unwrap().len(), 1);
    assert_eq!(s.out_files().unwrap().len(), 1);
}

#[test]
fn nested_transactions_and_panic_roll_back_without_poisoning_store() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let result: Result<()> = s.transaction(|s| {
        s.put_message(&msg(1, "outer", 1))?;
        s.transaction(|s| s.put_message(&msg(2, "inner", 2)))?;
        Err(OmcpError::Store("outer rejected"))
    });
    assert!(result.is_err());
    assert!(s.all_messages().unwrap().is_empty());
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: Result<()> = s.transaction(|s| {
            s.put_message(&msg(3, "panic", 3))?;
            panic!("test rollback on panic")
        });
    }));
    assert!(panic.is_err());
    assert!(s.conn.is_autocommit());
    assert!(s.all_messages().unwrap().is_empty());
    s.transaction(|s| {
        s.put_message(&msg(4, "retained outer", 4))?;
        let inner: Result<()> = s.transaction(|s| {
            s.put_message(&msg(5, "rejected inner", 5))?;
            Err(OmcpError::Store("inner rejected"))
        });
        assert!(inner.is_err());
        Ok(())
    })
    .unwrap();
    assert_eq!(s.all_messages().unwrap().len(), 1);
    assert_eq!(s.all_messages().unwrap()[0].id, [4; 16]);
}

#[test]
fn durable_received_tombstones_bind_peer_survive_burn_delete_reopen_and_stay_private() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("receive.db");
    let id = [0x56; 16];
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.transaction(|s| {
            s.put_message(&msg(0x56, "transient content", 1))?;
            s.record_received(&PEER, &id)
        })
        .unwrap();
        s.delete_burn(&id).unwrap();
        s.delete_message(&id).unwrap();
    }
    let raw = disk(dir.path());
    assert!(!contains(&raw, &PEER));
    assert!(!contains(&raw, &id));
    let s = Store::open(&path, &KEY).unwrap();
    assert!(s.received_before(&PEER, &id).unwrap());
    assert!(!s.received_before(&[8; 32], &id).unwrap());
    let restored = Store::open_in_memory(&[9; 32]).unwrap();
    restored
        .import_history(&s.export_history().unwrap())
        .unwrap();
    assert!(
        !restored.received_before(&PEER, &id).unwrap(),
        "local replay metadata is not backed up"
    );
    let new_peer = [0x57; 32];
    s.rekey_peer(&PEER, &new_peer).unwrap();
    assert!(s.received_before(&new_peer, &id).unwrap());
    assert!(!s.received_before(&PEER, &id).unwrap());
}

#[test]
fn received_tombstone_and_burn_message_commit_or_roll_back_together() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let mut burns = BurnStore::default();
    let id = [0x61; 16];
    burns
        .insert(
            id,
            BurnHeader::timer(10, UNREAD_TTL_7D).unwrap(),
            b"private burn body",
            0,
        )
        .unwrap();
    let snap = burns.snapshot(&id).unwrap();
    let mut message = msg(0x61, "", 0);
    message.burn = Some(snap.header.encode().to_vec());
    message.burn_state = BURN_UNOPENED;
    s.conn.execute_batch("CREATE TRIGGER fail_receive BEFORE INSERT ON received BEGIN SELECT RAISE(FAIL, 'injected'); END;").unwrap();
    let result: Result<()> = s.transaction(|s| {
        s.put_burn(&snap, &PEER, BurnDir::In, None)?;
        s.put_message(&message)?;
        s.record_received(&PEER, &id)
    });
    assert!(result.is_err());
    assert!(s.burns().unwrap().is_empty());
    assert!(s.message(&id).unwrap().is_none());
    assert!(!s.received_before(&PEER, &id).unwrap());
    s.conn.execute_batch("DROP TRIGGER fail_receive").unwrap();
    s.transaction(|s| {
        s.put_burn(&snap, &PEER, BurnDir::In, None)?;
        s.put_message(&message)?;
        s.record_received(&PEER, &id)
    })
    .unwrap();
    assert!(s.received_before(&PEER, &id).unwrap());
    assert!(s.message(&id).unwrap().is_some());
    assert_eq!(s.burns().unwrap().len(), 1);
}

#[test]
fn received_tombstones_have_bounded_oldest_eviction_without_duplicate_refresh() {
    let s = Store::open_in_memory(&KEY).unwrap();
    let id = |n: usize| (n as u128).to_be_bytes();
    s.transaction(|s| {
        for n in 0..MAX_RECEIVED_TOMBSTONES {
            s.record_received(&PEER, &id(n))?;
        }
        Ok(())
    })
    .unwrap();
    s.record_received(&PEER, &id(0)).unwrap();
    s.record_received(&PEER, &id(MAX_RECEIVED_TOMBSTONES))
        .unwrap();
    assert!(!s.received_before(&PEER, &id(0)).unwrap());
    assert!(s.received_before(&PEER, &id(1)).unwrap());
    assert!(s
        .received_before(&PEER, &id(MAX_RECEIVED_TOMBSTONES))
        .unwrap());
    let count: usize = s
        .conn
        .query_row("SELECT COUNT(*) FROM received", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, MAX_RECEIVED_TOMBSTONES);
    let other = [0x89; 32];
    s.record_received(&other, &id(MAX_RECEIVED_TOMBSTONES + 1))
        .unwrap();
    assert!(s.received_before(&PEER, &id(2)).unwrap());
    let rekeyed = [0x90; 32];
    s.rekey_peer(&other, &rekeyed).unwrap();
    assert!(
        s.received_before(&PEER, &id(2)).unwrap(),
        "re-key at capacity must not evict another peer"
    );
    assert!(s
        .received_before(&rekeyed, &id(MAX_RECEIVED_TOMBSTONES + 1))
        .unwrap());
}

#[test]
fn v2_store_migrates_to_receive_tombstones_without_changing_history_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("upgrade.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.put_message(&msg(1, "existing history", 1)).unwrap();
        s.conn
            .execute_batch("DROP TABLE received; PRAGMA user_version = 2;")
            .unwrap();
    }
    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(s.messages(&PEER).unwrap()[0].text, "existing history");
    assert!(!s.received_before(&PEER, &[1; 16]).unwrap());
    s.record_received(&PEER, &[1; 16]).unwrap();
    assert!(s.received_before(&PEER, &[1; 16]).unwrap());
}

#[test]
fn outgoing_refetch_ack_retires_work_without_rewriting_received_file_history() {
    for mine in [false, true] {
        let s = Store::open_in_memory(&KEY).unwrap();
        let id = queued_file(&s, 4);
        let mut received = s.message(&id).unwrap().unwrap();
        received.mine = mine;
        received.status = if mine { STATUS_SENT } else { STATUS_RECEIVED };
        let file = received.file.as_mut().unwrap();
        file.state = FILE_RECEIVED;
        file.done_bytes = file.size;
        file.local_path = Some("/private/verified.bin".into());
        file.hash = Some(vec![0x6f; 32]);
        s.put_message(&received).unwrap();
        for (done_bytes, complete) in [(5, false), (10, true)] {
            s.commit_outgoing(
                &PEER,
                &[],
                Some(OutgoingFileCommit {
                    id,
                    done_bytes,
                    complete,
                }),
            )
            .unwrap();
            assert_eq!(s.message(&id).unwrap().unwrap(), received);
            assert_eq!(s.out_files().unwrap().is_empty(), complete);
        }
        assert!(s.list_raw(Table::FileChunks, None).unwrap().is_empty());
    }
}

#[test]
fn v2_to_v3_migration_failure_rolls_back_partial_ddl_and_preserves_history() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("migration-failure.db");
    {
        let s = Store::open(&path, &KEY).unwrap();
        s.put_message(&msg(1, "history survives a failed upgrade", 1))
            .unwrap();
        // Simulate v2 with a conflicting legacy index name. The v3 migration
        // creates its table and first index, then fails on its second index.
        s.conn
            .execute_batch(
                "DROP TABLE received;
             PRAGMA user_version = 2;
             CREATE INDEX received_grp ON messages(seq);",
            )
            .unwrap();
    }
    assert!(Store::open(&path, &KEY).is_err());
    {
        let c = Connection::open(&path).unwrap();
        let version: u32 = c
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 2);
        let partial: usize = c
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN ('received', 'received_seq')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(partial, 0, "all DDL from the failed migration rolled back");
        let history: usize = c
            .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
            .unwrap();
        assert_eq!(history, 1);
        c.execute_batch("DROP INDEX received_grp").unwrap();
    }
    let s = Store::open(&path, &KEY).unwrap();
    assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(
        s.message(&[1; 16]).unwrap().unwrap().text,
        "history survives a failed upgrade"
    );
    s.record_received(&PEER, &[1; 16]).unwrap();
    assert!(s.received_before(&PEER, &[1; 16]).unwrap());
}
