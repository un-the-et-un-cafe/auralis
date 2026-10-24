import Foundation

/// Aggregate path for chips/banners: mesh / gateway / mixed / queued only.
public enum PathKind: String, Sendable { case mesh, gateway, mixed, queued }

public struct MeshStatus: Equatable, Sendable {
    public var nearbyPeers: Int
    public var queuedCount: Int
    public var aggregatePath: PathKind
    public init(nearbyPeers: Int, queuedCount: Int, aggregatePath: PathKind) {
        self.nearbyPeers = nearbyPeers
        self.queuedCount = queuedCount
        self.aggregatePath = aggregatePath
    }
}

public struct Identity: Equatable, Sendable {
    public var peerIdHex: String
    public var displayName: String
    public init(peerIdHex: String, displayName: String) {
        self.peerIdHex = peerIdHex
        self.displayName = displayName
    }
}

public struct Contact: Equatable, Identifiable, Codable, Sendable {
    public var id: String
    public var displayName: String
    /// 12 groups of 5 digits.
    public var safetyNumber: [String]
    public var verified: Bool
    public init(id: String, displayName: String, safetyNumber: [String], verified: Bool) {
        self.id = id
        self.displayName = displayName
        self.safetyNumber = safetyNumber
        self.verified = verified
    }

    /// Verified check badge only on verified contacts; unverified contacts show nothing.
    public var showsVerifiedBadge: Bool { verified }
}

public struct ContactPreview: Equatable, Sendable {
    public var contact: Contact
    public var alreadyKnown: Bool
    public var mismatchWith: Contact?
    public init(contact: Contact, alreadyKnown: Bool, mismatchWith: Contact?) {
        self.contact = contact
        self.alreadyKnown = alreadyKnown
        self.mismatchWith = mismatchWith
    }
}

/// UI-side value for the encrypted `burn` header (PROTOCOL §19.1). All burn rules live in Rust.
public struct BurnSpec: Equatable, Codable, Sendable {
    public enum Mode: String, Codable, Sendable { case onOpen, timer }
    public var mode: Mode
    public var secs: Int
    public var unreadTtlSecs: Int

    public static let unread1h = 3_600
    public static let unread24h = 86_400
    public static let unread7d = 604_800

    public init(mode: Mode, secs: Int, unreadTtlSecs: Int = BurnSpec.unread7d) {
        self.mode = mode
        self.secs = secs
        self.unreadTtlSecs = unreadTtlSecs
    }
    public static func onOpen(ttl: Int = unread7d) -> BurnSpec { BurnSpec(mode: .onOpen, secs: 0, unreadTtlSecs: ttl) }
    public static func timer(_ secs: Int, ttl: Int = unread7d) -> BurnSpec { BurnSpec(mode: .timer, secs: secs, unreadTtlSecs: ttl) }

    /// Per-chat option encoding (same as Android): -1 off, 0 on open, else timer secs.
    public static func fromOption(_ opt: Int, ttl: Int) -> BurnSpec? {
        switch opt {
        case -1: return nil
        case 0: return onOpen(ttl: ttl)
        default: return timer(opt, ttl: ttl)
        }
    }
    /// Chat info choices: on open, 10 s, 1 min, 1 h, off.
    public static let chatOptions = [0, 10, 60, 3_600, -1]
}

/// Burn content handed out exactly once by the core.
public struct BurnContent: Equatable, Sendable {
    public var text: String
    public var bytes: Data
    public var mime: String?
    public var burnAtMs: Int64?
    public init(text: String, bytes: Data, mime: String?, burnAtMs: Int64?) {
        self.text = text
        self.bytes = bytes
        self.mime = mime
        self.burnAtMs = burnAtMs
    }
}

/// Sender side: the peer opened our burn message (encrypted OPENED receipt).
public struct OpenedInfo: Equatable, Sendable {
    public var contactId: String
    public var msgId: String
    public var openedMs: Int64
    public var shredded: Bool
    public var burnAtMs: Int64?
    public init(contactId: String, msgId: String, openedMs: Int64, shredded: Bool, burnAtMs: Int64?) {
        self.contactId = contactId
        self.msgId = msgId
        self.openedMs = openedMs
        self.shredded = shredded
        self.burnAtMs = burnAtMs
    }
}

public struct IncomingText: Equatable, Sendable {
    public var contactId: String
    /// Empty for burn messages: content only via `OmcpCore.openBurn`.
    public var text: String
    public var createdMs: Int64
    public var envelopeId: String
    public var burn: BurnSpec?
    public var mime: String?
    public init(contactId: String, text: String, createdMs: Int64, envelopeId: String, burn: BurnSpec? = nil, mime: String? = nil) {
        self.contactId = contactId
        self.text = text
        self.createdMs = createdMs
        self.envelopeId = envelopeId
        self.burn = burn
        self.mime = mime
    }
}

/// A queued outbound file (encrypted into 64 KiB chunks by the Rust core).
public struct FileOfferInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var name: String
    public var mime: String
    public var size: UInt64
    public var chunks: Int
    public init(transferId: String, contactId: String, name: String, mime: String, size: UInt64, chunks: Int) {
        self.transferId = transferId
        self.contactId = contactId
        self.name = name
        self.mime = mime
        self.size = size
        self.chunks = chunks
    }
}

/// Transfer progress: sender = chunks sealed onto the link, receiver = chunks received.
public struct FileProgressInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var incoming: Bool
    public var name: String
    public var size: UInt64
    public var doneChunks: Int
    public var totalChunks: Int
    public init(transferId: String, contactId: String, incoming: Bool, name: String, size: UInt64, doneChunks: Int, totalChunks: Int) {
        self.transferId = transferId
        self.contactId = contactId
        self.incoming = incoming
        self.name = name
        self.size = size
        self.doneChunks = doneChunks
        self.totalChunks = totalChunks
    }
    public var fraction: Double { totalChunks == 0 ? 1 : Double(doneChunks) / Double(totalChunks) }
    public var doneBytes: UInt64 { UInt64(Double(size) * fraction) }
}

/// A fully received file whose BLAKE3 hash was verified in Rust.
public struct ReceivedFileInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var name: String
    public var mime: String
    public var bytes: Data
    public init(transferId: String, contactId: String, name: String, mime: String, bytes: Data) {
        self.transferId = transferId
        self.contactId = contactId
        self.name = name
        self.mime = mime
        self.bytes = bytes
    }
}

/// `integrity` (hash/AEAD failed), `invalid` or `busy`.
public struct FileFailureInfo: Equatable, Sendable {
    public var transferId: String
    public var contactId: String
    public var reason: String
    public init(transferId: String, contactId: String, reason: String) {
        self.transferId = transferId
        self.contactId = contactId
        self.reason = reason
    }
}

/// PDUs to transmit (in order) + decrypted messages from one link step.
public struct LinkResult: Sendable {
    public var outgoing: [Data]
    public var received: [IncomingText]
    public var established: Bool
    public var peerContactId: String?
    public var flushed: Int
    public var opened: [OpenedInfo]
    public var fileProgress: [FileProgressInfo]
    public var files: [ReceivedFileInfo]
    public var fileFailures: [FileFailureInfo]
    /// More file chunks wait: call `flush()` again once the BLE write/notify queue is empty.
    public var morePending: Bool
    public init(outgoing: [Data], received: [IncomingText], established: Bool, peerContactId: String?, flushed: Int,
                opened: [OpenedInfo] = [], fileProgress: [FileProgressInfo] = [], files: [ReceivedFileInfo] = [],
                fileFailures: [FileFailureInfo] = [], morePending: Bool = false) {
        self.outgoing = outgoing
        self.received = received
        self.established = established
        self.peerContactId = peerContactId
        self.flushed = flushed
        self.opened = opened
        self.fileProgress = fileProgress
        self.files = files
        self.fileFailures = fileFailures
        self.morePending = morePending
    }
}

public enum NetworkType: Sendable { case offline, wifi, cellular }

public struct GatewayDecision: Equatable, Sendable {
    public var bridge: Bool
    public var meteredWarning: Bool
    public init(bridge: Bool, meteredWarning: Bool) {
        self.bridge = bridge
        self.meteredWarning = meteredWarning
    }
}

public struct PasswordCheck: Equatable, Sendable {
    public var ok: Bool
    public var score: Int
    public var tooShort: Bool
    public init(ok: Bool, score: Int, tooShort: Bool) {
        self.ok = ok
        self.score = score
        self.tooShort = tooShort
    }
}

public struct BackupMeta: Equatable, Sendable {
    public var createdAtMs: Int64
    public var deviceHint: String
    public var needsPassword: Bool
    public init(createdAtMs: Int64, deviceHint: String, needsPassword: Bool) {
        self.createdAtMs = createdAtMs
        self.deviceHint = deviceHint
        self.needsPassword = needsPassword
    }
}

public struct RestoredBackup: Equatable, Sendable {
    public var phrase: String
    public var contacts: [Contact]
    public init(phrase: String, contacts: [Contact]) {
        self.phrase = phrase
        self.contacts = contacts
    }
}

public enum CoreError: Error, Equatable {
    case wrongBackupPassword
    case invalid(String)
    case fileTooLarge(size: UInt64, max: UInt64)
    case notLoaded
    case other(String)
}

/// One BLE connection's secure session (Noise hybrid + M2 fragments).
public protocol CoreLink: AnyObject {
    func start() throws -> LinkResult
    func onPdu(_ pdu: Data) throws -> LinkResult
    func sendText(_ text: String) throws -> LinkResult
    func flush() throws -> LinkResult
    var isEstablished: Bool { get }
    func close()
}

/// App-facing protocol core. Production: `RustOmcpCore` (omcp-ffi via UniFFI, in the app
/// target). Previews/tests: `FakeOmcpCore`. The UI layer never invents crypto results.
public protocol OmcpCore: AnyObject {
    var linked: Bool { get }
    var suiteLabel: String { get }

    func generateRecoveryPhrase() throws -> String
    func validateRecoveryPhrase(_ phrase: String) -> Bool
    func suggestRecoveryWords(_ prefix: String) -> [String]

    @discardableResult func load(phrase: String, displayName: String) throws -> Identity
    func identity() -> Identity?
    func qrPayload() throws -> String
    func advertisement() throws -> Data

    func previewContact(_ payload: String) throws -> ContactPreview
    func addContactFromQr(_ payload: String) throws -> Contact
    func restoreContact(_ contact: Contact) throws
    func contacts() -> [Contact]
    func setVerified(_ contactId: String, _ verified: Bool) throws
    func contactForAdvertisement(_ adv: Data) -> String?

    func queueMessage(_ contactId: String, text: String, burn: BurnSpec?) throws -> String
    func queueMedia(_ contactId: String, mime: String, bytes: Data, burn: BurnSpec?) throws -> String
    func openBurn(_ msgId: String, nowMs: Int64) throws -> BurnContent?
    func closeBurn(_ msgId: String) throws
    func tickBurn(nowMs: Int64) -> [String]
    func burnIsLive(_ msgId: String) -> Bool
    func burnSpecValid(_ spec: BurnSpec) -> Bool
    /// Queue a file (≤ `maxFileBytes()`, checked in Rust before encryption). Throws `.fileTooLarge`.
    func queueFile(_ contactId: String, name: String, mime: String, bytes: Data) throws -> FileOfferInfo
    @discardableResult func cancelFile(_ transferId: String) -> Bool
    func queuedFor(_ contactId: String) -> Int
    func pathStatus() -> MeshStatus

    func openInitiatorLink(_ contactId: String) throws -> CoreLink
    func openResponderLink() throws -> CoreLink

    func checkBackupPassword(_ password: String) -> PasswordCheck
    func createBackup(password: String, deviceHint: String, nowMs: Int64) throws -> Data
    /// iCloud "No password" mode: sealed under a 256-bit key kept in iCloud Keychain.
    func createBackup(key: Data, deviceHint: String, nowMs: Int64) throws -> Data
    func generateBackupKey() throws -> Data
    func backupInfo(_ blob: Data) -> BackupMeta?
    func openBackup(_ blob: Data, password: String) throws -> RestoredBackup
    func openBackup(_ blob: Data, key: Data) throws -> RestoredBackup
    func recoveryPhrase() throws -> String

    /// Core file cap (decimal bytes, 50 MB). UI must read this, never hard-code.
    func maxFileBytes() -> UInt64
    func gatewayDailyCapBytes() -> UInt64
    func gatewayDecision(wifiOnly: Bool, network: NetworkType, metered: Bool) -> GatewayDecision
}
