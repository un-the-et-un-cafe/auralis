# Durable local transport acceptance

This stage repairs the gap between generating ciphertext and handing it to the
native Bluetooth stack. It does **not** add a protocol acknowledgement, prove
peer receipt, or promise exactly-once delivery. OMCP envelopes, Noise and app/link
AEAD, fragmentation bytes, GATT identifiers, and file formats are unchanged.

## Contract and failure boundary

Every nonempty `LinkOutput.outgoing` has an opaque `batch_token`. A `MeshLink`
has at most one outstanding batch, including handshake PDUs. While it is pending,
`flush()` emits no PDUs, consumes no new send nonce, and does not return the old
ciphertext for retransmission. Native code retains that exact batch in order.

After **every** PDU is accepted, call `commit_outgoing(token)`. Its result has no
outgoing bytes; it reports accepted user-message count, accepted file progress,
and current `more_pending`. A single database transaction updates sender status
and deletes the matching durable outbox/final-file rows. Only then does memory
advance its file cursor or remove queued work. `send_text` also persists its
message and outbox row atomically before any encryption.

- Android central writes and peripheral notifications commit after successful
  callbacks for every PDU, not merely a successful submission call.
- iOS central writes commit after every `.withResponse` callback succeeds.
- iOS peripheral notifications commit after `updateValue` returns `true` for
  every PDU. `false` means backpressure: retain identical ciphertext and await
  readiness. It is neither acceptance nor a reason to reseal.
- These events mean **accepted by Bluetooth / OS transport**, not delivered to
  the other person. Notification acceptance especially cannot prove receipt.

A submission/callback/commit error, abort, shutdown or object drop retires the
key session and releases its backlog ownership. Uncommitted logical work remains
in encrypted storage for a fresh handshake. Never skip, replay or regenerate a
failed PDU on the same session. A database COMMIT failure rolls back the whole
transaction. A crash after OS acceptance but before durable commit may replay the
same logical message ID with fresh session encryption. A crash after commit does
not replay it, even if the OS later loses the bytes. Without an OMCP peer receipt,
that uncertainty cannot be removed by a local queue contract.

Only the latest successful token is idempotent, without repeating progress
or flushed events. Wrong-link, stale and post-shutdown tokens cannot mutate work.
A duplicate previous successful token also cannot acknowledge a newer pending
batch. One link owns a peer backlog until it drains or closes, so two links cannot
prepare the same logical send concurrently. Incoming data is still processed
while an outbound batch awaits callbacks; any control replies remain queued and
are exposed by `more_pending` after commit. Handshake responses can arrive before
the original write callback without dropping the queued application messages.

Cancelling a file or burning a message before commit cannot resurrect its local
record through a late callback. Cancellation/shredding cannot recall ciphertext
already handed to native code or the OS; it is not remote erasure. A cancelled
batch's bytes must still be drained in nonce order or the whole link closed.

## Resource boundaries

A batch snapshots at most 16 queued messages (each retaining the existing 16 KiB
text/media limit) and the existing two 64 KiB file chunks plus offer. Core pending
state holds only their logical IDs, token and staged file progress; it does not
retain another ciphertext copy. Durable work is not removed to hide queue growth.
Existing eight-active-link and incoming-file/fragment budgets remain unchanged.
This is not a whole-process memory cap or a new total local-outbox quota.

Native queues reject overlapping/tokenless nonempty batches. Android uses
operation tickets captured before serial dispatch and fences a disconnected
peripheral address while an old notification callback remains outstanding. The
active-plus-fenced server set is bounded at eight. A missing callback can keep a
peer fenced until transport restart. CoreBluetooth and Android do not expose a
unique operation ID on every callback; their normal ordered-callback guarantees
remain an assumption. An arbitrarily delayed duplicate callback arriving while a
later operation is in flight cannot always be distinguished by the platform API.
Local queue tests are not evidence of physical-device callback behavior.

## Receiver replay protection and storage upgrade

Authenticated text/media/burn message IDs are recorded with their authenticated
peer in an encrypted local `received` table, atomically with the received message
and any burn key/content. A surviving message record also blocks replay and
cross-peer/own-send/file-ID overwrite. File-offer/chunk envelope IDs are **not**
tombstoned: partial incoming file state is volatile and must accept fresh-session
replay after restart. The existing immutable completed-transfer record prevents
final-file replay from emitting another file or overwriting its saved path/hash.

The receiver keeps at most **16,384** tombstones, oldest first on eviction. Each
contains only peer and message ID, encrypted with the existing row-encryption
scheme; keyed opaque table IDs prevent raw peer/ID disclosure. They survive
message deletion and burn-key shredding and retain no content, MIME, message
keys or plaintext. As with existing encrypted tables, row counts, ciphertext
sizes and insertion order remain observable. After eviction, a replay whose
message record has also gone may be accepted again. This bounded window is not
forever/exactly-once deduplication or a durable peer-delivery acknowledgement.
Tombstones are local-only and excluded from identity/history/chat backups.

The append-only **schema v2 → v3** migration creates the received table/index in
one transaction. Existing encrypted rows, history format and backup formats are
unchanged. **Downgrade is unsafe:** the older Rust schema-v2 store refuses to open
a database upgraded to v3, but this is not a safe application-level downgrade
guard. Earlier Android and iOS `MeshRepository.loadCore` implementations caught
store-unavailable errors, attempted to rename the database, deleted its `-journal`,
and opened a fresh store. Older native binaries may therefore enter that recovery
path rather than visibly refuse the downgrade; preservation of the original
database and journal is not guaranteed. Do not replace the app binary as a
rollback strategy. The current store-opening safety checkpoint removes that
automatic reset and provides fail-closed startup/retry. Native runtime validation
remains a v1 release gate; changing new source cannot repair older binaries. Restoring supported backups is a separate explicit
recovery operation and does not restore the local replay window.

The Rust store now tracks nesting owned by live transaction guards. A failed
rollback or inconsistent SQLite transaction state permanently quarantines that
store instance: subsequent reads and writes fail until it is closed and reopened.
This prevents a later savepoint from falsely reporting a durable commit inside a
leaked transaction, including when a caller catches a nested failure. It does
not repair an unavailable database or make the native recovery path safe.

## Open receive-file finalization crash window (v1 blocker)

This sender-acceptance stage does **not** establish receive-side file crash safety.
`NodeState::persist_file_events` in `crates/omcp-ffi/src/persist.rs` records
`FILE_RECEIVED` when verified bytes are returned in `LinkOutput.files`, before
Android `MeshRepository.onFileReceived` or iOS `MeshRepository.onFileReceived`
calls the native file sink and `setReceivedFilePath`. A crash before the sink/path
update can therefore leave a completed record with no durable local file/path;
the existing completed-transfer replay guard then suppresses another delivery.
The final-file replay regression in this stage explicitly saves the path and does
not cover that gap. Do not describe this as durable end-to-end file delivery.

The next bounded stage must define recoverable receive-file staging/finalization.
Required recovery tests should crash/reopen (1) after hash verification/database
completion but before native file writing, and (2) after file writing but before
path metadata acknowledgement. Prove that verified bytes remain recoverable or
that a visible retry state is entered, without falsely claiming completion,
overwriting a different peer's record, or duplicating a fully finalized file.
This requires a coordinated receive-side contract, not weakening the current
completed-file ownership/replay guard in this sender-side patch.

## Review and verification

The protected-source manifest must remain unchanged until independent review
accepts exact file hashes and evidence. Relevant changed protected files:

| File | Rationale / required evidence |
| --- | --- |
| `crates/omcp-core/src/ble/gossip.rs` | Success-only transient replay-cache query, so a failed receive transaction does not consume a control ID. FFI keys it by authenticated peer and ID. |
| `crates/omcp-core/src/store/mod.rs` | Atomic outgoing commit, owned nesting and failed-rollback quarantine, append-only encrypted tombstones and peer migration. Tests inject first/middle/final writes, deferred-constraint COMMIT failure and authorizer-denied COMMIT/ROLLBACK/RELEASE; verify close/reopen, panic/caught nested failures, ownership, cancellation, bounded eviction and backup compatibility. |
| `crates/omcp-core/src/store/records.rs` | Private encrypted peer/message-ID tombstone record; no content, keys or OMCP wire record. |
| `crates/omcp-core/src/store/tests.rs` | Store failure/migration/dedupe/regression evidence. |
| `crates/omcp-ffi/src/lib.rs` | Token state machine, bounded peer-owned preparation, durable queue-first sends, atomic peer-bound receive replay guards. No crypto/framing algorithm change. |
| `crates/omcp-ffi/src/persist.rs` | Immediate zeroizing-queue purge when burn content expires/shreds, including pending transport and restart cleanup. |
| `crates/omcp-ffi/src/tests.rs` | OS-acceptance simulation and adversarial interruption/restart/DB-failure/interleaving/cancellation/final-file/bounds cases. |

Run pinned `cargo fmt --all -- --check`, workspace Clippy with warnings denied,
workspace tests (including golden-wire and simulations), native queue and real
Rust bridge tests, Android lint/compile, and iOS localization/syntax checks.
`check_protocol_invariants.py` is expected to reject the seven unreviewed changed
protected files until the independent review is accepted; do not reset its base
or auto-regenerate hashes. Xcode device/simulator and physical Hopla/Auralis
interoperability remain separate release gates, not claims made by Linux tests.
