# Scalable sync (PROTOCOL §22) — implementation notes, M1 conflicts, stubs

Status: **experimental (v0.2)**. Code: `crates/omcp-core/src/sync/`,
simulator: `crates/omcp-sim/` (`cargo run -p omcp-sim --release -- <seed>`).

## Compression side-channel (CRIME/BREACH) — read this first

Compressing before encrypting makes ciphertext length depend on content. If
attacker-chosen text shares a compression context with a secret, observed
lengths leak the secret. §22.3 makes compression a MAY and the following
MUSTs are enforced in `sync::compress` (each has a unit test):

| MUST | Enforcement |
|---|---|
| Pad every ciphertext to 256 B / 1 KiB / 4 KiB / 16 KiB, then multiples of 16 KiB | `seal_padded` always emits a bucket length; `open_padded` / `decode_padded` / `check_bucket_len` reject anything else and non-zero padding |
| Never compress secrets, keys, tokens, recovery data, control messages | `FrameClass::{Secret, Control, Binary}` are never compressed; the decoder rejects a compressed frame of those classes |
| Quoted/forwarded text in a separate frame or not compressed | `FrameClass::Quoted`; each frame gets a fresh zstd context; quoted compression is off unless `compress_quoted` |
| Static, public dictionaries only | `OMCP_DICT_V1` is a `const`; no API accepts a dictionary; decoder rejects unknown dictionary ids |
| No compression for burn or Nearby messages | `SealOptions { burn, nearby }` disable it |

Residual risk: padding only hides length *within* a bucket. A message that is
near a bucket edge can still cross buckets depending on compressibility.
That is accepted by §22.3 and is why attacker-influenced content is isolated.

## Conflicts with the M1–M4 design and proposed resolutions

1. **Mailbox tags vs. sealed address / blinded labels (§9.2, `rendezvous::label`).**
   M4 labels are 32 bytes, keyed on the *recipient's Ed25519 key*, rotating
   every **24 h**; §22 tags are 8 bytes, keyed on the *chat exporter secret*,
   rotating every **6 h**. *Resolution:* keep both. Tags route on the mesh;
   for gateway offload the tag is mapped to a 32-byte label
   (`tag::tag_rendezvous_label`, `BLAKE3-derive("OMCP-TagMailbox-v1", tag)`
   keyed over the epoch), so `OpaqueEnvelope` and the rendezvous server are
   unchanged. Proposed spec text: "the rendezvous mailbox for a chat is
   `tag_rendezvous_label(tag, tag_epoch)`; per-recipient blinded labels remain
   for 1:1 first contact and key-package delivery". The rendezvous server's
   token check (label = blinded_label(signer_pk, epoch)) does **not** cover tag
   labels yet — see stubs.
2. **"Tag replaces the sealed address" vs. `EnvelopeV1` framing (§10.1, §21).**
   `EnvelopeV1` has no tag field and its layout is pinned. *Resolution:* carry
   the tag in `header_aad` as TLV `0x54 | 0x08 | tag` (§10.1 already allows a
   destination hint there). No framing change, no version bump. Proposal: pin
   the `header_aad` TLV registry in §21.
3. **Tag derivation for 1:1 chats vs. the Noise handshake (§21.5).**
   §22.1 uses an `exporter_secret`; MLS groups have one, but the M1 Noise
   hybrid session is a *link* (hop) session, not an end-to-end chat secret.
   *Resolution:* define 1:1 chats as 2-member MLS groups (preferred), or
   derive `exporter_secret = HKDF-SHA-384(handshake_hash, "OMCP-tag-exporter")`
   from the end-to-end session. `mailbox_tag` accepts any 16–64 byte secret.
4. **Tag epoch (6 h) vs. label epoch (24 h) vs. seen-cache TTL (10 min).**
   Receivers accept current + previous tag epoch (`accepted_tags`). No change
   needed for the seen-cache.
5. **Gateway opaque-only forwarding (§9.1.2).** Interest routing, digests and
   reconciliation happen between mesh peers; the gateway still only sees
   `OpaqueEnvelope`s and never fetches media (`MediaHolder` has no auto-fetch
   path). Media holders index blobs by an opaque *locator*, not the plaintext
   BLAKE3 hash (the §7 `content_hash` would allow confirmation-of-file). No
   conflict once that locator rule is in the spec — proposed.
6. **500 MB/day gateway cap vs. §22.4 relay budgets (500 MB storage on Wi-Fi).**
   Different resources: the gateway cap is Internet bytes/day
   (`gateway::policy::DEFAULT_DAILY_BYTE_CAP`), the relay budget is mesh
   storage + mesh forwarding. *Resolution:* both apply; offload traffic counts
   against the gateway cap, BLE forwarding against the relay budget. Proposed
   clarification in §22.4. Note the M4 store-and-forward retention (72 h)
   matches §22.4's default; §22.4 adds a 7-day maximum (`clamp_expiry`).
7. **Metered rules.** M4 `GatewayPolicy` has `wifi_only`; §22.4 adds "unless
   the user allows mobile data, then only own chats + pending outbound".
   `budget::internet_sync_scope` implements that; wiring it into
   `Gateway::pump` is a stub.
8. **Chunk size: §7 files use 16 KiB, §22.3 media uses 64 KiB.** Both reuse
   `files::encrypt_file` (parameterised); `sync::media` uses 64 KiB. Not a wire
   conflict (chunk length is in `FileMeta`). The "BLAKE3 tree" in §22.3 is
   implemented as the §7 whole-file BLAKE3 + per-chunk AEAD; per-chunk BLAKE3
   verification before decryption (bao-style) is a stub.
9. **Flooding (§8.3) vs. interest routing.** `routing::plan_forward` floods
   at ≤ 32 visible peers and switches above. **Finding (SY4):** pure
   spray-and-wait (L = 8, k = 2) delivers ~0 % over a static 6-hop path; the
   ≥ 98 % result needs relays to advertise *carry tags* along the path.
   Proposed spec clarification: carry tags are advertised within
   `ttl_hops` of a friend, still inside the fixed-size padded digest.
10. **Adaptive TTL vs. `ttl_hops: u8`.** Max 12, fits; receipts are capped at
    2 hops (`receipts::ttl_for`).
11. **Padding vs. 1 MiB ciphertext cap (§21.4).** 1 MiB is a 16 KiB multiple,
    so the largest bucket equals the cap; `encode_padded` refuses to exceed it.

## Acceptance (§22.6) — see `crates/omcp-sim/tests/acceptance.rs`

SY1 also reports the metadata share of traffic. It is ~92 % — **the §22
"≤ 2 % metadata" goal is not met** in this model, because the median node is
idle (1 % active) and its traffic is hellos + per-epoch digests. Absolute
traffic is ~120 KB/day (target 5 MB). Options: count the 2 % only against
active users, or advertise digests on change rather than per epoch.

## Stubs / not done

- No real transport wiring: `plan_forward`, `TokenBucket`, `RelayStore`,
  `ReceiptBatcher`, `Plumtree` are state machines not yet hooked into
  `ble::gossip` / `gateway::Gateway`.
- Rendezvous server does not yet authorise tag-mailbox labels (tokens are
  tied to `blinded_label(signer_pk, epoch)`).
- Reconciliation runs over one combined set of shared-tag items; per-tag
  scoping/handshake message and BLE 3-round-trip cap are not enforced.
- Media: no "nearest holder" discovery; no bao-style verified streaming.
- HyperLogLog sketches are not exchanged on the wire yet.
- `OMCP_DICT_V1` is a placeholder word list, not a trained dictionary.
- SY1 models spray/delivery costs (documented in `sy1.rs`) rather than
  executing every hop; SY3 does not execute a full mesh, only the
  watermark catch-up exchange.
