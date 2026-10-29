package chat.omcp.app

import chat.omcp.app.core.*
import chat.omcp.app.data.*
import chat.omcp.app.files.*
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread

class ReceiptFinalizationTest {
    private class Core : OmcpCore by FakeOmcpCore() {
        val receipt = ReceiveReceiptInfo("11".repeat(16), "22".repeat(32), "file", "application/octet-stream", 3, "11".repeat(16) + "33".repeat(32), "44".repeat(32), 1)
        var conflictingCompleted = false
        var cleanup = false
        var cleanupAckFails = false
        var cleanupLists = 0
        var cleanupAcks = 0
        var pending = true
        var state = "Finalizing"
        var path: String? = null
        var beforeCommit: (() -> Unit)? = null
        var commits = 0
        var retryFails = false
        var commitFails = false
        var cancelFails = false
        var cancelResend = false
        var missingCalls = 0
        var reads = 0
        override fun contacts() = listOf(Contact(receipt.contactId, "Peer", emptyList(), false))
        override fun storedMessages(contactId: String) = if (state == "Deleted") emptyList() else listOf(
            StoredMessageInfo(receipt.transferId, receipt.contactId, "", true, 1, "Sent", null, null, "None", null,
                StoredFileInfo(receipt.name, receipt.mime, receipt.size, state, 3, path))) +
            if (conflictingCompleted) listOf(StoredMessageInfo("other", receipt.contactId, "", true, 1, "Sent", null, null, "None", null,
                StoredFileInfo("resend", "text/plain", 3, "Received", 3, "/private/absent"))) else emptyList()
        override fun pendingReceivedFiles() = if (pending) listOf(receipt) else emptyList()
        override fun cancelledReceivedFiles(): List<ReceiveReceiptInfo> { cleanupLists++; return if (cleanup) listOf(receipt) else emptyList() }
        override fun acknowledgeReceivedFileCleanup(receiptToken: String): Boolean {
            check(receiptToken == receipt.receiptToken)
            if (cleanupAckFails) error("cleanup ACK unavailable")
            cleanupAcks++; val existed = cleanup; cleanup = false; return existed
        }
        override fun retryReceivedFiles(): List<ReceiveReceiptInfo> { if (retryFails) error("another candidate failed"); return pendingReceivedFiles() }
        override fun readReceivedFileBlock(receiptToken: String, index: Int): ByteArray {
            check(pending && receiptToken == receipt.receiptToken && index == 0)
            reads++
            return byteArrayOf(1, 2, 3)
        }
        override fun commitReceivedFile(receiptToken: String, localPath: String) {
            check(pending && receiptToken == receipt.receiptToken)
            beforeCommit?.invoke()
            if (commitFails) error("commit unavailable")
            commits++; pending = false; state = "Received"; path = localPath
        }
        override fun cancelFile(transferId: String): Boolean {
            if (cancelFails) error("cancel database failure")
            if (!pending) return cancelResend
            pending = false; state = "Deleted"; return true
        }
        override fun markReceivedFileMissing(transferId: String, expectedLocalPath: String): Boolean {
            if (transferId == "other") error("reciprocal resend still owns file")
            check(state == "Received" && expectedLocalPath == path)
            missingCalls++; state = "Missing"; path = null; return true
        }
    }
    private fun message(repo: MeshRepository) = repo.messages.value.values.flatten().single()
    private fun repo(core: Core) = MeshRepository(core, InMemoryAppStore(), receiptExecutor = java.util.concurrent.Executor { it.run() })

    @Test fun trailingClockLambdaRemainsClockNotReceiptExecutor() {
        val r = MeshRepository(FakeOmcpCore(), InMemoryAppStore()) { 42L }
        r.saveIdentity((1..24).joinToString(" ") { "word$it" }, "Alice")
        val peer = r.addContact("omcp:1:${"b".repeat(64)}:Bob", false)
        r.send(peer.id, "hello")
        assertEquals(42L, message(r).timeMs)
    }

    @Test fun nilSinkAndWriteOrCommitFailureKeepSoleDurableReceiptFinalizing() {
        for (fault in listOf("nil", "write", "file-sync", "rename", "directory-sync", "commit")) {
            val c = Core(); val r = repo(c)
            if (fault != "nil") r.receiptSink = DurableReceiptSink { _, _ -> if (fault != "commit") error(fault); "/private/final" }
            c.commitFails = fault == "commit"
            r.onFileReceived(c.receipt)
            assertTrue(c.pending); assertEquals(0, c.commits)
            assertEquals(FileAttachment.State.Finalizing, message(r).file!!.state)
            assertTrue(message(r).mine); assertTrue(r.receiveFailure.value)
        }
    }
    @Test fun retryAfterRenameBeforeCommitPreservesOwnershipAndCommitsOnlyOnce() {
        val c = Core(); val r = repo(c); var attempts = 0
        r.receiptSink = DurableReceiptSink { _, _ -> attempts++; "/private/stable-generation" }
        c.commitFails = true; r.onFileReceived(c.receipt)
        c.commitFails = false; assertTrue(r.retryFileFinalization().get(3, TimeUnit.SECONDS))
        assertEquals(2, attempts); assertEquals(1, c.commits); assertTrue(message(r).mine)
        assertEquals(ChatMessage.Status.Sent, message(r).status)
        assertEquals(FileAttachment.State.Received, message(r).file!!.state)
        r.onFileReceived(c.receipt); assertEquals(2, attempts); assertEquals(1, c.commits)
    }
    @Test fun cancelledTokenAndDatabaseCancellationFailureNeverInventSuccess() {
        val c = Core(); val r = repo(c)
        r.onFileReceived(c.receipt)
        c.cancelFails = true
        assertThrows(IllegalStateException::class.java) { r.cancelFile(c.receipt.contactId, c.receipt.transferId) }
        assertTrue(c.pending); assertEquals(FileAttachment.State.Finalizing, message(r).file!!.state)
        c.cancelFails = false; r.cancelFile(c.receipt.contactId, c.receipt.transferId)
        var writes = 0; r.receiptSink = DurableReceiptSink { _, _ -> writes++; "/file" }
        r.onFileReceived(c.receipt); assertEquals(0, writes); assertEquals(0, c.commits)
        assertTrue(r.messages.value.values.flatten().isEmpty())
    }
    @Test fun cancellationCannotReturnBeforeSerializedWriterHasFinished() {
        val c = Core(); val r = repo(c)
        val entered = CountDownLatch(1); val release = CountDownLatch(1); val cancelled = CountDownLatch(1)
        r.receiptSink = DurableReceiptSink { _, _ -> entered.countDown(); check(release.await(3, TimeUnit.SECONDS)); "/file" }
        val writer = thread { r.onFileReceived(c.receipt) }
        assertTrue(entered.await(3, TimeUnit.SECONDS))
        val cancel = thread { r.cancelFile(c.receipt.contactId, c.receipt.transferId); cancelled.countDown() }
        assertFalse(cancelled.await(100, TimeUnit.MILLISECONDS))
        release.countDown(); writer.join(3000); cancel.join(3000)
        assertFalse(writer.isAlive); assertFalse(cancel.isAlive); assertEquals(1, c.commits)
        assertEquals(FileAttachment.State.Received, message(r).file!!.state)
    }
    @Test fun onlyPositiveMissingClassificationRetiresCurrentPath() {
        for (missing in listOf(false, true)) {
            val c = Core(); val r = repo(c)
            r.receiptSink = object : DurableReceiptSink {
                override fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource) = "/private/file"
                override fun isDefinitelyMissing(path: String) = missing
            }
            r.onFileReceived(c.receipt); r.reconcileMissingFiles()
            assertEquals(if (missing) 1 else 0, c.missingCalls)
            assertEquals(if (missing) FileAttachment.State.Missing else FileAttachment.State.Received, message(r).file!!.state)
        }
    }
    @Test fun cancellingReciprocalResendPreservesCompletedHistoryAndPath() {
        val c = Core(); val r = repo(c)
        r.receiptSink = DurableReceiptSink { _, _ -> "/private/retained" }
        r.onFileReceived(c.receipt)
        c.cancelResend = true
        r.cancelFile(c.receipt.contactId, c.receipt.transferId)
        assertEquals(FileAttachment.State.Received, message(r).file!!.state)
        assertEquals("/private/retained", message(r).file!!.localPath)
        assertTrue(message(r).mine)
    }

    @Test fun startupRecoveryIsScheduledAndDoesNotRunOnTheCallingThread() {
        val c = Core(); val queued = mutableListOf<Runnable>()
        val r = MeshRepository(c, InMemoryAppStore(), receiptExecutor = java.util.concurrent.Executor { queued += it })
        var writes = 0
        r.receiptSink = DurableReceiptSink { _, _ -> writes++; "/private/file" }
        val future = r.retryFileFinalization()
        assertEquals(0, writes); assertFalse(future.isDone)
        queued.removeAt(0).run()
        assertTrue(future.get(3, TimeUnit.SECONDS)); assertEquals(1, writes)
    }

    @Test fun failedCandidateRetryDoesNotBlockAlreadyDurableReceipts() {
        val c = Core(); val r = repo(c); c.retryFails = true
        r.receiptSink = DurableReceiptSink { _, _ -> "/private/file" }
        assertFalse(r.retryFileFinalization().get(3, TimeUnit.SECONDS))
        assertEquals(1, c.commits); assertFalse(c.pending)
        assertEquals(FileAttachment.State.Received, message(r).file!!.state)
        assertTrue(r.receiveFailure.value)
    }

    @Test fun cancelledCleanupIsBoundedAndRetriedUntilDirectoryAndAckSucceed() {
        val c = Core().apply { pending = false; state = "Deleted"; cleanup = true; retryFails = true }
        val r = repo(c)
        assertFalse(r.retryFileFinalization().get()) // No sink cannot acknowledge cleanup.
        assertTrue(c.cleanup); assertEquals(0, c.cleanupAcks)
        var discards = 0
        r.receiptSink = object : DurableReceiptSink {
            override fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource): String = error("cancelled")
            override fun discard(receipt: ReceiveReceiptInfo) { discards++; if (discards == 1) error("directory sync") }
        }
        assertFalse(r.retryFileFinalization().get())
        assertTrue(c.cleanup); assertEquals(0, c.cleanupAcks)
        c.cleanupAckFails = true
        assertFalse(r.retryFileFinalization().get())
        assertTrue(c.cleanup)
        c.cleanupAckFails = false
        assertFalse(r.retryFileFinalization().get()) // Candidate failure doesn't block cleanup.
        assertFalse(c.cleanup); assertEquals(1, c.cleanupAcks)
        assertEquals(4, c.cleanupLists) // Exactly one bounded page per explicit pass.
        assertEquals(3, discards)
        assertTrue(r.receiveFailure.value)
        c.retryFails = false
        assertTrue(r.retryFileFinalization().get())
        assertFalse(r.receiveFailure.value)
    }

    @Test fun oneMissingResendConflictDoesNotStarveIndependentPendingFinalization() {
        val c = Core().apply { conflictingCompleted = true }
        val r = repo(c)
        r.saveIdentity((1..24).joinToString(" ") { "word$it" }, "Alice")
        r.restore() // Load both independent pending and completed-resend records.
        r.receiptSink = object : DurableReceiptSink {
            override fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource) = "/private/new"
            override fun isDefinitelyMissing(path: String) = path == "/private/absent"
        }
        assertFalse(r.retryFileFinalization().get())
        assertEquals(1, c.commits); assertFalse(c.pending)
        assertTrue(r.receiveFailure.value)
        assertEquals("Received", c.state)
    }

    @Test fun candidatePathIsNeverExposedBeforeSuccessfulDatabaseCommit() {
        val c = Core(); val r = repo(c)
        var candidateCreated = false
        fun assertUnpublished() {
            assertEquals(FileAttachment.State.Finalizing, message(r).file!!.state)
            assertNull(message(r).file!!.localPath)
        }
        r.receiptSink = DurableReceiptSink { _, source ->
            assertTrue(source.authorizeWrite(replacingIncomplete = false))
            assertTrue(source.authorizeWrite(replacingIncomplete = true))
            candidateCreated = true
            assertUnpublished()
            "/private/candidate"
        }
        c.beforeCommit = { assertTrue(candidateCreated); assertUnpublished() }
        c.commitFails = true
        r.onFileReceived(c.receipt)
        assertTrue(candidateCreated); assertTrue(c.pending); assertUnpublished()
        c.commitFails = false
        assertTrue(r.retryFileFinalization().get())
        assertEquals(FileAttachment.State.Received, message(r).file!!.state)
        assertEquals("/private/candidate", message(r).file!!.localPath)
    }

}
