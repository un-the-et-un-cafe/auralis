package chat.omcp.app.files

import android.content.Context
import android.content.Intent
import androidx.core.content.FileProvider
import chat.omcp.app.core.ReceivedFileInfo
import chat.omcp.app.data.FileSink
import java.io.File

/**
 * Saves verified received files under app-private `files/received/<transfer>/<name>` and opens
 * them through a FileProvider (no storage permission, nothing shared until the user taps).
 */
class ReceivedFiles(private val context: Context) : FileSink {
    override fun save(file: ReceivedFileInfo): String? = runCatching {
        val dir = File(context.filesDir, "received/${safe(file.transferId)}").apply { mkdirs() }
        val out = File(dir, safe(file.name).ifEmpty { "file" })
        out.writeBytes(file.bytes)
        out.absolutePath
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
