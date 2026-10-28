import Foundation

/// Settings › Backup and restore: status, last backup date, change password, add/remove a
/// method, delete the cloud backup (O5), view the phrase (device auth gated in the UI).
public final class BackupSettings {
    private let repo: MeshRepository
    private let stores: [String: CloudBackupStore]
    private let keyStore: BackupKeyStore?
    private let deviceHint: String

    public init(repo: MeshRepository, stores: [CloudBackupStore], keyStore: BackupKeyStore?, deviceHint: String = "iPhone") {
        self.repo = repo
        self.stores = Dictionary(uniqueKeysWithValues: stores.map { ($0.method, $0) })
        self.keyStore = keyStore
        self.deviceHint = deviceHint
    }

    public var methods: Set<String> { repo.store.backupMethods }
    public var lastCloudBackupMs: Int64 { repo.store.lastCloudBackupMs }
    public var gate: BackupGate { repo.backupGate() }
    public func available(_ method: String) -> Bool { stores[method]?.available ?? false }

    /// (Re)upload to a cloud method. password nil = iCloud "No password" key mode.
    public func backUp(_ method: String, password: String?) throws {
        guard let store = stores[method] else { throw CoreError.other("unavailable") }
        try prepareBackup(method, password: password).upload(to: store)
        completeBackup(method)
    }

    /// Repository/core work stays on its owner; only this sealed upload may run off-thread.
    public func prepareBackup(_ method: String, password: String?) throws -> PreparedCloudBackup {
        guard let s = stores[method], s.available else { throw CoreError.other("unavailable") }
        let core = repo.core
        let blob: Data
        if let password {
            guard core.checkBackupPassword(password).ok else { throw CoreError.invalid("weak password") }
            blob = try core.createBackup(password: password, deviceHint: deviceHint, nowMs: repo.now())
        } else {
            guard method == backupICloud, let keyStore else { throw CoreError.invalid("password required") }
            let key = try keyStore.load() ?? core.generateBackupKey()
            try keyStore.save(key)
            blob = try core.createBackup(key: key, deviceHint: deviceHint, nowMs: repo.now())
        }
        return PreparedCloudBackup(store: s, blob: blob)
    }

    public func completeBackup(_ method: String) {
        repo.store.lastCloudBackupMs = repo.now()
        repo.markBackupMethod(method)
    }

    /// Change password = re-seal under the new one and overwrite the record.
    public func changePassword(_ method: String, to newPassword: String) throws {
        try backUp(method, password: newPassword)
    }

    /// Deletes the remote record (and the iCloud Keychain key) and removes the method.
    public func deleteCloudBackup(_ method: String) throws {
        try prepareDeletion(method)()
        try completeDeletion(method)
    }

    public func prepareDeletion(_ method: String) throws -> () throws -> Void {
        guard let store = stores[method] else { throw CoreError.other("unavailable") }
        return { try store.delete() }
    }

    public func completeDeletion(_ method: String) throws {
        // The remote record is already gone. A Keychain cleanup failure must not leave a
        // nonexistent backup counted as protection (or keep the 24-hour gate open).
        repo.markBackupMethod(method, add: false)
        if method == backupICloud { try keyStore?.delete() }
    }

    /// Only call after LocalAuthentication succeeded.
    public func revealPhrase() throws -> [String] {
        try repo.core.recoveryPhrase().split(separator: " ").map(String.init)
    }

    public func markWritten() { repo.markBackupMethod(backupWritten) }
    public func removeWritten() { repo.markBackupMethod(backupWritten, add: false) }
}
