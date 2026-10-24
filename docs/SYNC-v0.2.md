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

## M1 conflicts — resolved by §21.8 / §22.7 (v0.2.1) and implemented

| §22.7 item | Implementation |
|---|---|
| (1) Carry tags, distance vector, ≤ 1 h, ≤ 64, want > lowest carry d > spray | `routing::{CarryTable, neighbour_distance, plan_forward}`, `tag::carry_tag(tag, d)` entries in the same padded digest |
| (2) Hello 1 s → 60 s, delta digests + 5-min full refresh | `hello::{HelloTimer, DigestAdvertiser}`, `cuckoo::{DigestDelta, insert_keep_padding, remove_keep_padding}` |
| (3) Rendezvous label `HKDF-SHA-384(tag_secret, "OMCP-rdv" ‖ epoch, 32)` | `tag::rendezvous_label`, used by `budget::choose_route` |
| (4) TLV `0x54` per §21.8 registry | `tag::{tlv_class, tag_from_aad}` (rejects types outside the registry, wrong length, duplicates) |
| (5) 1:1 = 2-member MLS; secret = `MLS-Exporter("OMCP-tag", epoch, 32)` | `OmcpMlsMember::tag_exporter`, `impl TagSecretSource`, `tag::chat_tag`; tested with a real 2-member OpenMLS group |
| (6) Locator `BLAKE3-keyed(media_key, "OMCP-loc" ‖ content_hash)` | `media::locator` |
| (7) 64 KiB chunks everywhere | `files::DEFAULT_CHUNK_PLAINTEXT_LEN = 64 KiB`; media uses it |
| (8) SY6 thresholds normative | SY6 test: 10⁶ pairs, advantage < 0.005, \|z\| < 5 |

Unchanged from before: gateway stays opaque-only; the 500 MB/day gateway cap
(Internet bytes) and the §22.4 relay budgets (mesh storage/forwarding) both
apply; padding fits the 1 MiB cap.

## Findings that need a spec decision

1. **§22.7(1) as written does not meet SY4.** Carry state only exists where
   envelopes already flowed, so the first message on a static 6-hop path
   relies on spray-and-wait and is never delivered (cold 0 %). With one
   shared chat tag wanted by *both* members, the distance vector points to
   the *nearest* member, pulling A→B traffic back toward A (delivery 9.8 % /
   51 % for width 1 / 4). Per-recipient sub-tags alone make it worse (≈ 0 %:
   no delivery ever creates state near the recipient).
   **Proposed amendment (simulated as `Sync22Discovery`):** route on a
   per-recipient sub-tag, and when a holder has no wanter/carry neighbour
   *and* no carry state for the tag, flood within `ttl_hops` instead of
   spraying (route discovery); relays that held the envelope then hold carry
   state. Result: 100 % delivery (cold and warm), 20–49 % fewer
   transmissions than flooding.
2. **Idle ≤ 1 MB/day holds only at low radio degree.** The 5-min full refresh
   received from every neighbour dominates: ≈ 0.88 MB/day at 8 neighbours,
   ≈ 2.44 MB/day at 24. Options: refresh on request / on new neighbour only,
   or a 15-min refresh.
3. **"Active: metadata ≤ 2 % of payload" is not met** in either reading:
   raw (all metadata / own payload) ≈ 14× at 8 neighbours; above the idle
   baseline ≈ 5.8–7.6 %. A text-chat user's payload (~200 × 307 B/day) is
   too small next to periodic digests. Needs a definition (e.g. only
   per-message metadata: tags, recon diffs, receipts) or larger budgets.

## Stubs / not done

- No real transport wiring: `plan_forward`, `TokenBucket`, `RelayStore`,
  `ReceiptBatcher`, `Plumtree` are state machines not yet hooked into
  `ble::gossip` / `gateway::Gateway`.
- Rendezvous server does not yet authorise tag-mailbox labels (tokens are
  tied to `blinded_label(signer_pk, epoch)`).
- `tag_exporter` agrees only when members share the MLS epoch (TODO in
  `mls/group.rs`: also try the previous MLS epoch during commit races).
- Carry tags are not yet produced by `ble::gossip`; the discovery-flood
  amendment is only in the simulator.
- Reconciliation runs over one combined set of shared-tag items; per-tag
  scoping/handshake message and BLE 3-round-trip cap are not enforced.
- Media: no "nearest holder" discovery; no bao-style verified streaming.
- Delta digests are not yet framed on BLE (no message type assigned).
- HyperLogLog sketches are not exchanged on the wire yet.
- `OMCP_DICT_V1` is a placeholder word list, not a trained dictionary.
- SY1 models spray/delivery costs (documented in `sy1.rs`) rather than
  executing every hop; SY3 does not execute a full mesh, only the
  watermark catch-up exchange.
