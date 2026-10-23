# OMCP reference apps (M5 / M6)

| Path | Platform | Milestone |
|------|----------|-----------|
| [`android/`](./android/) | Android 12+ (API 31), Kotlin Compose | M5 |
| [`ios/`](./ios/) | iOS 17+, SwiftUI | M6 |
| [`REQUIREMENTS.md`](./REQUIREMENTS.md) | Product checklist (PROTOCOL §13) | — |

**Design source of truth:** [`../design/`](../design/) (lightweight mirror of Designy’s pack). Full pack + PNGs: `/home/box/shared/omcp-design/` and `/workspace/meshchat/design/`.

### Implemented vs deferred (this PR)

**Implemented (UI shells):** Chats list (empty-state default), chat thread, Contact-add + My QR, security centre, gateway settings (0/500 MB), Communities empty (v0.2), Live empty (v0.2), You hub, path chips/banners, EN/FR strings, omcp-core stubs.

**Deferred:** Onboarding / recovery phrase, real QR camera / NFC wiring, You→Profile/QR, real BLE mesh, MLS, file transfer, JNI/FFI to `crates/omcp-core`, encrypted backup, store listings.

M2–M4 must land before path indicators and gateway toggles drive real transport state.
