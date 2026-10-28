# Auralis native applications

Auralis is an alternative OMCP client and an independent native interpretation of the protocol. Both platforms use the shared Rust protocol engine, with a newly authored **Conversations / Mesh / You** application experience.

| Platform | Implementation | Build and verification |
| --- | --- | --- |
| Android 12+ | Kotlin, Jetpack Compose, Material 3 | [Android README](android/README.md) |
| iOS 17+ | SwiftUI, HoplaKit model layer, UniFFI | [iOS README](ios/README.md) · [Rewrite boundaries](ios/AURALIS_REWRITE.md) |

The app identity is `chat.auralis.app` (Android debug adds `.debug`). Internal source/module names are retained where changing them would add integration risk. New application identifiers allow installation alongside Hopla, but create independent storage and service scopes. Existing Hopla cloud backups are **not automatically migrated or discovered**. Read the platform prerequisites before configuring backup providers or signing.

## Current design and protocol references

- [Auralis identity assets](../design/brand/auralis/README.md)
- [OMCP specification](../PROTOCOL.md), unchanged from the audited upstream baseline
- [Compatibility audit and implemented/deferred matrix](../docs/AURALIS_PROTOCOL_AUDIT.md)
- [Verification evidence](../docs/AURALIS_VERIFICATION.md)
- [CI and manual unsigned packaging](../docs/AURALIS_RELEASE.md)

`REQUIREMENTS.md` and `M5_M6_SHELL.md` preserve upstream product/history context. Historical shell status is not the current Auralis capability matrix. Communities, Live, gateways and other roadmap surfaces must not be mistaken for completed transports.

This is experimental software, not a signed or independently security-audited production release. Linux checks and simulations do not replace cross-client testing on actual Android and iOS devices.
