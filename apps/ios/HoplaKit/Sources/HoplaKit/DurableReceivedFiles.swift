import Foundation
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif

/// Descriptor-relative private sink. No peer-supplied filename becomes a path.
/// The caller serializes this with cancel/retry and commits the receipt only
/// after this method returns. Filesystem errors never consume sealed bytes.
public final class DurableReceivedFiles: FileSink {
    public enum Checkpoint: Equatable { case beforeWrite, beforeReadback, beforeFileSync, beforeRename, afterRename, beforeUnlink, beforeDirectorySync }
    private let root: URL
    private let checkpoint: (Checkpoint) throws -> Void
    private let prepareRoot: (URL) throws -> Void

    public init(root: URL, checkpoint: @escaping (Checkpoint) throws -> Void = { _ in },
                prepareRoot: @escaping (URL) throws -> Void = { _ in }) {
        self.root = root
        self.checkpoint = checkpoint
        self.prepareRoot = prepareRoot
    }

    /// Resolve only a trusted, OS-provided container/temporary-directory anchor.
    /// Never pass a peer path, an injected sink root, or an app-controlled descendant.
    /// Foundation's resolvingSymlinksInPath intentionally removes /private on Darwin,
    /// so it cannot supply a physical path for our subsequent no-follow walk.
    public static func physicalTrustedDirectory(_ anchor: URL) throws -> URL {
        guard anchor.isFileURL, anchor.path.hasPrefix("/"), !anchor.path.contains("\0") else {
            throw CoreError.storageUnavailable("invalid trusted directory anchor")
        }
        guard let physical = anchor.path.withCString({ realpath($0, nil) }) else {
            throw NSError(domain: NSPOSIXErrorDomain, code: Int(errno), userInfo: nil)
        }
        defer { free(physical) }
        let path = String(cString: physical)
        var info = stat()
        guard lstat(physical, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFDIR) else {
            throw CoreError.storageUnavailable("trusted anchor is not a physical directory")
        }
        let result = URL(fileURLWithPath: path, isDirectory: true)
        guard result.path == path else { throw CoreError.storageUnavailable("trusted directory path changed") }
        return result
    }

    private func failure(_ operation: String) -> Error {
        NSError(domain: NSPOSIXErrorDomain, code: Int(errno), userInfo: [NSLocalizedDescriptionKey: operation])
    }
    private func invalid() -> Error { CoreError.storageUnavailable("unsafe received-file path or metadata") }
    private func components(_ path: String) throws -> [String] {
        guard path.hasPrefix("/"), !path.contains("\0") else { throw invalid() }
        let pieces = path.split(separator: "/", omittingEmptySubsequences: true).map(String.init)
        guard !pieces.contains("."), !pieces.contains(".."), "/" + pieces.joined(separator: "/") == path else { throw invalid() }
        return pieces
    }
    private func directory(_ path: String, create: Bool) throws -> Int32 {
        let parts = try components(path)
        var current = open("/", O_RDONLY | O_DIRECTORY | O_CLOEXEC)
        guard current >= 0 else { throw failure("open filesystem root") }
        do {
            for part in parts {
                var next = openat(current, part, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                if next < 0, errno == ENOENT, create {
                    if mkdirat(current, part, mode_t(0o700)) != 0, errno != EEXIST { throw failure("create private directory") }
                    guard fsync(current) == 0 else { throw failure("sync parent directory") }
                    next = openat(current, part, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
                }
                guard next >= 0 else { throw failure("open no-follow directory") }
                close(current); current = next
            }
            return current
        } catch { close(current); throw error }
    }
    private func fileInfo(_ fd: Int32) throws -> stat {
        var info = stat()
        guard fstat(fd, &info) == 0 else { throw failure("inspect private file") }
        guard info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG), info.st_nlink == 1 else { throw invalid() }
        return info
    }
    private func sameRoot(_ fd: Int32) throws {
        let again = try directory(root.path, create: false); defer { close(again) }
        var before = stat(), after = stat()
        guard fstat(fd, &before) == 0, fstat(again, &after) == 0,
              before.st_dev == after.st_dev, before.st_ino == after.st_ino else { throw invalid() }
    }
    private func verify(_ fd: Int32, receipt: ReceiveReceiptInfo, verifier: ReceiveFileVerifier) throws {
        let info = try fileInfo(fd)
        guard info.st_size >= 0, UInt64(info.st_size) == receipt.size else { throw invalid() }
        guard lseek(fd, 0, SEEK_SET) == 0 else { throw failure("rewind received file") }
        try checkpoint(.beforeReadback)
        var total: UInt64 = 0
        var buffer = [UInt8](repeating: 0, count: 65_536)
        defer { _ = buffer.withUnsafeMutableBytes { $0.initializeMemory(as: UInt8.self, repeating: 0) } }
        while true {
            let count = buffer.withUnsafeMutableBytes { read(fd, $0.baseAddress!, $0.count) }
            if count < 0 { if errno == EINTR { continue }; throw failure("read back received file") }
            if count == 0 { break }
            total += UInt64(count)
            guard total <= receipt.size else { throw invalid() }
            try verifier.update(Data(buffer.prefix(count)))
        }
        guard total == receipt.size else { throw invalid() }
        try verifier.finish()
    }
    private func protect(_ fd: Int32) throws {
        guard fchmod(fd, mode_t(0o600)) == 0 else { throw failure("protect received file permissions") }
        #if os(iOS) && !targetEnvironment(simulator)
        // Darwin fcntl protection class C: available after first authentication.
        // Applied to the already-open descriptor, never a symlink-following path.
        guard fcntl(fd, F_SETPROTECTIONCLASS, 3) == 0 else { throw failure("set received-file protection class") }
        #endif
    }

    public func finalize(_ receipt: ReceiveReceiptInfo, readBlock: (UInt32) throws -> Data,
                         verifier: ReceiveFileVerifier) throws -> String {
        let hex = CharacterSet(charactersIn: "0123456789abcdef")
        guard receipt.receiptToken.count == 96, receipt.receiptToken.unicodeScalars.allSatisfy(hex.contains),
              receipt.transferId.count == 32, receipt.receiptToken.hasPrefix(receipt.transferId),
              receipt.contentHash.count == 64, receipt.contentHash.unicodeScalars.allSatisfy(hex.contains),
              receipt.size <= 50_000_000, UInt64(receipt.blockCount) == (receipt.size + 65_535) / 65_536 else { throw invalid() }
        let dir = try directory(root.path, create: true); defer { close(dir) }
        try prepareRoot(root)
        try sameRoot(dir)
        let name = receipt.receiptToken + ".bin"
        let temporary = receipt.receiptToken + ".part"
        let finalPath = root.appendingPathComponent(name).path
        let existing = openat(dir, name, O_RDWR | O_NOFOLLOW | O_CLOEXEC)
        if existing >= 0 {
            defer { close(existing) }
            // Recover only the exact two-name publication pair left by a crash.
            var linked = stat()
            guard fstat(existing, &linked) == 0 else { throw failure("inspect existing file") }
            if linked.st_nlink == 2 {
                let partial = openat(dir, temporary, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
                guard partial >= 0 else { throw invalid() }
                defer { close(partial) }
                var other = stat()
                guard fstat(partial, &other) == 0, other.st_dev == linked.st_dev,
                      other.st_ino == linked.st_ino, other.st_nlink == 2 else { throw invalid() }
                try checkpoint(.beforeUnlink)
                guard unlinkat(dir, temporary, 0) == 0 else { throw failure("remove published partial name") }
            }
            try verify(existing, receipt: receipt, verifier: verifier)
            try checkpoint(.beforeFileSync)
            guard fsync(existing) == 0 else { throw failure("sync existing received file") }
            try checkpoint(.beforeDirectorySync)
            guard fsync(dir) == 0 else { throw failure("sync received directory") }
            try sameRoot(dir)
            return finalPath
        }
        guard errno == ENOENT else { throw failure("inspect final received file") }
        // A previous crash may have left this token's partial file. Never follow
        // or truncate it; only unlink a regular unshared entry after inspection.
        let stale = openat(dir, temporary, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        if stale >= 0 {
            defer { close(stale) }
            _ = try fileInfo(stale)
            guard unlinkat(dir, temporary, 0) == 0 else { throw failure("remove stale private partial") }
        } else if errno != ENOENT { throw failure("inspect private partial") }
        let fd = openat(dir, temporary, O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, mode_t(0o600))
        guard fd >= 0 else { throw failure("create private partial") }
        var renamed = false
        defer { close(fd); if !renamed { _ = unlinkat(dir, temporary, 0) } }
        try protect(fd)
        var written: UInt64 = 0
        for index in 0..<receipt.blockCount {
            var bytes = try readBlock(index)
            defer { bytes.resetBytes(in: 0..<bytes.count) }
            let expected = min(UInt64(65_536), receipt.size - written)
            guard UInt64(bytes.count) == expected else { throw invalid() }
            try checkpoint(.beforeWrite)
            try bytes.withUnsafeBytes { raw in
                var offset = 0
                while offset < raw.count {
                    let count = write(fd, raw.baseAddress!.advanced(by: offset), raw.count - offset)
                    if count < 0 { if errno == EINTR { continue }; throw failure("write private partial") }
                    guard count > 0 else { throw invalid() }
                    offset += count
                }
            }
            written += UInt64(bytes.count)
        }
        guard written == receipt.size else { throw invalid() }
        try verify(fd, receipt: receipt, verifier: verifier)
        try checkpoint(.beforeFileSync)
        guard fsync(fd) == 0 else { throw failure("sync private partial") }
        try checkpoint(.beforeRename)
        // linkat is atomic and refuses EEXIST: never clobber a raced-in final.
        guard linkat(dir, temporary, dir, name, 0) == 0 else { throw failure("publish received file without replacement") }
        renamed = true
        try checkpoint(.afterRename)
        try checkpoint(.beforeUnlink)
        guard unlinkat(dir, temporary, 0) == 0 else { throw failure("remove published partial name") }
        // Prove the path resolves no-follow to the same verified inode before
        // returning a reference that the store may commit.
        let published = openat(dir, name, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        guard published >= 0 else { throw failure("inspect published received file") }
        defer { close(published) }
        let source = try fileInfo(fd), destination = try fileInfo(published)
        guard source.st_dev == destination.st_dev, source.st_ino == destination.st_ino else { throw invalid() }
        try checkpoint(.beforeDirectorySync)
        guard fsync(dir) == 0 else { throw failure("sync received directory") }
        try sameRoot(dir)
        return finalPath
    }

    public func isDefinitelyMissing(_ path: String) throws -> Bool {
        guard path.hasPrefix(root.path + "/") else { throw invalid() }
        _ = try components(path)
        let url = URL(fileURLWithPath: path)
        let parentPath = url.deletingLastPathComponent().path
        let parent = try directory(parentPath, create: false)
        defer { close(parent) }
        let fd = openat(parent, url.lastPathComponent, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        if fd >= 0 { defer { close(fd) }; _ = try fileInfo(fd); return false }
        if errno == ENOENT {
            // ENOENT is meaningful only for the leaf beneath still-identical,
            // no-follow ancestors, never for a missing/moved container directory.
            let again = try directory(parentPath, create: false); defer { close(again) }
            var pinned = stat(), current = stat()
            guard fstat(parent, &pinned) == 0, fstat(again, &current) == 0,
                  pinned.st_dev == current.st_dev, pinned.st_ino == current.st_ino else { throw invalid() }
            return true
        }
        throw failure("check received-file availability")
    }

    /// Only this cancelled generation, never a directory scan or contact-based GC.
    public func discardCancelled(_ receipt: ReceiveReceiptInfo) throws {
        guard receipt.receiptToken.count == 96, receipt.transferId.count == 32,
              receipt.receiptToken.hasPrefix(receipt.transferId),
              receipt.receiptToken.unicodeScalars.allSatisfy(CharacterSet(charactersIn: "0123456789abcdef").contains) else { throw invalid() }
        let dir = try directory(root.path, create: false); defer { close(dir) }
        let names = [receipt.receiptToken + ".part", receipt.receiptToken + ".bin"]
        var opened: [(String, Int32, stat)] = []
        defer { for (_, fd, _) in opened { close(fd) } }
        for name in names {
            let fd = openat(dir, name, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
            if fd < 0 { if errno == ENOENT { continue }; throw failure("inspect cancelled receipt file") }
            var info = stat()
            guard fstat(fd, &info) == 0, info.st_mode & mode_t(S_IFMT) == mode_t(S_IFREG) else { close(fd); throw invalid() }
            opened.append((name, fd, info))
        }
        for (_, _, info) in opened where info.st_nlink != 1 {
            guard info.st_nlink == 2, opened.count == 2,
                  opened.allSatisfy({ $0.2.st_dev == info.st_dev && $0.2.st_ino == info.st_ino }) else { throw invalid() }
        }
        try sameRoot(dir)
        for (name, _, info) in opened {
            var current = stat()
            guard fstatat(dir, name, &current, AT_SYMLINK_NOFOLLOW) == 0,
                  current.st_dev == info.st_dev, current.st_ino == info.st_ino else { throw invalid() }
            guard unlinkat(dir, name, 0) == 0 else { throw failure("remove cancelled receipt file") }
        }
        guard fsync(dir) == 0 else { throw failure("sync cancelled receipt cleanup") }
        try sameRoot(dir)
    }

    public func readForResend(_ path: String) throws -> Data? {
        if try isDefinitelyMissing(path) { return nil }
        let url = URL(fileURLWithPath: path)
        let dir = try directory(url.deletingLastPathComponent().path, create: false)
        defer { close(dir) }
        let fd = openat(dir, url.lastPathComponent, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        guard fd >= 0 else { throw failure("open media for resend") }
        defer { close(fd) }
        let info = try fileInfo(fd)
        guard info.st_size >= 0, info.st_size <= 50_000_000 else { throw invalid() }
        var result = Data(), buffer = [UInt8](repeating: 0, count: 65_536)
        defer { _ = buffer.withUnsafeMutableBytes { $0.initializeMemory(as: UInt8.self, repeating: 0) } }
        while true {
            let count = buffer.withUnsafeMutableBytes { read(fd, $0.baseAddress!, $0.count) }
            if count < 0 { if errno == EINTR { continue }; throw failure("read media for resend") }
            if count == 0 { break }
            guard result.count + count <= Int(info.st_size) else { throw invalid() }
            result.append(contentsOf: buffer.prefix(count))
        }
        guard result.count == Int(info.st_size) else { throw invalid() }
        return result // Existing FFI resend API requires one bounded full payload.
    }

    /// Legacy backup helper only: Rust already authenticated the backup payload.
    /// Verify exact readback bytes in an isolated namespace; transport uses BLAKE3.
    public func save(_ file: ReceivedFileInfo) -> String? {
        guard file.transferId.count == 32,
              file.transferId.unicodeScalars.allSatisfy(CharacterSet(charactersIn: "0123456789abcdef").contains),
              file.bytes.count <= 50_000_000 else { return nil }
        let receipt = ReceiveReceiptInfo(transferId: file.transferId, contactId: file.contactId,
            name: file.name, mime: file.mime, size: UInt64(file.bytes.count),
            receiptToken: file.transferId + String(repeating: "0", count: 64),
            contentHash: String(repeating: "0", count: 64), blockCount: UInt32((file.bytes.count + 65_535) / 65_536))
        let legacy = DurableReceivedFiles(root: root.appendingPathComponent("legacy", isDirectory: true),
                                          checkpoint: checkpoint, prepareRoot: prepareRoot)
        return try? legacy.finalize(receipt, readBlock: { index in
            let start = Int(index) * 65_536
            return file.bytes.subdata(in: start..<min(start + 65_536, file.bytes.count))
        }, verifier: ExactReadbackVerifier(file.bytes))
    }
}

/// Only for already-authenticated legacy backup bytes, never transport receipts.
private final class ExactReadbackVerifier: ReceiveFileVerifier {
    private let expected: Data
    private var offset = 0
    init(_ expected: Data) { self.expected = expected }
    func update(_ bytes: Data) throws {
        guard bytes.count <= expected.count - offset,
              expected.subdata(in: offset..<(offset + bytes.count)) == bytes else {
            throw CoreError.storageUnavailable("legacy file readback mismatch")
        }
        offset += bytes.count
    }
    func finish() throws {
        guard offset == expected.count else { throw CoreError.storageUnavailable("legacy file length mismatch") }
    }
}
