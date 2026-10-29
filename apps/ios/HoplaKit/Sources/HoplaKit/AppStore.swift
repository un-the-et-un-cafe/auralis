import Foundation

public let backupICloud = "icloud"
public let backupDrive = "drive"
public let backupWritten = "written"
public let remindWindowMs: Int64 = 24 * 60 * 60 * 1000

/// Persistent app state. iOS impl: `DefaultsAppStore` (phrase in the Keychain); tests: `InMemoryAppStore`.
public protocol AppStore: AnyObject {
    var recoveryPhrase: String? { get set }
    var hasSavedIdentity: Bool { get }
    var displayName: String? { get set }
    var contacts: [Contact] { get set }
    var onboarded: Bool { get set }
    /// Global Private notifications. Off by default: sender + preview are shown.
    var privateNotifications: Bool { get set }
    /// Per-chat override: true = private, false = not private even if global is on.
    var chatPrivateOverride: [String: Bool] { get set }
    /// Lock screen shows only "Hopla · New message" (hiddenPreviewsBodyPlaceholder).
    var hideOnLockScreen: Bool { get set }
    /// Per-chat burn option: -1 off, 0 on open, else timer seconds.
    var burnChats: [String: Int] { get set }
    var burnUnreadTtl: [String: Int] { get set }
    var gatewayEnabled: Bool { get set }
    /// Off = Wi-Fi only (default). Metered cellular warns, never blocks.
    var gatewayAllowMobile: Bool { get set }
    /// Completed backup methods: "icloud", "drive", "written".
    var backupMethods: Set<String> { get set }
    var lastCloudBackupMs: Int64 { get set }
    /// "Remind me later" deadline (24 h); 0 = not deferred.
    var backupRemindDeadlineMs: Int64 { get set }
    /// 256-bit key of the encrypted local store. On device: Keychain, this device only.
    var storageKey: Data? { get set }
    var hasSavedStorageKey: Bool { get }
}

public extension AppStore {
    var hasSavedIdentity: Bool { recoveryPhrase != nil || onboarded }
    var hasSavedStorageKey: Bool { storageKey != nil }
}

public final class InMemoryAppStore: AppStore {
    public var recoveryPhrase: String?
    public var displayName: String?
    public var storageKey: Data?
    public var contacts: [Contact] = []
    public var onboarded = false
    public var privateNotifications = false
    public var chatPrivateOverride: [String: Bool] = [:]
    public var hideOnLockScreen = false
    public var burnChats: [String: Int] = [:]
    public var burnUnreadTtl: [String: Int] = [:]
    public var gatewayEnabled = false
    public var gatewayAllowMobile = false
    public var backupMethods: Set<String> = []
    public var lastCloudBackupMs: Int64 = 0
    public var backupRemindDeadlineMs: Int64 = 0
    public init() {}
}

public enum BackupGate: Equatable { case ok, banner, required }

/// Backup requirement (REQUIREMENTS "Onboarding & recovery backup", O6).
public func backupGate(_ store: AppStore, now: Int64) -> BackupGate {
    if !store.backupMethods.isEmpty { return .ok }
    if store.backupRemindDeadlineMs == 0 { return .required }
    return now < store.backupRemindDeadlineMs ? .banner : .required
}
