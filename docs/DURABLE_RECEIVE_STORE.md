# Durable receive store — core-only checkpoint

This checkpoint adds unused local store APIs. Transport, FFI events and native
file sinks are **not yet integrated**. It does not fix end-to-end receive
finalization by itself and changes no wire fields, cryptography or peer ACKs.

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
Receipt-owned IDs reject new outbound queue/file work in this checkpoint;
reciprocal resend requires an explicit future transition. Generic message
overwrites cannot change receipt-owned identity, file metadata,
state or path. Cancellation retains a failed-message receipt tombstone; message
deletion removes both index and blocks. Contact deletion cancels its pending
receipts but does not remove unrelated chat history.

## Limits and failure behavior

- Existing 50,000,000-byte per-file maximum.
- At most 8 durable pending receipts globally and 2 per peer. Future integration
  must combine these counts with volatile receiver admission, not double them.
- 128 MiB live budget includes ciphertext in both tables, 128 bytes per row for
  key/group/sequence bookkeeping, and 4,128 bytes per pending receipt reserved
  for final path/CBOR growth. Completed and cancelled index records count too.
- No implicit eviction of the sole verified copy. Admission may remain blocked
  until old history is explicitly deleted.
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
above its receipt allowance without dropping data. History export contains no
receipt capability/spool. Import preserves existing local receipt-owned rows,
rejects conflicting message ownership, skips colliding outbox entries and maps
imported Received/Finalizing files without bytes to Missing.

## Explicitly deferred

Enabling these APIs in FFI/native flow; bounded retention of a verified candidate
when staging fails; native serialization, no-follow path handling, fsync/rename/
directory-sync, restart reconciliation and cancellation writer coordination;
positive ENOENT classification; combined volatile/durable admission. A safe
atomic completed-index retirement/new-generation API is also needed before a
completed local file can become Missing and be refetched. No native behavior or
feature completeness claim is made at this stage.
