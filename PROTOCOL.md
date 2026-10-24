# Open Mesh Chat Protocol (OMCP) — Specification v0.1

**Status:** Draft for implementation  
**Date:** 2026-10-23  
**Audience:** Protocol implementers, mobile clients, gateway operators  
**Licence:** Proposed CC0 / Apache-2.0 dual for code and protocol text (final choice with project owners)

This document defines an open chat and data-sharing protocol that runs primarily over a Bluetooth Low Energy (BLE) mesh of peer devices. Every peer is also a potential Internet gateway for other peers. The network must remain usable during Internet outages and must be extremely secure by default. The design is versioned and extensible so later work can add transports, crypto suites, and application features without breaking existing peers.

---

## 1. Goals and non-goals

### 1.1 Goals

1. **Local-first mesh:** Peers exchange messages and shared objects over BLE mesh without requiring the public Internet.
2. **Optional Internet bridging:** Any peer with Internet access MAY act as a gateway, ferrying traffic for nearby BLE peers that lack Internet.
3. **Outage resilience:** Chat, presence, and previously synced data remain available when the Internet is down.
4. **Extreme security:** End-to-end encryption (E2EE) for all user content; forward secrecy; post-compromise security; authenticated peers; no cleartext content on gateways or relays.
5. **Open and evolvable:** Stable wire formats, explicit version negotiation, extension points, and a public change process.
6. **Cross-platform clients:** First-class Android and iOS apps implementing the same protocol.

### 1.2 Non-goals (v0.1)

- Replacing the global Internet or providing general-purpose IP routing.
- Guaranteeing delivery across arbitrary geographic distance without gateways or store-and-forward peers.
- Perfect anonymity against a global passive adversary (see threat model).
- Cryptocurrency, tokens, or payment rails inside the protocol.

### 1.3 Rules that do not change

- User message content is never readable by relays, gateways, or application servers.
- A peer MUST NOT be required to connect to any vendor cloud to chat on the local mesh.
- Cryptographic agility is mandatory; hard-coded single algorithms without negotiation are forbidden after v0.1 bootstrap.
- Protocol changes that break wire compatibility require a major version bump.

---

## 2. Architecture overview

```
┌─────────────┐   BLE mesh    ┌─────────────┐   BLE mesh    ┌─────────────┐
│  Peer A     │◄─────────────►│  Peer B     │◄─────────────►│  Peer C     │
│ (phone)     │               │ (gateway)   │               │ (phone)     │
└─────────────┘               └──────┬──────┘               └─────────────┘
                                     │ Internet (optional)
                                     ▼
                              ┌─────────────┐
                              │  Rendezvous │
                              │  / store-   │
                              │  and-forward│
                              │  (optional) │
                              └─────────────┘
```

- **Peer / Node:** Any device running OMCP. All peers are equal in the mesh; gateway capability is a role, not a privilege over content.
- **Mesh link:** BLE connections and advertisements used for discovery, routing hints, and frame transport.
- **Gateway role:** A peer that has Internet and volunteers to forward *encrypted* OMCP envelopes to other OMCP endpoints (other gateways, rendezvous services, or remote peers).
- **Rendezvous service (optional):** A minimal Internet service that stores opaque, encrypted envelopes addressed to peer public keys for later pickup. It NEVER sees plaintext.

---

## 3. Identities and keys

### 3.1 Peer identity

Each peer generates a long-term **Identity Key Pair**:

| Field | v0.1 algorithm | Notes |
|-------|----------------|-------|
| Identity signing | Ed25519 | Long-term; used to sign Identity Certificates and device attestations |
| Identity PQ hybrid (optional, recommended) | Ed25519 + ML-DSA-65 (Dilithium3) | Dual signatures during transition |
| Identity encryption / KEM long-term | X25519 + ML-KEM-768 hybrid | Used for sealed addressing and initial contact |

**PeerID** = `multihash(identity_signing_public_key)` truncated to 32 bytes, encoded as `omcp1:<base32>` for display.

Peers MAY rotate Identity Keys. Rotation requires signing a **Successor Statement** with the old key and publishing it to contacts over authenticated channels.

### 3.2 Device keys

Each installation holds:

1. **Identity Key** (above)
2. **Device Ephemeral Key** — rotated frequently; used in Noise_XX / Noise_IK handshakes
3. **Group sender keys / MLS epoch secrets** — for multi-party chats (see §6)
4. **Local storage key** — device-bound (Android Keystore / iOS Secure Enclave), wraps the above

### 3.3 Contact and trust

- First contact uses an **out-of-band verification** (QR code with PeerID + short authentication string, or NFC tap).
- Safety number: `SHA3-256(sorted(PeerID_A || PeerID_B))` displayed as 60 decimal digits in 12 groups of 5.
- Trust is local: each peer maintains its own contact book with verification state (`unverified`, `verified`, `revoked`).

---

## 4. Threat model

### 4.1 Assets

- Message and file content
- Membership of private groups
- Contact graphs (as far as practical)
- Gateway/relay metadata minimization

### 4.2 Adversaries

| Adversary | Assumed capability | Protocol response |
|-----------|--------------------|-------------------|
| Local radio observer | Captures BLE frames | Encrypted frames; frequent key rotation; padding |
| Malicious peer / gateway | Relays or drops traffic | E2EE; authenticated routing; no cleartext on gateway |
| Compromised device | Reads local storage | Secure Enclave / Keystore; optional passphrase; remote wipe via tombstone (best-effort) |
| Nation-state with harvest-now-decrypt-later | Stores ciphertext for decades | Hybrid PQ KEMs and signatures from v0.1 |
| Network partition / Internet outage | No WAN | Local mesh continues; store-and-forward when links return |

### 4.3 Explicit non-protections (v0.1)

- Traffic analysis of who talks to whom on a densely monitored mesh
- Physical seizure of an unlocked device
- Compelled disclosure of passphrases

---

## 5. Cryptographic suite (v0.1)

All algorithms are named in the handshake and MAY be upgraded via negotiation.

### 5.1 Suite `OMCP-2026-A` (mandatory)

| Purpose | Algorithm |
|---------|-----------|
| Handshake | Noise_IK_25519_ChaChaPoly_BLAKE2b with ML-KEM-768 hybrid (Noise Hybrid Draft style): shared secret = `HKDF(X25519_ss \|\| ML-KEM_ss)` |
| AEAD | ChaCha20-Poly1305 |
| Hash / KDF | BLAKE3 (keyed mode) and HKDF-SHA-384 for Noise compatibility paths |
| Signatures | Ed25519; dual-sign with ML-DSA-65 when both peers advertise PQ |
| Group messaging | Messaging Layer Security (MLS) RFC 9420, ciphersuite `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` **plus** PQ hybrid extension (see §6.2) |
| Sealed sender / addressing | HPKE (RFC 9180) with DHKEM(X25519) + ML-KEM-768 hybrid, HKDF-SHA384, ChaCha20-Poly1305 |
| Commitment / transcript | BLAKE3 |

### 5.2 Crypto agility

- Frames carry `suite_id` (uint16). Unknown suites → peer responds with `UNSUPPORTED_SUITE` and its supported list.
- Deprecation: suites enter `deprecated` then `forbidden` via signed **Suite Bulletin** documents distributed in-mesh and via gateways.

### 5.3 Randomness and nonces

- CSPRNG from platform (SecureRandom / SecRandomCopyBytes).
- 96-bit nonces for ChaCha20-Poly1305; never reused with the same key. Prefer counter||random construction per session.

---

## 6. Messaging model

### 6.1 One-to-one

1. Peers complete Noise hybrid handshake → transport keys.
2. Application messages are AEAD-encrypted frames with associated data: `suite_id`, `sender_peer_id`, `msg_id`, `timestamp_ms`, `content_type`.
3. Message IDs are 128-bit random; duplicates are ignored.
4. Delivery acknowledgements are separate encrypted control frames (not cleartext ACKs on the mesh).

### 6.2 Groups (MLS)

- Each group is an MLS group. Epoch secrets protect application messages.
- **PQ hybrid extension (OMCP-MLS-PQ):** For each epoch, members additionally encapsulate to every other member's ML-KEM public key and mix the shared secrets into the MLS exporter via HKDF. Spec of the extension binary format lives in `extensions/mls-pq.md` (to be developed; wire tag `0xOM01`).
- External joins require an invite authenticated by a current member (Welcome + Verification QR optional).

### 6.3 Content types (application layer)

| Code | Type | Notes |
|------|------|-------|
| 0x01 | `text/plain; charset=utf-8` | Default chat |
| 0x02 | `text/markdown` | Optional |
| 0x10 | `application/omcp-reaction` | Unicode + target msg_id |
| 0x20 | `application/omcp-file-meta` | Name, size, MIME, content hash |
| 0x21 | Chunked encrypted blob | See §7 |
| 0x30 | `application/omcp-presence` | Optional; encrypted to contacts only |
| 0x40 | Control (ACK, receipt, typing) | E2EE |

### 6.4 Sync and causality

- Each peer maintains a **per-conversation DAG** of message IDs with parent pointers (1–N parents) for offline concurrent edits/sends.
- Clients display a linearization by `( Lamport-like logical clock, PeerID )` with UI markers for branches when needed.
- Sync protocol: `HAVE` / `WANT` / `GIVE` over encrypted channels for missing DAG nodes and blobs.

---

## 7. Data sharing (files and objects)

1. Sender computes `content_hash = BLAKE3(plaintext)`.
2. Encrypts file with a random **Content Key** (256-bit); wraps Content Key to each recipient via HPKE hybrid.
3. Splits ciphertext into chunks (default 16 KiB) with per-chunk AEAD; advertises `file-meta` then serves chunks on request.
4. Mesh peers MAY cache encrypted chunks (LRU). Gateways MAY cache for Internet recipients. Caches never hold Content Keys unless they are intended recipients.
5. Integrity: recipient verifies `BLAKE3(plaintext)` after decrypt.

---

## 8. BLE mesh transport

### 8.1 Roles and GATT

- Service UUID (v0.1): `a0mc0001-0mcp-4c01-9e00-000000000001` (placeholder; finalize before ship).
- Characteristics: `Inbox` (write), `Outbox` (notify), `Control` (read/write).
- MTU: negotiate maximum; default fragment size 512 bytes application payload before link AEAD.

### 8.2 Discovery

- BLE advertisements include: protocol magic `OMCP`, version, truncated PeerID (first 6 bytes), capabilities flags (`GATEWAY`, `STORE_FORWARD`, `MLS`, `PQ`).
- Full PeerID and signed capability statement exchanged after connection.

### 8.3 Routing on the mesh

- **v0.1 routing:** epidemic / gossip with TTL and seen-cache (message_id bloom + LRU). Suitable for small/medium meshes (tens of peers).
- **v0.2 target:** distance-vector or source-routed paths with authenticated path metrics (to be specified).
- Loop prevention: `seen` set per `envelope_id` with TTL expiry (default 10 minutes; configurable).

### 8.4 Store-and-forward

- Peers MAY retain encrypted envelopes addressed to known contacts for up to `retention_hours` (default 72; user-configurable; max 168).
- On contact reappearance, pending envelopes are offered via `HAVE`.

---

## 9. Internet gateway role

### 9.1 Behaviour

1. Gateway peer maintains TLS 1.3 (or QUIC) sessions to configured rendezvous endpoints and/or remote OMCP peers.
2. It forwards **opaque envelopes** only: `{ destination_peer_id or sealed_address, suite_id, ciphertext, expiry }`.
3. It MUST NOT attempt decryption of user content. Implementation MUST use separate processes/sandboxes where OS allows.
4. Rate limits and battery policies are local; protocol defines `GATEWAY_BUSY` and `GATEWAY_OFFERING` control messages.

### 9.2 Addressing over Internet

- Preferred: **sealed sender** to PeerID via HPKE so the gateway learns only a blinded destination label.
- Optional account-free rendezvous: `POST /v1/enqueue` and `POST /v1/fetch` with authentication by short-lived signed tokens from the Peer Identity Key.

### 9.3 Outage behaviour

- When Internet is lost, gateway simply stops bridging; local mesh continues.
- When Internet returns, gateway drains its outbound queue and fetches pending envelopes for local peers that opted in.

### 9.4 Trust in gateways

- Using a gateway does not grant it trust over content.
- Users MAY pin allowed gateway PeerIDs for their traffic.
- Default: any mesh neighbour advertising `GATEWAY` may be used for *transport*; clients SHOULD prefer gateways whose PeerID is a verified contact when available.

---

## 10. Wire format

### 10.1 Envelope (canonical)

```
EnvelopeV1 {
  magic:           b"OMCP"
  version:         u16   // 1
  suite_id:        u16
  envelope_id:     [16]u8
  flags:           u16   // RELAY_OK, STORE_OK, PRIORITY, ...
  ttl_hops:        u8
  created_ms:      u64   // sender local clock; for ordering hints only
  header_aad_len:  u16
  header_aad:      [..]  // cleartext associated data (no secrets)
  ciphertext_len:  u32
  ciphertext:      [..]  // AEAD ciphertext + tag
}
```

`header_aad` MAY include destination hint (truncated or sealed), content_type class, and size — never plaintext bodies.

### 10.2 Framing on BLE

- Link layer: length-prefixed fragments with `frag_id`, `frag_index`, `frag_count`, per-link session AEAD (derived from Noise).
- Reassembly timeout: 30 seconds; incomplete messages dropped.

### 10.3 Version negotiation

- First control frame after handshake: `Hello { versions[], suites[], features[] }`.
- Intersection chosen by initiator using preference order published in this spec for each major version.

---

## 11. Security requirements (normative)

1. **E2EE default:** All user content types in §6.3 MUST be E2EE. Cleartext chat modes are forbidden.
2. **Forward secrecy:** Handshake and MLS epoch rotation MUST provide FS. Session keys wiped after use.
3. **Post-compromise security:** MLS + continuous key refresh; one-to-one sessions rekey at least every 7 days or 100 messages.
4. **PQ readiness:** Suite `OMCP-2026-A` hybrid KEMs REQUIRED for Internet-facing paths; REQUIRED for mesh after both peers advertise `PQ`.
5. **Auth:** No unsigned identity claims. Capability statements are signed.
6. **Replay:** `envelope_id` + timestamp window (±10 minutes skewed tolerance with logical clocks) + AEAD.
7. **Padding:** Optional traffic padding profiles (`off`, `low`, `medium`) to mitigate size side channels on gateways.
8. **Screenshots / notifications:** Clients SHOULD show sender and preview by default and MUST offer a "Private notifications" mode that shows only a generic "New message" (no sender, no content). Payloads are decrypted on-device only; gateways and push relays never see plaintext.
9. **Backups:** Encrypted backups only; recovery key is 24-word BIP-39-like phrase or platform secure recovery; cloud backup is optional and E2EE (client-side encrypted before upload; see apps/REQUIREMENTS.md "Onboarding & recovery backup").

---

## 12. Privacy

- Contact discovery is local/QR/NFC first. No compulsory central directory.
- Optional user-run directory: stores only PeerID + signed reachability blobs.
- Metadata minimization on gateways: sealed addressing, short retention, no logs of payload hashes beyond operational minima (implementations SHOULD document logging).

---

## 13. Smartphone applications

### 13.1 Scope

Native (or high-quality cross-platform) apps for **Android 12+** and **iOS 17+** implementing OMCP v0.1 as both mesh peers and optional gateways.

### 13.2 Shared product requirements

| Area | Requirement |
|------|-------------|
| Onboarding | Generate identity; show recovery phrase once; QR for contact add |
| Chats | 1:1 and groups; offline queue; clear “mesh / gateway / mixed” path indicators |
| Files | Share images, PDFs, up to configurable size (default 50 MB); progress UI |
| Gateway toggle | User can enable “Help nearby peers reach the Internet” with battery warning |
| Security centre | Safety numbers, verified badges, session reset, blocked peers |
| Accessibility | Dynamic type, screen readers, sufficient contrast |
| Localisation | English and French at minimum for first release (project may add more) |
| Open source | Client source published; reproducible builds targeted |

### 13.3 Android-specific

- Foreground service for mesh participation with persistent notification explaining BLE use.
- Use Android BLE APIs; request `BLUETOOTH_SCAN` / `CONNECT` / `ADVERTISE` and location only as required by OS version.
- Prefer Kotlin; protocol core in shared Rust (see §14) via JNI.
- WorkManager for deferred gateway fetch when Internet returns.
- Keystore + StrongBox when available for identity keys.

### 13.4 iOS-specific

- Background modes: Bluetooth LE central/peripheral as allowed; document App Store justification.
- MultipeerConnectivity MAY be researched as an *additional* local transport in v0.2; v0.1 BLE remains mandatory for cross-Android interop.
- SwiftUI client; protocol core in shared Rust via FFI.
- Secure Enclave for key wrapping; Keychain accessibility `afterFirstUnlock` carefully reviewed for mesh background needs.
- Notify users clearly when background mesh is limited by iOS.

### 13.5 UX copy principles

- Plain language for security states (“Verified contact”, “Waiting for mesh”, “Sent via neighbour’s Internet”).
- Never claim “unhackable” or “military grade”; state concrete properties (E2EE, PQ hybrid, open source).

### 13.6 Testing acceptance (apps)

1. Two phones, Airplane Mode + BLE on: exchange text and a 2 MB file within a 10 m indoor range.
2. Three phones, only middle one online: outer peers exchange messages via gateway within 60 seconds.
3. Kill Internet on gateway mid-chat: mesh path continues for local peers.
4. Safety number verification prevents MITM in a staged adversarial BLE peer test.
5. Uninstall/reinstall with recovery phrase restores identity and contact verification state (from encrypted backup).

---

## 14. Reference implementation plan

### 14.1 Repository layout (suggested)

```
omcp/
  PROTOCOL.md          # this document
  extensions/          # MLS-PQ and future extensions
  crates/
    omcp-core/         # Rust: crypto, framing, MLS, sync
    omcp-ble/          # BLE transport adapters
  apps/
    android/
    ios/
  rendezvous/          # optional minimal server
  docs/
  CHANGELOG.md
```

### 14.2 Language choice

- **Protocol core in Rust** (auditable, shared on Android/iOS).
- **Apps:** Kotlin + Swift UI layers.

### 14.3 Milestones

| Milestone | Deliverable |
|-----------|-------------|
| M0 | This spec merged; CI skeleton via Pipely |
| M1 | `omcp-core` Noise hybrid handshake + framing unit tests |
| M2 | BLE transport loopback + two-device text chat |
| M3 | MLS groups + file chunks |
| M4 | Gateway role + optional rendezvous |
| M5 | Android beta |
| M6 | iOS beta |
| M7 | External crypto review + threat-model update |

---

## 15. Compliance, export, and ethics

- Implementations MUST follow applicable cryptography export rules in their distribution regions.
- No hidden law-enforcement plaintext access. Any future lawful intercept feature would require a protocol major version and user-visible distrust warnings — not planned.
- Mesh networking in some jurisdictions may face radio regulation constraints; apps MUST use BLE only within legal power/duty-cycle limits.

---

## 16. Extension process

1. Draft extension in `extensions/NNNN-short-name.md` with motivation, wire tags, security impact.
2. Interop test vectors required before merge.
3. Feature bit advertised in `Hello`.
4. Two independent implementations (or one impl + comprehensive test vectors) before marking extension `stable`.

---

## 17. Open questions (track in issues)

1. Final BLE Service UUID registration / company ID.
2. Exact Noise hybrid construction reference (IETF draft pinning).
3. Whether iOS background BLE limits require a companion “always-on” gateway device profile.
4. Default retention and legal constraints for store-and-forward in EU (GDPR minimization).
5. Optional Tor/onion transport for rendezvous as v0.3 privacy upgrade.

---

## 18. Glossary

| Term | Meaning |
|------|---------|
| Peer | Device running OMCP |
| Gateway | Peer offering Internet bridging of opaque envelopes |
| Envelope | Versioned encrypted unit of transport |
| Suite | Named crypto algorithm set |
| MLS | Messaging Layer Security (group E2EE) |
| Sealed address | HPKE-encrypted destination so relays learn little |

---

## 19. Document control

| Version | Date | Notes |
|---------|------|-------|
| 0.1 | 2026-10-23 | Initial specification for Proggy / Pipely handoff |

**Next editors:** Implementers SHOULD open PRs against this file for any wire-format change. Semantic versioning: breaking wire changes → 1.0 major after first production interop; until then 0.x may break with explicit changelog entries.

---

## 20. Application features — communities and livestreams (v0.2 target)

Inspired by familiar messenger patterns (channels, communities, live video), implemented without weakening E2EE or mesh-first rules. v0.1 MVP stays 1:1 + small MLS groups; this section defines the next product layer so Proggy can plan capacity and Pipely can stage releases.

### 20.1 Communities

A **Community** is a signed container of related conversations:

| Object | E2EE | Mesh notes |
|--------|------|------------|
| Community roster + roles | Encrypted to members; public communities MAY publish a cleartext name/description only | Roster sync via MLS-like epoch or per-community group |
| Announcement channel | E2EE to members; optional sealed public preview for public communities | Large fan-out via gateway + chunked store-and-forward |
| Topic channels / forums | Each topic is its own MLS group (or subgroup) | Prefer lazy join; invite links are signed capability tokens |
| Voice chat rooms (optional later) | Separate media SFrame/MLS keys | BLE unsuitable for many-way audio; Internet or Wi‑Fi Direct required |

**Roles:** `owner`, `admin`, `moderator`, `member`, `guest`. Capability tokens are signed by the community identity key. Moderation actions are control frames inside the encrypted community; there is no plaintext moderation API on relays.

**Discovery:** Invite link / QR first. Optional public index stores only `{ community_id, name, description, member_count_bucket, join_policy }` — never message content.

### 20.2 Channels (broadcast)

- One-to-many with admin-controlled send rights.
- Recipients verify admin signatures on posts.
- Comments/reactions are separate E2EE threads keyed per channel epoch.
- Forwarding preserves original signature when the sender allows “signed forwards”.

### 20.3 Livestreams

Livestreams NEED a high-bandwidth path. On pure BLE mesh they are **not** supported beyond a low-bitrate status beacon.

| Mode | Transport | Security |
|------|-----------|----------|
| `mesh-beacon` | BLE | Encrypted title/status to community; no video |
| `lan-relay` | Wi‑Fi / local IP discovered via OMCP | SFrame (RFC 9605) or Insertable Streams keys distributed via MLS exporter |
| `internet-sfu` | Gateway or user-chosen SFU | E2EE with SFU seeing only encrypted media; keys never on SFU |

**Normative:** Livestream media keys are derived from the community/channel MLS epoch exporter + per-stream random. SFUs and gateways MUST NOT receive media keys. Viewers who are not community members cannot decrypt.

**Signalling:** WebRTC-style offers/answers carried as OMCP content-type `0x50 application/omcp-rtc-signal` (E2EE).

**Recording:** Local-only or E2EE cloud blob with Content Key wrapped to chosen recipients; default off.

### 20.4 Other Telegram-like surfaces (phased)

| Feature | Phase | Notes |
|---------|-------|-------|
| Usernames / @handles | v0.3 | Optional, user-run name servers; no global mandatory registry |
| Bots / mini-apps | v0.3 | Capability-scoped tokens; no privileged plaintext access |
| Stories / ephemeral posts | v0.2 | TTL enforced client-side; relays drop after expiry |
| Polls, stickers, threaded replies | v0.2 | Content types under `0x60–0x6F` |
| Secret chats (device-pair only) | v0.1 already covers via 1:1 Noise | |
| Payments / gifts | Out of scope | |

### 20.5 App UI additions (Android / iOS)

Update `apps/REQUIREMENTS.md` milestones:

- Community home: channels list, members, roles
- Live tab: start/join stream when `lan-relay` or `internet-sfu` available; clear “video needs Wi‑Fi/Internet” when only BLE is up
- Admin tools: slow mode, ban, delete-for-all (tombstone signed by admin)

### 20.6 Acceptance (communities / live)

1. Create a community of 3 peers on mesh; announcement post delivers to all offline-then-online members within retention window.
2. Start a livestream with Internet SFU; a member on another network watches with E2EE (keys never logged by SFU test harness).
3. BLE-only peers see live *announcement* but not video, with an explicit UI reason.


---

## 21. Normative wire pins (v0.1.1) — answers to M1 open questions

These pins supersede ambiguous wording in §§5, 10, and 17 for implementers. Update this section only via PR; bump the document patch version when changing a pin.

### 21.1 Suite ID

| suite_id | Name | Status |
|----------|------|--------|
| `0x0001` | `OMCP-2026-A` | Mandatory for v0.1 |

Unknown `suite_id` → `UNSUPPORTED_SUITE` with the peer’s supported list.

### 21.2 Envelope endianness

All multi-byte integer fields in `EnvelopeV1` (§10.1) are **big-endian (network byte order)**.

### 21.3 Envelope flag bits (`flags` u16)

Bit 0 = LSB.

| Bit | Name | Meaning when set |
|-----|------|------------------|
| 0 | `RELAY_OK` | Intermediate peers MAY gossip/relay this envelope |
| 1 | `STORE_OK` | Store-and-forward peers MAY retain until expiry / retention policy |
| 2 | `PRIORITY` | Prefer earlier scheduling on constrained relays (no content privilege) |
| 3–15 | Reserved | MUST be 0 on send; ignore on receive |

### 21.4 Maximum `ciphertext_len`

- Transport envelope ciphertext (AEAD ciphertext + tag): **1 048 576 bytes (1 MiB)** maximum.
- Envelopes larger than this MUST be rejected before decrypt.
- Application files larger than this use the chunked blob path (§7); each chunk is its own envelope under this limit.

### 21.5 Noise hybrid construction — `OMCP-Noise-Hybrid-v1`

**Pin (normative for suite `0x0001`):** pattern **Noise_IK** with classical DH **X25519**, AEAD **ChaCha20-Poly1305**, hash **BLAKE2b**, plus an ML-KEM-768 (FIPS 203) ephemeral encapsulation mixed into the Noise chaining key.

**Shared-secret mixing (answers §5.1 / Q6):**  
`X25519_ss` in the informal §5.1 text means the **Noise chaining-key state after the classical DH MixKey steps**, not a raw DH output concatenated outside Noise. After the classical Noise_IK DH contributions have been mixed into `ck` per the Noise spec, both parties compute:

```
ck' = HKDF-SHA-384(
        salt = ck,
        ikm  = mlkem_shared_secret,   // 32-byte ML-KEM-768 ss
        info = "OMCP-Noise-Hybrid-v1",
        L    = 32
      )
ck ← ck'
```

Then continue Noise splitting / transport key derivation from `ck` as usual.

**ML-KEM transcript binding:** The initiator’s ML-KEM public key (and the responder’s ciphertext) are included in the Noise handshake messages as cleartext handshake payloads (or dedicated handshake fields) and are covered by subsequent `MixHash` so they cannot be stripped without failing authentication.

**Wire compatibility:** `OMCP-Noise-Hybrid-v1` is **OMCP-defined**. It is **not** claimed wire-compatible with `snow` HFS names such as `Noise_XXhfs_25519+MLKEM768_*` or with libp2p `/noise-mlkem768-hfs/*`. A future suite (`0x0002+`) MAY adopt a finalized Noise HFS / community KAT set; until then interop tests use OMCP KATs only.

**References (informative):** Noise Protocol Framework; FIPS 203 ML-KEM; TLS hybrid practice in RFC 9954 / draft-ietf-tls-ecdhe-mlkem (concatenated secrets in TLS — different combiner; do not copy TLS concat into Noise without this section’s HKDF mix).

### 21.6 Document control addendum

| Version | Date | Notes |
|---------|------|-------|
| 0.1.1 | 2026-10-23 | Wire pins for M1 PR #1 |


### 21.7 Handshake field order (OMCP-Noise-Hybrid-v1)

Within each Noise handshake message that carries ML-KEM material:

1. Emit **ML-KEM public fields first** (initiator ephemeral encapsulation key `ek` in message 1; responder ciphertext `ct` in message 2), in cleartext as required by the hybrid construction.
2. Then emit the **Noise sealed/encrypted payload** (identity proofs, certificates, Hello, etc.).

Rationale: MixHash MUST bind `ek`/`ct` before AEAD seal so an adversary cannot strip or reorder PQ material without failing authentication. Implementations MUST NOT place `ek`/`ct` after the sealed payload.

This matches commit `9e90d33` on PR #1.
