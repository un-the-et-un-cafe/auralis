import HoplaKit
import UserNotifications

/// Notification Service Extension for content-free APNs wake-ups (N3). APNs carries only the
/// opaque mailbox id (`PushPayload`); Apple never sees the sender or text.
///
/// STUB (M6): fetching the mailbox from the gateway and decrypting with the Rust core needs the
/// identity shared through an App Group keychain and the gateway mailbox API (M4). Until then
/// the extension validates the payload and shows the generic "Hopla · New message", which is
/// also exactly what Private notifications show. Malformed payloads (anything beyond the mailbox
/// id and the fixed generic alert) are suppressed to the same generic text.
final class NotificationService: UNNotificationServiceExtension {
    private var handler: ((UNNotificationContent) -> Void)?
    private var best: UNMutableNotificationContent?

    override func didReceive(_ request: UNNotificationRequest, withContentHandler contentHandler: @escaping (UNNotificationContent) -> Void) {
        handler = contentHandler
        let content = UNMutableNotificationContent()
        content.title = PushPayload.fallbackTitle
        content.body = NSLocalizedString("notif_new_message", comment: "")
        content.threadIdentifier = "hopla"
        content.categoryIdentifier = "omcp.message.hidden"
        content.sound = .default
        best = content
        // TODO(M4): if let p = PushPayload.from(userInfo: request.content.userInfo) { fetch p.mailboxId,
        // decrypt via omcp-ffi, apply NotificationPolicy (sender + preview unless Private/burn). }
        _ = PushPayload.from(userInfo: request.content.userInfo)
        contentHandler(content)
    }

    override func serviceExtensionTimeWillExpire() {
        if let handler, let best { handler(best) }
    }
}
