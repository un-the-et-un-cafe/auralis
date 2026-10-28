import Foundation
import Security
import XCTest
import HoplaKit
@testable import OpenMeshChat

final class AppLifecycleTests: XCTestCase {
    func testActualKeychainInsertUpdateAndReadBack() throws {
        // Exercise the app host's actual entitlements, not only injected SecItem closures.
        // A unique synthetic item never touches recovery phrases, storage keys or cloud keys.
        let account = "auralis.test.roundtrip.\(UUID().uuidString)"
        defer { Keychain.delete(account: account, synchronizable: false) }
        let first = Data([0x41, 0x55, 0x52])
        let updated = Data([0x41, 0x4c, 0x49, 0x53])
        _ = try XCTUnwrap(Keychain.write(first, account: account, synchronizable: false) ? true : nil,
                      "The app host must carry real Keychain entitlements; do not replace this test with an in-memory store.")
        XCTAssertEqual(Keychain.read(account: account), first)
        XCTAssertTrue(Keychain.write(updated, account: account, synchronizable: false))
        XCTAssertEqual(Keychain.read(account: account), updated)
    }

    func testLaunchScreenMetadataOptsIntoNativeDisplayGeometry() {
        XCTAssertNotNil(Bundle.main.object(forInfoDictionaryKey: "UILaunchScreen") as? [String: Any])
        XCTAssertNil(Bundle.main.object(forInfoDictionaryKey: "UILaunchScreen_Generation"),
                     "Generation is an Xcode build setting, not a valid raw launch-screen Info.plist entry.")
    }

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
