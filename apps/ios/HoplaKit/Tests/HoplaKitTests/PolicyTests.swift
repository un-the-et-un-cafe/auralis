import XCTest
@testable import HoplaKit

final class PolicyTests: XCTestCase {
    /// SS1/SS2 policy: secure layer always on, cover in the app switcher and while captured.
    func testSS_coverPolicy() {
        XCTAssertTrue(ScreenCapturePolicy.secureLayerAlwaysOn)
        XCTAssertFalse(ScreenCapturePolicy.sendsScreenshotNotice)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .active, isCaptured: false), .none)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .inactive, isCaptured: false), .appSwitcher)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .background, isCaptured: false), .appSwitcher)
        XCTAssertEqual(ScreenCapturePolicy.cover(phase: .active, isCaptured: true), .recording)
    }

    /// GATT values must match Android exactly (reads the Kotlin source when present).
    func testGattMatchesAndroid() throws {
        let kt = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("android/app/src/main/java/chat/omcp/app/core/OmcpCore.kt")
        guard let src = try? String(contentsOf: kt, encoding: .utf8) else { throw XCTSkip("Android source not available") }
        for (name, v) in [("SERVICE", OmcpGatt.service), ("INBOX", OmcpGatt.inbox), ("OUTBOX", OmcpGatt.outbox),
                          ("CONTROL", OmcpGatt.control), ("CCCD", OmcpGatt.cccd)] {
            XCTAssertTrue(src.contains("const val \(name) = \"\(v)\""), name)
        }
        XCTAssertTrue(src.contains("MANUFACTURER_ID = 0xFFFF"))
        XCTAssertEqual(OmcpGatt.manufacturerId, 0xFFFF)
        XCTAssertTrue(src.contains("const val LOCAL_NAME = \"\(OmcpGatt.localName)\""))
    }

    func testFileLimitFromCore() throws {
        let (r, core) = makeRepo()
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let c = try r.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        XCTAssertEqual(FilePolicy.check(size: 50_000_000, core: core), .ok)
        XCTAssertEqual(FilePolicy.check(size: 50_000_001, core: core), .tooLarge(max: 50_000_000))
        XCTAssertEqual(FilePolicy.label(core.maxFileBytes()), "50 MB")
        core.maxFile = 10 // UI follows the core, not a constant
        XCTAssertThrowsError(try r.queueFile(c.id, name: "a", size: 11, mime: "a/b"))
    }

    func testGatewayFromCorePolicy() {
        let (r, core) = makeRepo()
        let s = r.store
        XCTAssertFalse(GatewayPolicy.state(store: s, core: core, network: .wifi, metered: false).decision.bridge)
        s.gatewayEnabled = true
        let st = GatewayPolicy.state(store: s, core: core, network: .cellular, metered: true)
        XCTAssertFalse(st.decision.bridge, "Wi-Fi only by default")
        XCTAssertEqual(st.usedBytes, 0)
        XCTAssertEqual(st.dailyCapBytes, 500_000_000)
        s.gatewayAllowMobile = true
        let m = GatewayPolicy.state(store: s, core: core, network: .cellular, metered: true).decision
        XCTAssertTrue(m.bridge); XCTAssertTrue(m.meteredWarning)
    }

    /// Verified badge only on verified contacts.
    func testContactAddVerifyOrNot() throws {
        let (r, core) = makeRepo()
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let v = try r.addContact(qr: "omcp:1:aaa:Ann", action: .verifyAndChat)
        let u = try r.addContact(qr: "omcp:1:bbb:Ben", action: .chatWithoutVerifying)
        XCTAssertTrue(v.showsVerifiedBadge)
        XCTAssertFalse(u.showsVerifiedBadge)
        let pre = try core.previewContact("omcp:1:ccc:Ann")
        XCTAssertEqual(pre.mismatchWith?.id, "aaa")
        XCTAssertEqual(pre.contact.safetyNumber.count, 12)
        XCTAssertThrowsError(try core.previewContact("https://evil"))
    }

    func testRetryLimiterBackoffCaps() {
        let clock = Clock()
        let l = RetryLimiter(clock: clock.fn)
        for _ in 0..<20 { l.onFailure() }
        XCTAssertEqual(l.remainingLockMs(), RetryLimiter.maxMs)
        l.onSuccess()
        XCTAssertTrue(l.canTry)
    }
}
