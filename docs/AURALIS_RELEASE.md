# Auralis build and release operations

Auralis retains the upstream OMCP core, wire identifiers, native bridges and tests.
Its app presentation is maintained separately. This pipeline creates **review
artifacts**, not a production-readiness claim or a deployment service.

## What runs

| Entry point | Gates and outputs |
| --- | --- |
| Pull request, push to `main`, or manual `CI` | Workflow lint; Rust formatting, Clippy and workspace tests; Android native bridge build, debug assembly, lint and JVM tests; SwiftPM tests, EN/FR string parity, native bridge build and iOS simulator build/tests. |
| Manual **Package unsigned release candidates** | Explicit unsigned-artifact confirmation, then the **same full CI workflow** for the dispatched revision. Only after all gates pass: Android release tests/lint and unsigned APK/AAB; unsigned iOS device archive and OMCP FFI SDK. |

`required checks` fails if any constituent CI gate failed, was cancelled or was
skipped. Configure it as a required check in the fork's branch protection after
its first successful run. Workflow files alone do **not** enable branch protection.
If a merge queue is introduced, add and validate a `merge_group` trigger first.

CI preserves the upstream Rust simulations, Android unit/bridge tests, HoplaKit
SwiftPM tests and Xcode simulator bridge tests. Android device instrumentation,
physical Bluetooth interoperability, real cloud backup and visual accessibility
checks are **not** covered by these jobs.

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
  Android uses JDK 17, SDK 35, NDK `27.2.12479018`, and `cargo-ndk 4.1.2`.
  SDK command-line tools 19 (`13114758`) are downloaded from Google with a
  pinned SHA-256 checksum, rather than using the runner's mutable `latest`
  SDK manager. This setup accepts the Android SDK licenses during the build.
  Rust uses the repository's `rust-toolchain.toml`; Cargo CI uses `--locked`.
- iOS uses the `macos-15` Apple silicon runner because the upstream Rust script
  builds an arm64 simulator slice. It fails early on another architecture.
  See [GitHub's runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).

Do not add signing credentials to ordinary PR jobs. Future credential-bearing
jobs should use protected environments, reviewers, least-privilege credentials,
and only trusted reviewed revisions. See
[GitHub's workflow security guidance](https://docs.github.com/en/actions/reference/security/secure-use).

## Make an unsigned review candidate

1. Review the commit, update app versions in the platform projects when needed,
   and confirm its source is intended for packaging. The workflow does not bump
   versions, create tags, or choose a release channel.
2. In the fork's **Actions**, select **Package unsigned release candidates** →
   **Run workflow**, select the reviewed ref, and check `confirm_unsigned`.
   The workflow must exist on the default branch to be available for dispatch.
3. Wait for **all** quality and packaging jobs to pass. A failed/missing job is
   not a release candidate; inspect the uploaded test reports and logs.
4. Download the SHA-labelled artifacts from that run:
   - `auralis-android-unsigned-<sha>`: unsigned release APK, unsigned AAB,
     resolved `Cargo.lock`, Cargo metadata, `BUILD_INFO.txt`, and `SHA256SUMS`.
   - `auralis-ios-unsigned-<sha>`: `Auralis-unsigned.xcarchive.tar.gz`,
     `OmcpFFI-sdk.tar.gz` (XCFramework plus Swift bindings), dependency records,
     `BUILD_INFO.txt`, and `SHA256SUMS`. Tar archives preserve bundle symlinks.
5. Check the commit and run URL in `BUILD_INFO.txt`. From each extracted artifact
   directory, verify its files with `sha256sum -c SHA256SUMS` (Linux) or
   `shasum -a 256 -c SHA256SUMS` (macOS).

Unsigned review artifacts expire after **14 days**. Debug APKs expire after
**7 days**; diagnostic reports after **14 days**. Download reviewed evidence
before expiry. Artifacts and logs may be readable by people with repository
access; do not put private credentials or user data into them.

Checksums detect corruption, not publisher identity. These manifests are not
signed provenance or a complete cross-language SBOM. Hosted images, JDK patches,
Android platform-tools, Homebrew/XcodeGen and Xcode can change independently;
recorded tool versions and the Rust lockfile do **not** make builds bit-for-bit
reproducible. Gradle dependency verification/locking and a complete SBOM remain
release-hardening work.

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
  client secret. Debug builds without one may use simulated Drive behavior;
  that is not evidence of live backup working.

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

Internal Xcode targets, the `HoplaKit` module, and app identifiers currently
remain upstream-compatible. **App/store identifiers are not OMCP wire
identifiers.** Confirm ownership and choose the fork's own registered app IDs,
CloudKit containers, keychain groups and OAuth registrations before a separately
installable public release. Changing these requires a deliberate migration and
backup-compatibility decision, not an automatic search-and-replace.
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

The workflow authoring environment could validate YAML, embedded shell syntax,
action pins and `actionlint`, but did not run these workflows on GitHub-hosted
Android/macOS runners or produce release artifacts. A passing local Rust run is
separate evidence and does not prove native packaging. After authorized
publication to a fork, the first CI and manually dispatched packaging runs are
mandatory integration verification. No GitHub publishing, signing, deployment,
store submission or external repository settings change is performed by adding
these files.
