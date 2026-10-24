# Hopla — OMCP Android beta (M5)

**Experimental · not production-ready · untested on real hardware.**
Consumer name: **Hopla**. Protocol / crates stay **OMCP**. Package id `chat.omcp.app`.

minSdk 31 (Android 12+) · Kotlin · Jetpack Compose · Material 3 · Rust core via UniFFI.

## Architecture

```
Compose UI ── ViewModels ── MeshRepository ── OmcpCore (interface)
                                   │              ├─ RustOmcpCore  → chat.omcp.ffi (UniFFI) → libomcp_ffi.so → omcp-core
                                   │              └─ FakeOmcpCore  (previews / JVM tests, no crypto)
                                   └─ BleMeshTransport (GATT server+client) in MeshService (foreground, connectedDevice)
```

- **`crates/omcp-ffi`** — safe UniFFI API over `omcp-core`: identity from 24-word phrase, QR contact
  add/preview with safety numbers, offline queue, per-connection `MeshLink` (hybrid Noise IK + ML-KEM
  handshake, M2 link-AEAD fragments), path status, gateway policy, 50 MB file cap, encrypted backup.
  `omcp-core` keeps `#![forbid(unsafe_code)]`; the only unsafe is UniFFI's generated scaffolding.
- **BLE** (PROTOCOL §8.1): every phone advertises the service and hosts `Inbox` (write) / `Outbox`
  (notify) / `Control` (read → 13-byte discovery ad). The lower PeerID dials (central = Noise
  initiator). The §8.1 placeholder UUID is not valid hex, so the beta uses
  `4f4d4350-000{1..4}-4c01-9e00-000000000001` (`OmcpGatt`). Discovery ad rides in the scan response
  as manufacturer data (id 0xFFFF, testing). PDU framing (`0x01` handshake chunks, `0x02` link
  fragments, ≤ 512 B) is documented in `crates/omcp-ffi/src/lib.rs`.
- Mutual QR add is required: a responder refuses initiators that aren't contacts.

## Onboarding & backup (apps/REQUIREMENTS.md “Onboarding & recovery backup (v0.1.1)”)

Welcome → Profile → **Secure your account** (Google Drive and/or Write it down + 3-word quiz, or
“Remind me later” = 24 h with banner, then blocked) → Done (own QR + “Find people nearby”).
Restore-on-reinstall prompt when a cloud backup is found; 24-word phrase restore always available.
Settings › Backup: status/last date, change password (re-upload), delete cloud backup, view phrase
behind BiometricPrompt / device credential (FLAG_SECURE while words are shown).

Backup blob = Rust core (`omcp_core::backup`): Argon2id (64 MiB, t=3, p=1) + XChaCha20-Poly1305,
AAD `OMCP-Backup-v1‖suite‖created_at`, CBOR `{v,kdf,salt,nonce,ct,created_at,device_hint}`; plaintext
contains entropy + contacts + verification state. Password: ≥ 8 chars, zxcvbn ≥ 3.

**Google Drive is a stub** (`DriveAppDataBackupStore.available = false`): no OAuth client id ships in
this repo. Debug builds use an in-memory “simulated Drive” (`BuildConfig.SIMULATED_DRIVE_BACKUP`) so
the flow can be exercised; it does not survive reinstall. TODO: Credential Manager + `drive.appdata`
REST (see class KDoc), WorkManager retry when offline.

Onboarding screens are plain layouts (`ui/components/Plain.kt`) pending design v0.1.2. Contact-add,
QR and file-confirm follow DESIGN_SPEC §5.19–§5.24.

### Acceptance tests (Android)

| Test | Status |
|------|--------|
| O1 ≤ 4 screens to chats with cloud backup | JVM test (`OnboardingViewModelTest`); timing ≤ 60 s not measured on device |
| O2 blob has no plaintext phrase/key | Rust tests (`backup::tests`, `omcp-ffi`) + Kotlin bridge test |
| O3 reinstall → restore from Drive | Logic tested with in-memory store; **real Drive deferred** (OAuth) |
| O4 wrong password fails, rate-limited, phrase fallback | Rust + JVM tests (`RetryLimiter`) |
| O5 delete removes remote record | JVM test against in-memory store; real Drive API check deferred |
| O6 skip → banner, blocked after 24 h | JVM test |
| iCloud parts of O2/O3, platform-key mode UI | Deferred to iOS (M6); core supports `platform-key` KDF |

## What works vs. stubbed

| Area | Status |
|------|--------|
| Identity, phrase, encrypted backup, restore | Real (Rust) |
| QR show (ZXing) / scan (zxing-android-embedded), preview + safety number, verify / chat without verifying, mismatch block | Real |
| 1:1 text over BLE GATT, offline queue flush on connect | Real code path; verified Kotlin↔Rust in JVM test with simulated PDUs, **not on phones** |
| Notifications | Previews off by default (“New Hopla message”) |
| Gateway settings | Policy from core (500 MB/day, Wi-Fi only, metered warns); no real gateway transport |
| File send | Picker + 50 MB check (core constant) + queued bubble; **chunk transfer over BLE TODO** |
| Groups (MLS), NFC, session reset / block, Room persistence of messages | TODO |
| Communities / Live | “Coming in v0.2” empty states |

## Build

Needs JDK 17, Android SDK 35, NDK 27, Rust + `cargo-ndk`, targets `aarch64-linux-android x86_64-linux-android`.

```bash
cd apps/android
export ANDROID_HOME=~/android-sdk ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.2.12479018
./scripts/build-rust.sh           # .so + Kotlin bindings into app/build/rust (git-ignored)
./gradlew assembleDebug testDebugUnitTest
```

`RustBridgeTest` loads the host `target/debug/libomcp_ffi.so` through JNA (skipped if absent).
App icon is a **placeholder** (redesign pending).
