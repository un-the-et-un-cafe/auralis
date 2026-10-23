# Open Mesh Chat Protocol (OMCP)

Open chat and data-sharing over a Bluetooth mesh. Peers talk without the Internet; any peer with connectivity can bridge others. Designed for outages and strong cryptography (Noise hybrid + MLS + post-quantum KEMs).

- **Protocol:** [PROTOCOL.md](./PROTOCOL.md) (v0.1.1 draft — includes §20 communities & livestreams target, and **§21 normative wire pins**)
- **Apps checklist:** [apps/REQUIREMENTS.md](./apps/REQUIREMENTS.md) (Android 12+ / iOS 17+)
- **Core:** Rust shared library [`crates/omcp-core`](./crates/omcp-core) (`omcp-core`)
- **Extensions:** [extensions/mls-pq.md](./extensions/mls-pq.md)

## Status

Milestone **M1** in progress: Noise hybrid handshake + framing unit tests in `omcp-core` (see PRs). Spec is v0.1.1 with §21 pins.

### §21 pins implemented in `omcp-core`

- **suite_id** `0x0001` (`OMCP-2026-A`), big-endian envelopes, flags LSB `RELAY_OK` / `STORE_OK` / `PRIORITY`, max ciphertext **1_048_576** bytes (§21.1–§21.4).
- **`OMCP-Noise-Hybrid-v1`** (§21.5): after classical Noise_IK DH `MixKey`s into `ck`, both sides set `ck' = HKDF-SHA-384(salt=ck, ikm=mlkem_ss, info="OMCP-Noise-Hybrid-v1", L=32)` then continue Noise Split from `ck`. Initiator ML-KEM encapsulation key in msg1 (MixHash'd); responder ciphertext in msg2 (MixHash'd).
- **Not** wire-compatible with snow HFS / libp2p `/noise-mlkem768-hfs/*` — OMCP-defined only.

## Licence

[MIT](./LICENSE) © Jakob Michahelles
