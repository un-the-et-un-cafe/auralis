package chat.omcp.app.files

import chat.omcp.app.core.ReceiveReceiptInfo
import chat.omcp.app.core.ReceiptVerifier

/** No bulk plaintext event: each read is bounded by the sealed-store block size. */
interface ReceiptSource {
    fun readBlock(index: Int): ByteArray
    fun verifier(): ReceiptVerifier
    /** Exact current capability authorization; false/throw never authorizes mutation. */
    fun authorizeWrite(replacingIncomplete: Boolean): Boolean = false
    /** Full sealed-spool validation and exact current Pending capability; explicit repair only. */
    fun authorizeRepair(): Boolean = false
}

/** Returns only after readback verification, file and directory sync. The database commit publishes the path. */
fun interface DurableReceiptSink {
    fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource): String
    fun repair(receipt: ReceiveReceiptInfo, source: ReceiptSource): String = error("explicit receipt repair unavailable")
    fun discard(receipt: ReceiveReceiptInfo) { error("durable receipt cleanup unavailable") }
    /** False includes permission/protection errors, symlinks and uncertain ancestors. */
    fun isDefinitelyMissing(path: String): Boolean = false
}
