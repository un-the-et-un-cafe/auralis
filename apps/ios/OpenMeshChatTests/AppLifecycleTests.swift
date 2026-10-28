import Foundation
import Security
import XCTest
import HoplaKit
@testable import OpenMeshChat

final class AppLifecycleTests: XCTestCase {
    func testKeychainUpdateDoesNotAttemptInsertAfterAccessFailure() {
        var adds = 0
        let saved = Keychain.write(Data([1]), account: "test", synchronizable: false,
                                   update: { _, _ in errSecInteractionNotAllowed },
                                   add: { _, _ in adds += 1; return errSecSuccess })
        XCTAssertFalse(saved)
        XCTAssertEqual(adds, 0)
    }

    func testKeychainExistingItemUpdatesWithoutDeleteOrInsert() {
        var adds = 0
        XCTAssertTrue(Keychain.write(Data([1]), account: "test", synchronizable: false,
                                    update: { _, _ in errSecSuccess },
                                    add: { _, _ in adds += 1; return errSecSuccess }))
        XCTAssertEqual(adds, 0)
    }

    func testKeychainConcurrentInsertRetriesUpdate() {
        var updates = 0
        XCTAssertTrue(Keychain.write(Data([1]), account: "test", synchronizable: false,
                                    update: { _, _ in updates += 1; return updates == 1 ? errSecItemNotFound : errSecSuccess },
                                    add: { _, _ in errSecDuplicateItem }))
        XCTAssertEqual(updates, 2)
    }

    @MainActor
    func testBackupDeadlineUpdatesWithoutMessagesAndAfterResume() async {
        let store = InMemoryAppStore()
        var now: Int64 = 1000
        store.backupRemindDeadlineMs = now + 100
        let model = AppModel(core: FakeOmcpCore(), store: store, persistentStore: false, clock: { now })
        XCTAssertEqual(model.backupGate, .banner)
        now += 100
        model.refreshTimeSensitiveState()
        XCTAssertEqual(model.backupGate, .required)
        store.backupMethods.insert(backupWritten)
        model.refreshTimeSensitiveState()
        XCTAssertEqual(model.backupGate, .ok)
    }

    @MainActor
    func testInvalidStoredIdentityCannotBypassOnboarding() async {
        let store = InMemoryAppStore()
        store.recoveryPhrase = "invalid recovery phrase"
        store.onboarded = true
        let model = AppModel(core: FakeOmcpCore(), store: store, persistentStore: false)
        XCTAssertFalse(model.onboarded)
    }
}
