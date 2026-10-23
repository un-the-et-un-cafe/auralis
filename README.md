# Open Mesh Chat Protocol (OMCP)

Open chat and data-sharing over a Bluetooth mesh. Peers talk without the Internet; any peer with connectivity can bridge others. Designed for outages and strong cryptography (Noise hybrid + MLS + post-quantum KEMs).

- **Protocol:** [PROTOCOL.md](./PROTOCOL.md) (v0.1 draft, includes §20 communities & livestreams target)
- **Apps checklist:** [apps/REQUIREMENTS.md](./apps/REQUIREMENTS.md) (Android 12+ / iOS 17+)
- **Core:** Rust shared library [`crates/omcp-core`](./crates/omcp-core) (`omcp-core`)
- **Extensions:** [extensions/mls-pq.md](./extensions/mls-pq.md)

## Status

Milestone **M0** (spec + CI skeleton). Next: **M1** — Noise hybrid handshake + framing unit tests in `omcp-core`.

## Licence

[MIT](./LICENSE) © Jakob Michahelles
