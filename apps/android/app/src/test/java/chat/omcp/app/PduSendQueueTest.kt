package chat.omcp.app

import chat.omcp.app.mesh.PduSendQueue
import org.junit.Assert.*
import org.junit.Test

class PduSendQueueTest {
    private val pdus = listOf(byteArrayOf(1), byteArrayOf(2), byteArrayOf(3))

    @Test fun `only all successful callbacks make a batch ready to commit`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", pdus)
        val sent = mutableListOf<Int>()
        val send: (ByteArray) -> Boolean = { sent += it[0].toInt(); true }
        for (i in pdus.indices) {
            assertTrue(queue.submit(send))
            assertTrue(queue.submit(send))
            assertEquals(i + 1, sent.size)
            assertNull(queue.readyToken)
            val ticket = requireNotNull(queue.callbackTicket)
            assertTrue(queue.complete(ticket, true))
            assertFalse(queue.complete(ticket, true))
            if (i < pdus.lastIndex) assertNull(queue.readyToken)
        }
        assertEquals(listOf(1, 2, 3), sent)
        assertEquals("batch", queue.readyToken)
        assertFalse(queue.isEmpty) // durable commit still owed
        queue.committed("batch")
        assertTrue(queue.isEmpty)
        assertNull(queue.readyToken)
    }

    @Test fun `first middle and final async failures never permit commit or continuation`() {
        for (failedIndex in pdus.indices) {
            val queue = PduSendQueue()
            queue.addBatch("batch", pdus)
            for (i in 0..failedIndex) {
                assertTrue(queue.submit { true })
                assertTrue(queue.complete(requireNotNull(queue.callbackTicket), i != failedIndex))
            }
            assertNull(queue.readyToken)
            assertEquals("batch", queue.batchToken) // owner aborts this exact Rust batch
            assertFalse(queue.submit { fail("Must close instead of sending"); true })
            assertThrows(IllegalStateException::class.java) { queue.committed("batch") }
            assertThrows(IllegalStateException::class.java) { queue.addBatch("next", pdus) }
        }
    }

    @Test fun `first middle and final synchronous rejections cannot skip or retry`() {
        for (failedIndex in pdus.indices) {
            val queue = PduSendQueue()
            queue.addBatch("batch", pdus)
            var sends = 0
            for (i in 0..failedIndex) {
                val accepted = queue.submit { sends++; i != failedIndex }
                if (accepted) assertTrue(queue.complete(requireNotNull(queue.callbackTicket), true))
                else assertEquals(failedIndex, i)
            }
            assertNull(queue.readyToken)
            assertNull(queue.callbackTicket)
            assertFalse(queue.submit { sends++; true })
            assertEquals(failedIndex + 1, sends)
        }
    }

    @Test fun `permission revocation while submitting also closes the session`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", pdus)
        assertFalse(queue.submit { throw SecurityException("permission revoked") })
        assertNull(queue.callbackTicket)
        assertNull(queue.readyToken)
    }

    @Test fun `queued duplicate callback cannot acknowledge the next PDU`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", pdus)
        queue.submit { true }
        val stale = requireNotNull(queue.callbackTicket)
        assertTrue(queue.complete(stale, true))
        queue.submit { true }
        val current = requireNotNull(queue.callbackTicket)
        assertFalse(queue.complete(stale, true))
        assertSame(current, queue.callbackTicket)
        assertNull(queue.readyToken)
    }

    @Test fun `old generation callback drains retired queue and never replacement`() {
        val old = PduSendQueue()
        old.addBatch("old", pdus)
        old.submit { true }
        val late = requireNotNull(old.callbackTicket)
        old.cancel()
        assertSame(late, old.callbackTicket) // peripheral must fence reconnect while this exists
        val replacement = PduSendQueue()
        replacement.addBatch("replacement", pdus)
        replacement.submit { true }
        val current = requireNotNull(replacement.callbackTicket)
        assertFalse(replacement.complete(late, true))
        assertSame(current, replacement.callbackTicket)
        assertFalse(old.complete(late, true))
        assertNull(old.callbackTicket)
        assertNull(old.readyToken)
    }

    @Test fun `empty incoming result preserves the outstanding batch and overlapping batches fail`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", pdus)
        queue.submit { true }
        val ticket = queue.callbackTicket
        queue.addBatch(null, emptyList())
        assertEquals("batch", queue.batchToken)
        assertSame(ticket, queue.callbackTicket)
        assertThrows(IllegalStateException::class.java) { queue.addBatch("next", pdus) }
        assertThrows(IllegalArgumentException::class.java) { PduSendQueue().addBatch(null, pdus) }
        assertThrows(IllegalArgumentException::class.java) { PduSendQueue().addBatch("orphan", emptyList()) }
    }

    @Test fun `wrong premature or duplicate commit cannot clear queued work`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", listOf(pdus.first()))
        assertThrows(IllegalStateException::class.java) { queue.committed("batch") }
        queue.submit { true }
        assertThrows(IllegalStateException::class.java) { queue.committed("batch") }
        queue.complete(requireNotNull(queue.callbackTicket), true)
        assertThrows(IllegalStateException::class.java) { queue.committed("wrong") }
        assertEquals("batch", queue.readyToken)
        queue.committed("batch")
        assertThrows(IllegalStateException::class.java) { queue.committed("batch") }
    }

    @Test fun `commit failure leaves token available for abort rather than advancing`() {
        val queue = PduSendQueue()
        queue.addBatch("batch", listOf(pdus.first()))
        queue.submit { true }
        queue.complete(requireNotNull(queue.callbackTicket), true)
        // The transport invokes Rust before committed(); a storage error closes the session.
        assertEquals("batch", queue.readyToken)
        queue.cancel()
        assertEquals("batch", queue.batchToken)
        assertNull(queue.readyToken)
        assertFalse(queue.submit { fail("Must reconnect after commit failure"); true })
    }
}
