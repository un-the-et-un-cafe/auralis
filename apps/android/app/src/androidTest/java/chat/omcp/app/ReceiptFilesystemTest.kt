package chat.omcp.app

import android.system.Os
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import chat.omcp.app.core.ReceiveReceiptInfo
import chat.omcp.app.core.ReceiptVerifier
import chat.omcp.app.files.AndroidReceiptFiles
import chat.omcp.app.files.ReceiptSource
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.MessageDigest

/** Actual Android syscalls, not JVM filesystem substitutes. */
@RunWith(AndroidJUnit4::class)
class ReceiptFilesystemTest {
    private val context get() = ApplicationProvider.getApplicationContext<android.content.Context>()
    private fun root() = File(context.cacheDir, "receipt-test-${System.nanoTime()}").apply { check(mkdir()) }
    private fun receipt(size: Int) = ReceiveReceiptInfo("11".repeat(16), "22".repeat(32), "../../peer-name", "application/octet-stream", size.toLong(), "11".repeat(16) + "33".repeat(32), "44".repeat(32), (size + 65535) / 65536)
    private fun source(bytes: ByteArray, reads: (() -> Unit)? = null) = object : ReceiptSource {
        override fun readBlock(index: Int): ByteArray {
            reads?.invoke()
            return bytes.copyOfRange(index * 65536, minOf(bytes.size, (index + 1) * 65536))
        }
        override fun verifier() = object : ReceiptVerifier {
            // This filesystem test isolates sink ordering; RustBridgeTest exercises real BLAKE3.
            val hash = MessageDigest.getInstance("SHA-256")
            var count = 0
            override fun update(bytes: ByteArray) { count += bytes.size; hash.update(bytes) }
            override fun finish() {
                check(count == bytes.size)
                check(hash.digest().contentEquals(MessageDigest.getInstance("SHA-256").digest(bytes)))
            }
        }
    }
    @Test fun realDescriptorWritesVerifySyncAndRecoverExistingFinalWithoutReadingSpoolAgain() {
        val dir = root()
        try {
            for (size in listOf(0, 1, 65535, 65536, 65537)) {
                val nested = File(dir, "$size").apply { mkdir() }
                val bytes = ByteArray(size) { (it % 251).toByte() }; val r = receipt(size)
                val sink = AndroidReceiptFiles(nested)
                val path = sink.finalize(r, source(bytes))
                assertArrayEquals(bytes, File(path).readBytes())
                assertTrue(path.startsWith(File(nested, "received").canonicalPath + "/"))
                assertFalse(path.contains("peer-name"))
                val recovered = sink.finalize(r, source(bytes) { error("must verify existing final, not rewrite") })
                assertEquals(path, recovered)
            }
        } finally { dir.deleteRecursively() }
    }
    @Test fun writeSyncRenameAndDirectorySyncFaultsRetainRetryableGeneration() {
        for (fault in listOf("create", "write", "file-sync", "publish", "after-link", "unlink", "directory-sync")) {
            val dir = root()
            try {
                val bytes = ByteArray(65537) { 7 }; val r = receipt(bytes.size)
                val failing = AndroidReceiptFiles(dir) { if (it == fault) error(fault) }
                assertThrows(IllegalStateException::class.java) { failing.finalize(r, source(bytes)) }
                val path = AndroidReceiptFiles(dir).finalize(r, source(bytes))
                assertArrayEquals(bytes, File(path).readBytes())
            } finally { dir.deleteRecursively() }
        }
    }
    @Test fun symlinkDirectoriesAndLeavesNeverRedirectWritesOrProduceMissingClassification() {
        val dir = root(); val outside = root()
        try {
            val r = receipt(3); val sink = AndroidReceiptFiles(dir)
            Os.symlink(outside.absolutePath, File(dir, "received").absolutePath)
            assertThrows(Exception::class.java) { sink.finalize(r, source(byteArrayOf(1, 2, 3))) }
            assertFalse(sink.isDefinitelyMissing(File(dir, "received/absent").absolutePath))
            assertTrue(outside.listFiles()!!.isEmpty())
            Os.remove(File(dir, "received").absolutePath); File(dir, "received").mkdir()
            val final = File(dir, "received/receipt-${r.receiptToken}.bin")
            val victim = File(outside, "victim").apply { writeText("do not replace") }
            Os.symlink(victim.absolutePath, final.absolutePath)
            assertThrows(Exception::class.java) { sink.finalize(r, source(byteArrayOf(1, 2, 3))) }
            assertEquals("do not replace", victim.readText())
            assertFalse(sink.isDefinitelyMissing(final.absolutePath))
            Os.remove(final.absolutePath)
            assertTrue(sink.isDefinitelyMissing(final.absolutePath))
            assertFalse(sink.isDefinitelyMissing(File(dir, "received/missing-parent/absent").absolutePath))
        } finally { dir.deleteRecursively(); outside.deleteRecursively() }
    }
    @Test fun wrongExistingLengthOrHashFailsWithoutOverwritingAndCancelRemovesOwnGeneration() {
        val dir = root()
        try {
            val r = receipt(3); val sink = AndroidReceiptFiles(dir); val bytes = byteArrayOf(1, 2, 3)
            val path = sink.finalize(r, source(bytes))
            File(path).writeBytes(byteArrayOf(9, 9, 9))
            assertThrows(IllegalStateException::class.java) { sink.finalize(r, source(bytes)) }
            assertArrayEquals(byteArrayOf(9, 9, 9), File(path).readBytes())
            sink.discard(r); assertFalse(File(path).exists())
        } finally { dir.deleteRecursively() }
    }
    @Test fun destinationAppearingAtPublicationIsNeverOverwritten() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            val final = File(dir, "received/receipt-${r.receiptToken}.bin")
            val sink = AndroidReceiptFiles(dir) { if (it == "publish") final.writeBytes(byteArrayOf(9, 9, 9)) }
            assertThrows(IllegalStateException::class.java) { sink.finalize(r, source(bytes)) }
            assertArrayEquals(byteArrayOf(9, 9, 9), final.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun matchingExistingPublicationCanBeVerifiedWithoutOverwritingIt() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            val final = File(dir, "received/receipt-${r.receiptToken}.bin")
            var inode = 0L
            val sink = AndroidReceiptFiles(dir) { if (it == "publish") { final.writeBytes(bytes); inode = Os.lstat(final.path).st_ino } }
            assertEquals(final.path, sink.finalize(r, source(bytes)))
            assertEquals(inode, Os.lstat(final.path).st_ino)
            assertFalse(File(final.path + ".part").exists())
        } finally { dir.deleteRecursively() }
    }

    @Test fun foreignHardLinksAndCorruptCrashPairsAreRefusedWithoutUnlinking() {
        for (foreign in listOf(false, true)) {
            val dir = root()
            try {
                val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
                val final = File(dir, "received/receipt-${r.receiptToken}.bin")
                val part = File(final.path + ".part")
                val interrupted = AndroidReceiptFiles(dir) { if (it == "after-link") error("crash") }
                assertThrows(IllegalStateException::class.java) { interrupted.finalize(r, source(bytes)) }
                assertEquals(2L, Os.lstat(final.path).st_nlink)
                if (foreign) Os.link(final.path, File(dir, "foreign-link").path)
                else final.writeBytes(byteArrayOf(9, 9, 9))
                assertThrows(IllegalStateException::class.java) { AndroidReceiptFiles(dir).finalize(r, source(bytes)) }
                assertTrue(part.exists())
                assertTrue(final.exists())
                assertEquals(if (foreign) 3L else 2L, Os.lstat(final.path).st_nlink)
            } finally { dir.deleteRecursively() }
        }
    }

    @Test fun regularFileInDirectorySlotIsRejectedWithoutModifyingIt() {
        val dir = root()
        try {
            val impostor = File(dir, "received").apply { writeText("not a directory") }
            assertThrows(Exception::class.java) {
                AndroidReceiptFiles(dir).finalize(receipt(3), source(byteArrayOf(1, 2, 3)))
            }
            assertEquals("not a directory", impostor.readText())
            assertFalse(AndroidReceiptFiles(dir).isDefinitelyMissing(File(impostor, "absent").path))
        } finally { dir.deleteRecursively() }
    }

}
