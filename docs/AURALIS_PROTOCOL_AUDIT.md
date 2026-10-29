# Auralis: OMCP interoperability and implementation audit

> Historical foundation audit: implementation observations below describe the original audited baseline. Subsequent compatible resource/ownership hardening is documented in [RESOURCE_LIMITS.md](RESOURCE_LIMITS.md) and admitted only by the explicit reviewed-hash manifest. This is not independent security certification.


Reviewed 2026-10-28 against upstream commit
`233210d5b13f443798428d126266c520999ad387` (`open-mesh-chat`, chat-backup merge).
This is a source review and regression-test addition, **not an independent
cryptographic audit, a production-readiness certification, or a hardware test**.

## Decision

Auralis can have its own visual identity while using the same OMCP Rust core,
FFI, discovery service, identity format and application messages as Hopla.
That is the lowest-risk route to compatibility with the existing Hopla beta.
It does **not** make the beta a complete implementation of every requirement in
`PROTOCOL.md` or `apps/REQUIREMENTS.md`.

The current deliverable should be described as an **experimental, compatible
client of the Hopla beta implementation**, pending cross-build and physical-device
verification. Do not describe communities, livestreams, multi-hop encrypted
delivery, Internet gateways, or post-quantum MLS group messaging as working
features simply because their specifications or supporting primitives exist.

## Review basis and precedence

The entire `PROTOCOL.md` was read, including its late §21 wire pins, §22.7
superseding decisions and §23.8 clarifications. Also inspected: repository and
platform READMEs, app requirements, shell-history notes, storage/chat-backup/sync
documents, MLS-PQ extension, core framing/handshake/device/BLE/burn/file/MLS/
gateway/rendezvous/sync code, FFI integration and existing acceptance tests.
No checkout `AGENTS.md` or `.agents/skills` instructions were present.

Important precedence rules:

- §21 explicitly replaces ambiguous early envelope and hybrid-handshake wording.
- §22.7(9–12) supersedes earlier spray-and-wait, metadata and refresh decisions.
- §3.4 v0.1.4a replaces legacy X25519-as-PeerID with a signing-key-derived PeerID.
- §19.1 makes outer content type `0x02` mean encrypted `InnerV1`; the early §6.3
  table still lists `0x02` as Markdown. Do not implement that older interpretation
  on the current beta wire.
- READMEs are historical in places: the root roadmap says M2–M4 are planned,
  old app docs call message storage in-memory, and old notification tables say
  previews are off. Current source and later focused docs establish that these
  statements are stale; none is proof of device acceptance.
- “v0.1” in the title is not a complete compatibility identifier: the document
  also contains v0.1.4b and v0.2.1 requirements. Pin the source commit and the
  implemented feature set, not just the title.

## Compatibility contract: keep these independent of branding

| Surface | Required stable behavior / value | Evidence |
|---|---|---|
| Envelope | `OMCP`, version `1`, suite `0x0001`; network byte order; 16-byte message ID; 1,048,576-byte ciphertext maximum; exact full-buffer decode | `PROTOCOL.md` §10, §21; `omcp-core/src/framing.rs` |
| Flags | `RELAY_OK=1`, `STORE_OK=2`, `PRIORITY=4`; reserved bits zero on send, ignored on receive | §21.3; `framing.rs` |
| Crypto | OMCP-defined hybrid Noise IK, X25519, ChaCha20-Poly1305, BLAKE2b, ML-KEM-768; `OMCP-Noise-Hybrid-v1` HKDF label; transcript binds KEM fields before sealed payload | §21.5/21.7; `handshake/hybrid.rs`, `handshake/symmetric.rs` |
| Identity | PeerID = **BLAKE2b with 32-byte output** over Ed25519 identity public key; not truncation of a 64-byte digest, and not X25519 public key | §3.1/3.4; `device.rs` |
| Device | 209-byte v2 certificate binds identity, per-install Ed25519 device and X25519 DH keys; session-hash signature required; reject superseded devices | §3.4; `device.rs`; FFI `complete_handshake` |
| Handshake extensions | `omcp-ffi-v1`, certificate tag `0x01`, binding tag `0x02`; signature contexts `OMCP-DeviceCert-v2`, `OMCP-DeviceBind-v1` | §3.4; `device.rs` |
| Contact QR | `omcp:3:<peer hex>:<identity hex>:<device hex>:<dh hex>:<name>`; legacy `omcp:1:` / `omcp:2:` accepted through existing migration | §3.4; `omcp-ffi/src/lib.rs` |
| Discovery | 13 bytes: `OMCP | version u16 BE | PeerID prefix 6 | caps u8`; gateway/store/MLS/PQ bits 1/2/4/8 | §8.2; `ble/discovery.rs` |
| Actual beta GATT | Service `4f4d4350-0001-4c01-9e00-000000000001`; Inbox/Outbox/Control replace `0001` with `0002`/`0003`/`0004`; control read exposes discovery bytes | Android and iOS `OmcpGatt`; platform READMEs |
| Beta BLE PDUs | `0x01 | idx u8 | total u8 | chunk` for handshake; `0x02 | link frame` for established traffic; handshake chunk 480 bytes, link plaintext chunk 400 bytes | `omcp-ffi/src/lib.rs` transport documentation |
| Application encryption | Separate application-key HKDF label `OMCP-App-AEAD-v1`; ordered counters; AAD binds suite, sender, message ID, timestamp and content type | `ble/message.rs`, `ble/mod.rs` |
| Inner content | All current inner messages use outer `0x02`; preserve text/media/file/OPENED/re-fetch tags and encrypted burn header | §19.1/7.1; `burn.rs` |
| Burn | Timer 10/60/3600 s; unread TTL 3600/86400/604800 s; OPENED never burns and must originate from recipient; exclude burn content from backup, previews and retained sent files | §19.1; `burn.rs`, FFI tests, storage docs |
| File compatibility | 64 KiB plaintext chunks, BLAKE3 integrity, beta offer carries `transfer_id | content_key | FileMeta` inside encrypted InnerV1; 50,000,000-byte app cap | §7; `files/`; `omcp-ffi/src/files.rs` |
| Re-fetch | `0x31/0x32/0x33`, 16-byte original transfer ID + 32-byte hash; resend request and optional UTF-8 note are a **single** message; note at most 280 bytes | §7.1; `burn::MediaControl` |
| Backup formats | Preserve `OMCP-Backup-v1`, `HCB1`, **`hopla/chat-backup/v1`**, kind/sequence AAD, padding, and current encrypted schema | §11.1; `backup.rs`, `chat_backup/` |
| Routing primitives | 6-hour tags, 8-byte keyed BLAKE2b outputs, TLVs `0x54`/`0x56`, leaf and epoch byte order; 6-byte beacons `0x48`/`0x52`; digest BLAKE2b output size **4**, not truncation | §21.8/21.9/22.7; `sync/tag.rs`, `sync/hello.rs` |

Keep existing KDF labels, magic values, protocol names and schema identifiers
even when they contain “Hopla”. They are format identities, not user-visible
copy. CloudKit zone/record identifiers (`HoplaBackupV1`, `OMCPBackup`, etc.)
also require migration planning before changes. A separate app's sandbox,
CloudKit container and Drive app-data access do **not** automatically provide
access to Hopla's backups even when the encrypted blob formats are identical.

Product display name, icon, colors, typography, layout and explanatory copy can
change without affecting the wire. An Android application ID / iOS bundle ID
change needs deliberate platform entitlement, OAuth, keychain, backup and
co-installation checks; it is not an OMCP protocol change.

## Feature parity: implementation versus roadmap

“Present” below means there is an implementation in the reviewed source, not
that an external service or physical-phone test has passed.

| Feature | Existing Hopla implementation | What Auralis may honestly expose |
|---|---|---|
| Identity, recovery phrase, verification | Real Rust BIP39/identity/cert/QR/safety-number paths; restore changes device keys, preserves identity and invalidates others' verification | Preserve complete flow, legacy migration, restore notice and stale-QR rejection |
| Onboarding and backups | Android/iOS flows; password or iCloud-key encrypted identity backup; Drive/CloudKit adapters; fake-provider tests; provider credentials/entitlements required | Show configured/unavailable states honestly; no fictitious successful cloud backup |
| Direct 1:1 BLE text | Real per-contact FFI links and native GATT adapters; mutual contact add; queue drains on connect; loopback/bridge tests | Real direct chat, queued state, current device limitations; no hardware-range or delivery-time guarantee |
| Persistent chats and files | Encrypted row-level SQLite store, contact/history/outbox/outgoing transfer persistence, platform-wrapped key | Preserve restart behavior; don't rely on the old in-memory-history README |
| Files | Picker/size validation, encrypted chunks, hash-verified receive, reconnect replay; sender completion is link submission, not end-to-end delivery ACK | Progress/pause/retry, integrity failures and precise “sent” terminology |
| Burn/view-once | Core encrypted inner headers, timers, OPENED, crypto-shredding, UI viewer paths; beta file path explicitly rejects burn files | Existing supported burn flows only; do not imply arbitrary file/video/group burn works |
| Chat/media backup | Incremental text and snapshots; optional media backup; missing-media placeholders; one-message resend request; kept sent files default 90 days / 2 GB | Preserve text restore, opt-in media controls, hash checks, expiry and burn exclusions |
| Notifications | Real local policy and global/per-chat privacy controls; content-free push payload models | Local notifications; distinguish remote push placeholders from registered working FCM/APNs |
| Privacy capture controls | Native Android secure windows; proposed supported iOS scene-local privacy covers | iOS ordinary screenshots remain possible; capture transitions/recents need native device validation; see [review proposal](IOS_SUPPORTED_PRIVACY_REVIEW.md) |
| Gateway settings | Policy/caps/pinning and in-memory gateway/rendezvous core; **no mobile gateway transport integration**; FFI only returns Mesh/Queued paths | Settings/policy surface and explanatory empty states, not live bridging or invented data usage |
| MLS groups | Small-group OpenMLS core and in-memory tests; **not connected to current app conversation model** | Explicitly unavailable/roadmap until group UI, persistence and wire integration exist |
| Multi-hop sync | Routing, budgets, receipts, reconciliation, padding and tag primitives + simulation; **not wired into mobile BLE/gateway** | Explain experimental core work; never label simulation as deployed 10,000-peer performance |
| Communities, channels, roles, moderation | Protocol/product design; no corresponding working mobile feature stack | Clearly marked future surfaces; do not fabricate membership, reports or enforcement |
| Livestreams, calls, stories, polls, stickers, threads | Roadmap/policy models only; BLE video explicitly excluded | Not working features; no simulated media disguised as live |
| Nearby dating, usernames, bots, Tor | v0.3 designs / out of current beta scope | Not enabled or advertised as functional |
| NFC, manual PeerID entry, session reset/block | Listed as unfinished in platform docs; no complete FFI block/reset API in reviewed surface | Do not present inert controls as completed security actions |
| Accessibility/localization | EN/FR resources and native UI infrastructure exist | Verify contrast, type scaling, VoiceOver/TalkBack and translations after visual redesign |

Useful distinctions for parity: inherit real functions; retain clear future
states for aspirational functions; improve visual design without silently
inventing implementations or weakening privacy behavior.

## Material gaps and risks

### Release blockers for broad protocol/security claims

1. **End-to-end multi-hop integration is absent.** The beta's application keys
   are derived from its directional Noise link keys (`ble/message.rs` and FFI
   `seal_into`). That works for direct intended peers. Merely adding relay
   forwarding or re-encrypting at each hop would not establish relay-blind E2EE.
   §22.7(5) instead calls for 2-member MLS for every 1:1 conversation; that is not
   the app's current path. Design and test the end-to-end layer before relaying.
2. **Internet sealed sender is not the specified PQ HPKE.**
   `rendezvous/sealed.rs` explicitly implements an HPKE-style classical
   X25519/HKDF-SHA256 box, not RFC 9180 wire format and without ML-KEM. This does
   not satisfy the draft's hybrid Internet-facing requirement. The file-wrap
   helper is hybrid but also custom/non-RFC-9180; beta FFI file offers bypass
   that helper and carry the content key inside the direct encrypted channel.
3. **MLS-PQ is provisional, not group-message PQ protection.**
   `extensions/mls-pq.md` says application messages use ordinary OpenMLS epoch
   secrets. The hybrid helper tests use a complete list of shared secrets that
   individual recipients do not naturally possess; KEM keys are out of band,
   and the extension is not bound into production MLS commits/key packages.
   Do not claim post-quantum group ciphertext or full group PCS from that test.
4. **Handshake specification and implementation need independent review.**
   §21 describes ephemeral ML-KEM material; the implementation uses the
   install's seed-derived static ML-KEM key across sessions (`StaticIdentity`,
   `OwnDevice::static_identity`). Classical ephemeral DH is present, but its
   PQ forward-secrecy/compromise properties need review rather than inference.
   Generic Noise-HFS, libp2p Noise or a standard HPKE implementation is not a
   drop-in wire substitute. Existing randomized agreement/tamper tests are
   valuable but not independent deterministic full-handshake KATs.
5. **Negotiation is not a mobile wire protocol yet.** `negotiate::Hello` says
   wire encoding is deferred; the FFI handshake carries device proofs but no
   serialized version/suite/feature Hello. The 6-byte sync HELLO is a different
   control message and must not be confused with feature negotiation.
6. **Real transport/service acceptance is unestablished.** The gateway test uses
   an in-memory rendezvous, not deployed TLS/QUIC. Platform notes identify BLE,
   background behavior and Google/Apple flows as untested on actual services or
   phones. There is no basis for claims about outage survival or disaster use
   until the physical scenarios pass. Root README itself warns against
   life-critical reliance.

### Compatibility and hardening issues to track without silently changing wire

- **BLE UUID conflict:** early §8.1 and `ble/discovery.rs` retain an invalid
  placeholder UUID. Both apps use the valid beta UUID in the contract above.
  Independent clients need that beta convention documented normatively.
- **Unspecified beta transport details:** FFI handshake chunking, fixed fragment
  sizes, file-offer key placement and mutual-contact admission are documented
  in source, not fully in `PROTOCOL.md`. Spec-only reimplementations may fail
  to interoperate even when their high-level algorithms match.
- **Safety-number mismatch:** early §3.3 says SHA3-256; legacy `safety.rs` uses
  BLAKE2b-512 with an `OMCP-Safety-v1` label. Current v2 behavior uses device keys
  and the later §3.4 rule. Copying early prose alone gives incompatible numbers.
- **Reserved send flags:** `EnvelopeV1::encode` currently serializes reserved
  bits if callers set them; it does not enforce the §21.3 send restriction.
  Current constructors set only known bits. The new test protects permissive
  receive behavior; it deliberately does not normalize this issue into a new
  wire rule.
- **No observed periodic app rekey mechanism:** FFI established links track
  nonce counters, but source review did not find enforcement of §11's 100
  messages / 7 days session rekey requirement. Verify before making that claim.
- **Burn expiry is not carried in BLE EnvelopeV1:** endpoints enforce unread
  expiry; `OpaqueEnvelope` has expiry for gateways. Future BLE store-forward
  needs a compatible way for relays to enforce expiry without revealing burn.
- **Restore discovery:** a restored device changes DH keys; contacts must be
  dialed by it or rescan its new QR before they can connect with new keys.
  Keep this behavior visible rather than promising instantaneous remote notice.
- **Media locators are two different concepts:** §7.1 uses a 16-byte original
  transfer ID for partner re-fetch; §22.7 uses a keyed opaque BLAKE3 locator for
  relay indexing. Do not replace one with the other or expose plaintext hashes
  in public relay indexes.
- **Metadata is not anonymity:** M4 blinded labels are derivable by anyone who
  knows the recipient public key; the signed fetch token is visible to the
  rendezvous service. §22 secret-derived labels are not yet authorized by that
  server. “Gateway never sees plaintext content” is narrower than “nobody can
  identify the recipient.”
- **Burn erasure is best-effort on flash:** storage docs acknowledge old wrapped
  keys in freed filesystem/journal/wear-levelled blocks. A SQLite byte scan is
  useful evidence but not a guarantee against forensic flash recovery.
- **Resource/ordering tests remain important:** malformed fragment counts,
  repeated fragments, incomplete handshakes, nonce ordering, transfer limits,
  timestamp skew and peer churn should be fuzzed and exercised. The current
  source review is not a completed adversarial resource audit.
- **Reproducibility:** upstream dependencies are mostly exactly pinned but
  `serde` is broad; upstream ignores `Cargo.lock` and stable Rust floats. The
  Auralis task now retains the generated lockfile and pins Rust to 1.99.1.
  These improve repeatability but do not establish reproducible mobile builds.
- **Moderation and Nearby are not ready launch surfaces:** §23.8 explicitly
  leaves governance/legal review open; no review of those policies or production
  implementation has been performed here.

## Licensing and attribution

The root `LICENSE` grants MIT terms, copyright 2026 OMCP contributors, and the
workspace crates declare MIT. Preserve that license and copyright notice with
the fork and redistributed substantial source portions; identify Hopla/OMCP as
the upstream project without implying its maintainers endorse Auralis.

The protocol header still says **proposed CC0 / Apache-2.0 dual licensing,
pending owners' final choice**. That is inconsistent repository metadata, not
evidence that a new dual license was finalized. This audit does not relicense
the protocol or delete notices. Record and resolve the ambiguity with upstream
before making distribution/licensing representations beyond the root license.
No trademark or third-party asset permission audit was performed; original
Auralis branding avoids suggesting an official Hopla release.

## Verification added and how to establish interoperability

New test file: `crates/omcp-core/tests/auralis_wire_contract.rs`.
Its 14 focused tests use the public API and cover:

- independent canonical envelope and discovery golden bytes;
- all truncated envelope prefixes, trailing bytes, inclusive ciphertext cap,
  oversized declared length, maximum AAD size and reserved receive flags;
- independent PeerID digest, certificate v2 layout and signed-field tampering;
- burn header bytes, invalid values and non-burning control messages;
- old/new resend forms and the UTF-8 byte limit;
- independent keyed tag/subtag, HKDF rendezvous and beacon-digest vectors;
- unsupported-suite negotiation failure rather than silent fallback.

Golden bytes/digests were independently calculated with Python `struct`,
`hashlib.blake2b(digest_size=...)` and `hmac`/SHA-384. This guards against accidental
branding changes and self-consistent encode/decode bugs. It is not an independent
implementation of the entire cryptographic protocol.

### Evidence status at audit handoff

| Check | Result |
|---|---|
| Full protocol/source review above | Completed |
| Independent Python generation of golden values | Completed |
| `cargo test --workspace` | **Passed: 196 tests, 0 failures, 0 ignored**; 131 core unit, 14 new wire-contract, 4 gateway integration, 38 FFI, 9 simulator acceptance; doc-test targets contain 0 tests |
| New Rust tests | **All 14 passed** in the workspace run and again with `--locked` after formatting on pinned Rust 1.99.1 |
| `cargo fmt --all -- --check` | **Passed** after formatting only the new test file |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | **Passed**; terminal build log inspected at `/tmp/auralis-clippy.log` |
| Protocol source invariant checker | **Passed: 71 files byte-identical to the pinned upstream commit**, including core/FFI/simulator source, specification and extensions |
| Rust core source changes by this audit | None; only a new integration test and this document |
| Physical Android/iOS pair, different client builds | Not run |
| Production rendezvous / Google Drive / iCloud service check | Not run |
| Independent security audit | Not performed |

The workspace run was executed by the root task using its locally provisioned
Rust toolchain; its complete log was inspected at
`/tmp/auralis-cargo-test.log`. Counts above come from all terminal `test result`
lines, not from source test annotations. `git diff` confirmed no changes to
`crates/omcp-core/src`, `crates/omcp-ffi/src`, `crates/omcp-sim/src` or
`PROTOCOL.md` at this checkpoint. `git diff --check` found no whitespace issues
in the audit-owned files.

`scripts/check_protocol_invariants.py` is a standard-library-only source guard.
The original immutable upstream commit remains the comparison anchor. Normal CI
accepts only exact per-file exceptions in `scripts/protocol-review-manifest.json`:
original upstream SHA-256, accepted SHA-256, compatibility rationale and named
regression evidence. Everything else in the protected scopes must match upstream.
New/removed files, unlisted edits, stale hashes, duplicate or malformed manifest
entries, absent baselines and symlinks fail closed. Embedded tests in protected
Rust source files are protected too; UI and build metadata are not protected by
this source guard. The checker reads Git archive members in memory and never
extracts an archive onto disk.

The manifest is not an approval mechanism or independent security audit. For each
exception, a reviewer must inspect the source diff against upstream, determine
whether wire/crypto/persistence compatibility is retained, review the named tests,
and approve the code **and exact manifest entry together**. CI never generates or
updates accepted hashes automatically. Do not reset the baseline to `HEAD`, use
wildcards, skip the gate or call a hash update an audit. If a protected file needs
a later change, that exact change needs the same review again.

`python3 -m unittest discover -s scripts/tests -v` exercises accepted and unchanged
trees, unreviewed modified/added/removed paths, invalid/duplicate metadata and
protected symlinks. `--upstream-only` is a diagnostic comparison that ignores all
exceptions and exposes the complete divergence from upstream; it is expected to
fail after approved hardening. It is not the normal CI mode. The command no longer
accepts a movable replacement `--base`.

Run the repository's canonical gates, without weakening them:

```sh
python3 scripts/check_protocol_invariants.py
cargo test -p omcp-core --test auralis_wire_contract
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The workspace suite includes FFI bidirectional text, unknown-initiator refusal,
offline queues, file reconnect, persistent store, backup, re-fetch integrity,
restore notice, stale certificate/binding and legacy identity migration tests.
`omcp-sim/tests/acceptance.rs` covers SY1–SY6 with fixed seeds; SY1 models costs
and SY3 models catch-up rather than proving full radio-network behavior.

### Acceptance ladder (do not skip the final two levels)

1. **Source preservation:** compare protocol-bearing Rust and protocol documents
   against the pinned upstream commit plus explicit reviewed per-file hashes;
   inspect and approve every difference and its regression evidence. Verify matching Android/iOS UUIDs and FFI constants.
2. **Core and platform checks:** run all canonical Rust gates; Android
   `assembleDebug testDebugUnitTest`; iOS HoplaKit tests, string parity, generated
   bridge tests and Xcode build/tests. Do not treat bridge tests skipped because
   a native library is absent as passing coverage.
3. **Two-build interop harness:** compile a pinned, unmodified Hopla peer and an
   Auralis peer as separate builds/processes. Exchange actual FFI PDUs (not shared
   Rust objects) in both directions. Test QR v3 and legacy contact import, text,
   Unicode, 2 MB file, reconnect/restart, restore/reverification, burn and media
   re-fetch. Include modified certificate, wrong recipient, replay, corrupted
   chunk and oversized input rejection. Same-library loopback is not independent
   implementation interop.
4. **Physical cross-app matrix:** Hopla Android ↔ Auralis Android, Hopla iOS ↔
   Auralis iOS, Hopla Android ↔ Auralis iOS, and Auralis Android ↔ Hopla iOS.
   Test both dialer roles, simultaneous discovery, airplane mode + BLE, background
   and locked screens, notifications, interruption and restore. Record OS,
   devices, build hashes, time-to-deliver, file hash and packet/log evidence with
   no private message content or key logging.
5. **Unimplemented-spec gates:** only after real gateway/group/sync wiring exists,
   run the three-phone gateway/outage and staged MITM scenarios from §13.6,
   external rendezvous tests and the relevant §20/22/23 acceptance cases. A visual
   fork cannot mark those passed by inheriting currently absent functionality.

Completion criterion for “interoperable with Hopla beta”: terminal passing core
and platform gates plus mixed-build wire tests and the physical cross-app matrix
for every feature claimed. Completion criterion for “fully OMCP compliant” is
strictly stronger and remains blocked by the explicit implementation/spec gaps
above. No deployment, repository publication or protocol changes were performed
by this audit.

### Explicit reviewed additions (manifest schema 2)

The guard also supports schema `2`, whose exact top-level fields are `schema`,
`upstream_commit`, `changes`, and `additions`. Schema `1` remains supported with
its existing fields and semantics. The immutable upstream commit is unchanged.
`changes` continues to require the original baseline hash for existing files.

Each `additions` entry must contain exactly `path`, `accepted_sha256`, `rationale`,
and `evidence` (a nonempty list of nonempty strings). An addition must be absent
from the baseline, use a unique canonical relative literal path inside an existing
protected directory, and match its explicitly reviewed SHA-256. Existing-file
exceptions cannot be replaced with additions. Symlinks (including scope ancestor
symlinks), path escapes, wildcards and special files are rejected. Missing,
modified or unlisted protected files still fail the guard.

Schema support does not approve any new source: a reviewer must inspect each new
module and its regression evidence before adding its exact hash. Do not regenerate
or automatically populate this list to make CI pass. This policy change leaves the
current schema-1 manifest and all previously accepted source hashes untouched.
