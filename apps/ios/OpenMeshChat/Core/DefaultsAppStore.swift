import Foundation
import HoplaKit
import Security

/// UserDefaults for settings; the recovery phrase lives in the device Keychain
/// (this-device-only, never synced, never in backups of the app container).
final class DefaultsAppStore: AppStore {
    private let d: UserDefaults
    private let phraseAccount = "omcp.recoveryPhrase"
    private let storageKeyAccount = "omcp.storageKey"
    init(_ d: UserDefaults = .standard) { self.d = d }

    var recoveryPhrase: String? {
        get { Keychain.read(account: phraseAccount).map { String(decoding: $0, as: UTF8.self) } }
        set {
            if let v = newValue { Keychain.write(Data(v.utf8), account: phraseAccount, synchronizable: false) } else {
                Keychain.delete(account: phraseAccount, synchronizable: false)
            }
        }
    }

    /// Local-store key: Keychain, AfterFirstUnlockThisDeviceOnly (BLE background needs it
    /// after first unlock), never synced and never in device backups.
    var storageKey: Data? {
        get { Keychain.read(account: storageKeyAccount) }
        set {
            if let v = newValue { Keychain.write(v, account: storageKeyAccount, synchronizable: false) } else {
                Keychain.delete(account: storageKeyAccount, synchronizable: false)
            }
        }
    }

    private func codable<T: Codable>(_ key: String, _ def: T) -> T {
        guard let data = d.data(forKey: key), let v = try? JSONDecoder().decode(T.self, from: data) else { return def }
        return v
    }
    private func setCodable<T: Codable>(_ key: String, _ v: T) { d.set(try? JSONEncoder().encode(v), forKey: key) }

    var displayName: String? { get { d.string(forKey: "omcp.name") } set { d.set(newValue, forKey: "omcp.name") } }
    var contacts: [Contact] { get { codable("omcp.contacts", []) } set { setCodable("omcp.contacts", newValue) } }
    var onboarded: Bool { get { d.bool(forKey: "omcp.onboarded") } set { d.set(newValue, forKey: "omcp.onboarded") } }
    var privateNotifications: Bool { get { d.bool(forKey: "omcp.privateNotif") } set { d.set(newValue, forKey: "omcp.privateNotif") } }
    var chatPrivateOverride: [String: Bool] { get { codable("omcp.chatPrivate", [:]) } set { setCodable("omcp.chatPrivate", newValue) } }
    var hideOnLockScreen: Bool { get { d.bool(forKey: "omcp.hideLock") } set { d.set(newValue, forKey: "omcp.hideLock") } }
    var burnChats: [String: Int] { get { codable("omcp.burnChats", [:]) } set { setCodable("omcp.burnChats", newValue) } }
    var burnUnreadTtl: [String: Int] { get { codable("omcp.burnTtl", [:]) } set { setCodable("omcp.burnTtl", newValue) } }
    var gatewayEnabled: Bool { get { d.bool(forKey: "omcp.gateway") } set { d.set(newValue, forKey: "omcp.gateway") } }
    var gatewayAllowMobile: Bool { get { d.bool(forKey: "omcp.gatewayMobile") } set { d.set(newValue, forKey: "omcp.gatewayMobile") } }
    var backupMethods: Set<String> { get { codable("omcp.backupMethods", []) } set { setCodable("omcp.backupMethods", newValue) } }
    var lastCloudBackupMs: Int64 {
        get { Int64(d.double(forKey: "omcp.lastBackup")) } set { d.set(Double(newValue), forKey: "omcp.lastBackup") }
    }
    var backupRemindDeadlineMs: Int64 {
        get { Int64(d.double(forKey: "omcp.remindDeadline")) } set { d.set(Double(newValue), forKey: "omcp.remindDeadline") }
    }
}

/// Minimal generic-password Keychain helper.
enum Keychain {
    static let service = "chat.omcp.app"

    private static func query(_ account: String, _ sync: Bool) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account,
         kSecAttrSynchronizable as String: sync ? kCFBooleanTrue! : kCFBooleanFalse!]
    }

    static func read(account: String, synchronizable: Bool = false) -> Data? {
        var q = query(account, synchronizable)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: AnyObject?
        return SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
    }

    @discardableResult
    static func write(_ data: Data, account: String, synchronizable: Bool) -> Bool {
        delete(account: account, synchronizable: synchronizable)
        var q = query(account, synchronizable)
        q[kSecValueData as String] = data
        // iCloud Keychain item must be AfterFirstUnlock (not ThisDeviceOnly) to sync.
        q[kSecAttrAccessible as String] = synchronizable ? kSecAttrAccessibleAfterFirstUnlock : kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        return SecItemAdd(q as CFDictionary, nil) == errSecSuccess
    }

    static func delete(account: String, synchronizable: Bool) {
        SecItemDelete(query(account, synchronizable) as CFDictionary)
    }
}
