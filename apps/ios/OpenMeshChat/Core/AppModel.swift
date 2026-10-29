import Combine
import Foundation
import HoplaKit
import UIKit

/// App-wide state: the repository over the real Rust core, cloud backup stores, BLE and
/// notifications. SwiftUI views observe `revision` to refresh.
@MainActor
final class AppModel: ObservableObject {
    @Published private(set) var revision = 0
    @Published var onboarded: Bool
    @Published private(set) var backupGate: BackupGate
    let repo: MeshRepository
    let iCloud = ICloudBackupStore()
    let drive = GoogleDrive.makeStore()
    let backupKeys = ICloudKeychainBackupKeyStore()
    private(set) var ble: BleMeshTransport?
    private var burnTimer: Timer?

    init(core: OmcpCore = RustOmcpCore(), store: AppStore = DefaultsAppStore(),
         persistentStore: Bool = true,
         clock: @escaping () -> Int64 = { Int64(Date().timeIntervalSince1970 * 1000) }) {
        let receiptQueue = DispatchQueue(label: "chat.auralis.received-files", qos: .utility)
        repo = MeshRepository(core: core, store: store, dbPath: persistentStore ? Self.localStorePath() : nil,
            persistentStoreRequired: persistentStore, dbPathProvider: persistentStore ? { Self.localStorePath() } : nil,
            receiveExecutor: { work in receiptQueue.async(execute: work) },
            receivePublisher: { work in DispatchQueue.main.async(execute: work) }, clock: clock)
        onboarded = false
        backupGate = repo.backupGate()
        repo.notifier = LocalNotifier()
        repo.fileSink = ReceivedFiles()
        // Chat backup (CB1–CB5) in iCloud: private DB, zone HoplaBackupV1 (REQUIREMENTS v0.1.4a).
        repo.chatBackup = ChatBackupSync(core: core, blobs: RecordChatBlobStore(backend: CloudKitChatRecordBackend()))
        repo.genericBody = L("notif_new_message")
        // Flow steps may run off the main thread (cloud I/O); always publish on main.
        repo.onChange = { [weak self] in
            Task { @MainActor in
                guard let self else { return }
                self.revision &+= 1
                self.onboarded = !self.repo.storageBlocked && self.repo.store.onboarded && self.repo.core.identity() != nil
                self.refreshTimeSensitiveState()
            }
        }
        onboarded = repo.restore() && store.onboarded
        // Keep sent media: age limit on start (the size limit applies on every insert).
        _ = repo.core.evictKeptMedia(nowMs: Int64(Date().timeIntervalSince1970 * 1000))
        // Burn timers + unread expiry, decided in Rust; ≤ 250 ms so B1 (≤ 1 s) holds.
        burnTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.repo.tickBurn()
                self?.refreshTimeSensitiveState()
            }
        }
        if onboarded { startMesh() }
    }

    /// Application Support/omcp-store.db, excluded from iCloud/iTunes backup (its key is
    /// this-device-only anyway) and protected until first unlock.
    nonisolated static func localStorePath() -> String? {
        let fm = FileManager.default
        guard var dir = try? fm.url(for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true) else {
            return nil
        }
        try? fm.setAttributes([.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication], ofItemAtPath: dir.path)
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try? dir.setResourceValues(values)
        return dir.appendingPathComponent("omcp-store.db").path
    }

    var core: OmcpCore { repo.core }
    var store: AppStore { repo.store }

    func makeOnboarding() -> OnboardingFlow {
        OnboardingFlow(repo: repo, iCloud: iCloud, drive: drive, keyStore: backupKeys, deviceHint: UIDevice.current.model)
    }

    func makeBackupSettings() -> BackupSettings {
        BackupSettings(repo: repo, stores: [iCloud, drive], keyStore: backupKeys, deviceHint: UIDevice.current.model)
    }

    func startMesh() {
        guard !repo.storageBlocked, ble == nil, core.identity() != nil else { return }
        let t = BleMeshTransport(repo: repo)
        repo.kick = t
        ble = t
        t.start()
    }

    /// Expiry must be observed even when no message/contact state changes while idle.
    func refreshTimeSensitiveState() {
        let next = repo.backupGate()
        if backupGate != next { backupGate = next }
    }

    func retryStore() {
        guard repo.storageBlocked else { return }
        onboarded = repo.restore() && store.onboarded
        revision &+= 1
        if onboarded { startMesh() }
    }

    func touch() { revision &+= 1 }
}
