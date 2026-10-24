import XCTest
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
