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
internal class AndroidReceiptFiles(private val privateRoot: File, private val beforeOperation: (String) -> Unit = {}) : DurableReceiptSink {
    override fun finalize(receipt: ReceiveReceiptInfo, source: ReceiptSource): String = withDirectory { dir ->
        validate(receipt)
        val name = "receipt-${receipt.receiptToken}.bin"
        val temporary = "$name.part"
        if (dir.stat(name) != null) {
            recoverPublishedPair(dir, temporary, name, receipt, source)
            verify(dir, name, receipt, source)
            dir.sync()
            return@withDirectory dir.checkedPath(name)
        }
        // Only this exact generation's single-link regular temp may be discarded.
        dir.stat(temporary)?.let { previous ->
            check(previous.st_nlink == 1L)
            dir.removeMatching(temporary, previous)
        }
        beforeOperation("create")
        val descriptor = dir.open(temporary, OsConstants.O_WRONLY or OsConstants.O_CREAT or OsConstants.O_EXCL)
        val writtenIdentity = Os.fstat(descriptor)
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
        verify(dir, temporary, receipt, source)
        dir.assertPinned()
        // Atomic no-replace publication on the same filesystem. Android Os has no
        // renameat2(RENAME_NOREPLACE); link fails EEXIST instead of overwriting.
        beforeOperation("publish")
        try { Os.link(dir.child(temporary), dir.child(name)) }
        catch (e: ErrnoException) {
            if (e.errno != OsConstants.EEXIST) throw e
            recoverPublishedPair(dir, temporary, name, receipt, source)
            verify(dir, name, receipt, source)
            dir.stat(temporary)?.let { dir.removeMatching(temporary, writtenIdentity) }
            beforeOperation("directory-sync")
            dir.sync()
            return@withDirectory dir.checkedPath(name)
        }
        beforeOperation("after-link")
        val published = requireNotNull(dir.stat(name))
        check(published.st_dev == writtenIdentity.st_dev && published.st_ino == writtenIdentity.st_ino)
        beforeOperation("unlink")
        dir.removeMatching(temporary, writtenIdentity)
        beforeOperation("directory-sync")
        dir.sync()
        verify(dir, name, receipt, source)
        dir.checkedPath(name)
    }

    override fun discard(receipt: ReceiveReceiptInfo) = withDirectory { dir ->
        validate(receipt)
        val name = "receipt-${receipt.receiptToken}.bin"
        val temporary = "$name.part"
        val final = dir.stat(name)
        if (final != null && final.st_nlink != 1L) {
            val part = requireNotNull(dir.stat(temporary))
            check(final.st_nlink == 2L && part.st_nlink == 2L && final.st_dev == part.st_dev && final.st_ino == part.st_ino)
            dir.removeMatching(temporary, part)
        }
        for (owned in listOf(temporary, name)) {
            dir.stat(owned)?.let { identity ->
                check(identity.st_nlink == 1L)
                dir.removeMatching(owned, identity)
            }
        }
        dir.sync()
    }

    /** A crash after link but before unlink leaves exactly two names for one inode. */
    private fun recoverPublishedPair(dir: Directory, temporary: String, name: String, receipt: ReceiveReceiptInfo, source: ReceiptSource) {
        val final = requireNotNull(dir.stat(name))
        if (final.st_nlink == 1L) return
        check(final.st_nlink == 2L) { "unexpected final hard links" }
        val part = requireNotNull(dir.stat(temporary))
        check(part.st_dev == final.st_dev && part.st_ino == final.st_ino && part.st_nlink == 2L) { "unowned publication pair" }
        verify(dir, name, receipt, source, allowPublicationPair = true)
        val checkedFinal = requireNotNull(dir.stat(name))
        check(checkedFinal.st_dev == part.st_dev && checkedFinal.st_ino == part.st_ino && checkedFinal.st_nlink == 2L)
        dir.removeMatching(temporary, part)
        dir.sync()
        check(requireNotNull(dir.stat(name)).st_nlink == 1L)
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

    private fun verify(dir: Directory, name: String, r: ReceiveReceiptInfo, source: ReceiptSource, allowPublicationPair: Boolean = false) {
        val fd = dir.open(name, OsConstants.O_RDONLY, allowPublicationPair)
        try {
            check(Os.fstat(fd).st_size == r.size) { "received file length mismatch" }
            source.verifier().use { verifier ->
                val buffer = ByteArray(65_536)
                try {
                    var total = 0L
                    while (true) {
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
            val childFd = Os.open(parent.child("received"), OsConstants.O_RDONLY or OsConstants.O_DIRECTORY or OsConstants.O_NOFOLLOW or OsConstants.O_CLOEXEC, 0)
            Directory(File(root, "received"), childFd).use { dir ->
                parent.assertPinned()
                dir.assertPinned()
                return body(dir)
            }
        }
    }

    private class Directory(val path: File, descriptor: FileDescriptor = Os.open(path.absolutePath, OsConstants.O_RDONLY or OsConstants.O_DIRECTORY or OsConstants.O_NOFOLLOW or OsConstants.O_CLOEXEC, 0)) : AutoCloseable {
        private val handle = try { ParcelFileDescriptor.dup(descriptor) } finally { Os.close(descriptor) }
        private val identity = Os.fstat(handle.fileDescriptor)
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
        fun open(name: String, flags: Int, allowPublicationPair: Boolean = false): FileDescriptor {
            assertPinned()
            val fd = Os.open(child(name), flags or OsConstants.O_NOFOLLOW or OsConstants.O_CLOEXEC, 384)
            try { val stat = Os.fstat(fd); check(OsConstants.S_ISREG(stat.st_mode) && (stat.st_nlink == 1L || (allowPublicationPair && stat.st_nlink == 2L))) { "receipt is not private regular file" }; return fd }
            catch (e: Exception) { Os.close(fd); throw e }
        }
        fun removeMatching(name: String, identity: android.system.StructStat) {
            val current = requireNotNull(stat(name))
            check(current.st_dev == identity.st_dev && current.st_ino == identity.st_ino) { "owned temporary inode changed" }
            Os.unlink(child(name))
            assertPinned()
        }
        fun sync() { Os.fsync(handle.fileDescriptor); assertPinned() }
        override fun close() = handle.close()
    }

    companion object {
        private fun checkDirectoryChain(path: File) {
            val parts = generateSequence(path.absoluteFile) { it.parentFile }.toList().asReversed()
            for (part in parts) {
                val mode = Os.lstat(part.absolutePath).st_mode
                if (!OsConstants.S_ISDIR(mode) || OsConstants.S_ISLNK(mode)) throw IOException("unsafe private directory")
            }
        }
    }
}
