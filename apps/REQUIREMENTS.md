# OMCP Smartphone Apps — Build Requirements

Derived from PROTOCOL.md §13. Implementers (Proggy) treat this as the product checklist; Pipely owns CI, signing prep, and GitHub release automation.

## Platforms

- Android 12+ (API 31+), Kotlin UI, Rust core via JNI
- iOS 17+, SwiftUI, Rust core via FFI

## Must-have features (MVP)

1. Identity creation + 24-word recovery phrase, secured through the onboarding backup flow (see "Onboarding & recovery backup")
2. QR / NFC contact add with safety numbers
3. 1:1 E2EE chat over BLE mesh (Airplane Mode works)
4. Group chat via MLS (small groups first, ≤32 members in MVP)
5. File share ≤50 MB with chunked encrypted transfer
6. Gateway toggle with battery/privacy disclosure
7. Offline queue + path indicator (mesh / gateway / mixed)
8. Security centre: verify, reset session, block
9. Encrypted local backup / restore
10. Notification previews off by default

## Acceptance tests

See PROTOCOL.md §13.6. All five scenarios must pass before beta tag.

## Out of scope for MVP

- Desktop clients
- Tor transport
- Voice/video
- Public global directory

## Post-MVP (communities & livestreams)

See PROTOCOL.md §20.

- Communities with roles, announcement channel, topic channels
- Livestream via LAN relay or Internet SFU with E2EE media keys; BLE mesh shows status only
- Stories/ephemeral, polls, stickers, threads in v0.2
- Usernames and bots deferred to v0.3

## Onboarding & recovery backup (v0.1.1)

Goal: a new user is chatting in under 60 seconds, and nobody loses their identity because they skipped writing down 24 words.

### Flow
1. **Welcome**: one screen with the value proposition and a "Get started" button. No account, phone number or email needed.
2. **Profile**: display name (optional) and avatar (optional). The identity key and 24-word phrase are generated in the background.
3. **Secure your account**: the user picks one or more backup methods. At least one is required before the user can leave onboarding, but there's a "Remind me later" option that allows 24 h of use with a persistent banner.
   - **Google Drive** (Android default; also available on iOS)
   - **iCloud** (iOS default)
   - **Write it down** (shows the 24 words, then a quiz on 3 random words)
4. **Done**: show your QR code so people can add you. This also offers a nearby-contact add over BLE.

### Cloud backup requirements
- **Never upload the plaintext phrase.** The phrase is encrypted on the device before upload:
  - KDF: Argon2id (m=64 MiB, t=3, p=1, 16-byte random salt) over a **backup password** the user picks (min. 8 characters, with a zxcvbn score ≥ 3), *or*, on iCloud only, a 256-bit key held in iCloud Keychain (synced, end-to-end encrypted by Apple). The user can choose "No password" for iCloud.
  - Cipher: XChaCha20-Poly1305, with AAD = `"OMCP-Backup-v1" || suite_id || created_at`.
  - Blob format (CBOR): `{v:1, kdf, salt, nonce, ct, created_at, device_hint}`.
- **Google Drive**: the blob goes in the `appDataFolder` scope (hidden from the user's Drive and other apps). OAuth scope is `drive.appdata` only. Sign in with the Google account already on the device (Credential Manager).
- **iCloud**: the blob goes in the CloudKit **private** database (an `OMCPBackup` record). The key, when there's no password, lives in iCloud Keychain with `kSecAttrSynchronizable` and `kSecAttrAccessibleAfterFirstUnlock`.
- **Restore**: on a fresh install, the app detects an existing backup ("Welcome back, restore from Google Drive or iCloud?") and asks for the backup password if one was set. The 24-word phrase restore is always available as a fallback.
- **Management** (Settings > Backup): see the backup status and last-backup date, change the password, add or remove a backup method, delete the cloud backup, and view the phrase again (biometric or device PIN required).
- **Privacy**: no cloud backup is enabled without an explicit opt-in. No analytics on phrase content. Gateway or mesh traffic is never used for backups, only the platform's own cloud APIs over the Internet. If you're offline, the backup is queued and retried.
- **Honest copy**: the app tells the user plainly that anyone with access to their cloud account *and* the backup password can restore their identity, and that if they forget the password they need the written phrase.

### Acceptance tests
- O1: A fresh install reaches the chat list in ≤ 4 screens and ≤ 60 s for a user who picks a cloud backup.
- O2: The Drive and iCloud blobs never contain the phrase or the key in plaintext (verified by inspecting the uploaded bytes in a test).
- O3: Reinstall, then restore from Google Drive (Android) and from iCloud (iOS). The identity, contacts and verification state are restored.
- O4: A wrong backup password fails cleanly, with rate-limited retries. The phrase fallback still works.
- O5: Deleting the cloud backup in Settings removes the remote record (verified via API).
- O6: Skipping the backup shows a banner, and after 24 h the user can't continue without completing a backup method.
