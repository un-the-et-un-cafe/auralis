import AuthenticationServices
import CloudKit
import Foundation
import UIKit
import HoplaKit

/// iCloud backup: one `OMCPBackup` record in the CloudKit **private** database. The record holds
/// only the sealed blob from the Rust core (never the phrase or the key). Blocking API: call off
/// the main thread. Requires the iCloud + CloudKit capability with container
/// `iCloud.chat.auralis.app` (see OpenMeshChat.entitlements).
final class ICloudBackupStore: CloudBackupStore {
    let method = backupICloud
    // Lazy: CKContainer(identifier:) traps when the binary lacks the iCloud entitlement (e.g. unsigned
    // simulator builds in CI), so only create it when CloudKit is actually used.
    private lazy var db = CKContainer(identifier: "iCloud.chat.auralis.app").privateCloudDatabase
    private let recordID = CKRecord.ID(recordName: "omcp-backup-v1")
    static let recordType = "OMCPBackup"

    var available: Bool { FileManager.default.ubiquityIdentityToken != nil }

    private func wait<T>(_ body: (@escaping (Result<T, Error>) -> Void) -> Void) throws -> T {
        let sem = DispatchSemaphore(value: 0)
        var out: Result<T, Error>!
        body { out = $0; sem.signal() }
        sem.wait()
        return try out.get()
    }

    func upload(_ blob: Data) throws {
        let rec = CKRecord(recordType: Self.recordType, recordID: recordID)
        rec["blob"] = blob as CKRecordValue
        rec["v"] = 1 as CKRecordValue
        let _: Void = try wait { done in
            let op = CKModifyRecordsOperation(recordsToSave: [rec], recordIDsToDelete: nil)
            op.savePolicy = .allKeys
            op.qualityOfService = .userInitiated
            op.modifyRecordsResultBlock = { done($0.map { _ in () }) }
            db.add(op)
        }
    }

    func download() throws -> Data? {
        do {
            let rec: CKRecord = try wait { done in
                db.fetch(withRecordID: recordID) { r, e in
                    if let r { done(.success(r)) } else { done(.failure(e ?? CoreError.other("cloudkit"))) }
                }
            }
            return rec["blob"] as? Data
        } catch let e as CKError where e.code == .unknownItem {
            return nil
        }
    }

    func delete() throws {
        do {
            let _: CKRecord.ID = try wait { done in
                db.delete(withRecordID: recordID) { id, e in
                    if let id { done(.success(id)) } else { done(.failure(e ?? CoreError.other("cloudkit"))) }
                }
            }
        } catch let e as CKError where e.code == .unknownItem {}
    }
}

/// iCloud "No password" key: iCloud Keychain item, kSecAttrSynchronizable +
/// kSecAttrAccessibleAfterFirstUnlock (end-to-end encrypted by Apple).
final class ICloudKeychainBackupKeyStore: BackupKeyStore {
    private let account = "omcp.backupKey.v1"
    func load() throws -> Data? { Keychain.read(account: account, synchronizable: true) }
    func save(_ key: Data) throws {
        guard Keychain.write(key, account: account, synchronizable: true) else { throw CoreError.other("keychain") }
    }
    func delete() throws { Keychain.delete(account: account, synchronizable: true) }
}

// MARK: Google Drive (logic + tests in HoplaKit/GoogleDrive.swift)

/// Builds the Drive store from Info.plist `GOOGLE_OAUTH_CLIENT_ID` (empty → unavailable).
enum GoogleDrive {
    static func makeStore(info: [String: Any]? = Bundle.main.infoDictionary) -> GoogleDriveBackupStore {
        let http = URLSessionTransport()
        guard let config = GoogleOAuthConfig.fromInfo(info) else {
            return GoogleDriveBackupStore(config: nil, http: http, tokens: nil)
        }
        let tokens = GoogleTokenManager(config: config, http: http, store: KeychainRefreshTokenStore(),
                                        authorize: WebAuthenticator.shared.authorize)
        return GoogleDriveBackupStore(config: config, http: http, tokens: tokens)
    }
}

/// Blocking URLSession transport (Drive and OAuth calls already run off the main thread).
final class URLSessionTransport: HTTPTransport {
    func send(method: String, url: URL, headers: [String: String], body: Data?) throws -> (status: Int, body: Data) {
        precondition(!Thread.isMainThread, "network on main thread")
        var r = URLRequest(url: url, timeoutInterval: 30)
        r.httpMethod = method
        headers.forEach { r.setValue($0.value, forHTTPHeaderField: $0.key) }
        r.httpBody = body
        let sem = DispatchSemaphore(value: 0)
        var out: (Int, Data) = (0, Data())
        var err: Error?
        URLSession.shared.dataTask(with: r) { d, resp, e in
            out = ((resp as? HTTPURLResponse)?.statusCode ?? 0, d ?? Data())
            err = e
            sem.signal()
        }.resume()
        sem.wait()
        if let err { throw err }
        return out
    }
}

/// Refresh token: device-only Keychain item (not iCloud-synced; survives reinstall).
final class KeychainRefreshTokenStore: RefreshTokenStore {
    private let account = "omcp.googleDrive.refreshToken.v1"
    func load() -> String? { Keychain.read(account: account).map { String(decoding: $0, as: UTF8.self) } }
    func save(_ token: String) { Keychain.write(Data(token.utf8), account: account, synchronizable: false) }
    func clear() { Keychain.delete(account: account, synchronizable: false) }
}

/// Google's consent page in an `ASWebAuthenticationSession` (system browser sheet, shares no
/// cookies with the app). Called from a background thread; presents on main and waits.
final class WebAuthenticator: NSObject, ASWebAuthenticationPresentationContextProviding {
    static let shared = WebAuthenticator()
    private var session: ASWebAuthenticationSession?

    func authorize(_ url: URL, callbackScheme: String) throws -> URL {
        precondition(!Thread.isMainThread, "authorize blocks; call off the main thread")
        let sem = DispatchSemaphore(value: 0)
        var result: Result<URL, Error> = .failure(GoogleOAuthError.cancelled)
        DispatchQueue.main.async {
            let s = ASWebAuthenticationSession(url: url, callbackURLScheme: callbackScheme) { cb, err in
                if let cb { result = .success(cb) } else { result = .failure(err.map { _ in GoogleOAuthError.cancelled } ?? GoogleOAuthError.cancelled) }
                sem.signal()
            }
            s.presentationContextProvider = self
            s.prefersEphemeralWebBrowserSession = true
            self.session = s
            if !s.start() { sem.signal() }
        }
        sem.wait()
        session = nil
        return try result.get()
    }

    func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        UIApplication.shared.connectedScenes.compactMap { ($0 as? UIWindowScene)?.keyWindow }.first ?? ASPresentationAnchor()
    }
}

// MARK: iCloud chat backup (REQUIREMENTS v0.1.4a; naming + manifest logic in HoplaKit/ICloudChatRecords.swift)

/// Private database, custom zone `HoplaBackupV1`. Each record has only `blob` (CKAsset), `seq` and
/// `created`; blobs are Rust-sealed and padded. Blocking: call off the main thread.
final class CloudKitChatRecordBackend: ChatRecordBackend {
    private lazy var db = CKContainer(identifier: "iCloud.chat.auralis.app").privateCloudDatabase
    private let zoneID = CKRecordZone.ID(zoneName: ICloudChatRecords.zone, ownerName: CKCurrentUserDefaultName)
    private var zoneReady = false

    var available: Bool { FileManager.default.ubiquityIdentityToken != nil }

    private func wait<T>(_ body: (@escaping (Result<T, Error>) -> Void) -> Void) throws -> T {
        let sem = DispatchSemaphore(value: 0)
        var out: Result<T, Error>!
        body { out = $0; sem.signal() }
        sem.wait()
        return try out.get()
    }

    private func ensureZone() throws {
        if zoneReady { return }
        let _: Void = try wait { done in
            let op = CKModifyRecordZonesOperation(recordZonesToSave: [CKRecordZone(zoneID: zoneID)], recordZoneIDsToDelete: nil)
            op.qualityOfService = .utility
            op.modifyRecordZonesResultBlock = { done($0.map { _ in () }) }
            db.add(op)
        }
        zoneReady = true
    }

    private func id(_ name: String) -> CKRecord.ID { CKRecord.ID(recordName: name, zoneID: zoneID) }

    func save(_ r: ChatCloudRecord) throws {
        try ensureZone()
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try r.blob.write(to: url, options: .completeFileProtection)
        defer { try? FileManager.default.removeItem(at: url) }
        let rec = CKRecord(recordType: r.type, recordID: id(r.name))
        rec["blob"] = CKAsset(fileURL: url)
        rec["seq"] = Int64(bitPattern: r.seq) as CKRecordValue
        rec["created"] = r.createdMs as CKRecordValue
        let _: Void = try wait { done in
            let op = CKModifyRecordsOperation(recordsToSave: [rec], recordIDsToDelete: nil)
            op.savePolicy = .allKeys
            op.qualityOfService = .utility
            op.modifyRecordsResultBlock = { done($0.map { _ in () }) }
            db.add(op)
        }
    }

    func fetch(_ name: String) throws -> ChatCloudRecord? {
        try ensureZone()
        do {
            let rec: CKRecord = try wait { done in
                db.fetch(withRecordID: id(name)) { r, e in
                    if let r { done(.success(r)) } else { done(.failure(e ?? CoreError.other("cloudkit"))) }
                }
            }
            guard let url = (rec["blob"] as? CKAsset)?.fileURL, let data = try? Data(contentsOf: url) else { return nil }
            return ChatCloudRecord(type: rec.recordType, name: name, blob: data,
                                   seq: UInt64(bitPattern: (rec["seq"] as? Int64) ?? 0),
                                   createdMs: (rec["created"] as? Int64) ?? 0)
        } catch let e as CKError where e.code == .unknownItem || e.code == .zoneNotFound {
            return nil
        }
    }

    func delete(_ name: String) throws {
        try ensureZone()
        do {
            let _: CKRecord.ID = try wait { done in
                db.delete(withRecordID: id(name)) { i, e in
                    if let i { done(.success(i)) } else { done(.failure(e ?? CoreError.other("cloudkit"))) }
                }
            }
        } catch let e as CKError where e.code == .unknownItem {}
    }
}

/// Each worker operation creates and owns its blocking provider. No mutable provider,
/// repository or UI object crosses the actor boundary; only sealed Sendable values do.
enum AppCloudBackupIO {
    private static func store(_ method: String) throws -> CloudBackupStore {
        switch method {
        case backupICloud: return ICloudBackupStore()
        case backupDrive: return GoogleDrive.makeStore()
        default: throw CoreError.invalid("backup destination")
        }
    }

    static func download(_ methods: [String]) -> [(method: String, blob: Data)] {
        methods.compactMap { method in
            guard let store = try? store(method), let blob = try? store.download() else { return nil }
            return (method, blob)
        }
    }

    static func upload(_ upload: PreparedCloudBackup) throws {
        try upload.upload(to: store(upload.method))
    }

    static func delete(_ method: String) throws { try store(method).delete() }
}
