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
        val vm = ChatThreadViewModel(repo, c.id)
        vm.onFilePicked("clip.mp4", core.maxFileBytes() + 1, "video/mp4")
        assertTrue(vm.fileConfirm.value!!.tooLarge)
        vm.confirmFile()
        assertNull(repo.messages.value[c.id])
        vm.onFilePicked("flyer.jpg", 2_000_000, "image/jpeg")
        assertFalse(vm.fileConfirm.value!!.tooLarge)
        assertEquals(50_000_000L, vm.fileConfirm.value!!.maxBytes)
        vm.confirmFile()
        assertEquals("flyer.jpg", repo.messages.value[c.id]!!.single().file!!.name)
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
