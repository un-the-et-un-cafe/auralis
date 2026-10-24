# Open Mesh Chat Protocol (OMCP)

**OMCP is an open protocol** for secure chat and data sharing on a Bluetooth Low Energy mesh, with optional Internet bridging. Compatible clients can interoperate without depending on a single vendor app or cloud.

This repository holds the **protocol specification**, a **shared reference core**, and a **reference mobile application**. The community is encouraged to build their own clients, gateways, and tools against the same wire format.

| | |
| --- | --- |
| **Status** | Draft v0.1 — not production-ready |
| **Licence** | [MIT](./LICENSE) |
| **Spec** | [PROTOCOL.md](./PROTOCOL.md) |
| **Reference apps** | Android 12+ / iOS 17+ (see [apps/REQUIREMENTS.md](./apps/REQUIREMENTS.md)) |

---

## What OMCP is

- A **mesh-first** messaging protocol: peers exchange end-to-end encrypted messages over BLE without requiring the public Internet.
- An **optional gateway role**: any peer with Internet access may forward *opaque* encrypted envelopes for neighbours — never plaintext.
- **Outage-resilient**: local chat and store-and-forward continue when the WAN is down.
- **Cryptographically modern**: Noise hybrid (X25519 + ML-KEM-768), MLS for groups, HPKE sealed addressing (see suite `OMCP-2026-A` / §21).

## What OMCP is not

- Not a closed, app-store-only product.
- Not a requirement to use this repository’s UI.
- Not a cryptocurrency, token, or investment scheme.
- Not a claim of “unbreakable” security — properties are stated in the threat model ([PROTOCOL.md](./PROTOCOL.md) §4).

---

## Protocol vs software in this repo

```
┌─────────────────────────────────────────────────────────┐
│  PROTOCOL.md          Normative wire format & security  │  ← implementers
├─────────────────────────────────────────────────────────┤
│  crates/omcp-core     Reference Rust library            │  ← optional reuse
├─────────────────────────────────────────────────────────┤
│  apps/ (Android/iOS)  Reference clients                 │  ← one of many UIs
└─────────────────────────────────────────────────────────┘
```

1. **Protocol** — [PROTOCOL.md](./PROTOCOL.md) is the source of truth. Independent implementations that pass the stated acceptance criteria are first-class OMCP peers.
2. **Reference core** — `omcp-core` is a convenience implementation of framing, handshake, crypto helpers, an in-process BLE loopback chat path (M2), experimental MLS groups + file chunks (M3), and an experimental gateway role + in-memory rendezvous reference (M4). You may use it, fork it, or replace it. It does **not** claim production BLE hardware readiness or production MLS/PQ maturity.
3. **Reference app** — The Android and iOS clients in this project demonstrate the protocol (mesh chat, gateway toggle, communities/livestream *targets* per §20). They are **examples**, not the exclusive interface to the network.

Third parties may ship their own apps, bots, gateways, and desktop clients under any licence compatible with the MIT protocol text and their own code, so long as they do not misrepresent unofficial software as the sole “official” OMCP client.

---

## Quick links

| Document | Purpose |
| --- | --- |
| [PROTOCOL.md](./PROTOCOL.md) | Full specification (identity, crypto, BLE, gateways, wire pins §21) |
| [apps/REQUIREMENTS.md](./apps/REQUIREMENTS.md) | Reference app MVP checklist |
| [extensions/mls-pq.md](./extensions/mls-pq.md) | MLS post-quantum extension (provisional M3 wire + mix) |
| [design/](./design/) | Reference UI design pack (when present) |

---

## Roadmap (milestones)

| Milestone | Focus |
| --- | --- |
| **M0** | Spec + CI skeleton |
| **M1** | `omcp-core` Noise hybrid + envelope framing ([PR #1](https://github.com/un-the-et-un-cafe/open-mesh-chat/pull/1)) |
| **M2** | BLE transport loopback + two-device text chat (in-process GATT pipe in `omcp-core`; not production phone BLE) |
| **M3** | MLS groups + file chunks (OpenMLS + chunk crypto in `omcp-core`; **not production-ready**, PQ extension provisional — see [extensions/mls-pq.md](./extensions/mls-pq.md)) |
| **M4** | Gateway role + optional rendezvous (`omcp-core::gateway` / `rendezvous`: opaque forwarding, 500 MB/day cap, Wi-Fi-only default, sealed-sender blinded labels, in-memory `/v1/enqueue`+`/v1/fetch` reference with Ed25519 short-lived tokens; **experimental, not production-ready** — no real TLS/QUIC transport) |
| **M5–M6** | Android / iOS reference betas |
| **M7** | External crypto review |

Communities and livestreams are specified as a **v0.2 product layer** (§20); the mesh MVP remains 1:1 and small groups.

---

## Contributing

- Protocol changes that affect the wire format need a clear PR against `PROTOCOL.md`, test vectors where applicable, and a version note in §19 / §21.
- Client and UI work should track the reference design pack and `apps/REQUIREMENTS.md`, but alternate designs are welcome in other repositories.
- Please open issues for interop bugs and proposed extensions (`extensions/`).

---

## Security

Report vulnerabilities privately to the maintainers before public disclosure. Do not file exploit details in public issues until a fix or coordinated disclosure plan exists.

---

## Licence

[MIT](./LICENSE) © OMCP contributors.
