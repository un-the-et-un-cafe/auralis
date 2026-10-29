package chat.omcp.app.files

import android.os.ParcelFileDescriptor
import android.system.ErrnoException
import android.system.Os
import android.system.OsConstants
import chat.omcp.app.core.ReceiveReceiptInfo
import java.io.File
import java.io.FileDescriptor
import java.io.IOException

/** All child operations use a pinned private directory descriptor, never a peer filename. */
internal class AndroidReceiptFiles(
    private val privateRoot: File,
    private val namespace: Namespace = Namespace.Transport,
    private val beforeOperation: (String) -> Unit = {},
) : DurableReceiptSink {
    internal enum class Namespace { Transport, Backup }
    private fun fileName(receipt: ReceiveReceiptInfo) = when (namespace) {
        Namespace.Transport -> "receipt-${receipt.receiptToken}.bin"
        Namespace.Backup -> "backup-${receipt.receiptToken}.bin"
    }
    override fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource): String = withDirectory { dir ->
        validate(receipt)
        val name = fileName(receipt)
        check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
        val existing = dir.stat(name)
        if (existing != null) {
            check(existing.st_nlink == 1L) { "receipt has foreign hard links" }
            if (existing.st_size == receipt.size) {
                // A complete candidate is never rewritten, even when verification fails.
                verify(dir, name, receipt, source)
                beforeOperation("directory-sync")
                dir.sync()
                check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
                return@withDirectory dir.checkedPath(name)
            }
            check(existing.st_size < receipt.size) { "receipt exceeds expected size" }
            // Only a positively short, owned generation can be repaired. Read/verifier/
            // permission failures never enter this path. Legacy backup sources refuse it.
            check(source.authorizeWrite(replacingIncomplete = true)) { "incomplete receipt repair not authorized" }
            beforeOperation("remove-incomplete")
            check(source.authorizeWrite(replacingIncomplete = true)) { "receipt no longer pending" }
            dir.removeMatching(name, existing)
            dir.sync()
        }
        check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
        beforeOperation("create")
        check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
        val descriptor = dir.open(name, OsConstants.O_WRONLY or OsConstants.O_CREAT or OsConstants.O_EXCL)
        val writtenIdentity = try { Os.fstat(descriptor) } catch (e: Exception) { Os.close(descriptor); throw e }
        try {
            var written = 0L
            for (index in 0 until receipt.blockCount) {
                val block = source.readBlock(index)
                try {
                    val expected = minOf(65_536L, receipt.size - written).toInt()
                    check(block.size == expected && expected > 0) { "invalid receipt block size" }
                    var offset = 0
                    while (offset < block.size) {
                        beforeOperation("write")
                        val count = Os.write(descriptor, block, offset, block.size - offset)
                        check(count > 0) { "receipt write made no progress" }
                        offset += count
                    }
                    written += block.size
                } finally { block.fill(0) }
            }
            check(written == receipt.size) { "receipt length mismatch" }
            beforeOperation("file-sync")
            Os.fsync(descriptor)
        } finally { Os.close(descriptor) }
        val current = requireNotNull(dir.stat(name))
        check(current.st_dev == writtenIdentity.st_dev && current.st_ino == writtenIdentity.st_ino)
        verify(dir, name, receipt, source)
        beforeOperation("directory-sync")
        dir.sync()
        check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
        // File creation is private staging, not publication. Only the subsequent
        // token-bound database commit exposes Received/localPath to UI/FileProvider.
        dir.checkedPath(name)
    }

    /** User-authorized repair only: normal finalize never deletes a full-size bad candidate. */
    override fun repair(receipt: ReceiveReceiptInfo, source: ReceiptSource): String {
        check(namespace == Namespace.Transport) { "backup files cannot authorize receipt repair" }
        withDirectory { dir ->
            validate(receipt)
            check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
            val name = fileName(receipt)
            val candidate = requireNotNull(dir.stat(name))
            check(candidate.st_nlink == 1L && candidate.st_size == receipt.size) { "repair requires full-size private candidate" }
            try {
                verify(dir, name, receipt, source)
                // Already valid (e.g. an earlier commit failed): do not remove it.
                return@withDirectory
            } catch (_: chat.omcp.app.core.ReceiveContentMismatch) {
                // Only this typed positive mismatch permits the destructive repair path.
            }
            beforeOperation("repair-remove")
            check(source.authorizeRepair()) { "sealed receipt repair validation failed" }
            check(source.authorizeWrite(replacingIncomplete = false)) { "receipt no longer pending" }
            val current = requireNotNull(dir.stat(name))
            check(current.st_nlink == 1L && current.st_size == receipt.size &&
                current.st_dev == candidate.st_dev && current.st_ino == candidate.st_ino) { "repair candidate changed" }
            dir.removeMatching(name, candidate)
            dir.sync()
        }
        return finalize(receipt, source)
    }

    override fun discard(receipt: ReceiveReceiptInfo) = withDirectory { dir ->
        validate(receipt)
        val name = fileName(receipt)
        // Also clean pre-publication files from the earlier unpublished temp scheme.
        for (owned in listOf("$name.part", name)) {
            dir.stat(owned)?.let { identity ->
                check(identity.st_nlink == 1L)
                dir.removeMatching(owned, identity)
            }
        }
        dir.sync()
    }

    override fun isDefinitelyMissing(path: String): Boolean = try {
        val candidate = File(path)
        val root = privateRoot.canonicalFile
        val received = File(root, "received")
        if (!candidate.isAbsolute || candidate.toPath().normalize().toString() != candidate.absolutePath ||
            !candidate.absolutePath.startsWith(received.absolutePath + "/")) false
        else {
            check(!OsConstants.S_ISLNK(Os.lstat(privateRoot.absolutePath).st_mode))
            val parent = requireNotNull(candidate.parentFile)
            checkDirectoryChain(parent)
            val before = Os.lstat(parent.absolutePath)
            try { Os.lstat(candidate.absolutePath); false } catch (e: ErrnoException) {
                if (e.errno != OsConstants.ENOENT) throw e
                checkDirectoryChain(parent)
                val after = Os.lstat(parent.absolutePath)
                before.st_dev == after.st_dev && before.st_ino == after.st_ino
            }
        }
    } catch (_: Exception) { false }

    private fun validate(r: ReceiveReceiptInfo) {
        require(r.receiptToken.matches(Regex("[0-9a-f]{96}"))) { "invalid receipt capability" }
        require(r.transferId == r.receiptToken.take(32)) { "receipt/message mismatch" }
        require(r.contentHash.matches(Regex("[0-9a-f]{64}"))) { "invalid receipt digest" }
        require(r.size in 0..50_000_000 && r.blockCount.toLong() == (r.size + 65_535) / 65_536) { "invalid receipt dimensions" }
    }

    private fun verify(dir: Directory, name: String, r: ReceiveReceiptInfo, source: ReceiptSource) {
        val fd = dir.open(name, OsConstants.O_RDONLY)
        try {
            check(Os.fstat(fd).st_size == r.size) { "received file length mismatch" }
            source.verifier().use { verifier ->
                val buffer = ByteArray(65_536)
                try {
                    var total = 0L
                    while (true) {
                        beforeOperation("read")
                        val count = Os.read(fd, buffer, 0, buffer.size)
                        if (count == 0) break
                        check(count > 0 && total + count <= r.size) { "invalid receipt readback" }
                        val block = buffer.copyOf(count)
                        try { verifier.update(block) } finally { block.fill(0) }
                        total += count
                    }
                    check(total == r.size) { "short receipt readback" }
                    verifier.finish()
                } finally { buffer.fill(0) }
            }
            Os.fsync(fd)
        } finally { Os.close(fd) }
        dir.assertPinned()
    }

    private fun <T> withDirectory(body: (Directory) -> T): T {
        // Canonicalize trusted system aliases, but never allow the app's files leaf to be a link.
        check(!OsConstants.S_ISLNK(Os.lstat(privateRoot.absolutePath).st_mode)) { "private root is a symlink" }
        val root = privateRoot.canonicalFile
        checkDirectoryChain(root)
        Directory(root).use { parent ->
            try { Os.mkdir(parent.child("received"), 448) } catch (e: ErrnoException) {
                if (e.errno != OsConstants.EEXIST) throw e
            }
            parent.sync()
            parent.assertPinned()
            val childFd = openDirectoryDescriptor(parent.child("received"))
            Directory(File(root, "received"), childFd).use { dir ->
                parent.assertPinned()
                dir.assertPinned()
                return body(dir)
            }
        }
    }

    private class Directory(val path: File, descriptor: FileDescriptor = openDirectoryDescriptor(path.absolutePath)) : AutoCloseable {
        private val handle = try { ParcelFileDescriptor.dup(descriptor) } finally { Os.close(descriptor) }
        private val identity = try { Os.fstat(handle.fileDescriptor) } catch (e: Exception) { handle.close(); throw e }
        fun child(name: String): String {
            require(!name.contains('/') && name != "." && name != "..")
            return "/proc/self/fd/${handle.fd}/$name"
        }
        fun assertPinned() {
            checkDirectoryChain(path)
            val now = Os.lstat(path.absolutePath)
            check(now.st_dev == identity.st_dev && now.st_ino == identity.st_ino) { "private directory changed" }
        }
        fun checkedPath(name: String): String { assertPinned(); return File(path, name).absolutePath }
        fun stat(name: String): android.system.StructStat? = try {
            val stat = Os.lstat(child(name))
            check(OsConstants.S_ISREG(stat.st_mode)) { "receipt path is not a regular file" }
            stat
        } catch (e: ErrnoException) {
            if (e.errno != OsConstants.ENOENT) throw e
            assertPinned()
            null
        }
        fun open(name: String, flags: Int): FileDescriptor {
            assertPinned()
            val fd = Os.open(child(name), flags or OsConstants.O_NOFOLLOW or OsConstants.O_CLOEXEC, 384)
            try { val stat = Os.fstat(fd); check(OsConstants.S_ISREG(stat.st_mode) && stat.st_nlink == 1L) { "receipt is not private regular file" }; return fd }
            catch (e: Exception) { Os.close(fd); throw e }
        }
        fun removeMatching(name: String, identity: android.system.StructStat) {
            val current = requireNotNull(stat(name))
            check(current.st_dev == identity.st_dev && current.st_ino == identity.st_ino) { "owned temporary inode changed" }
            // Public API remove(3) unlinks this checked regular file. Paths are private,
            // generation-specific and all receipt mutations share the repository monitor.
            Os.remove(child(name))
            assertPinned()
        }
        fun sync() { Os.fsync(handle.fileDescriptor); assertPinned() }
        override fun close() = handle.close()
    }

    companion object {
        private fun openDirectoryDescriptor(path: String): FileDescriptor {
            // O_DIRECTORY is hidden from the public Android SDK. Validate the opened
            // no-follow descriptor instead; NONBLOCK prevents a raced FIFO from hanging.
            val fd = Os.open(path, OsConstants.O_RDONLY or OsConstants.O_NOFOLLOW or
                OsConstants.O_CLOEXEC or OsConstants.O_NONBLOCK, 0)
            try {
                check(OsConstants.S_ISDIR(Os.fstat(fd).st_mode)) { "receipt parent is not a directory" }
                return fd
            } catch (e: Exception) { Os.close(fd); throw e }
        }

        private fun checkDirectoryChain(path: File) {
            val parts = generateSequence(path.absoluteFile) { it.parentFile }.toList().asReversed()
            for (part in parts) {
                val mode = Os.lstat(part.absolutePath).st_mode
                if (!OsConstants.S_ISDIR(mode) || OsConstants.S_ISLNK(mode)) throw IOException("unsafe private directory")
            }
        }
    }
}
