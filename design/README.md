# OMCP App Design Pack v0.1.0

**Date:** 2026-10-23 (Europe/Paris)  
**Audience:** Proggy (Android/iOS implementers), Pipely (release), product / spec owners (product)  
**Source of truth for product rules:** `../PROTOCOL.md` §13, §20 and `../apps/REQUIREMENTS.md`  
**This pack:** visual language, IA, screen specs, and static HTML mockups — not wire protocol.

## Where to start

1. Read **[DESIGN_SPEC.md](./DESIGN_SPEC.md)** — principles, tokens, IA, navigation, screen-by-screen specs, platform notes, EN/FR copy, component inventory.
2. Open **[mockups/](./mockups/)** in a browser — phone frames at 390×844. Shared tokens in `mockups/tokens.css`.
3. Use **PNG screenshots** (`mockups/*.png`) in tickets and PR descriptions.
4. Mirror copy for Proggy/Pipely: `/home/box/shared/omcp-design/` (identical tree).

## MVP (v0.1) vs post-MVP (v0.2)

| Area | v0.1 MVP | v0.2 |
|------|----------|------|
| Identity + 24-word recovery | ✓ | — |
| QR / NFC contacts + safety numbers | ✓ | — |
| 1:1 + small MLS groups (≤32) | ✓ | — |
| Files ≤50 MB + progress | ✓ | — |
| Gateway toggle + battery/privacy disclosure | ✓ | — |
| Offline queue + path indicators (mesh / gateway / mixed) | ✓ | — |
| Security centre | ✓ | — |
| Encrypted backup / restore | ✓ | — |
| Notification previews **off** by default | ✓ | — |
| Communities (roles, announcement + topics) | sketched | ✓ |
| Livestreams (mesh-beacon / lan-relay / internet-sfu) | sketched (empty states) | ✓ |
| Stories, polls, stickers, threads | — | ✓ |
| Usernames, bots | — | v0.3 |

Mockups marked **Communities** and **Live** are forward-looking so capacity can be planned; ship gating stays on MVP acceptance tests (PROTOCOL §13.6).

## How Proggy should consume this

1. **Map components** in DESIGN_SPEC § Component inventory → Kotlin (Compose or View) / SwiftUI views. Prefer shared naming (`PathChip`, `SafetyNumberBlock`, `GatewayDisclosure`).
2. **Tokens** → Android `Color`/`Type`/`Dimens` XML or Compose `OmcpTheme`; iOS `Asset` colors + `Font` styles. Light **and** dark required; WCAG AA.
3. **Copy** → string catalogs `en` + `fr` from § Copy principles; never invent “military grade” / “unhackable” language.
4. **Path indicator** is a first-class UI element (chat list row, thread banner, settings legend) — not a debug overlay.
5. **Do not** treat HTML mockups as production UI; they are layout + state references. Native widgets and platform HIG win on spacing micro-adjustments.
6. **Platform constraints** (§ Platform notes): Android persistent BLE foreground notification; iOS background BLE limits with clear user messaging.
7. Open product questions at the end of DESIGN_SPEC — resolve with product/spec owners before locking v0.1 UI freeze.

## Files

```
design/
  README.md              ← you are here
  DESIGN_SPEC.md
  CHANGELOG.md
  mockups/
    tokens.css
    chats-list.html (+ .png)
    chat-thread.html (+ .png)
    security-centre.html (+ .png)
    gateway-settings.html (+ .png)
    communities-home.html (+ .png)
    live-tab.html (+ .png)
```

## Constraints for this pack

- No GitHub push from design work; PROTOCOL.md untouched.
- Clarity of mesh vs online state over decorative chrome.
- Path indicators visible but calm (mesh is normal, not an alert).

## In-repo mirror note

See [SOURCE.md](./SOURCE.md). PNGs from Designy mockups are intentionally not committed.
