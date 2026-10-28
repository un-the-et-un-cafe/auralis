package chat.omcp.app

import chat.omcp.app.files.readFileBounded
import java.io.ByteArrayInputStream
import java.io.IOException
import java.io.InputStream
import org.junit.Assert.*
import org.junit.Test

class BoundedFileReadTest {
    @Test fun `empty and exact limit files are accepted`() {
        assertArrayEquals(byteArrayOf(), ByteArrayInputStream(byteArrayOf()).readFileBounded(0))
        val bytes = ByteArray(16_384) { (it % 256).toByte() }
        assertArrayEquals(bytes, ByteArrayInputStream(bytes).readFileBounded(bytes.size.toLong()))
    }

    @Test fun `unknown or lying provider is stopped after one byte beyond the limit`() {
        var reads = 0
        val endless = object : InputStream() {
            override fun read(): Int { reads++; return 1 }
        }
        assertThrows(IOException::class.java) { endless.readFileBounded(10_000) }
        assertEquals(10_001, reads)
    }

    @Test fun `zero length provider read does not spin or skip bytes`() {
        val stream = object : ByteArrayInputStream(byteArrayOf(1, 2, 3)) {
            override fun read(b: ByteArray, off: Int, len: Int): Int = if (available() > 0) 0 else -1
        }
        assertArrayEquals(byteArrayOf(1, 2, 3), stream.readFileBounded(3))
    }

    @Test fun `provider IO errors remain errors rather than partial attachments`() {
        val broken = object : InputStream() {
            override fun read(): Int = throw IOException("provider disconnected")
        }
        assertThrows(IOException::class.java) { broken.readFileBounded(100) }
    }
}
