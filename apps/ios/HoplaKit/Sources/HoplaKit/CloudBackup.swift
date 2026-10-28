import Foundation

/// A cloud place for the encrypted backup blob (iCloud CloudKit private DB, Google Drive
/// appDataFolder). Implementations only ever see the sealed blob from the Rust core.
public protocol CloudBackupStore: AnyObject {
    var method: String { get }
    var available: Bool { get }
    func upload(_ blob: Data) throws
    func download() throws -> Data?
    func delete() throws
}

/// The 256-bit key for iCloud "No password" mode. iOS: iCloud Keychain item with
/// kSecAttrSynchronizable + kSecAttrAccessibleAfterFirstUnlock.
public protocol BackupKeyStore: AnyObject {
    func load() throws -> Data?
    func save(_ key: Data) throws
    func delete() throws
}

public final class InMemoryCloudBackupStore: CloudBackupStore {
    public let method: String
    public var available: Bool
    public private(set) var uploaded: [Data] = []
    public var blob: Data?
    public init(method: String, available: Bool = true) {
        self.method = method
        self.available = available
    }
    public func upload(_ blob: Data) throws {
        uploaded.append(blob)
        self.blob = blob
    }
    public func download() throws -> Data? { blob }
    public func delete() throws { blob = nil }
}

public final class InMemoryBackupKeyStore: BackupKeyStore {
    public var key: Data?
    public init() {}
    public func load() throws -> Data? { key }
    public func save(_ key: Data) throws { self.key = key }
    public func delete() throws { key = nil }
}

/// A prepared upload owns only a sealed blob and destination identifier, never a provider,
/// repository or UI state.
/// Prepare on the repository's owner, upload on a worker, then commit success on the owner.
public struct PreparedCloudBackup: Sendable {
    public let method: String
    private let blob: Data

    public init(store: CloudBackupStore, blob: Data) {
        self.method = store.method
        self.blob = blob
    }

    public func upload(to store: CloudBackupStore) throws {
        guard store.method == method else { throw CoreError.invalid("backup destination") }
        try store.upload(blob)
    }
}
