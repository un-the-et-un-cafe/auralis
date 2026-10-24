import CloudKit
import Foundation
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

/// Google Drive backup on iOS: blob in `appDataFolder`, scope `drive.appdata` only, via the
/// Drive v3 REST API. STUB: no Google OAuth client is configured for the iOS beta, so
/// `tokenProvider` returns nil and the method shows as unavailable. Plug in GoogleSignIn
/// (scope https://www.googleapis.com/auth/drive.appdata) to enable it.
final class GoogleDriveBackupStore: CloudBackupStore {
    let method = backupDrive
    var tokenProvider: () -> String? = { nil }
    private let fileName = "omcp-backup-v1.cbor"
    var available: Bool { tokenProvider() != nil }

    private func request(_ url: String, method: String = "GET", body: Data? = nil, contentType: String? = nil) throws -> (Data, Int) {
        guard let token = tokenProvider(), let u = URL(string: url) else { throw CoreError.other("drive unavailable") }
        var r = URLRequest(url: u)
        r.httpMethod = method
        r.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        if let contentType { r.setValue(contentType, forHTTPHeaderField: "Content-Type") }
        r.httpBody = body
        let sem = DispatchSemaphore(value: 0)
        var out: (Data, Int) = (Data(), 0)
        var err: Error?
        URLSession.shared.dataTask(with: r) { d, resp, e in
            out = (d ?? Data(), (resp as? HTTPURLResponse)?.statusCode ?? 0)
            err = e
            sem.signal()
        }.resume()
        sem.wait()
        if let err { throw err }
        return out
    }

    private func fileId() throws -> String? {
        let (d, _) = try request("https://www.googleapis.com/drive/v3/files?spaces=appDataFolder&q=name%3D%27\(fileName)%27&fields=files(id)")
        let j = try JSONSerialization.jsonObject(with: d) as? [String: Any]
        return ((j?["files"] as? [[String: Any]])?.first?["id"]) as? String
    }

    func upload(_ blob: Data) throws {
        if let id = try fileId() {
            _ = try request("https://www.googleapis.com/upload/drive/v3/files/\(id)?uploadType=media", method: "PATCH", body: blob, contentType: "application/octet-stream")
            return
        }
        let boundary = "omcp-\(UUID().uuidString)"
        var body = Data("--\(boundary)\r\nContent-Type: application/json\r\n\r\n{\"name\":\"\(fileName)\",\"parents\":[\"appDataFolder\"]}\r\n--\(boundary)\r\nContent-Type: application/octet-stream\r\n\r\n".utf8)
        body.append(blob)
        body.append(Data("\r\n--\(boundary)--".utf8))
        let (_, code) = try request("https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart", method: "POST", body: body,
                                    contentType: "multipart/related; boundary=\(boundary)")
        guard (200..<300).contains(code) else { throw CoreError.other("drive \(code)") }
    }

    func download() throws -> Data? {
        guard let id = try fileId() else { return nil }
        return try request("https://www.googleapis.com/drive/v3/files/\(id)?alt=media").0
    }

    func delete() throws {
        guard let id = try fileId() else { return }
        _ = try request("https://www.googleapis.com/drive/v3/files/\(id)", method: "DELETE")
    }
}
