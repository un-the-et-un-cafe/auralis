# Google Drive backup — OAuth client setup

Hopla stores **one file**, `omcp-backup-v1.cbor`, in the user's hidden Drive **`appDataFolder`**.
The file is the encrypted CBOR backup sealed by the Rust core (Argon2id password key); it never
contains the recovery phrase or a key in clear. The only OAuth scope is
`https://www.googleapis.com/auth/drive.appdata`: Hopla can't see any other Drive files, and the
user can remove the data in Drive › Settings › Manage apps › Hopla › Delete hidden app data.

No client ID or secret is committed. Native (Android / iOS) OAuth clients have **no client
secret**; the client ID is injected at build time:

| Platform | Where the client ID goes | Used for |
|----------|--------------------------|----------|
| Android | Gradle property `GOOGLE_OAUTH_CLIENT_ID` (`-P…`, `~/.gradle/gradle.properties`, or env var) → `BuildConfig.GOOGLE_OAUTH_CLIENT_ID` | Turns Drive backup on. Google matches the Android client by **package name + signing SHA-1**, so the id itself is not sent. |
| iOS | `apps/ios/Config/Secrets.xcconfig` (git-ignored; copy `Secrets.xcconfig.example`) → Info.plist `GOOGLE_OAUTH_CLIENT_ID` + URL scheme `GOOGLE_OAUTH_REVERSED_CLIENT_ID` | Authorization code + PKCE in `ASWebAuthenticationSession`, redirect `com.googleusercontent.apps.<id>:/oauth2redirect`. |

Empty value → the Drive option shows as unavailable (Android debug builds then use an in-memory
"simulated Drive").

## Steps

1. **Project.** In <https://console.cloud.google.com/> create (or pick) a project, e.g. `hopla-backup`.
2. **Enable the API.** APIs & Services › Library › **Google Drive API** › Enable.
3. **Consent screen** (Google Auth Platform › Branding / Audience / Data access):
   - App name **Hopla**, user support email, developer contact email, app logo and privacy policy
     URL (needed before publishing).
   - Audience: **External**. While in *Testing*, add your Google accounts under *Test users*
     (max 100; refresh tokens of test apps expire after 7 days).
   - Data access › Add or remove scopes › add **`.../auth/drive.appdata`** only. Google lists it as
     a non-sensitive scope at the time of writing; check the console’s classification.
   - To ship: *Publish app* (and complete brand verification if the console asks).
4. **Android client** — Clients (Credentials) › Create client › **Android**:
   - Package name `chat.omcp.app`; SHA-1 of the **release** signing certificate. With Play App
     Signing use the *App signing key certificate* SHA-1 from Play Console › Setup › App signing.
   - Create a second Android client for debug builds: package `chat.omcp.app.debug`, SHA-1 from
     `keytool -list -v -keystore ~/.android/debug.keystore -alias androiddebugkey -storepass android -keypass android`.
   - Build with the client ID, e.g. add to `~/.gradle/gradle.properties`
     `GOOGLE_OAUTH_CLIENT_ID=1234567890-xxxx.apps.googleusercontent.com`
     or `./gradlew assembleRelease -PGOOGLE_OAUTH_CLIENT_ID=…` (CI: env var `GOOGLE_OAUTH_CLIENT_ID`
     from a secret). Devices need Google Play services (Google Identity Services
     `AuthorizationClient`).
5. **iOS client** — Create client › **iOS**:
   - Bundle ID `chat.omcp.app` (add Team ID / App Store ID if you have them).
   - Copy the **Client ID** and the **iOS URL scheme** (reversed client ID) into
     `apps/ios/Config/Secrets.xcconfig`:
     ```
     GOOGLE_OAUTH_CLIENT_ID = 1234567890-xxxx.apps.googleusercontent.com
     GOOGLE_OAUTH_REVERSED_CLIENT_ID = com.googleusercontent.apps.1234567890-xxxx
     ```
     CI: write this file from secrets before `xcodegen generate`.
6. **Check it.** Fresh install → onboarding › Secure your account › Google Drive with a backup
   password → Google consent → "Backed up". Then Drive › Settings › Manage apps shows Hopla with
   hidden app data. Reinstall: the backup is found (Android: silent authorization; iOS: the
   refresh token in the device Keychain survives reinstall) and restore asks for the password.
   Settings › Backup › Delete removes the file (O5).

## Behaviour notes

- Uploads happen only on Wi-Fi/cellular Internet, never over the mesh or a gateway.
- The fresh-install probe never shows Google UI (Android `downloadSilently`, iOS non-interactive
  token). On a *new* iPhone with no Keychain token, restore from Drive needs a sign-in first
  (iCloud restore is the primary path on iOS).
- 401 from Drive → one token refresh and retry; other errors surface as "Backup failed".
