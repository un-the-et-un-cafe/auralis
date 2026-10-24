# OMCP Design Pack — Changelog

## v0.1.6 — 2026-10-24

Moderation & reputation (PROTOCOL §23, **v0.2**) and the maintainer’s Block-screenshots toggle. `DESIGN_SPEC.md` §5H (§5.61–5.70).

### Added
- `mod-report`, `mod-community` (Vouch, reputation indicator + explainer), `mod-states` (Under review — admins only, Reported, Restricted, Why? sheet, counter-signal), `mod-join-warning`, `mod-stream` (blur for new viewers, Continue watching?, reduced motion), `mod-appeal` (owner), `mod-jury`. 46 PNG frames.
- `settings-privacy` Block screenshots toggle: `--off`, `--confirm-off` (+ dark, iOS, FR). Burn-after-read and view-once stay protected when Off.
- §12 Q7–Q10.

### Changed
- `settings-privacy` regenerated (toggle replaces “Always on” badge); §5.57 footnote superseded.

### Constraints observed
- `brand/` and `tokens.css` unchanged. No GitHub push. No real names/photos. Mirror updated.

## v0.1.5 — 2026-10-24

Maintainer decisions (REQUIREMENTS § Maintainer decisions, 2026-10-24). `DESIGN_SPEC.md` §5G (§5.56–5.60).

### Added
- `settings-privacy` (Screenshot protection · Always on, honest second-camera line), `app-switcher-cover` (iOS blurred brand cover, Android black FLAG_SECURE card, iOS recording/mirroring cover).
- Nearby v0.3: `nearby-session` (Go Nearby 30/60/120, iOS Live Activity, Dynamic Island compact + expanded, Android ongoing notification), `nearby-area`, `nearby-radar` (live radar empty, reduced motion), `nearby-skeleton` (0 / 300 / 900 ms timing). Status chip (“Live nearby · 42 min left” / “Area mode”). Motion table with durations + easing.
- `chat-info-notifications--opt-out` (+`-dark`, `-ios`, `-fr`).

### Changed
- Private calls: “Hopla · Incoming call”. Android Hide on lock screen: “Hopla · New message”, never SECRET. Per-chat Private can now be turned off.
- Nearby cards refined (photo scrim, frosted chips, page bars); likes counter moved under buttons; match screen refined (glow, icebreakers).
- Burn viewer copy mentions screenshot blocking + second camera.

### Removed
- All screenshot notices: `burn-bubbles--screenshot-ios.png`, `burn-bubbles--screenshot-android.png`, related copy keys.

### Constraints observed
- `brand/` and `tokens.css` unchanged (cover references `brand/hopla-v2/icon.svg` read-only). No GitHub push. No real names/photos. Mirror updated.

## v0.1.4 — 2026-10-24

Notifications (REQUIREMENTS § Notifications v0.1.2), Burn after read (§ Burn after read v0.1.3), and Hopla Nearby dating (§ Hopla Nearby, **v0.3**, opt-in).

### Added

- Notifications: `notif-lockscreen` (Full / Private / Hide on lock screen, Android + iOS, EN + FR, shade + history), `settings-notifications`, `chat-info-notifications` (incl. mute sheet). `DESIGN_SPEC.md` §5D (§5.36–5.40).
- Burn after read: `burn-composer` (long-press Send menu, armed), `burn-bubbles` (sealed / open with countdown ring / placeholders / iOS screenshot notice / Android FLAG_SECURE row), `burn-viewer` (view once, iOS recording), `burn-chat-info` (timer picker). §5F (§5.50–5.55).
- Nearby v0.3: `nearby-tab`, `nearby-onboarding` (18+ gate), `nearby-profile`, `nearby-discover` (buttons + optional swipe, reduced motion, empty, 100-likes limit), `nearby-match`, `nearby-chat` (Reveal identity: confirm / pending / incoming / revealed, blurred first photo), `nearby-safety` (match list, block, report 2-step, safety tips, share my date). §5E (§5.41–5.49). Illustrated abstract placeholders only.
- 14 HTML mockups, 96 PNG frames; generator `/workspace/omcp-v014/gen.py` (reuses v0.1.1 helpers; local `<style>` per page, tokens only).
- §12 open questions Q4–Q8.

### Changed

- v0.3 tab bar: 5 tabs (Nearby between Communities and Live). Chat info gains Notifications group and 🔥 Burn after read row.

### Constraints observed

- `brand/` and `tokens.css` unchanged. No GitHub push. No real people’s names or photos. Mirror updated.

## v0.1.3 — 2026-10-24

Verified badge as a reward; friendly invitation to verify. “Chat without verifying” stays on QR/NFC adds.

### Added

- `VerifiedBadge` (filled mint seal, `--brand`), `VerifyGhostSeal` (dashed, `--ink-3`, header only), `VerifyHintRow` (dismissible), “earn the badge” animation with reduced-motion fallback — `DESIGN_SPEC.md` §5C (§5.32–5.35), EN + FR copy.
- PNGs: `chats-list--dark`, `chats-list--ios`, `chat-thread--dark`, `--ios`, `--ios-dark`, `--unverified`, `--unverified-dark`, `--unverified-ios`, `--verified`, `--safety-details`.
- Generator `/workspace/omcp-v013/gen.py` (reuses v0.1.1 helpers); `chats-list.html` and `chat-thread.html` are now generated.

### Changed

- Chats list: amber “Unverified” badge on Morgan Reed removed; unverified rows show nothing. Verified mark redrawn as `VerifiedBadge` in brand mint (was `--ok` green).
- §2.1 `--warn` no longer used for unverified; §5.2, §5.3, §7, §9 updated.

### Constraints observed

- `brand/` and `tokens.css` unchanged (badge uses existing tokens; dark frames set `color:var(--ink)` inline). No GitHub push. Mirror updated.

## v0.1.2 — 2026-10-24

Simplified onboarding (4 screens: Welcome › Profile › Secure your account › Done) and recovery backup, per `apps/REQUIREMENTS.md` § Onboarding & recovery backup.

### Added

- Mockups + PNGs (390×844, `tokens.css`, Playwright/Chromium @2x): `onboarding-profile` (+`--filled`), `onboarding-secure` (+`--ios`, `--password-android`, `--password-ios`, `--success`, `--failure`), `onboarding-secure-phrase` (+`--checked`), `onboarding-secure-quiz` (+`--mismatch`), `onboarding-secure-reminder` (+`--expired`), `onboarding-done`, `settings-backup` (+`--queued`, `--view-phrase-auth`, `--delete-confirm`), `restore-reinstall` (+`--password`, `--wrong-password`, `--error`).
- `DESIGN_SPEC.md` §5B (§5.25–5.31): states, EN + FR copy, a11y, Android (Credential Manager, `drive.appdata`, `appDataFolder`) and iOS (CloudKit private DB, iCloud Keychain) notes; allowed security claims list; components `BackupMethodOption`, `BackupPasswordSheet`, `BackupStatusCard`, `BackupReminderBanner`.

### Changed

- `onboarding-welcome`: approved Hopla logo (`brand/hopla-v2/icon.svg`, inlined), “Get started” / “Restore”, 4-step stepper.
- §5.1 flow rewritten; §5.14 updated; §12 questions replaced.
- README screen index: new screens; `onboarding-recovery`, `onboarding-recovery-confirm`, `onboarding-permissions` marked **deprecated** (files kept, HTML titles/captions labelled).

### Constraints observed

- `brand/` and `tokens.css` unchanged; PROTOCOL.md not modified; no GitHub push. Mirror updated (excluding `brand/`).

## v0.1.1 — 2026-10-24

Screens the Android implementer needs next: onboarding with 24-word recovery, QR contact add with safety numbers, file transfer. Consumer app name is now **Hopla** (protocol stays OMCP).

### Added

- Mockups (390×844, `tokens.css`) + PNG renders (390 px @2x, Playwright/Chromium): `onboarding-welcome`, `onboarding-recovery` (+`--checked`), `onboarding-recovery-confirm` (+`--mismatch`), `onboarding-restore`, `onboarding-permissions` (+`--bluetooth-denied`), `contact-add-chooser`, `qr-scan` (+`--not-omcp`), `qr-my-code`, `contact-preview-safety` (+`--mismatch`), `file-picker-confirm` (+`--too-large`), `file-transfer`.
- `DESIGN_SPEC.md` §5A (§5.14–5.24): per-screen states, EN + FR copy, accessibility labels, Android/iOS notes; new components (`AppLogo`, `OnboardingStepper`, `PhraseWordField`, `PermissionCard`, `BlockingExplainer`, `ContactAddOption`, `MyQrCard`, `ContactPreviewSheet`, `FileSendConfirmSheet`).
- `tokens.css`: v0.1.1 component classes appended (no existing token values changed).
- `README.md`: screen index.

### Changed

- User-facing copy uses “Hopla” (notification default “New Hopla message”, Android foreground notification). Logo shown as a neutral placeholder tile; `brand/` untouched.
- §12 open questions: v0.1.0 tab visibility, gateway cap (500 MB/day, Wi‑Fi-only, warn on metered) and preview defaults recorded as decided; three new questions.
- README audience line de-duplicated (aligned with mirror).

### Constraints observed

- PROTOCOL.md not modified; no GitHub push; no “military grade” / “unhackable” copy.
- Mirror `/home/box/shared/omcp-design/` updated to match (excluding `brand/`).

## v0.1.0 — 2026-10-23

First strong design pass for OMCP smartphone apps (Android 12+, iOS 17+).

### Added

- `README.md` — entry point; MVP vs v0.2; Proggy consumption guide.
- `DESIGN_SPEC.md` — principles; light/dark WCAG AA tokens; type & spacing; IA; tab/stack navigation; screen specs for onboarding, chats, 1:1 & groups, contact add, file progress, gateway disclosure, path indicators & queue, security centre, communities, channels, live (incl. BLE-only empty state), admin tools sketch; Android/iOS platform notes; EN/FR copy principles (§13.5); component inventory for Kotlin/SwiftUI.
- `mockups/tokens.css` — shared design tokens.
- Static HTML phone frames (390×844) + PNG screenshots:
  - `chats-list`
  - `chat-thread` (path banner + file progress)
  - `security-centre`
  - `gateway-settings`
  - `communities-home` (v0.2 sketch)
  - `live-tab` (v0.2 sketch, BLE-only video callout)
- Mirror tree at `/home/box/shared/omcp-design/` for Proggy/Pipely.

### Constraints observed

- PROTOCOL.md not modified; no GitHub push.
- Path indicators calm (mesh ≠ alert).
- No “military grade” / “unhackable” copy.
