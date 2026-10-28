# Auralis verification

Verified 2026-10-28 on Debian 13 x86_64 with official Swift 6.4.1 and repository-pinned Rust 1.99.1.

## iOS logic and Rust bridge: passed on Linux

`apps/ios/scripts/verify-linux.sh` completed successfully:

| Check | Result |
| --- | --- |
| EN/FR localization parity | 287 keys; 266 statically used |
| Localization-validator regression tests | 5 passed |
| HoplaKit platform-independent unit tests | 39 passed |
| Real Rust core through generated Swift UniFFI bindings | 8 bridge tests passed |
| Swift syntax parsing | All 51 repository iOS Swift files passed |

The bridge tests exercise backup/restore and contact reverification, chunked file transfer, policy constants, encrypted-store persistence and recovery, safety numbers, chat, burn-once behavior, and OPENED receipts. They use the actual host Rust library, not a stub.

Verification exposed an existing date-dependent test: the same-week CloudKit snapshot replacement test used a fixed date for one store but the current date for another. Both stores now receive the same fixed test clock. Production behavior and the Rust wire protocol were not changed by this fix.

## Reproduce

1. Install a Swift 5.9+ Linux toolchain from the [official Swift downloads](https://www.swift.org/install/linux/debian/13/); this run used 6.4.1. Follow the [official detached-signature verification instructions](https://www.swift.org/install/linux/tarball/).
2. Install the Rust toolchain declared in `rust-toolchain.toml` and add Swift and Cargo to `PATH`.
3. From the repository root:

```bash
swift --version
rustc --version
./apps/ios/scripts/verify-linux.sh
```

The tested Swift 6.4.1 Debian 13 archive was authenticated by the Swift 6.x release PGP key, fingerprint `52BB7E3DE28A71BE22EC05FFEF80A866B47A981F`. Its locally recorded SHA256 was `3273c647a4e29231e1d0962a170f4ba14eca06675591dfe384e7c22b0e476a47`. Generated bindings may report that optional `swiftformat` is absent; this did not prevent compilation or testing.

## Not established by this run

Linux syntax parsing does **not** typecheck Apple frameworks or establish a full iOS build. Xcode compilation, simulator tests, SwiftUI layout, iOS lifecycle/background BLE behavior, signing, and physical-device testing remain separate gates. No IPA or App Store-ready release is implied. The repository's macOS CI job is the configured Xcode/simulator gate.
