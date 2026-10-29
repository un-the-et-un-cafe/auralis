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
- Emulator script + API 36 package XML: retain API 31/35; add reviewed API 36/16-KB
  image, exact page-size assertion and instrumentation page-size argument.
- `NativePageSizeTest.kt`: real JNA/UniFFI phrase generation/validation, two
  identities, mutual QR contacts, safety-number and storage-key calls on the
  asserted runtime page size. Never disables secure windows or substitutes a
  fake core. Existing runtime regression tests run alongside it.
- `AppSmokeTest.kt`, `ImeAnimationObserver.kt` and `ImeDismissalAssertionTest.kt`:
  test-only public IME animation observation and eight readiness state tests;
  lifecycle cleanup preserves app callbacks, bounded waits and one Back.
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
- CI/unsigned workflows: wire package checks, preserve API 31/35 and iOS 18.5,
  add API 36/16-KB + iOS26.2; record diagnostics per runtime. No new publication,
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

### Initial local results (before hosted CI)

- `actionlint` 1.7.12: pass; official release archive checksum verified.
- Source invariant guard: 72 protected files, 14 already-reviewed exceptions,
  unchanged protocol manifest; pass. Production SecureCapture diff: empty.
- Python fixture/regression suite: pass (final count recorded in handoff).
- SwiftPM: all 99 tests pass; privacy resource copied byte-for-byte.
- Gradle 8.11.1 `help`: pass with AGP 8.10.1/Kotlin 2.2.21 resolved from official
  registries through the executor's configured proxy. Used writable temporary
  user/cache directories and local JDK21; **not** an assemble/native/JDK17 pass.
- JNA native ELF metadata: both ABIs pass. CLI19/official API 36 image/emulator
  package registration and disposable AVD creation: pass. No emulator boot,
  instrumentation, Android APK/AAB build, macOS archive or hosted CI run at that
  initial checkpoint. Hosted results follow below.

### First hosted-run corrections (2026-10-29)

CI [40851221613](https://github.com/un-the-et-un-cafe/auralis/actions/runs/40851221613)
used PR merge tree `61ab248728b785da719ac7474f6eeb628cfccadf`, identical to reviewed
head `52bf1f289551ecf0d7f51ee14066d57b5aadbe5d`. API 31/35 each passed 29 tests.
API 36 observed `PAGE_SIZE=16384` and its real `NativePageSizeTest` passed, but the
whole suite was **not green**: one UI smoke assertion failed after a single Back.
Logcat shows Back routed to the already-hidden IME while the 2-GiB Google APIs
image killed multiple background processes, including Gboard, under pressure.
That correlation is a hypothesis, not proof of cause. The bounded infrastructure
change allocates 4 GiB only to API 36, checks available host RAM with a 2 GiB reserve,
and retains the identical single-Back/selection assertions. No runtime test is
skipped or automatically retried and no paid runner tier is selected.

The first artifact gate incorrectly rejected graphics-path 1.0.1's isolated
RELRO segment using an unconditional end-modulus rule. The current
[official guide](https://developer.android.com/guide/practices/page-sizes#check-the-relro-security-flag)
explicitly allows an isolated RELRO LOAD with no writable-data overlap. Actual
arm64/x86_64 library headers match that safe layout, including linker padding.
The checker now verifies the dedicated owner LOAD and absence of other writable
LOADs in the rounded protection range; fixtures retain failures for shared
writable tails and overlapping LOADs. This is not a dependency upgrade or a
blanket removal of RELRO checks. Actual bundle-derived unsigned verification
remained unexecuted in the first run because this earlier check stopped the job.

The second run, CI 40855504730 on head `a20923b`, passed the actual APK/AAB/ELF
and unsigned bundle-derived APK gates. Bundletool explicitly emitted unsigned
output, and the independent checker found no v1 entries or APK Signing Block.
However, 4-GiB allocation did **not** fix API 36 UI readiness: the first Back
following profile recreation was consumed without reaching Welcome, and the
original 10-second destination assertion failed. The 16-KB FFI test passed again;
that is not whole-suite success. The allocation is retained for independent
resource-headroom reasons (the Google APIs guest still showed pressure), not as
a claimed navigation fix. No additional RAM escalation or paid tier is selected.

A narrowly authorized test-only readiness change observes public IME animation
lifecycle before recreation, in addition to client insets and input focus. It
must remain lifecycle-safe, preserve application callbacks, use bounded waits,
and issue exactly one Back with the original destination assertion. Its full
matrix validation is pending; no production navigation or security behavior is
changed to make the test pass.

Independent static review and eight JVM state tests passed for the observer.
An animation-free snapshot does not prove absence of queued IME transactions
before `onPrepare`; the full runtime gate must establish whether this is enough.
Keyboard restoration after activity recreation is not imposed: current editors
and assertions promise draft restoration, not IME visibility. No force-show,
refocus, repeated Back or relaxed destination assertion is introduced.
