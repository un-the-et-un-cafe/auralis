package chat.omcp.app

import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.burn.BurnVault
import chat.omcp.app.core.FakeOmcpCore
import chat.omcp.app.core.IncomingText
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.FileAttachment
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.notify.NotificationPolicy
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** REQUIREMENTS "Burn after read (v0.1.3)" B1, B2, B4, B5 (Android, JVM). B3 = FLAG_SECURE, device-only. */
class BurnAfterReadTest {
    private var now = 1_000L
    private val store = InMemoryAppStore()
    private val repo = MeshRepository(FakeOmcpCore(), store) { now }
        .also { it.saveIdentity((1..24).joinToString(" ") { i -> "word$i" }, "Alice") }
    private val bob = repo.addContact("omcp:1:${"b".repeat(64)}:Bob", verified = false).id
    private val opened = mutableListOf<String>()

    init {
        repo.burnControl = chat.omcp.app.burn.BurnControl { _, id, _ -> opened += id }
    }

    private fun msgs() = repo.messages.value[bob].orEmpty()

    @Test fun `B1 timer text burns on time and is crypto-shredded`() {
        repo.onReceived(IncomingText(bob, "secret", now, "e1"), BurnSpec.timer(10))
        val m = msgs().single()
        assertEquals("", m.text) // plaintext never in the message list
        assertTrue(repo.burnVault.contains("e1"))
        assertEquals("secret", String(repo.openBurn(bob, "e1")!!))
        assertEquals(listOf("e1"), opened) // OPENED receipt
        now += 9_999
        assertEquals(0, repo.tickBurn())
        now += 1 // tick period is 250 ms in the UI → ≤ 1 s
        assertEquals(1, repo.tickBurn())
        assertTrue(msgs().isEmpty())
        assertFalse(repo.burnVault.contains("e1"))
        assertEquals(0, repo.burnVault.size())
    }

    @Test fun `B1 sender side burns after OPENED receipt timer`() {
        repo.send(bob, "hello", BurnSpec.timer(10))
        val id = msgs().single().id
        assertEquals("", msgs().single().text)
        repo.onOpenedReceipt(bob, id, now)
        now += 10_000
        repo.tickBurn()
        assertTrue(msgs().isEmpty())
        assertFalse(repo.burnVault.contains(id))
    }

    @Test fun `per-chat burn default applies to sends`() {
        repo.setChatBurn(bob, 60, BurnSpec.UNREAD_24H)
        assertEquals(BurnSpec.timer(60, BurnSpec.UNREAD_24H), repo.chatBurn(bob))
        repo.send(bob, "x")
        assertEquals(BurnSpec.timer(60, BurnSpec.UNREAD_24H), msgs().single().burn)
        repo.setChatBurn(bob, -1)
        repo.send(bob, "y")
        assertNull(msgs().last().burn)
    }

    @Test fun `B2 view-once image opens exactly once then placeholder`() {
        val jpeg = byteArrayOf(1, 2, 3)
        repo.onReceived(
            IncomingText(bob, "", now, "img"), BurnSpec.onOpen(),
            FileAttachment("p.jpg", 3, "image/jpeg", FileAttachment.State.Received), jpeg.copyOf(),
        )
        val m = msgs().single()
        assertTrue(m.isViewOnceMedia)
        assertFalse(m.canShare) // save / forward / quote / share sheet disabled
        assertArrayEquals(jpeg, repo.openBurn(bob, "img"))
        assertFalse(repo.burnVault.contains("img")) // single-use: shredded on view
        assertNull(repo.openBurn(bob, "img"))
        repo.closeBurn(bob, "img")
        assertEquals(ChatMessage.BurnState.Opened, msgs().single().burnState) // "🔥 Photo opened"
        assertNull(repo.openBurn(bob, "img"))
    }

    @Test fun `B4 unopened burn expires at unread_ttl`() {
        repo.onReceived(IncomingText(bob, "later", now, "e2"), BurnSpec.onOpen(BurnSpec.UNREAD_1H))
        assertEquals(now + 3_600_000, BurnSpec.onOpen(BurnSpec.UNREAD_1H).expiresAtMs(now))
        now += 3_599_999
        assertEquals(0, repo.tickBurn())
        now += 1
        assertEquals(1, repo.tickBurn())
        assertTrue(msgs().isEmpty())
        assertFalse(repo.burnVault.contains("e2"))
    }

    @Test fun `B5 burn content never in notification previews`() {
        val n = NotificationPolicy.content(store, bob, "Bob", "secret", "Hopla", "New message", burn = true)
        assertEquals("Hopla", n.title)
        assertFalse(listOf(n.title, n.body, n.publicTitle, n.publicBody).any { "secret" in it || "Bob" in it })
    }

    @Test fun `vault shred zeroes and rejects reads`() {
        val v = BurnVault()
        v.put("a", "x".toByteArray())
        assertNotNull(v.read("a"))
        v.shred("a")
        assertNull(v.read("a"))
    }

    @Test(expected = IllegalArgumentException::class)
    fun `invalid timer rejected like the core`() {
        BurnSpec.timer(11)
    }
}
