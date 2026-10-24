import AuthenticationServices
import CloudKit
import Foundation
import UIKit
import HoplaKit

/// iCloud backup: one `OMCPBackup` record in the CloudKit **private** database. The record holds
/// only the sealed blob from the Rust core (never the phrase or the key). Blocking API: call off
/// the main thread. Requires the iCloud + CloudKit capability with container
/// `iCloud.chat.omcp.app` (see OpenMeshChat.entitlements).
final class ICloudBackupStore: CloudBackupStore {
    let method = backupICloud
    private let db = CKContainer(identifier: "iCloud.chat.omcp.app").privateCloudDatabase
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
