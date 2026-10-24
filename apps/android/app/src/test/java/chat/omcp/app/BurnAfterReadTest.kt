package chat.omcp.app

import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.core.IncomingText
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.notify.NotificationPolicy
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * UI-side burn behaviour that doesn't need the Rust core. Burn rules themselves
 * (B1, B2, B4 and OPENED) are proven end to end over two Rust nodes in [RustBridgeTest].
 */
class BurnAfterReadTest {
    private val store = InMemoryAppStore()
    private val repo = MeshRepository(FakeOmcpCore(), store) { 1_000L }
        .also { it.saveIdentity((1..24).joinToString(" ") { i -> "word$i" }, "Alice") }
    private val bob = repo.addContact("omcp:1:${"b".repeat(64)}:Bob", verified = false).id
    private fun msgs() = repo.messages.value[bob].orEmpty()

    @Test fun `per-chat burn default applies to sends and plaintext stays out of the UI list`() {
        repo.setChatBurn(bob, 60, BurnSpec.UNREAD_24H)
        assertEquals(BurnSpec.timer(60, BurnSpec.UNREAD_24H), repo.chatBurn(bob))
        repo.send(bob, "x")
        assertEquals(BurnSpec.timer(60, BurnSpec.UNREAD_24H), msgs().single().burn)
        assertEquals("", msgs().single().text)
        repo.setChatBurn(bob, -1)
        repo.send(bob, "y")
        assertNull(msgs().last().burn)
        assertEquals("y", msgs().last().text)
    }

    @Test fun `received view-once media can't be shared and keeps no plaintext`() {
        repo.onReceived(IncomingText(bob, "", 1, "img", BurnSpec.onOpen(), "image/jpeg"))
        val m = msgs().single()
        assertTrue(m.isViewOnceMedia)
        assertFalse(m.canShare) // save / forward / quote / share sheet disabled
        assertEquals(ChatMessage.BurnState.Unopened, m.burnState)
    }

    @Test fun `B5 burn content never in notification previews`() {
        val n = NotificationPolicy.content(store, bob, "Bob", "secret", "Hopla", "New message", burn = true)
        assertEquals("Hopla", n.title)
        assertFalse(listOf(n.title, n.body, n.publicTitle, n.publicBody).any { "secret" in it || "Bob" in it })
    }
}
