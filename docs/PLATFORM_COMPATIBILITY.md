# Build-only platform compatibility migration (2026-10-29)

Baseline: public main `5e993c0f2e5f2f5e3404955e976f5713f0171402`.
This is **not a release, signing change, store declaration or v1 readiness claim**.
Crypto, protocol/wire sources, storage implementation and production iOS
`SecureCapture.swift` are unchanged. Unsupported iOS secure-canvas reliance is a
separate owner/product/security decision; this migration neither removes it nor
claims universal screenshot prevention.

## Reviewed pins and primary evidence

| Component | Pin / purpose | Primary source |
| --- | --- | --- |
| Android compile/target | API 36; minimum remains 31 | [Play target requirements](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en) |
| AGP / Gradle / JDK | 8.10.1 / 8.11.1 / 17 | [AGP 8.10 compatibility](https://developer.android.com/build/releases/agp-8-10-0-release-notes) |
| Kotlin + Compose compiler plugin | both 2.2.21; Compose BOM unchanged | [Kotlin compatibility table](https://kotlinlang.org/docs/gradle-configure-project.html) |
| NDK / cargo-ndk | retain 27.2.12479018 / 4.1.2 | [16-KB linker requirements](https://developer.android.com/guide/practices/page-sizes) |
| JNA | AAR and host test JAR both 5.18.1 | [JNA release notes](https://github.com/java-native-access/jna/blob/5.18.1/CHANGES.md) |
| Xcode / SDK | 26.3 build 17C529 / iOS 26.2 | [Actual runner image inventory](https://github.com/actions/runner-images/blob/macos-15-arm64/20260907.0337/images/macos/macos-15-arm64-Readme.md), [Apple SDK requirement](https://developer.apple.com/news/upcoming-requirements/) |
| Apple simulator | iPhone 16, iOS 18.5 **and** 26.2; minimum deployment remains 17.0 | Same image inventory; **iOS 17 runtime testing remains missing** |
| Bundletool | 1.18.2 | [Official release](https://github.com/google/bundletool/releases/tag/1.18.2) |

AGP 8.10 supports API 36, Gradle 8.11.1 and JDK 17. Kotlin 2.2.21's supported
AGP/Gradle ranges include those pins. This is not a blanket dependency update.
Apple's Xcode 26+/iOS 26 SDK requirement has applied since April 28, 2026.
The current canonical Android guide gives **February 1, 2027** as the update
blocking date for missing 16-KB support; do not reuse the older 2025 deadline.

The actual earlier hosted iOS job on 2026-10-29 reported image
`macos-15-arm64/20260907.0337.1`; its linked inventory lists the chosen Xcode and
runtimes. Setup now asserts build number and both device/simulator SDK versions
and retains its toolchain inventory. Hosted image labels themselves remain
mutable; missing pins fail, never fall back to `latest`.

### Download provenance

All downloads use ordinary official endpoints. No denied-access workaround,
unknown executable, account registration or new license acceptance is needed.
SDK setup now uses already-accepted licenses and fails if requested packages
are unavailable; it does not run `yes | sdkmanager --licenses`.

- Gradle distribution SHA-256:
  `f397b287023acdba1e9f6fc5ea72d22dd63669d59ed4a289a29b1a76eee151c6`,
  checked against [official checksum](https://services.gradle.org/distributions/gradle-8.11.1-bin.zip.sha256).
- Bundletool 1.18.2 downloaded from its official release; computed SHA-256:
  `378b5434cd1378bef6b2bc527b8c7f0ff2584b273830335bce54d6d0813c8584`.
- API 36 Google APIs 16-KB x86_64 revision 7:
  `x86_64-ps16k-36_r07.zip`, size 1,849,428,145 bytes, official SHA-1
  `dd783282e84bf475a02eba6777c79fc5695e1583` from
  [Google repository metadata](https://dl.google.com/android/repository/sys-img/google_apis/sys-img2-3.xml).
  Download verified against that metadata; computed SHA-256:
  `56005e428f0d30105789bde96f60bbd32c5c62e710321abd7c10f9e591889ff7`.
  The existing pinned emulator 37.2.12 exceeds its minimum 35.4.9.
  Local package registration preserves the official package ID for CLI 19;
  a local disposable AVD was successfully created from these exact downloaded
  tools/image (not booted; no runtime result is claimed).
  Revision/API source properties are asserted after extracting the checked ZIP.
- JNA 5.18.1 AAR from Maven Central: computed SHA-256
  `7f053e3ec99e14dd71259c82c1c8a02738d64a13c31226b2acc170f3060951e0`.
  Both packaged `libjnidispatch.so` ABIs passed the ELF checker locally. This
  alone does not validate the final app or JNA calls on a 16-KB runtime.

## Source inventory and changed responsibilities

- Android root/app Gradle files and wrapper properties: only target/toolchain/
  JNA pins, explicit NDK version and Gradle distribution checksum.
- Android Rust build script: target-scoped 16-KB max/common page-size linker
  flags; reject global Rust flags that would silently override them. Host bindgen
  and iOS flags are unchanged.
- Android setup composite: SDK 36, existing pinned tooling, no blanket licenses.
- Emulator script + API36 package XML: retain API31/35; add reviewed API36/16-KB
  image, exact page-size assertion and instrumentation page-size argument.
- `NativePageSizeTest.kt`: real JNA/UniFFI phrase generation/validation, two
  identities, mutual QR contacts, safety-number and storage-key calls on the
  asserted runtime page size. Never disables secure windows or substitutes a
  fake core. Existing runtime regression tests run alongside it.
- `check-android-artifacts.py` and tests: every expected ABI/core/JNA library,
  all native ELF load/RELRO alignment, ZIP structure/alignment, official
  `zipalign`, bundle native-packaging configuration; malformed fixtures fail.
- `validate-android-packages.sh`: checksum-pinned bundletool, actual APK/AAB
  checks and unsigned universal APK derived from the bundle. Isolated empty
  home locations prevent debug-key discovery; signed derived output is rejected.
- iOS setup/project/SwiftPM resources and manifests: version assertions and
  evidence-backed required-reason resources only. See
  [complete privacy inventory](IOS_PRIVACY_INVENTORY.md), including the
  **unresolved SQLite filesystem API reason**. No collection/tracking answers.
- `check-ios-archive.py` and tests: inspect unsigned archive metadata, each
  relevant manifest and actual native symbol inventory; structural success is
  explicitly not privacy-review completion or store readiness.
- CI/unsigned workflows: wire package checks, preserve API31/35 and iOS18.5,
  add API36/16-KB + iOS26.2; record diagnostics per runtime. No new publication,
  tagging, signing credentials, provider configuration or deployment steps.
- Release documentation: reconcile the already-merged durable finalization,
  repair/request-again checkpoint; keep physical/provider/privacy gates open.

## Validation boundaries

Local Linux verification can establish helper fixtures, source invariants,
SwiftPM logic/resource inclusion and downloaded ELF metadata. It cannot replace
macOS Xcode archive/simulator execution or an accelerated 16-KB device. This
executor has no Xcode or `/dev/kvm`, and its installed JVM is 21 rather than the
canonical CI JDK 17. Any local Gradle bootstrap is not a canonical Android build.

Before merging, run the complete exact-SHA CI and independently review its
artifact checks and real FFI instrumentation results. Before release, separately
run authorized unsigned packaging, inspect archive symbols/privacy report,
resolve SQLite reason policy and iOS secure-capture policy, test iOS17 and real
Android/iOS/Hopla interoperability, real provider recovery, power-loss/full-disk
and gesture-level flows. No signing/store upload is authorized by this work.

### Local results for this working tree

- `actionlint` 1.7.12: pass; official release archive checksum verified.
- Source invariant guard: 72 protected files, 14 already-reviewed exceptions,
  unchanged protocol manifest; pass. Production SecureCapture diff: empty.
- Python fixture/regression suite: pass (final count recorded in handoff).
- SwiftPM: all 99 tests pass; privacy resource copied byte-for-byte.
- Gradle 8.11.1 `help`: pass with AGP 8.10.1/Kotlin 2.2.21 resolved from official
  registries through the executor's configured proxy. Used writable temporary
  user/cache directories and local JDK21; **not** an assemble/native/JDK17 pass.
- JNA native ELF metadata: both ABIs pass. CLI19/official API36 image/emulator
  package registration and disposable AVD creation: pass. No emulator boot,
  instrumentation, Android APK/AAB build, macOS archive or hosted CI run yet.
