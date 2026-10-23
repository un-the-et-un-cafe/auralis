# OMCP Design Pack — Changelog

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
