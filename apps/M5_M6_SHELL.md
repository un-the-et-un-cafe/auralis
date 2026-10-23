# M5/M6 app shells — screens & Designy notes

Branch: `feat/m5-m6-app-shells`  
Design pack: `design/` (mirror) ← Designy `/home/box/shared/omcp-design/`

## Screens implemented

| Screen | Android | iOS | Notes |
|--------|---------|-----|-------|
| Tab root (4 tabs) | ✓ | ✓ | Chats, Communities, Live, You |
| Chats list | ✓ | ✓ | Sample rows + path chips |
| Chat thread | ✓ | ✓ | Path banner + composer shell |
| Security centre | ✓ | ✓ | Safety number placeholder grid |
| Gateway settings | ✓ | ✓ | 500 MB/day, Wi‑Fi default, metered **warn** |
| Communities home | ✓ empty | ✓ empty | “Coming in v0.2” (Jakob) |
| Live tab | ✓ empty | ✓ empty | “Coming in v0.2” + BLE-only copy |
| You hub | ✓ | ✓ | Links to Gateway / Security |
| PathChip / PathBanner | ✓ | ✓ | Designy colours |
| EN + FR strings | ✓ | ✓ | Parity for shell copy |
| omcp-core stub | JNI stub | FFI stub | `CORE_LINKED = false` |

## Deferred (not in this PR)

- Onboarding / 24-word recovery
- QR / NFC contact add
- Real BLE / MLS / gateway transport (M2–M4)
- File transfer UI wired to chunks
- Encrypted backup
- Gradle wrapper JAR / committed `xcodeproj` (generate locally)
- Enabling CI android/ios jobs (`if: false` retained)

## Designy mismatches / questions to forward

1. **Resolved by Jakob (baked in):** Q1 show Communities+Live empty; Q2 500 MB + Wi‑Fi default + metered warn; Q3 Android companion recommended on Gateway/iOS copy.
2. **Mockup vs product:** Gateway mockup shows “200 MB used of 500 MB”; shell starts at **0 used** until metering exists — confirm OK for beta.
3. **Chats sample data:** Shell ships Designy-like placeholder conversations so navigation is reviewable; empty-state copy still present in strings for when samples are removed.
4. **You tab:** Designy IA lists Profile / QR of self — shell has PeerID stub only; QR of self deferred with contact-add.
5. **iOS background banner on Chats:** Designy §8.2 suggests a Chats banner (“iOS may pause mesh…”). Shell currently puts companion/BLE guidance on **Gateway**; consider adding a calm Chats status strip on iOS only in a follow-up.
6. **Dark mode:** Tokens mapped; not screenshot-tested (no device in this environment).
7. **HTML mockups** committed for reference; **PNGs omitted** (see `design/SOURCE.md`).

## Build status (2026-10-23, Linux box)

- **Android:** No JDK/SDK on agent box — not assembled here. Open `apps/android` in Android Studio or enable Pipely `android` job.
- **iOS:** Structure + `project.yml` only; needs macOS `xcodegen` + Xcode.
