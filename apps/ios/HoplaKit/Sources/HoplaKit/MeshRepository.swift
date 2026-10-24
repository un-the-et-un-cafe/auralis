import Foundation

public struct FileAttachment: Equatable, Sendable {
    public enum State: Sendable { case queued, sending, receiving, paused, received, failed, integrityFailed }
    public var name: String
    public var sizeBytes: UInt64
    public var mime: String
    public var state: State
}

public struct ChatMessage: Equatable, Identifiable, Sendable {
    public enum Status: Sendable { case queued, sent, received }
    public enum BurnState: Sendable { case none, unopened, open, opened }
    public var id: String
    public var contactId: String
    /// Always empty for burn messages: their content lives only in the Rust shredding store.
    public var text: String
    public var mine: Bool
    public var timeMs: Int64
    public var status: Status
    public var file: FileAttachment?
    public var burn: BurnSpec?
    public var burnState: BurnState = .none
    public var burnAtMs: Int64?

    public var isViewOnceMedia: Bool {
        guard burn != nil, let f = file else { return false }
        return f.mime.hasPrefix("image/") || f.mime.hasPrefix("video/")
    }
    /// Burnable content can't be saved, forwarded, quoted, shared or searched (B2/B5).
    public var canShare: Bool { burn == nil }
}

/// Something that raises local notifications (UNUserNotificationCenter on iOS).
public protocol Notifier: AnyObject {
    func post(_ content: NotificationContent, id: String)
}

/// Something that can push queued messages out now (the BLE mesh transport).
public protocol MeshKick: AnyObject {
    func kick(_ contactId: String)
}

/// App state + core glue, the iOS twin of Android `MeshRepository`. Not thread-safe:
/// call from the main actor.
public final class MeshRepository {
    public let core: OmcpCore
    public let store: AppStore
    public weak var kick: MeshKick?
    public var notifier: Notifier?
    public var onChange: (() -> Void)?
    public var genericBody = "New message"
    private let clock: () -> Int64
    public private(set) var messages: [String: [ChatMessage]] = [:]
    private var seq = 0

    public init(core: OmcpCore, store: AppStore, clock: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }) {
        self.core = core
        self.store = store
        self.clock = clock
    }

    public func now() -> Int64 { clock() }

    /// Load identity + contacts from the store on launch. Returns false if there is no identity yet.
    @discardableResult public func restore() -> Bool {
        guard let phrase = store.recoveryPhrase else { return false }
        guard (try? core.load(phrase: phrase, displayName: store.displayName ?? "Me")) != nil else { return false }
        for c in store.contacts { try? core.restoreContact(c) }
        return true
    }

    public func saveIdentity(phrase: String, name: String) throws {
        try core.load(phrase: phrase, displayName: name)
        store.recoveryPhrase = phrase
        store.displayName = name
    }

    public func restoreFromBackup(_ r: RestoredBackup, name: String, method: String) throws {
        try saveIdentity(phrase: r.phrase, name: name)
        for c in r.contacts { try core.restoreContact(c) }
        store.backupMethods.insert(method)
        persistContacts()
    }

    public var contacts: [Contact] { core.contacts() }
    public func contact(_ id: String) -> Contact? { contacts.first { $0.id == id } }

    public func backupGate() -> BackupGate { HoplaKit.backupGate(store, now: clock()) }

    public func remindBackupLater() {
        if store.backupRemindDeadlineMs == 0 { store.backupRemindDeadlineMs = clock() + remindWindowMs }
    }

    public func markBackupMethod(_ m: String, add: Bool = true) {
        if add { store.backupMethods.insert(m) } else { store.backupMethods.remove(m) }
        changed()
    }

    public func completeOnboarding() {
        store.onboarded = true
        changed()
    }

    // MARK: contacts

    /// "Verify & chat" → verified = true; "Chat without verifying" → false (no badge).
    @discardableResult
    public func addContact(qr: String, action: ContactAddAction) throws -> Contact {
        let c = try core.addContactFromQr(qr)
        if action == .verifyAndChat { try core.setVerified(c.id, true) }
        persistContacts()
        return contact(c.id) ?? c
    }

    public func setVerified(_ id: String, _ v: Bool) throws {
        try core.setVerified(id, v)
        persistContacts()
    }

    private func persistContacts() {
        store.contacts = core.contacts()
        changed()
    }

    // MARK: burn settings

    public func chatBurn(_ contactId: String) -> BurnSpec? {
        BurnSpec.fromOption(store.burnChats[contactId] ?? -1, ttl: store.burnUnreadTtl[contactId] ?? BurnSpec.unread7d)
    }

    public func setChatBurn(_ contactId: String, option: Int, unreadTtl: Int = BurnSpec.unread7d) throws {
        if let s = BurnSpec.fromOption(option, ttl: unreadTtl), !core.burnSpecValid(s) { throw CoreError.invalid("burn option") }
        store.burnChats[contactId] = option == -1 ? nil : option
        store.burnUnreadTtl[contactId] = unreadTtl
        changed()
    }

    // MARK: messages

    /// Send text. `burnOnce` = the composer's per-message 🔥 toggle; otherwise the chat default.
    public func send(_ contactId: String, text: String, burnOnce: BurnSpec? = nil) throws {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty else { return }
        let burn = burnOnce ?? chatBurn(contactId)
        let id = try core.queueMessage(contactId, text: t, burn: burn)
        append(ChatMessage(id: id, contactId: contactId, text: burn == nil ? t : "", mine: true, timeMs: clock(),
                           status: .queued, burn: burn, burnState: burn == nil ? .none : .unopened))
        kick?.kick(contactId)
    }

    /// Record an outgoing file after checking the core's cap (50 MB).
    public func queueFile(_ contactId: String, name: String, size: UInt64, mime: String) throws {
        if case .tooLarge(let max) = FilePolicy.check(size: size, core: core) {
            throw CoreError.fileTooLarge(size: size, max: max)
        }
        seq += 1
        append(ChatMessage(id: "file-\(seq)", contactId: contactId, text: "", mine: true, timeMs: clock(), status: .queued,
                           file: FileAttachment(name: name, sizeBytes: size, mime: mime, state: .queued)))
    }

    public func markFlushed(_ contactId: String, count: Int) {
        guard count > 0 else { return }
        var left = count
        messages[contactId] = messages[contactId]?.map { m in
            guard left > 0, m.mine, m.file == nil, m.status == .queued else { return m }
            left -= 1
            var n = m
            n.status = .sent
            return n
        }
        changed()
    }

    /// Mesh/local delivery: store and notify on-device (sender + preview unless private/burn).
    public func onReceived(_ msg: IncomingText) {
        let file = msg.mime.map { FileAttachment(name: $0.hasPrefix("video/") ? "video" : "photo", sizeBytes: 0, mime: $0, state: .received) }
        append(ChatMessage(id: msg.envelopeId, contactId: msg.contactId, text: msg.burn == nil ? msg.text : "", mine: false,
                           timeMs: msg.createdMs, status: .received, file: file, burn: msg.burn,
                           burnState: msg.burn == nil ? .none : .unopened))
        let sender = contact(msg.contactId)?.displayName ?? "?"
        let content = NotificationPolicy.content(store, chatId: msg.contactId, sender: sender, text: msg.text,
                                                 genericBody: genericBody, burn: msg.burn != nil)
        notifier?.post(content, id: msg.envelopeId)
    }

    public func openBurn(_ contactId: String, msgId: String) throws -> BurnContent? {
        guard let m = find(contactId, msgId), m.burn != nil, !m.mine, m.burnState == .unopened else { return nil }
        guard let c = try core.openBurn(msgId, nowMs: clock()) else { return nil }
        var n = m
        n.burnState = .open
        n.burnAtMs = c.burnAtMs
        replace(n)
        kick?.kick(contactId) // deliver the OPENED receipt
        return c
    }

    public func closeBurn(_ contactId: String, msgId: String) throws {
        guard let m = find(contactId, msgId), m.burnState == .open else { return }
        try core.closeBurn(msgId)
        burnNow(m)
    }

    public func onOpened(_ o: OpenedInfo) {
        guard let m = find(o.contactId, o.msgId) else { return }
        if o.shredded { burnNow(m) } else {
            var n = m
            n.burnState = .open
            n.burnAtMs = o.burnAtMs
            replace(n)
        }
    }

    /// Timers + unread expiry, decided in Rust. Call every ≤ 250 ms. Returns how many burned.
    @discardableResult public func tickBurn() -> Int {
        let ids = Set(core.tickBurn(nowMs: clock()))
        let due = messages.values.flatMap { $0 }.filter { ids.contains($0.id) && $0.burnState != .opened }
        due.forEach(burnNow)
        return due.count
    }

    /// Search index: burnable content is never indexed (B5).
    public func search(_ q: String) -> [ChatMessage] {
        let q = q.lowercased()
        guard !q.isEmpty else { return [] }
        return messages.values.flatMap { $0 }.filter { $0.burn == nil && $0.text.lowercased().contains(q) }
    }

    private func burnNow(_ m: ChatMessage) {
        if m.isViewOnceMedia {
            var n = m
            n.burnState = .opened // "🔥 Photo opened"
            n.burnAtMs = nil
            replace(n)
        } else {
            messages[m.contactId]?.removeAll { $0.id == m.id }
            changed()
        }
    }

    private func find(_ contactId: String, _ id: String) -> ChatMessage? { messages[contactId]?.first { $0.id == id } }

    private func replace(_ m: ChatMessage) {
        messages[m.contactId] = messages[m.contactId]?.map { $0.id == m.id ? m : $0 }
        changed()
    }

    private func append(_ m: ChatMessage) {
        messages[m.contactId, default: []].append(m)
        changed()
    }

    private func changed() { onChange?() }
}
