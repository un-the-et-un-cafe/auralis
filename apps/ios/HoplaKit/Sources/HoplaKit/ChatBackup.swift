import Foundation

/// Named, already-encrypted blobs in the provider's app-private space (iCloud / Drive appDataFolder).
/// Only Rust-sealed, 64 KiB-padded chat-backup blobs go here (REQUIREMENTS "Encrypted chat backup").
public protocol ChatBlobStore: AnyObject {
    var available: Bool { get }
    func list(prefix: String) throws -> [String]
    func put(_ name: String, _ blob: Data) throws
    func get(_ name: String) throws -> Data?
    func delete(_ name: String) throws
}

/// Tests, previews and simulated builds.
public final class InMemoryChatBlobStore: ChatBlobStore {
    public private(set) var blobs: [String: Data] = [:]
    public let available = true
    public init() {}
    public func list(prefix: String) -> [String] { blobs.keys.filter { $0.hasPrefix(prefix) }.sorted() }
    public func put(_ name: String, _ blob: Data) { blobs[name] = blob }
    public func get(_ name: String) -> Data? { blobs[name] }
    public func delete(_ name: String) { blobs[name] = nil }
}

public struct ChatRestoreResult: Equatable, Sendable {
    public var messages: Int
    public var mediaPlaceholders: Int
    public var mediaRestored: Int
}

/// Hourly text pass (Wi-Fi only by default), daily media pass (CB5, off by default) and restore.
/// Crypto, padding and change tracking are in Rust; this only moves opaque blobs. Blocking.
public final class ChatBackupSync {
    public static let chatPrefix = "hopla-chat-"
    public static let mediaPrefix = "hopla-media-"

    private let core: OmcpCore
    private let blobs: ChatBlobStore
    private let readFile: (String) -> Data?
    private let clock: () -> Int64

    public init(core: OmcpCore, blobs: ChatBlobStore,
                readFile: @escaping (String) -> Data? = { FileManager.default.contents(atPath: $0) },
                clock: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }) {
        self.core = core; self.blobs = blobs; self.readFile = readFile; self.clock = clock
    }

    /// `hopla-chat-<seq>-<kind>.bin` → seq.
    public static func seq(of name: String) -> UInt64? {
        guard name.hasPrefix(chatPrefix) else { return nil }
        return UInt64(name.dropFirst(chatPrefix.count).prefix { $0 != "-" })
    }

    @discardableResult
    public func runText(onWifi: Bool, allowMobile: Bool = false, force: Bool = false) throws -> Int {
        guard blobs.available else { return 0 }
        let now = clock()
        if !force && !core.chatBackupDue(nowMs: now, onWifi: onWifi, allowMobile: allowMobile) { return 0 }
        guard let b = try core.chatBackupNext(nowMs: now, forceSnapshot: false) else { return 0 }
        try blobs.put(b.name, b.blob)
        try core.chatBackupCommit(seq: b.seq)
        if let below = b.deleteBelowSeq {
            for n in try blobs.list(prefix: Self.chatPrefix) where (Self.seq(of: n) ?? .max) < below { try? blobs.delete(n) }
        }
        return 1
    }

    @discardableResult
    public func runMedia(onWifi: Bool) throws -> Int {
        guard blobs.available, core.mediaBackupDue(nowMs: clock(), onWifi: onWifi) else { return 0 }
        var n = 0
        for item in core.mediaBackupTodo() {
            guard let path = item.localPath, let bytes = readFile(path),
                  let b = try? core.sealMediaBackup(hash: item.hash, bytes: bytes) else { continue }
            try blobs.put(b.name, b.blob)
            try core.mediaBackupCommit(hash: item.hash, nowMs: clock())
            n += 1
        }
        return n
    }

    /// Turning media off deletes the media blobs (the UI asked first, §5.96).
    public func setMedia(_ p: MediaBackupPrefs) throws {
        for name in try core.setMediaBackupSettings(p) { try? blobs.delete(name) }
    }

    /// Media in the backup are restored hash-verified via `saveMedia`; the rest stay placeholders.
    public func restore(saveMedia: (String, String, Data) -> String?) throws -> ChatRestoreResult {
        guard blobs.available else { return ChatRestoreResult(messages: 0, mediaPlaceholders: 0, mediaRestored: 0) }
        let names = try blobs.list(prefix: Self.chatPrefix)
        if names.isEmpty { return ChatRestoreResult(messages: 0, mediaPlaceholders: 0, mediaRestored: 0) }
        let s = try core.chatBackupRestore(names.compactMap { try? blobs.get($0) })
        var restored = 0
        for p in core.mediaPlaceholders() {
            guard let blob = try? blobs.get(p.blobName),
                  let bytes = try? core.restoreMediaFromBackup(msgId: p.msgId, blob: blob),
                  let path = saveMedia(p.msgId, p.mime, bytes) else { continue }
            try? core.setReceivedFilePath(p.msgId, path: path)
            restored += 1
        }
        return ChatRestoreResult(messages: s.messages, mediaPlaceholders: s.mediaPlaceholders - restored, mediaRestored: restored)
    }
}
