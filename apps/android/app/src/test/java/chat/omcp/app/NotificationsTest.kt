package chat.omcp.app

import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.notify.LockVisibility
import chat.omcp.app.notify.NotificationContent
import chat.omcp.app.notify.NotificationPolicy
import chat.omcp.app.notify.PushPayload
import chat.omcp.app.notify.StubPushReceiver
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** REQUIREMENTS "Notifications (v0.1.2)" acceptance N1–N3 (Android). */
class NotificationsTest {
    private val sender = "Alice"
    private val text = "meet at the cafe at 9"

    private fun render(store: InMemoryAppStore, chat: String = "c1"): NotificationContent =
        NotificationPolicy.content(store, chat, sender, text, "Hopla", "New message")

    private fun NotificationContent.allText() = listOf(title, body, publicTitle, publicBody).joinToString("|")

    @Test fun `N1 fresh install shows sender and preview`() {
        val n = render(InMemoryAppStore())
        assertEquals(sender, n.title)
        assertEquals(text, n.body)
        assertTrue(n.showAvatar)
        assertFalse(n.private)
        // Lock screen uses the generic public version by default (VISIBILITY_PRIVATE).
        assertEquals(LockVisibility.PRIVATE, n.visibility)
        assertFalse(n.publicTitle.contains(sender) || n.publicBody.contains(text))
    }

    @Test fun `N2 global private hides sender and content everywhere`() {
        val s = InMemoryAppStore().apply { privateNotifications = true }
        val n = render(s)
        assertEquals("Hopla", n.title)
        assertEquals("New message", n.body)
        assertFalse(n.showAvatar)
        assertFalse(n.allText().contains(sender))
        assertFalse(n.allText().contains("cafe"))
    }

    @Test fun `N2 per-chat private only affects that chat`() {
        val s = InMemoryAppStore()
        NotificationPolicy.setChatPrivate(s, "c1", true)
        assertTrue(render(s, "c1").private)
        assertFalse(render(s, "c1").allText().contains(sender))
        assertEquals(sender, render(s, "c2").title)
        NotificationPolicy.setChatPrivate(s, "c1", false)
        assertEquals(sender, render(s, "c1").title)
    }

    @Test fun `hide on lock screen uses secret visibility`() {
        val s = InMemoryAppStore().apply { hideOnLockScreen = true }
        assertEquals(LockVisibility.SECRET, render(s).visibility)
    }

    @Test fun `N3 push payload carries only the opaque mailbox id`() {
        val p = PushPayload("mbx_3f9a-01")
        val data = p.toData()
        assertEquals(setOf(PushPayload.KEY_MAILBOX), data.keys)
        val wire = data.toString()
        assertFalse(wire.contains(sender))
        assertFalse(wire.contains(text))
        assertEquals(p, PushPayload.fromData(data))
    }

    @Test fun `N3 receiver rejects payloads with plaintext fields`() {
        var woke: PushPayload? = null
        val r = StubPushReceiver { woke = it }
        r.onPush(mapOf("mb" to "x1", "body" to text))
        assertNull(woke)
        r.onPush(mapOf("mb" to "$sender: $text"))
        assertNull(woke)
        r.onPush(mapOf("mb" to "x1"))
        assertEquals(PushPayload("x1"), woke)
        assertNull(r.token())
    }
}
