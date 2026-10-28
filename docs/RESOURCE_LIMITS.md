# Transport and file resource limits

Auralis applies **local admission budgets**, not a new OMCP version. Envelope
encoding, Noise/ML-KEM suites, key derivation, content encryption, fragment
headers, GATT identifiers and file-offer/chunk bytes are unchanged. These bounds
are not proof of physical-device Hopla interoperability or comprehensive DoS
protection. The existing golden-wire and end-to-end tests must still pass.

## Authenticated fragment reassembly

`crates/omcp-core/src/ble/fragment.rs` derives the maximum message from the entire
legal V1 envelope, **including maximum-length associated data**:

```
41 fixed bytes + 65,535 header_aad bytes + 1,048,576 ciphertext bytes
= 1,114,152 bytes per completed message
```

Per link, admission is limited to:

| Resource | Budget | Rationale |
| --- | ---: | --- |
| Incomplete message IDs | 8 | Bound map and timeout scanning work. |
| Declared fragment slots across all messages | 65,535 | Preserve the full u16 count for one message without multiplying it by arbitrary IDs. |
| Retained plaintext fragment bytes | 4,456,608 | Four maximum-size V1 envelopes, regardless of how many IDs are interleaved. |
| Incomplete lifetime | 30 seconds | Existing protocol timeout; evicted on the next push. |

A single legal 65,535-fragment message remains admissible. A concurrent message
can fail admission while those slots are occupied; this is explicit local
backpressure, not an undisclosed smaller fragment-count field. Every fragment
header is authenticated before header-directed changes to message state (timeout
eviction is independent of header authenticity).
The u16 frame-length field limits the temporary decrypted chunk to 65,507 bytes.
Slot and byte budgets are checked before retaining that chunk or allocating a
count-sized vector. Reservation failure returns a descriptive `FragmentMalformed`
error instead of continuing with partial allocation.

On a 64-bit target the slot vectors use at most 65,535 × 24 = 1,572,840 bytes
for `Option<Vec<u8>>` entries. Payloads add at most 4,456,608 bytes. Completion
may temporarily add 1,114,152 bytes for its exactly reserved output. Allocator
bookkeeping, bounded map/record overhead and caller-owned frames/outputs are
additional; these figures are **buffer budgets, not an exact process RSS cap**.
The slot count also bounds the number of separate payload allocations.

Exact **freshly authenticated** duplicates do not consume additional slots,
bytes or progress. Conflicting duplicates and changing counts discard the
affected message. A raw encrypted-PDU replay is not an idempotent application
retry: its link nonce is stale and authentication fails. Outbound fragmentation
checks size/count/frame dimensions before collecting chunks or consuming a send
nonce, so invalid local parameters cannot desynchronize the session.

## Unauthenticated handshakes and link admission

`crates/omcp-ffi/src/lib.rs` retains the existing 480-byte handshake chunk and
full 255-value count range: **122,400 bytes per link**. A valid next chunk must
have a nonzero total, `index < total`, the expected sequential index and the
same total. Oversized, duplicate and out-of-order handshake chunks fail closed;
no repeated appends or u8 `index + 1` overflow are possible. The complete legal
255-chunk transport range is tested independently of Noise authentication.

A node admits **8 active links**, counting both unauthenticated and established
connections. This bounds handshake buffers across a node to 979,200 bytes and
also bounds the number of authenticated reassemblers. A ninth link receives
`FfiError::State` with an explicit resource-budget message; close/drop an idle
link before retrying. This is local connection policy, not a mesh membership
limit or a change to discovery/handshake encoding.

Shutdown, object drop and any `on_pdu` error release the admission slot, discard
handshake/reassembly buffers and keys, and remove the nearby-peer registration.
A failed session cannot resume after a receive nonce has advanced; reconnect
must use a new handshake. The platform remains responsible for disconnecting its
BLE connection and for connection/idle timeouts; bounded idle links can still
occupy all eight slots until shut down.

## File metadata and incremental receiving

The previous receiver allocated one `Option<FileChunk>` for every offered chunk.
A valid 50,000,000-byte offer with one-byte chunks therefore requested 50,000,000
slots before delivering any content. `FileReassembler` instead uses:

```
plaintext buffer = advertised file size (at most 50,000,000 bytes)
receipt bitmap  = ceil(num_chunks / 8)
```

The layout is validated before either allocation: positive chunk size, exact
`num_chunks == max(1, ceil(size / chunk_size))`, and the existing 50 MB size
limit. The FFI keeps its existing maximum chunk size of 64 KiB. It **does not
introduce a minimum chunk size**:

| Valid maximum file layout | Chunks | Receiver buffer bytes |
| --- | ---: | ---: |
| Default 64 KiB chunks | 763 | 50,000,096 |
| One-byte chunks | 50,000,000 | 56,250,000 |
| Empty file | 1 | 1 |

These maximum-layout calculations are tested using metadata only, without
allocating a maximum file or millions of slots. Small one-byte-chunk tests
exercise actual encryption, out-of-order reception, duplicate receipt and hash
verification.

The node reserves **128 MiB** for incoming transfers, at most **8 incoming
transfers total** and **2 per authenticated peer**. Each reservation includes
the file buffer, bitmap, twice the encoded offer remainder for decoded metadata
and allocation slack, and fixed record overhead. Thus two maximum-size files
remain admissible, even with one-byte chunks. Admission counts paused transfers
across reconnects; changing connections cannot multiply the per-peer or node
allowance. A count/byte/allocation refusal produces the existing `busy` file
failure; malformed layouts produce `invalid`.

File metadata is bounded to the 1 MiB envelope ceiling before decoding. A wrapped
key count must fit the actual remaining wire bytes before reserving key structs;
a tiny packet claiming 65,535 keys cannot allocate those keys. Encoded FFI
chunks are bounded to `20 + 65,536 + 16` bytes before copying their ciphertext.
Each chunk's index and exact ciphertext length must match its offered range.
The existing ChaCha20-Poly1305/AAD scheme authenticates a chunk before plaintext
is retained; completion still checks the complete BLAKE3 hash. This changes
allocation and verification timing, not encryption or wire format.

Exact duplicate chunks and offers retain existing progress. Conflicting offers
from the same peer fail and release the old transfer. An offer or chunk from a
different peer with the same ID cannot overwrite/delete another peer's state.
File events also validate durable-record peer/direction ownership, and an incoming
offer cannot reuse an active outgoing-transfer or non-file message ID. A
same-peer completed `FILE_RECEIVED` ID is immutable: replayed offers allocate
nothing, emit no second file and cannot reset the saved path/hash/state. Explicit
missing/requested/unavailable placeholders remain re-fetchable, including
hash-bound own-sent media after restore.
Malformed chunks, integrity failures, cancellation and completion release the
reservation. Incomplete plaintext is zeroized on drop; successful completion
moves the buffer into `ReceivedFile` without a second whole-file copy. That
returned buffer is thereafter caller-owned and is outside the incoming-state
budget. Native/FFI copies, application storage, local queued sends and objects a
caller retains are not a whole-process memory guarantee.

Partial incoming files deliberately survive disconnect for the existing replay
and resume behavior. Their slots remain bounded but do not expire autonomously;
callers can cancel them, or restart the node to drop in-memory partial state.
No partial file is presented as a successful received file without hash checking.

The standalone core `encrypt_file` API still materializes encrypted chunks. It
checks a **128 MiB estimated encrypted-chunk memory budget** before encryption,
including struct/tag/allocation overhead; extreme local requests should use
larger chunks. This does not constrain the incremental receiver's wire count
range. The default 64 KiB / 50 MB sender remains within that budget.

## Transport acceptance is not peer delivery

[Durable local transport acceptance](TRANSPORT_ACCEPTANCE.md) now retains queued
text and final-file work until every PDU in an opaque batch is accepted by the
native transport and a local database transaction commits. Pending flushes do
not reseal ciphertext. One owner per peer and a 16-message batch bound accompany
the existing two-file-chunks-per-flush limit. Abort/shutdown preserves uncommitted
logical work for a fresh session. Native OS acceptance still does not prove peer
receipt; crashes between OS acceptance and database commit may replay work.

Receiver replay protection is bounded to retained message records plus the most
recent 16,384 encrypted peer/message-ID tombstones. See the linked contract for
schema-v3 downgrade limitations, burn metadata retention, eviction behavior,
callback assumptions and verification requirements.

## Regression gates

Run the repository's pinned toolchain:

```
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
python3 scripts/check_protocol_invariants.py
```

The protected-source check must retain its original audited baseline and allow
only explicit reviewed per-file hashes with rationale and test evidence. These
compatible hardening edits are not grounds to disable the gate or reset the
entire baseline to the current checkout.
