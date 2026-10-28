# Auralis v1.0 roadmap and release gates

**Status: development plan, not a release certification.** Written 2026-10-28
against foundation commit `eeba7a9cd77199f6ff4124af7c22c6da06400623`, on
`feat/v1-reliability`. Source observations below describe that baseline; later
fixes need their own reviewed commits and evidence. No checkbox is satisfied
merely by adding this document.

## Product contract

The goal is a dependable, independently branded **native Android 12+ and iOS 17+
OMCP client**: people can exchange private messages and files, recover their
identity, understand what is actually connected, and interoperate with Hopla.
Cloud/browser-based development does not turn the product into a web wrapper.

**Auralis remains free and open source.** No pre-production monetization,
paywalls, advertising/tracking SDKs or income-project dependencies belong in this
roadmap. Any discussion of monetization comes only after production ships and
does not authorize implementation. A separate income project must stay separate.

The instruction to continue until v1.0 is not permission to relabel the current
beta, claim unimplemented features, or silently reduce requested feature parity.
We will stabilize existing native 1:1 functionality first because every later
feature depends on it. **That milestone alone is not v1.0.**

Three distinctions govern the plan:

1. **Implemented Hopla parity:** preserve real upstream functions on both
   platforms, including recovery, backup, privacy and file-recovery behavior;
   parity means functioning workflows, not similar-looking screens.
2. **Outstanding MVP requirements:** groups, gateway delivery, security actions
   and other explicit requirements do not become optional merely because the
   beta never finished them. They remain open release scope below. A narrower
   v1.0 requires an explicit product-owner scope decision, documented with its
   omissions, rather than an engineering assumption.
3. **Later product expansion:** retain the communities/live/Nearby and other
   phased requirements in the roadmap. An unavailable preview is honest UI,
   not delivery of a requested feature. If earlier user-approved parity scope
   includes one of these, it is a release requirement, not automatically deferred
   by the upstream document's version labels.

Sources: [protocol](../PROTOCOL.md), [app requirements](../apps/REQUIREMENTS.md),
[compatibility audit](AURALIS_PROTOCOL_AUDIT.md),
[release operations](AURALIS_RELEASE.md), and
[historical verification](AURALIS_VERIFICATION.md). Later normative clauses
override earlier conflicting prose as described in the audit. Auralis version
1.0 does not mean conformance to every feature mentioned under OMCP's mixed
v0.1/v0.2/v0.3 draft headings.

## What the current implementation proves—and does not

| Surface | Baseline implementation | Still required |
| --- | --- | --- |
| Identity and trust | BIP39 recovery, signing-derived PeerID, device certificates, QR v3 and legacy migration, safety numbers, restore notices | Native recovery failure handling; physical cross-client verification; complete block/reset actions |
| 1:1 chat | Hybrid Noise direct links, encrypted application messages, persistent history/outbox, native BLE adapters | Reliable native write failure/reconnect behavior, bounded hostile input, delivery semantics and rekey review |
| Files | 64 KiB encrypted chunks, 50,000,000-byte cap, hash checks, outgoing persistence/reconnect replay | Bounded receiver allocation, failure/cancel/restart tests, physical maximum-size transfer, honest submission-versus-delivery state |
| Backup/recovery | Encrypted local store, identity backup, text/media backup core, Drive/CloudKit adapters and fake-provider tests | Real account/provisioning tests, remote deletion, interrupted restore and device-key supersession evidence |
| Privacy | Burn text/core media paths, notification policy, capture protections, EN/FR native UI | Real picker/view-once parity, all-screen privacy/accessibility checks, real notification behavior; no forensic-erasure guarantee |
| Groups | OpenMLS primitives and in-memory tests | Persistent native group conversation model, invitations/membership/epochs, interoperable wire integration; PQ extension review |
| Mesh/gateway | Sync/routing simulations, gateway policy and in-memory rendezvous | Relay-blind end-to-end layer, mobile multi-hop/gateway integration, real transport/service and outage tests |
| Remote notifications | Content-free payload models; local notifications | Working registered FCM/APNs mailbox fetch/decrypt and lifecycle tests; iOS extension is a stub |
| Communities/live/Nearby | Requirements, policy models and unavailable surfaces | Complete feature stacks and their security/safety/service acceptance; not working features |

Existing green tests are useful historical evidence, not evidence for later
commits or for hardware/service behavior. Fake backends, loopback, simulator,
syntax parsing, unsigned packaging and signed-device acceptance are separate
evidence levels. Do not promote one into another.

## Phase 1 — make existing 1:1 behavior dependable

These work packages are independently assignable except where noted. Completion
produces a **reliability candidate**, not a final release.

### R1. Bound untrusted input and resource use

Baseline findings:

- `omcp-core/src/ble/fragment.rs` allocates from an authenticated peer's
  `frag_count` and retains an unbounded number of incomplete fragment IDs.
- `omcp-ffi/src/lib.rs` handshake assembly lacks strict chunk ordering/count,
  accumulated-byte and completion-state validation. Repeated chunks can grow
  its buffer; malformed indices must not overflow or panic.
- `omcp-ffi/src/files.rs` accepts small `chunk_plaintext_len` values and allocates
  `num_chunks` slots. A 50 MB size limit does not prevent a 50-million-slot
  allocation when a peer advertises 1-byte chunks. Its two-incoming-files limit
  is per peer, not a complete process-wide memory budget.

Acceptance:

- Validate before allocation; bound per-frame, per-message, per-link and
  process-wide pending bytes/counts; expire incomplete transfers/handshakes and
  release resources on close. Document selected limits and rejection semantics.
- Test zero/maximum counts, skipped/repeated/reordered indices, inconsistent
  totals, oversize PDUs, fragment-ID churn, stale partial messages, same transfer
  ID from different peers, invalid chunk lengths and aggregate transfer load.
- Demonstrate bounded memory and no panic on those tests, while retaining valid
  Hopla handshake/file vectors, zero-length files and maximum valid payloads.
  Receive hardening must not accidentally reject ordinary beta traffic.

### R2. Complete the security centre

The baseline has verification but no complete native/FFI block or session-reset
workflow, although `apps/REQUIREMENTS.md` lists both as MVP requirements.

Acceptance:

- Define and persist blocked identity semantics through legacy contact migration
  and restore; enforce them at dial, handshake, receive, queue/send and media
  re-fetch boundaries—not just by hiding a conversation.
- Confirm the effect on existing queued traffic, active links and retained
  history before a destructive action. An unblock must not silently resend
  cancelled traffic or restore verification that was intentionally cleared.
- Session reset closes affected links and clears the intended session state
  without deleting identity/history or claiming to rotate the peer's keys.
  Reconnection performs a fresh authenticated handshake; superseded keys remain
  refused. Define how reset interacts with pending messages/files.
- Rust tests cover persistence, active and future links, re-fetch and reconnect;
  real-bridge tests cover both platforms; native UI exposes errors rather than
  reporting success for an action that failed.

### R3. Make native transport completion truthful and recoverable

At the baseline, Android `BleMeshTransport.kt` removes queued PDUs before
submission, ignores synchronous GATT rejection and ignores completion status in
`onCharacteristicWrite`/`onNotificationSent`. iOS `BleMeshTransport.swift` also
continues after a failed write whose PDU was already removed. This can strand a
busy queue, lose traffic or desynchronize ordered encryption counters.

Acceptance:

- Handle synchronous rejection, asynchronous failure, timeout, radio-off,
  revoked permissions, unsubscribe, disconnect and process death. No stuck busy
  state, overlapping write or continuing an invalid encrypted session.
- Recover through a defined new-link/application replay contract. Do not blindly
  replay a nonce-bearing fragment on an advanced link. Retain unconfirmed
  application work durably and deduplicate at the recipient.
- Test failure before submission, after submission but before callback, at each
  handshake stage, between file chunks, and at the last chunk. A reconnect must
  neither duplicate visible messages nor silently discard unsent work.
- Test sender and receiver restart independently; cancellation stays cancelled;
  corruption produces an integrity failure and never a usable partial file.
- “Queued,” “submitted/sent,” “received” and “read” have distinct meanings. The
  beta does not provide an end-to-end file ACK. Do not show “delivered” based on
  link submission; a future ACK requires reviewed wire/compatibility design.

### R4. Preserve recovery and privacy while fixing reliability

- Reopen the encrypted store with the correct wrapped key; refuse a wrong key or
  identity without overwriting data. Exercise full disk, corrupt/partial data,
  interrupted migration and interrupted backup/restore; avoid silent in-memory
  fallback when a persistent write fails.
- Cover O1–O6, CB1–CB5 and RS1–RS3 from the app requirements. Restore keeps the
  user's verification of others, creates a new local device key, clears others'
  verification of this account, changes safety numbers and rejects stale QR/key
  proofs. A phrase alone cannot recreate contacts absent a backup.
- Keep 90-day/2-GB sent-media defaults, hash-checked re-fetch, unavailable/resend
  states and the single-message optional note. Never retain/serve burn media.
- Verify B1–B5, N1–N3 and SS1–SS2 against actual supported UI paths. Do not claim
  the app's picker supports view-once video or arbitrary burn files merely
  because a small core-only media test passes. Finish required native media
  paths or record the explicit unresolved parity decision.
- The latest screenshot requirements include a default-on user toggle, with
  burn protection always on. Baseline app-wide protection is not proof of that
  complete toggle behavior. Review iOS secure-layer limitations on real OSes.
- Keep cloud opt-in, disabled/simulated/unconfigured states unmistakable. Real
  configuration must never fall back to a fake successful upload.

## Phase 2 — close outstanding MVP/protocol obligations

These are **open work**, not features excluded from v1.0 by this document.

| ID | Required outcome | Evidence needed before calling it complete |
| --- | --- | --- |
| M1 Groups | Small groups through 32 members, persistent state, invite/join/remove/leave, offline epochs and both native UIs | Multi-device old/new-member tests; removed members cannot decrypt later traffic; restart/concurrent membership recovery; cross-client version contract; independent PQ-extension review before any PQ group claim |
| M2 Relay-blind mesh | End-to-end conversation keys independent of per-hop Noise; integrate reviewed routing/reconciliation/store-forward | §22.7 requires 2-member MLS for 1:1; relays must be unable to decrypt. Three-plus-node physical tests, partitions, duplicates, expiry and restart. Passing existing SY simulations alone does not satisfy this |
| M3 Gateway | Opt-in mobile Internet bridging, pinning, caps/metering, Wi-Fi/cellular policy, authenticated service transport, recovery after outages | All five §13.6 scenarios, real TLS/QUIC/service logs with no plaintext, malicious gateway tests, expired/replayed envelope rejection and accurate path UI |
| M4 Session security | Reviewed handshake construction, rekey at 100 messages or 7 days, replay/skew policy and actual version/suite/feature negotiation | Deterministic transcript/KATs plus independent review; boundary tests at 99/100/101 messages and clock limits; old/new-client interop; unknown-suite/downgrade rejection |
| M5 Contact access | QR and NFC contact-add workflow as required, verification/reset/block parity | Physical NFC/QR acceptance on supported hardware; wrong-code/key/device tests; accessible non-camera recovery path. Manual PeerID entry must not imply possession or verification of missing keys |
| M6 Cloud and notification completion | Real encrypted Drive/iCloud roundtrips, management, local and content-free remote notifications | Provider-side opaque-byte inspection, reinstall/restore/delete, background/permission handling, no plaintext pushes, correct private/burn notification history |

Dependencies matter: M2 must not be implemented by forwarding today's
link-derived application ciphertext or decrypting/re-encrypting at relays. M3
depends on a reviewed end-to-end architecture. Current sealed-sender code is a
classical custom construction, not the specified hybrid RFC 9180 solution; the
MLS-PQ helper is not evidence that group application ciphertext has PQ security.
Protocol design/review and cross-client rollout are required, not algorithm
substitution hidden inside a reliability patch.

If completing these changes would break the deployed beta wire, preserve a
documented compatible path, negotiate the new capability, and fail safely when
unsupported. Follow §16 extension/version rules. Resolve conflicting draft
language with an explicit reviewed decision rather than silently choosing the
easiest interpretation.

## Phase 3 — retained product expansion, not simulated functionality

Upstream explicitly phases the following after its MVP. They can follow a
properly agreed bounded v1, **unless the user's agreed parity scope requires
them earlier**. Keep that distinction visible in release planning.

- **Communities/channels:** roles, signed administration, topic conversations,
  invitations, offline delivery and moderation. Acceptance: §20.6(1), membership
  confidentiality, role authorization and persistent state on both platforms.
- **Livestreams:** genuine LAN/SFU encrypted media and key distribution;
  §20.6(2–3), with a BLE-only status/reason, never fake video or media keys at SFU.
- **Stories, polls, stickers, threads:** defined content types, retention,
  authorization, cross-client fallback and mobile feature tests.
- **Nearby dating:** explicit opt-in/18+ gating, unlinked rotating identity,
  coarse location, block/report and confirmed reveal; D1–D6/NX1–NX3 including a
  physical locked-iOS session. No claim a Live Activity bypasses OS BLE limits.
- **Moderation/reputation:** §23 MR1–MR6 and later clarifications; governance,
  appeals, abuse testing, hash-program authorization and qualified legal review
  before launch. Never scan private chats or fabricate human moderation.
- **Usernames/bots:** scoped authorization and optional user-run discovery,
  not a mandatory central directory or privileged plaintext access.

Desktop clients, Tor and voice/video calls are outside the upstream MVP; they
remain explicit future decisions, not silently delivered by policy classes.
General-purpose Internet routing, payments and guarantees of anonymity or
emergency availability are not the product contract.

## Release evidence: exact tests, exact builds

### Automated gates (necessary, never sufficient)

For the final reviewed candidate SHA:

1. `cargo fmt --all -- --check`
2. `cargo clippy --locked --workspace --all-targets -- -D warnings`
3. `cargo test --locked --workspace` including wire-contract, core/FFI,
   persistence/backup, gateway and simulator suites; add the regressions above.
4. `python3 scripts/check_protocol_invariants.py` under an explicitly reviewed
   invariant policy. Its foundation default requires byte-identical upstream
   protocol-bearing source. Intentional fixes therefore require an audited
   change/baseline record; never reset it to current HEAD solely to turn CI green.
5. Android Rust/JNI build, `./gradlew assembleDebug lintDebug testDebugUnitTest`
   in `apps/android`; test release build/lint too. Build the generated bindings,
   not just fake-core UI tests. Add native/instrumented failure tests where JVM
   fakes cannot represent BLE or lifecycle behavior.
6. `swift test` in `apps/ios/HoplaKit`, iOS strings checker/regression tests,
   Rust XCFramework/bindings, complete Xcode app/extension build and simulator
   RustBridgeTests. Linux `verify-linux.sh` is additional evidence, not an Apple
   framework typecheck or device test.
7. All constituent hosted CI jobs and `required checks` green on that SHA;
   both manual unsigned packaging jobs succeed for the same revision. Record
   artifact hashes, tool versions and run URLs before retention expires.

Retain upstream compatibility fixtures and test an independently built pinned
Hopla version against Auralis. Two nodes using the same changed library can
agree on the same bug. New framing/crypto/storage changes need golden vectors,
backward-compatible migration tests and a fresh-context review of the diff.

### Physical interoperability matrix

Record actual models, OS versions, app build IDs/SHAs, pairing direction,
permissions, radio/network state, test steps, elapsed times and observed result.
Use owned/authorized test devices/accounts and synthetic content. Required pairs:

| Auralis endpoint | Other endpoint |
| --- | --- |
| Android | Independently built Hopla Android |
| Android | Independently built Hopla iOS |
| iOS | Independently built Hopla Android |
| iOS | Independently built Hopla iOS |
| Android | Auralis Android |
| iOS | Auralis iOS |
| Android | Auralis iOS |

Test both directions and both relevant central/peripheral roles; record the
pinned Hopla baseline and any newer release actually supported. Test Android 12
and iOS 17 as minimum supported OSes plus current supported OSes/OEM variation.
The upstream SS1 matrix mentions Android 10–15 even though the app minimum is
12; document that mismatch rather than claim installation below the app minimum.

For each supported pair, require:

- Fresh setup, mutual QR add, verify/unverify, wrong/stale QR, blocked peer,
  session reset and a staged adversarial identity/MITM attempt.
- §13.6(1): Airplane Mode with BLE enabled, text and a 2 MB file at 10 m indoors;
  record actual time/range rather than inventing a latency guarantee.
- Zero-byte, chunk-boundary, 2 MB and 50,000,000-byte files; oversized rejection,
  hash corruption, cancel, receiver low-storage, restart and reconnect. Measure
  peak memory and transfer time; retain resource-budget pass/fail evidence.
- Queue while absent; enter/leave range; disable/re-enable Bluetooth; kill and
  restart either app; foreground/background/locked and OS-killed states. Run a
  documented repeated-interruption sequence, not one happy-path reconnect.
- Recovery with a changed device key, retained verifications of others, contact
  restore notice after authentic sync, refusal of superseded keys and stale QR.
  RS1's five seconds is measured **after sync**, not from an unreachable restore.
- Burn timer/once/expiry and OPENED receipt behavior; no sensitive previews,
  backups/share exports or unprotected app-switcher content. A database scan
  does not prove forensic erasure from wear-levelled flash or prevent a camera.

Gateway and multi-hop are separate physical topologies: reproduce §13.6(2)'s
three phones/only middle online/outer message within 60 seconds, §13.6(3)'s
Internet loss with local mesh continuing, and a relay-only intermediate peer
that cannot read either endpoint's message. Add return-of-Internet, offline
expiry and malicious/blocked relay cases. A direct two-phone demo cannot count.

### Real service, UX and operational gates

- Drive `appDataFolder` and CloudKit private database: O1–O6, text/media restore,
  rate limiting, wrong password, offline retry, revoked consent, quota/full
  storage, interrupted uploads and API-confirmed remote deletion. Inspect
  uploaded bytes for opaque encrypted/padded content, never log real phrases.
- O1: cloud-backup onboarding in at most four screens/60 seconds under recorded
  normal online conditions; offline/unconfigured service takes an honest path.
  O6: the 24-hour postponement boundary survives restart/clock handling.
- EN/FR, light/dark, TalkBack/VoiceOver, large text, contrast, focus and reduced
  motion; every permission denied/revoked; Back/Cancel/repeated taps and restored
  navigation; screen-reader recovery quiz and confirmation flows.
- Signed install/upgrade/uninstall/reinstall on both platforms, separate Hopla
  and Auralis sandboxes, application/extension entitlements and backup/keychain
  accessibility. Identical backup formats do not grant access to Hopla's cloud
  container or migrate its local data.
- Review privacy disclosures, supported versions/known limitations, licenses,
  upstream attribution, crash/log redaction, dependency inventory and security
  advisories. Resolve protocol-license ambiguity with its owners. Establish
  support/security-reporting and rollback/recovery instructions.

## Gates needing people, devices or external authority

Code and cloud CI can progress independently. The following cannot honestly be
marked passed by source changes or fabricated results:

| Gate | Exact action / target | Why required; current evidence limit |
| --- | --- | --- |
| Physical tests | Arrange authorized Android/iPhone devices running the candidate and pinned Hopla builds; execute the matrix above and retain reports | No physical test result is established by this roadmap; browser/cloud execution cannot prove radio, battery or locked-background behavior |
| Android distribution | Owner supplies a release-signing process/keystore outside git; register the Auralis app and relevant OAuth signing certificates; authorize the intended distribution channel | Existing workflow builds unsigned review artifacts, not a signed public release |
| Apple distribution | Owner provisions `chat.auralis.app`, `chat.auralis.app.notifications`, `iCloud.chat.auralis.app`, team signing/profiles and matching capabilities; validate a signed archive/export | Source entitlements do not create registrations, provision CloudKit or grant signing authority |
| Cloud and push services | Register Auralis-specific Google OAuth; provision CloudKit schema; implement/configure gateway/mailbox and FCM/APNs registration before claiming remote delivery; test authorized accounts | Fake-provider tests and locally stored push tokens do not prove live services. Do not request private keys/passwords in chat |
| Independent security review | Engage an independent qualified reviewer for the final protocol/core/native-storage/recovery/transport design; provide exact source, threat model, vectors and findings; remediate and retest | The audit in this repo is a source review, not independent cryptographic assurance. Static ML-KEM reuse, MLS-PQ, sealed addressing and negotiation need explicit conclusions |
| Release approval | Product owner confirms remaining feature scope and release channel; authorized maintainer reviews candidate/evidence, signing and publication | Development permission is not permission to merge, tag, publish, deploy services, incur fees or submit to stores |

External items are **pending/unverified**, not denied: this document records no
attempt to provision, sign, publish or access devices. If a tool later denies an
operation, record its exact action, target, reason and confirmed execution state;
do not bypass it or repeatedly retry an uncertain side effect. Continue independent
authorized coding while requesting only the specific missing access/decision.

## Final v1.0 decision record

Before announcing v1.0, store a release record containing:

- [ ] Approved scope and explicit unresolved/deferred parity decisions; no
  required feature left represented only by a placeholder or test fake.
- [ ] Candidate commit, supported Hopla versions and reviewed wire/storage
  change list; no unreviewed changes after acceptance.
- [ ] Full exact-SHA CI and release-packaging evidence, signed artifact hashes,
  installation/upgrade checks and provenance/dependency records.
- [ ] Physical cross-client, failure-recovery, privacy/accessibility and live
  provider results; every skipped test called out, not counted as passed.
- [ ] Independent security findings and dispositions; no unresolved critical or
  high-risk issue affecting the supported release contract.
- [ ] Operational/service ownership, disclosures, recovery/rollback/support and
  required distribution/legal approvals.
- [ ] Explicit maintainer release decision and separately authorized publication.

A failing mandatory gate means **not ready**. An unrun gate means **not verified**.
An owner-approved scope change must be visible to users; it cannot erase a
security defect in a shipped feature. Passing CI, producing an APK/archive, or
changing version numbers to `1.0.0` is never by itself completion.
