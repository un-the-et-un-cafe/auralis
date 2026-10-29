# iOS build and required-reason API inventory

Reviewed **2026-10-29**, against source base `5e993c0f2e5f2f5e3404955e976f5713f0171402`
and its bounded platform migration. This is a source inventory and structural
archive gate, **not a complete privacy review, collection declaration, or store approval**.
No signing credentials, provider registration, account changes, or submission are involved.

## Toolchain and coverage

- Pin Xcode **26.3**, build **17C529**, iPhoneOS and simulator SDK **26.2**.
  Setup verifies the actual version/build/SDK and saves an inventory; it does not
  silently select another installed Xcode.
- The observed Auralis hosted job on 2026-10-29 used
  `macos-15-arm64/20260907.0337.1`. Its
  [versioned installed-software inventory](https://github.com/actions/runner-images/blob/macos-15-arm64/20260907.0337/images/macos/macos-15-arm64-Readme.md)
  lists these tools and iOS 18.5/26.2 simulator runtimes. Hosted image labels can
  change; the runtime must still be checked in each job.
- Deployment remains **iOS 17.0**. Runtime testing on 18.5 is retained alongside
  26.2, but **neither is iOS 17 coverage**. A real iOS 17 runtime/device test is an
  open acceptance gap; do not raise minimum OS to conceal that gap.
- Apple requires Xcode 26+ and iOS 26 SDK+ for uploads since April 28, 2026.
  [Current requirement](https://developer.apple.com/news/upcoming-requirements/).

## Evidence-backed declarations and placement

Apple requires reasons to describe actual usage, including linked SDK usage, and
places manifests with the responsible executable/library. See
[required-reason guidance](https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api),
[API categories and approved reasons](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype),
and [privacy manifest packaging](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files).

| Owner and production evidence | Declaration | Bundled resource |
| --- | --- | --- |
| App: `OpenMeshChat/Core/DefaultsAppStore.swift:8–11` and settings getters/setters; `OpenMeshChatApp.swift:65` stores its APNs token; `OmcpRootView.swift:9` uses `@AppStorage` for the app's BLE-tip setting. Default app-local preferences, not shared app-group/system preferences. | `NSPrivacyAccessedAPICategoryUserDefaults`: `CA92.1` | `OpenMeshChat.app/PrivacyInfo.xcprivacy` |
| HoplaKit: `DurableReceivedFiles.swift` calls `lstat`, `fstat`, `fstatat` to validate private file types, identity, size, and link safety. `MeshRepository.swift:216` inspects the app's database/journal/WAL/SHM metadata. `OpenMeshChat/Platform/ReceivedFiles.swift` binds the sink to the app container. | `NSPrivacyAccessedAPICategoryFileTimestamp`: `C617.1` (app-container metadata) | SwiftPM resource `HoplaKit_HoplaKit.bundle/PrivacyInfo.xcprivacy`, copied with consumers; the archive gate checks app and notification extension copies. |
| OmcpFFI: bundled SQLite uses `stat`/`fstat` for the application's private database files. `libsqlite3-sys 0.30.1`, locked checksum `2e99fb7a497b1e3339bc746195567ed8d3e24945ecd636e3619d20b9de9e9149`; `sqlite3/sqlite3.c` defines `osStat`/`osFstat` around lines 38798–38811 and uses them throughout the Unix VFS. | `NSPrivacyAccessedAPICategoryFileTimestamp`: `C617.1` | `Frameworks/OmcpFFI.framework/PrivacyInfo.xcprivacy`, explicitly included by XcodeGen independently of regenerated bindings. |

HoplaKit remains a SwiftPM library; adding a resource declaration does not change
its storage logic. OmcpFFI's Rust static archive is linked into the dynamic Swift
wrapper framework, so the framework manifest travels with the executable that
contains that native code. Confirm this linkage from the actual archive, not
merely the source layout. The notification service's own production source uses
notification/payload policy APIs, not direct defaults/filesystem/boot-time APIs;
no unsupported extension-specific reason is invented.

These manifests contain **only `NSPrivacyAccessedAPITypes`**. They do not assert
`NSPrivacyTracking=false`, an empty collected-data list, “no data collected,” or
answers to any store privacy questionnaire. Those require a separate complete
product/service data-flow review.

## Investigated paths that must not receive guessed reasons

### User-selected file imports

`OpenMeshChat/Screens/ChatsScreens.swift:203–210` uses a document importer,
security-scoped access, and `resourceValues(forKeys: [.fileSizeKey])` to show a
chosen file's size. Apple's current FileTimestamp list includes timestamp keys
and POSIX metadata functions, but **does not list `fileSizeKey` itself**. Source
inspection did not find a direct listed timestamp call on this import path.
Therefore `3B52.1` is not added just because a file picker exists. If subsequent
artifact/API review establishes a covered metadata call for that path, the
user-granted-file reason `3B52.1` is the candidate to review, not app-container
`C617.1`. Do not confuse file contents/size selected for sending with disk capacity.

### Monotonic timing

`crates/omcp-ffi/src/lib.rs:2096,2115,2124` uses `Instant::now()` for frame
reassembly and gossip deduplication. The pinned Rust **1.99.1** implementation
uses `clock_gettime(CLOCK_UPTIME_RAW)` on Apple:
[time/unix.rs](https://github.com/rust-lang/rust/blob/1.99.1/library/std/src/sys/time/unix.rs),
[Timespec implementation](https://github.com/rust-lang/rust/blob/1.99.1/library/std/src/sys/pal/unix/time.rs).
Apple's reviewed SystemBootTime list names `systemUptime` and
`mach_absolute_time()`, not `clock_gettime`. No direct production Swift use of
those listed APIs was found. **Do not infer a `35F9.1` declaration from an old Rust
implementation or from the type name `Instant`.** If a linked dependency imports
`mach_absolute_time`, trace its actual usage before choosing a reason. The
archive report retains all undefined symbols and highlights timing symbols.

### Unresolved SQLite filesystem-type inspection — release privacy blocker

The bundled SQLite Apple VFS calls `fstatfs` at `sqlite3.c:44827` under
`#if defined(__APPLE__) || SQLITE_ENABLE_LOCKING_STYLE`; it inspects
`f_fstypename` for `msdos`/`exfat`, not free capacity. Other configured locking
branches contain `statfs` (for example around line 44078). Apple's current
DiskSpace list includes these functions, but its approved reasons cover displayed
capacity, observable space-availability handling, user-submitted bug reports, or
health research. **No matching approved reason has been established for this
filesystem-type-only use. `E174.1` must not be fabricated.**

The actual native link must be inspected to identify retained calls. Absence of a
symbol alone is not proof that all indirect uses are absent. The source dependency
and storage behavior are unchanged in this migration. Resolution needs a separate
reviewed decision or authoritative Apple guidance; neither patching SQLite nor
claiming policy compliance is part of this build migration.

## Reproducible unsigned-archive structural check

On the selected macOS toolchain, after the existing unsigned archive command:

```sh
python3 scripts/ci/check-ios-archive.py apps/ios/build/Auralis.xcarchive \
  --report apps/ios/build/Privacy/archive-review.json
```

The checker rejects missing/malformed/mismatched reason manifests, unexpected
collection/tracking keys in these source-owned manifests, absent executable
bundles, a provisioning profile or source-owned signature (including an ad-hoc
`LC_CODE_SIGNATURE` without a `_CodeSignature` directory), wrong toolchain/minimum
OS metadata, non-device architecture, and wrong Mach-O platform/minimum/SDK.
It invokes `xcrun lipo`, `otool`, and `nm` read-only on each expected executable
(app, notification extension, OmcpFFI framework), then inventories every additional
Mach-O file under the app, including copied Swift dylibs and other frameworks.
It recognizes thin/universal, 32/64-bit Mach-O magic and rejects symlinks rather
than silently skipping native code or following paths outside the app. Source-owned
executables must meet exact architecture/minimum/SDK pins; other native files retain
their actual architecture/load commands/build versions without pretending Apple's
prebuilt libraries were compiled with our own SDK/minimum. Embedded signatures are
rejected for source-owned executables only. Vendor binaries may retain signatures:
the JSON records `embedded_code_signature` for every native file and does not strip,
verify, or authenticate vendor signatures. Vendor runtime provenance/signature
review remains explicitly open. Thus “unsigned” here means our app/extension/FFI
outputs have no signing, not that every Apple-supplied dependency is unsigned.
The JSON preserves load commands and undefined
symbols per executable and highlights known filesystem/timing API names. Finding
a symbol does not automatically add any required-reason declaration.

**Success means structural validation only.** The report deliberately sets
`privacy_review_complete` and `release_ready` to `false`, retaining the SQLite
reason gap. Objective-C selectors, dynamic dispatch, inlining, transitive API use,
and store-side review are not fully determined by an undefined-symbol scan.
Review Xcode's aggregate archive privacy report on macOS before eventual release;
this script is not a replacement for it. Linux fixture tests do not constitute an
Xcode build or archive validation.

## Separate capture-policy decision

`SecureCapture.swift` production behavior is byte-identical in this migration.
Its undocumented private-canvas reliance remains an unresolved product/security
acceptance decision. Neither the SDK update nor these manifests establishes
universal screenshot prevention or App Store acceptance.

## First hosted simulator symbol evidence (2026-10-29)

Normal CI 40851221613 built with Xcode 26.3/17C529. Its iOS 18.5 job passed 99 SwiftPM
and 23 native tests plus UI smoke. Downloaded artifact 12888152379's embedded
SHA256SUMS were verified; BUILD_INFO identifies PR merge SHA
`e3d0f3ec36da9dd566d0ac0d6996f837404e8f7e`, whose tree exactly equals reviewed
head 52bf1f2 (`61ab248728b785da719ac7474f6eeb628cfccadf`).

`llvm-nm --undefined-only` on that actual simulator
`Frameworks/OmcpFFI.framework/OmcpFFI` lists `_fstatfs`, `_statfs`, `_fstat`,
`_fstatat`, `_lstat`, `_stat` and `_clock_gettime`. `_mach_absolute_time` was not
an undefined import of this binary. This confirms that the filesystem-policy
question is present in an Apple-compiled simulator artifact; it does **not**
prove the final device archive's imports, whole-app API inventory or compliance.
All expected app/HoplaKit/OmcpFFI manifests were present in the simulator bundle.
The device-archive workflow has not been dispatched; its independent inventory
and the unresolved SQLite approved-reason decision remain required.
