import XCTest
@testable import HoplaKit

final class NotificationTests: XCTestCase {
    /// N1: default shows sender + preview.
    func testN1_defaultSenderAndPreview() throws {
        let (r, _) = makeRepo()
        let n = RecordingNotifier()
        r.notifier = n
        try r.saveIdentity(phrase: try r.core.generateRecoveryPhrase(), name: "A")
        let c = try r.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        r.onReceived(IncomingText(contactId: c.id, text: "hi there", createdMs: 1, envelopeId: "e1"))
        XCTAssertEqual(n.posted.first?.title, "Bob")
        XCTAssertEqual(n.posted.first?.body, "hi there")
        XCTAssertEqual(n.posted.first?.showAvatar, true)
    }

    /// N2: Private (global) → no sender/content anywhere, including the lock-screen placeholder.
    func testN2_privateGlobalAndPerChatOverrideBothWays() {
        let s = InMemoryAppStore()
        s.privateNotifications = true
        s.hideOnLockScreen = true
        let p = NotificationPolicy.content(s, chatId: "c1", sender: "Bob", text: "secret", genericBody: "New message")
        XCTAssertEqual(p.title, "Hopla")
        XCTAssertEqual(p.body, "New message")
        XCTAssertFalse(p.showAvatar)
        XCTAssertEqual(p.threadId, "hopla")
        for field in [p.title, p.body, p.hiddenPreviewsPlaceholder, p.threadId] {
            XCTAssertFalse(field.contains("Bob")); XCTAssertFalse(field.contains("secret"))
        }
        // Per chat: off even though global is on.
        NotificationPolicy.setChatPrivate(s, chatId: "c1", false)
        XCTAssertEqual(NotificationPolicy.content(s, chatId: "c1", sender: "Bob", text: "x", genericBody: "New message").title, "Bob")
        // Per chat: on even though global is off.
        s.privateNotifications = false
        NotificationPolicy.setChatPrivate(s, chatId: "c2", true)
        XCTAssertTrue(NotificationPolicy.content(s, chatId: "c2", sender: "Bob", text: "x", genericBody: "New message").isPrivate)
        NotificationPolicy.setChatPrivate(s, chatId: "c2", nil)
        XCTAssertFalse(NotificationPolicy.isPrivate(s, chatId: "c2"))
    }

    func testPrivateCallShowsIncomingCall() {
        let s = InMemoryAppStore()
        s.privateNotifications = true
        let c = NotificationPolicy.callContent(s, chatId: "c", caller: "Bob", genericCall: "Incoming call")
        XCTAssertEqual(c.title, "Hopla"); XCTAssertEqual(c.body, "Incoming call")
        s.privateNotifications = false
        XCTAssertEqual(NotificationPolicy.callContent(s, chatId: "c", caller: "Bob", genericCall: "Incoming call").title, "Bob")
    }

    /// N3: the APNs payload is content-free; anything else is rejected.
    func testN3_apnsPayloadHasNoPlaintext() throws {
        let p = PushPayload(mailboxId: "mb_123-abc")
        let json = try JSONSerialization.data(withJSONObject: p.apnsPayload(), options: [.sortedKeys])
        XCTAssertEqual(String(decoding: json, as: UTF8.self),
                       #"{"aps":{"alert":{"body":"New message","title":"Hopla"},"mutable-content":1},"mb":"mb_123-abc"}"#)
        let back = try JSONSerialization.jsonObject(with: json) as! [AnyHashable: Any]
        XCTAssertEqual(PushPayload.from(userInfo: back), p)
        XCTAssertNil(PushPayload.from(userInfo: ["mb": "x", "sender": "Bob"]))
        XCTAssertNil(PushPayload.from(userInfo: ["mb": "x", "aps": ["alert": ["title": "Bob", "body": "hi"]]]))
        XCTAssertNil(PushPayload.from(userInfo: ["mb": "has space"]))
        XCTAssertNil(PushPayload.from(userInfo: ["mb": String(repeating: "a", count: 129)]))
    }
}
