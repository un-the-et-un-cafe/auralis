package chat.omcp.app

import chat.omcp.app.mesh.ReceiptRecoveryGate
import org.junit.Assert.*
import org.junit.Test

class ReceiptRecoveryGateTest {
    @Test fun noPerChunkOrPerFlushRetryLoop() {
        val gate = ReceiptRecoveryGate()
        assertFalse(gate.observe(false, false))
        assertTrue(gate.observe(true, false))
        repeat(1000) { assertFalse(gate.observe(true, false)) }
        assertTrue(gate.observe(true, true))
        repeat(1000) { assertFalse(gate.observe(true, true)) }
        assertTrue(ReceiptRecoveryGate().observe(true, false))
    }
    @Test fun simultaneousEstablishedAndStorageErrorSchedulesOnlyOnePass() {
        val gate = ReceiptRecoveryGate()
        assertTrue(gate.observe(true, true))
        assertFalse(gate.observe(true, true))
    }
}
