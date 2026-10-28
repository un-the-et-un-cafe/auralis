package chat.omcp.app.files

import java.io.ByteArrayOutputStream
import java.io.IOException
import java.io.InputStream

/** Provider sizes are advisory: enforce the core limit while reading, not after allocation. */
fun InputStream.readFileBounded(maxBytes: Long): ByteArray {
    require(maxBytes in 0..Int.MAX_VALUE.toLong())
    val output = ByteArrayOutputStream()
    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
    var total = 0L
    while (true) {
        // Read at most one byte beyond the cap to distinguish exact-cap files from overflow.
        val count = read(buffer, 0, minOf(buffer.size.toLong(), maxBytes - total + 1).toInt())
        if (count == -1) return output.toByteArray()
        if (count == 0) {
            // Be robust to providers that return a zero-length read without EOF.
            val next = read()
            if (next == -1) return output.toByteArray()
            if (total == maxBytes) throw IOException("File exceeds the size limit")
            output.write(next)
            total++
        } else {
            total += count
            if (total > maxBytes) throw IOException("File exceeds the size limit")
            output.write(buffer, 0, count)
        }
    }
}
