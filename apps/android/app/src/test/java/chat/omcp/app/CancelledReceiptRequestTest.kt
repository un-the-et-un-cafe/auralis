package chat.omcp.app

import chat.omcp.app.core.*
import chat.omcp.app.data.*
import org.junit.Assert.*
import org.junit.Test

class CancelledReceiptRequestTest {
    private class Core : OmcpCore by FakeOmcpCore() {
        var receipt = ReceiveReceiptInfo("11".repeat(16), "22".repeat(32), "file", "application/octet-stream", 3,
            "11".repeat(16) + "33".repeat(32), "44".repeat(32), 1)
        var cleanupDone = true
        var state = "Failed"
        var requestFails = false
        var lookupFails = false
        var lookups = 0
        var requests = 0
        override fun contacts() = listOf(Contact(receipt.contactId, "Peer", emptyList(), false))
        override fun storedMessages(contactId: String) = listOf(StoredMessageInfo(receipt.transferId, receipt.contactId, "", true, 1, "Sent", null, null, "None", null,
            StoredFileInfo(receipt.name, receipt.mime, receipt.size, state, 0, null)))
        override fun recoverableReceivedFile(contactId: String, transferId: String): ReceiveReceiptInfo? {
            lookups++
            if (lookupFails) error("store unavailable")
            return receipt.takeIf { cleanupDone && state == "Failed" && contactId == it.contactId && transferId == it.transferId }
        }
        override fun requestCancelledReceivedFile(receipt: ReceiveReceiptInfo) {
            check(recoverableReceivedFile(receipt.contactId, receipt.transferId) == receipt)
            if (requestFails) error("transaction failed")
            requests++; state = "Requested"
        }
    }
    private fun repo(c: Core): MeshRepository = MeshRepository(c, InMemoryAppStore().apply { recoveryPhrase = (1..24).joinToString(" ") { "word$it" } }, receiptExecutor = java.util.concurrent.Executor { it.run() }).also { it.restore() }
    private fun discover(r: MeshRepository, c: Core): ReceiptRequestAction? {
        r.discoverRequestableFiles(c.receipt.contactId, listOf(c.receipt.transferId)).get()
        return r.requestableReceipts.value[c.receipt.transferId]
    }
    @Test fun restartDiscoveryRequiresCleanupAndNeverSendsAutomatically() {
        val c = Core(); c.cleanupDone = false; val r = repo(c)
        assertNull(discover(r,c)); assertEquals(0,c.requests)
        c.cleanupDone = true
        val action = discover(r,c)!!
        assertEquals(action, discover(r,c)) // harmless repeat keeps a live confirmation usable
        assertEquals(0,c.requests)
        assertTrue(r.requestCancelledFileAgain(c.receipt.transferId, action.actionId).get())
        assertEquals("Requested", c.state); assertEquals(1,c.requests)
        assertEquals(FileAttachment.State.Requested, r.messages.value.values.flatten().single().file!!.state)
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId, action.actionId).get())
        assertNull(r.requestableReceipts.value[c.receipt.transferId])
    }
    @Test fun oldConfirmationRejectsRekeySameTokenAndReloadThenRediscovery() {
        val c = Core(); val r = repo(c); val old = discover(r,c)!!
        c.receipt = c.receipt.copy(name = "changed metadata")
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId, old.actionId).get())
        val newer = discover(r,c)!!
        r.restore()
        val afterReload = discover(r,c)!!
        assertNotEquals(newer.actionId, afterReload.actionId)
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId, newer.actionId).get())
        c.receipt = c.receipt.copy(contactId = "55".repeat(32)) // actual rekey keeps token
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId, afterReload.actionId).get())
        assertEquals(0,c.requests)
    }
    @Test fun failureConsumesConfirmationButPreservesFailedAndNoTransportKick() {
        val c = Core(); val r = repo(c); var kicks = 0; r.kick = MeshKick { kicks++ }
        val action = discover(r,c)!!; c.requestFails = true
        val revision = r.receiptEligibilityRevision.value
        assertTrue(runCatching { r.requestCancelledFileAgain(c.receipt.transferId, action.actionId).get() }.isFailure)
        assertEquals("Failed",c.state); assertEquals(0,c.requests); assertEquals(0,kicks)
        assertTrue(r.receiptEligibilityRevision.value > revision) // stationary visible collector rediscovers
        assertNull(r.requestableReceipts.value[c.receipt.transferId])
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId, action.actionId).get())
        c.requestFails = false; val newAction = discover(r,c)!!
        assertNotEquals(action.actionId,newAction.actionId)
        assertTrue(r.requestCancelledFileAgain(c.receipt.transferId,newAction.actionId).get()); assertEquals(1,kicks)
    }
    @Test fun wrongActionsAndNoLongerRecoverableSnapshotsCannotRequest() {
        val c = Core(); val r = repo(c); val a = discover(r,c)!!
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId,"wrong").get())
        c.cleanupDone = false
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId,a.actionId).get())
        c.cleanupDone = true; val b = discover(r,c)!!; c.state = "Received"
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId,b.actionId).get())
        assertEquals(0,c.requests)
    }
    @Test fun unavailableLookupCannotSendAndRestoredStorageNeedsFreshConfirmation() {
        val c = Core(); val r = repo(c); val a = discover(r,c)!!
        c.lookupFails = true
        assertTrue(runCatching { r.requestCancelledFileAgain(c.receipt.transferId,a.actionId).get() }.isFailure)
        assertEquals("Failed",c.state); assertEquals(0,c.requests)
        assertTrue(r.requestableReceipts.value.isEmpty())
        c.lookupFails = false
        val b = discover(r,c)!!
        assertNotEquals(a.actionId,b.actionId)
        assertFalse(r.requestCancelledFileAgain(c.receipt.transferId,a.actionId).get())
        assertEquals(0,c.requests)
    }
    @Test fun visibleDiscoveryIsBoundedAndOffscreenNeverLookedUpOrRequested() {
        val c = Core(); val r = repo(c)
        r.discoverRequestableFiles(c.receipt.contactId,(0..40).map { "other$it" } + c.receipt.transferId).get()
        assertEquals(0,c.lookups); assertTrue(r.requestableReceipts.value.isEmpty())
        discover(r,c); assertEquals(1,c.lookups)
        r.discoverRequestableFiles(c.receipt.contactId,emptyList()).get()
        assertTrue(r.requestableReceipts.value.isEmpty()); assertEquals(0,c.requests)
    }
}
