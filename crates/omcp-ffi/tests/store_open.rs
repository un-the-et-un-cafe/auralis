//! Native entry-point regressions for fail-closed persistent store opening.
use omcp_ffi::{generate_recovery_phrase, OmcpNode};

#[test]
fn invalid_identity_does_not_create_a_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("must-not-exist.db");
    assert!(OmcpNode::open(
        "not a recovery phrase".into(),
        "Test".into(),
        path.to_string_lossy().into(),
        vec![1; 32]
    )
    .is_err());
    assert!(!path.exists());
}

#[test]
fn other_identity_does_not_migrate_existing_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("owned.db");
    let phrase = generate_recovery_phrase().unwrap();
    let other = generate_recovery_phrase().unwrap();
    let key = vec![9; 32];
    drop(
        OmcpNode::open(
            phrase.clone(),
            "Owner".into(),
            path.to_string_lossy().into(),
            key.clone(),
        )
        .unwrap(),
    );
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("DROP TABLE received; PRAGMA user_version=2; PRAGMA journal_mode=WAL")
            .unwrap();
    }
    let before = std::fs::read(&path).unwrap();
    assert!(OmcpNode::open(
        other,
        "Other".into(),
        path.to_string_lossy().into(),
        key.clone()
    )
    .is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(OmcpNode::open(phrase, "Owner".into(), path.to_string_lossy().into(), key).is_ok());
}
