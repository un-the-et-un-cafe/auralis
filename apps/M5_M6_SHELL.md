# M5/M6 app shells — screens & Designy notes

Branch: `feat/m5-m6-app-shells`  
Design pack: `design/` (mirror) ← Designy `/home/box/shared/omcp-design/`

## Screens implemented

| Screen | Android | iOS | Notes |
|--------|---------|-----|-------|
| Tab root (4 tabs) | ✓ | ✓ | Chats, Communities, Live, You |
| Chats list | ✓ | ✓ | **Empty-state default**; path chips |
| Chat thread | ✓ | ✓ | Path banner (mesh/gateway/mixed/queued only) + composer shell |
| Contact add | ✓ | ✓ | Chooser: Scan QR · **My QR** · NFC · PeerID (shells) |
| My QR | ✓ | ✓ | Minimal stub under Contact-add (You → Profile/QR deferred) |
| Security centre | ✓ | ✓ | Safety number placeholder grid |
| Gateway settings | ✓ | ✓ | **0 of 500 MB**, Wi‑Fi default, metered **warn**; iOS BLE + companion callout |
| Communities home | ✓ empty | ✓ empty | “Coming in v0.2” (product decision) |
| Live tab | ✓ empty | ✓ empty | “Coming in v0.2” + BLE-only copy |
| You hub | ✓ | ✓ | Links to Gateway / Security |
| PathChip / PathBanner | ✓ | ✓ | Designy colours |
| EN + FR strings | ✓ | ✓ | Parity for shell copy |
| omcp-core stub | JNI stub | FFI stub | `CORE_LINKED = false` |

## Designy lock-ins (normative for this PR)

1. **Gateway cap meter:** always show **0 of 500 MB** used until real metering exists. Never ship a fake “200 of 500” fill from the mockup.
2. **Sample chats:** removed from release/default UI. Beta shows **empty-state only** — “Add someone nearby with QR or NFC.” Sample rows remain only behind a clearly named flag, **off by default**:
   - Android: `BuildConfig.SHOW_SAMPLE_CHATS_FOR_DESIGN_REVIEW` (`app/build.gradle.kts`) via `DesignReviewFlags`
   - iOS: `DesignReviewFlags.showSampleChatsForDesignReview` (`Core/DesignReviewFlags.swift`)
3. **iOS BLE pause callout:** on **You → Gateway**, next to the Android companion recommendation. **Not** on the Chats/thread path banner (that stays mesh/gateway/mixed/queued only). Optional dismissible tip on first iOS launch (`omcp.iosBleTipDismissed`); after that, Gateway owns it.
4. **You → self QR:** deferred; **Contact-add → My QR** is in this PR so self-QR is reachable without You → Profile/QR.

## Deferred (not in this PR)

- Onboarding / 24-word recovery
- Real camera QR / NFC / PeerID wiring (chooser shells only)
- You → Profile / QR of self (My QR covered via Contact-add)
- Real BLE / MLS / gateway transport (M2–M4)
- File transfer UI wired to chunks
- Encrypted backup
- Gradle wrapper JAR / committed `xcodeproj` (generate locally)
- Enabling CI android/ios jobs (`if: false` retained)

## Earlier product pins (still baked in)

- Q1: show Communities + Live empty (v0.2 copy), not hidden
- Q2: 500 MB/day + Wi‑Fi-only default + metered warn (not hard-block)
- Q3: Android companion recommended on Gateway / iOS copy

## Still open / follow-up (not blocking Designy lock-ins)

- Dark mode: tokens mapped; not screenshot-tested (no device in this environment)
- HTML mockups committed for reference; **PNGs omitted** (see `design/SOURCE.md`)
- Real metering fill on Gateway when M4 gateway transport lands

## Build status (2026-10-23, Linux box)

- **Android:** No JDK/SDK on agent box — not assembled here. Open `apps/android` in Android Studio or enable Pipely `android` job.
- **iOS:** Structure + `project.yml` only; needs macOS `xcodegen` + Xcode.

## M5 Android beta update (2026-10-24)

See `apps/android/README.md`. `OmcpCoreStub` is replaced by the `OmcpCore` interface (real
`RustOmcpCore` via `crates/omcp-ffi`, `FakeOmcpCore` for previews/tests). Display name is now
**Hopla**; icon is a placeholder. Onboarding follows REQUIREMENTS “Onboarding & recovery backup
(v0.1.1)” with plain layouts (the v0.1.1 recovery/confirm/restore mockups are superseded by that
flow). Android CI job is enabled.

## M6 iOS beta update (2026-10-24)

See `apps/ios/README.md`. Same scope as the Android M5 beta: `OmcpCoreStub` is replaced by the
`OmcpCore` protocol in the HoplaKit SwiftPM package (`RustOmcpCore` via the omcp-ffi UniFFI
Swift bindings, `FakeOmcpCore` for previews/tests). Display name is **Hopla**, placeholder icon,
plain onboarding layouts. iOS CI job is enabled on `macos-latest`.
