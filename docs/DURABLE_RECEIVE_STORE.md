# Durable receive finalization — integration candidate

This candidate wires the local store, metadata-only FFI events and serialized
native file sinks. It is not a production release: exact-head native SDK tests
and real-device/Hopla interoperability remain separate gates. No wire fields,
cryptography or peer ACK semantics change.

## State and ownership

Append-only schema v4 introduces sealed receipt-index and block tables. A stage
requires an existing same-peer, non-burn file message with matching size/hash,
no active outgoing work for that ID and a receiving/paused/requested state.
Message ownership (`mine`) is preserved, including own-sent refetch candidates.
Verified input remains caller-owned; sealing uses independent 64 KiB local
blocks, unrelated to wire chunk size. Empty input has zero local blocks.
Index, blocks and `FILE_FINALIZING = 11` commit atomically before a receipt may
be returned. Each receipt carries its message ID and a random 256-bit capability.

Reads require the exact capability and matching current message. Commit verifies
all authenticated blocks and the whole-file hash, then atomically stores the
caller-supplied durable path, marks Received and removes blocks. The platform
must first validate/sync its private file; core cannot validate that filesystem.
Completed metadata remains attached to the message for exact-token/exact-path
retry. Different path, deleted or cancelled tokens cannot recreate history.
Receipt-owned IDs reject generic outbound writes. An explicit reciprocal resend
authorization checks peer, ownership, state, size and whole plaintext hash while
preserving the existing Received history and generation. Generic message
overwrites cannot change receipt-owned identity, file metadata,
state or path. Cancellation retains a failed-message receipt tombstone; message
deletion removes blocks and retains a cleanup-only obligation until native
exact-generation deletion and directory sync are acknowledged. Contact deletion cancels its pending
receipts but does not remove unrelated chat history.

## Limits and failure behavior

- Existing 50,000,000-byte per-file maximum.
- At most 8 combined partial/verified/durable pending receives globally and 2 per
  peer, with conservative reservation accounting before admitting partial input.
- 128 MiB live budget includes ciphertext in both tables, 128 bytes per row for
  key/group/sequence bookkeeping, and 4,128 bytes per pending receipt reserved
  for final path/CBOR growth. Completed and cancelled index records count too.
- No implicit eviction of the sole verified copy. Admission may remain blocked
  until old history is explicitly deleted and its cleanup obligations completed.
- These are live logical storage bounds, **not** a SQLite allocated-file,
  rollback-journal, process-RSS or caller input-buffer cap. Metadata enumeration
  streams one sealed row at a time; block reads are at most 64 KiB.
- Store initialization explicitly selects DELETE journal mode, secure_delete=ON
  and temp_store=MEMORY. It does not override synchronous: the pinned bundled
  SQLite connection uses FULL (2), verified on the actual disk-backed Store by
  regression test, not EXTRA (3). Filesystem/hardware flush behavior and directory
  durability across power loss remain outside this checkpoint.
- Body/commit failures roll back; uncertain rollback quarantines the connection
  under the existing store policy. Disk full can trigger SQLite automatic
  rollback, so callers must reopen after quarantine.

Peer rekey updates receipts atomically with messages and rejects a merged peer
above its combined receive allowance without dropping data. Existing target
contact metadata is preserved during a legacy identity merge. History export contains no
receipt capability/spool. Import preserves existing local receipt-owned rows,
rejects conflicting message ownership, skips colliding outbox entries and maps
imported Received/Finalizing files without bytes to Missing.

## Native publication and recovery

Transport keeps a bounded zeroizing verified candidate if SQLite staging fails;
no Received event or success path is emitted before the durable transaction.
An automatic candidate gets one stage attempt; explicit recovery retries each
candidate once and isolates unrelated failures. Partial pre-verification input
remains volatile. Metadata receipts expose only token-bound 64 KiB block reads.

Native repositories serialize finalization, cancellation and identity changes
away from the UI thread. Generation-specific private paths use no-follow
checks/descriptor ownership. Both platforms require exact length/BLAKE3 readback,
file fsync and directory fsync before the token-bound database commit. The
**database commit is the atomic visibility boundary**: Pending/Finalizing files
have no published history path and must not be opened/shared by the UI.

Android app SELinux policy denies hard links. Android therefore creates its
private generation file with O_EXCL and writes it before database publication;
that private file can exist partially while the receipt remains Pending. Restart
verifies/reuses a full-size file without overwriting it. Only a positively short,
regular, single-link same-generation file may be repaired after a fresh exact
Pending capability check and pinned-directory/dev+inode validation under the
repository's serialized mutation lock. Full-size hash mismatch, oversize,
unknown read/verification/core-state errors and uncertain ownership preserve
both existing file and encrypted spool. Ordinary Retry never overwrites a
full-size corrupt candidate. The separate, explicit **Repair local copy** action
is described below; it is not startup/reconnect automation. Cancellation still
retains the Failed tombstone rather than authorizing same-ID refetch. Once
cancelled, requesting the same logical transfer again remains separate work;
the sender can send a new attachment/new message ID. New creation never replaces
an existing file. This is not a filesystem atomic-rename claim or protection against a
hostile process sharing the app UID.

iOS retains its same-filesystem no-overwrite hard-link publication, owned-temp
unlink and directory sync before database commit. EEXIST verifies rather than
overwrites; crash-pair recovery requires the expected inode relationship.
Trusted OS container anchors alone use POSIX realpath; injected roots and all
appended descendants retain no-follow checks. Unsupported filesystem operations
on either platform preserve the durable receipt rather than bypassing policy.

For existing media from older app versions, the platform wrapper may map only
its current trusted OS lexical app-storage prefix to the pinned physical root.
The relative suffix must be nonempty and component-valid; traversal, foreign
containers, prefix lookalikes and descendant symlinks remain refused. Persisted
paths are never passed wholesale to realpath, injected roots are not normalized,
and the original database path remains unchanged for missing-path CAS. Unknown
storage/access failures are never interpreted as absence. Backup-file writes
use a structurally separate namespace and cannot repair/overwrite receipt files.

Cancellation first commits durable Failed state, then removes only that token's
private files and syncs the directory. Schema v5 adds a cleanup pagination index
and atomically regroups authenticated v4 cancelled rows during opening without
rewriting their ciphertext or sequence. Subsequent metadata mutations check the
live budget transactionally; a full old metadata-only store can refuse cleanup
ACK/deletion growth until other already-cleaned history frees space. It never
drops cancellation protection silently to recover capacity. Sealed
`cleanup_done`/`message_deleted` flags distinguish retained Failed tombstones
from deleted-message cleanup obligations. Cleanup ACK is token/state-bound and
requires the original Failed owner or explicitly recorded message absence.
Failed-message tombstones remain to prevent history import/replay resurrection;
deleted-message obligations may be retired after ACK. A durable cursor advances
one bounded page of at most 32 cleanup records per recovery pass, so an undeletable
first generation does not starve later entries. No broad directory garbage
collection is performed; uncertain parent/path failures retain the obligation.

A completed path becomes Missing only after positive absence is established,
with an exact path compare-and-swap and no active reciprocal resend. Refetch
gets a fresh random token and generation-specific path; old writers/ACKs cannot
authorize that generation. Cancellation/transport failure during reciprocal
resend does not delete the original Received bubble or its local file.

## Explicit local-copy repair (candidate)

A positively identified full-length native content mismatch is typed separately
from every storage, capability, read/permission and incomplete-read error.
The verifier checks the live Pending receipt's entire metadata snapshot before
classifying a mismatch; changed metadata or stale/cancelled/completed tokens
cannot authorize repair. An explicit user confirmation binds the exact receipt
and current identity/lifecycle, not merely a message ID or display filename.

Before unlinking any candidate, native code re-proves the local mismatch, checks
no-follow regular/single-link/dev+inode ownership, validates the **entire**
retained encrypted spool through a bounded core transaction, and rechecks fresh
Pending metadata. The transaction ends before filesystem work. Only the same
owned generation is replaced; generic exceptions never fall through to deletion.
The rebuilt file must pass length/hash readback, file and directory sync, then
token-bound commit. Failure retains the encrypted sole copy and no Received path
is exposed. Ordinary Retry stays nondestructive for full-size mismatches.

This action repairs a Pending local candidate using already verified bytes; it
is not a network re-request, cancelled-generation retirement or import bypass.
Explicit request-again after cancellation, physical-device interruption and
cross-client interoperability remain separate validation/work items.

## Validation boundaries

Core and FFI unit/integration tests, portable Swift tests and focused JVM tests
are local evidence, not Android Os/Darwin filesystem proof. Generated bindings,
hosted platform compilation/instrumentation, physical interruption/power-loss
behavior and actual Hopla interoperability must be reported separately. No
claim of flawless operation, peer delivery confirmation or release readiness
follows from source review or portable tests alone.
