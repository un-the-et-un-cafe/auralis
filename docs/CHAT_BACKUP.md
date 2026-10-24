# Encrypted chat backup and restore indicator

Implements REQUIREMENTS "Encrypted chat backup" (CB1–CB5) and "Restore indicator (security)" /
"Re-verification after restore" (RS1–RS2). Wire formats: PROTOCOL.md §3.4, §7.1, §11.1.

## What is implemented

| Item | Where | Test |
|---|---|---|
| CB1 key = HKDF-SHA256(24-word seed, info `hopla/chat-backup/v1`); blobs padded to 64 KiB; provider app-private space | `omcp-core::chat_backup`, Android `ChatBackupSync` + `DriveChatBlobStore` (appDataFolder) | core `cb1_*`, ffi `cb1_cb4_*`, Android `ChatBackupBridgeTest` |
| CB2 wrong words cannot decrypt | `chat_backup::restore` | core/ffi `cb2_wrong_words_cannot_decrypt` |
| CB3 media restore as placeholders; re-fetch from partner by locator + hash; "no longer have it" → Ask to resend | ffi `request_missing_media`, controls 0x31/0x32/0x33 | ffi `cb3_placeholder_refills_*`, `cb3_refetch_with_wrong_bytes_is_rejected` |
| CB4 incremental uploads (digest log + weekly snapshot), hourly, Wi-Fi only by default | `chat_backup::plan`, `ChatBackupWorker` | core `cb4_*`, ffi `cb1_cb4_*` |
| CB5 optional media backup (off by default, daily, per type), turn-off deletes media blobs | `MediaSettings`, `seal_media`/`open_media` | ffi `cb5_media_backup_on_restores_hash_verified_off_uploads_nothing` |
| History moved out of the Argon2id identity backup (old blobs with `history` still read) | `backup.rs`, ffi seal uses `history: None` | ffi `identity_backup_no_longer_carries_history` |
| Burn content never backed up | `plan` skips burn rows | core `cb1_*` |
| RS1 new device key signed by identity key with `restored_at`, unsuppressible notice + badge, old key superseded | `omcp-core::device`, handshake payload v1 | core `rs1_*`, ffi `rs1_*`, Android `ChatBackupBridgeTest` |
| RS2 safety number over identity + current device key; verified cleared; old QR fails | `safety_number_v2`, QR `omcp:3:` | ffi `rs2_*`, Android `ChatBackupBridgeTest` |
| v0.1.4a device binding: device key signs the Noise handshake hash; missing/superseded cert or replayed old cert fails the handshake | `device::OwnDevice::handshake_payload`, `verify_handshake_payload` | core `device::*binding*`, ffi `c_missing_certificate_or_binding_fails_the_handshake`, `c_replayed_old_certificate_is_rejected_and_notice_still_shows` |
| v0.1.4a PeerID = BLAKE2b-256(identity key), per-device certified X25519, contacts keyed by PeerID, store/backup migration | `device::peer_id_from_identity`, `Store::rekey_peer`/`rebind_owner` | ffi `d_*`, store `v014a_rekey_legacy_peer_and_owner` |
| v0.1.4a iCloud chat backup: zone `HoplaBackupV1`, `ChatLogSegment`/`ChatSnapshot`/`MediaBlob`/`BackupManifest` | HoplaKit `ICloudChatRecords`, `RecordChatBlobStore`; app `CloudKitChatRecordBackend` | HoplaKit `ICloudChatRecordsTests` |

## Spec gaps (not decided here)

1. **Scope of "chats".** The store models 1:1 text, file metadata, timestamps and names. Reactions, edits (only captured as the changed row), groups and communities aren't modelled yet, so they aren't backed up.
2. ~~Device-key possession isn't bound to the session.~~ Resolved (v0.1.4a): the device key signs the handshake hash in every handshake.
3. ~~Identity signing key is TOFU.~~ Resolved for QR v3 contacts (the QR carries the identity key, checked against the PeerID). Legacy (v1/v2 QR) contacts still pin on first cert.
4. ~~PeerID.~~ Resolved (v0.1.4a): BLAKE2b-256 of the Ed25519 identity key; contacts keyed by PeerID; legacy X25519-keyed contacts migrate on the first verified handshake.
5. **Groups.** There are no groups in the app yet, so the group "Restored, not verified" tag (§5.99) isn't wired.
6. **Gateway rules for missing media.** "Wi-Fi only via gateway" and "Use mobile data for missing media" can't be enforced: there is no gateway transport yet. Re-fetch is over the direct BLE link only.
7. **Ask to resend.** The design shows an editable chat text; REQUIREMENTS describes a one-tap request. Implemented as both: control 0x33 plus the confirmed text. The partner-side prompt has no design (Android shows a callout).
8. **Inline media.** Non-burn inline `AppContent::Media` has no locator, so it can't be re-fetched.
9. ~~iCloud.~~ Resolved (v0.1.4a): CloudKit private DB, zone `HoplaBackupV1`; iOS chat backup is on. The CloudKit backend is only syntax-checked on Linux (no device run).
10. **Sent-file copies.** To answer a re-request for a file *you sent*, a local copy is needed. The apps don't keep one, so the answer is "unavailable".
11. **Timings.** The 5 s notice and 10 s re-fetch targets were only verified in loopback tests.
12. **Quota/estimate UI** (§5.96 near-full/full, size estimate, first-upload progress) is minimal.
13. ~~RS2 vs O3.~~ Confirmed (v0.1.4a): the restored device keeps *its own* verifications of others, and contacts clear their verification of it.
14. **Restored device dials first.** A restored device has a new X25519 key. Its contacts only have the old one, so they can't open a link to it until it dials them (the notice then shows) or they scan its new QR.
15. **Manifest index.** The iCloud `manifest` record's blob is a JSON index of core blob names (opaque seqs/keyed hashes) to record names; it isn't encrypted separately.
16. REQUIREMENTS clarifications (e) and (f) are on hold and weren't changed.
