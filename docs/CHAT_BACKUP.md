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
| RS2 safety number over identity + current device key; verified cleared; old QR fails | `safety_number_v2`, QR `omcp:2:` | ffi `rs2_*`, Android `ChatBackupBridgeTest` |

## Spec gaps (not decided here)

1. **Scope of "chats".** The store models 1:1 text, file metadata, timestamps and names. Reactions, edits (only captured as the changed row), groups and communities aren't modelled yet, so they aren't backed up.
2. **Device-key possession isn't bound to the session.** A modified client holding the 24 words could replay the old device cert and suppress the notice. Proposed fix: a post-handshake signature by the device key over the handshake hash.
3. **Identity signing key is TOFU.** It's pinned from the first cert seen; the QR v2 carries only the device key.
4. **PeerID.** PROTOCOL §3.1 says multihash of Ed25519; the implementation keys contacts by X25519.
5. **Groups.** There are no groups in the app yet, so the group "Restored, not verified" tag (§5.99) isn't wired.
6. **Gateway rules for missing media.** "Wi-Fi only via gateway" and "Use mobile data for missing media" can't be enforced: there is no gateway transport yet. Re-fetch is over the direct BLE link only.
7. **Ask to resend.** The design shows an editable chat text; REQUIREMENTS describes a one-tap request. Implemented as both: control 0x33 plus the confirmed text. The partner-side prompt has no design (Android shows a callout).
8. **Inline media.** Non-burn inline `AppContent::Media` has no locator, so it can't be re-fetched.
9. **iCloud.** Record names/zones aren't specified. iOS keeps the same blob names behind a protocol and isn't connected to CloudKit.
10. **Sent-file copies.** To answer a re-request for a file *you sent*, a local copy is needed. The apps don't keep one, so the answer is "unavailable".
11. **Timings.** The 5 s notice and 10 s re-fetch targets were only verified in loopback tests.
12. **Quota/estimate UI** (§5.96 near-full/full, size estimate, first-upload progress) is minimal.
13. **RS2 vs O3.** RS2 says "verified is cleared for all contacts". Following DESIGN_SPEC §5.98 and O3, the restored device keeps *its own* verifications of others, and contacts clear their verification of it.
