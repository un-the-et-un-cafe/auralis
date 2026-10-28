# Auralis iOS implementation boundary

## Newly authored application experience

The native shell now organizes the app around **Conversations / Mesh / You**. Auralis’s reusable
SwiftUI card, typography, action, avatar, recovery-word and brand components are used throughout
the inbox, chat composer and viewer, onboarding, recovery, contact add, verification, backup,
notification, privacy, storage and gateway screen presentations. This is a real SwiftUI client,
not a static mockup or a simulated transport.

The new Mesh workspace reads `model.core.pathStatus()`. The inbox reads repository contacts and
messages; name search does not index burn content. The burn control, file actions, QR actions,
settings bindings and backup methods call the existing implementation. No fabricated peer,
traffic, delivery, backup or verification state was added. The future Communities and Live
surfaces are reachable from Mesh and remain clearly unavailable.

The original working screen callbacks and some message/restore subviews are intentionally
retained and restyled. “Application rewrite” does **not** mean that every existing line, all
platform adapters, or the cryptographic implementation was replaced.

## Deliberately retained implementation

- `HoplaKit`: repository, models, onboarding and backup state machines, transport policy,
  storage/restore logic, burn policy, cloud record schemas and retry limits.
- `RustOmcpCore`, the OMCP FFI interface, BLE transport/GATT identifiers, advertisement bytes,
  packet formats, QR payloads and cryptographic labels.
- Device Keychain storage rules, internal account/service names, the encrypted local database,
  file verification/saving and screen-capture protection. No cryptographic changes.
- Real CloudKit/Google Drive integrations, biometric checks, notification privacy rules,
  camera scanning and QR generation. Only the app-specific CloudKit container selection changes.
- Xcode project/scheme/target paths and the internal `HoplaKit` package name.
- The strict legacy APNs fallback payload (`Hopla` / `New message`). Local and extension-rendered
  notification titles use Auralis; raw fallback delivery may still show the legacy generic title.

`NotificationPolicy.appName` is the only platform-independent behavior changed: presentation
branding. The notification and burn test expectations are updated, and a regression test asserts
that Auralis presentation still accepts the legacy generic APNs payload.

## Distinct installation identity

| Surface | Auralis identifier |
| --- | --- |
| App | `chat.auralis.app` |
| Notification extension | `chat.auralis.app.notifications` |
| CloudKit container | `iCloud.chat.auralis.app` |
| Keychain access group | `$(AppIdentifierPrefix)chat.auralis.app` |

These allow a separate install alongside Hopla. Provisioning/signing, APNs and CloudKit setup
must be completed in the intended Apple account; no external services were provisioned here.
The Google iOS OAuth client must also be associated with the Auralis bundle ID. Keeping internal record names and
crypto labels does not grant access to Hopla’s app sandbox, Keychain access group or private
cloud container. There is no automatic cross-app migration.

## Interaction fixes included

- Returning from a chat/preview to the scanner restarts the capture session on a serial queue;
  a late camera permission response does not start the camera after leaving the screen.
- “Choose another” waits for file-confirmation dismissal before presenting the picker again;
  file confirmation also has an explicit Cancel action.
- New inbound messages and burn expiry no longer force a reader away from older history.
- Revealed backup words clear when leaving or deactivating the screen. A pending biometric
  result is invalidated on disappearance/background. Backup work rejects repeated activation
  and disables conflicting controls while running.
- First-launch Bluetooth guidance is not presented over the required-backup gate.
- Safety-number grids become single-column at accessibility text sizes; primary actions grow
  with text, common settings actions have 44-point minimum targets, and tertiary light text
  reaches at least 4.91:1 contrast on the three base surfaces.

The old store-and-forward toggle was view-local state with no backend effect. It is now an
explicit unavailable note, not a falsely working control. Other pre-existing beta limitations
remain documented in [README.md](README.md).

## Verification in the Linux implementation workspace

- **Passed:** EN/FR generation and freshness/format validation: 287 keys.
- **Passed:** 5 localization regression tests, including stale/missing resources, placeholder
  type mismatch and escaped headline line breaks.
- **Passed:** `bash -n` for the Rust build and Linux verification scripts; `git diff --check`.
- **Passed:** plist and asset-catalog parsing; launcher is opaque RGB, 1024×1024; app display
  name and unchanged Bluetooth background modes verified. Launcher pixels visually inspected.
- **Passed:** the full `scripts/verify-linux.sh` workflow using an official Swift 6.4.1 Linux
  toolchain: 39 HoplaKit tests, 8 real Rust FFI bridge tests, 5 localization tests and syntax
  parsing of all 51 Swift files. The initial missing-toolchain blocker was resolved during the
  task. A pre-existing date-dependent iCloud record-store test now injects the same fixed clock
  into both stores; production backup logic was not changed for that test fix.
- **Not established:** SwiftUI/UIKit type-checking, Xcode build, simulator launch, signing,
  physical-device behavior or cloud-service roundtrips. Linux syntax parsing is not an iOS build.
- All Cargo calls in the native scripts now use `--locked`.

## Required macOS/device acceptance before release

Run the canonical Xcode build/test commands in [README.md](README.md), then verify:

1. Fresh onboarding, interruption/resume, optional password, phrase quiz and all restore errors;
   required-backup gating, repeated actions, deletion confirmation and late biometric completion.
2. QR permission denial, preview dismissal, mismatch refusal, verified/unverified add, chat → Back
   → scan again, and QR brightness restoration.
3. Real Auralis ↔ Hopla/OMCP messaging over BLE, offline queue, reconnect, file-too-large → choose
   another, file cancel/open, delivery states, burn open/close/countdown and notification privacy.
4. Existing chat scroll position after inbound traffic and expiry; keyboard and multiline composer
   layout; both appearances, EN/FR, smallest supported phone and accessibility text sizes.
5. Backup service provisioning and true cloud roundtrips; secure-capture/app-switcher behavior;
   separate installation beside Hopla with no unintended data sharing.

Native CI and physical-device results must be recorded separately. Source review and static
resource validation are not substitutes for these checks.
