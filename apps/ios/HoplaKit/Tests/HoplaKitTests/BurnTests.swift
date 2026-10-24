import XCTest
@testable import HoplaKit

final class BurnTests: XCTestCase {
    func setup() throws -> (MeshRepository, FakeOmcpCore, Contact, RecordingNotifier) {
        let (r, core) = makeRepo()
        let n = RecordingNotifier()
        r.notifier = n
        try r.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "A")
        let c = try r.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        return (r, core, c, n)
    }

    /// B2: view-once image opens exactly once; then "Photo opened" placeholder, no sharing.
    func testB2_viewOnceOpensOnce() throws {
        let (r, core, c, _) = try setup()
        core.injectIncomingBurn(id: "m1", text: "", mime: "image/jpeg", spec: .onOpen())
        r.onReceived(IncomingText(contactId: c.id, text: "", createdMs: 1, envelopeId: "m1", burn: .onOpen(), mime: "image/jpeg"))
        let m = r.messages[c.id]![0]
        XCTAssertTrue(m.isViewOnceMedia); XCTAssertFalse(m.canShare)
        XCTAssertNotNil(try r.openBurn(c.id, msgId: "m1"))
        XCTAssertNil(try r.openBurn(c.id, msgId: "m1"))
        try r.closeBurn(c.id, msgId: "m1")
        XCTAssertEqual(r.messages[c.id]![0].burnState, .opened)
        XCTAssertNil(try r.openBurn(c.id, msgId: "m1"))
    }

    /// B1 (UI side): when Rust reports a shred, the text disappears from the list.
    func testB1_tickRemovesShreddedText() throws {
        let (r, core, c, _) = try setup()
        try r.send(c.id, text: "bye", burnOnce: .timer(10))
        let id = r.messages[c.id]![0].id
        XCTAssertEqual(r.messages[c.id]![0].text, "", "burn text never stored in the UI model")
        core.pendingShred = [id]
        XCTAssertEqual(r.tickBurn(), 1)
        XCTAssertTrue(r.messages[c.id]!.isEmpty)
    }

    /// B5: burn content is excluded from notifications and search.
    func testB5_excludedFromNotificationsAndSearch() throws {
        let (r, _, c, n) = try setup()
        r.onReceived(IncomingText(contactId: c.id, text: "", createdMs: 1, envelopeId: "m2", burn: .onOpen()))
        r.onReceived(IncomingText(contactId: c.id, text: "searchable", createdMs: 2, envelopeId: "m3"))
        XCTAssertEqual(n.posted[0].title, "Hopla"); XCTAssertEqual(n.posted[0].body, "New message")
        XCTAssertEqual(r.search("search").map(\.id), ["m3"])
        try r.send(c.id, text: "hidden burn", burnOnce: .onOpen())
        XCTAssertTrue(r.search("hidden").isEmpty)
    }

    func testChatBurnOptionsValidatedByCore() throws {
        let (r, _, c, _) = try setup()
        for o in BurnSpec.chatOptions { try r.setChatBurn(c.id, option: o) }
        XCTAssertNil(r.chatBurn(c.id))
        try r.setChatBurn(c.id, option: 60, unreadTtl: BurnSpec.unread1h)
        XCTAssertEqual(r.chatBurn(c.id), .timer(60, ttl: BurnSpec.unread1h))
        XCTAssertThrowsError(try r.setChatBurn(c.id, option: 60, unreadTtl: 5))
    }
}
