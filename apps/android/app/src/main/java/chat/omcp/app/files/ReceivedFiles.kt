package chat.omcp.app.files

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import chat.omcp.app.core.ReceivedFileInfo
import chat.omcp.app.data.FileSink
import java.io.File

/**
 * Saves verified received files under app-private capability-specific paths and opens
 * them through a FileProvider (no storage permission, nothing shared until the user taps).
 */
class ReceivedFiles(private val context: Context) : FileSink, DurableReceiptSink {
    // Kotlin lazy caches only a successful result; transient resolution errors can retry.
    private val paths by lazy { TrustedReceivedPaths(context.filesDir, context.filesDir.canonicalFile) }
    private val receipts = AndroidReceiptFiles(context.filesDir)
    private val backups = AndroidReceiptFiles(context.filesDir, AndroidReceiptFiles.Namespace.Backup)
    override fun finalize(receipt: chat.omcp.app.core.ReceiveReceiptInfo, source: ReceiptSource): String = receipts.finalize(receipt, source)
    override fun repair(receipt: chat.omcp.app.core.ReceiveReceiptInfo, source: ReceiptSource): String = receipts.repair(receipt, source)
    override fun discard(receipt: chat.omcp.app.core.ReceiveReceiptInfo) = receipts.discard(receipt)
    override fun isDefinitelyMissing(path: String): Boolean {
        return try {
            // Resolve only current OS-provided anchors, never the persisted full path.
            val mapped = paths.map(path) ?: return false
            receipts.isDefinitelyMissing(mapped)
        } catch (_: Exception) { false }
    }

    /** Backup-media writes use the same no-follow/sync pipeline, without claiming a core receipt. */
    override fun save(file: ReceivedFileInfo): String? = runCatching {
        require(file.bytes.size <= 50_000_000)
        fun digest(bytes: ByteArray) = java.security.MessageDigest.getInstance("SHA-256").digest(bytes)
        fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it.toInt() and 255) }
        val id = file.transferId.takeIf { it.matches(Regex("[0-9a-f]{32}")) }
            ?: hex(digest(file.transferId.toByteArray()).copyOf(16))
        val expected = digest(file.bytes)
        val receipt = chat.omcp.app.core.ReceiveReceiptInfo(
            id, file.contactId, file.name, file.mime, file.bytes.size.toLong(),
            id + hex(expected), hex(expected), (file.bytes.size + 65_535) / 65_536,
        )
        backups.finalize(receipt, object : ReceiptSource {
            // Legacy sealed backup import may create, but cannot authorize destructive repair.
            override fun authorizeWrite(replacingIncomplete: Boolean) = !replacingIncomplete
            override fun readBlock(index: Int) = file.bytes.copyOfRange(index * 65_536, minOf(file.bytes.size, (index + 1) * 65_536))
            override fun verifier() = object : chat.omcp.app.core.ReceiptVerifier {
                private val hash = java.security.MessageDigest.getInstance("SHA-256")
                private var size = 0L
                override fun update(bytes: ByteArray) { hash.update(bytes); size += bytes.size }
                override fun finish() { check(size == file.bytes.size.toLong() && hash.digest().contentEquals(expected)) }
            }
        })
    }.getOrNull()

    fun openIntent(path: String, mime: String): Intent {
        val uri = FileProvider.getUriForFile(context, context.packageName + ".files", File(path))
        return Intent(Intent.ACTION_VIEW)
            .setDataAndType(uri, mime.ifEmpty { "application/octet-stream" })
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }

    companion object {
        /** Strip path separators and control characters from a peer-supplied name. */
        fun safe(name: String): String =
            name.substringAfterLast('/').substringAfterLast('\\')
                .filter { it >= ' ' && it != ':' }
                .trim().trimStart('.').take(120)
    }
}
