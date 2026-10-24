/// Content-free APNs wake-up (REQUIREMENTS "Notifications (v0.1.2)" › Delivery). The payload
/// carries only the opaque mailbox id; the Notification Service Extension fetches and decrypts
/// on-device. Apple never sees the sender or text (N3).
///
/// APNs shape: `{"aps":{"mutable-content":1,"alert":{"title":"Hopla","body":"New message"}},"mb":"<id>"}`.
/// The alert is a fixed generic fallback shown only if the extension can't run in time.
public struct PushPayload: Equatable {
    public static let keyMailbox = "mb"
    public static let fallbackTitle = "Hopla"
    public static let fallbackBody = "New message"

    public let mailboxId: String
    public init(mailboxId: String) { self.mailboxId = mailboxId }

    /// The full APNs payload a gateway is allowed to send.
    public func apnsPayload() -> [String: Any] {
        [
            "aps": [
                "mutable-content": 1,
                "alert": ["title": Self.fallbackTitle, "body": Self.fallbackBody],
            ] as [String: Any],
            Self.keyMailbox: mailboxId,
        ]
    }

    public static func validMailboxId(_ id: String) -> Bool {
        !id.isEmpty && id.count <= 128
            && id.unicodeScalars.allSatisfy { ($0.isASCII && ($0.properties.isAlphabetic || ("0"..."9").contains($0))) || $0 == "-" || $0 == "_" }
    }

    /// Parse a received userInfo. Rejects any extra top-level key or any alert other than the
    /// fixed generic one, so a relay can never smuggle content through Apple.
    public static func from(userInfo: [AnyHashable: Any]) -> PushPayload? {
        let keys = Set(userInfo.keys.compactMap { $0 as? String })
        guard keys.count == userInfo.count, keys.isSubset(of: ["aps", keyMailbox]),
              let id = userInfo[keyMailbox] as? String, validMailboxId(id) else { return nil }
        if let aps = userInfo["aps"] {
            guard let aps = aps as? [String: Any] else { return nil }
            guard Set(aps.keys).isSubset(of: ["mutable-content", "alert", "content-available", "sound"]) else { return nil }
            if let alert = aps["alert"] {
                guard let a = alert as? [String: Any], a.count == 2,
                      a["title"] as? String == fallbackTitle, a["body"] as? String == fallbackBody else { return nil }
            }
        }
        return PushPayload(mailboxId: id)
    }
}
