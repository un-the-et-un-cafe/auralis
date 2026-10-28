# iOS v1 reliability tranche — 2026-10-28

These are implementation and local-test results, **not** a v1 release sign-off,
physical-device test result, or independent security certification. OMCP wire formats,
GATT identifiers, cryptographic algorithms, and the secure-capture implementation are unchanged.

## Confirmed defects addressed

- **Onboarding/cloud actor ownership:** the flow, repository, and SwiftUI mutations now stay
  on the main actor. Blocking provider operations receive only Sendable sealed blobs/method
  identifiers and construct their own provider on the worker. Results commit on the owner;
  failed uploads never count as successful backups. This replaces the background capture of
  non-Sendable `OnboardingFlow`; no `@unchecked Sendable`, `@preconcurrency`, or warning
  suppression was added. Local core/key preparation remains synchronous on the owner.
- **Recovery cancellation and failure:** leaving phrase restore clears entered secrets;
  unavailable/malformed recovery words cannot enter a quiz that indexes missing words.
  A late backup-discovery result cannot override a newer onboarding route. Discovery stops
  after a valid iCloud backup rather than unnecessarily proceeding to Google authorization.
- **Secret persistence:** Keychain replacement updates in place, preserving an existing item
  if an update fails. A concurrent insertion retries an update, never deletes the winner.
  Repository initialization verifies the storage-key write before opening the database;
  onboarding cannot advance after a failed phrase write and retries the same identity.
- **Backup deletion and deadlines:** a successful remote deletion removes its claimed backup
  method even if subsequent Keychain cleanup fails. The 24-hour gate updates while the app
  is idle and when the scene resumes, not only after message/contact changes. A failed local
  identity load no longer routes into the main app merely because preferences say onboarded.
- **Notification timing:** permission is requested after onboarding (or for an already
  onboarded launch), rather than interrupting first-launch onboarding. Backup failure copy
  no longer promises an automatic retry that does not exist.
- **Files:** picker data is read in bounded chunks and checked against the core's byte cap,
  including when a file provider grows/replaces a file after size confirmation. Unknown or
  stale metadata can no longer cause an unbounded `Data(contentsOf:)` allocation first.
- **BLE session failures:** failed ATT writes close the session and discard remaining
  ciphertext; late callbacks cannot resume/replay it. Rejected inbound session writes are
  not acknowledged as successful. Unsubscribing clears that responder's old queued
  ciphertext so a subsequent session does not inherit it.

**Still open:** the transport/core still marks messages flushed before successful transport
acknowledgement. Fail-closed session handling is not durable retry/ACK delivery semantics.
The coordinated FFI/native acknowledgement stage remains a release blocker; no guaranteed
message delivery or physical reconnection result is claimed here.

## Tests implemented

### Platform-independent package

`HoplaKit` now has **55 passing XCTest tests** (16 added in this tranche): eight onboarding/
backup regressions, two secret-persistence failure regressions, three bounded-file reader
cases, and three ordered-PDU queue failure cases. These use deterministic test doubles where
appropriate; they are not proofs of provider connectivity or real-device BLE behavior.

### Hosted iOS tests

The XcodeGen `OpenMeshChat` scheme now includes both:

- `OpenMeshChatTests`: existing real-Rust bridge tests plus seven `AppLifecycleTests`
  (Keychain replacement outcomes, actual Keychain insert/update/readback, idle/resume backup
  gate, failed identity-load routing, and native launch-screen metadata).
- `OpenMeshChatUITests/AuralisSmokeTests/testFreshOnboardingCancellationNavigationSettingsAndRelaunch`:
  actual app onboarding with **Remind me later**, restore-input cancellation/clearing,
  recovery-card cancellation, contact/tab/back navigation, backup/privacy/notification/
  storage screens, a persistent notification preference, Home/resume, and process relaunch.

The UI smoke requires a **fresh dedicated simulator** (erase that test simulator before the
scheme run). It intentionally uses the real default core, persistence and app routing; no
sample identity, fake app screen, launch-time data reset, or screen-security bypass exists.
Assertions inspect accessibility controls; screenshots are not used to circumvent secure
capture. The single ordered smoke retains its state to verify a genuine relaunch.

## Verification performed in this Linux workspace

- Official Swift 6.4.1: `swift test --package-path apps/ios/HoplaKit --jobs 2` — **55 passed**.
- `swiftc -parse` for every iOS Swift source, excluding generated/build directories — passed.
- `python3 apps/ios/scripts/strings.py --check` — EN/FR 287-key parity/freshness passed.
- `python3 -m unittest discover -s apps/ios/scripts -p 'test_*.py'` — **5 passed**.
- `git diff --check -- apps/ios` — passed.

Native XCTest and SwiftUI/UIKit/CoreBluetooth type checking cannot run on this Linux
executor. Hosted run [40696797061](https://github.com/un-the-et-un-cafe/auralis/actions/runs/40696797061)
subsequently built the native app and passed all 13 then-current lifecycle/real-Rust bridge
tests. The real-app UI smoke passed recovery cancellation/clearing and reached Profile, but
correctly stopped when creating the persisted identity failed. Decoded xcresult accessibility
and unified-log records establish the actual cause: UI-test app PID 23620 remained on Profile
with the identity error, and Security returned `-34018`, reporting that the client had neither
`application-identifier` nor `keychain-access-groups` entitlements. This was not an offscreen
selector, delayed transition, or reason to substitute in-memory persistence.

The simulator build had disabled code signing. The coordinated CI correction uses local
ad-hoc simulator signing and checks entitlements before execution; it does not provision a
team/certificate or change the unsigned device-release artifact. A new real-Keychain test uses
only a unique synthetic account and removes that item afterward. The UI smoke now explicitly
asserts arrival at Secure your account before looking for recovery controls.

The same artifact exposed a separate 320×480 legacy app viewport on the iPhone 16 simulator.
The raw Info.plist incorrectly contained `UILaunchScreen_Generation` (a generation setting),
not Apple's `UILaunchScreen` dictionary. That metadata is corrected and guarded by native
metadata and UI-geometry assertions; screen-security code remains unchanged.

**The signing correction, two new native tests, launch metadata correction, and the remainder
of the UI journey still need a new hosted run.** No full UI pass is claimed from the failed run.
Physical iPhone/Android interop, background BLE, secure capture behavior, device Keychain
entitlements, Face ID/passcode, and real iCloud/Google account operations still require
provisioned-device acceptance.
