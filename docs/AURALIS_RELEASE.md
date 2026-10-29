# Auralis build and release operations

Auralis retains the upstream OMCP formats, wire identifiers, native bridges and tests;
explicit compatible core hardening is tracked by the source review manifest.
Its app presentation is maintained separately. This pipeline creates **review
artifacts**, not a production-readiness claim or a deployment service.

## What runs

| Entry point | Gates and outputs |
| --- | --- |
| Pull request, push to `main`, or manual `CI` | Workflow lint; Rust formatting, Clippy and workspace tests; Android native bridge build, debug assembly, lint and JVM tests; real-app instrumentation on API 31/35 and API 36 16-KB emulators; SwiftPM tests, EN/FR string parity, native bridge build and iOS bridge/UI simulator tests. Debug APK and Apple silicon simulator app are testing outputs. |
| Manual **Package unsigned release candidates** | Explicit unsigned-artifact confirmation and exact reviewed SHA check, then the **same full CI workflow** for the dispatched revision. Only after all gates pass: Android release tests/lint and unsigned APK/AAB; unsigned iOS device archive and OMCP FFI SDK. |

`required checks` fails if any constituent CI gate failed, was cancelled or was
skipped. Configure it as a required check in the fork's branch protection after
its first successful run. Workflow files alone do **not** enable branch protection.
If a merge queue is introduced, add and validate a `merge_group` trigger first.

CI preserves the upstream Rust simulations, Android unit/bridge tests, HoplaKit
SwiftPM tests and Xcode simulator bridge tests, and adds actual native UI smoke
tests. Android runs fresh AOSP x86_64 API 31 and API 35 images; API 35 uses dark
mode and 200% font scale on its disposable emulator. iOS creates a new iPhone 16
simulator on iOS 18.5 for the full `OpenMeshChat` test scheme. No real account,
OAuth setup, sample conversations, simulated cloud provider or privacy bypass is
injected. Tests use generated synthetic identities and test data only.

iOS simulator test builds use local ad-hoc signatures (`CODE_SIGN_IDENTITY=-`)
without a signing certificate, development team, provisioning account or profile.
CI verifies the signature and generated simulator entitlement input's existing
app-only Keychain group before a real Keychain insert/update/readback regression
proves runtime access. Run `40696797061` proved that disabling simulator
signing removed its entitlements (`errSecMissingEntitlement`, `-34018`), so the
app correctly refused to complete onboarding without saving its identity. This
test-build fix does not change Keychain accessibility, entitlement scope, screen
privacy or the unsigned **device** archive workflow.

These tests exercise selected onboarding, cancellation, navigation, persistence
and permission/privacy behavior; they are not exhaustive accessibility or visual
acceptance. Physical Bluetooth interoperability and real cloud backup are **not**
covered. Merely adding the jobs is not a passing runtime result: retain the
successful run and native reports for the exact reviewed SHA.

The required Android runtime matrix first checks the real root-owned `/dev/kvm`
character device and existing access. Run `40696797061` demonstrated that the
hosted runner UID lacked access to its `root:kvm` mode `0660` device. Following
explicit authorization, `scripts/ci/kvm_access.py` may add **only the current
unprivileged runner UID** read/write access, and only on disposable GitHub-hosted
Linux x86_64 runners with that exact baseline ACL. Unexpected device/ACL state
fails closed. The owner, group and other permissions remain unchanged. An
`always()` cleanup revokes the job's UID grant; hard runner termination also
discards the disposable machine. Existing access needs no ACL change.

The emulator still checks `-accel-check` and requires `-accel on`. There is no
global `chmod`, device ownership change, group-membership expansion, persistent
udev rule, permissive SELinux setting, or software-CPU fallback. Missing KVM or
denied ACL tooling fails the gate without retrying a permission change. Never
apply hosted-runner setup instructions to a user's computer. See
[GitHub's KVM availability announcement](https://github.blog/changelog/2024-04-02-github-actions-hardware-accelerated-android-virtualization-now-available/)
and [Android's acceleration reference](https://developer.android.com/studio/run/emulator-acceleration).

### Security and maintenance

- Workflows have only `contents: read`. Checkout does not persist credentials.
  They do not use `pull_request_target`, inherit secrets, or publish releases.
- All external GitHub Actions are pinned to full commit SHAs. The pins were
  verified against their official release pages on 2026-10-28:
  [checkout 7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1),
  [upload-artifact 7.0.2](https://github.com/actions/upload-artifact/releases/tag/v7.0.2),
  [setup-java 6.1.0](https://github.com/actions/setup-java/releases/tag/v6.1.0), and
  [Gradle actions 6.4.0](https://github.com/gradle/actions/releases/tag/v6.4.0).
- Gradle wrapper JAR validation is enabled. Gradle uses its basic GitHub cache;
  pull requests and non-default branches cannot write through this setup.
  The workspace's Rust build output is not shared between CI runs.
- Jobs have time limits. New PR runs cancel obsolete runs; manual packaging
  does not interrupt an already running packaging job for the same ref.
- Dependabot proposes weekly Actions, Cargo and Gradle updates. Review these
  changes; no auto-merge or automatic dependency upgrade is configured.
- Workflow lint installs a versioned `actionlint` using Go's module tooling.
  Android uses JDK 17, SDK 36, NDK `27.2.12479018`, and `cargo-ndk 4.1.2`.
  SDK command-line tools 19 (`13114758`) are downloaded from Google with a
  pinned SHA-256 checksum, rather than using the runner's mutable `latest`
  SDK manager. This setup uses previously accepted Android SDK licenses; new terms require approval.
  Rust uses the repository's `rust-toolchain.toml`; Cargo CI uses `--locked`.
- Android runtime tooling is pinned to official emulator **37.2.12**, build
  `16428233`, AOSP API 31 revision **5** and API 35 revision **2** archives, each
  with SHA-256 verification. The downloaded bytes were checked against Google's
  repository metadata on 2026-10-28 before recording these SHA-256 pins. Updating
  any pin requires reviewing the official release and rerunning all runtime
  jobs; do not replace them with `latest` to make a failing gate pass.
- iOS uses the `macos-15` Apple silicon runner because the upstream Rust script
  builds an arm64 simulator slice. It selects installed **Xcode 26.3 (17C529)** explicitly
  using job-local `DEVELOPER_DIR` and fails if it is absent. **XcodeGen 2.46.0**
  is downloaded from its [official release](https://github.com/yonaskolb/XcodeGen/releases/tag/2.46.0)
  and verified against its published SHA-256, not installed from mutable Homebrew.
  See [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
- The protocol source guard retains immutable upstream commit `233210d5` and
  accepts only exact per-file hashes in `scripts/protocol-review-manifest.json`,
  with original hash, rationale and regression evidence. It rejects unlisted
  modifications, additions, removals, duplicate entries and symlinks. A manifest
  entry records intended acceptance, not an independent security audit; reviewers
  must inspect code and evidence before approving it. See the
  [protocol audit](AURALIS_PROTOCOL_AUDIT.md).

Do not add signing credentials to ordinary PR jobs. Future credential-bearing
jobs should use protected environments, reviewers, least-privilege credentials,
and only trusted reviewed revisions. See
[GitHub's workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use).

## Make an unsigned review candidate

1. Review the commit, update app versions in the platform projects when needed,
   and confirm its source is intended for packaging. The workflow does not bump
   versions, create tags, or choose a release channel.
2. In the fork's **Actions**, select **Package unsigned release candidates** →
   **Run workflow**, select the reviewed ref, paste its full 40-character commit
   into `expected_sha`, and check `confirm_unsigned`. If the ref moved, the run
   fails rather than silently packaging different code. The workflow must exist
   on the default branch to be available for dispatch.

   Equivalent command, **only after packaging authorization**, with placeholders
   replaced by the reviewed ref and SHA:

   ```sh
   gh workflow run release-unsigned.yml --repo un-the-et-un-cafe/auralis \
     --ref <reviewed-branch-or-tag> \
     -f expected_sha=<full-reviewed-commit-sha> -f confirm_unsigned=true
   ```

   This queues real hosted work; it is not a dry run. No workflow is dispatched
   automatically by these instructions.
3. Wait for **all** quality and packaging jobs to pass. A failed/missing job is
   not a release candidate; inspect the uploaded test reports and logs.
4. Download the SHA-labelled artifacts from that run:
   - `auralis-android-unsigned-<sha>`: unsigned release APK, unsigned AAB,
     resolved `Cargo.lock`, Cargo metadata, `BUILD_INFO.txt`, and `SHA256SUMS`.
   - `auralis-ios-unsigned-<sha>`: `Auralis-unsigned.xcarchive.tar.gz`,
     `OmcpFFI-sdk.tar.gz` (XCFramework plus Swift bindings), dependency records,
     `BUILD_INFO.txt`, and `SHA256SUMS`. Tar archives preserve bundle symlinks.
   - The nested quality workflow also supplies `auralis-debug-apk-<sha>` and
     `auralis-ios-simulator-app-<sha>` for installation testing, as described below.
5. Check the commit and run URL in `BUILD_INFO.txt`. From each extracted artifact
   directory, verify its files with `sha256sum -c SHA256SUMS` (Linux) or
   `shasum -a 256 -c SHA256SUMS` (macOS).

Unsigned review artifacts expire after **14 days**. Debug APKs and simulator
apps expire after **7 days**; diagnostic reports after **14 days**. Download
reviewed evidence before expiry. Artifacts and logs may be readable by people with repository
access; do not put private credentials or user data into them.

Checksums detect corruption, not publisher identity. These manifests are not
signed provenance or a complete cross-language SBOM. Hosted OS images, JDK patches
and Android platform-tools can change independently;
recorded tool versions and the Rust lockfile do **not** make builds bit-for-bit
reproducible. Gradle dependency verification/locking and a complete SBOM remain
release-hardening work.

## Install testing artifacts, not unsigned device packages

From the selected completed run's **Artifacts** section, download the SHA-named
package. Confirm the overall run is green, not just one packaging job. With an
already authorized GitHub CLI login, downloads are read-only:

```sh
gh run view <run-id> --repo un-the-et-un-cafe/auralis
gh run download <run-id> --repo un-the-et-un-cafe/auralis \
  --name auralis-debug-apk-<sha> --dir android-test-build
gh run download <run-id> --repo un-the-et-un-cafe/auralis \
  --name auralis-ios-simulator-app-<sha> --dir ios-simulator-build
```

Verify `BUILD_INFO.txt` and `SHA256SUMS` before installing. Use synthetic test data:
these are development builds, not approved production binaries.

- **Android:** the CI debug APK is signed with the runner's generated debug key
  and can be installed on an explicitly selected Android 12+ test device/emulator
  using `adb -s <serial> install -r app-debug.apk`. Its ID is
  `chat.auralis.app.debug`, separate from release and Hopla. The debug key can
  differ between runs. An update-signature mismatch is not permission to silently
  uninstall: uninstalling destroys local identity and history. Back up and obtain
  consent before replacing existing test data. The release `*-unsigned.apk` and
  AAB are **not** this installable debug artifact.
- **iOS simulator:** on Apple silicon macOS with Xcode and an iOS 18.5 simulator,
  extract `Auralis-simulator.app.tar.gz`, choose/boot a disposable simulator, then
  run `xcrun simctl install <udid> OpenMeshChat.app` and
  `xcrun simctl launch <udid> chat.auralis.app`. Keep the app bundle intact; the tar
  preserves its contents and symlinks. This build is arm64 simulator code, not an
  iPhone build or TestFlight IPA. It provides no physical BLE evidence.

CI retains Android HTML/XML reports and emulator/host/logcat diagnostics, and an
iOS `.xcresult` containing bridge/UI results. Manual iOS archive builds also
retain a separate archive result bundle. Inspect native test counts and failure
attachments rather than treating successful compilation as executed tests.

## Signing and service configuration: not wired into these workflows

**No user-provided secret is required for current CI or unsigned packaging.**
The following are prerequisites for a separately authorized distribution setup,
not secret names to add expecting this workflow to consume them.

### Android

- An Android signing/upload keystore, its store password, key alias and key
  password, managed outside git. Do not use the generated debug key for release.
- For Play distribution, a publisher account/app registration and configured
  Play App Signing. A publishing credential would only be needed if a later
  approved workflow automates uploads; none is requested or used here.
- APKs must be signed before installation; AABs need an upload signature for
  Play. An unsigned APK or AAB is not a ready-to-install release. See
  [Android signing documentation](https://developer.android.com/studio/publish/app-signing).
- Google Drive backup needs the correct native OAuth registration for the
  application ID and signing certificate. The build supports
  `GOOGLE_OAUTH_CLIENT_ID` as a Gradle property/environment variable, but these
  workflows do not inject it. Native OAuth client IDs are identifiers, not a
  client secret. Without one, normal builds report Drive backup unavailable.
  Explicit debug-only simulated backup is never enabled by these workflows and
  is not evidence of live backup working.

### iOS

- An Apple Developer team, registered app and notification-extension IDs,
  suitable signing certificates/private keys and provisioning profiles for
  **both targets**. Confirm CloudKit, keychain and push entitlements match the
  chosen team, identifiers, capabilities and distribution environment.
- A signed archive/export flow and reviewed export options. An unsigned
  `.xcarchive` cannot be installed on an iPhone or submitted to TestFlight as
  produced here; this pipeline deliberately does not export an IPA.
- App Store Connect credentials are only needed for a separately approved
  TestFlight/App Store upload step. No Apple credentials are consumed here.
- Google Drive configuration belongs in the ignored
  `apps/ios/Config/Secrets.xcconfig`, using `GOOGLE_OAUTH_CLIENT_ID` and
  `GOOGLE_OAUTH_REVERSED_CLIENT_ID`. No real OAuth configuration is injected.

Internal Xcode targets, the `HoplaKit` module and the Android source namespace
retain their upstream names. The separately installable app identifiers are
`chat.auralis.app` (Android debug adds `.debug`) and
`chat.auralis.app.notifications` for the iOS notification extension. iOS uses
`iCloud.chat.auralis.app` and an Auralis-scoped keychain group.
**App/store identifiers are not OMCP wire identifiers.** The corresponding app
IDs, CloudKit container, keychain entitlement and OAuth registrations must be
registered and provisioned by their owner before distribution; changing source
configuration does not create those registrations. Existing Hopla local data or
cloud backups are not automatically migrated into Auralis's separate sandbox.
Any such migration needs an explicit compatibility and recovery plan.
See the existing [Drive setup](../apps/GOOGLE_DRIVE_BACKUP.md) and
[Apple distribution guide](https://developer.apple.com/documentation/xcode/distributing-your-app-for-beta-testing-and-releases).

## Release acceptance and evidence limits

Before any public distribution, establish and retain evidence of:

- A green CI run and both packaging jobs for the **exact reviewed commit**.
- Android↔Android, iOS↔iOS and Android↔iOS OMCP tests on physical devices:
  discovery, reconnect/background behavior, contact verification, text/files,
  burn-after-read, offline operation and gateway transitions.
- Real backup/restore and interrupted/retried flows; privacy/permission handling,
  accessibility, localization and install/upgrade behavior on supported OSes.
- Signing/entitlement validation, owned app identifiers, store declarations,
  privacy information and a recovery/rollback plan.
- An independent security review appropriate to the intended risk. The upstream
  OMCP protocol remains a draft; passing tests is not a cryptographic audit.

YAML, embedded shell, action-pin and `actionlint` validation is separate from
hosted workflow execution. Consult the CI run for the exact current commit;
an older successful run does not validate later changes. Passing Rust or
simulator tests does not prove unsigned device packaging, which requires its
own manually dispatched run. Signed distribution and physical-device acceptance
remain separate gates even after all build jobs pass. Adding these files alone
does not publish a repository, sign or deploy an app, submit it to a store, or
change external repository settings.

## Reliability checkpoint and remaining v1 gates

Main `5e993c0f2e5f2f5e3404955e976f5713f0171402` includes the
[durable transport-acceptance stage](TRANSPORT_ACCEPTANCE.md),
[durable receive finalization](DURABLE_RECEIVE_STORE.md), explicit local-file
repair and cancelled-generation request-again. Post-merge
[CI run 40844258022](https://github.com/un-the-et-un-cafe/auralis/actions/runs/40844258022)
passed all seven gates: API 31/35 each ran 28 instrumentation tests; Apple ran
99 SwiftPM and 23 native tests, including 16 RustBridge tests and UI smoke.
This is historical evidence for that exact main commit, not validation of the
platform migration or all real-device failure modes.

Receive finalization is no longer an unimplemented next step. The source now
retains recoverable staged data, verifies final bytes and persists completion
before pruning staging. It includes crash/reopen, local-repair and cancelled
receipt regression coverage. Real power-loss, storage-full, radio/background,
Android↔iOS and independently pinned Hopla acceptance remain open. Actual
SwiftUI gesture flows are not established merely by repository-level tests.

**Safe downgrade is still not promised.** Current store-opening safety checks
key/owner before migrations and preserves the original database/journal on
blocked startup. Older binaries may rename/reset stores they cannot open;
do not treat refusal by the Rust schema layer as a safe downgrade procedure.

The [platform migration](PLATFORM_COMPATIBILITY.md) adds API 36, 16-KB artifact
checks and Xcode 26 SDK builds. Its exact-SHA hosted runs, unsigned archives,
16-KB FFI execution and iOS 17 physical/runtime coverage must be established
separately. Privacy-reason inventory (including bundled SQLite), unsupported
iOS screenshot-protection policy, real provider backup/restore, remaining
product scope, independent security review and owner-authorized signing/store
work remain release gates. No v1 label, store readiness, account setup or
publication is implied by green tests.
