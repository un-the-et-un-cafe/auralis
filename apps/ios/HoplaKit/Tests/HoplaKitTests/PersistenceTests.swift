import XCTest
@testable import HoplaKit
@testable import HoplaKit

/// Repository side of the encrypted local store (the store itself is tested in Rust and in
/// RustBridgeTests): the key is generated once and kept in the AppStore (Keychain on device),
/// and contacts no longer go to plain UserDefaults once a store is in use.
final class PersistenceTests: XCTestCase {
    func testStorageKeyIsGeneratedOnceAndContactsStayOutOfDefaults() throws {
        let store = InMemoryAppStore()
        store.contacts = [Contact(id: String(repeating: "ab", count: 32), displayName: "Old", safetyNumber: [], verified: true)]
        store.recoveryPhrase = try FakeOmcpCore().generateRecoveryPhrase()
        let repo = MeshRepository(core: FakeOmcpCore(), store: store, dbPath: "/tmp/omcp-test.db")
        XCTAssertTrue(repo.restore())
        let key = try XCTUnwrap(store.storageKey)
        XCTAssertEqual(key.count, 32)
        XCTAssertTrue(store.contacts.isEmpty, "migrated into the core, removed from defaults")
        XCTAssertEqual(repo.contacts.first?.displayName, "Old")
        XCTAssertTrue(MeshRepository(core: FakeOmcpCore(), store: store, dbPath: "/tmp/omcp-test.db").restore())
        XCTAssertEqual(store.storageKey, key)
    }

    func testWithoutStoreContactsStillPersistToDefaults() throws {
        let (repo, core) = makeRepo()
        try repo.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "Ana")
        XCTAssertNil(repo.store.storageKey)
    }

    func testBackupHistoryIsOptional() {
        XCTAssertNil(RestoredBackup(phrase: "p", contacts: []).history)
        XCTAssertEqual(RestoredBackup(phrase: "p", contacts: [], history: Data([1])).history, Data([1]))
    }
}

final class ChatBackupSyncTests: XCTestCase {
    func testBlobSeqParsingAndFakeCoreUploadsNothing() throws {
        XCTAssertEqual(ChatBackupSync.seq(of: "hopla-chat-0000000000000012-1.bin"), 12)
        XCTAssertNil(ChatBackupSync.seq(of: "hopla-media-abc.bin"))
        let cloud = InMemoryChatBlobStore()
        let sync = ChatBackupSync(core: FakeOmcpCore(), blobs: cloud)
        XCTAssertEqual(try sync.runText(onWifi: true, force: true), 0)
        XCTAssertTrue(cloud.blobs.isEmpty)
    }
}

/// Simulates a Keychain that rejects writes while the rest of preferences remain writable.
private final class RejectingSecretStore: AppStore {
    var sealedPhraseUnavailable = false
    var sealedKeyUnavailable = false
    var hasSavedIdentity: Bool { sealedPhraseUnavailable || recoveryPhrase != nil }
    var hasSavedStorageKey: Bool { sealedKeyUnavailable || storageKey != nil }
    var rejectPhrase = false
    var rejectKey = false
    private var phrase: String?
    private var key: Data?
    var recoveryPhrase: String? { get { phrase } set { if !rejectPhrase { phrase = newValue } } }
    var storageKey: Data? { get { key } set { if !rejectKey { key = newValue } } }
    var displayName: String?
    var contacts: [Contact] = []
    var onboarded = false
    var privateNotifications = false
    var chatPrivateOverride: [String: Bool] = [:]
    var hideOnLockScreen = false
    var burnChats: [String: Int] = [:]
    var burnUnreadTtl: [String: Int] = [:]
    var gatewayEnabled = false
    var gatewayAllowMobile = false
    var backupMethods: Set<String> = []
    var lastCloudBackupMs: Int64 = 0
    var backupRemindDeadlineMs: Int64 = 0
}

final class SecretPersistenceFailureTests: XCTestCase {
    func testOnboardingCannotAdvanceAfterRejectedPhraseWriteAndCanRetry() {
        let store = RejectingSecretStore()
        store.rejectPhrase = true
        let (repo, _) = makeRepo(store: store)
        let flow = OnboardingFlow(repo: repo, iCloud: nil, drive: nil, keyStore: nil)
        flow.getStarted(); flow.profileNext()
        XCTAssertEqual(flow.state.step, .profile)
        XCTAssertEqual(flow.state.error, "identity")
        XCTAssertNil(store.recoveryPhrase)
        XCTAssertFalse(store.onboarded)
        // Core may already be loaded; retry must persist the same identity, not skip the write.
        let id = repo.core.identity()?.peerIdHex
        store.rejectPhrase = false
        flow.profileNext()
        XCTAssertEqual(flow.state.step, .secure)
        XCTAssertNotNil(store.recoveryPhrase)
        XCTAssertEqual(repo.core.identity()?.peerIdHex, id)
    }

    func testRejectedStorageKeyDoesNotOpenDatabaseWithAnUnpersistedKey() throws {
        let store = RejectingSecretStore()
        store.rejectKey = true
        let core = FakeOmcpCore()
        let repo = MeshRepository(core: core, store: store, dbPath: "/tmp/auralis-rejected-key.db")
        XCTAssertThrowsError(try repo.saveIdentity(phrase: core.generateRecoveryPhrase(), name: "A"))
        XCTAssertNil(core.identity())
        XCTAssertNil(store.storageKey)
        XCTAssertNil(store.recoveryPhrase)
    }
}

final class StoreOpenFailureTests: XCTestCase {
    func testFailedOpenPreservesFilesAndRetriesOnceWithSameKey() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appendingPathComponent("store.db")
        let journal = dir.appendingPathComponent("store.db-journal")
        let original = Data([1, 2, 3]), originalJournal = Data([4, 5])
        try original.write(to: db); try originalJournal.write(to: journal)
        let core = FakeOmcpCore(), store = InMemoryAppStore()
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        store.storageKey = Data(repeating: 7, count: 32)
        core.storageOpenError = .storageUnavailable("locked")
        let repo = MeshRepository(core: core, store: store, dbPath: db.path)
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertEqual(core.storageOpenCount, 1)
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertEqual(core.storageOpenCount, 2)
        XCTAssertEqual(try Data(contentsOf: db), original)
        XCTAssertEqual(try Data(contentsOf: journal), originalJournal)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: dir.path).count, 2)
        core.storageOpenError = nil
        XCTAssertTrue(repo.restore())
        XCTAssertFalse(repo.storageBlocked)
        XCTAssertEqual(core.storageOpenCount, 3)
        XCTAssertEqual(store.storageKey, Data(repeating: 7, count: 32))
    }

    func testExistingStoreWithMissingKeyOrIdentityFailsClosed() throws {
        let db = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try Data([1]).write(to: db)
        defer { try? FileManager.default.removeItem(at: db) }
        let core = FakeOmcpCore(), store = InMemoryAppStore()
        let repo = MeshRepository(core: core, store: store, dbPath: db.path)
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertNil(store.storageKey)
        XCTAssertEqual(core.storageOpenCount, 0)
        XCTAssertEqual(try Data(contentsOf: db), Data([1]))
    }

    func testMissingRequiredStorePathDoesNotFallBackToMemory() throws {
        let core = FakeOmcpCore(), store = InMemoryAppStore()
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        let repo = MeshRepository(core: core, store: store, persistentStoreRequired: true)
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertNil(core.identity())
    }

    func testConfirmedAbsentDatabaseCanCreateKeyAndOpen() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let path = dir.appendingPathComponent("absent.db").path
        XCTAssertThrowsError(try FileManager.default.attributesOfItem(atPath: path))
        let core = FakeOmcpCore(), store = InMemoryAppStore()
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        let repo = MeshRepository(core: core, store: store, dbPath: path)
        XCTAssertTrue(repo.restore())
        XCTAssertFalse(repo.storageBlocked)
        XCTAssertEqual(core.storageOpenCount, 1)
        XCTAssertEqual(store.storageKey?.count, 32)
    }

    func testUnavailableStoreDirectoryCanResolveOnRetryWithoutMemoryFallback() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let canonical = dir.appendingPathComponent("store.db").path
        var path: String?
        var resolutions = 0
        let core = FakeOmcpCore(), store = InMemoryAppStore()
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        let repo = MeshRepository(core: core, store: store, persistentStoreRequired: true,
                                  dbPathProvider: { resolutions += 1; return path })
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertNil(core.identity())
        XCTAssertNil(store.storageKey)
        XCTAssertEqual(core.storageOpenCount, 0)
        path = canonical
        XCTAssertTrue(repo.restore())
        XCTAssertFalse(repo.storageBlocked)
        XCTAssertNotNil(core.identity())
        let count = resolutions
        path = nil
        XCTAssertTrue(repo.restore())
        XCTAssertEqual(resolutions, count, "a resolved canonical store path is never replaced")
    }

    func testUnavailableSealedSecretsAreNotTreatedAsAbsent() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let core = FakeOmcpCore(), store = RejectingSecretStore()
        let repo = MeshRepository(core: core, store: store, dbPath: dir.appendingPathComponent("absent.db").path)
        store.sealedPhraseUnavailable = true
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertNil(store.recoveryPhrase)
        XCTAssertNil(store.storageKey)
        store.sealedPhraseUnavailable = false
        store.recoveryPhrase = try core.generateRecoveryPhrase()
        store.sealedKeyUnavailable = true
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertNil(store.storageKey)
        XCTAssertEqual(core.storageOpenCount, 0)
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: dir.path).isEmpty)
    }

    func testNewInstallCanStillOnboard() {
        let repo = MeshRepository(core: FakeOmcpCore(), store: InMemoryAppStore())
        XCTAssertFalse(repo.restore())
        XCTAssertFalse(repo.storageBlocked)
    }
}
