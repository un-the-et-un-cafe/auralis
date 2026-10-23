# OMCP Smartphone Apps — Design Spec v0.1.0

**Platforms:** Android 12+ (Kotlin UI + Rust core), iOS 17+ (SwiftUI + Rust core)  
**Date:** 2026-10-23  
**References:** PROTOCOL.md §13 (apps), §13.5 (copy), §20 (communities & livestreams); apps/REQUIREMENTS.md

---

## 1. Design principles

1. **Mesh-first clarity.** The user always knows whether a message rides local BLE, a neighbour’s Internet gateway, or is queued. Path state is calm and informative — never alarmist for ordinary mesh use.
2. **Trust through concreteness.** Show verifiable facts (“Verified contact”, “End-to-end · Noise hybrid”) instead of marketing superlatives. Forbidden: “military grade”, “unhackable”, “NSA-proof”.
3. **Privacy by default.** Notification previews off; gateway and store-and-forward opt-in with plain disclosure; recovery phrase shown once with clear “write this down” UX.
4. **One-thumb, low chrome.** Primary actions ≥44×44 pt; list rows ≥56 pt; decorative shadows/gradients avoided; system fonts only.
5. **Bilingual from day one.** English and French string parity; layouts tolerate +40 % text expansion; no text baked into images.
6. **Accessible.** WCAG 2.1 AA contrast; Dynamic Type / fontScale to 200 %; TalkBack / VoiceOver labels on path chips, toggles, and verification states; colour never the sole status carrier (icon + text).
7. **Platform honest.** Surface OS limits (especially iOS background BLE) in product copy, not as buried footnotes.

---

## 2. Design tokens

Implemented in `mockups/tokens.css` as CSS custom properties. Map 1:1 into native themes.

### 2.1 Colour (WCAG AA)

| Token | Light | Dark | Role |
|-------|-------|------|------|
| `--bg` | `#f3f5f8` | `#0c1219` | App background |
| `--surface` | `#ffffff` | `#151d28` | Bars, cards, bubbles (them) |
| `--surface-2` | `#eef1f5` | `#1c2633` | Inputs, secondary fills |
| `--ink` | `#0f1b2d` (≥12:1 on bg) | `#e8eef5` (≥12:1) | Primary text |
| `--ink-2` | `#4a5b70` (≥4.7:1) | `#a3b1c2` (≥4.7:1) | Secondary text |
| `--ink-3` | `#6b7c90` | `#7a8a9c` | Tertiary / timestamps |
| `--line` | `#d8dee8` | `#2a3646` | Dividers |
| `--brand` | `#0b6e6a` (white on brand ≥4.5:1) | `#3dbdb6` | Primary actions, active tab |
| `--brand-soft` | `#e0f3f1` | `#143836` | Sent bubble, soft fills |
| `--ok` / soft | `#0f6b3a` / `#e3f4ea` | `#5dca8e` / `#143525` | Verified |
| `--warn` / soft | `#8a5a10` / `#faf0d8` | `#e0b45a` / `#3a2e14` | Unverified, BLE-only live |
| `--danger` / soft | `#9b1c2e` / `#fce8eb` | `#f07178` / `#3a1820` | Block, destructive |
| `--focus` | `#0a66c2` | `#8ec8ff` | Focus ring ≥3 px |

**Path colours** (calm; mesh must not read as “warning”):

| Path | Token pair | Meaning |
|------|------------|---------|
| Mesh | `--path-mesh` `#0b6e6a` / soft `#e0f3f1` | Local BLE only |
| Gateway | `--path-gateway` `#1a5f9e` / soft `#e4eef8` | Via neighbour Internet |
| Mixed | `--path-mixed` `#5a4a9e` / soft `#eeebf8` | Combination in thread |
| Queued | `--path-queue` `#8a5a10` / soft `#faf0d8` | Waiting for a path |

Dark mode mirrors with lighter chroma on soft backgrounds (see `tokens.css`).

### 2.2 Typography

System stack only:

```
system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif
```

Mono (safety numbers, PeerID): `ui-monospace, "SF Mono", Menlo, Consolas, monospace`.

| Step | Size / line | Weight | Use |
|------|-------------|--------|-----|
| xs | 11–12 / 1.4 | 600 | Tab labels, path chip micro |
| sm | 13 / 1.45 | 400–600 | Meta, badges, timestamps |
| body | 15–16 / 1.45 | 400 | Messages, settings desc (≥16 px inputs on iOS) |
| title | 17–18 / 1.3 | 600–700 | Nav titles, row titles |
| display | 22–28 / 1.2 | 700 | Onboarding headlines |

Respect Dynamic Type / `sp` scaling to 200 %. Do not truncate safety numbers; wrap in mono grid.

### 2.3 Spacing, radii, hit targets

- 4-pt scale: 4 / 8 / 12 / 16 / 24 / 32 / 48.
- Radii: 6 (sm), 10 (controls), 14 (cards), pill 999.
- Min touch target 44×44 pt; list row min height 56–64.
- Separation via 1 px borders; avoid heavy shadows.

---

## 3. Information architecture

```
Root tabs (MVP + v0.2 shell)
├── Chats          ← MVP primary
│   ├── Chat list (path chips, unread, verified mark)
│   ├── 1:1 thread (+ path banner, composer, file progress)
│   ├── Group thread (MLS ≤32)
│   ├── Contact add (QR / NFC / safety number)
│   └── Chat security sheet → Security centre
├── Communities    ← v0.2 (tab visible but empty/gated in MVP builds optional)
│   ├── Community home (announcement + topics)
│   ├── Channel / topic thread
│   └── Admin tools (roles, slow mode, tombstone)
├── Live           ← v0.2
│   ├── Live list + BLE-only empty / limited state
│   └── Start/join (lan-relay / internet-sfu)
└── You            ← MVP
    ├── Profile / PeerID / QR of self
    ├── Gateway settings
    ├── Security & privacy (global)
    ├── Backup / restore
    ├── Notifications (previews OFF default)
    └── About / open source / licences
```

**Onboarding (pre-tabs):** Welcome → Create identity | Restore → Recovery phrase (once) → Permissions (BLE, notifications, optional local network) → Home.

**Modal / sheets:** Safety number compare, gateway disclosure on first enable, file send confirm (size), block confirm, session reset confirm.

---

## 4. Navigation

| Pattern | Android | iOS |
|---------|---------|-----|
| Root | `NavigationBar` 4 destinations (Material 3) | `TabView` 4 tabs |
| Stack | Nav host per tab | `NavigationStack` per tab |
| Chat → Security | Push or modal sheet | Push |
| Add contact | Full-screen cover (camera/NFC) | Full-screen cover |
| Gateway first-on | Modal disclosure (must scroll + confirm) | Same |

Back behaviour: system back / swipe; path banner is not a navigation element.

**MVP shipping tip:** If Communities/Live are not ready, keep tabs but show a short “Coming in v0.2” empty state rather than removing tabs (avoids IA churn). Product may instead hide those two tabs behind a feature flag — decide before beta (open question Q1).

---

## 5. Screen-by-screen specs

### 5.1 Onboarding — identity + recovery (MVP)

**Goals:** Create a local identity without an account server; ensure recovery phrase is recorded; never upload phrase.

**Flow:**

1. **Welcome** — “Chat over Bluetooth mesh. Internet optional.” Primary: Create identity. Secondary: Restore from backup / phrase.
2. **Display name** (local only) — optional photo; PeerID generated underneath in mono truncated form.
3. **Recovery phrase** — 24 words in a 3×8 or 4×6 grid; “I wrote it down” checkbox required; screenshot warning; Copy disabled by default (optional long-press enable). Shown **once**.
4. **Confirm phrase** — challenge 3 random word positions.
5. **Permissions** — BLE (required), Notifications (optional, previews stay off), Local network (iOS, for future LAN relay). Deny BLE → blocking explainer, cannot proceed to mesh.
6. **Done** → Chats empty state with “Add contact” CTA.

**Restore:** Phrase entry → decrypt local backup if present → restore contacts + verification state (PROTOCOL §13.6.5).

**Copy anchors (EN):** “Write these 24 words down and keep them private.” / “Nobody can read your messages — not even gateways.”  
**FR:** “Notez ces 24 mots et gardez-les privés.” / “Personne ne peut lire vos messages — pas même les passerelles.”

### 5.2 Chats list (MVP) — mockup `chats-list`

- Title “Chats”; actions Search, Add.
- **Status strip** under nav: aggregate `PathChip` (e.g. “Mesh · 4 nearby”) + optional “N queued”.
- Rows: avatar, title + verified check, preview, time, unread pill, **per-row path micro-chip** when not default mesh.
- Unverified contacts show warn badge on name (not on path).
- Empty: illustration-free; “Add someone nearby with QR or NFC.”

### 5.3 1:1 chat thread (MVP) — mockup `chat-thread`

- Nav: back, name + verified, “Nearby · last seen…”, shield → Security centre.
- **Path banner** (full width, calm colour): e.g. “Via mesh · 2 hops · encrypted end-to-end” / “Sent via neighbour’s Internet” / “Waiting for mesh”.
- Bubbles: them = surface+border; me = brand-soft. Meta line: time · delivery (Sending / Queued / Sent / Delivered / Read) · path word if differs from banner.
- File bubble: type tile, name, size, progress bar, cancel while sending.
- Composer: attach, field, send. No GIFs/stickers in MVP.

### 5.4 Group chat (MVP)

Same as 1:1 with: group avatar (rounded square), member count subtitle, “N members · MLS”, admin overflow (add/remove member, leave). Path banner reflects group fan-out (often Mixed when some members via gateway). Member list links to per-contact safety numbers.

### 5.5 Contact add — QR / NFC / safety number (MVP)

1. **Chooser:** Scan QR | My QR | NFC tap | Enter PeerID (advanced).
2. **Scan:** Camera with OMCP QR frame; on hit → preview sheet: display name (if any), PeerID, **Safety number**, actions Verify & chat / Cancel.
3. **NFC:** “Hold phones together”; same preview sheet.
4. **Safety number UI:** 60 decimal digits in 12 groups of 5 (PROTOCOL §3.3), mono, large tap-to-mark-verified. States: Unverified | Verified | Revoked.
5. After verify → chat opens; verified mark appears on list/thread.

### 5.6 File transfer progress (MVP)

- In-thread card (see mockup chat-thread).
- Optional persistent notification on Android while transfer ≥ few seconds (content: “Sending file…” — **no filename** if notification previews off).
- Failures: “Couldn’t finish — will retry on mesh” with Retry / Cancel. Max 50 MB; oversize blocked before send with plain size message.

### 5.7 Gateway toggle + battery/privacy disclosure (MVP) — mockup `gateway-settings`

- Entry: You → Gateway.
- Master toggle **Act as gateway** — first enable opens mandatory disclosure sheet:
  - Helps nearby peers reach the Internet.
  - Forwards **encrypted** envelopes only; you cannot read content.
  - Uses battery and may use mobile data.
  - Confirm CTA: “Enable gateway”.
- Settings: Allow on mobile data (off default), daily data cap + meter, store-and-forward (off default on mobile), legend of path chips.
- Android: enabling gateway implies willingness to run foreground service (see §8).

### 5.8 Path indicators + offline queue (MVP)

| State | Chip / banner label (EN) | Label (FR) |
|-------|--------------------------|------------|
| Mesh | Via mesh | Via le mesh |
| Gateway | Via neighbour’s Internet / Gateway | Via Internet d’un voisin / Passerelle |
| Mixed | Mixed paths | Chemins mixtes |
| Queued | Waiting for mesh / Queued | En attente du mesh / En file |

- Queue: messages stay in composer-sent state with queue chip; auto-send on path restore; user can delete from queue long-press.
- Never use red/danger styling for mesh or queue; danger reserved for block / security failure.

### 5.9 Security centre (MVP) — mockup `security-centre`

Per-contact (from chat shield) and global (You → Security):

- Identity header: avatar, name, PeerID, verification badge.
- Safety number block + Mark verified / Scan QR.
- Session: encryption summary (E2EE · Noise hybrid · PQ ready when negotiated), last refresh, **Reset session**.
- Block contact (danger button + confirm).
- Global: notification preview toggle (OFF default), passphrase lock (optional), backup, blocked list, “What gateways can see” explainer.

### 5.10 Communities home (v0.2) — mockup `communities-home`

- List of communities user belongs to; each expands or navigates to: Announcement channel + topic channels (`#general`, etc.).
- Role chip on header (Owner / Admin / Moderator / Member / Guest).
- Join via invite link / QR only (no global directory in v0.2).
- Roster encrypted to members; public communities may show cleartext name/description only (PROTOCOL §20.1).

### 5.11 Channel / topic (v0.2)

- Announcement: admin-only compose; members read + react; signed posts.
- Topic: MLS group UX like group chat; lazy join; invite = signed capability token.
- Path banner still applies for delivery path.

### 5.12 Live tab (v0.2) — mockup `live-tab`

- Lists streams from communities.
- **BLE-only empty / limited state (normative UX):** callout “Video needs Wi‑Fi or Internet” + live cards in `mesh-beacon` mode show title/status only, badge “No video path”, CTA “Connect to Wi‑Fi to go live / watch”.
- Modes: `mesh-beacon` | `lan-relay` | `internet-sfu` (PROTOCOL §20.3). Media keys never on SFU.
- Start stream gated until lan-relay or internet-sfu available.

### 5.13 Admin tools sketch (v0.2)

From community → Admin:

- Roles management (capability tokens signed by community identity key).
- Slow mode duration.
- Remove member / ban (encrypted control frames).
- Delete-for-all (admin-signed tombstone).
- No plaintext moderation API on relays — UI copy must not imply server-side takedown.

---

## 6. Path indicator component (normative UX)

**`PathChip`:** pill, 7 px dot + label, colours from §2.1. Used in list rows (micro), status strip, settings legend.

**`PathBanner`:** full-width bar under chat nav; one line; updates live as routing changes; VoiceOver: “Delivery path: mesh, 2 hops, end-to-end encrypted.”

Do not animate path changes as errors; cross-fade colour/label 200 ms.

---

## 7. Copy principles (EN / FR) — PROTOCOL §13.5

- Plain language for security states.
- Prefer verbs users understand: verify, wait, send via neighbour’s Internet, block.
- Concrete crypto only when useful: “End-to-end encrypted”, “Post-quantum hybrid when both sides support it”.
- **Never:** military grade, unhackable, completely anonymous, guaranteed delivery worldwide.

| Concept | EN | FR |
|---------|----|----|
| Verified contact | Verified contact | Contact vérifié |
| Unverified | Unverified | Non vérifié |
| Waiting for mesh | Waiting for mesh | En attente du mesh |
| Via neighbour Internet | Sent via neighbour’s Internet | Envoyé via l’Internet d’un voisin |
| Gateway help | Help nearby peers reach the Internet | Aider les pairs proches à rejoindre Internet |
| Notification default | New OMCP message | Nouveau message OMCP |
| BLE-only live | Video needs Wi‑Fi or Internet | La vidéo nécessite le Wi‑Fi ou Internet |
| Recovery | Write these 24 words down… | Notez ces 24 mots… |

Keep FR translations reviewed by a human before store submission; table is design intent, not final L10n freeze.

---

## 8. Platform notes

### 8.1 Android 12+

- **Foreground service** for mesh participation with a **persistent notification** explaining BLE use (user-visible, not dismissible while mesh is active). Suggested text: “OMCP is using Bluetooth so you can chat nearby.” Action: Open / Pause mesh.
- Permissions: `BLUETOOTH_SCAN`, `CONNECT`, `ADVERTISE`; location only as required by OS version — justify in-app before system prompt.
- Keystore + StrongBox when available for identity keys.
- WorkManager for deferred gateway fetch when Internet returns.
- Notification content: generic title unless user opts into previews.

### 8.2 iOS 17+

- Background BLE central/peripheral as allowed; App Store justification documented.
- **Tell users clearly** when background mesh is limited (banner on Chats: “iOS may pause mesh in background — open the app to sync”).
- Secure Enclave key wrapping; Keychain accessibility reviewed for background needs.
- MultipeerConnectivity research deferred to v0.2 additional transport; BLE remains mandatory for Android interop.
- Local Network permission for lan-relay livestreams.

---

## 9. Component inventory (Proggy mapping)

| Component | Responsibility | Android sketch | SwiftUI sketch |
|-----------|----------------|----------------|----------------|
| `OmcpTheme` | Tokens light/dark | Compose `MaterialTheme` wrapper | `Environment` + Asset colors |
| `PathChip` | Path pill | Composable | `View` |
| `PathBanner` | Thread path bar | Composable | `View` |
| `ChatRow` | List row + unread + path | LazyColumn item | `List` row |
| `AvatarView` | User / group | — | — |
| `VerifiedMark` | Check shield | Icon | Image |
| `SafetyNumberBlock` | 12×5 digit grid | — | — |
| `MessageBubble` | Text + meta | — | — |
| `FileTransferCard` | Progress / cancel | — | — |
| `ComposerBar` | Attach + field + send | — | — |
| `GatewayDisclosure` | First-enable sheet | Dialog / ModalBottomSheet | sheet |
| `ToggleSettingRow` | Title, desc, switch | — | — |
| `Callout` | Info / warn banners | — | — |
| `EmptyState` | Icon + title + body + CTA | — | — |
| `PrimaryButton` / `DangerButton` | 48 pt height | — | — |
| `TabRoot` | 4 tabs | NavigationBar | TabView |
| `RecoveryPhraseGrid` | 24 words once | — | — |
| `QrScannerScreen` | Contact add | CameraX | AVFoundation |
| `LiveStreamCard` | v0.2 beacon / live | — | — |
| `RoleChip` | Community role | — | — |
| `AdminToolsList` | v0.2 sketch | — | — |

Shared Rust core exposes state (path, verification, queue depth); UI never re-implements crypto decisions.

---

## 10. Accessibility checklist

- [ ] All path chips have accessibility labels including path name + meaning.
- [ ] Contrast ≥ 4.5:1 for text; ≥ 3:1 for large/UI graphics.
- [ ] Focus order: nav → path banner → messages → composer.
- [ ] Reduce Motion: disable non-essential transitions.
- [ ] RTL-ready layout (logical margins) even if v0.1 ships LTR-only locales.
- [ ] Safety number readable at 200 % zoom without horizontal clip.

---

## 11. Out of scope (do not design into MVP)

Desktop clients, Tor transport, voice/video calls (non-livestream), public global directory, payments, stickers/GIF keyboards, “read receipts” as a social flex (delivery states are OK and already specified).

---

## 12. Open product questions (max 3)

See final report to Jakob/Writey — mirrored here:

1. **Tab visibility in MVP:** Show Communities + Live tabs with “Coming in v0.2” empty states, or hide behind feature flag until M-community?
2. **Gateway data cap default:** 500 MB/day Wi‑Fi-only as in mockup — confirm numbers and whether metered cellular should hard-block vs warn.
3. **iOS companion gateway:** PROTOCOL §17 Q3 — does product want a recommended “always-on gateway device” profile (Android-preferred) called out in You → Gateway empty guidance?

---

## Document control

| Version | Date | Notes |
|---------|------|-------|
| 0.1.0 | 2026-10-23 | First design pack: tokens, IA, MVP + v0.2 sketches, 6 HTML mockups |
