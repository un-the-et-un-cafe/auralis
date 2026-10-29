package chat.omcp.app.files

import chat.omcp.app.core.ReceiveReceiptInfo
import chat.omcp.app.core.ReceiptVerifier

/** No bulk plaintext event: each read is bounded by the sealed-store block size. */
interface ReceiptSource {
    fun readBlock(index: Int): ByteArray
    fun verifier(): ReceiptVerifier
}

/** Returns only after readback verification, file sync, atomic no-overwrite publication and directory sync. */
fun interface DurableReceiptSink {
    fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource): String
    fun discard(receipt: ReceiveReceiptInfo) { error("durable receipt cleanup unavailable") }
    /** False includes permission/protection errors, symlinks and uncertain ancestors. */
    fun isDefinitelyMissing(path: String): Boolean = false
}
