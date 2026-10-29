import Foundation

/// Deterministic in-memory core for SwiftUI previews and unit tests. NOT crypto: backups are
/// XOR-"sealed" only so tests can check plumbing; the real sealing is in Rust (tested there).
public final class FakeOmcpCore: OmcpCore {
    public let linked = false
    public let suiteLabel = "OMCP-2026-A (fake)"
    private var me: Identity?
    private var phrase: String?
    private var book: [String: Contact] = [:]
    private var order: [String] = []
    private var queued: [String: Int] = [:]
    private var burns: [String: (text: String, mime: String?, live: Bool, opened: Bool, spec: BurnSpec)] = [:]
    private var counter = 0
    public var maxFile: UInt64 = 50_000_000
    /// Deterministic store-open failure injection for repository tests.
    public var storageOpenError: CoreError?
    public private(set) var storageOpenCount = 0

    @discardableResult
    public func open(phrase: String, displayName: String, dbPath: String, storageKey: Data) throws -> Identity {
        storageOpenCount += 1
        if let error = storageOpenError { throw error }
        return try load(phrase: phrase, displayName: displayName)
    }

    /// Test-only receipt model; no cryptographic claim. Production uses Rust.
    public var repairValidationError: CoreError?
    public var receiptCommitError: CoreError?
    public var receiptCancelError: CoreError?
    public var receiptRetryError: CoreError?
    public var receiptCleanupAckError: CoreError?
    public var recoverableReceiptError: CoreError?
    public var cancelledRequestError: CoreError?
    public private(set) var cancelledRequestCount = 0
    private var cleanedCancelledReceipts: [String: ReceiveReceiptInfo] = [:]
    private var cancelledReceipts: [String: ReceiveReceiptInfo] = [:]
    public var cancelPreservingReceived = false
    public var receiptEnumerationError: CoreError?
    private var receiptPayloads: [String: (ReceiveReceiptInfo, Data)] = [:]
    private var receiptRows: [String: StoredMessageInfo] = [:]
    public func injectReceipt(_ receipt: ReceiveReceiptInfo, bytes: Data, mine: Bool = false) {
        receiptPayloads[receipt.receiptToken] = (receipt, bytes)
        receiptRows[receipt.transferId] = StoredMessageInfo(id: receipt.transferId, contactId: receipt.contactId, text: "", mine: mine,
            timeMs: 1, status: .received, mime: nil, burn: nil, burnState: .none, burnAtMs: nil,
            file: StoredFileInfo(name: receipt.name, mime: receipt.mime, size: receipt.size, state: .finalizing, doneBytes: receipt.size, localPath: nil))
    }
    public func storedMessages(_ contactId: String) -> [StoredMessageInfo] { receiptRows.values.filter { $0.contactId == contactId }.sorted { $0.id < $1.id } }
    public func pendingReceivedFiles() throws -> [ReceiveReceiptInfo] {
        if let error = receiptEnumerationError { throw error }
        return receiptPayloads.values.map { $0.0 }.sorted { $0.transferId < $1.transferId }
    }
    public func cancelledReceivedFiles() throws -> [ReceiveReceiptInfo] { Array(cancelledReceipts.values.prefix(32)) }
    public func acknowledgeReceivedFileCleanup(_ receiptToken: String) throws -> Bool {
        if let error = receiptCleanupAckError { throw error }
        guard let receipt = cancelledReceipts.removeValue(forKey: receiptToken) else { return false }
        cleanedCancelledReceipts[receiptToken] = receipt
        return true
    }
    public func recoverableReceivedFile(_ contactId: String, msgId: String) throws -> ReceiveReceiptInfo? {
        if let error = recoverableReceiptError { throw error }
        guard book[contactId] != nil, receiptRows[msgId]?.file?.state == .failed else { return nil }
        return cleanedCancelledReceipts.values.first { $0.contactId == contactId && $0.transferId == msgId }
    }
    public func requestCancelledReceivedFile(_ receipt: ReceiveReceiptInfo) throws {
        if let error = cancelledRequestError { throw error }
        guard try recoverableReceivedFile(receipt.contactId, msgId: receipt.transferId) == receipt,
              var row = receiptRows[receipt.transferId] else { throw CoreError.invalid("stale cancelled receipt") }
        cleanedCancelledReceipts.removeValue(forKey: receipt.receiptToken)
        row.file?.state = .requested; receiptRows[receipt.transferId] = row
        cancelledRequestCount += 1
    }
    public func retryReceivedFiles() throws -> [ReceiveReceiptInfo] {
        if let error = receiptRetryError { throw error }
        return try pendingReceivedFiles()
    }
    public func readReceivedFileBlock(_ receiptToken: String, index: UInt32) throws -> Data {
        guard let (r, bytes) = receiptPayloads[receiptToken], index < r.blockCount else { throw CoreError.invalid("stale receipt") }
        let start = Int(index) * 65_536
        return bytes.subdata(in: start..<min(bytes.count, start + 65_536))
    }
    public func validateReceivedFileForRepair(_ receiptToken: String) throws -> ReceiveReceiptInfo {
        if let error = repairValidationError { throw error }
        guard let (receipt, _) = receiptPayloads[receiptToken] else { throw CoreError.invalid("stale repair token") }
        return receipt
    }
    public func receivedFileVerifier(_ receiptToken: String) throws -> ReceiveFileVerifier {
        guard let (_, bytes) = receiptPayloads[receiptToken] else { throw CoreError.invalid("stale receipt") }
        return FakeReceiptVerifier(bytes)
    }
    public func commitReceivedFile(_ receiptToken: String, path: String) throws {
        if let error = receiptCommitError { throw error }
        guard let (r, _) = receiptPayloads[receiptToken], var row = receiptRows[r.transferId] else { throw CoreError.invalid("stale receipt") }
        row.file?.state = .received; row.file?.localPath = path
        receiptRows[r.transferId] = row; receiptPayloads.removeValue(forKey: receiptToken)
    }
    public func markReceivedFileMissing(_ transferId: String, expectedPath: String) throws -> Bool {
        guard var row = receiptRows[transferId], row.file?.state == .received, row.file?.localPath == expectedPath else { return false }
        row.file?.state = .missing; row.file?.localPath = nil; row.file?.doneBytes = 0; receiptRows[transferId] = row
        return true
    }
    private final class FakeReceiptVerifier: ReceiveFileVerifier {
        private let expected: Data; private var position = 0; private var mismatch = false
        init(_ expected: Data) { self.expected = expected }
        func update(_ bytes: Data) throws {
            guard bytes.count <= expected.count - position else { throw CoreError.invalid("readback length") }
            mismatch = mismatch || expected.subdata(in: position..<(position + bytes.count)) != bytes
            position += bytes.count
        }
        func finish() throws {
            guard position == expected.count else { throw CoreError.invalid("readback length") }
            if mismatch { throw CoreError.localReceiveContentMismatch }
        }
    }

    public init() {}

    private static let words = (0..<2048).map { "w\(String($0, radix: 36))x" }

    public func generateRecoveryPhrase() throws -> String {
        counter += 1
        return (0..<24).map { Self.words[(counter * 97 + $0 * 31) % 2048] }.joined(separator: " ")
    }

    public func validateRecoveryPhrase(_ phrase: String) -> Bool {
        let w = phrase.split(separator: " ")
        return w.count == 24 && w.allSatisfy { Self.words.contains(String($0)) }
    }

    public func suggestRecoveryWords(_ prefix: String) -> [String] { Array(Self.words.filter { $0.hasPrefix(prefix) }.prefix(3)) }

    public func load(phrase: String, displayName: String) throws -> Identity {
        guard validateRecoveryPhrase(phrase) else { throw CoreError.invalid("phrase") }
        self.phrase = phrase
        let id = Identity(peerIdHex: String(UInt64(bitPattern: Int64(phrase.hashValue)), radix: 16), displayName: displayName)
        me = id
        return id
    }

    public func identity() -> Identity? { me }
    public func qrPayload() throws -> String {
        guard let me else { throw CoreError.notLoaded }
        return "omcp:1:\(me.peerIdHex):\(me.displayName)"
    }
    public func advertisement() throws -> Data { Data(repeating: 1, count: 13) }

    private func parse(_ payload: String) throws -> Contact {
        let p = payload.split(separator: ":", maxSplits: 3).map(String.init)
        guard p.count == 4, p[0] == "omcp", p[1] == "1", !p[2].isEmpty else { throw CoreError.invalid("qr") }
        let sn = (0..<12).map { String(format: "%05d", (abs(p[2].hashValue) / ($0 + 1)) % 100_000) }
        return Contact(id: p[2], displayName: p[3], safetyNumber: sn, verified: false)
    }

    public func previewContact(_ payload: String) throws -> ContactPreview {
        let c = try parse(payload)
        let mismatch = book.values.first { $0.displayName == c.displayName && $0.id != c.id }
        return ContactPreview(contact: c, alreadyKnown: book[c.id] != nil, mismatchWith: mismatch)
    }

    public func addContactFromQr(_ payload: String) throws -> Contact {
        let c = try parse(payload)
        if book[c.id] == nil { order.append(c.id); book[c.id] = c }
        return book[c.id]!
    }

    public func restoreContact(_ contact: Contact) throws {
        if book[contact.id] == nil { order.append(contact.id) }
        book[contact.id] = contact
    }

    public func contacts() -> [Contact] { order.compactMap { book[$0] } }
    public func setVerified(_ contactId: String, _ verified: Bool) throws {
        guard book[contactId] != nil else { throw CoreError.invalid("unknown contact") }
        book[contactId]!.verified = verified
    }
    public func contactForAdvertisement(_ adv: Data) -> String? { nil }

    public func queueMessage(_ contactId: String, text: String, burn: BurnSpec?) throws -> String {
        guard book[contactId] != nil else { throw CoreError.invalid("unknown contact") }
        counter += 1
        let id = String(format: "%032x", counter)
        queued[contactId, default: 0] += 1
        if let burn { burns[id] = (text, nil, true, false, burn) }
        return id
    }

    public func queueMedia(_ contactId: String, mime: String, bytes: Data, burn: BurnSpec?) throws -> String {
        let id = try queueMessage(contactId, text: "", burn: burn)
        if burn != nil { burns[id]?.mime = mime }
        return id
    }

    /// Test hook: simulate receiving a burn message.
    public func injectIncomingBurn(id: String, text: String, mime: String? = nil, spec: BurnSpec) {
        burns[id] = (text, mime, true, false, spec)
    }

    public func openBurn(_ msgId: String, nowMs: Int64) throws -> BurnContent? {
        guard let b = burns[msgId], b.live, !b.opened else { return nil }
        burns[msgId]!.opened = true
        let at: Int64? = b.spec.mode == .timer ? nowMs + Int64(b.spec.secs) * 1000 : nil
        return BurnContent(text: b.mime == nil ? b.text : "", bytes: Data(), mime: b.mime, burnAtMs: at)
    }

    public func closeBurn(_ msgId: String) throws { burns[msgId]?.live = false }
    public var pendingShred: [String] = []
    public func tickBurn(nowMs: Int64) -> [String] {
        defer { pendingShred = [] }
        pendingShred.forEach { burns[$0]?.live = false }
        return pendingShred
    }
    public func burnIsLive(_ msgId: String) -> Bool { burns[msgId]?.live ?? false }
    public func burnSpecValid(_ spec: BurnSpec) -> Bool {
        [BurnSpec.unread1h, BurnSpec.unread24h, BurnSpec.unread7d].contains(spec.unreadTtlSecs)
            && (spec.mode == .onOpen ? spec.secs == 0 : (1...3600).contains(spec.secs))
    }
    /// Fake: records the offer only (no crypto); chunking is Rust-only.
    public private(set) var files: Set<String> = []
    public func queueFile(_ contactId: String, name: String, mime: String, bytes: Data) throws -> FileOfferInfo {
        guard book[contactId] != nil else { throw CoreError.invalid("unknown contact") }
        guard UInt64(bytes.count) <= maxFile else { throw CoreError.fileTooLarge(size: UInt64(bytes.count), max: maxFile) }
        counter += 1
        let id = "file-\(counter)"
        files.insert(id)
        return FileOfferInfo(transferId: id, contactId: contactId, name: name, mime: mime, size: UInt64(bytes.count),
                             chunks: max(1, (bytes.count + 65_535) / 65_536))
    }
    public func cancelFile(_ transferId: String) throws -> Bool {
        if let error = receiptCancelError { throw error }
        if cancelPreservingReceived { return true }
        if let token = receiptPayloads.first(where: { $0.value.0.transferId == transferId })?.key {
            cancelledReceipts[token] = receiptPayloads[token]?.0
            receiptPayloads.removeValue(forKey: token)
            if var row = receiptRows[transferId], var file = row.file {
                file.state = .failed; file.localPath = nil; file.doneBytes = 0
                row.file = file; receiptRows[transferId] = row
            }
            return true
        }
        return files.remove(transferId) != nil
    }
    public func queuedFor(_ contactId: String) -> Int { queued[contactId] ?? 0 }
    public func pathStatus() -> MeshStatus {
        let q = queued.values.reduce(0, +)
        return MeshStatus(nearbyPeers: 0, queuedCount: q, aggregatePath: .queued)
    }

    public func openInitiatorLink(_ contactId: String) throws -> CoreLink { throw CoreError.other("fake core has no links") }
    public func openResponderLink() throws -> CoreLink { throw CoreError.other("fake core has no links") }

    public func checkBackupPassword(_ password: String) -> PasswordCheck {
        let tooShort = password.count < 8
        let score = tooShort ? 0 : min(4, Set(password).count / 3)
        return PasswordCheck(ok: !tooShort && score >= 3, score: score, tooShort: tooShort)
    }

    private func xor(_ d: Data, _ k: Data) -> Data { Data(d.enumerated().map { $0.element ^ k[$0.offset % k.count] }) }
    private func payload() throws -> Data {
        guard let phrase else { throw CoreError.notLoaded }
        let cs = contacts().map { "\($0.id)|\($0.displayName)|\($0.verified)" }.joined(separator: "\n")
        return Data((phrase + "\n" + cs).utf8)
    }

    public func createBackup(password: String, deviceHint: String, nowMs: Int64) throws -> Data {
        Data("P".utf8) + xor(try payload(), Data(("k" + password).utf8))
    }
    public func createBackup(key: Data, deviceHint: String, nowMs: Int64) throws -> Data {
        guard key.count == 32 else { throw CoreError.invalid("key") }
        return Data("K".utf8) + xor(try payload(), key)
    }
    public func generateBackupKey() throws -> Data { Data((0..<32).map { _ in UInt8.random(in: 1...255) }) }
    public func backupInfo(_ blob: Data) -> BackupMeta? {
        guard let f = blob.first else { return nil }
        return BackupMeta(createdAtMs: 0, deviceHint: "fake", needsPassword: f == UInt8(ascii: "P"))
    }
    private func open(_ blob: Data, _ k: Data) throws -> RestoredBackup {
        let s = String(decoding: xor(blob.dropFirst(), k), as: UTF8.self).split(separator: "\n").map(String.init)
        guard let phrase = s.first, validateRecoveryPhrase(phrase) else { throw CoreError.wrongBackupPassword }
        let cs = s.dropFirst().map { line -> Contact in
            let p = line.split(separator: "|").map(String.init)
            return Contact(id: p[0], displayName: p[1], safetyNumber: [], verified: p[2] == "true")
        }
        return RestoredBackup(phrase: phrase, contacts: cs)
    }
    public func openBackup(_ blob: Data, password: String) throws -> RestoredBackup {
        guard blob.first == UInt8(ascii: "P") else { throw CoreError.invalid("not a password backup") }
        return try open(blob, Data(("k" + password).utf8))
    }
    public func openBackup(_ blob: Data, key: Data) throws -> RestoredBackup {
        guard blob.first == UInt8(ascii: "K"), key.count == 32 else { throw CoreError.invalid("not a key backup") }
        return try open(blob, key)
    }
    public func recoveryPhrase() throws -> String {
        guard let phrase else { throw CoreError.notLoaded }
        return phrase
    }

    public func maxFileBytes() -> UInt64 { maxFile }
    public func gatewayDailyCapBytes() -> UInt64 { 500_000_000 }
    public func gatewayDecision(wifiOnly: Bool, network: NetworkType, metered: Bool) -> GatewayDecision {
        switch network {
        case .offline: return GatewayDecision(bridge: false, meteredWarning: false)
        case .wifi: return GatewayDecision(bridge: true, meteredWarning: metered)
        case .cellular: return wifiOnly ? GatewayDecision(bridge: false, meteredWarning: false) : GatewayDecision(bridge: true, meteredWarning: metered)
        }
    }
}
