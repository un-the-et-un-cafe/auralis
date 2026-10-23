# OMCP-MLS-PQ extension

**Status:** Provisional (M3) — wire format and vectors may change before external review.  
**Wire tag (payload magic):** ASCII `OM01` (`0x4F 0x4D 0x30 0x31`)  
**MLS ExtensionType (provisional):** `Unknown(0xF001)` — not IANA-registered; M3 uses this for documentation only. Full OpenMLS group-context extension hooks are **not** required for M3 tests; see “Integration note” below.

See PROTOCOL.md §5.1 (group suite) and §6.2 (Groups / PQ hybrid).

---

## 1. Goal

For each MLS epoch, members additionally encapsulate ML-KEM-768 to every other member’s ML-KEM public key and **mix** those shared secrets into the MLS exporter via HKDF, so group application secrets remain confidential against a cryptographically relevant quantum adversary who later breaks the classical MLS DH tree (hybrid / “belt and suspenders”).

Ciphersuite for the classical MLS layer (unchanged):  
`MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` (RFC 9420).

KEM: **ML-KEM-768** (FIPS 203), same as suite `OMCP-2026-A` Noise hybrid.

---

## 2. Binary format (payload)

All multi-byte integers are **big-endian**.

```
OmcpMlsPqExtensionV1 {
  magic:          [u8; 4]     // "OM01"
  version:        u8          // = 1
  epoch:          u64         // MLS epoch for which encapsulations apply
  n:              u16         // number of EncapsEntry (n ≤ 31 for M3 groups ≤ 32)
  entries:        EncapsEntry[n]
}

EncapsEntry {
  recipient_leaf: u32         // MLS LeafNodeIndex of recipient
  mlkem_ct:       [u8; 1088]  // ML-KEM-768 ciphertext to that member’s EK
}
```

**Encoding length:** `4 + 1 + 8 + 2 + n * (4 + 1088)`.

Who produces entries (provisional M3 choice): the **committing member** (or any member after epoch merge) encapsulates to every *other* current member’s ML-KEM encapsulation key and publishes the list. Recipients keep only the entry addressed to their leaf index (others are integrity-checked by group AEAD when carried as app/extension data; out-of-band delivery is allowed in M3 tests).

**ML-KEM public keys** are not in this payload; they are exchanged out-of-band with credentials / key packages (M3: associated with each `OmcpMlsMember`). A future revision may bind EKs into an MLS `Unknown` leaf extension.

---

## 3. Hybrid mix (normative for M3)

Let `E` be the current MLS epoch (u64).

1. `mls_exporter = MlsGroup.export_secret(label="OMCP-MLS-PQ", context=E as u64 BE, key_length=32)`
2. Collect ML-KEM shared secrets `ss[0..n)` corresponding to the `n` encapsulations in **ascending `recipient_leaf` order** (sender reconstructs all `ss_i` from encapsulate; each recipient reconstructs **only** their own `ss` via decapsulate — for the mix, M3 tests use the full ordered list known to the sender, and verify recipients derive the same hybrid when given the same ordered `ss` vector; see note below).
3. `pq_ikm = BLAKE3( ss[0] || ss[1] || … || ss[n-1] )` → 32 bytes  
   (empty `n=0` → BLAKE3 of empty input).
4. ```
   hybrid_secret = HKDF-SHA-384(
     salt = mls_exporter,
     ikm  = pq_ikm,
     info = "OMCP-MLS-PQ-v1",
     L    = 32
   )
   ```

`hybrid_secret` is the OMCP group exporter used for application AAD binding / optional extra keys. **MLS application messages themselves remain encrypted under OpenMLS epoch secrets**; the hybrid mix proves PQ contribution to exported material (PROTOCOL §6.2).

### Recipient-only mix (follow-up)

A production design should define a mix that each member can compute from **only** secrets they hold (e.g. pairwise encapsulations where every member encapsulates to every other, or a tree of KEMs). M3 documents the simpler “ordered shared-secret list” helper and tests that changing any `ss_i` changes `hybrid_secret`. Full wire integration into OpenMLS commits is **provisional**.

---

## 4. Test vectors (M3)

Deterministic inputs (all hex):

| Field | Value |
|-------|--------|
| `mls_exporter` | `00` × 32 |
| `ss[0]` | `01` × 32 |
| `ss[1]` | `02` × 32 |
| `info` | `OMCP-MLS-PQ-v1` |

Expected:

| Field | Value |
|-------|--------|
| `pq_ikm` = BLAKE3(`01`×32 \|\| `02`×32) | `8d67bc7836d128b108be2c965538f37bbcee3e7503e35e58fbb0446432e05206` |
| `hybrid_secret` | `39dcdbba4ddd834fffc736e68d69e9a1b0442b76582264488edc676a8acc13a5` |

Wire round-trip: encode `OmcpMlsPqExtensionV1` with `epoch=7`, `n=1`, `recipient_leaf=1`, `mlkem_ct=00…`, decode equals encode (`mls_pq_extension_roundtrip` test).

---

## 5. Integration note (OpenMLS)

M3 ships `omcp_core::mls::mls_pq` as a **helper** that:

- encodes/decodes the binary format above;
- performs ML-KEM encaps/decaps;
- mixes secrets per §3.

It does **not** yet register `ExtensionType::Unknown(0xF001)` inside OpenMLS group context / leaf capabilities. Application messages and Welcomes use stock OpenMLS. Follow-up work: bind ML-KEM EKs into key packages and carry `OmcpMlsPqExtensionV1` on epoch commits.

---

## 6. Document control

| Version | Date | Notes |
|---------|------|-------|
| 0.1-m3 | 2026-10 | Initial minimal format + mix for milestone M3 |
