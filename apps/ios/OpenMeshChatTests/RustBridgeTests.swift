import Foundation
import XCTest
import HoplaKit
#if canImport(HoplaRust)
import HoplaRust // Linux: scripts/verify-linux.sh
#else
@testable import OpenMeshChat // iOS simulator (xcodebuild test)
#endif

/// Real Rust core through the Swift bindings: two nodes, QR add with safety numbers, a Noise
/// link over in-memory "BLE", burn after read with OPENED, and iCloud/Drive backups (O2–O4).
final class RustBridgeTests: XCTestCase {
    func node(_ name: String) throws -> (RustOmcpCore, MeshRepository) {
        let core = RustOmcpCore()
        let repo = MeshRepository(core: core, store: InMemoryAppStore())
        try repo.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: name)
        return (core, repo)
    }

    /// Feed PDUs back and forth until both sides are quiet; collect what each side received.
    func pump(_ a: CoreLink, _ b: CoreLink, _ first: LinkResult, into ra: MeshRepository, _ rb: MeshRepository) throws {
        var queue: [(toB: Bool, pdus: [Data])] = [(true, first.outgoing)]
        while !queue.isEmpty {
            let (toB, pdus) = queue.removeFirst()
            for p in pdus {
                let out = try (toB ? b : a).onPdu(p)
                let repo = toB ? rb : ra
                out.received.forEach(repo.onReceived)
                out.opened.forEach(repo.onOpened)
                if !out.outgoing.isEmpty { queue.append((!toB, out.outgoing)) }
            }
        }
    }

    func testLinkedAndPolicyFromCore() {
        let c = RustOmcpCore()
        XCTAssertTrue(c.linked)
        XCTAssertEqual(c.maxFileBytes(), 50_000_000)
        XCTAssertEqual(c.gatewayDailyCapBytes(), 500_000_000)
        XCTAssertFalse(c.gatewayDecision(wifiOnly: true, network: .cellular, metered: true).bridge)
        XCTAssertTrue(c.gatewayDecision(wifiOnly: false, network: .cellular, metered: true).meteredWarning)
    }

    func testQrSafetyNumbersAndChatAndBurn() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let pa = try ca.previewContact(try cb.qrPayload())
        XCTAssertEqual(pa.contact.safetyNumber.count, 12)
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .verifyAndChat)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .chatWithoutVerifying)
        XCTAssertEqual(bOnA.safetyNumber, aOnB.safetyNumber)
        XCTAssertTrue(bOnA.showsVerifiedBadge)
        XCTAssertFalse(aOnB.showsVerifiedBadge)

        try ra.send(bOnA.id, text: "hello")
        try ra.send(bOnA.id, text: "burn me", burnOnce: .onOpen())
        let la = try ca.openInitiatorLink(bOnA.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        let got = rb.messages[aOnB.id] ?? []
        XCTAssertEqual(got.map(\.text), ["hello", ""])
        let burnId = got[1].id
        XCTAssertNotNil(got[1].burn)
        XCTAssertTrue(rb.search("burn").isEmpty)

        let opened = try rb.openBurn(aOnB.id, msgId: burnId)
        XCTAssertEqual(opened?.text, "burn me")
        XCTAssertNil(try rb.openBurn(aOnB.id, msgId: burnId), "content only once")
        try rb.closeBurn(aOnB.id, msgId: burnId)
        XCTAssertFalse(cb.burnIsLive(burnId))
        // OPENED receipt flows back and shreds the sender copy.
        try pump(lb, la, try lb.flush(), into: rb, ra)
        XCTAssertFalse(ca.burnIsLive(burnId))
    }

    /// O2 + O3 + O4 with the real Argon2id / XChaCha20-Poly1305 sealing.
    func testBackupsThroughOnboarding() throws {
        let (core, repo) = try node("Ana")
        _ = try repo.addContact(qr: try RustOmcpCore().qrPayloadFor(name: "Ben"), action: .verifyAndChat)
        let phrase = try core.recoveryPhrase()
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = InMemoryBackupKeyStore()
        let settings = BackupSettings(repo: repo, stores: [icloud], keyStore: keys)
        try settings.backUp(backupICloud, password: nil)
        let blob = try XCTUnwrap(icloud.blob)
        XCTAssertNil(blob.range(of: Data(phrase.utf8)), "O2 phrase")
        XCTAssertNil(blob.range(of: try XCTUnwrap(keys.key)), "O2 key")

        // O3: reinstall, restore from iCloud with the Keychain key.
        let core2 = RustOmcpCore()
        let repo2 = MeshRepository(core: core2, store: InMemoryAppStore())
        let f = OnboardingFlow(repo: repo2, iCloud: icloud, drive: nil, keyStore: keys)
        f.detectBackup()
        XCTAssertEqual(f.state.step, .restorePrompt)
        XCTAssertTrue(f.restoreFromCloud())
        XCTAssertEqual(core2.identity()?.peerIdHex, core.identity()?.peerIdHex)
        XCTAssertEqual(repo2.contacts.map(\.verified), [true])

        // O4: password mode, wrong password fails cleanly.
        try settings.changePassword(backupICloud, to: "violet-orbit-71-tundra")
        let f2 = OnboardingFlow(repo: MeshRepository(core: RustOmcpCore(), store: InMemoryAppStore()),
                                iCloud: icloud, drive: nil, keyStore: keys)
        f2.detectBackup()
        XCTAssertEqual(f2.state.foundBackup?.needsPassword, true)
        f2.setRestorePassword("wrong-password-123")
        XCTAssertFalse(f2.restoreFromCloud())
        XCTAssertEqual(f2.state.error, "wrongpw")
        f2.setRestorePassword("violet-orbit-71-tundra")
        XCTAssertTrue(f2.restoreFromCloud())
    }
}

extension RustOmcpCore {
    /// A throwaway identity's QR, for tests.
    func qrPayloadFor(name: String) throws -> String {
        _ = try load(phrase: try generateRecoveryPhrase(), displayName: name)
        return try qrPayload()
    }
}
