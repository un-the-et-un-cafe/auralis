//! Durable verified receive candidates and token-bound finalization/cleanup.
//!
//! The 128 MiB budget counts live sealed index/block bytes plus a conservative
//! 128-byte per-row allowance and pending completion reservations. It does not
//! bound SQLite allocation, rollback journals, or transient caller buffers.
//! Completed indices remain message-bound for exact-token retry. Cancellation
//! drops blocks but retains a bounded-budget tombstone and a native cleanup
//! obligation; ACK marks cleanup done while the Failed message exists. Deletion
//! retains cleanup-only metadata until exact-generation cleanup is acknowledged.
//! New receives may be refused until history and cleanup obligations are cleared.
//! A positively missing completed generation can be retired through a path CAS.
//! SQLite uses DELETE journal and default FULL synchronous mode, not EXTRA; this
//! is not a claim of hardware power-loss durability or peer delivery ACK.
use super::*;

pub const RECEIVED_BLOCK_BYTES: usize = 65_536;
pub const MAX_RECEIVED_SPOOL_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_PENDING_RECEIPTS: usize = 8;
pub const MAX_PENDING_RECEIPTS_PER_PEER: usize = 2;
const ROW_ALLOWANCE: u64 = 128;
// Reserve completion-path ciphertext growth before accepting the sole copy.
const COMPLETION_RESERVATION: u64 = 4128;
const SEALED_OVERHEAD: u64 = 40;
const PENDING: u8 = 0;
const COMPLETE: u8 = 1;
const CANCELLED: u8 = 2;
const CLEANUP_GROUP: &[u8] = b"received-cancelled-cleanup-v1";
const CLEANUP_CURSOR: &[u8] = b"received-cleanup-cursor-v1";
const CLEANUP_PAGE: usize = 32;

/// Random 256-bit capability bound to one existing local message. Never backed up.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivedFileToken {
    pub message_id: [u8; 16],
    pub nonce: [u8; 32],
}
impl std::fmt::Debug for ReceivedFileToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReceivedFileToken").finish_non_exhaustive()
    }
}

/// Metadata only; callers retrieve at most one bounded plaintext block at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceivedFileReceipt {
    pub token: ReceivedFileToken,
    pub peer: [u8; 32],
    pub hash: [u8; 32],
    pub size: u64,
    pub name: String,
    pub mime: String,
    pub blocks: u32,
}
#[derive(Clone, Serialize, Deserialize)]
struct ReceiptRec {
    receipt: ReceivedFileReceipt,
    mine: bool,
    locator: Option<Vec<u8>>,
    state: u8,
    path: Option<String>,
    #[serde(default)]
    cleanup_done: bool,
    #[serde(default)]
    message_deleted: bool,
}

fn block_id(token: &ReceivedFileToken, index: u32) -> [u8; 36] {
    let mut id = [0; 36];
    id[..32].copy_from_slice(&token.nonce);
    id[32..].copy_from_slice(&index.to_be_bytes());
    id
}
fn refused() -> OmcpError {
    OmcpError::Store("invalid received-file receipt")
}

impl Store {
    // Called only inside authenticated append-only v5 migration transaction.
    pub(super) fn migrate_receipt_cleanup_groups(&self) -> Result<()> {
        self.each_receipt(|rec| {
            if rec.state == CANCELLED {
                self.conn
                    .execute(
                        "UPDATE receive_index SET grp=?1 WHERE id=?2",
                        params![
                            self.gid(CLEANUP_GROUP).as_slice(),
                            self.rid(Table::ReceiveIndex, &rec.receipt.token.message_id)
                                .as_slice()
                        ],
                    )
                    .map_err(db)?;
            }
            Ok(())
        })
    }
    pub(super) fn has_receipt(&self, id: &[u8; 16]) -> Result<bool> {
        Ok(self.receipt(id)?.is_some())
    }
    fn validate_receipt(rec: &ReceiptRec) -> Result<()> {
        crate::files::check_file_size(rec.receipt.size)?;
        if rec.receipt.blocks as u64 != rec.receipt.size.div_ceil(RECEIVED_BLOCK_BYTES as u64)
            || rec.receipt.name.len() > 65_535
            || rec.receipt.mime.len() > 65_535
            || rec.state > CANCELLED
            || (rec.state != CANCELLED && (rec.cleanup_done || rec.message_deleted))
        {
            return Err(refused());
        }
        Ok(())
    }
    fn receipt(&self, id: &[u8; 16]) -> Result<Option<ReceiptRec>> {
        let rec: Option<ReceiptRec> = self.get(Table::ReceiveIndex, id)?;
        if let Some(rec) = &rec {
            Self::validate_receipt(rec)?;
        }
        Ok(rec)
    }
    // Iterate one sealed metadata record at a time, never all file blocks.
    fn each_receipt(&self, mut visit: impl FnMut(ReceiptRec) -> Result<()>) -> Result<()> {
        self.ensure_healthy()?;
        let mut stmt = self
            .conn
            .prepare("SELECT id, ct FROM receive_index ORDER BY seq")
            .map_err(db)?;
        let mut rows = stmt.query([]).map_err(db)?;
        while let Some(row) = rows.next().map_err(db)? {
            let id: Vec<u8> = row.get(0).map_err(db)?;
            let id: [u8; 32] = id.try_into().map_err(|_| OmcpError::StoreAuth)?;
            let ct: Vec<u8> = row.get(1).map_err(db)?;
            let rec: ReceiptRec = uncbor(&xopen(
                &self.row_key,
                &Self::aad(Table::ReceiveIndex, &id),
                &ct,
            )?)?;
            Self::validate_receipt(&rec)?;
            if self.rid(Table::ReceiveIndex, &rec.receipt.token.message_id) != id {
                return Err(refused());
            }
            visit(rec)?;
        }
        Ok(())
    }
    fn put_receipt(&self, rec: &ReceiptRec) -> Result<()> {
        self.put(
            Table::ReceiveIndex,
            &rec.receipt.token.message_id,
            Some(if rec.state == CANCELLED && !rec.cleanup_done {
                CLEANUP_GROUP
            } else {
                &rec.receipt.peer
            }),
            rec,
        )
    }
    fn receipt_message(&self, rec: &ReceiptRec) -> Result<MessageRec> {
        let m = self
            .message(&rec.receipt.token.message_id)?
            .ok_or_else(refused)?;
        let f = m.file.as_ref().ok_or_else(refused)?;
        let state = match rec.state {
            PENDING => FILE_FINALIZING,
            COMPLETE => FILE_RECEIVED,
            CANCELLED => FILE_FAILED,
            _ => return Err(refused()),
        };
        if m.peer != rec.receipt.peer
            || m.mine != rec.mine
            || m.burn.is_some()
            || m.burn_state != BURN_NONE
            || f.hash.as_deref() != Some(rec.receipt.hash.as_slice())
            || f.size != rec.receipt.size
            || f.name != rec.receipt.name
            || f.mime != rec.receipt.mime
            || f.locator != rec.locator
            || f.state != state
            || f.done_bytes
                != if rec.state == CANCELLED {
                    0
                } else {
                    rec.receipt.size
                }
            || f.local_path != rec.path
        {
            return Err(refused());
        }
        Ok(m)
    }
    fn checked_receipt(&self, token: &ReceivedFileToken) -> Result<ReceiptRec> {
        let rec = self.receipt(&token.message_id)?.ok_or_else(refused)?;
        if rec.receipt.token != *token {
            return Err(refused());
        }
        self.receipt_message(&rec)?;
        Ok(rec)
    }
    pub(super) fn guard_receipt_message(&self, m: &MessageRec) -> Result<()> {
        if let Some(rec) = self.receipt(&m.id)? {
            if rec.message_deleted {
                return Err(refused());
            }
            let f = m.file.as_ref().ok_or_else(refused)?;
            let state = match rec.state {
                PENDING => FILE_FINALIZING,
                COMPLETE => FILE_RECEIVED,
                CANCELLED => FILE_FAILED,
                _ => return Err(refused()),
            };
            if m.peer != rec.receipt.peer
                || m.mine != rec.mine
                || m.burn.is_some()
                || m.burn_state != BURN_NONE
                || f.hash.as_deref() != Some(rec.receipt.hash.as_slice())
                || f.size != rec.receipt.size
                || f.name != rec.receipt.name
                || f.mime != rec.receipt.mime
                || f.locator != rec.locator
                || f.state != state
                || f.done_bytes
                    != if rec.state == CANCELLED {
                        0
                    } else {
                        rec.receipt.size
                    }
                || f.local_path != rec.path
            {
                return Err(refused());
            }
        }
        Ok(())
    }
    /// Budgeted live encrypted payload plus conservative per-row bookkeeping.
    pub fn received_spool_bytes(&self) -> Result<u64> {
        self.ensure_healthy()?;
        let bytes: u64 = self.conn.query_row("SELECT COALESCE(SUM(n),0) FROM (SELECT length(ct)+128 AS n FROM receive_index UNION ALL SELECT length(ct)+128 AS n FROM receive_blocks)", [], |r| r.get(0)).map_err(db)?;
        let mut reserved = 0;
        self.each_receipt(|rec| {
            if rec.state == PENDING {
                reserved += COMPLETION_RESERVATION;
            }
            Ok(())
        })?;
        bytes.checked_add(reserved).ok_or_else(refused)
    }
    /// Stage bytes only for an existing non-burn file message with matching owner,
    /// size and authenticated hash. No absent/deleted message is ever recreated.
    /// Input stays borrowed; only one 64 KiB sealing buffer is allocated at a time.
    pub fn stage_received_file(
        &self,
        peer: &[u8; 32],
        id: &[u8; 16],
        bytes: &[u8],
    ) -> Result<ReceivedFileReceipt> {
        crate::files::check_file_size(bytes.len() as u64)?;
        let hash = *blake3::hash(bytes).as_bytes();
        self.transaction(|s| {
            if s.get::<OutFileRec>(Table::OutFiles, id)?.is_some()
                || s.get::<OutboxRec>(Table::Outbox, id)?.is_some()
            {
                return Err(OmcpError::Store("active outgoing work owns file ID"));
            }
            let mut m = s.message(id)?.ok_or_else(refused)?;
            let f = m.file.as_ref().ok_or_else(refused)?;
            if m.peer != *peer
                || m.burn.is_some()
                || m.burn_state != BURN_NONE
                || f.size != bytes.len() as u64
                || f.hash.as_deref() != Some(hash.as_slice())
                || f.name.len() > 65_535
                || f.mime.len() > 65_535
            {
                return Err(refused());
            }
            if let Some(rec) = s.receipt(id)? {
                s.receipt_message(&rec)?;
                if rec.state != PENDING {
                    return Err(refused());
                }
                // Verify the existing sole copy rather than hiding corrupt spool behind idempotence.
                s.verify_received_blocks(&rec)?;
                return Ok(rec.receipt);
            }
            if !matches!(f.state, FILE_RECEIVING | FILE_PAUSED | FILE_REQUESTED)
                || f.local_path.is_some()
            {
                return Err(refused());
            }
            let pending = s.pending_received_files()?;
            if pending.len() >= MAX_PENDING_RECEIPTS
                || pending.iter().filter(|r| r.peer == *peer).count()
                    >= MAX_PENDING_RECEIPTS_PER_PEER
            {
                return Err(OmcpError::Store("received-file admission limit"));
            }
            let receipt = ReceivedFileReceipt {
                token: ReceivedFileToken {
                    message_id: *id,
                    nonce: rand()?,
                },
                peer: *peer,
                hash,
                size: f.size,
                name: f.name.clone(),
                mime: f.mime.clone(),
                blocks: bytes.len().div_ceil(RECEIVED_BLOCK_BYTES) as u32,
            };
            let rec = ReceiptRec {
                receipt: receipt.clone(),
                mine: m.mine,
                locator: f.locator.clone(),
                state: PENDING,
                path: None,
                cleanup_done: false,
                message_deleted: false,
            };
            let required = COMPLETION_RESERVATION
                + cbor(&rec)?.len() as u64
                + SEALED_OVERHEAD
                + ROW_ALLOWANCE
                + bytes.len() as u64
                + receipt.blocks as u64 * (SEALED_OVERHEAD + ROW_ALLOWANCE);
            if s.received_spool_bytes()?
                .checked_add(required)
                .is_none_or(|n| n > MAX_RECEIVED_SPOOL_BYTES)
            {
                return Err(OmcpError::Store("received-file spool budget"));
            }
            s.put_receipt(&rec)?;
            for (i, block) in bytes.chunks(RECEIVED_BLOCK_BYTES).enumerate() {
                s.put_raw(
                    Table::ReceiveBlocks,
                    &block_id(&receipt.token, i as u32),
                    Some(&receipt.token.nonce),
                    block,
                )?;
            }
            let f = m.file.as_mut().ok_or_else(refused)?;
            f.state = FILE_FINALIZING;
            f.done_bytes = f.size;
            s.put_message(&m)?;
            Ok(receipt)
        })
    }
    pub fn pending_received_files(&self) -> Result<Vec<ReceivedFileReceipt>> {
        let mut pending = Vec::new();
        self.each_receipt(|rec| {
            if rec.state == PENDING {
                self.receipt_message(&rec)?;
                if pending.len() >= MAX_PENDING_RECEIPTS {
                    return Err(refused());
                }
                pending.push(rec.receipt);
            }
            Ok(())
        })?;
        Ok(pending)
    }
    fn received_block(&self, rec: &ReceiptRec, index: u32) -> Result<Zeroizing<Vec<u8>>> {
        if rec.state != PENDING || index >= rec.receipt.blocks {
            return Err(refused());
        }
        let block = self
            .get_raw(Table::ReceiveBlocks, &block_id(&rec.receipt.token, index))?
            .ok_or_else(refused)?;
        let expected = (rec.receipt.size - index as u64 * RECEIVED_BLOCK_BYTES as u64)
            .min(RECEIVED_BLOCK_BYTES as u64);
        if block.len() as u64 != expected {
            return Err(refused());
        }
        Ok(block)
    }
    pub fn read_received_block(
        &self,
        token: &ReceivedFileToken,
        index: u32,
    ) -> Result<Zeroizing<Vec<u8>>> {
        let rec = self.checked_receipt(token)?;
        self.received_block(&rec, index)
    }
    fn verify_received_blocks(&self, rec: &ReceiptRec) -> Result<()> {
        let mut hash = blake3::Hasher::new();
        for i in 0..rec.receipt.blocks {
            hash.update(&self.received_block(rec, i)?);
        }
        if hash.finalize().as_bytes() != &rec.receipt.hash {
            return Err(OmcpError::FileHashMismatch);
        }
        Ok(())
    }
    fn delete_received_blocks(&self, rec: &ReceiptRec) -> Result<()> {
        // Group deletion also removes unexpected extra rows; no unbounded plaintext load.
        self.conn
            .execute(
                "DELETE FROM receive_blocks WHERE grp=?1",
                params![self.gid(&rec.receipt.token.nonce).as_slice()],
            )
            .map_err(db)?;
        Ok(())
    }
    /// Caller must have durably verified and synced this private filesystem path.
    /// Store cannot validate a platform filesystem. Exact token/path retries are
    /// idempotent; differing paths and stale/cancelled/deleted tokens fail closed.
    pub fn commit_received_file(&self, token: &ReceivedFileToken, path: &str) -> Result<()> {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(refused());
        }
        self.transaction(|s| {
            let mut rec = s.checked_receipt(token)?;
            if rec.state == COMPLETE {
                return if rec.path.as_deref() == Some(path) {
                    Ok(())
                } else {
                    Err(refused())
                };
            }
            if rec.state != PENDING {
                return Err(refused());
            }
            s.verify_received_blocks(&rec)?;
            let mut m = s.receipt_message(&rec)?;
            rec.state = COMPLETE;
            rec.path = Some(path.to_owned());
            s.put_receipt(&rec)?;
            s.delete_received_blocks(&rec)?;
            if s.received_spool_bytes()? > MAX_RECEIVED_SPOOL_BYTES {
                return Err(OmcpError::Store("received-file spool budget"));
            }
            let f = m.file.as_mut().ok_or_else(refused)?;
            f.state = FILE_RECEIVED;
            f.local_path = rec.path.clone();
            f.done_bytes = f.size;
            s.put_message(&m)
        })
    }
    /// Durable cancellation; successful cancellation cannot be undone by stale
    /// staging/commit calls. Completed local files are not cancelled or deleted.
    pub fn cancel_received_file(&self, peer: &[u8; 32], id: &[u8; 16]) -> Result<bool> {
        self.transaction(|s| {
            let Some(mut rec) = s.receipt(id)? else {
                return Ok(false);
            };
            if rec.receipt.peer != *peer {
                return Err(refused());
            }
            let mut m = s.receipt_message(&rec)?;
            if rec.state != PENDING {
                return Ok(false);
            }
            s.delete_received_blocks(&rec)?;
            rec.state = CANCELLED;
            s.put_receipt(&rec)?;
            let f = m.file.as_mut().ok_or_else(refused)?;
            f.state = FILE_FAILED;
            f.done_bytes = 0;
            s.put_message(&m)?;
            Ok(true)
        })
    }
    /// Retire a completed generation only after the platform positively proved
    /// its exact private path absent. The path comparison is a generation CAS;
    /// permissions/protection failures are not authority to call this method.
    pub fn mark_received_file_missing(&self, id: &[u8; 16], expected_path: &str) -> Result<bool> {
        self.transaction(|s| {
            let Some(mut m) = s.message(id)? else {
                return Ok(false);
            };
            let Some(f) = m.file.as_ref() else {
                return Ok(false);
            };
            if m.burn.is_some()
                || f.state != FILE_RECEIVED
                || f.local_path.as_deref() != Some(expected_path)
            {
                return Ok(false);
            }
            if s.get::<OutFileRec>(Table::OutFiles, id)?.is_some() {
                return Err(OmcpError::Store("outgoing resend still owns file"));
            }
            if let Some(rec) = s.receipt(id)? {
                s.receipt_message(&rec)?;
                if rec.state != COMPLETE {
                    return Err(refused());
                }
                s.delete_receipt(id)?;
            }
            let f = m.file.as_mut().ok_or_else(refused)?;
            f.state = FILE_MISSING;
            f.local_path = None;
            f.done_bytes = 0;
            s.put_message(&m)?;
            Ok(true)
        })
    }
    /// Explicit reciprocal resend authorization. Re-hash the supplied plaintext
    /// against the existing receipt/message before retaining encrypted send work.
    /// It never changes received history or retires its receipt capability.
    pub fn put_received_file_resend<C: AsRef<[u8]>>(
        &self,
        f: &OutFileRec,
        chunks: &[C],
        bytes: &[u8],
    ) -> Result<()> {
        self.transaction(|s| {
            let m = s.message(&f.id)?.ok_or_else(refused)?;
            let file = m.file.as_ref().ok_or_else(refused)?;
            if m.peer != f.peer
                || m.burn.is_some()
                || m.burn_state != BURN_NONE
                || file.size != f.size
                || f.size != bytes.len() as u64
                || file.hash.as_deref() != Some(blake3::hash(bytes).as_bytes().as_slice())
            {
                return Err(refused());
            }
            if let Some(rec) = s.receipt(&f.id)? {
                s.receipt_message(&rec)?;
                if rec.state != COMPLETE {
                    return Err(refused());
                }
            }
            if !matches!(file.state, FILE_RECEIVED | FILE_SENT) {
                return Err(refused());
            }
            s.put_out_file_rows(f, chunks)
        })
    }
    /// Legacy backup path attachment is never a bypass around receipt commit.
    pub fn set_legacy_received_file_path(&self, id: &[u8; 16], path: &str) -> Result<()> {
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(refused());
        }
        self.transaction(|s| {
            if s.has_receipt(id)? {
                return Err(refused());
            }
            let mut m = s.message(id)?.ok_or_else(refused)?;
            if m.burn.is_some() || m.burn_state != BURN_NONE {
                return Err(refused());
            }
            let f = m.file.as_mut().ok_or_else(refused)?;
            if !matches!(
                f.state,
                FILE_MISSING | FILE_REQUESTED | FILE_UNAVAILABLE | FILE_RECEIVED
            ) {
                return Err(refused());
            }
            f.state = FILE_RECEIVED;
            f.local_path = Some(path.to_owned());
            f.done_bytes = f.size;
            s.put_message(&m)
        })
    }
    /// Return at most 32 outstanding cleanup generations. A sealed durable
    /// cursor advances even when a platform cannot delete an individual file,
    /// so repeated bounded passes fairly reach later obligations after restart.
    pub fn cancelled_received_files(&self) -> Result<Vec<ReceivedFileReceipt>> {
        self.transaction(|s| {
            let cursor = s.get_meta::<Vec<u8>>(CLEANUP_CURSOR)?.filter(|v| !v.is_empty());
            if cursor.as_ref().is_some_and(|c| c.len() != 32) { return Err(refused()); }
            let mut stmt = s.conn.prepare("SELECT id, ct FROM receive_index WHERE grp=?1 AND (?2 IS NULL OR id>?2) ORDER BY id LIMIT 32").map_err(db)?;
            let mut rows = stmt.query(params![s.gid(CLEANUP_GROUP).as_slice(), cursor]).map_err(db)?;
            let mut results = Vec::new(); let mut last = None;
            while let Some(row) = rows.next().map_err(db)? {
                let id: Vec<u8> = row.get(0).map_err(db)?;
                let rid: [u8;32] = id.as_slice().try_into().map_err(|_| refused())?;
                let ct: Vec<u8> = row.get(1).map_err(db)?;
                let rec: ReceiptRec = uncbor(&xopen(&s.row_key, &Self::aad(Table::ReceiveIndex, &rid), &ct)?)?;
                Self::validate_receipt(&rec)?;
                if s.rid(Table::ReceiveIndex, &rec.receipt.token.message_id) != rid || rec.state != CANCELLED || rec.cleanup_done { return Err(refused()); }
                s.validate_cleanup_owner(&rec)?;
                last = Some(id); results.push(rec.receipt);
            }
            drop(rows); drop(stmt);
            let next = if results.len() == CLEANUP_PAGE { last.unwrap_or_default() } else { Vec::new() };
            s.put_meta(CLEANUP_CURSOR, &next)?;
            Ok(results)
        })
    }
    fn validate_cleanup_owner(&self, rec: &ReceiptRec) -> Result<()> {
        if rec.message_deleted {
            if self.message(&rec.receipt.token.message_id)?.is_some() {
                return Err(refused());
            }
        } else {
            self.receipt_message(rec)?;
        }
        Ok(())
    }
    /// Native caller has removed only this generation's owned files and synced
    /// the directory. Keep the cancellation tombstone while its Failed message
    /// exists; deleted-message obligations can be retired without resurrection.
    pub fn acknowledge_received_file_cleanup(&self, token: &ReceivedFileToken) -> Result<bool> {
        self.transaction(|s| {
            let Some(mut rec) = s.receipt(&token.message_id)? else {
                return Ok(false);
            };
            if rec.receipt.token != *token || rec.state != CANCELLED {
                return Err(refused());
            }
            s.validate_cleanup_owner(&rec)?;
            if rec.cleanup_done {
                return Ok(false);
            }
            if rec.message_deleted {
                s.del_raw(Table::ReceiveIndex, &token.message_id)?;
            } else {
                rec.cleanup_done = true;
                s.put_receipt(&rec)?;
                if s.received_spool_bytes()? > MAX_RECEIVED_SPOOL_BYTES {
                    return Err(OmcpError::Store("received-file spool budget"));
                }
            }
            Ok(true)
        })
    }
    pub(super) fn retain_deleted_receipt_cleanup(&self, id: &[u8; 16]) -> Result<()> {
        if let Some(mut rec) = self.receipt(id)? {
            self.delete_received_blocks(&rec)?;
            if rec.cleanup_done {
                self.del_raw(Table::ReceiveIndex, id)?;
            } else {
                rec.state = CANCELLED;
                rec.path = None;
                rec.message_deleted = true;
                self.put_receipt(&rec)?;
                if self.received_spool_bytes()? > MAX_RECEIVED_SPOOL_BYTES {
                    return Err(OmcpError::Store("received-file spool budget"));
                }
            }
        }
        Ok(())
    }
    pub(super) fn delete_receipt(&self, id: &[u8; 16]) -> Result<()> {
        if let Some(rec) = self.receipt(id)? {
            self.delete_received_blocks(&rec)?;
            self.del_raw(Table::ReceiveIndex, id)?;
        }
        Ok(())
    }
    pub(super) fn rekey_receipts(&self, old: &[u8; 32], new: &[u8; 32]) -> Result<()> {
        self.each_receipt(|mut rec| {
            if rec.receipt.peer == *old {
                rec.receipt.peer = *new;
                self.put_receipt(&rec)?;
            }
            Ok(())
        })?;
        if self.received_spool_bytes()? > MAX_RECEIVED_SPOOL_BYTES {
            return Err(OmcpError::Store("received-file spool budget"));
        }
        let pending = self.pending_received_files()?;
        if pending.iter().filter(|r| r.peer == *new).count() > MAX_PENDING_RECEIPTS_PER_PEER {
            return Err(OmcpError::Store("received-file admission limit"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::hooks::{AuthAction, AuthContext, Authorization, TransactionOperation};
    const KEY: [u8; 32] = [9; 32];
    const PEER: [u8; 32] = [2; 32];
    fn message(id: u8, peer: [u8; 32], bytes: &[u8]) -> MessageRec {
        MessageRec {
            id: [id; 16],
            peer,
            mine: false,
            time_ms: 1,
            status: STATUS_RECEIVED,
            text: String::new(),
            mime: None,
            burn: None,
            burn_state: BURN_NONE,
            burn_at_ms: None,
            file: Some(FileRec {
                name: "private-file.bin".into(),
                mime: "application/octet-stream".into(),
                size: bytes.len() as u64,
                state: FILE_RECEIVING,
                done_bytes: 0,
                local_path: None,
                hash: Some(blake3::hash(bytes).as_bytes().to_vec()),
                locator: Some(vec![id; 16]),
            }),
        }
    }
    fn stage(s: &Store, id: u8, peer: [u8; 32], bytes: &[u8]) -> ReceivedFileReceipt {
        s.put_message(&message(id, peer, bytes)).unwrap();
        s.stage_received_file(&peer, &[id; 16], bytes).unwrap()
    }
    fn blocks(s: &Store) -> u64 {
        s.conn
            .query_row("SELECT COUNT(*) FROM receive_blocks", [], |r| r.get(0))
            .unwrap()
    }
    #[test]
    fn receipt_boundaries_stream_and_finalize_without_losing_ownership() {
        for size in [
            0,
            1,
            65_535,
            65_536,
            65_537,
            crate::files::MAX_FILE_BYTES as usize,
        ] {
            let s = Store::open_in_memory(&KEY).unwrap();
            let bytes = Zeroizing::new(vec![0x5a; size]);
            let mut m = message(1, PEER, &bytes);
            m.mine = true;
            s.put_message(&m).unwrap();
            let r = s.stage_received_file(&PEER, &m.id, &bytes).unwrap();
            assert_eq!(s.stage_received_file(&PEER, &m.id, &bytes).unwrap(), r);
            assert_eq!(s.pending_received_files().unwrap(), vec![r.clone()]);
            let mut hash = blake3::Hasher::new();
            for i in 0..r.blocks {
                let b = s.read_received_block(&r.token, i).unwrap();
                assert!(b.len() <= RECEIVED_BLOCK_BYTES);
                hash.update(&b);
            }
            assert_eq!(hash.finalize().as_bytes(), &r.hash);
            assert!(s.read_received_block(&r.token, r.blocks).is_err());
            s.commit_received_file(&r.token, "/private/file").unwrap();
            s.commit_received_file(&r.token, "/private/file").unwrap();
            assert!(s.commit_received_file(&r.token, "/different").is_err());
            assert!(s.read_received_block(&r.token, 0).is_err());
            assert!(s.stage_received_file(&PEER, &m.id, &bytes).is_err());
            assert_eq!(blocks(&s), 0);
            assert!(s.pending_received_files().unwrap().is_empty());
            let m = s.message(&m.id).unwrap().unwrap();
            assert!(m.mine);
            assert_eq!(m.status, STATUS_RECEIVED);
            assert_eq!(m.file.unwrap().state, FILE_RECEIVED);
        }
    }
    #[test]
    fn receipt_restart_cancel_and_deleted_token_never_recreate_message() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spool.db");
        let r = {
            let s = Store::open(&path, &KEY).unwrap();
            let journal: String = s
                .conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            let synchronous: u32 = s
                .conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            assert_eq!(journal, "delete");
            assert_eq!(synchronous, 2, "bundled SQLite default FULL, not EXTRA");

            stage(&s, 1, PEER, b"private payload")
        };
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.pending_received_files().unwrap(), vec![r.clone()]);
        assert_eq!(
            &*s.read_received_block(&r.token, 0).unwrap(),
            b"private payload"
        );
        assert!(s.cancel_received_file(&[3; 32], &[1; 16]).is_err());
        assert!(s.cancel_received_file(&PEER, &[1; 16]).unwrap());
        assert!(!s.cancel_received_file(&PEER, &[1; 16]).unwrap());
        assert!(s.commit_received_file(&r.token, "/private/file").is_err());
        assert!(s
            .stage_received_file(&PEER, &[1; 16], b"private payload")
            .is_err());
        assert_eq!(blocks(&s), 0);
        drop(s);
        let s = Store::open(&path, &KEY).unwrap();
        assert!(s.commit_received_file(&r.token, "/private/file").is_err());
        s.delete_message(&[1; 16]).unwrap();
        assert_eq!(s.cancelled_received_files().unwrap()[0].token, r.token);
        assert!(s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert_eq!(s.received_spool_bytes().unwrap(), 0);
        assert!(s.commit_received_file(&r.token, "/private/file").is_err());
        assert!(s.message(&[1; 16]).unwrap().is_none());
    }
    #[test]
    fn cancelled_cleanup_retains_import_protection_and_rejects_stale_ack() {
        let s = Store::open_in_memory(&KEY).unwrap();
        let r = stage(&s, 1, PEER, b"payload");
        let history = s.export_history().unwrap();
        s.cancel_received_file(&PEER, &[1; 16]).unwrap();
        let mut wrong = r.token.clone();
        wrong.nonce[0] ^= 1;
        assert!(s.acknowledge_received_file_cleanup(&wrong).is_err());
        assert_eq!(s.cancelled_received_files().unwrap(), vec![r.clone()]);
        assert!(s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert!(!s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert!(s.cancelled_received_files().unwrap().is_empty());
        s.import_history(&history).unwrap();
        assert_eq!(
            s.message(&[1; 16]).unwrap().unwrap().file.unwrap().state,
            FILE_FAILED
        );
        assert!(s.stage_received_file(&PEER, &[1; 16], b"payload").is_err());
        s.delete_message(&[1; 16]).unwrap();
        assert!(!s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert_eq!(s.received_spool_bytes().unwrap(), 0);
        s.import_history(&history).unwrap();
        let mut requested = s.message(&[1; 16]).unwrap().unwrap();
        requested.file.as_mut().unwrap().state = FILE_REQUESTED;
        s.put_message(&requested).unwrap();
        let replacement = s.stage_received_file(&PEER, &[1; 16], b"payload").unwrap();
        assert_ne!(replacement.token, r.token);
        assert!(s.acknowledge_received_file_cleanup(&r.token).is_err());
        assert_eq!(s.pending_received_files().unwrap(), vec![replacement]);
    }
    #[test]
    fn deleted_cleanup_requires_absence_and_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deleted.db");
        let s = Store::open(&path, &KEY).unwrap();
        let r = stage(&s, 1, PEER, b"payload");
        s.commit_received_file(&r.token, "/generation-one").unwrap();
        s.delete_message(&[1; 16]).unwrap();
        assert!(s.put_message(&message(1, PEER, b"payload")).is_err());
        drop(s);
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(s.cancelled_received_files().unwrap(), vec![r.clone()]);
        // Simulate incompatible writer/corruption through test-only raw storage.
        s.put(
            Table::Messages,
            &[1; 16],
            Some(&PEER),
            &message(1, PEER, b"payload"),
        )
        .unwrap();
        assert!(s.acknowledge_received_file_cleanup(&r.token).is_err());
        s.del_raw(Table::Messages, &[1; 16]).unwrap();
        assert!(s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert!(!s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert!(s.message(&[1; 16]).unwrap().is_none());
        assert!(s.commit_received_file(&r.token, "/generation-one").is_err());
    }
    #[test]
    fn cleanup_cursor_is_bounded_fair_durable_and_tolerates_ack_deletions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cursor.db");
        let s = Store::open(&path, &KEY).unwrap();
        for id in 1..=40 {
            stage(&s, id, PEER, b"x");
            s.delete_message(&[id; 16]).unwrap();
        }
        let first = s.cancelled_received_files().unwrap();
        assert_eq!(first.len(), 32);
        // Leave the first undeletable obligation; successful later ACKs cannot starve the tail.
        for r in first.iter().skip(1) {
            assert!(s.acknowledge_received_file_cleanup(&r.token).unwrap());
        }
        drop(s);
        let s = Store::open(&path, &KEY).unwrap();
        let second = s.cancelled_received_files().unwrap();
        assert_eq!(second.len(), 8);
        assert!(second.iter().all(|r| !first.contains(r)));
        for r in second {
            s.acknowledge_received_file_cleanup(&r.token).unwrap();
        }
        assert_eq!(
            s.cancelled_received_files().unwrap(),
            vec![first[0].clone()]
        );
        s.acknowledge_received_file_cleanup(&first[0].token)
            .unwrap();
        assert!(s.cancelled_received_files().unwrap().is_empty());
    }
    #[test]
    fn v4_cancelled_groups_migrate_atomically_after_authentication() {
        for commit_failure in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("v4.db");
            let s = Store::open(&path, &KEY).unwrap();
            s.bind_owner(&PEER).unwrap();
            let cancelled = stage(&s, 1, PEER, b"cancel");
            s.cancel_received_file(&PEER, &[1; 16]).unwrap();
            let complete = stage(&s, 2, PEER, b"complete");
            s.commit_received_file(&complete.token, "/complete")
                .unwrap();
            let pending = stage(&s, 3, PEER, b"pending");
            // Old v4 serialization omitted both flags and used peer group.
            #[derive(Serialize)]
            struct OldReceipt {
                receipt: ReceivedFileReceipt,
                mine: bool,
                locator: Option<Vec<u8>>,
                state: u8,
                path: Option<String>,
            }
            let rec = s.receipt(&[1; 16]).unwrap().unwrap();
            s.put(
                Table::ReceiveIndex,
                &[1; 16],
                Some(&PEER),
                &OldReceipt {
                    receipt: rec.receipt,
                    mine: rec.mine,
                    locator: rec.locator,
                    state: rec.state,
                    path: rec.path,
                },
            )
            .unwrap();
            s.conn
                .execute_batch("DROP INDEX receive_index_cleanup; PRAGMA user_version=4;")
                .unwrap();
            drop(s);
            assert!(Store::open(&path, &[8; 32]).is_err());
            assert!(Store::open_for_owner(&path, &KEY, &[7; 32], &[6; 32]).is_err());
            let unchanged = Connection::open(&path).unwrap();
            assert_eq!(
                unchanged
                    .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                    .unwrap(),
                4
            );
            drop(unchanged);
            if commit_failure {
                let conn = Connection::open(&path).unwrap();
                conn.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
                    AuthAction::Transaction {
                        operation: TransactionOperation::Unknown,
                    } => Authorization::Deny,
                    _ => Authorization::Allow,
                }));
                assert!(Store::init(conn, &KEY, None).is_err());
                let check = Connection::open(&path).unwrap();
                assert_eq!(
                    check
                        .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                        .unwrap(),
                    4
                );
            }
            let s = Store::open(&path, &KEY).unwrap();
            assert_eq!(s.schema_version().unwrap(), 5);
            assert_eq!(s.cancelled_received_files().unwrap(), vec![cancelled]);
            assert_eq!(s.pending_received_files().unwrap(), vec![pending]);
            s.commit_received_file(&complete.token, "/complete")
                .unwrap();
        }
    }
    #[test]
    fn legacy_metadata_rekey_growth_refuses_budget_overflow_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("budget.db");
        let s = Store::open(&path, &KEY).unwrap();
        let r = stage(&s, 1, PEER, b"a");
        s.commit_received_file(&r.token, "/private").unwrap();
        #[derive(Serialize)]
        struct OldReceipt {
            receipt: ReceivedFileReceipt,
            mine: bool,
            locator: Option<Vec<u8>>,
            state: u8,
            path: Option<String>,
        }
        let rec = s.receipt(&[1; 16]).unwrap().unwrap();
        s.put(
            Table::ReceiveIndex,
            &[1; 16],
            Some(&PEER),
            &OldReceipt {
                receipt: rec.receipt,
                mine: rec.mine,
                locator: rec.locator,
                state: rec.state,
                path: rec.path,
            },
        )
        .unwrap();
        let before: Vec<u8> = s
            .conn
            .query_row("SELECT ct FROM receive_index", [], |r| r.get(0))
            .unwrap();
        // An unrelated opaque block occupies remaining quota; no plaintext allocation.
        let filler = MAX_RECEIVED_SPOOL_BYTES - s.received_spool_bytes().unwrap() - ROW_ALLOWANCE;
        s.conn
            .execute(
                "INSERT INTO receive_blocks(id,grp,seq,ct) VALUES(?1,NULL,1,zeroblob(?2))",
                params![[7u8; 32].as_slice(), filler],
            )
            .unwrap();
        assert_eq!(s.received_spool_bytes().unwrap(), MAX_RECEIVED_SPOOL_BYTES);
        assert!(s.rekey_peer(&PEER, &[3; 32]).is_err());
        assert_eq!(s.message(&[1; 16]).unwrap().unwrap().peer, PEER);
        let after: Vec<u8> = s
            .conn
            .query_row("SELECT ct FROM receive_index", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(s.received_spool_bytes().unwrap(), MAX_RECEIVED_SPOOL_BYTES);
    }
    #[test]
    fn receipt_rejects_burn_hash_owner_and_forged_token_and_mutation() {
        let s = Store::open_in_memory(&KEY).unwrap();
        let mut m = message(1, PEER, b"a");
        m.burn = Some(vec![0; 9]);
        s.put_message(&m).unwrap();
        assert!(s.stage_received_file(&PEER, &m.id, b"a").is_err());
        m.burn = None;
        s.put_message(&m).unwrap();
        assert!(s.stage_received_file(&[3; 32], &m.id, b"a").is_err());
        assert!(s.stage_received_file(&PEER, &m.id, b"b").is_err());
        let r = s.stage_received_file(&PEER, &m.id, b"a").unwrap();
        let mut bad = r.token.clone();
        bad.nonce[0] ^= 1;
        assert!(s.read_received_block(&bad, 0).is_err());
        assert!(s.commit_received_file(&bad, "/file").is_err());
        for mutation in 0..5 {
            let mut m = s.message(&[1; 16]).unwrap().unwrap();
            match mutation {
                0 => m.peer = [3; 32],
                1 => m.mine = true,
                2 => m.file.as_mut().unwrap().name = "changed".into(),
                3 => m.file.as_mut().unwrap().state = FILE_RECEIVED,
                _ => m.file.as_mut().unwrap().done_bytes = 0,
            };
            assert!(s.put_message(&m).is_err());
        }
        assert_eq!(s.pending_received_files().unwrap(), vec![r]);
    }
    #[test]
    fn receipt_corrupt_missing_reordered_or_wrong_hash_blocks_cannot_commit() {
        for fault in 0..4 {
            let s = Store::open_in_memory(&KEY).unwrap();
            let bytes = vec![7; 65_537];
            let r = stage(&s, 1, PEER, &bytes);
            match fault {
                0 => {
                    s.del_raw(Table::ReceiveBlocks, &block_id(&r.token, 0))
                        .unwrap();
                }
                1 => {
                    s.put_raw(
                        Table::ReceiveBlocks,
                        &block_id(&r.token, 0),
                        Some(&r.token.nonce),
                        &vec![8; 65_536],
                    )
                    .unwrap();
                }
                2 => {
                    s.conn
                        .execute("UPDATE receive_blocks SET ct=zeroblob(length(ct))", [])
                        .unwrap();
                }
                _ => {
                    let a = s.rid(Table::ReceiveBlocks, &block_id(&r.token, 0));
                    let b = s.rid(Table::ReceiveBlocks, &block_id(&r.token, 1));
                    s.conn.execute("UPDATE receive_blocks SET ct=(SELECT ct FROM receive_blocks WHERE id=?1) WHERE id=?2",params![a.as_slice(),b.as_slice()]).unwrap();
                }
            }
            assert!(s.commit_received_file(&r.token, "/file").is_err());
            assert_eq!(
                s.message(&[1; 16]).unwrap().unwrap().file.unwrap().state,
                FILE_FINALIZING
            );
            assert!(s.has_receipt(&[1; 16]).unwrap());
        }
    }
    #[test]
    fn receipt_admission_counts_persisted_peers_and_global_slots() {
        let s = Store::open_in_memory(&KEY).unwrap();
        for id in 1..=8 {
            stage(&s, id, [id.div_ceil(2); 32], b"a");
        }
        s.put_message(&message(9, [9; 32], b"a")).unwrap();
        assert!(s.stage_received_file(&[9; 32], &[9; 16], b"a").is_err());
        assert_eq!(s.pending_received_files().unwrap().len(), 8);
        s.cancel_received_file(&[1; 32], &[1; 16]).unwrap();
        s.stage_received_file(&[9; 32], &[9; 16], b"a").unwrap();
        s.cancel_received_file(&[1; 32], &[2; 16]).unwrap();
        s.put_message(&message(10, [2; 32], b"a")).unwrap();
        assert!(s.stage_received_file(&[2; 32], &[10; 16], b"a").is_err());
    }
    #[test]
    fn receipt_ciphertext_budget_includes_blocks_index_and_completion_reservations() {
        let s = Store::open_in_memory(&KEY).unwrap();
        let bytes = Zeroizing::new(vec![1; 44_000_000]);
        let a = stage(&s, 1, [1; 32], &bytes);
        stage(&s, 2, [2; 32], &bytes);
        let used = s.received_spool_bytes().unwrap();
        assert!(used > 88_000_000 + 2 * COMPLETION_RESERVATION);
        let big = Zeroizing::new(vec![2; 46_200_000]);
        s.put_message(&message(3, [3; 32], &big)).unwrap();
        assert!(s.stage_received_file(&[3; 32], &[3; 16], &big).is_err());
        assert_eq!(s.received_spool_bytes().unwrap(), used);
        s.commit_received_file(&a.token, &"x".repeat(4096)).unwrap();
        assert!(s.received_spool_bytes().unwrap() < used);
    }
    #[test]
    fn receipt_rekey_and_backup_preserve_pending_and_do_not_export_capabilities() {
        let s = Store::open_in_memory(&KEY).unwrap();
        let r = stage(&s, 1, PEER, b"secret payload");
        let history = s.export_history().unwrap();
        assert!(!history.windows(32).any(|w| w == r.token.nonce));
        assert!(!history.windows(14).any(|w| w == b"secret payload"));
        s.import_history(&history).unwrap();
        assert_eq!(s.pending_received_files().unwrap(), vec![r.clone()]);
        let restored = Store::open_in_memory(&[8; 32]).unwrap();
        restored.import_history(&history).unwrap();
        assert!(restored.pending_received_files().unwrap().is_empty());
        assert_eq!(
            restored
                .message(&[1; 16])
                .unwrap()
                .unwrap()
                .file
                .unwrap()
                .state,
            FILE_MISSING
        );
        s.rekey_peer(&PEER, &[3; 32]).unwrap();
        let pending = s.pending_received_files().unwrap();
        assert_eq!(pending[0].peer, [3; 32]);
        assert_eq!(pending[0].token, r.token);
        assert!(s.cancel_received_file(&PEER, &[1; 16]).is_err());
        s.commit_received_file(&r.token, "/file").unwrap();
        let completed = s.export_history().unwrap();
        restored.import_history(&completed).unwrap_err(); // same ID, different owning peer
        s.import_history(&completed).unwrap();
        s.commit_received_file(&r.token, "/file").unwrap();
        s.delete_message(&[1; 16]).unwrap();
        assert_eq!(s.cancelled_received_files().unwrap()[0].token, r.token);
        assert!(s.acknowledge_received_file_cleanup(&r.token).unwrap());
        assert_eq!(s.received_spool_bytes().unwrap(), 0);
    }
    #[test]
    fn receipt_rekey_collision_rolls_back_and_contact_delete_cancels_only_its_receipts() {
        let s = Store::open_in_memory(&KEY).unwrap();
        stage(&s, 1, PEER, b"a");
        stage(&s, 2, [3; 32], b"a");
        stage(&s, 3, [3; 32], b"a");
        assert!(s.rekey_peer(&PEER, &[3; 32]).is_err());
        assert_eq!(s.message(&[1; 16]).unwrap().unwrap().peer, PEER);
        assert_eq!(s.pending_received_files().unwrap().len(), 3);
        s.delete_contact(&PEER).unwrap();
        assert_eq!(s.pending_received_files().unwrap().len(), 2);
        assert_eq!(
            s.message(&[1; 16]).unwrap().unwrap().file.unwrap().state,
            FILE_FAILED
        );
    }
    fn deny_commit(ctx: AuthContext<'_>) -> Authorization {
        match ctx.action {
            AuthAction::Transaction {
                operation: TransactionOperation::Unknown,
            } => Authorization::Deny,
            _ => Authorization::Allow,
        }
    }
    #[test]
    fn receipt_readonly_and_commit_fault_leave_stage_and_cancel_atomic() {
        let s = Store::open_in_memory(&KEY).unwrap();
        s.put_message(&message(1, PEER, b"a")).unwrap();
        s.conn.execute_batch("PRAGMA query_only=ON").unwrap();
        assert!(s.stage_received_file(&PEER, &[1; 16], b"a").is_err());
        s.conn.execute_batch("PRAGMA query_only=OFF").unwrap();
        assert_eq!(blocks(&s), 0);
        assert!(s.pending_received_files().unwrap().is_empty());
        s.conn.authorizer(Some(deny_commit));
        assert!(s.stage_received_file(&PEER, &[1; 16], b"a").is_err());
        s.conn
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(blocks(&s), 0);
        assert!(s.pending_received_files().unwrap().is_empty());
        let r = s.stage_received_file(&PEER, &[1; 16], b"a").unwrap();
        s.conn.authorizer(Some(deny_commit));
        assert!(s.cancel_received_file(&PEER, &[1; 16]).is_err());
        s.conn
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(s.pending_received_files().unwrap(), vec![r.clone()]);
        assert_eq!(blocks(&s), 1);
        s.conn.authorizer(Some(deny_commit));
        assert!(s.commit_received_file(&r.token, "/file").is_err());
        s.conn
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        assert_eq!(s.pending_received_files().unwrap(), vec![r]);
        assert_eq!(blocks(&s), 1);
    }

    #[test]
    fn receipt_active_outgoing_work_and_imported_outbox_cannot_claim_receive() {
        let s = Store::open_in_memory(&KEY).unwrap();
        let mut m = message(1, PEER, b"a");
        m.mine = true;
        m.file.as_mut().unwrap().state = FILE_PAUSED;
        s.put_message(&m).unwrap();
        let f = OutFileRec {
            id: m.id,
            peer: PEER,
            name: "active".into(),
            size: 1,
            total: 1,
            offer: vec![7],
        };
        s.put_out_file(&f, &[b"a"]).unwrap();
        assert!(s.stage_received_file(&PEER, &m.id, b"a").is_err());
        assert_eq!(s.message(&m.id).unwrap(), Some(m));
        assert_eq!(s.out_files().unwrap().len(), 1);
        s.delete_out_file(&f.id).unwrap();
        let q = OutboxRec {
            id: f.id,
            peer: PEER,
            created_ms: 1,
            inner: vec![1],
            control: false,
            burn: false,
        };
        s.put_outbox(&q, None).unwrap();
        assert!(s.stage_received_file(&PEER, &f.id, b"a").is_err());
        s.delete_outbox(&f.id).unwrap();
        let r = s.stage_received_file(&PEER, &f.id, b"a").unwrap();
        assert!(s.put_out_file(&f, &[b"a"]).is_err());
        assert!(s.put_outbox(&q, None).is_err());
        let mut h: History = uncbor(&s.export_history().unwrap()).unwrap();
        h.outbox.push(q);
        s.import_history(&cbor(&h).unwrap()).unwrap();
        assert!(s.outbox().unwrap().is_empty());
        assert_eq!(s.pending_received_files().unwrap(), vec![r]);
    }
    #[test]
    fn receipt_full_and_busy_database_fail_without_partial_spool() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fault.db");
        let s = Store::open(&path, &KEY).unwrap();
        let bytes = vec![1; 65_537];
        s.put_message(&message(1, PEER, &bytes)).unwrap();
        s.conn.busy_timeout(std::time::Duration::ZERO).unwrap();
        let other = Connection::open(&path).unwrap();
        other.execute_batch("BEGIN IMMEDIATE").unwrap();
        assert!(s.stage_received_file(&PEER, &[1; 16], &bytes).is_err());
        other.execute_batch("ROLLBACK").unwrap();
        let pages: u64 = s
            .conn
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        s.conn
            .execute_batch(&format!("PRAGMA max_page_count={pages}"))
            .unwrap();
        assert!(s.stage_received_file(&PEER, &[1; 16], &bytes).is_err());
        // SQLITE_FULL may auto-rollback; the existing transaction guard correctly
        // quarantines an uncertain connection rather than accepting more writes.
        assert!(s.poisoned.get());
        assert!(s.pending_received_files().is_err());
        drop(s);
        let s = Store::open(&path, &KEY).unwrap();
        assert_eq!(blocks(&s), 0);
        assert!(s.pending_received_files().unwrap().is_empty());
    }
    #[test]
    #[ignore = "subprocess fixture; parent exercises abrupt exit without destructors"]
    fn receipt_crash_child() {
        let path = std::env::var("AURALIS_RECEIPT_CRASH_PATH").unwrap();
        let mode = std::env::var("AURALIS_RECEIPT_CRASH_MODE").unwrap();
        let s = Store::open(Path::new(&path), &KEY).unwrap();
        if mode == "stage" {
            stage(&s, 1, PEER, b"restart sole copy");
        } else {
            let r = s.pending_received_files().unwrap().remove(0);
            if mode == "commit" {
                s.commit_received_file(&r.token, "/private/file").unwrap();
            } else {
                s.cancel_received_file(&PEER, &r.token.message_id).unwrap();
            }
        }
        std::process::exit(0);
    }
    #[test]
    fn receipt_abrupt_exit_after_stage_finalize_and_cancel_is_recoverable() {
        for mode in ["commit", "cancel"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("crash.db");
            let run = |mode: &str| {
                let output = std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "store::receipts::tests::receipt_crash_child",
                        "--ignored",
                    ])
                    .env("AURALIS_RECEIPT_CRASH_PATH", &path)
                    .env("AURALIS_RECEIPT_CRASH_MODE", mode)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            };
            run("stage");
            let token = {
                let s = Store::open(&path, &KEY).unwrap();
                let r = s.pending_received_files().unwrap().remove(0);
                assert_eq!(
                    &*s.read_received_block(&r.token, 0).unwrap(),
                    b"restart sole copy"
                );
                r.token
            };
            run(mode);
            let s = Store::open(&path, &KEY).unwrap();
            assert!(s.pending_received_files().unwrap().is_empty());
            assert_eq!(blocks(&s), 0);
            if mode == "commit" {
                s.commit_received_file(&token, "/private/file").unwrap();
            } else {
                assert!(s.commit_received_file(&token, "/private/file").is_err());
            }
        }
    }

    #[test]
    fn receipt_guard_and_message_write_hold_one_writer_transaction() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc,
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.db");
        let s = Store::open(&path, &KEY).unwrap();
        s.put_message(&message(1, PEER, b"a")).unwrap();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (start_tx, start_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let second = Store::open(&path, &KEY).unwrap();
            second.conn.busy_timeout(std::time::Duration::ZERO).unwrap();
            ready_tx.send(()).unwrap();
            start_rx.recv().unwrap();
            let blocked = second.stage_received_file(&PEER, &[1; 16], b"a").is_err();
            done_tx.send(blocked).unwrap();
        });
        ready_rx.recv().unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let flag = observed.clone();
        s.conn.authorizer(Some(move |ctx: AuthContext<'_>| {
            if matches!(
                ctx.action,
                AuthAction::Insert {
                    table_name: "messages"
                }
            ) && !flag.swap(true, Ordering::SeqCst)
            {
                start_tx.send(()).unwrap();
                assert!(
                    done_rx
                        .recv_timeout(std::time::Duration::from_secs(3))
                        .unwrap(),
                    "another writer staged between receipt guard and message write"
                );
            }
            Authorization::Allow
        }));
        let mut changed = message(1, PEER, b"a");
        changed.peer = [8; 32];
        s.put_message(&changed).unwrap();
        s.conn
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        worker.join().unwrap();
        assert!(observed.load(Ordering::SeqCst));
        assert!(s.pending_received_files().unwrap().is_empty());
        assert_eq!(s.message(&[1; 16]).unwrap().unwrap().peer, [8; 32]);
    }
}
