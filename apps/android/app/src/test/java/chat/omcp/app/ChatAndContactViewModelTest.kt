package chat.omcp.app

import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.NetworkType
import chat.omcp.app.core.PathKind
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.ui.vm.ChatThreadViewModel
import chat.omcp.app.ui.vm.ContactAddViewModel
import chat.omcp.app.ui.vm.GatewayViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import chat.omcp.app.core.FileFailureInfo
import chat.omcp.app.core.FileProgressInfo
import chat.omcp.app.core.ReceivedFileInfo
import chat.omcp.app.data.FileAttachment
import chat.omcp.app.data.FileSink
import org.junit.Test

class ChatAndContactViewModelTest {
    private val core = FakeOmcpCore()
    private val store = InMemoryAppStore()
    private val repo = MeshRepository(core, store) { 42L }
        .also { it.saveIdentity((1..24).joinToString(" ") { i -> "word$i" }, "Alice") }
    private val bobQr = "omcp:1:${"b".repeat(64)}:Bob"

    @Test
    fun `scan preview then verify and chat`() {
        val vm = ContactAddViewModel(repo)
        vm.onScanned("https://example.com")
        assertTrue(vm.state.value.notOmcp)
        vm.onScanned(bobQr)
        val p = vm.state.value.preview!!
        assertEquals(12, p.contact.safetyNumber.size)
        assertTrue(repo.contacts.value.isEmpty()) // preview never saves
        val id = vm.verifyAndChat()
        assertNotNull(id)
        assertTrue(repo.contacts.value.single().verified)
        assertTrue(store.contacts.single().verified)
    }

    @Test
    fun `chat without verifying keeps contact unverified`() {
        val vm = ContactAddViewModel(repo)
        vm.onScanned(bobQr)
        assertNotNull(vm.chatWithoutVerifying())
        assertFalse(repo.contacts.value.single().verified)
    }

    @Test
    fun `mismatch blocks chat actions`() {
        repo.addContact(bobQr, verified = true)
        val vm = ContactAddViewModel(repo)
        vm.onScanned("omcp:1:${"c".repeat(64)}:Bob")
        assertNotNull(vm.state.value.preview!!.mismatchWith)
        assertNull(vm.verifyAndChat())
        assertNull(vm.chatWithoutVerifying())
        assertEquals(1, repo.contacts.value.size)
    }

    @Test
    fun `send queues then flush marks sent and receive appends`() {
        val c = repo.addContact(bobQr, verified = false)
        val vm = ChatThreadViewModel(repo, c.id)
        vm.send("  hello ")
        vm.send("   ")
        val msgs = repo.messages.value[c.id]!!
        assertEquals(1, msgs.size)
        assertEquals("hello", msgs[0].text)
        assertEquals(ChatMessage.Status.Queued, msgs[0].status)
        assertEquals(PathKind.Queued, vm.path())
        assertEquals(listOf("hello"), core.simulateLinkUp(c.id))
        repo.markFlushed(c.id, 1)
        assertEquals(ChatMessage.Status.Sent, repo.messages.value[c.id]!![0].status)
        repo.onReceived(IncomingText(c.id, "hi", 43, "e1"))
        assertEquals(2, repo.messages.value[c.id]!!.size)
        assertEquals(PathKind.Mesh, vm.path())
    }

    @Test
    fun `file over core limit is too large before any encryption`() {
        val c = repo.addContact(bobQr, verified = false)
        val vm = ChatThreadViewModel(repo, c.id, kotlinx.coroutines.Dispatchers.Unconfined)
        var reads = 0
        vm.onFilePicked("clip.mp4", core.maxFileBytes() + 1, "video/mp4") { reads++; ByteArray(0) }
        assertTrue(vm.fileConfirm.value!!.tooLarge)
        vm.confirmFile()
        assertNull(repo.messages.value[c.id])
        assertEquals("too large must not even read the file", 0, reads)
        vm.onFilePicked("flyer.jpg", 2_000_000, "image/jpeg") { reads++; ByteArray(2_000_000) }
        assertFalse(vm.fileConfirm.value!!.tooLarge)
        assertEquals(50_000_000L, vm.fileConfirm.value!!.maxBytes)
        vm.confirmFile()
        assertEquals(1, reads)
        val m = repo.messages.value[c.id]!!.single()
        assertEquals("flyer.jpg", m.file!!.name)
        assertEquals(FileAttachment.State.Queued, m.file!!.state)
    }

    @Test
    fun `file transfer states follow link events`() {
        val c = repo.addContact(bobQr, verified = false)
        val id = repo.sendFile(c.id, "a.pdf", "application/pdf", ByteArray(200_000))
        repo.onFileProgress(FileProgressInfo(id, c.id, false, "a.pdf", 200_000, 2, 4))
        var f = repo.messages.value[c.id]!!.single().file!!
        assertEquals(FileAttachment.State.Sending, f.state)
        assertEquals(100_000L, f.doneBytes)
        repo.pauseTransfers(c.id)
        assertEquals(FileAttachment.State.Paused, repo.messages.value[c.id]!!.single().file!!.state)
        repo.onFileProgress(FileProgressInfo(id, c.id, false, "a.pdf", 200_000, 4, 4))
        f = repo.messages.value[c.id]!!.single().file!!
        assertEquals(FileAttachment.State.Sent, f.state)

        // Incoming: progress creates a Receiving bubble, then the verified file lands.
        val saved = mutableListOf<String>()
        repo.fileSink = FileSink { saved += it.name; "/data/files/${it.name}" }
        repo.onFileProgress(FileProgressInfo("in1", c.id, true, "b.jpg", 10, 0, 1))
        assertEquals(FileAttachment.State.Receiving, repo.messages.value[c.id]!!.last().file!!.state)
        repo.onFileReceived(ReceivedFileInfo("in1", c.id, "b.jpg", "image/jpeg", ByteArray(10)))
        f = repo.messages.value[c.id]!!.last().file!!
        assertEquals(FileAttachment.State.Received, f.state)
        assertEquals("/data/files/b.jpg", f.localPath)
        assertEquals(listOf("b.jpg"), saved)

        repo.onFileProgress(FileProgressInfo("in2", c.id, true, "c.bin", 10, 0, 1))
        repo.onFileFailed(FileFailureInfo("in2", c.id, "integrity"))
        assertEquals(FileAttachment.State.IntegrityFailed, repo.messages.value[c.id]!!.last().file!!.state)

        repo.cancelFile(c.id, id)
        assertTrue(repo.messages.value[c.id]!!.none { it.id == id })
        assertFalse(core.files.contains(id))
    }

    @Test
    fun `gateway defaults wifi only and warns not blocks on metered`() {
        val vm = GatewayViewModel(repo)
        assertEquals(500, vm.dailyCapMb)
        assertEquals(0, vm.usedMb)
        assertFalse(vm.gatewayOn.value)
        assertFalse(vm.decision(NetworkType.Cellular, metered = true).bridge)
        vm.setAllowMobile(true)
        val d = vm.decision(NetworkType.Cellular, metered = true)
        assertTrue(d.bridge && d.meteredWarning)
    }
}
