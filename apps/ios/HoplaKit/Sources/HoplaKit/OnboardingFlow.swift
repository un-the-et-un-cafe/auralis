import Foundation

/// REQUIREMENTS "Onboarding & recovery backup (v0.1.1)": Welcome → Profile → Secure → Done
/// (4 screens, O1). Side paths: WriteDown → Quiz (3 random words), ICloudPassword (optional),
/// RestorePrompt (backup found on reinstall), RestorePhrase (24-word fallback, always available).
/// Plain layouts only: the v0.1.2 onboarding design is not approved.
public enum OnboardingStep: Equatable { case welcome, restorePrompt, restorePhrase, profile, secure, writeDown, quiz, done }

public struct QuizField: Equatable {
    public var position: Int
    public var input = ""
    public var ok = false
    public var error = false
}

public struct OnboardingState: Equatable {
    public var step: OnboardingStep = .welcome
    public var displayName = ""
    public var error: String?
    public var iCloudAvailable = false
    public var driveAvailable = false
    /// iCloud: empty = "No password" key mode (key in iCloud Keychain).
    public var usePassword = false
    public var password = ""
    public var passwordOk = false
    public var passwordTooShort = false
    public var iCloudDone = false
    public var driveDone = false
    public var writtenDone = false
    public var words: [String] = []
    public var quiz: [QuizField] = []
    public var foundBackup: BackupMeta?
    public var foundMethod: String?
    public var restorePassword = ""
    public var restoreLockMs: Int64 = 0
    public var restorePhrase = ""

    public init(iCloudAvailable: Bool = false, driveAvailable: Bool = false) {
        self.iCloudAvailable = iCloudAvailable
        self.driveAvailable = driveAvailable
    }

    public var anyBackupDone: Bool { iCloudDone || driveDone || writtenDone }
    public var quizOk: Bool { quiz.count == 3 && quiz.allSatisfy(\.ok) }
}

public final class OnboardingFlow {
    public private(set) var state: OnboardingState { didSet { onChange?(state) } }
    public var onChange: ((OnboardingState) -> Void)?
    private let repo: MeshRepository
    private let iCloud: CloudBackupStore?
    private let drive: CloudBackupStore?
    private let keyStore: BackupKeyStore?
    private let deviceHint: String
    private var rng: RandomNumberGenerator
    private let limiter: RetryLimiter
    private var pendingBlob: Data?

    public init(repo: MeshRepository, iCloud: CloudBackupStore?, drive: CloudBackupStore?, keyStore: BackupKeyStore?,
                deviceHint: String = "iPhone", rng: RandomNumberGenerator = SystemRandomNumberGenerator()) {
        self.repo = repo
        self.iCloud = iCloud
        self.drive = drive
        self.keyStore = keyStore
        self.deviceHint = deviceHint
        self.rng = rng
        self.limiter = RetryLimiter(clock: repo.now)
        state = OnboardingState(iCloudAvailable: iCloud?.available ?? false, driveAvailable: drive?.available ?? false)
    }

    private var core: OmcpCore { repo.core }
    private var name: String {
        let n = state.displayName.trimmingCharacters(in: .whitespaces)
        return n.isEmpty ? "Me" : n
    }

    /// Fresh install: look for an existing backup ("Welcome back"). iCloud first (iOS default).
    public func detectBackup() {
        for store in [iCloud, drive].compactMap({ $0 }) where store.available {
            guard let blob = try? store.download() else { continue }
            acceptBackupSearch([(store.method, blob)])
            if state.step != .welcome { return }
        }
    }

    /// Only destination identifiers cross to the app's background provider workers.
    public var availableBackupMethods: [String] {
        [iCloud, drive].compactMap { $0 }.filter { $0.available }.map { $0.method }
    }

    public func acceptBackupSearch(_ candidates: [(method: String, blob: Data)]) {
        guard state.step == .welcome else { return }
        for candidate in candidates {
            guard let meta = core.backupInfo(candidate.blob) else { continue }
            pendingBlob = candidate.blob
            state.foundBackup = meta
            state.foundMethod = candidate.method
            state.step = .restorePrompt
            return
        }
    }

    public func getStarted() { state.step = .profile }
    public func setName(_ n: String) { state.displayName = String(n.prefix(64)) }

    /// Profile → Secure. Identity persistence is required before leaving this step.
    public func profileNext() {
        do {
            let phrase = core.identity() == nil ? try core.generateRecoveryPhrase() : try core.recoveryPhrase()
            try repo.saveIdentity(phrase: phrase, name: name)
        } catch {
            state.error = "identity"
            return
        }
        state.error = nil
        state.step = .secure
    }

    // MARK: cloud

    public func setUsePassword(_ on: Bool) {
        state.usePassword = on
        if !on { setPassword("") }
    }

    public func setPassword(_ pw: String) {
        let c = core.checkBackupPassword(pw)
        state.password = pw
        state.passwordOk = c.ok
        state.passwordTooShort = c.tooShort
    }

    /// Prepare on the repository owner. No state is marked backed up until upload succeeds.
    public func prepareBackup(_ method: String) throws -> PreparedCloudBackup {
        let store = method == backupICloud ? iCloud : (method == backupDrive ? drive : nil)
        guard let store, store.available else { throw CoreError.other("unavailable") }
        let blob: Data
        if method == backupICloud && !state.usePassword {
            guard let keyStore else { throw CoreError.other("keychain") }
            let key = try keyStore.load() ?? core.generateBackupKey()
            try keyStore.save(key)
            blob = try core.createBackup(key: key, deviceHint: deviceHint, nowMs: repo.now())
        } else {
            guard state.passwordOk else { throw CoreError.invalid("weak password") }
            blob = try core.createBackup(password: state.password, deviceHint: deviceHint, nowMs: repo.now())
        }
        return PreparedCloudBackup(store: store, blob: blob)
    }

    public func completeBackup(_ method: String, succeeded: Bool) {
        guard succeeded else { state.error = "backup"; return }
        repo.store.lastCloudBackupMs = repo.now()
        repo.markBackupMethod(method)
        if method == backupICloud { state.iCloudDone = true }
        if method == backupDrive { state.driveDone = true }
        state.password = ""
        state.passwordOk = false
        state.passwordTooShort = false
        state.error = nil
    }

    @discardableResult public func backupToICloud() -> Bool { backup(to: backupICloud) }
    @discardableResult public func backupToDrive() -> Bool { backup(to: backupDrive) }

    private func backup(to method: String) -> Bool {
        do {
            guard let store = method == backupICloud ? iCloud : drive else { return false }
            try prepareBackup(method).upload(to: store)
            completeBackup(method, succeeded: true)
            return true
        } catch {
            completeBackup(method, succeeded: false)
            return false
        }
    }

    // MARK: write it down

    public func writeItDown() {
        guard let phrase = try? core.recoveryPhrase(), core.validateRecoveryPhrase(phrase) else {
            state.error = "identity"
            return
        }
        let words = phrase.split(separator: " ").map(String.init)
        guard words.count == 24 else { state.error = "identity"; return }
        state.words = words
        state.error = nil
        state.step = .writeDown
    }

    public func startQuiz() {
        guard state.words.count == 24 else { state.error = "identity"; return }
        state.quiz = pickPositions().map { QuizField(position: $0) }
        state.step = .quiz
    }

    /// 3 random positions, never consecutive, never all in one 3-word row.
    public func pickPositions() -> [Int] {
        while true {
            var all = Array(0..<24)
            all.shuffle(using: &rng)
            let p = all.prefix(3).sorted()
            let consecutive = zip(p, p.dropFirst()).contains { $1 - $0 == 1 }
            if !consecutive && Set(p.map { $0 / 3 }).count > 1 { return p }
        }
    }

    public func setQuizInput(_ i: Int, _ input: String) {
        guard state.quiz.indices.contains(i) else { return }
        let pos = state.quiz[i].position
        guard state.words.indices.contains(pos) else { return }
        state.quiz[i].input = input
        state.quiz[i].ok = input.trimmingCharacters(in: .whitespaces).lowercased() == state.words[pos].lowercased()
        state.quiz[i].error = false
    }

    @discardableResult public func submitQuiz() -> Bool {
        guard state.quizOk else {
            for i in state.quiz.indices { state.quiz[i].error = !state.quiz[i].ok }
            return false
        }
        repo.markBackupMethod(backupWritten)
        state.writtenDone = true
        state.words = []
        state.quiz = []
        state.step = .secure
        return true
    }

    public func backToSecure() {
        state.words = []
        state.quiz = []
        state.step = .secure
    }

    @discardableResult public func secureNext() -> Bool {
        guard state.anyBackupDone else { return false }
        state.step = .done
        return true
    }

    /// "Remind me later": 24 h of use with a persistent banner, then blocked (O6).
    public func remindLater() {
        repo.remindBackupLater()
        state.step = .done
    }

    public func finish() { repo.completeOnboarding() }

    // MARK: restore

    public func setRestorePassword(_ pw: String) { state.restorePassword = pw }

    @discardableResult public func restoreFromCloud() -> Bool {
        guard let blob = pendingBlob, let meta = state.foundBackup, let method = state.foundMethod else { return false }
        guard limiter.canTry else {
            state.restoreLockMs = limiter.remainingLockMs()
            return false
        }
        do {
            let r: RestoredBackup
            if meta.needsPassword {
                r = try core.openBackup(blob, password: state.restorePassword)
            } else {
                guard let key = try keyStore?.load() else {
                    state.error = "nokey"
                    return false
                }
                r = try core.openBackup(blob, key: key)
            }
            limiter.onSuccess()
            try repo.restoreFromBackup(r, name: name, method: method)
            clearRestoreSecrets()
            state.restoreLockMs = 0
            state.error = nil
            state.step = .done
            return true
        } catch CoreError.wrongBackupPassword {
            limiter.onFailure()
            state.error = "wrongpw"
            state.restoreLockMs = limiter.remainingLockMs()
            state.restorePassword = ""
            return false
        } catch {
            state.error = "restore"
            return false
        }
    }

    public func usePhraseInstead() {
        state.restorePassword = ""
        state.error = nil
        state.step = .restorePhrase
    }

    public func setRestorePhrase(_ p: String) { state.restorePhrase = p }

    @discardableResult public func restoreFromPhrase() -> Bool {
        let p = state.restorePhrase.lowercased().split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
        guard core.validateRecoveryPhrase(p) else {
            state.error = "badphrase"
            return false
        }
        do { try repo.restoreFromPhrase(p, name: name) } catch {
            state.error = "badphrase"
            return false
        }
        repo.markBackupMethod(backupWritten)
        clearRestoreSecrets()
        state.error = nil
        state.step = .done
        return true
    }

    private func clearRestoreSecrets() {
        state.restorePassword = ""
        state.restorePhrase = ""
    }

    public func backToWelcome() {
        clearRestoreSecrets()
        state.error = nil
        state.step = .welcome
    }

    /// Screens on the happy path to the chat list (O1).
    public static let happyPathScreens: [OnboardingStep] = [.welcome, .profile, .secure, .done]
}
