# OMCP Smartphone Apps — Build Requirements

Derived from PROTOCOL.md §13. Implementers (Proggy) treat this as the product checklist; Pipely owns CI, signing prep, and GitHub release automation.

## Platforms

- Android 12+ (API 31+), Kotlin UI, Rust core via JNI
- iOS 17+, SwiftUI, Rust core via FFI

## Must-have features (MVP)

1. Identity creation + 24-word recovery display (once)
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
