package chat.omcp.app

import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.core.WrongBackupPasswordException
import chat.omcp.app.burn.BurnSpec
import chat.omcp.app.data.ChatMessage
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * Kotlin ↔ Rust through the real UniFFI bindings (host .so via JNA): two nodes,
 * QR add both ways, hybrid handshake + text both directions over simulated GATT PDUs.
 */
class RustBridgeTest {
    @Before
    fun needLib() {
        val dir = System.getProperty("jna.library.path") ?: ""
        assumeTrue("host libomcp_ffi not built", File(dir, "libomcp_ffi.so").exists())
    }

    private fun pump(a: CoreLink, b: CoreLink, first: List<ByteArray>): List<IncomingText> {
        val got = mutableListOf<IncomingText>()
        var toB = first
        var toA = emptyList<ByteArray>()
        while (toB.isNotEmpty() || toA.isNotEmpty()) {
            val nextA = mutableListOf<ByteArray>()
            toB.forEach { p -> assertTrue(p.size <= 512); b.onPdu(p).also { got += it.received; nextA += it.outgoing } }
            val nextB = mutableListOf<ByteArray>()
            (toA + nextA).forEach { p -> a.onPdu(p).also { got += it.received; nextB += it.outgoing } }
            toA = emptyList(); toB = nextB
        }
        return got
    }

    @Test
    fun endToEndTextThroughBindings() {
        val alice = RustOmcpCore().apply { load(generateRecoveryPhrase(), "Alice") }
        val bob = RustOmcpCore().apply { load(generateRecoveryPhrase(), "Bob") }
        val b = alice.addContactFromQr(bob.qrPayload())
        val a = bob.addContactFromQr(alice.qrPayload())
        assertEquals(b.safetyNumber, a.safetyNumber)
        assertEquals(b.id, alice.contactForAdvertisement(bob.advertisement()))

        alice.queueText(b.id, "queued before link")
        val la = alice.openInitiatorLink(b.id)
        val lb = bob.openResponderLink()
        val got = pump(la, lb, la.start().outgoing)
        assertTrue(la.isEstablished() && lb.isEstablished())
        assertEquals(listOf("queued before link"), got.map { it.text })
        val back = pump(lb, la, lb.sendText("bonjour").outgoing)
        assertEquals("bonjour", back.single().text)
        assertEquals(1, alice.pathStatus().nearbyPeers)
        la.close(); lb.close()
        assertEquals(50_000_000L, alice.maxFileBytes())
    }

    @Test
    fun fileTransferThroughBindingsIn64kChunks() {
        val alice = RustOmcpCore().apply { load(generateRecoveryPhrase(), "Alice") }
        val bob = RustOmcpCore().apply { load(generateRecoveryPhrase(), "Bob") }
        val b = alice.addContactFromQr(bob.qrPayload())
        bob.addContactFromQr(alice.qrPayload())
        val la = alice.openInitiatorLink(b.id)
        val lb = bob.openResponderLink()
        pump(la, lb, la.start().outgoing)
        val data = ByteArray(200_000) { (it % 251).toByte() }
        val offer = alice.queueFile(b.id, "flyer.jpg", "image/jpeg", data)
        assertEquals(4, offer.chunks)
        val repo = MeshRepository(bob, InMemoryAppStore())
        var got: ByteArray? = null
        repo.fileSink = chat.omcp.app.data.FileSink { got = it.bytes; "mem:${it.name}" }
        var flushes = 0
        do {
            val out = la.flush()
            flushes++
            out.outgoing.forEach { p ->
                assertTrue(p.size <= 512)
                val r = lb.onPdu(p)
                r.fileProgress.forEach(repo::onFileProgress)
                r.files.forEach(repo::onFileReceived)
            }
        } while (out.morePending)
        assertTrue(flushes >= 2)
        val m = repo.messages.value.values.flatten().single()
        assertEquals(chat.omcp.app.data.FileAttachment.State.Received, m.file!!.state)
        assertEquals("flyer.jpg", m.file!!.name)
        assertArrayEquals(data, got)
        try {
            alice.queueFile(b.id, "big.bin", "", ByteArray(50_000_001))
            error("expected too large")
        } catch (_: IllegalArgumentException) {
        }
        la.close(); lb.close()
    }

    // ---- Burn after read end to end: two Rust nodes behind two MeshRepositories ----

    /** One app instance. Same [store] + [dbPath] again = the app restarted. */
    private class Side(name: String, var now: Long, val store: InMemoryAppStore = InMemoryAppStore(), dbPath: String? = null) {
        val core = RustOmcpCore()
        val repo = MeshRepository(core, store, dbPath) { now }.also {
            if (store.recoveryPhrase == null) it.saveIdentity(core.generateRecoveryPhrase(), name) else assertTrue(it.restore())
        }
    }

    private class Pair2(val a: Side, val b: Side, val la: CoreLink, val lb: CoreLink, val bobId: String, val aliceId: String)

    private fun pairUp(): Pair2 {
        val t = System.currentTimeMillis()
        val a = Side("Alice", t)
        val b = Side("Bob", t)
        val bobId = a.repo.addContact(b.core.qrPayload(), verified = false).id
        val aliceId = b.repo.addContact(a.core.qrPayload(), verified = false).id
        val la = a.core.openInitiatorLink(bobId)
        val lb = b.core.openResponderLink()
        deliver(a, b, la, lb, la.start())
        return Pair2(a, b, la, lb, bobId, aliceId)
    }

    /** Ping-pong PDUs; every LinkResult is fed through the repository like BleMeshTransport does. */
    private fun deliver(a: Side, b: Side, la: CoreLink, lb: CoreLink, first: chat.omcp.app.core.LinkResult, fromA: Boolean = true) {
        fun apply(s: Side, r: chat.omcp.app.core.LinkResult) {
            r.received.forEach(s.repo::onReceived)
            r.opened.forEach(s.repo::onOpened)
        }
        apply(if (fromA) a else b, first)
        var pending = listOf(fromA to first.outgoing)
        while (pending.isNotEmpty()) {
            val next = mutableListOf<kotlin.Pair<Boolean, List<ByteArray>>>()
            for ((srcA, pdus) in pending) for (p in pdus) {
                val (dst, link) = if (srcA) b to lb else a to la
                val r = link.onPdu(p)
                apply(dst, r)
                if (r.outgoing.isNotEmpty()) next += !srcA to r.outgoing
            }
            pending = next
        }
    }

    @Test
    fun burnTimerOpenedDeletesOnBothSides() {
        val p = pairUp()
        p.a.repo.send(p.bobId, "self-destruct", BurnSpec.timer(10))
        val id = p.a.repo.messages.value[p.bobId]!!.single().id
        deliver(p.a, p.b, p.la, p.lb, p.la.flush())
        val got = p.b.repo.messages.value[p.aliceId]!!.single()
        assertEquals(id, got.id)
        assertEquals("", got.text) // plaintext never in the UI list
        assertEquals(BurnSpec.timer(10), got.burn)

        val opened = p.b.repo.openBurn(p.aliceId, id, p.b.now)!!
        assertEquals("self-destruct", opened.text)
        assertNull(p.b.repo.openBurn(p.aliceId, id, p.b.now)) // exactly once
        deliver(p.a, p.b, p.la, p.lb, p.lb.flush(), fromA = false) // OPENED → Alice
        assertEquals(ChatMessage.BurnState.Open, p.a.repo.messages.value[p.bobId]!!.single().burnState)
        assertTrue(p.a.core.burnIsLive(id) && p.b.core.burnIsLive(id))

        p.a.now = p.b.now + 10_000; p.b.now += 10_000
        assertEquals(1, p.b.repo.tickBurn())
        assertEquals(1, p.a.repo.tickBurn())
        assertTrue(p.a.repo.messages.value[p.bobId]!!.isEmpty())
        assertTrue(p.b.repo.messages.value[p.aliceId]!!.isEmpty())
        assertFalse(p.a.core.burnIsLive(id) || p.b.core.burnIsLive(id)) // crypto-shredded in Rust
        p.la.close(); p.lb.close()
    }

    @Test
    fun burnViewOnceImageOpenedShredsSender() {
        val p = pairUp()
        val jpeg = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 7)
        val id = p.a.core.queueMedia(p.bobId, "image/jpeg", jpeg, BurnSpec.onOpen())
        deliver(p.a, p.b, p.la, p.lb, p.la.flush())
        val m = p.b.repo.messages.value[p.aliceId]!!.single()
        assertTrue(m.isViewOnceMedia && !m.canShare)
        assertArrayEquals(jpeg, p.b.repo.openBurn(p.aliceId, id, p.b.now)!!.bytes)
        assertFalse(p.b.core.burnIsLive(id)) // single use: gone from Rust on view
        p.b.repo.closeBurn(p.aliceId, id)
        assertEquals(ChatMessage.BurnState.Opened, p.b.repo.messages.value[p.aliceId]!!.single().burnState)
        assertNull(p.b.repo.openBurn(p.aliceId, id, p.b.now)) // second tap → placeholder
        assertTrue(p.a.core.burnIsLive(id))
        deliver(p.a, p.b, p.la, p.lb, p.lb.flush(), fromA = false)
        assertFalse(p.a.core.burnIsLive(id)) // sender's copy shredded on OPENED
        p.la.close(); p.lb.close()
    }

    @Test
    fun burnUnreadTtlExpiresOnBothSides() {
        val p = pairUp()
        p.a.repo.send(p.bobId, "unread", BurnSpec.onOpen(BurnSpec.UNREAD_1H))
        val id = p.a.repo.messages.value[p.bobId]!!.single().id
        deliver(p.a, p.b, p.la, p.lb, p.la.flush())
        val sent = p.b.repo.messages.value[p.aliceId]!!.single().timeMs
        p.b.now = sent + 3_600_000 - 1
        assertEquals(0, p.b.repo.tickBurn())
        p.b.now = sent + 3_600_000 + 1_000
        p.a.now = p.b.now
        assertEquals(1, p.b.repo.tickBurn())
        assertEquals(1, p.a.repo.tickBurn())
        assertNull(p.b.repo.openBurn(p.aliceId, id, p.b.now))
        assertFalse(p.a.core.burnIsLive(id) || p.b.core.burnIsLive(id))
        assertFalse(p.a.core.burnSpecValid(BurnSpec(BurnSpec.Mode.Timer, 11, BurnSpec.UNREAD_1H)))
        p.la.close(); p.lb.close()
    }

    @Test
    fun backupThroughBindings() {
        val core = RustOmcpCore().apply { load(generateRecoveryPhrase(), "Alice") }
        val blob = core.createBackup("correct horse battery staple", "test", 1L)
        assertTrue(!String(blob, Charsets.ISO_8859_1).contains(core.recoveryPhrase()))
        try {
            core.openBackup(blob, "wrong wrong wrong 1")
            throw AssertionError("expected wrong password")
        } catch (_: WrongBackupPasswordException) {}
        assertEquals(core.recoveryPhrase(), core.openBackup(blob, "correct horse battery staple").phrase)
    }

    // ---- Encrypted local store: chats survive an app restart ----

    @Test
    fun messagesQueueAndBurnSurviveRestart() {
        val dir = kotlin.io.path.createTempDirectory("omcp-store").toFile()
        val db = File(dir, "omcp-store.db").path
        val t = System.currentTimeMillis()
        val a = Side("Alice", t)
        val bobStore = InMemoryAppStore()
        var b = Side("Bob", t, bobStore, db)
        val bobId = a.repo.addContact(b.core.qrPayload(), verified = false).id
        val aliceId = b.repo.addContact(a.core.qrPayload(), verified = true).id
        assertTrue(bobStore.contacts.isEmpty()) // not in plain prefs: in the encrypted store
        val secret = "burn: meet at gate 7"
        a.repo.send(bobId, "hello Bob")
        a.repo.send(bobId, secret, BurnSpec.timer(10))
        var la = a.core.openInitiatorLink(bobId)
        var lb = b.core.openResponderLink()
        deliver(a, b, la, lb, la.start())
        b.repo.send(aliceId, "queued for Alice") // b's link isn't flushed: stays queued
        la.close(); lb.close()
        assertNotNull(bobStore.storageKey)
        assertFalse(String(File(db).readBytes(), Charsets.ISO_8859_1).contains("hello Bob"))

        // Restart Bob: contacts, verification, chat, queue and the unopened burn message are back.
        b = Side("Bob", t, bobStore, db)
        assertTrue(b.repo.contacts.value.single().verified)
        val chat = b.repo.messages.value[aliceId]!!
        assertEquals(listOf("hello Bob", "", "queued for Alice"), chat.map { it.text })
        assertEquals(ChatMessage.BurnState.Unopened, chat[1].burnState)
        assertEquals(ChatMessage.Status.Queued, chat[2].status)
        assertEquals(1, b.core.queuedFor(aliceId))
        assertEquals(secret, b.repo.openBurn(aliceId, chat[1].id, b.now)!!.text)
        b.now += 10_000
        assertEquals(1, b.repo.tickBurn())

        // Restart again: the burned message is gone for good; the queue still goes out.
        b = Side("Bob", b.now, bobStore, db)
        assertEquals(listOf("hello Bob", "queued for Alice"), b.repo.messages.value[aliceId]!!.map { it.text })
        assertFalse(b.core.burnIsLive(chat[1].id))
        la = a.core.openInitiatorLink(bobId)
        lb = b.core.openResponderLink()
        deliver(a, b, la, lb, la.start())
        assertEquals("queued for Alice", a.repo.messages.value[bobId]!!.last().text)
        assertEquals(0, b.core.queuedFor(aliceId))
        la.close(); lb.close()
        dir.listFiles()!!.forEach { assertFalse(String(it.readBytes(), Charsets.ISO_8859_1).contains(secret)) }
    }

    @Test
    fun unreadableStoreIsSetAsideNotFatal() {
        val dir = kotlin.io.path.createTempDirectory("omcp-store").toFile()
        val db = File(dir, "omcp-store.db").path
        val store = InMemoryAppStore()
        Side("Bob", 1L, store, db)
        store.storageKey = ByteArray(32) { 7 } // e.g. Keystore wiped
        val b = Side("Bob", 2L, store, db)
        assertTrue(b.repo.onboarded.value)
        assertTrue(dir.listFiles()!!.any { it.name.startsWith("omcp-store.db.unreadable-") })
    }
}
