package chat.omcp.app

import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.IncomingText
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.core.WrongBackupPasswordException
import org.junit.Assert.assertEquals
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
}
