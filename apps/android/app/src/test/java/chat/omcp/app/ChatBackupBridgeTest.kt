package chat.omcp.app

import chat.omcp.app.backup.ChatBackupSync
import chat.omcp.app.backup.InMemoryChatBlobStore
import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.LinkResult
import chat.omcp.app.core.RestoreNoticeInfo
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.core.VerificationFailedException
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/** CB1–CB4 + RS1/RS2 through the real UniFFI bindings and [ChatBackupSync]. */
class ChatBackupBridgeTest {
    @Before
    fun needLib() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
    }

    private class Side(val now: Long) {
        val core = RustOmcpCore()
        val store = InMemoryAppStore()
        val repo = MeshRepository(core, store, null) { now }
        val notices = mutableListOf<RestoreNoticeInfo>()
    }

    private fun apply(s: Side, r: LinkResult) {
        r.received.forEach(s.repo::onReceived)
        r.restoreNotices.forEach { s.notices += it; s.repo.onRestoreNotice(it) }
    }

    private fun link(a: Side, b: Side, bId: String) {
        val la: CoreLink = a.core.openInitiatorLink(bId)
        val lb: CoreLink = b.core.openResponderLink()
        val first = la.start()
        drainBatches(la, lb, first) { sourceA, result ->
            apply(if (sourceA) a else b, result)
        }
        la.close(); lb.close()
    }

    @Test
    fun cbRestoreChatsAndRsNoticeAndReverify() {
        val t = System.currentTimeMillis()
        val alice = Side(t).apply { repo.saveIdentity(core.generateRecoveryPhrase(), "Alice") }
        val bob = Side(t).apply { repo.saveIdentity(core.generateRecoveryPhrase(), "Bob") }
        val bobId = alice.repo.addContact(bob.core.qrPayload(), verified = true).id
        val aliceId = bob.repo.addContact(alice.core.qrPayload(), verified = true).id
        val oldAliceQr = alice.core.qrPayload()
        alice.repo.send(bobId, "see you at the market")
        link(alice, bob, bobId)
        assertEquals("see you at the market", bob.repo.messages.value[aliceId]?.last()?.text)

        // CB1/CB4: incremental, padded blobs in the provider space.
        val cloud = InMemoryChatBlobStore()
        val sync = ChatBackupSync(alice.core, cloud, { null }, { t })
        assertEquals(1, sync.runText(onWifi = true, force = true))
        assertTrue(cloud.blobs.isNotEmpty())
        cloud.blobs.values.forEach { assertEquals(0, it.size % 65536) }
        assertFalse(cloud.blobs.values.any { String(it, Charsets.ISO_8859_1).contains("market") })

        // New phone: identity backup + chat backup.
        val idBlob = alice.core.createBackup("correct horse battery staple", "test", t)
        val restored = alice.core.openBackup(idBlob, "correct horse battery staple")
        val alice2 = Side(t + 1000)
        alice2.repo.chatBackup = ChatBackupSync(alice2.core, cloud, { null }, { t + 1000 })
        alice2.repo.restoreFromBackup(restored, "Alice")
        val r = alice2.repo.restoreChats()
        assertNotNull(r)
        assertTrue(r!!.messages >= 1)
        assertEquals("see you at the market", alice2.repo.messages.value[bobId]?.last { it.status != ChatMessage.Status.System }?.text)
        assertNotNull(alice2.repo.restoredAtMs.value)
        assertNotEquals(oldAliceQr, alice2.core.qrPayload())

        // RS1: Bob sees the unsuppressible notice on the next link; RS2: verified cleared.
        // v0.1.4a: the restored device has a new X25519, so it dials first.
        link(alice2, bob, bobId)
        assertEquals(1, bob.notices.size)
        val c = bob.core.contacts().first { it.id == aliceId }
        assertFalse(c.verified)
        assertTrue(c.restoredUnverified)
        assertEquals(ChatMessage.Status.System, bob.repo.messages.value[aliceId]?.last()?.status)

        // RS2: the old QR no longer verifies; the new one does.
        try {
            bob.repo.addContact(oldAliceQr, verified = true)
            throw AssertionError("old QR must not verify")
        } catch (_: VerificationFailedException) {}
        val re = bob.repo.addContact(alice2.core.qrPayload(), verified = true)
        assertTrue(re.verified)
        assertFalse(re.restoredUnverified)
    }
}
