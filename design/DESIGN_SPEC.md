# OMCP Smartphone Apps — Design Spec v0.1.9

**Platforms:** Android 12+ (Kotlin UI + Rust core), iOS 17+ (SwiftUI + Rust core)  
**Date:** 2026-10-24 (v0.1.1; v0.1.0 2026-10-23)  
**Consumer app name:** Hopla (protocol remains OMCP)  
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
| `--warn` / soft | `#8a5a10` / `#faf0d8` | `#e0b45a` / `#3a2e14` | BLE-only live, queue (no longer used for unverified contacts, v0.1.3) |
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

**Onboarding (pre-tabs, v0.1.2):** Welcome → Profile → Secure your account (cloud and/or Write it down, or Remind me later) → Done (QR + nearby, Bluetooth asked in context) → Home. Fresh install with backup → Restore on reinstall.

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

### 5.1 Onboarding — identity + recovery backup (MVP, revised v0.1.2)

**Goals:** a new user is chatting in under 60 s with no account, phone number or email; nobody loses their identity because they skipped writing down 24 words; the phrase is never uploaded in plaintext.

**Flow (v0.1.2, ≤ 4 screens — REQUIREMENTS O1):**

1. **Welcome** (§5.14) — value proposition, **Get started**; secondary **Restore** (24-word fallback).
2. **Profile** (§5.25) — display name + photo, both optional; identity key + phrase generated in background.
3. **Secure your account** (§5.26) — one or more of Google Drive (Android default) / iCloud (iOS default) / Write it down (§5.27); “Remind me later” = 24 h with banner (§5.29).
4. **Done** (§5.28) — your QR code + nearby add over BLE (Bluetooth asked here, in context) → Chats.

**Restore:** fresh install with a cloud backup → §5.31; otherwise Welcome › Restore → 24 words (§5.17). Management: Settings › Backup (§5.30).

**Superseded (v0.1.1):** separate Recovery phrase (§5.15), Confirm phrase (§5.16) and Permissions (§5.18) steps — kept for reference, mockups labelled deprecated.

**Copy anchors (EN):** “Write these 24 words down and keep them private.” / “Nobody can read your messages — not even gateways.”  
**FR:** “Notez ces 24 mots et gardez-les privés.” / “Personne ne peut lire vos messages — pas même les passerelles.”

### 5.2 Chats list (MVP) — mockup `chats-list`

- Title “Chats”; actions Search, Add.
- **Status strip** under nav: aggregate `PathChip` (e.g. “Mesh · 4 nearby”) + optional “N queued”.
- Rows: avatar, title + verified check, preview, time, unread pill, **per-row path micro-chip** when not default mesh.
- Verified contacts: mint `VerifiedBadge` right after the name. Unverified contacts: **nothing** in the row (v0.1.3, see §5.32).
- Empty: illustration-free; “Add someone nearby with QR or NFC.”

### 5.3 1:1 chat thread (MVP) — mockup `chat-thread`

- Nav: back, name + `VerifiedBadge` (verified) or `VerifyGhostSeal` (unverified), “Nearby · last seen…”, shield → Security centre. Unverified threads add `VerifyHintRow` (§5.33).
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

**v0.1.1 detail:** §5.19–5.22 (mockups `contact-add-chooser`, `qr-scan`, `qr-my-code`, `contact-preview-safety`).

### 5.6 File transfer progress (MVP)

- In-thread card (see mockup chat-thread).
- Optional persistent notification on Android while transfer ≥ few seconds (content: “Sending file…” — **no filename** if notification previews off).
- Failures: “Couldn’t finish — will retry on mesh” with Retry / Cancel. Max 50 MB; oversize blocked before send with plain size message.
- **v0.1.1 detail:** §5.23 `file-picker-confirm`, §5.24 `file-transfer`.

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

## 5A. v0.1.1 per-screen specs — onboarding, contact add, file transfer

Added 2026-10-24. Consumer app name is **Hopla**; the protocol stays **OMCP** (use “OMCP” only where the user meets a protocol artefact, e.g. “not an OMCP code”, `omcp1:` PeerIDs, About). Logo is a **neutral placeholder tile** until `brand/hopla-v2/` lands — implement as a swappable `AppLogo` slot. Mockups: `mockups/<screen>.html`; extra states render as `mockups/<screen>--<state>.png`.

Shared rules for every screen below:
- Light + dark tokens from `tokens.css`; mockups render light; dark is token-driven, no separate art.
- Hit targets ≥ 48 dp / 44 pt; Dynamic Type / `sp` to 200 % — word grids reflow 3 → 2 columns at ≥ 150 %.
- Disabled primary buttons use `.btn.disabled` (surface-2 fill, ink-3 text) and expose `aria-disabled` / `isEnabled=false` **plus** a hint saying why.
- Never “military grade”, “unhackable”, “completely anonymous”.
- Onboarding progress bar = 5 steps: Welcome · Phrase · Confirm · Permissions · Done (name step from §5.1 sits between Welcome and Phrase but is not counted as a step; it is optional).

### 5.14 Onboarding — Welcome — `onboarding-welcome` (updated v0.1.2)

**Purpose:** one screen, value proposition, **Get started**. No account, phone number or email. Secondary **Restore** opens the 24-word restore (§5.17); cloud restore is offered automatically by §5.31 when a backup is detected.

| State | Notes |
|-------|-------|
| Default | Stepper 1/4, Hopla logo (`brand/hopla-v2/icon.svg`, 88 pt, inlined), app name, tagline, body, reassurance callout, Get started, Restore, footnote |
| Backup detected | Not shown — §5.31 replaces this screen |

| Copy | EN | FR |
|------|----|----|
| Tagline | Chat over Bluetooth mesh. Internet optional. | Discutez via le mesh Bluetooth. Internet facultatif. |
| Body | No account, phone number or email needed. Your identity is created on this phone. | Pas de compte, de numéro de téléphone ni d’e-mail. Votre identité est créée sur ce téléphone. |
| Reassurance | Nobody can read your messages — not even gateways. | Personne ne peut lire vos messages — pas même les passerelles. |
| Primary | Get started | Commencer |
| Secondary | Restore | Restaurer |
| Footnote | Built on the open OMCP protocol | Basé sur le protocole ouvert OMCP |

**A11y:** logo `role=img` “Hopla logo”; heading level 1 = “Hopla”. Focus order: heading → body → Get started → Restore.
**Android:** edge-to-edge, no toolbar; system back exits app. **iOS:** no nav bar; swipe-back disabled on this root.

### 5.15 Onboarding — Recovery phrase — `onboarding-recovery` (+ `--checked`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** show 24 BIP-39-style words **once**, get a written copy. Never uploaded, never in backups in plaintext.

| State | Notes |
|-------|-------|
| Unchecked (default) | Grid 3×8 numbered words (mono), screenshot warning callout, checkbox off, **Continue disabled** |
| Checked | Checkbox on, Continue enabled |
| Screenshot detected | Toast + callout turns danger: “Screenshot taken. Delete it from your photos and any cloud backup.” (iOS `userDidTakeScreenshotNotification`; Android 14+ `ScreenCaptureCallback`) |
| Back pressed | Confirm dialog “Leave without saving your phrase? You’ll get a new one next time.” |

Copy button: **absent** by default (spec §5.1 “Copy disabled”); optional long-press-to-enable is out of v0.1.1 mockups.

| Copy | EN | FR |
|------|----|----|
| Title | Recovery phrase | Phrase de récupération |
| Headline | Write these 24 words down and keep them private. | Notez ces 24 mots et gardez-les privés. |
| Body | On paper, in order. They are the only way to restore your identity on a new phone. Hopla never uploads them. | Sur papier, dans l’ordre. C’est le seul moyen de restaurer votre identité sur un nouveau téléphone. Hopla ne les envoie jamais. |
| Warning | Don’t take a screenshot — screenshots can sync to cloud photo backups. | Ne faites pas de capture d’écran — elles peuvent être synchronisées avec vos sauvegardes photo en ligne. |
| Checkbox | I wrote it down and stored it somewhere safe. | Je l’ai notée et rangée en lieu sûr. |
| Disabled hint | Tick the box to continue | Cochez la case pour continuer |
| CTA | Continue | Continuer |

**A11y:** grid `role=list`, each word read as “Word 1, anchor”; TalkBack/VoiceOver reads in numeric order (column-major visual reading is not used — row-major 1-2-3). Checkbox is a single target including its label. Continue disabled hint is announced.
**Android:** `WindowManager.LayoutParams.FLAG_SECURE` on this screen (blocks screenshots and recents thumbnail) — the warning is still shown for photos taken with another device. **iOS:** cannot block screenshots; hide words in app switcher snapshot (blur overlay on `sceneWillResignActive`). *(v0.1.5: superseded by §5.57 — app-wide secure layer on iOS too; keep the warning, reword to “Don’t photograph these words with another device” in a future pass.)*

### 5.16 Onboarding — Confirm phrase — `onboarding-recovery-confirm` (+ `--mismatch`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** verify 3 random positions (never consecutive, never all from the same row).

| State | Notes |
|-------|-------|
| In progress | Field per position; correct field gets ok border + check; focused field shows autocomplete chips (prefix match, max 3) |
| Mismatch | Wrong field danger border + inline error; others keep state; Confirm disabled |
| All correct | Confirm enabled → Permissions |
| 3 failed attempts | Callout suggests “Show phrase again” (no lockout; phrase still on device) |

| Copy | EN | FR |
|------|----|----|
| Title | Check your phrase | Vérifiez votre phrase |
| Headline | Enter 3 words from your list | Saisissez 3 mots de votre liste |
| Body | Positions are random each time. This proves the phrase is written down correctly. | Les positions changent à chaque fois. Cela confirme que la phrase est bien notée. |
| Field label | Word #{n} | Mot n° {n} |
| Error | That’s not word #{n}. Check your paper copy. | Ce n’est pas le mot n° {n}. Vérifiez votre copie papier. |
| Secondary | Show phrase again | Revoir la phrase |
| CTA | Confirm | Confirmer |

**A11y:** errors `role=alert` / `announceForAccessibility`; chips are buttons “Suggestion: fossil”. Keyboard: `autocorrect off`, `autocapitalize none`, `textContentType`/`importantForAutofill = no`, incognito keyboard flag on Android (`IME_FLAG_NO_PERSONALIZED_LEARNING`).

### 5.17 Onboarding — Restore — `onboarding-restore`

**Purpose:** enter 24 words, restore identity + local encrypted backup if present (PROTOCOL §13.6.5).

| State | Notes |
|-------|-------|
| Empty | 24 dashed slots, first focused |
| Entering | Filled slots solid; focused slot with brand outline; chips for prefix matches; tap chip or space/enter commits |
| Invalid word | Slot danger + “Word {n} “{w}” isn’t in the word list. Did you mean “{s}”?” |
| Checksum fail (all 24 valid) | Callout danger “These 24 words don’t form a valid phrase. Check the order.” |
| Paste | Pasting 24 space-separated words fills all slots; clipboard cleared after 60 s (Android 13+ also hides clipboard preview via `EXTRA_IS_SENSITIVE`) |
| Restoring | Full-screen progress “Restoring identity…” then “Restored 14 contacts” |

| Copy | EN | FR |
|------|----|----|
| Title | Restore identity | Restaurer l’identité |
| Headline | Enter your 24 words | Saisissez vos 24 mots |
| Body | Type each word in order. Suggestions come from the standard word list only. | Saisissez chaque mot dans l’ordre. Les suggestions viennent uniquement de la liste standard. |
| Backup note | If a local encrypted backup is found, contacts and verification states are restored too. | Si une sauvegarde chiffrée locale est trouvée, les contacts et leurs états de vérification sont aussi restaurés. |
| CTA | Restore | Restaurer |

**A11y:** slot labels “Word 7 of 24, invalid”; progress announced “13 of 24 words entered”. Same keyboard flags as §5.16.

### 5.18 Onboarding — Permissions — `onboarding-permissions` (+ `--bluetooth-denied`) — **DEPRECATED v0.1.2** (see §5.27 / §5.28)

**Purpose:** pre-prompt explainer before each OS dialog. Decided defaults: notification **previews off**.

| State | Notes |
|-------|-------|
| Default | 3 cards: Bluetooth (Required), Notifications (Optional), Local network (Optional · iOS only — card hidden on Android) |
| Granted | Card shows ok badge “Allowed” |
| Bluetooth denied | Replace screen with blocking explainer: icon, reason, how-to path, Open Settings / Try again. Continue unavailable — user can’t reach mesh features |
| Bluetooth off (radio) | Same explainer, CTA “Turn on Bluetooth” (Android `ACTION_REQUEST_ENABLE`) |

| Copy | EN | FR |
|------|----|----|
| Headline | Let Hopla reach people nearby | Laissez Hopla joindre les personnes à proximité |
| Sub | We explain each one before your phone asks. | Nous expliquons chaque autorisation avant que votre téléphone ne la demande. |
| Bluetooth | Needed to send and receive messages over the mesh. Required. | Nécessaire pour envoyer et recevoir des messages via le mesh. Obligatoire. |
| Notifications | Know when a message arrives. Previews stay off: you’ll see “New Hopla message”, not the content. Change in Settings. | Soyez averti à l’arrivée d’un message. Les aperçus restent désactivés : vous verrez « Nouveau message Hopla », pas le contenu. Modifiable dans les réglages. |
| Local network | iOS only. Lets Hopla find phones on the same Wi‑Fi for faster transfers later. | iOS uniquement. Permet à Hopla de trouver les téléphones sur le même Wi‑Fi pour des transferts plus rapides. |
| Denied title | Hopla needs Bluetooth to work | Hopla a besoin du Bluetooth pour fonctionner |
| Denied body | Messages travel phone-to-phone over Bluetooth. Without it, you can’t join the mesh or add contacts nearby. | Les messages passent de téléphone à téléphone via Bluetooth. Sans lui, impossible de rejoindre le mesh ou d’ajouter des contacts à proximité. |
| Denied CTAs | Open Settings · Try again | Ouvrir les réglages · Réessayer |

**A11y:** each card is a group with heading; badges read as text (“Required”). Denied screen title is the focused heading on appear.
**Android:** request `BLUETOOTH_SCAN` (`neverForLocation`), `BLUETOOTH_CONNECT`, `BLUETOOTH_ADVERTISE` together (Nearby devices); `POST_NOTIFICATIONS` on 13+; after 2 denials the OS stops prompting → go straight to explainer + `ACTION_APPLICATION_DETAILS_SETTINGS`. Foreground-service notification (§8.1) starts after BLE granted. **iOS:** `NSBluetoothAlwaysUsageDescription`, `NSLocalNetworkUsageDescription`; Bluetooth denial is final until Settings → explainer path `Settings › Hopla › Bluetooth`; notification request uses `.alert, .badge, .sound` with `UNNotificationContent` showing generic body (previews off).

### 5.19 Contact add — Chooser — `contact-add-chooser`

Full-screen cover from Chats “+”. Four options; Enter PeerID grouped under **Advanced**.

| Copy | EN | FR |
|------|----|----|
| Title | Add contact | Ajouter un contact |
| Intro | Meet in person for the strongest check. Each option shows a safety number before you chat. | Rencontrez-vous en personne pour la vérification la plus sûre. Chaque option affiche un numéro de sécurité avant de discuter. |
| Scan | Scan QR code — Point your camera at their Hopla code | Scanner un QR code — Visez le code Hopla de votre contact |
| My QR | Show my QR code — Let them scan you | Afficher mon QR code — Laissez-le vous scanner |
| NFC | Tap phones (NFC) — Hold the backs of both phones together | Toucher les téléphones (NFC) — Collez le dos des deux téléphones |
| PeerID | Enter PeerID — Paste an omcp1:… ID. You’ll need to compare safety numbers separately. | Saisir un PeerID — Collez un identifiant omcp1:… Vous devrez comparer les numéros de sécurité séparément. |

**States:** NFC row hidden when hardware absent; on iOS NFC row shows only if reader session available (iPhone reads; HCE emulation is Android-only → on iOS, NFC = read-only, other phone must be Android). **A11y:** rows are buttons with combined label+description.

### 5.20 Contact add — Scan QR — `qr-scan` (+ `--not-omcp`)

| State | Notes |
|-------|-------|
| Scanning | Dark camera, white corner viewfinder 240 pt, hint, torch / from photo / my code buttons |
| Torch on | Torch button inverted (white fill), `aria-pressed=true` |
| Not OMCP | Toast danger at top, viewfinder corners turn danger for 2 s, scanning continues |
| Camera denied | Empty state “Camera access is off” + Open Settings + “Show my QR code instead” |
| Hit | Haptic + transition to preview sheet (§5.22) |

Valid code = URI `omcp:` / `omcp1:` PeerID + SAS payload (PROTOCOL §3.3). Anything else (URLs, Wi‑Fi, other apps) → not-OMCP error; **never** open URLs from scanned codes.

| Copy | EN | FR |
|------|----|----|
| Hint | Point at their Hopla QR code. It scans automatically. | Visez le QR code Hopla. Le scan est automatique. |
| Low light | Too dark? Turn on the torch. | Trop sombre ? Allumez la lampe. |
| Error title | Not a Hopla contact code | Ce n’est pas un code contact Hopla |
| Error body | This QR isn’t an OMCP identity code. Ask them to open Add contact › Show my QR code. | Ce QR n’est pas un code d’identité OMCP. Demandez-lui d’ouvrir Ajouter un contact › Afficher mon QR code. |
| Buttons | Torch · From photo · My code | Lampe · Depuis une photo · Mon code |

**A11y:** viewfinder `role=img` “Camera viewfinder”; error announced as alert; VoiceOver hint “Move phone slowly over the code”. **Android:** CameraX + ML Kit barcode (bundled model, no Play-services download dependency for de-Googled devices — fallback ZXing). **iOS:** `AVCaptureMetadataOutput` `.qr`; `DataScannerViewController` acceptable on iOS 17.

### 5.21 Contact add — My QR — `qr-my-code`

| Element | Notes |
|---------|-------|
| QR | Black on white always (even dark mode), quiet zone ≥ 4 modules, centre logo slot (placeholder tile ≤ 20 % area, ECC level H) |
| Name + PeerID | PeerID short form `omcp1:xxxx…xxxx` (first 4 / last 4 of base32); “Copy full PeerID” |
| Brightness | Raise to max on appear, restore on leave; hint callout |
| Actions | Share (system share sheet: text `omcp1:…` + QR PNG), Scan theirs |

| Copy | EN | FR |
|------|----|----|
| Title | My QR code | Mon QR code |
| Brightness hint | Screen brightness is raised while this code is shown. Ask them to scan from Hopla › Add contact. | La luminosité est augmentée pendant l’affichage du code. Demandez-lui de scanner depuis Hopla › Ajouter un contact. |
| Privacy note | Your code contains only your public PeerID — no phone number or location. | Votre code ne contient que votre PeerID public — ni numéro ni position. |
| Actions | Share · Scan theirs · Copy full PeerID | Partager · Scanner le sien · Copier le PeerID complet |

**A11y:** QR `role=img` “Your Hopla contact QR code”; PeerID read character-grouped. **Android:** `window.attributes.screenBrightness = 1f`. **iOS:** `UIScreen.main.brightness` save/restore.

### 5.22 Contact add — Preview + safety number — `contact-preview-safety` (+ `--mismatch`)

Bottom sheet over the camera after a scan/NFC hit.

| State | Notes |
|-------|-------|
| New contact (unverified) | Avatar, name (if shared), PeerID short, warn badge Unverified, `SafetyNumberBlock` 4×3 grid of 12 groups × 5 digits, Verify & chat / Chat without verifying / Cancel |
| Mismatch (known contact, number changed) | Danger badge + callout, differing groups underlined + danger; actions Scan again / Don’t trust this code / Cancel. No “chat” action |
| Verified | Success haptic → thread opens with verified mark |

| Copy | EN | FR |
|------|----|----|
| Section | Safety number | Numéro de sécurité |
| Compare | Compare these 60 digits with the number on {name}’s screen. If they match, nobody is in the middle. | Comparez ces 60 chiffres avec le numéro affiché chez {name}. S’ils correspondent, personne ne s’est interposé. |
| Primary | Numbers match — Verify & chat | Les numéros correspondent — Vérifier et discuter |
| Secondary | Chat without verifying | Discuter sans vérifier |
| Mismatch title | Safety numbers don’t match | Les numéros de sécurité ne correspondent pas |
| Mismatch body | This code is different from the one saved for {name}. Their identity may have changed, or someone may be impersonating them. Check in person before chatting. | Ce code diffère de celui enregistré pour {name}. Son identité a peut-être changé, ou quelqu’un se fait passer pour lui/elle. Vérifiez en personne avant de discuter. |
| Mismatch actions | Scan again · Don’t trust this code | Scanner à nouveau · Ne pas faire confiance |

**A11y:** safety number read group-by-group (“Group 1: 3 3 7 6 6”), each group a focusable element; at 200 % the grid reflows to 2 columns (never horizontal scroll). Mismatched groups announced “differs”. Sheet is modal; focus trapped; Escape/back = Cancel. **Android:** `ModalBottomSheet`, `skipPartiallyExpanded=true`. **iOS:** `.sheet` with `.presentationDetents([.large])`.

### 5.23 File send — Picker confirm — `file-picker-confirm` (+ `--too-large`)

After the system picker (Android `ACTION_OPEN_DOCUMENT` / Photo Picker; iOS `PHPickerViewController` / `UIDocumentPickerViewController`), before encryption starts.

| State | Notes |
|-------|-------|
| Confirm | File tile, name, type; rows Size (“{x} MB of 50 MB max”), Type, Route (`PathChip`), Estimated time; path hint callout; Cancel / Send |
| Route = gateway | Callout: “This goes via a neighbour’s Internet. It counts toward their daily gateway allowance.” (gateway default 500 MB/day, Wi‑Fi only) |
| Route = queue | Callout: “{name} isn’t reachable now. The file will send when they’re back in range.” Send label → “Queue” |
| Too large (> 50 MB) | Danger file card, callout with exact size; Send replaced by “Choose another”. Checked **before** any encryption/chunking |

| Copy | EN | FR |
|------|----|----|
| Title | Send file to {name}? | Envoyer le fichier à {name} ? |
| Size row | {x} MB of 50 MB max | {x} Mo sur 50 Mo max |
| Mesh hint | Via mesh is slower for large files. It keeps sending in the background and resumes if {name} moves out of range. | Via le mesh, les gros fichiers sont plus lents. L’envoi continue en arrière-plan et reprend si {name} s’éloigne. |
| Too-large title | File is too large | Fichier trop volumineux |
| Too-large body | {x} MB is over the 50 MB limit. Hopla can send files up to 50 MB. Try a shorter clip or compress it first. | {x} Mo dépasse la limite de 50 Mo. Hopla envoie des fichiers jusqu’à 50 Mo. Essayez un extrait plus court ou compressez-le. |
| Actions | Cancel · Send · Choose another | Annuler · Envoyer · Choisir un autre |

Estimated time: core supplies throughput estimate; show “~{n} min nearby” only when ≥ 1 min, otherwise hide row. Units: decimal MB (1 MB = 10⁶ B) — 50 MB limit is 50 000 000 B to match core; confirm with Proggy (see Q).
**A11y:** sheet heading is title; Route chip label “Route: mesh, local Bluetooth only”.

### 5.24 File transfer in thread — `file-transfer`

Extends §5.3 file bubble / §5.6. One mockup shows all states. PROTOCOL §7: encrypted chunks (16 KiB) with per-chunk AEAD; resume = re-request missing chunks, so **paused is resumable without restart**.

| State | Line copy (EN) | FR | Progress | Actions |
|-------|----------------|----|----------|---------|
| Completed (received) | {size} · Received · meta “Integrity checked” | {size} · Reçu · « Intégrité vérifiée » | full brand | tap = open |
| Sending | Sending · {a} of {b} MB · ~{n} min | Envoi · {a} sur {b} Mo · ~{n} min | brand | Pause, Cancel (icon buttons) |
| Receiving | Receiving · {a} of {b} MB | Réception · {a} sur {b} Mo | brand | Cancel |
| Paused (out of range) | Paused · out of range · {a} of {b} MB · meta “Waiting for mesh · resumes automatically” | En pause · hors de portée · … · « En attente du mesh · reprise automatique » | `--path-queue` | Resume now |
| Queued | Queued · {size} · next in line | En file d’attente · {size} · prochain envoi | empty | — |
| Failed | Couldn’t finish — will retry on mesh | Échec — nouvel essai via le mesh | danger, frozen at last % | Retry, Cancel |
| Integrity fail (receiver) | File didn’t match — ask {name} to resend | Le fichier ne correspond pas — demandez à {name} de le renvoyer | danger | Delete |

Rules: one active upload per contact (others queue); transfers survive app background on Android via foreground service; notification text “Sending file…” / « Envoi d’un fichier… » with **no filename** while previews are off. Meta path word follows §5.3 (only when differs from banner).
**A11y:** progress `role=progressbar` with value text “41 percent, about 3 minutes left”; announce state changes politely (not every %, max every 10 %). Icon buttons labelled with filename (“Pause sending market-flyer.jpg”).
**Android:** foreground service type `dataSync`/`connectedDevice`; WorkManager for gateway leg. **iOS:** background BLE limits → banner “iOS may pause transfers in background — keep Hopla open to finish faster” when app backgrounds mid-transfer.

---

## 5B. v0.1.2 — Simplified onboarding + recovery backup

Source of truth: `apps/REQUIREMENTS.md` § “Onboarding & recovery backup (v0.1.1)”. Acceptance: O1 fresh install → chat list in **≤ 4 screens, ≤ 60 s** with a cloud backup. Stepper = `OnboardingStepper` with 4 segments. Write-it-down screens are a sub-flow **inside step 3** (stepper stays at 3/4) and don’t count toward O1 for cloud users.

```
Welcome (1) → Profile (2) → Secure your account (3) ─┬─ cloud: password sheet → success / failure ─┐
                                                      ├─ Write it down → 3-word check ─────────────┤→ Done (4) → Chats
                                                      └─ Remind me later → banner 24 h → blocking ─┘
Fresh install with a backup found → Restore on reinstall (replaces Welcome)
```

**Security claims allowed in copy (and only these):** phrase is encrypted on the phone before upload; the provider (Google/Apple) can’t read it; anyone with the cloud account **and** the backup password can restore; forgotten password ⇒ written phrase needed; backups use only the platform cloud over the Internet, never mesh/gateways; offline ⇒ queued and retried. Do **not** claim “zero-knowledge”, “unhackable”, “military grade”, or that a cloud backup can’t be restored by anyone else.

### 5.25 Profile — `onboarding-profile` (+ `--filled`)

| State | Notes |
|-------|-------|
| Empty | Avatar placeholder (camera glyph), name field placeholder “Your name (optional)”, primary reads **Skip**; footnote “Creating your ID…” while identity key + 24-word phrase generate in background |
| Filled | Initials or photo, primary **Continue**, footnote shows PeerID short form |
| Key-gen failed | Callout danger “Couldn’t create your identity. Try again.” (same as §5.14) |

| Copy | EN | FR |
|------|----|----|
| Title | Your profile | Votre profil |
| Heading | How should contacts see you? | Comment vos contacts vous verront-ils ? |
| Sub | Both are optional. You can change them later. | Les deux sont facultatifs. Vous pourrez les modifier plus tard. |
| Photo | Add photo / Change photo | Ajouter une photo / Changer la photo |
| Field | Display name · Your name (optional) | Nom affiché · Votre nom (facultatif) |
| Privacy | Your name and photo are shared only with your contacts — never with a server or people nearby you haven’t added. | Votre nom et votre photo ne sont partagés qu’avec vos contacts — jamais avec un serveur ni avec des personnes à proximité que vous n’avez pas ajoutées. |
| Primary | Skip / Continue | Passer / Continuer |

**A11y:** photo button label “Add profile photo, optional”; field label “Display name, optional”. Footnote is `aria-live=polite` so “Your ID is ready” is announced.  
**Android:** Photo Picker (`PickVisualMedia`), no storage permission. **iOS:** `PhotosPicker`, no library permission prompt.

### 5.26 Secure your account — `onboarding-secure` (+ `--ios`, `--password-android`, `--password-ios`, `--success`, `--failure`)

Multi-select list (`BackupMethodOption`, checkbox, 72 pt min). At least one method must complete before leaving onboarding, or “Remind me later”. Platform default is pre-selected and badged **Recommended**: Android → Google Drive; iOS → iCloud, with Google Drive also offered on iOS. **No cloud method is enabled without this explicit tap** (preselection is visual; upload only starts after Continue + password sheet). If the user deselects everything, Continue is disabled.

| State | Notes |
|-------|-------|
| Android default | Google Drive (Recommended, selected), Write it down |
| iOS default | iCloud (Recommended, selected), Google Drive, Write it down |
| Password sheet — Drive | Password + confirm, 4-segment strength meter (zxcvbn ≥ 3 and ≥ 8 chars required; meter green only when met), CTA “Back up to Google Drive” disabled until valid. Google account picked via Credential Manager before the sheet (system UI) |
| Password sheet — iCloud | Same + checkbox “No password — protect with a key in iCloud Keychain instead”; when checked, the fields collapse |
| Uploading | CTA spinner “Encrypting and uploading…”, sheet not dismissible (Argon2id ≈ 1 s on mid-range) |
| Success | Check mark, “Backed up to {service}”, time, kv card (method, protected by, account), secondary “Also write it down”, primary Continue → Done |
| Failure — offline | “Backup didn’t finish”, callout: queued + auto-retry, never over mesh/gateways. Actions: Try again, Write it down instead, Continue — retry later. Continue counts as a method **pending**; banner from §5.29 applies until upload succeeds |
| Failure — auth/quota | Same layout; body “{service} said no: {reason}.” e.g. “Your Google Drive is full.” No queueing |
| Write it down selected | Continue opens §5.27 |

| Copy | EN | FR |
|------|----|----|
| Title | Secure your account | Sécurisez votre compte |
| Heading | Don’t lose your identity | Ne perdez pas votre identité |
| Sub | Pick one or more ways to back up your 24-word recovery phrase. | Choisissez une ou plusieurs façons de sauvegarder votre phrase de récupération de 24 mots. |
| Drive (Android) | Google Drive · Encrypted on this phone with a backup password before upload. Google can’t read it. | Google Drive · Chiffrée sur ce téléphone avec un mot de passe de sauvegarde avant l’envoi. Google ne peut pas la lire. |
| Drive (iOS) | Google Drive · Also available on iPhone. Needs a backup password. | Google Drive · Aussi disponible sur iPhone. Nécessite un mot de passe de sauvegarde. |
| iCloud | iCloud · Encrypted on this phone before upload. Apple can’t read it. | iCloud · Chiffrée sur ce téléphone avant l’envoi. Apple ne peut pas la lire. |
| Write it down | See your 24 words, then answer a quick 3-word check. | Affichez vos 24 mots, puis répondez à une vérification rapide de 3 mots. |
| Badge | Recommended | Recommandé |
| Honest note | Anyone with your {Google/iCloud} account **and** your backup password can restore your identity. If you forget the password, you’ll need the written phrase. | Toute personne ayant accès à votre compte {Google/iCloud} **et** à votre mot de passe de sauvegarde peut restaurer votre identité. Si vous oubliez le mot de passe, il vous faudra la phrase écrite. |
| Actions | Continue · Remind me later | Continuer · Me le rappeler plus tard |
| Sheet title | Create a backup password | Créez un mot de passe de sauvegarde |
| Sheet body (Drive) | Your phrase is encrypted with this password on your phone, then saved in a hidden Hopla folder in your Google Drive. | Votre phrase est chiffrée avec ce mot de passe sur votre téléphone, puis enregistrée dans un dossier Hopla masqué de votre Google Drive. |
| Sheet body (iCloud) | …then saved in iCloud. | …puis enregistrée dans iCloud. |
| No password | No password — protect with a key in iCloud Keychain instead | Sans mot de passe — protéger avec une clé dans le trousseau iCloud |
| Hint | At least 8 characters. Hopla can’t reset it for you. | Au moins 8 caractères. Hopla ne peut pas le réinitialiser. |
| Strength | Too weak · Strong password | Trop faible · Mot de passe robuste |
| CTA | Back up to {service} | Sauvegarder sur {service} |
| Success | Backed up to {service} · Encrypted on this phone before upload. | Sauvegardé sur {service} · Chiffré sur ce téléphone avant l’envoi. |
| Failure | Backup didn’t finish · We couldn’t reach {service}. Nothing was uploaded. · You’re offline. Your encrypted backup is queued and will upload automatically when you’re back online. Hopla never sends backups over the mesh or gateways. | La sauvegarde n’a pas abouti · Impossible de joindre {service}. Rien n’a été envoyé. · Vous êtes hors ligne. Votre sauvegarde chiffrée est en attente et sera envoyée automatiquement dès votre retour en ligne. Hopla n’envoie jamais de sauvegarde via le mesh ou les passerelles. |
| Failure actions | Try again · Write it down instead · Continue — retry later | Réessayer · L’écrire plutôt · Continuer — réessayer plus tard |

**A11y:** each option is one focusable checkbox: “{title}, {description}, recommended, checked”. Honest note is read after the options, before Continue. Strength meter exposes text (“Strong password”), not colour alone. Password fields support password managers but `autocomplete="new-password"`; no paste block.  
**Android:** Credential Manager account picker → OAuth scope **`drive.appdata` only**; blob stored in **`appDataFolder`** (hidden from the user’s Drive UI and other apps — copy says “hidden Hopla folder”, never “invisible to Google”). Don’t request full Drive scope. **iOS:** blob in **CloudKit private database** (`OMCPBackup` record). No-password key in **iCloud Keychain** (`kSecAttrSynchronizable`, `kSecAttrAccessibleAfterFirstUnlock`) — “end-to-end encrypted by Apple” is only said of the Keychain key, not of CloudKit. If the user isn’t signed in to iCloud, iCloud option shows “Sign in to iCloud in Settings” and is disabled. Google Drive on iOS uses Google Sign-In.

### 5.27 Write it down — `onboarding-secure-phrase` (+ `--checked`) and `onboarding-secure-quiz` (+ `--mismatch`)

Adapts deprecated §5.15/§5.16: same grid, screenshot warning, checkbox and 3-random-word check (autocomplete from BIP-39 list). Changes: stepper fixed at 3/4; heading copy unchanged; body no longer says “Hopla never uploads them” (cloud backup now uploads an *encrypted* copy) → “They can always restore your identity, even without a cloud backup.”; CTA “Check my words”; “Show words again”. No skip on the quiz itself — the way out is Back to §5.26 (choose another method or Remind me later). Success → Done (or back to §5.26 success if a cloud method is also selected).

| Copy | EN | FR |
|------|----|----|
| Title | Write it down / Check your words | Notez-la / Vérifiez vos mots |
| Body | On paper, in order. They can always restore your identity, even without a cloud backup. | Sur papier, dans l’ordre. Ils permettent toujours de restaurer votre identité, même sans sauvegarde cloud. |
| CTA | Check my words | Vérifier mes mots |
| Quiz | Enter 3 words from your list · Positions are random each time. | Saisissez 3 mots de votre liste · Les positions changent à chaque fois. |
| Error | That’s not word #{n}. Check your paper copy. | Ce n’est pas le mot n° {n}. Vérifiez votre copie papier. |

**A11y / platform:** as §5.15–5.16 (`FLAG_SECURE` on Android; iOS screenshot notification → warning toast). *(v0.1.5: superseded by §5.57.)*

### 5.28 Done — `onboarding-done`

Hopla logo (`brand/hopla-v2/icon.svg`, inlined), “You’re all set”, `MyQrCard` (QR with logo, name, PeerID), `ContactAddOption` “Add someone nearby” (BLE). Bluetooth permission is requested **here, in context** (replaces deprecated §5.18): tapping the nearby option shows the system prompt; deny → `BlockingExplainer` from §5.18. Notifications are asked on first received message. Primary “Start chatting” → Chats.

| Copy | EN | FR |
|------|----|----|
| Heading | You’re all set | Tout est prêt |
| Sub | Show this code so people can add you. | Montrez ce code pour qu’on puisse vous ajouter. |
| Nearby | Add someone nearby · Find people next to you over Bluetooth. We’ll ask for Bluetooth access first. | Ajouter quelqu’un à proximité · Trouvez les personnes autour de vous via Bluetooth. Nous vous demanderons d’abord l’accès au Bluetooth. |
| Primary | Start chatting | Commencer à discuter |

**A11y:** QR `role=img` “Your Hopla contact QR code”; brightness raised as §5.21. **Android:** `BLUETOOTH_SCAN`/`CONNECT`/`ADVERTISE` with `neverForLocation`. **iOS:** `NSBluetoothAlwaysUsageDescription` at first nearby tap.

### 5.29 Remind me later — `onboarding-secure-reminder` (+ `--expired`)

“Remind me later” goes straight to Done. For 24 h a persistent warn banner sits under the Chats nav (not dismissible; tap → §5.26). After 24 h an `alertdialog` sheet blocks use until a method completes (O6). A queued-but-not-uploaded cloud backup counts as pending (banner text “Waiting for Internet to finish your backup”), not as complete.

| Copy | EN | FR |
|------|----|----|
| Banner | Back up your account — {n} h left before Hopla pauses. · Back up | Sauvegardez votre compte — encore {n} h avant que Hopla se mette en pause. · Sauvegarder |
| Blocking | Back up to keep using Hopla · It’s been 24 hours. Choose a backup method so you can’t lose your identity. Your chats are safe on this phone. · Choose a backup method | Sauvegardez pour continuer à utiliser Hopla · 24 heures se sont écoulées. Choisissez une méthode de sauvegarde pour ne pas perdre votre identité. Vos discussions restent sur ce téléphone. · Choisir une méthode |

**A11y:** banner `role=status`; blocking sheet `role=alertdialog`, focus trapped, system back doesn’t dismiss (Android) / no swipe-down (iOS). Incoming messages keep being received while blocked (open question 1).

### 5.30 Settings › Backup — `settings-backup` (+ `--queued`, `--view-phrase-auth`, `--delete-confirm`)

Entry: Settings › Backup and Security centre (§5.9) row “Backup”.

| Item | Notes |
|------|-------|
| Status card | Badge Backed up / Waiting for Internet / Not backed up (danger) / Failed; last-backup date+time; **Back up now** (secondary) |
| Methods | Per method row: name, account, protection (password / iCloud Keychain key), On badge. Written phrase row shows date the quiz was passed. “Add or remove a method” → §5.26 list in edit mode. Removing the last method triggers the §5.29 24 h rule |
| Change backup password | Old password → new password sheet (§5.26 rules) → re-encrypt and re-upload |
| View recovery phrase | Gated by biometric **or device PIN** (`BiometricPrompt` with `DEVICE_CREDENTIAL` / `LAPolicy.deviceOwnerAuthentication`), then §5.27 grid read-only |
| Delete cloud backup | Danger; confirm sheet; deletes the remote record (O5); shows success toast “Backup deleted from {service}” |
| Honest note | As §5.26 plus “never the mesh or gateways” |

| Copy | EN | FR |
|------|----|----|
| Title | Backup | Sauvegarde |
| Status | Backed up · Waiting for Internet · Last backup: {date} · new backup queued | Sauvegardé · En attente d’Internet · Dernière sauvegarde : {date} · nouvelle sauvegarde en attente |
| Actions | Back up now · Add or remove a method · Change backup password · View recovery phrase · Delete cloud backup | Sauvegarder maintenant · Ajouter ou retirer une méthode · Changer le mot de passe de sauvegarde · Afficher la phrase de récupération · Supprimer la sauvegarde cloud |
| Auth | Confirm it’s you · Use your fingerprint or device PIN to show your recovery phrase. · Use PIN | Confirmez votre identité · Utilisez votre empreinte ou le code de l’appareil pour afficher votre phrase de récupération. · Utiliser le code |
| Delete | Delete Google Drive backup? · The backup is removed from Google Drive. You can still restore with your written phrase. If you have no other method, you’ll need to set one up within 24 hours. · Delete backup · Cancel | Supprimer la sauvegarde Google Drive ? · La sauvegarde est supprimée de Google Drive. Vous pourrez toujours restaurer avec votre phrase écrite. Sans autre méthode, vous devrez en configurer une sous 24 heures. · Supprimer · Annuler |

**A11y:** status badge text, not colour; destructive row announced “Delete cloud backup, button, destructive”. **Android/iOS:** same layout; iOS shows “iCloud” and “iCloud Keychain key” in the protection line when no password.

### 5.31 Restore on reinstall — `restore-reinstall` (+ `--password`, `--wrong-password`, `--error`)

On first launch the app checks Drive `appDataFolder` (after Credential Manager account pick) / CloudKit private DB. If a backup exists this screen replaces Welcome; otherwise Welcome shows. The 24-word restore (`onboarding-restore`, §5.17) is always reachable.

| State | Notes |
|-------|-------|
| Detected | Logo, “Welcome back”, kv card (account, created, device hint from blob), Restore from {service} · Use my 24 words instead · Start fresh |
| Password | Shown only if the blob has a password KDF; iCloud no-password backups skip straight to restoring |
| Wrong password | Inline error with remaining tries; after 3 failures, 5-minute cooldown with countdown (O4 rate limit); phrase fallback always enabled |
| Restoring | Progress “Restoring contacts and verification…” then Chats (O3) |
| Error | Danger callout (offline / not signed in / record unreadable); Try again · Use my 24 words instead · Start fresh |
| Start fresh | Confirm: “Start fresh? Your old identity stays in {service} until you delete it.” |

| Copy | EN | FR |
|------|----|----|
| Heading | Welcome back | Bon retour |
| Detected | We found a Hopla backup in your {service}. | Nous avons trouvé une sauvegarde Hopla dans votre {service}. |
| Actions | Restore from {service} · Use my 24 words instead · Start fresh | Restaurer depuis {service} · Utiliser mes 24 mots · Repartir de zéro |
| Password | Enter the backup password you chose for this backup. · Forgot it? Hopla can’t reset it. Restore with your 24 words instead. | Saisissez le mot de passe choisi pour cette sauvegarde. · Oublié ? Hopla ne peut pas le réinitialiser. Restaurez plutôt avec vos 24 mots. |
| Wrong | Wrong password. {n} tries left, then wait 5 minutes. | Mot de passe incorrect. Encore {n} essais, puis patientez 5 minutes. |
| Error (iOS) | We couldn’t load your iCloud backup. · Check that you’re online and signed in to iCloud with the same Apple Account, then try again. | Impossible de charger votre sauvegarde iCloud. · Vérifiez que vous êtes en ligne et connecté à iCloud avec le même compte Apple, puis réessayez. |

**A11y:** error `role=alert`; cooldown countdown announced once per minute, not every second.

## 5C. v0.1.3 — Verified badge and verify invitation

Decision (the maintainer, 2026-10-24): “Chat without verifying” stays on QR/NFC adds. Verification is framed as a **reward**: verified contacts earn a mint badge; unverified contacts get a friendly invitation in the thread only. No red, no amber (amber is the queue path colour), no warning copy.

### 5.32 `VerifiedBadge` — `chats-list` (+`--dark`, `--ios`), `chat-thread` (+`--dark`, `--ios`, `--ios-dark`, `--verified`, `--safety-details`)

| Property | Value |
|---|---|
| Shape | 12-point scalloped seal, filled, with knocked-out check (stroke 2.4 on 24 grid). Hopla’s own shape — not Instagram’s blue rosette. |
| Fill / check | `--brand` (light `#0b6e6a`, dark `#3dbdb6`) / `--surface` |
| Size | 15 px in list row (row-title 15–16 px), 16 px in header; scales with Dynamic Type / font scale (`1em`-ish). 4 px gap after name. |
| Placement | Immediately after the display name, before any truncation ellipsis (name truncates, badge never does). |
| Header hit target | 44×44 pt (iOS) / 48×48 dp (Android) around the 16 px glyph → opens **Safety-number details** sheet (`--safety-details`): “Verified contact”, date + method, 12 groups of 5 digits, “Scan Alex’s code again”, “Remove verification”. |
| List row | Not separately tappable (row opens the chat). |
| A11y | Row: appended to the name — “Alex Rivera, Verified”. Header: button “Verified. Show safety number” / “Vérifié. Afficher le numéro de sécurité”. Contrast vs surface: 6.4:1 light, 8.1:1 dark (≥3:1 non-text). |
| Path indicators | No clash: path indicators are pill chips with a dot + label living in the row aside / banner; the badge is a glyph attached to the name and never carries a label or pill. Mesh shares the brand hue by design, shape and position separate them. |
| Groups | No badge on group rows (MLS groups have no single safety number). |

### 5.33 Unverified: `VerifyGhostSeal` + `VerifyHintRow` — `chat-thread--unverified` (+`-dark`, `-ios`)

- **Header ghost seal:** same seal outline, dashed (2.6/2), stroke `--ink-3`, check in `--ink-3`, no fill. 16 px, 44/48 hit target. Tap → verify flow (Scan QR / compare safety number, §5.22). A11y: “Not verified yet. Verify to get the badge” / “Pas encore vérifié. Vérifiez pour obtenir le badge”. Contrast `--ink-3` vs surface 4.3:1 light, 4.9:1 dark.
- **List row: nothing.** Chosen over a tiny ghost seal because: (1) at 15 px a dashed outline reads as noise next to the path chips and unread pills; (2) the list then has one meaning — “mint seal = verified” — that users learn faster than a two-state glyph; (3) unverified is a normal state, not something to flag on every row. The invitation lives where the user can act on it (the thread).
- **Hint row** (below nav, above path banner): `--surface` background, 1 px `--line` bottom, brand shield 18 px, text 13 px `--ink`, `btn-primary btn-sm` “Verify”, ✕ dismiss (36 px glyph, 44/48 target). Neutral background on purpose, so it never looks like the mesh path banner beneath it. `role="note"`.
  - Dismiss: hides for this contact for 30 days (ghost seal stays). Reappears after 30 days or on next in-person proximity, max once.
- Vanishes (ghost seal and hint) the moment the contact is verified.

### 5.34 “Earn the badge” moment — `chat-thread--verified` (end frame)

On return from a successful verify (QR match or safety number marked):
1. 0 ms — hint row collapses (height → 0, opacity → 0, 200 ms ease-out).
2. 120 ms — ghost seal dashes close (stroke-dashoffset to solid, 180 ms), then fill grows from centre in `--brand` (scale 0.6→1.0 of the fill, 240 ms, `cubic-bezier(.2,.8,.2,1)`), check turns `--surface`.
3. 540 ms — single 1.0→1.12→1.0 pop (160 ms). Success haptic (Android `CONFIRM`, iOS `.success`).
4. Toast “Alex is verified” / “Alex est vérifié”, 2.5 s, `role="status"` (announced politely).
Total ≈ 700 ms. **Reduced motion** (Android `ANIMATOR_DURATION_SCALE = 0` / remove animations, iOS Reduce Motion): no scale or dash animation — 150 ms cross-fade from ghost to filled seal; haptic and toast kept. The animation plays once per contact; reopening the chat shows the static badge.

### 5.35 Copy (EN / FR)

| Key | EN | FR |
|---|---|---|
| badge.verified.a11y | Verified | Vérifié |
| badge.verified.header.a11y | Verified. Show safety number | Vérifié. Afficher le numéro de sécurité |
| badge.ghost.a11y | Not verified yet. Verify to get the badge | Pas encore vérifié. Vérifiez pour obtenir le badge |
| hint.verify | Verify {name} to get the verified badge | Vérifiez {name} pour obtenir le badge vérifié |
| hint.verify.cta | Verify | Vérifier |
| hint.dismiss.a11y | Dismiss | Masquer |
| toast.verified | {name} is verified | {name} est vérifié·e |
| sheet.title | Verified contact | Contact vérifié |
| sheet.body | You and {name} verified this safety number in person on {date} ({method}). | Vous et {name} avez vérifié ce numéro de sécurité en personne le {date} ({method}). |
| sheet.rescan | Scan {name}’s code again | Scanner à nouveau le code de {name} |
| sheet.remove | Remove verification | Retirer la vérification |

Platform: Android renders the seal as an `ImageVector` tinted with `MaterialTheme` brand role; iOS as an SF-Symbol-style custom symbol (`hopla.seal.check` / `.dashed`) with `.symbolRenderingMode(.palette)`. Both honour light/dark via tokens.


## 5D. v0.1.4 — Notifications (REQUIREMENTS § Notifications v0.1.2)

Mockups: `notif-lockscreen`, `settings-notifications`, `chat-info-notifications`. Generator `/workspace/omcp-v014/gen.py`.

### 5.36 Notification modes — `notif-lockscreen` (+`--ios`, `--private`, `--private-ios`, `--hidden`, `--hidden-ios`, `--fr`, `--private-fr-ios`, `--burn`, `--shade-private`, `--shade-private-dark`)

| Mode | Title | Body | Avatar | Lock screen |
|---|---|---|---|---|
| **Full** (default, N1) | Sender name (group: “Group · Sender”) | Text preview | Sender avatar | Same as unlocked |
| **Private** (N2) | `Hopla` | `New message` | None (app icon only) | Same — nothing to hide |
| **Hide on lock screen** | Full or Private when unlocked | — | — | Android `VISIBILITY_PRIVATE` with public version “Hopla · New message” (count only); (never SECRET — v0.1.5). iOS `hiddenPreviewsBodyPlaceholder` = “New message”, with `showsPreview`-style category |

- Private applies everywhere the OS stores a notification: banner, lock screen, shade, Android notification history, iOS Notification Centre, Wear/CarPlay mirroring. The app never posts the name or text, so the OS cannot log it (N2).
- Calls in Private: “Hopla · Incoming call” (Decision 1, §5.56). Mentions and livestream starts in Private: `Hopla` / `New message`.
- Burn-after-read content never appears in a preview, even in Full: body = “🔥 Burn-after-read message” (§5.42).
- Delivery: mesh/local raise notifications on-device. Internet wake-ups are content-free FCM/APNs pushes (opaque mailbox id only); decrypt in the iOS Notification Service Extension / Android FCM handler (N3). Settings explains this in one callout.

### 5.37 Settings › Notifications — `settings-notifications` (+`--dark`, `--private`, `--ios`, `--ios-dark`, `--fr`)
Groups: **Messages** (Message notifications · Private notifications · Hide on lock screen · live Preview card) → **Also notify** (Calls · Mentions · Livestream starts) → Quiet hours (start–end) · Communities default level (All / Mentions / None) → content-free push callout. The Preview card re-renders immediately when Private changes, so the user sees exactly what will show.

### 5.38 Chat info › Notifications — `chat-info-notifications` (+`--private`, `--dark`, `--ios`, `--mute`, `--fr`)
Mute (Off / 1 hour / 8 hours / 1 week / Forever, bottom sheet) · Private notifications (per chat; caption “Follows your default (Off)” or “Overrides your default (Off)”) · Hide on lock screen (per chat) · Preview card. Per-chat setting wins in both directions (v0.1.5, §5.56).

### 5.39 Copy (EN / FR)
| Key | EN | FR |
|---|---|---|
| notif.private.title / body | Hopla / New message | Hopla / Nouveau message (rendered “Hopla : Nouveau message”) |
| notif.private.call | Incoming call | Appel entrant |
| settings.private | Private notifications | Notifications privées |
| settings.private.desc | Show only “Hopla: New message”. No name, preview or photo. | Affiche seulement « Hopla : Nouveau message ». Ni nom, ni aperçu, ni photo. |
| settings.hideLock | Hide on lock screen | Masquer sur l’écran verrouillé |
| settings.hideLock.desc | Nothing shows until your phone is unlocked. | Rien ne s’affiche tant que le téléphone est verrouillé. |
| settings.quiet | Quiet hours | Heures calmes |
| settings.push | Internet wake-ups go through Google or Apple with no content. Your phone decrypts the message itself, so they never see who sent it or what it says. | Les notifications Internet passent par Google ou Apple sans aucun contenu : votre téléphone déchiffre le message lui-même. Ils ne voient jamais l’expéditeur ni le texte. |
| chat.private.follow / override | Follows your default (Off). / Overrides your default (Off). | Suit votre réglage général (désactivé). / Remplace votre réglage général (désactivé). |
| chat.mute.* | Mute Alex · Messages still arrive, without sound or banners. · 1 hour / 8 hours / 1 week / Forever | Mettre Alex en sourdine · Les messages arrivent, sans son ni bannière. · 1 heure / 8 heures / 1 semaine / Toujours |

### 5.40 Accessibility & platform
- Toggles are `role=switch` with the visible title as label; descriptions linked as hint (`contentDescription` / `accessibilityHint`). Preview card announced as “Preview: Hopla, New message”.
- In Private mode the screen-reader announcement of a notification is also only “Hopla, New message” (the spoken text equals the posted text — no hidden `tickerText`/`accessibilityLabel` with the name).
- Contrast: lock-screen cards are system-drawn; mockups approximate. Toggles meet 3:1 non-text contrast.
- Android: one channel per type (Messages, Calls, Mentions, Live); Private sets no `largeIcon`, no `MessagingStyle` person, no `shortcutId` (otherwise conversation bubbles leak the name). Hide on lock = `setVisibility(PRIVATE)` + `setPublicVersion(“Hopla · New message”)`; Never SECRET (Decision 3).
- iOS: NSE builds content; Private omits `INSendMessageIntent` (communication notification avatar) and uses `threadIdentifier` without names. Hide on lock = category `hiddenPreviewsBodyPlaceholder` “New message” + `.hiddenPreviewsShowTitle` off.

---

## 5E. v0.3 — Hopla Nearby: dating (REQUIREMENTS § Hopla Nearby, opt-in) — **all v0.3**

Mockups: `nearby-tab`, `nearby-onboarding`, `nearby-profile`, `nearby-discover`, `nearby-match`, `nearby-chat`, `nearby-safety`. Every frame carries a violet **v0.3** tag. Tone: warm (amber→violet wash built from `--warn-soft` / `--path-mixed-soft`), Hopla teal for primary actions, rounded cards, no flames, no hearts raining. Avatars and photos are **illustrated abstract placeholders** only.

Tab bar in v0.3 becomes 5 tabs: Chats · Communities · **Nearby** · Live · You (icon = ripple).

### 5.41 Nearby entry / off — `nearby-tab` (+`--dark`, `--ios`, `--fr`)
Off on a fresh install (D1): ripple illustration, 3 promises (18+ only · separate profile · never exact location), **Turn on Nearby**, footnote “Off. Nothing is sent until you turn it on.” No packets until onboarding completes.

### 5.42 18+ onboarding — `nearby-onboarding` (+`--under-18`, `--rules`, `--privacy`, `--dark`, `--ios`, `--fr`)
4 steps: (1) date of birth → only age range is shared, DOB stays on device; under 18 → blocking callout, **Back to chats**, Nearby stays off on device (store age-estimation check slots in here if required); (2) community rules, “I agree”; (3) how privacy works (daily key, rough distance, secret likes, hidden identity); (4) profile editor.

### 5.43 Profile editor — `nearby-profile` (+`--dark`, `--ios`, `--fr`)
Up to 6 photos (first = Main; re-encode + EXIF/GPS strip notice), first name/nickname, age shown as range, bio, interests (tags), Looking for (Dating / Friends / Both segmented), Show me (gender, age range), Visibility (Pause · Only shared interests · Hide from my contacts — hashed, local).

### 5.44 Discover — `nearby-discover` (+`--swiping`, `--reduced-motion`, `--dark`, `--ios`, `--empty`, `--limit`, `--fr`)
- Card: photo, distance **bucket only** chip (“< 1 km · mesh”, “< 5 km”, “in your area”) — never metres, never a map (D2). Name + age range, intent pill, bio, interests with shared ones highlighted, flag button (block/report, one tap).
- **Buttons always visible**: Pass · Profile · Like (64 px / 48 px, labelled). Swipe is an optional shortcut. Likes-left pill (100/day); limit state; empty state.
- Motion: swipe tilts ≤ 7° with PASS/LIKE stamp. `prefers-reduced-motion` / Remove animations: no tilt, no fling — 150 ms cross-fade.
- A like is sealed; UI never says “X liked you” before a mutual like (D3).

### 5.45 Match — `nearby-match` (+`--dark`, `--ios`, `--reduced-motion`, `--fr`)
“It’s mutual” · “You and Robin matched” · explains new E2EE chat and hidden identities · **Say hello** / Keep browsing. Confetti dots fade in once (600 ms); reduced motion = static. Match notification follows §5.36 (Private → “Hopla / New message”).

### 5.46 Match chat + Reveal identity — `nearby-chat` (+`--reveal-confirm`, `--reveal-pending`, `--reveal-incoming`, `--revealed`, `--photo-blurred`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
Header: placeholder avatar, “Nearby match · identity hidden”, flag (block/report). Violet banner with **Reveal identity**. Confirm sheet lists what is shared (Hopla name, profile photo, contact key; not number, not location). States: pending (“Nothing is shared until Robin taps too”), incoming (“Reveal too”), revealed (system row + contact card, not verified yet). First-contact photos blurred, tap to view. No messages possible before a match.

### 5.47 Safety — `nearby-safety` (+`--block`, `--report`, `--report-confirm`, `--done`, `--tips`, `--share-date`, `--dark`, `--tips-dark`, `--ios`, `--fr`)
- Block/report one tap from profile card (flag), chat header (flag), match list (swipe actions + screen-reader custom actions).
- Block sheet: hidden across key rotations, they aren’t told; optional “Also report”. Undo toast.
- Report: step 1 reason; step 2 review bundle (profile card required, messages opt-in), moderator shown and changeable, **Send report** — nothing sent before (D5).
- Safety tips shown before first meet-up; **Share my date** sends time + typed place to one trusted contact as an E2EE message.

### 5.48 Copy (EN / FR) — key strings
| Key | EN | FR |
|---|---|---|
| tab | Nearby | À proximité |
| entry.cta / foot | Turn on Nearby / Off. Nothing is sent until you turn it on. | Activer À proximité / Désactivé. Rien n’est envoyé tant que vous ne l’activez pas. |
| age.title / under | How old are you? / Nearby is for adults 18+ | Quel âge avez-vous ? / Réservé aux 18 ans et plus |
| dist | < 1 km · < 5 km · in your area | < 1 km · < 5 km · dans votre secteur |
| btn | Pass · Profile · Like | Passer · Profil · Like |
| limit | That’s all for today. You’ve used your 100 likes. | C’est tout pour aujourd’hui. Vous avez utilisé vos 100 likes. |
| match | You and Robin matched / Say hello | Robin et vous, c’est un match / Dire bonjour |
| reveal | Reveal identity · Reveal your identity to Robin? · Offer to reveal · Waiting for Robin… · Reveal too | Révéler mon identité · Révéler votre identité à Robin ? · Proposer de révéler · En attente de Robin… · Révéler aussi |
| block | Block Robin? Robin won’t see you in Nearby again, even after their keys change. Robin isn’t told. | Bloquer Robin ? Robin ne vous verra plus dans À proximité, même après le changement de clés. Robin n’est pas prévenu. |
| report.confirm | Check before sending · Nothing is sent until you tap Send report. | Vérifiez avant d’envoyer · Rien n’est envoyé avant que vous touchiez Envoyer. |
| tips / share | Before your first meet-up / Share my date | Avant votre premier rendez-vous / Partager mon rendez-vous |

### 5.49 Accessibility & platform
- Every swipe has a button; TalkBack/VoiceOver custom actions on the card: Like, Pass, View profile, Block or report. Card announced “Robin, 25 to 29, less than 1 km, Dating, 2 shared interests”.
- Placeholder art has `aria-label`; real photos need user-written alt text option (optional field, v0.3.1).
- Reduced motion respected for swipe, match confetti, card entrance. Targets ≥ 44/48 px.
- Android: Nearby BLE advertising uses its own foreground service notification (“Nearby is on”) only while enabled. iOS: background BLE limits mean discovery mostly in foreground; say so in onboarding step 3 if confirmed (§12 Q7).
- Screenshots of Nearby profiles are not blocked (no claim made).

---

## 5F. v0.1.4 — Burn after read (REQUIREMENTS § Burn after read v0.1.3)

Mockups: `burn-composer`, `burn-bubbles`, `burn-viewer`, `burn-chat-info`. Accent: `--warn` (amber) + 🔥, never red/danger.

### 5.50 Composer — `burn-composer` (+`--armed`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
Long-press **Send** → menu: 🔥 Burn after read toggle (next message or image), Delete unread after 1 h / 24 h / **7 d** (default), Schedule. Armed: amber outlined field, amber 🔥 send button, status line “Burn after read on · deletes unread after 7 d · Turn off”. Disarms after one send (per-message).

### 5.51 Bubbles — `burn-bubbles` (+`--sender`, `--open`, `--placeholders`, `--dark`, `--ios`, `--ios-dark`, `--fr`)
- Receiver: dashed amber bubble, blurred text, **Tap to read**, unread expiry. Photo: blurred, “Photo · view once”.
- Sender: same shape, “Not opened yet”; honest one-line note under the first burn message per chat.
- Opened: text visible, countdown ring + m:ss; at 0 the bubble collapses (no animation under reduced motion) to “🔥 Message read and deleted” on both sides.
- Placeholders: “🔥 Photo opened”, “🔥 Message read and deleted”, “Expired unread”. Save / forward / quote / copy disabled on burn bubbles (long-press menu shows them disabled).
- Screenshots: **superseded v0.1.5** — no notices; screenshots are blocked app-wide (§5.57).

### 5.52 View-once viewer — `burn-viewer` (+`--ios`, `--ios-recording`, `--fr`)
Full-screen black, close, sender, “🔥 View once”, ring + seconds. Footer (v0.1.5): “Save, forward and quote are off. Screenshots are blocked, but another device could still photograph the screen.” iOS while captured: “Hidden while the screen is recorded or mirrored”. Closing = gone; second tap shows placeholder (B2).

### 5.53 Chat info timer — `burn-chat-info` (+`--dark`, `--ios`, `--fr`)
Timer after opening: Off · On open (deleted when closed) · 10 s · 1 min · 1 h. Delete unread after: 1 h / 24 h / 7 d. Callout “What it can’t do”. Footnote: never in previews, backups, search, share sheets. Groups: per-member burn; hidden in channels > 256 members (row replaced by “Not available in large channels — use stories”).

### 5.54 Copy (EN / FR)
| Key | EN | FR |
|---|---|---|
| burn.name | Burn after read | Lecture unique |
| menu.desc | Next message deletes itself once read | Le prochain message s’efface après lecture |
| unread | Delete unread after · 1 h · 24 h · 7 d | S’efface sans lecture après · 1 h · 24 h · 7 j |
| bubble.tap | Tap to read | Toucher pour lire |
| bubble.sent | Not opened yet | Pas encore ouvert |
| photo | Photo · view once / 🔥 Photo opened | Photo · voir une fois / 🔥 Photo ouverte |
| burned / expired | 🔥 Message read and deleted / Expired unread | 🔥 Message lu et effacé / Expiré sans être lu |
| honest | Burn after read stops saving in the app. It can’t stop someone photographing the screen with another device. | Lecture unique empêche la sauvegarde dans l’app, pas une photo de l’écran prise avec un autre appareil. |
| notif | 🔥 Burn-after-read message | 🔥 Message éphémère |

### 5.55 Accessibility & platform
- Countdown ring is `role=timer`; screen readers announce remaining time on open, every 5 s (≤ 10 s timers: every second for the last 3), and “Message deleted” at 0 (polite live region). Longer timers (1 min / 1 h) announce on open and at 10 s.
- Sealed bubble label: “Burn after read message from Alex. Double-tap to read. Timer starts when opened.”
- Reduced motion: no ring sweep animation (number updates only), no dissolve on delete.
- Ring + number never colour-only; amber has text label 🔥.
- Android: `FLAG_SECURE` app-wide (v0.1.5); content excluded from Auto Backup, search, share targets. iOS: secure-capture layer + `UIScreen.isCaptured` cover (no screenshot notification); content in in-memory store; excluded from iCloud backup and Spotlight.

## 5G. v0.1.5 — the maintainer’s decisions (REQUIREMENTS § Maintainer decisions, 2026-10-24)

Generator `/workspace/omcp-v015/gen.py` (supersedes v0.1.4 generator; same helpers).

### 5.56 Notification changes (Decisions 1–3)
- **Private calls:** title `Hopla`, body `Incoming call` → rendered “Hopla · Incoming call” / « Hopla · Appel entrant ». No caller name, no avatar. Everything else: “Hopla · New message”. `notif-lockscreen--private`, `--private-ios`, `--private-fr-ios`, `--shade-private`.
- **Per-chat opt-out:** a chat may turn Private **off** while the global toggle is on. Caption: “Off for this chat. Your default is On.” / « Désactivé pour cette discussion. Votre réglage général : activé. » Preview card then shows sender + text. `chat-info-notifications--opt-out` (+`-dark`, `-ios`, `-fr`). Replaces the v0.1.4 “privacy only tightens” rule.
- **Android Hide on lock screen:** always `VISIBILITY_PRIVATE` + public version “Hopla · New message” (never blank, no SECRET). `notif-lockscreen--hidden`.

### 5.57 No screenshots anywhere (Decision 5) — `settings-privacy`, `app-switcher-cover`
- Android: `FLAG_SECURE` on every window → screenshots, recordings, casting and recents thumbnail are black. Recents keeps the system app label/icon above a black card (`app-switcher-cover--android`).
- iOS: sensitive views inside the secure-capture layer (blank in captures). App switcher: blurred brand cover — Hopla icon on teal radial gradient (`app-switcher-cover`). While `UIScreen.isCaptured`: full-screen cover “Hopla is hidden while your screen is recorded or mirrored · Stop recording to continue.” (`--ios-recording`, `-fr`).
- **All screenshot notices removed** (📸 row and Android info row; `burn-bubbles--screenshot-*` deleted). Sender is never told; there is nothing to tell.
- Settings › Privacy › Screen: **Screenshot protection · Always on** — “Screenshots and screen recordings of Hopla come out black or blank, and the app switcher shows a cover. Another device can still take a photo of your screen.” / « Les captures et enregistrements d’écran de Hopla sont noirs ou vides, et l’aperçu des apps récentes est masqué. Un autre appareil peut quand même photographier votre écran. » Footnote: cannot be turned off. *(v0.1.6: superseded — user toggle, §5.70.)*
- Banned copy: “screenshot-proof”, “impossible to capture”, “100 % private”.

### 5.58 Nearby premium — session, area mode, radar, instant start (Decision 4) — v0.3
**Go Nearby session** — `nearby-session` (+`--live`, `--ios-live-activity`, `--ios-island`, `--ios-island-expanded`, `--android-ongoing`, `--android-ongoing-dark`, `--dark`, `--ios`, `--fr`, `--fr-live-activity`)
- Sheet (rev. v0.1.8): radar glyph, “Go Nearby”, “You stay visible until you turn Nearby off”, auto-pause rules, **Go Nearby**. See §5.75.
- iOS Live Activity / Dynamic Island: see §5.75 (active and paused states; counts only, never names or photos).
- Android: foreground service (`connectedDevice`) ongoing notification — see §5.75 (actions Open · Turn off).
- In-app **status chip** (header, right): green dot “Live nearby”, grey “Paused”, or pin “Area mode”. Tap → Nearby sheet. No OS-limitation warnings anywhere.

**Area mode** — `nearby-area` (+`--sheet`, `--dark`, `--ios`, `--fr`): outside a session, gateway + coarse geohash cell; sheet explains in one sentence, Area mode toggle, radius (< 1 km / < 5 km / Area), Go Nearby. Matches/likes arrive as normal notifications (NX3 ≤ 5 min).

**Live radar empty** — `nearby-radar` (+`--dark`, `--reduced-motion`, `--live`, `--ios`, `--fr`): “Nobody nearby yet · We’re quietly looking around you.” Concentric rings, soft sweep (4 s linear loop, 14 % violet), centre pulse (2.4 s ease-in-out, scale 1→1.15, opacity .10→0). No fake blips. CTAs: Go Nearby · 60 min + Widen radius (during a session only Widen radius). Reduced motion: no sweep, no pulse, static rings; status text changes are announced politely.

**Instant start** — `nearby-skeleton` (+`--blurhash`, `--resolved`, `--dark`, `--blurhash-dark`)
| t (from tab open) | State | Rule |
|---|---|---|
| 0 ms | Cached card painted from local store (skeleton only if cache empty) | No spinner, ever |
| ≤ 120 ms | Skeleton shimmer starts *only* if still empty (1.2 s linear loop; off under reduced motion) | Avoid flash: skeleton shown ≥ 200 ms or not at all |
| ≤ 300 ms | Card text + blurhash photo | Buttons enabled immediately |
| ≤ 900 ms | Sharp photo cross-fades over blurhash (240 ms ease-out) | NX1: first card < 1 s |
| > 1 s, no cache | Radar empty state | — |

### 5.59 Cards, match and motion spec
Card (refined): 28 px radius, photo 372 px with page bars, frosted flag button, bottom scrim with name 26/800, age range, frosted chips (distance bucket, intent); body: bio + interests, shared ones marked ✦. `nearby-discover` (+ states), `nearby-match` (glow, overlapping avatars, icebreaker chips → prefill composer).

| Motion | Duration | Easing | Reduced motion |
|---|---|---|---|
| Drag follow | 1:1 with finger, 60/120 fps | — | Buttons only animate |
| Card tilt | max 7° at 40 % width | linear to drag | none |
| Like / Pass fling | 280 ms | cubic-bezier(.2,.8,.2,1) | 150 ms cross-fade |
| Snap back (cancel) | 360 ms | spring (damping .8, response .35) | 150 ms fade |
| Next card rise (scale .96→1, y 8→0) | 220 ms | cubic-bezier(.2,.8,.2,1) | instant |
| Button press | 90 ms scale .92 | ease-out | colour only |
| Match: avatars slide in from sides | 420 ms | spring (damping .7, response .45), 60 ms stagger | 200 ms fade |
| Match: heart pop (0→1.15→1) | 320 ms | cubic-bezier(.34,1.56,.64,1) | static |
| Match: glow breathe | 2.4 s loop ×2 then stop | ease-in-out | none |
| Sheet in / out | 300 / 220 ms | cubic-bezier(.2,.8,.2,1) / ease-in | fade 150 ms |
| Haptics | light on Like/Pass commit, success on match, none on drag | iOS UIImpactFeedback / Android `CONFIRM` | unchanged (system setting) |

### 5.60 Copy (EN / FR) — v0.1.5 keys
| Key | EN | FR |
|---|---|---|
| chip.live / paused | Live nearby / Paused | Visible à proximité / En pause |
| chip.area | Area mode | Mode secteur |
| go.title / cta | Go Nearby / Go Nearby | Go Nearby / Go Nearby |
| go.body | Discover people around you live, even with your phone locked. | Découvrez les gens autour de vous en direct, même téléphone verrouillé. |
| la.counts | people nearby · new likes | personnes autour · nouveaux likes |
| radar | Nobody nearby yet · We’re quietly looking around you. New profiles will appear here. · Widen radius | Personne pour l’instant · On cherche en douceur autour de vous. Les nouveaux profils apparaîtront ici. · Élargir le rayon |
| area.title | Area mode keeps going | Le mode secteur reste actif |
| ice | Break the ice | Pour briser la glace |
| privacy.ss | Screenshot protection · Always on | Protection contre les captures · Toujours active |

A11y: chip is `role=status`; Live Activity/Island read “Live nearby, 5 people nearby, 2 new likes”; skeleton is `aria-busy` with “Loading nearby profiles”; radar has a text alternative.

## 5H. v0.1.6 — Moderation & reputation (PROTOCOL §23, **v0.2**) and Block-screenshots toggle

Generator `/workspace/omcp-v016/gen.py`. All moderation frames carry a blue **v0.2** tag. Tone: calm, explanatory, never shaming. Every banner says *why* and *what happens next*. Colours: review = `--path-mixed`, Reported = `--warn` (amber), Restricted = neutral `--ink-2`; never `--danger` for a community state.

### 5.61 Report sheet — `mod-report` (+`--sent`, `--limit`, `--dark`, `--ios`, `--fr`)
Reasons: Illegal content · Harassment or hate · Scam or spam · Unwanted sexual content · Violence · Something else. Anonymity line: sent under the per-community pseudonym (§23.2; MR6). Sent: “Your report adds to others… several independent people need to report the same thing.” Limit: 10/day explained as protection against coordinated campaigns. Reporter weight is never shown.

### 5.62 Vouch + reputation indicator — `mod-community` (+`--vouch`, `--vouched`, `--reputation`, `--dark`, `--ios`, `--fr`)
- Vouch (♥) in the community header; sheet: signed by community pseudonym, once per 30 days, weight depends on network trust (EigenTrust). Vouched state = secondary button, `aria-pressed=true`.
- Reputation indicator: 4-bar pill with a word (New community / Building trust / Well supported / Strongly supported), mapping R bands 0–.25/.25–.5/.5–.75/.75–1. Never a number, never member names. Explainer sheet lists the 5 inputs (qualitative bars), 14-day smoothing, “higher reputation → more reports needed”, and “never protects against known-illegal-content matching”.

### 5.63 State banners — `mod-states` (+`--reported`, `--reported-why`, `--restricted`, `--reported-dark`, `--restricted-dark`, `--ios`, `--fr`)
| Stage (§23.4) | Who sees | UI |
|---|---|---|
| Under review (≥ 0.5·T) | **Admins only** | Violet banner “Some reports came in. Nothing changes for members.” + See details |
| Reported (≥ T) | Everyone | Amber banner “Photos and videos are paused for now. Text is still readable. The owner can appeal.” Media → “Media paused” placeholder. Insiders get counter-signal “It’s fine” (weighted, capped 50 %). Why? sheet: what changes / what happens next (14-day decay, appeal). |
| Restricted (≥ 3·T + upheld by followed labeler) | Everyone | Neutral banner naming the labeler confirmation; **text only** (media not rendered: “1 photo not shown”); joins closed; no Vouch button. |
| CSAM hash match (§23.1.5) | — | Media blocked + reported regardless of R; per-item placeholder “This media can’t be shown” (no mockup, no appeal path in UI). |

### 5.64 Join warning — `mod-join-warning` (+`--restricted`, `--dark`, `--ios`, `--fr`)
Reported: “Before you join” notice, **Join anyway** / Go back. Restricted: “Joining is closed for now” + Manage my moderation lists.

### 5.65 Stream blur — `mod-stream` (+`--continue`, `--reduced-motion`, `--ios`, `--fr`)
Threshold 0.3·T for host R. New viewers: blurred video, “Blurred for new viewers · Viewers reported it during the stream” + Why? / Leave. Current viewers: “Continue watching?” sheet (Continue watching / Leave stream / Report it too). Blur ramps 0→24 px over 400 ms ease-out; **reduced motion: instant**. Chat stays visible.

### 5.66 Appeal (owner) — `mod-appeal` (+`--pending`, `--cleared`, `--dark`, `--ios`, `--fr`)
Explains jury (15 random verified, independent, never reported/vouched), 72 h, ⅔, once per 30 days, signed aggregate votes. Pending: timeline + “9 / 15 jurors have voted” (direction hidden). Cleared: “12 of 15 votes”, Reported cleared, track record improves, Check the votes. (Upheld result: same layout, neutral icon, “The jury upheld the reports”, next appeal date.)

### 5.67 Jury vote — `mod-jury` (+`--voted`, `--dark`, `--ios`, `--fr`)
“You’re on a jury · Is this community fine?” Context card (main reason, owner statement, reported posts with media blurred), time left, Not fine / Fine (equal weight, Fine primary only for layout consistency), “I can’t judge this (step aside)”.

### 5.68 Copy (EN / FR) — key strings
| Key | EN | FR |
|---|---|---|
| vouch | Vouch / Vouched | Recommander / Recommandée |
| rep.levels | New community · Building trust · Well supported · Strongly supported | Nouvelle · En construction · Bien soutenue · Très soutenue |
| review | Under review | En cours d’examen |
| reported | Reported by participants · Photos and videos are paused for now. Text is still readable. The owner can appeal. | Signalée par des participants · Images et vidéos sont en pause pour l’instant. Le texte reste lisible. Le propriétaire peut faire appel. |
| restricted | Restricted · Only text is shown; new joins are closed. | Restreinte · Seul le texte s’affiche ; les nouvelles adhésions sont fermées. |
| counter | It’s fine | Elle est correcte |
| join | Before you join · Join anyway | Avant de rejoindre · Rejoindre quand même |
| stream | Blurred for new viewers · Continue watching? | Live flouté pour les nouveaux spectateurs · Continuer à regarder ? |
| appeal | Appeal this decision · Send appeal · The jury found your community fine | Faire appel · Envoyer l’appel · Le jury a jugé votre communauté correcte |
| jury | Is this community fine? · Fine · Not fine · I can’t judge this (step aside) | Cette communauté est-elle correcte ? · Correcte · Pas correcte · Je ne peux pas juger (se retirer) |

### 5.69 Accessibility & platform
Banners `role=status` (announced once on entry, not on every scroll). Reputation pill `role=img` “Reputation: Well supported, 3 of 4”. Blurred media has text alternative “Media paused”. “Continue watching?” is `alertdialog` with focus on Continue. Jury buttons equal size, no colour-only meaning. Android: banners as Material `Card`; iOS: inset grouped section header. Reduced motion: blur and banner slide-ins become instant/150 ms fades.

### 5.70 Settings › Privacy › Block screenshots (the maintainer, v0.1.6) — `settings-privacy` (+`--dark`, `--ios`, `--confirm-off`, `--confirm-off-dark`, `--confirm-off-ios`, `--off`, `--off-dark`, `--off-ios`, `--fr`, `--confirm-off-fr`, `--off-fr`)
- Toggle **Block screenshots**, default **On** (= §5.57 behaviour). On copy unchanged (honest second-camera line).
- Turning off → confirm sheet “Allow screenshots?”: app switcher will show chats; captures may land in photo backups; contacts aren’t notified; burn-after-read and view-once stay protected. Allow screenshots / Keep blocking (default focus).
- Off: Android clears `FLAG_SECURE` on normal windows; iOS removes the secure-capture layer and app-switcher cover. **Burn viewer, opened burn text and view-once media keep `FLAG_SECURE` / secure layer / capture cover.** Off copy: superseded by §5.71 (v0.1.7).
- Supersedes §5.57’s “cannot be turned off” footnote.

| Key | EN | FR |
|---|---|---|
| toggle | Block screenshots | Bloquer les captures d’écran |
| off.desc | *(v0.1.7 → §5.71)* Screenshots are allowed. Burn-after-read and view-once content is always protected. Another device can still take a photo of your screen. | Captures autorisées. Les messages éphémères et les médias à voir une fois restent toujours protégés. Un autre appareil peut quand même photographier votre écran. |
| confirm.title | Allow screenshots? | Autoriser les captures d’écran ? |
| confirm.cta | Allow screenshots / Keep blocking | Autoriser les captures / Garder le blocage |

## 5I. v0.1.7 — Approved decisions (maintainer, 2026-10-24)

Generator `/workspace/omcp-v017/gen.py`.

### 5.71 Block screenshots — aligned with REQUIREMENTS Decision 5 / SS1
- Toggle **Block screenshots**, **on by default** (Settings › Privacy). On copy now starts “On by default.”; both On and Off copy keep the second-camera line (Decision 5: the app says plainly that a second camera can still photograph the screen).
- Off copy: “Screenshots are allowed. Burn-after-read and view-once content is always protected. Another device can still take a photo of your screen.” / « Captures autorisées. Les messages éphémères et les médias à voir une fois restent toujours protégés. Un autre appareil peut quand même photographier votre écran. »
- Confirm bullet: “Burn-after-read and view-once content is always protected.” / « Les messages éphémères et le contenu à voir une fois restent toujours protégés. » `settings-privacy` (all 12 frames re-rendered).
- SS1: toggle on → every screen black/blank; toggle off → burn/view-once still black/blank (B3).

### 5.72 App switcher by toggle state — `app-switcher-cover` (+`--android`, `--ios-off`, `--android-off`, `--ios-recording`, `-fr`)
| Toggle | Android recents | iOS app switcher |
|---|---|---|
| **On** (default) | Black card (`FLAG_SECURE`), system app label | Blurred brand privacy cover |
| **Off** | Normal preview (system snapshot) | Normal preview |
Open burn/view-once screens keep `FLAG_SECURE` / secure layer, so their snapshot stays black/blank even when Off.

### 5.73 Nearby indicator — Private notifications — `nearby-session`
With **Private notifications on**, the Live Activity, Dynamic Island and Android ongoing notification **hide the counts** (“Nearby is on · Visible until you turn it off”). Pause reasons are still shown (they reveal nothing about other people). Session-length rules: superseded by §5.75.

### 5.74 Moderation confirmations
Confirmed (unchanged since v0.1.6): Reported and Restricted follow PROTOCOL §23.4 exactly (§5.63); Under review is shown to **admins only**; reputation uses **four word levels**, no number (§5.62).

## 5J. v0.1.8 — Nearby stays on until turned off (REQUIREMENTS Decision 4 + D6) — v0.3

Generator `/workspace/omcp-v018/gen.py`. Mockups: `nearby-session` (rebuilt), `nearby-checkin` (new), `settings-nearby` (new); `nearby-discover`, `nearby-radar`, `nearby-area`, `nearby-skeleton`, `nearby-tab` regenerated with the new chip / CTA.

### 5.75 Visibility model
- One tap on **Go Nearby** makes you visible **until you turn it off**. No duration picker, no extension action, no countdown anywhere.
- **Auto-pause** (quiet, no alert, within 60 s of the condition — D6) and **auto-resume**:
| Reason | Indicator title | Sub-line | Icon |
|---|---|---|---|
| 2 h without motion | Paused: no motion for 2 h | Resumes when you move | standing figure |
| 01:00–07:00 local | Paused: night (01:00–07:00) | Resumes at 07:00 | moon |
| Battery < 20 % | Paused: battery below 20% | Resumes when charging or above 20% | battery |
While paused nothing is broadcast; the indicator stays (greyed), so the user always knows Nearby is on.
- **Check-in** every 4 h of *visible* time (paused time doesn’t count): notification + in-app prompt “Still up for meeting people?” with **Yes** / **Turn off**. No answer within 15 min → Nearby turns off; a quiet notification and an in-app note say so (“You didn’t answer the check-in, so we turned it off. Your matches and chats are kept.”), with **Turn back on**.
- **Persistent indicator** whenever Nearby is on (active or paused) — D6: iOS Live Activity + Dynamic Island; Android foreground-service ongoing notification (non-dismissible), actions Open · Turn off.

### 5.76 Indicator states — `nearby-session--…`
| State | iOS Live Activity | Dynamic Island compact | Android ongoing |
|---|---|---|---|
| Active | ripple + “Live nearby · Visible until you turn it off”, tiles (people nearby, new likes), Turn off | ripple · “5 ♥2” | “Live nearby” / “5 people nearby · 2 new likes” |
| Active + Private | “Nearby is on”, no tiles, “counts hidden” line | ripple only | “Hopla · Nearby is on” / “Visible until you turn it off” |
| Paused (any reason, normal or Private) | reason icon (grey) + reason title + resume line | reason icon · “Paused” | reason title / resume line (Private: prefixed “Hopla ·”) |
Frames: `--ios-live-activity[-paused-{motion,night,battery}][-private]`, `--android-ongoing[-paused-…][-private]`, `--ios-island[-private|-paused]`, `--ios-island-expanded[-paused]`, dark and FR variants; `--live` / `--paused` chip frames.
Platform: iOS Live Activities are ended by the system after 8 h (12 h on the lock screen); the app restarts the activity on each check-in “Yes” and on resume, so the indicator never lapses (D6). Android uses `FOREGROUND_SERVICE_CONNECTED_DEVICE`, `setOngoing(true)`, low-importance channel “Nearby status”.

### 5.77 Check-in — `nearby-checkin` (+`--android`, `--in-app`, `--in-app-dark`, `--in-app-ios`, `--turned-off`, `--turned-off-dark`, `--turned-off-notif`, `--fr`, `--in-app-fr`, `--turned-off-fr`)
Notification with actions Yes / Turn off and line “No answer in 15 min turns Nearby off.” In-app `alertdialog`: Yes, stay visible (primary, default focus) / Turn off Nearby; time it turns off shown as a plain time (not a ticking countdown; `aria-live=off`). Under Private notifications the check-in body is still generic (no names), so it’s shown as is.

### 5.78 Settings › Nearby — `settings-nearby` (+`--dark`, `--ios`, `--fr`)
Visible nearby toggle (“Stays on until you turn it off”), Auto-pause list (3 rules + check-in), indicator explanation, My profile, Area mode, Safety tips. Rules are informational (not user-configurable in v0.3).

### 5.79 Copy (EN / FR)
| Key | EN | FR |
|---|---|---|
| go.body | You stay visible until you turn Nearby off. An indicator is always shown while you are. | Vous restez visible jusqu’à ce que vous désactiviez À proximité. Un indicateur reste toujours affiché. |
| active.sub | Visible until you turn it off | Visible jusqu’à ce que vous désactiviez |
| pause.motion | Paused: no motion for 2 h · Resumes when you move | Pause : pas de mouvement depuis 2 h · Reprend dès que vous bougez |
| pause.night | Paused: night (01:00–07:00) · Resumes at 07:00 | Pause : nuit (01:00–07:00) · Reprend à 07:00 |
| pause.battery | Paused: battery below 20% · Resumes when charging or above 20% | Pause : batterie sous 20 % · Reprend en charge ou au-dessus de 20 % |
| checkin | Still up for meeting people? · Yes · Turn off · No answer in 15 min turns Nearby off. | Toujours partant·e pour faire des rencontres ? · Oui · Désactiver · Sans réponse sous 15 min, À proximité se désactive. |
| off | Nearby is off: no answer to the check-in. · Turn back on | À proximité est désactivé : pas de réponse au point de contrôle. · Réactiver |

### 5.80 Accessibility
Indicator is `role=status`; state changes (paused/resumed) announced politely once, never on a timer. Pause reason is in text, icons are decorative. Check-in prompt traps focus, Yes first; actions ≥ 48 px. Reduced motion: radar/ripple static in paused state; no pulsing indicator dot.

## 5K. v0.1.9 — Area-mode indicator; check-in counts visible time (REQUIREMENTS Decision 4, Q12/Q13) — v0.3

Generator `/workspace/omcp-v019/gen.py`.

### 5.81 Persistent indicator in area mode — `nearby-area-indicator`
Area mode (gateway visibility outside Go Nearby) also shows the persistent indicator (D6). Label **Area mode** / **Mode secteur**; glyph = hexagonal cell with centre dot (the coarse geohash cell), tinted `--path-mixed` violet instead of the teal ripple — distinct, calm, no pulse.
| State | iOS Live Activity | Dynamic Island compact | Android ongoing |
|---|---|---|---|
| Active | hex + “Area mode · Visible in your area (~5 km) until you turn it off”, tiles (profiles in your area, new likes), Turn off | hex · “3 ♥1” | “Area mode” / “3 profiles in your area · 1 new like” |
| Active + Private | “Area mode is on”, no tiles | hex only | “Hopla · Area mode is on” |
| Paused | reason icon + “Area mode · Paused: …” + resume line | reason icon · “Area · Paused” | “Area mode · Paused: …” (Private prefixed “Hopla ·”) |
Frames: `nearby-area-indicator` (+`--ios-live-activity[-paused-night][-private]`, `--android-ongoing[-paused-night][-private]`, `--ios-island`, `--ios-island-private`, `--ios-island-expanded`, `--ios-island-paused`, `--android-ongoing-dark`, `--android-ongoing-paused-motion-dark`, `--fr-live-activity`, `--fr-android-ongoing-private`, `--fr-live-activity-paused-night`). Same auto-pause rules and check-in apply. Switching from area mode to Go Nearby morphs glyph hex→ripple (240 ms cross-fade; instant under reduced motion).

### 5.82 Check-in counts visible time only
- The 4 h counter runs only while Nearby (Go Nearby or area mode) is actually visible; any pause stops it. No check-in fires while paused (e.g. overnight).
- Copy: Settings › Nearby “Check-in every 4 h of visibility — Counts visible time only, not pauses. Next one after 2 h 40 min more visible time.”; Go Nearby sheet “After every 4 h of visible time, we check you’re still up for it”; in-app prompt “You’ve been visible for 4 h (pauses not counted).”
- The remaining visible time is shown **only** in Settings › Nearby (static, rounded to 10 min), never on the lock screen or as a ticking countdown.
| Key | EN | FR |
|---|---|---|
| checkin.rule | Check-in every 4 h of visibility | Point de contrôle toutes les 4 h visibles |
| checkin.desc | Counts visible time only, not pauses. Next one after {t} more visible time. | Compte seulement le temps visible (pas les pauses). Prochain dans {t} de visibilité. |
| area.title / sub | Area mode · Visible in your area (~5 km) until you turn it off | Mode secteur · Visible dans votre secteur (~5 km) jusqu’à désactivation |

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
| Unverified | Not verified yet | Pas encore vérifié |
| Waiting for mesh | Waiting for mesh | En attente du mesh |
| Via neighbour Internet | Sent via neighbour’s Internet | Envoyé via l’Internet d’un voisin |
| Gateway help | Help nearby peers reach the Internet | Aider les pairs proches à rejoindre Internet |
| Notification default | New Hopla message | Nouveau message Hopla |
| Not an OMCP code | Not a Hopla contact code | Ce n’est pas un code contact Hopla |
| Safety mismatch | Safety numbers don’t match | Les numéros de sécurité ne correspondent pas |
| File too large | {x} MB is over the 50 MB limit | {x} Mo dépasse la limite de 50 Mo |
| Transfer paused | Paused · out of range | En pause · hors de portée |
| BLE-only live | Video needs Wi‑Fi or Internet | La vidéo nécessite le Wi‑Fi ou Internet |
| Recovery | Write these 24 words down… | Notez ces 24 mots… |

Keep FR translations reviewed by a human before store submission; table is design intent, not final L10n freeze.

---

## 8. Platform notes

### 8.1 Android 12+

- **Foreground service** for mesh participation with a **persistent notification** explaining BLE use (user-visible, not dismissible while mesh is active). Suggested text: “Hopla is using Bluetooth so you can chat nearby.” Action: Open / Pause mesh.
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
| `VerifiedBadge` | Filled mint seal + check (§5.32) | Icon / IconButton in header | Image / Button in header |
| `VerifyGhostSeal` | Dashed muted seal, header only (§5.33) | IconButton | Button |
| `VerifyHintRow` | Slim dismissible hint + Verify (§5.33) | Row | HStack |
| `SafetyNumberBlock` | 12×5 digit grid | — | — |
| `MessageBubble` | Text + meta | — | — |
| `FileTransferCard` | Sending / receiving / paused / queued / failed / done | — | — |
| `ComposerBar` | Attach + field + send | — | — |
| `GatewayDisclosure` | First-enable sheet | Dialog / ModalBottomSheet | sheet |
| `ToggleSettingRow` | Title, desc, switch | — | — |
| `Callout` | Info / warn banners | — | — |
| `EmptyState` | Icon + title + body + CTA | — | — |
| `PrimaryButton` / `DangerButton` | 48 pt height | — | — |
| `TabRoot` | 4 tabs | NavigationBar | TabView |
| `RecoveryPhraseGrid` | 24 words once | `LazyVerticalGrid` + `FLAG_SECURE` | `LazyVGrid` + snapshot blur |
| `AppLogo` | Placeholder slot until hopla-v2 | — | — |
| `OnboardingStepper` | 5-step progress | — | — |
| `PhraseWordField` | Word entry + autocomplete chips | `TextField` + chip row | `TextField` + chips |
| `PermissionCard` | Pre-prompt explainer | — | — |
| `BlockingExplainer` | BLE denied / camera denied | — | — |
| `ContactAddOption` | Chooser row | — | — |
| `MyQrCard` | Own QR + PeerID short | ZXing encoder | CoreImage `CIQRCodeGenerator` |
| `ContactPreviewSheet` | Scan result + safety number + mismatch | `ModalBottomSheet` | `.sheet` |
| `FileSendConfirmSheet` | Size/type/route/limit | `ModalBottomSheet` | `.sheet` |
| `QrScannerScreen` | Contact add | CameraX | AVFoundation |
| `LiveStreamCard` | v0.2 beacon / live | — | — |
| `RoleChip` | Community role | — | — |
| `AdminToolsList` | v0.2 sketch | — | — |
| `BackupMethodOption` (v0.1.2) | Multi-select backup method row, Recommended badge | `Row` + `Checkbox`, `toggleable()` | `Toggle` with `.toggleStyle(.checkbox-like)` row |
| `BackupPasswordSheet` (v0.1.2) | Password + confirm, zxcvbn meter, iCloud no-password option | `ModalBottomSheet` | `.sheet` with `.presentationDetents([.large])` |
| `BackupStatusCard` (v0.1.2) | Status badge, last backup, Back up now | `Card` | `GroupBox` |
| `BackupReminderBanner` (v0.1.2) | 24 h countdown banner + blocking sheet | `Surface` under `TopAppBar` | `safeAreaInset(edge: .top)` |

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

## 12. Open product questions

Resolved in v0.1.2: v0.1.1 Q1 (skip phrase confirm) — superseded by REQUIREMENTS: no skip on the quiz, but “Remind me later” gives 24 h. v0.1.1 Q2 (unverified chat) and Q3 (50 MB unit) remain open, parked behind:

1. **24 h lock scope:** after the deadline (§5.29), should incoming messages still be received and readable behind the blocking sheet, or is the whole app paused? (Mockup assumes receive continues, UI blocked.)
2. **Pre-selected cloud default vs explicit opt-in:** REQUIREMENTS says “Android default / iOS default” *and* “no cloud backup without explicit opt-in”. Mockups pre-select and badge it Recommended, but nothing uploads until Continue + password. Is a pre-checked box acceptable, or must it start unchecked?
3. **Wrong-password limits:** O4 asks for rate-limited retries; mockups assume 3 tries then 5-minute cooldown (escalating). Confirm numbers with the Rust core.
Resolved in v0.1.5 by the maintainer (REQUIREMENTS § Decisions): Q4 calls → “Hopla · Incoming call”; Q5 per-chat opt-out allowed; Q6 Android lock → “Hopla · New message”; Q7 Nearby must be a pleasure on iOS → sessions + Live Activity + area mode (§5.58); Q8 no screenshots anywhere (§5.57).

Resolved 2026-10-24 (maintainer approved; REQUIREMENTS Decision 5 / SS1 updated by Writey):
- ~~Q4 Live Activity on Private notifications~~ → counts hidden (§5.73).
- ~~Q5 Android recents~~ → black when Block screenshots is on, normal preview when off (§5.72).
- ~~Q6 Session length~~ → superseded 2026-10-24: no time limit; auto-pause + 4 h check-in (§5.75).
- ~~Q7 Block-screenshots toggle vs REQUIREMENTS~~ → REQUIREMENTS now has the toggle, on by default; burn/view-once always protected (§5.71).
- ~~Q8 Restricted text~~ → follow §23 (§5.63).
- ~~Q9 Under review visibility~~ → admins only.
- ~~Q10 Reputation display~~ → four word levels.

Resolved 2026-10-24:
- ~~Q11 SS2 wording vs toggle Off~~ → fixed in REQUIREMENTS (Writey, commit 6a0ee67).
- ~~Q12 Area mode and D6~~ → indicator shows in area mode too (§5.81).
- ~~Q13 Check-in while paused~~ → counts visible time only; paused time doesn’t count (§5.82).

No open questions. (Note: REQUIREMENTS Decision 4 “Honest status” still quotes an old chip example with minutes left; the pack uses “Live nearby” / “Paused” / “Area mode”.)

---

## Document control

| Version | Date | Notes |
|---------|------|-------|
| 0.1.0 | 2026-10-23 | First design pack: tokens, IA, MVP + v0.2 sketches, 6 HTML mockups |
| 0.1.1 | 2026-10-24 | Hopla naming; §5.14–5.24 onboarding, contact add, file transfer; 11 new mockups (17 PNG frames); resolved v0.1.0 questions |
| 0.1.2 | 2026-10-24 | 4-screen onboarding + recovery backup (§5.1, §5.14 updated; §5.25–5.31 new); §5.15/5.16/5.18 deprecated; 8 new + 1 updated mockups (24 PNG frames); new §12 questions |
| 0.1.3 | 2026-10-24 | Verified badge §5C |
| 0.1.9 | 2026-10-24 | Area-mode persistent indicator §5.81; check-in counts visible time §5.82; Q11–Q13 resolved |
| 0.1.8 | 2026-10-24 | Nearby stays on until turned off: auto-pause, 4 h check-in, persistent indicator §5.75–5.80; `nearby-checkin`, `settings-nearby` new |
| 0.1.7 | 2026-10-24 | Approved decisions §5.71–5.74: toggle copy aligned to REQUIREMENTS, private Live Activity/Island, recents by toggle, session length; §12 Q4–Q10 resolved |
| 0.1.6 | 2026-10-24 | Moderation & reputation (§23, v0.2) §5.61–5.69; Block screenshots toggle §5.70; 7 new + 1 updated mockups; §12 Q7–Q10 |
| 0.1.5 | 2026-10-24 | the maintainer decisions: §5.56–5.60 (calls, per-chat opt-out, Android lock, no screenshots, Nearby session/area/radar/skeleton/motion); 6 new mockups; screenshot notices removed |
| 0.1.4 | 2026-10-24 | Notifications §5.36–5.40; Hopla Nearby dating (v0.3) §5.41–5.49; Burn after read §5.50–5.55; 14 new mockups (96 PNG frames); §12 Q4–Q8 |
