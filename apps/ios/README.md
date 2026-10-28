# Auralis for iOS — independent OMCP client

**iOS 17+** · SwiftUI · Rust core (`crates/omcp-ffi`) through UniFFI Swift bindings.
Auralis is a newly authored native application experience built on the MIT-licensed Hopla /
OpenMeshChat implementation. It keeps the interoperable **OMCP** core and transport, rather than
forking the cryptography. Linux core/bridge tests and Swift syntax checks pass; this remains a native development build awaiting Xcode and device validation, not a release.

## The Auralis experience

- **Conversations**: a people-first inbox with contact search, true path/queue status, QR entry,
  redesigned message/file cards, and a visible burn-after-read composer control.
- **Mesh**: a dedicated workspace showing the core’s actual nearby-peer and queued-message
  counts, connection guidance and gateway controls. Communities and Live remain explicitly
  unavailable previews here, not misleading active tabs.
- **You**: an identity card and personal QR, with newly laid-out backup/recovery, privacy,
  notification, storage and gateway settings.
- A four-stage onboarding experience, honest restore/unavailable/error states, accessible
  recovery-word cards, and redesigned safety-number/mismatch flows.
- Ink / mint visual system, system-responsive light and dark appearance, Dynamic Type, native
  controls, localized EN/FR copy and the shared three-stroke Auralis launcher mark.

See [AURALIS_REWRITE.md](AURALIS_REWRITE.md) for exact retained boundaries, verification results,
provisioning requirements and the native acceptance checklist. This is not a claim that every
line was rewritten: tested state machines, callbacks, core adapters and security machinery are
intentionally retained.

## Layout

| Path | What |
|------|------|
| `HoplaKit/` | SwiftPM package, **platform-independent**: models, `OmcpCore` protocol + `FakeOmcpCore`, `MeshRepository`, `OnboardingFlow`, `BackupSettings`, `NotificationPolicy`, `PushPayload`, `ScreenCapturePolicy`, `OmcpGatt`, file/gateway policy, `RetryLimiter`. Builds and tests on Linux. |
| `OpenMeshChat/` | SwiftUI app target (display name **Auralis**): screens, `RustOmcpCore` (UniFFI adapter), CoreBluetooth transport, CloudKit/Keychain/Drive backup stores, notifications, secure-capture layer. |
| `NotificationService/` | Notification Service Extension for content-free APNs pushes (stub). |
| `OpenMeshChatTests/` | Real-Rust `RustBridgeTests` plus `AppLifecycleTests` for Keychain update failures, backup deadlines and failed local restore routing. |
| `OpenMeshChatUITests/` | Real-app onboarding/cancellation/navigation/settings/relaunch smoke on a fresh simulator; secure capture stays enabled. |
| `Generated/`, `build/` | git-ignored: `OmcpFFI.swift` bindings and `OmcpFFI.xcframework`. |
| `scripts/build-rust.sh` | macOS: build `aarch64-apple-ios` + `aarch64-apple-ios-sim` static libs, generate bindings, make the xcframework. |
| `scripts/verify-linux.sh` | Linux: HoplaKit tests, then HoplaKit + generated bindings + `RustOmcpCore.swift` + `RustBridgeTests` against the host `libomcp_ffi.so`, then `swiftc -parse` of every Swift file. |
| `scripts/strings.py` | Source of truth for EN/FR strings; `--check` verifies keys, format arguments and generated-resource freshness; `test_strings.py` regression-tests the generator. |

## App identity and provisioning

The app bundle ID is `chat.auralis.app`, its notification extension is
`chat.auralis.app.notifications`, and the CloudKit container is `iCloud.chat.auralis.app`.
This permits Auralis and Hopla to coexist. Register and provision these identifiers with your
own Apple team before a device build; no signing account or cloud container was provisioned by
this work. Configure an Auralis-specific Google iOS OAuth client associated with `chat.auralis.app` before using Google Drive backup.

The Keychain **access group** is Auralis-specific. Internal account/service strings, CloudKit
record/zone schemas, database names and cryptographic labels remain compatible, but iOS app
sandboxes are separate: Auralis cannot silently read Hopla’s local data or private cloud container.
Do not assume automatic migration. The OMCP wire identity is separate from these app identifiers.
Internal Xcode project/scheme names and `HoplaKit` remain unchanged to preserve build interfaces.

## Build (macOS)

```bash
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
brew install xcodegen
cd apps/ios
./scripts/build-rust.sh
xcodegen generate
xcodebuild -project OpenMeshChat.xcodeproj -scheme OpenMeshChat \
  -destination 'platform=iOS Simulator,name=iPhone 15' CODE_SIGNING_ALLOWED=NO build test
(cd HoplaKit && swift test)
```

The Rust scripts use `--locked` with the committed root `Cargo.lock`.

The `ios` job in `.github/workflows/ci.yml` runs both native XCTest targets and the real-app
UI smoke on a fresh simulator. Use a dedicated erased simulator for local UI smoke runs:
its ordered journey deliberately retains state to check process relaunch. See
[V1_RELIABILITY.md](V1_RELIABILITY.md) for the exact changes, test scope and validation limits.

## Features

- **Onboarding** (Auralis welcome, progress and recovery cards): Welcome → Profile →
  Secure your account → Done (O1: 4 screens). Secure: **iCloud** (CloudKit private DB record
  `OMCPBackup`; default "No password" mode seals the blob with a 256-bit key from
  `generate_backup_key()` kept in iCloud Keychain, `kSecAttrSynchronizable` +
  `kSecAttrAccessibleAfterFirstUnlock`; optional backup password → Argon2id), **Google Drive**
  (appDataFolder, password only), **Write it down** (24 words + quiz on 3 random words),
  **Remind me later** (24 h banner, then a blocking backup screen). Sealing and opening are in
  Rust (`create_backup_with_key` / `open_backup_with_key` / password variants).
- **Restore on reinstall**: detects an iCloud/Drive backup ("Welcome back"), asks for the password
  if set (rate-limited: 3 tries, then 30 s doubling to 15 min), 24-word fallback always available.
  Settings › Backup and restore: status, last backup, back up again / change password, delete the
  cloud backup (+ Keychain key), view the phrase behind Face ID / passcode.
- **Contact add** (design v0.1.1 §5.19–5.22): scan QR (AVFoundation, non-OMCP codes rejected and
  never opened), My QR (CoreImage, ECC H, brightness raised), preview sheet with the 12×5 safety
  number, **Verify & chat** / **Chat without verifying**; mismatch has no chat action. Verified
  seal only on verified contacts; unverified shows nothing.
- **BLE** (`Mesh/BleMeshTransport.swift`): same GATT as Android (`OmcpGatt` = Android
  `OmcpGatt`, cross-checked by a unit test): service `4f4d4350-0001-4c01-9e00-000000000001`,
  Inbox `…0002…` (write), Outbox `…0003…` (notify), Control `…0004…` (read: 13-byte
  advertisement), lower PeerID dials.
- **Notifications**: default ON with sender + preview; global and per-chat Private (per-chat
  override both ways); private calls show "Auralis · Incoming call"; Hide on lock screen uses a
  category whose `hiddenPreviewsBodyPlaceholder` is "New message" and hides the title. Burn
  messages never show content. APNs payloads are content-free (`PushPayload`: `mb` mailbox id +
  fixed generic alert, `mutable-content`).
- **Burn after read** entirely through the Rust core (queue with `burn` header, `open_burn` once,
  `close_burn`, `tick_burn` every 250 ms, OPENED receipts). View-once media: "🔥 Photo opened";
  no copy/share/quote; excluded from notifications and search.
- **No screenshots**: the whole window is re-parented into a secure `UITextField` canvas layer
  (blank in screenshots, recordings, mirroring), a blurred app-switcher cover when not active, and
  a cover while `UIScreen.isCaptured`. No screenshot notice. Settings › Privacy has the honest
  "second camera" copy.
- **Files**: picker confirm with the 50 MB cap read from `max_file_bytes()` (too-large state before
  the file is read or encrypted). Rust encrypts 64 KiB chunks (`queue_file`); the CoreBluetooth
  transport drains them on write/notify (`more_pending`), pauses on disconnect and resumes on the
  next link; received files are BLAKE3-verified in Rust, saved in Application Support and opened
  with Quick Look. Bubble states follow Designy v0.1.1 `file-transfer`.
- **Gateway**: cap from `default_gateway_daily_cap_bytes()`, Wi-Fi-only/metered decision from
  `gateway_decision()`; meter stays at 0 until real metering.
- EN/FR strings and a 1024×1024 opaque RGB Auralis launcher icon generated from
  `design/brand/auralis/icon.svg`.

## Background BLE on iOS (notes)

- `UIBackgroundModes`: `bluetooth-central`, `bluetooth-peripheral`, `remote-notification`.
- Both managers use state-restoration identifiers (`omcp.central`, `omcp.peripheral`) so iOS can
  relaunch Auralis for BLE events.
- In the background iOS strips the local name, moves service UUIDs to the overflow area, coalesces
  scan results and ignores `AllowDuplicates`; scans must name the service UUID (they do).
  Two backgrounded iPhones discover each other poorly; foreground or an Android companion nearby
  works best (copy on You › Gateway). Nearby sessions will use a Live Activity (v0.3).
- iOS peripherals can't advertise manufacturer data, so iOS serves the 13-byte discovery
  advertisement on the **Control** characteristic and advertises the local name `OMCP`. Android
  scans for the service UUID, the local name *and* Apple's overflow-area manufacturer data (what a
  backgrounded iPhone sends), then connects, discovers services and reads Control
  (`ScanClassifier.kt`). iPhone → Android works (iOS reads Control).

## Stubs / not done

- Google Drive on iOS: implemented (`HoplaKit/GoogleDrive.swift`: PKCE OAuth + Drive v3
  `appDataFolder`, tested on Linux against fakes; app side `ASWebAuthenticationSession`, device
  Keychain refresh token). Needs an iOS OAuth client in `Config/Secrets.xcconfig` (git-ignored),
  see [`../GOOGLE_DRIVE_BACKUP.md`](../GOOGLE_DRIVE_BACKUP.md); unavailable without it. Not run
  against real Google.
- Notification Service Extension: validates the payload and shows "Auralis · New message"; mailbox
  fetch + decrypt waits for the M4 gateway mailbox API and an App Group keychain.
- APNs token is stored locally only (no gateway registration yet).
- Calls don't exist yet; `NotificationPolicy.callContent` is the policy the call stack will use.
- NFC, PeerID entry, Nearby: not in this beta. File transfer has no end-to-end ack yet (sender
  shows "Sent" once the last chunk is on the link).
- The core’s encrypted local store reloads contacts and message history; the Swift repository
  keeps its current UI projection in memory. No storage format was changed by this rewrite.
- The strict, legacy APNs wire fallback still uses the literal title `Hopla`. The extension and
  local notifications display `Auralis`; if the extension cannot run, the legacy generic title
  can appear. Changing that wire contract requires coordinated protocol work.
- The former store-and-forward toggle only changed local view state; it is replaced by an
  explicit unavailable note. Gateway metering is still zero until the underlying implementation
  supplies real traffic measurements.
