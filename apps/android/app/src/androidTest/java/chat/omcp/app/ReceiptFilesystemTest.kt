package chat.omcp.app

import android.system.Os
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import chat.omcp.app.core.ReceiveReceiptInfo
import chat.omcp.app.core.ReceiptVerifier
import chat.omcp.app.core.ReceiveContentMismatch
import chat.omcp.app.files.AndroidReceiptFiles
import chat.omcp.app.files.ReceiptSource
import chat.omcp.app.files.TrustedReceivedPaths
import org.junit.Assert.*
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.security.MessageDigest

/** Actual Android syscalls, not JVM filesystem substitutes. */
@RunWith(AndroidJUnit4::class)
class ReceiptFilesystemTest {
    private val context get() = ApplicationProvider.getApplicationContext<android.content.Context>()
    private fun root() = File(context.cacheDir.canonicalFile, "receipt-test-${System.nanoTime()}").apply { check(mkdir()) }
    private fun receipt(size: Int) = ReceiveReceiptInfo("11".repeat(16), "22".repeat(32), "../../peer-name", "application/octet-stream", size.toLong(), "11".repeat(16) + "33".repeat(32), "44".repeat(32), (size + 65535) / 65536)
    private fun source(bytes: ByteArray, reads: (() -> Unit)? = null,
        authorization: (Boolean) -> Boolean = { true }, verifierFailure: Boolean = false,
        repairAuthorization: () -> Boolean = { false }) = object : ReceiptSource {
        override fun authorizeRepair() = repairAuthorization()
        override fun authorizeWrite(replacingIncomplete: Boolean) = authorization(replacingIncomplete)
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
                if (verifierFailure) throw java.io.IOException("verifier unavailable")
                check(count == bytes.size)
                if (!hash.digest().contentEquals(MessageDigest.getInstance("SHA-256").digest(bytes))) throw ReceiveContentMismatch()
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
                val recovered = sink.finalize(r, source(bytes, reads = { error("must verify existing final, not rewrite") }))
                assertEquals(path, recovered)
            }
        } finally { dir.deleteRecursively() }
    }
    @Test fun createWriteSyncAndDirectorySyncFaultsRetainRetryableGeneration() {
        for (fault in listOf("create", "write", "file-sync", "read", "directory-sync")) {
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
            assertThrows(ReceiveContentMismatch::class.java) { sink.finalize(r, source(bytes)) }
            assertArrayEquals(byteArrayOf(9, 9, 9), File(path).readBytes())
            sink.discard(r); assertFalse(File(path).exists())
        } finally { dir.deleteRecursively() }
    }
    @Test fun destinationAppearingAtExclusiveCreateIsNeverOverwritten() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            val final = File(dir, "received/receipt-${r.receiptToken}.bin")
            val sink = AndroidReceiptFiles(dir) { if (it == "create") final.writeBytes(byteArrayOf(9, 9, 9)) }
            val failure = assertThrows(android.system.ErrnoException::class.java) { sink.finalize(r, source(bytes)) }
            assertEquals(android.system.OsConstants.EEXIST, failure.errno)
            assertArrayEquals(byteArrayOf(9, 9, 9), final.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun matchingExistingCandidateIsVerifiedWithoutOverwritingIt() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            File(dir, "received").mkdir()
            val final = File(dir, "received/receipt-${r.receiptToken}.bin").apply { writeBytes(bytes) }
            val inode = Os.lstat(final.path).st_ino
            assertEquals(final.path, AndroidReceiptFiles(dir).finalize(r, source(bytes)))
            assertEquals(inode, Os.lstat(final.path).st_ino)
        } finally { dir.deleteRecursively() }
    }

    @Test fun interruptedShortWriteRepairsOnlyWithCurrentPendingAuthorization() {
        val dir = root()
        try {
            val bytes = ByteArray(65537) { 7 }; val r = receipt(bytes.size)
            var writes = 0
            val interrupted = AndroidReceiptFiles(dir) { if (it == "write" && ++writes == 2) error("crash") }
            assertThrows(IllegalStateException::class.java) { interrupted.finalize(r, source(bytes)) }
            val final = File(dir, "received/receipt-${r.receiptToken}.bin")
            assertEquals(65536L, final.length())
            val saved = final.readBytes(); val inode = Os.lstat(final.path).st_ino
            for (authorization in listOf<(Boolean) -> Boolean>({ false }, { replacing -> !replacing })) {
                assertThrows(IllegalStateException::class.java) {
                    AndroidReceiptFiles(dir).finalize(r, source(bytes, authorization = authorization))
                }
                assertEquals(inode, Os.lstat(final.path).st_ino)
                assertArrayEquals(saved, final.readBytes())
            }
            var pending = true
            val revoked = AndroidReceiptFiles(dir) { if (it == "remove-incomplete") pending = false }
            assertThrows(IllegalStateException::class.java) {
                revoked.finalize(r, source(bytes, authorization = { pending }))
            }
            assertEquals(inode, Os.lstat(final.path).st_ino)
            assertArrayEquals(saved, final.readBytes())
            assertArrayEquals(bytes, File(AndroidReceiptFiles(dir).finalize(r, source(bytes))).readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun existingReadVerifierAndAuthorizationFailuresNeverDeleteOrRewriteBytes() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            val final = File(AndroidReceiptFiles(dir).finalize(r, source(bytes)))
            val inode = Os.lstat(final.path).st_ino
            assertThrows(java.io.IOException::class.java) {
                AndroidReceiptFiles(dir) { if (it == "read") throw java.io.IOException("protected read") }.finalize(r, source(bytes))
            }
            assertThrows(java.io.IOException::class.java) {
                AndroidReceiptFiles(dir).finalize(r, source(bytes, verifierFailure = true))
            }
            assertThrows(IllegalStateException::class.java) {
                AndroidReceiptFiles(dir).finalize(r, source(bytes, authorization = { false }))
            }
            assertThrows(java.io.IOException::class.java) {
                AndroidReceiptFiles(dir).finalize(r, source(bytes, authorization = { throw java.io.IOException("database unavailable") }))
            }
            assertEquals(inode, Os.lstat(final.path).st_ino)
            assertArrayEquals(bytes, final.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun revokedCapabilityBeforeExclusiveCreateLeavesNoCandidate() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3)
            var pending = true
            val sink = AndroidReceiptFiles(dir) { if (it == "create") pending = false }
            assertThrows(IllegalStateException::class.java) {
                sink.finalize(r, source(bytes, authorization = { pending }))
            }
            assertFalse(File(dir, "received/receipt-${r.receiptToken}.bin").exists())
        } finally { dir.deleteRecursively() }
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

    @Test fun backupNamespaceCannotOverwriteTransportAndCannotRepairShortBackup() {
        val dir = root()
        try {
            val r = receipt(3); val liveBytes = byteArrayOf(1, 2, 3); val backupBytes = byteArrayOf(9, 9, 9)
            val live = File(AndroidReceiptFiles(dir).finalize(r, source(liveBytes)))
            val backupSink = AndroidReceiptFiles(dir, AndroidReceiptFiles.Namespace.Backup)
            val backup = File(backupSink.finalize(r, source(backupBytes, authorization = { replacing -> !replacing })))
            assertNotEquals(live.path, backup.path)
            assertArrayEquals(liveBytes, live.readBytes())
            assertArrayEquals(backupBytes, backup.readBytes())
            backup.writeBytes(byteArrayOf(9))
            val inode = Os.lstat(backup.path).st_ino
            assertThrows(IllegalStateException::class.java) {
                backupSink.finalize(r, source(backupBytes, authorization = { replacing -> !replacing }))
            }
            assertEquals(inode, Os.lstat(backup.path).st_ino)
            assertArrayEquals(byteArrayOf(9), backup.readBytes())
            assertArrayEquals(liveBytes, live.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun legacyTrustedRootAliasMapsWithoutResolvingDescendantSymlinks() {
        val dir = root(); val outside = root()
        try {
            val lexical = File(context.cacheDir, dir.name)
            val paths = TrustedReceivedPaths(lexical, dir)
            val sink = AndroidReceiptFiles(dir)
            File(dir, "received").mkdir()
            val actual = File(dir, "received/legacy.bin").apply { writeBytes(byteArrayOf(1, 2, 3)) }
            val oldPath = File(lexical, "received/legacy.bin").absolutePath
            // Existing legacy paths still open/read for media replay; no path rewrite in DB.
            assertArrayEquals(byteArrayOf(1, 2, 3), File(oldPath).readBytes())
            val mapped = requireNotNull(paths.map(oldPath))
            assertFalse(sink.isDefinitelyMissing(mapped))
            Os.remove(actual.path)
            assertTrue(sink.isDefinitelyMissing(mapped))
            Os.symlink(outside.path, File(dir, "received/sub").path)
            assertFalse(sink.isDefinitelyMissing(requireNotNull(paths.map(File(lexical, "received/sub/absent").path))))
            assertFalse(sink.isDefinitelyMissing(requireNotNull(paths.map(File(lexical, "received/missing-parent/absent").path))))
            assertNull(paths.map(File(lexical.parentFile, lexical.name + "-foreign/received/absent").path))
        } finally { dir.deleteRecursively(); outside.deleteRecursively() }
    }

    @Test fun explicitRepairRequiresTypedMismatchAndValidatedSpoolWhileRetryNeverDeletes() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3); val sink = AndroidReceiptFiles(dir)
            val file = File(sink.finalize(r, source(bytes)))
            file.writeBytes(byteArrayOf(9, 9, 9))
            val before = file.readBytes(); val inode = Os.lstat(file.path).st_ino
            assertThrows(ReceiveContentMismatch::class.java) { sink.finalize(r, source(bytes)) }
            assertEquals(inode, Os.lstat(file.path).st_ino); assertArrayEquals(before, file.readBytes())
            var validated = 0
            assertEquals(file.path, sink.repair(r, source(bytes, repairAuthorization = { validated++; true })))
            assertEquals(1, validated); assertArrayEquals(bytes, file.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun repairReadVerifierSpoolAndStaleErrorsPreserveCandidate() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3); val sink = AndroidReceiptFiles(dir)
            val file = File(sink.finalize(r, source(bytes))).apply { writeBytes(byteArrayOf(9, 9, 9)) }
            val before = file.readBytes(); val inode = Os.lstat(file.path).st_ino
            assertThrows(java.io.IOException::class.java) {
                AndroidReceiptFiles(dir) { if (it == "read") throw java.io.IOException("protected") }
                    .repair(r, source(bytes, repairAuthorization = { true }))
            }
            assertThrows(java.io.IOException::class.java) { sink.repair(r, source(bytes, verifierFailure = true, repairAuthorization = { true })) }
            assertThrows(IllegalStateException::class.java) { sink.repair(r, source(bytes)) } // No sealed validation.
            assertThrows(java.io.IOException::class.java) { sink.repair(r, source(bytes, repairAuthorization = { throw java.io.IOException("bad sealed block") })) }
            var pending = true
            val cancelled = AndroidReceiptFiles(dir) { if (it == "repair-remove") pending = false }
            assertThrows(IllegalStateException::class.java) { cancelled.repair(r, source(bytes, authorization = { pending }, repairAuthorization = { pending })) }
            assertEquals(inode, Os.lstat(file.path).st_ino); assertArrayEquals(before, file.readBytes())
            assertThrows(IllegalStateException::class.java) {
                AndroidReceiptFiles(dir, AndroidReceiptFiles.Namespace.Backup).repair(r, source(bytes, repairAuthorization = { true }))
            }
            assertArrayEquals(before, file.readBytes())
        } finally { dir.deleteRecursively() }
    }

    @Test fun repairRefusesCandidateInodeReplacementDuringValidation() {
        val dir = root()
        try {
            val bytes = byteArrayOf(1, 2, 3); val r = receipt(3); val sink = AndroidReceiptFiles(dir)
            val file = File(sink.finalize(r, source(bytes))).apply { writeBytes(byteArrayOf(9, 9, 9)) }
            val changed = AndroidReceiptFiles(dir) { if (it == "repair-remove") {
                Os.rename(file.path, file.path + ".saved")
                file.writeBytes(byteArrayOf(8, 8, 8))
            } }
            assertThrows(IllegalStateException::class.java) { changed.repair(r, source(bytes, repairAuthorization = { true })) }
            assertArrayEquals(byteArrayOf(8, 8, 8), file.readBytes())
            assertArrayEquals(byteArrayOf(9, 9, 9), File(file.path + ".saved").readBytes())
        } finally { dir.deleteRecursively() }
    }

}
