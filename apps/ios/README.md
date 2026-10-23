# OMCP iOS reference app (M6 shell)

**iOS 17+** · SwiftUI · Design source: [`../../design/`](../../design/) (Designy pack).

## What’s in this shell

| Screen | Status |
|--------|--------|
| TabView (Chats / Communities / Live / You) | Implemented |
| Chats list + path chips | UI + sample rows |
| Chat thread + path banner + composer | UI shell |
| Security centre | UI shell |
| Gateway settings (500 MB/day, Wi‑Fi default, metered warn) | UI shell |
| Communities / Live | “Coming in v0.2” empty states (Jakob) |
| iOS BLE background callout + Android companion hint | Copy in Gateway |
| Onboarding / QR / NFC / real BLE | Deferred (M2+) |
| `omcp-core` FFI | Stub only (`OmcpCoreStub`) — no crypto |

## Open in Xcode (macOS)

```bash
brew install xcodegen   # once
cd apps/ios
xcodegen generate
open OpenMeshChat.xcodeproj
```

Or: File → New → Project → App, then add the `OpenMeshChat/` sources and `Info.plist`.

Linux CI cannot compile SwiftUI; structure is validated by review until a macOS runner (Pipely M6) is enabled.

## Product rules baked in

- Communities + Live tabs visible with v0.2 empty copy (not hidden).
- Gateway: 500 MB/day, Wi‑Fi-only default, warn (not hard-block) on metered cellular.
- Recommend always-on Android companion for iOS background BLE limits.
