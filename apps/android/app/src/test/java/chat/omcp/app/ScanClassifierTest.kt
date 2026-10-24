package chat.omcp.app

import chat.omcp.app.core.OmcpGatt
import chat.omcp.app.mesh.ProbeLimiter
import chat.omcp.app.mesh.ScanClassifier
import chat.omcp.app.mesh.ScanClassifier.Decision
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.UUID

class ScanClassifierTest {
    private val svc = UUID.fromString(OmcpGatt.SERVICE)

    private fun classify(uuids: List<UUID> = emptyList(), mfr: Map<Int, ByteArray> = emptyMap(), name: String? = null) =
        ScanClassifier.classify(uuids, mfr, name, svc, OmcpGatt.MANUFACTURER_ID, OmcpGatt.LOCAL_NAME)

    @Test fun androidPeerWithAdDialsDirectly() {
        val ad = ByteArray(13) { it.toByte() }
        val d = classify(listOf(svc), mapOf(OmcpGatt.MANUFACTURER_ID to ad))
        assertTrue(d is Decision.Dial && d.advertisement.contentEquals(ad))
    }

    @Test fun foregroundIphoneServiceUuidOnlyIsProbed() {
        assertEquals(Decision.Probe("service-uuid"), classify(listOf(svc)))
    }

    @Test fun iphoneLocalNameOnlyIsProbed() {
        assertEquals(Decision.Probe("local-name"), classify(name = "OMCP"))
    }

    @Test fun backgroundIphoneOverflowAreaIsProbed() {
        val overflow = ByteArray(17).also { it[0] = 0x01; it[5] = 0x40 }
        assertEquals(Decision.Probe("ios-overflow"), classify(mfr = mapOf(0x004C to overflow)))
    }

    @Test fun otherDevicesIgnored() {
        assertEquals(Decision.Ignore, classify(mfr = mapOf(0x004C to byteArrayOf(0x10, 0x05, 0x01))))
        assertEquals(Decision.Ignore, classify(listOf(UUID.randomUUID()), name = "Headphones"))
        // Malformed OMCP manufacturer data isn't dialled, but a UUID still allows a probe.
        assertEquals(Decision.Ignore, classify(mfr = mapOf(OmcpGatt.MANUFACTURER_ID to ByteArray(3))))
    }

    @Test fun probeLimiterBacksOffAndCapsConcurrency() {
        var now = 0L
        val l = ProbeLimiter(maxConcurrent = 2, negativeTtlMs = 1000, clock = { now })
        assertTrue(l.tryBegin("a"))
        assertFalse(l.tryBegin("a"))
        assertTrue(l.tryBegin("b"))
        assertFalse(l.tryBegin("c"))
        l.end("a", useful = false)
        assertFalse(l.tryBegin("a"))
        assertTrue(l.tryBegin("c"))
        l.end("b", useful = true)
        now = 1001
        assertTrue(l.tryBegin("a"))
    }
}
