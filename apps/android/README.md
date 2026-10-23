# OMCP Android reference app (M5 shell)

**minSdk 31 (Android 12+)** · Kotlin · Jetpack Compose · Material 3  
Design source: [`../../design/`](../../design/) (Designy pack).

## What’s in this shell

| Screen | Status |
|--------|--------|
| Tab root (Chats / Communities / Live / You) | Implemented |
| Chats list + path chips | UI + sample rows |
| Chat thread + path banner + composer | UI shell |
| Security centre | UI shell |
| Gateway settings (500 MB/day, Wi‑Fi default, metered warn) | UI shell |
| Communities / Live | “Coming in v0.2” empty states (Jakob) |
| Onboarding / QR / NFC / real BLE | Deferred (M2+) |
| `omcp-core` JNI | Stub only (`OmcpCoreStub`) — no crypto |

## Build

Requires JDK 17+ and Android SDK 35.

```bash
cd apps/android
./gradlew :app:assembleDebug
```

Gradle wrapper JAR is not committed yet — generate with `gradle wrapper` once a JDK is available, or open the folder in Android Studio.

## Product rules baked in

- Communities + Live tabs visible with v0.2 empty copy (not hidden).
- Gateway: 500 MB/day, Wi‑Fi-only default, warn (not hard-block) on metered cellular.
- Path indicators are first-class UI (never danger-coloured for normal mesh).
