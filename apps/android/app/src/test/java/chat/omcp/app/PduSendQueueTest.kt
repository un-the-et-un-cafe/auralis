package chat.omcp.app

import chat.omcp.app.mesh.PduSendQueue
import org.junit.Assert.*
import org.junit.Test

class PduSendQueueTest {
    @Test fun `only one GATT operation is submitted until its successful callback`() {
        val queue = PduSendQueue()
        queue.addAll(listOf(byteArrayOf(1), byteArrayOf(2)))
        val sent = mutableListOf<Int>()
        val send: (ByteArray) -> Boolean = { sent += it[0].toInt(); true }
        assertTrue(queue.submit(send))
        assertTrue(queue.submit(send))
        assertEquals(listOf(1), sent)
        assertFalse(queue.isEmpty)
        assertTrue(queue.complete(true))
        assertTrue(queue.submit(send))
        assertEquals(listOf(1, 2), sent)
        assertTrue(queue.complete(true))
        assertTrue(queue.isEmpty)
        assertFalse(queue.complete(true))
    }

    @Test fun `synchronous rejection poisons session without retrying or skipping a PDU`() {
        val queue = PduSendQueue()
        queue.addAll(listOf(byteArrayOf(1), byteArrayOf(2)))
        var calls = 0
        assertFalse(queue.submit { calls++; false })
        assertFalse(queue.submit { calls++; true })
        assertFalse(queue.complete(true))
        assertEquals(1, calls)
        assertThrows(IllegalStateException::class.java) { queue.addAll(listOf(byteArrayOf(3))) }
    }

    @Test fun `failed async write or notification never advances remaining encrypted PDUs`() {
        val queue = PduSendQueue()
        queue.addAll(listOf(byteArrayOf(1), byteArrayOf(2)))
        assertTrue(queue.submit { true })
        assertTrue(queue.complete(false))
        assertFalse(queue.submit { fail("Must close link instead of sending"); true })
    }

    @Test fun `permission revocation while submitting also closes the session`() {
        val queue = PduSendQueue()
        queue.addAll(listOf(byteArrayOf(1)))
        assertFalse(queue.submit { throw SecurityException("permission revoked") })
        assertFalse(queue.complete(true))
    }
}
