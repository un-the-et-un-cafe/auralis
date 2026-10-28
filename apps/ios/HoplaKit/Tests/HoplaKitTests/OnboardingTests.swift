import XCTest
@testable import HoplaKit

final class OnboardingTests: XCTestCase {
    func flow(_ clock: Clock = Clock(), icloud: InMemoryCloudBackupStore = .init(method: backupICloud),
              keys: InMemoryBackupKeyStore = .init(), repo: MeshRepository? = nil) -> (OnboardingFlow, MeshRepository) {
        let r = repo ?? makeRepo(clock).0
        let f = OnboardingFlow(repo: r, iCloud: icloud, drive: InMemoryCloudBackupStore(method: backupDrive),
                               keyStore: keys, rng: SeededRNG(s: 7))
        return (f, r)
    }

    /// O1: Welcome → Profile → Secure (iCloud, no password) → Done: 4 screens.
    func testO1_cloudBackupReachesChatsInFourScreens() {
        let (f, r) = flow()
        var seen = [f.state.step]
        f.onChange = { s in if seen.last != s.step { seen.append(s.step) } }
        f.getStarted()
        f.setName("Ana")
        f.profileNext()
        XCTAssertTrue(f.backupToICloud())
        XCTAssertTrue(f.secureNext())
        f.finish()
        XCTAssertEqual(seen, OnboardingFlow.happyPathScreens)
        XCTAssertLessThanOrEqual(seen.count, 4)
        XCTAssertTrue(r.store.onboarded)
        XCTAssertEqual(r.backupGate(), .ok)
    }

    func testSecureRequiresAMethod() {
        let (f, _) = flow()
        f.getStarted(); f.profileNext()
        XCTAssertFalse(f.secureNext())
        XCTAssertEqual(f.state.step, .secure)
    }

    /// O2 plumbing: the uploaded bytes never contain the phrase (real crypto is tested in Rust).
    func testO2_uploadNeverContainsPhrase() throws {
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let (f, r) = flow(icloud: icloud)
        f.getStarted(); f.profileNext()
        f.setUsePassword(true); f.setPassword("correct horse battery staple")
        XCTAssertTrue(f.backupToICloud())
        let phrase = try r.core.recoveryPhrase()
        for blob in icloud.uploaded {
            XCTAssertNil(blob.range(of: Data(phrase.utf8)))
        }
    }

    /// O3: reinstall + restore from iCloud (no password): identity, contacts, verification.
    func testO3_restoreFromICloudKeyMode() throws {
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = InMemoryBackupKeyStore()
        let (f, r) = flow(icloud: icloud, keys: keys)
        f.getStarted(); f.profileNext()
        try r.addContact(qr: "omcp:1:abcd:Bo", action: .verifyAndChat)
        XCTAssertTrue(f.backupToICloud())
        XCTAssertNotNil(keys.key)
        let id = r.core.identity()

        let (f2, r2) = flow(icloud: icloud, keys: keys)
        f2.detectBackup()
        XCTAssertEqual(f2.state.step, .restorePrompt)
        XCTAssertEqual(f2.state.foundBackup?.needsPassword, false)
        XCTAssertTrue(f2.restoreFromCloud())
        XCTAssertEqual(r2.core.identity()?.peerIdHex, id?.peerIdHex)
        XCTAssertEqual(r2.contacts.first?.verified, true)
    }

    /// O4: wrong password fails cleanly with rate-limited retries; phrase fallback still works.
    func testO4_wrongPasswordRateLimitedAndPhraseFallback() throws {
        let clock = Clock()
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let (f, r) = flow(clock, icloud: icloud)
        f.getStarted(); f.profileNext()
        f.setUsePassword(true); f.setPassword("correct horse battery staple")
        XCTAssertTrue(f.backupToICloud())
        let phrase = try r.core.recoveryPhrase()

        let (f2, _) = flow(clock, icloud: icloud)
        f2.detectBackup()
        for _ in 0..<RetryLimiter.freeTries {
            f2.setRestorePassword("nope")
            XCTAssertFalse(f2.restoreFromCloud())
            XCTAssertEqual(f2.state.error, "wrongpw")
        }
        XCTAssertGreaterThan(f2.state.restoreLockMs, 0)
        f2.setRestorePassword("correct horse battery staple")
        XCTAssertFalse(f2.restoreFromCloud(), "locked out")
        clock.ms += RetryLimiter.baseMs
        XCTAssertTrue(f2.restoreFromCloud())

        let (f3, r3) = flow(clock, icloud: InMemoryCloudBackupStore(method: backupICloud, available: false))
        f3.usePhraseInstead()
        f3.setRestorePhrase("  " + phrase.uppercased() + " ")
        XCTAssertTrue(f3.restoreFromPhrase())
        XCTAssertEqual(try r3.core.recoveryPhrase(), phrase)
    }

    /// O6: skip → banner for 24 h → required.
    func testO6_remindLaterBannerThenBlock() {
        let clock = Clock()
        let (f, r) = flow(clock)
        f.getStarted(); f.profileNext(); f.remindLater(); f.finish()
        XCTAssertEqual(r.backupGate(), .banner)
        clock.ms += remindWindowMs - 1
        XCTAssertEqual(r.backupGate(), .banner)
        clock.ms += 1
        XCTAssertEqual(r.backupGate(), .required)
        r.markBackupMethod(backupWritten)
        XCTAssertEqual(r.backupGate(), .ok)
    }

    func testWriteItDownQuiz() {
        let (f, r) = flow()
        f.getStarted(); f.profileNext(); f.writeItDown()
        XCTAssertEqual(f.state.words.count, 24)
        f.startQuiz()
        let p = f.state.quiz.map(\.position)
        XCTAssertEqual(p.count, 3)
        XCTAssertFalse(zip(p, p.dropFirst()).contains { $1 - $0 == 1 })
        XCTAssertFalse(f.submitQuiz())
        XCTAssertTrue(f.state.quiz.allSatisfy(\.error))
        for (i, q) in f.state.quiz.enumerated() { f.setQuizInput(i, f.state.words[q.position].uppercased()) }
        XCTAssertTrue(f.submitQuiz())
        XCTAssertTrue(r.store.backupMethods.contains(backupWritten))
        XCTAssertTrue(f.state.words.isEmpty, "words are cleared after the quiz")
    }

    func testPickPositionsNeverAllInOneRow() {
        let (f, _) = flow()
        for _ in 0..<500 {
            let p = f.pickPositions()
            XCTAssertGreaterThan(Set(p.map { $0 / 3 }).count, 1)
        }
    }

    /// Settings › Backup: delete removes the remote record and the key (O5 plumbing).
    func testO5_deleteCloudBackup() throws {
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = InMemoryBackupKeyStore()
        let (r, _) = makeRepo()
        try r.saveIdentity(phrase: try r.core.generateRecoveryPhrase(), name: "A")
        let s = BackupSettings(repo: r, stores: [icloud], keyStore: keys)
        try s.backUp(backupICloud, password: nil)
        XCTAssertNotNil(icloud.blob)
        try s.deleteCloudBackup(backupICloud)
        XCTAssertNil(icloud.blob)
        XCTAssertNil(keys.key)
        XCTAssertFalse(s.methods.contains(backupICloud))
        XCTAssertThrowsError(try s.changePassword(backupICloud, to: "short"))
    }
    func testPreparedUploadDoesNotCommitUntilOwnerAcceptsSuccess() throws {
        let cloud = InMemoryCloudBackupStore(method: backupICloud)
        let (f, r) = flow(icloud: cloud)
        f.getStarted(); f.profileNext()
        let upload = try f.prepareBackup(backupICloud)
        XCTAssertTrue(r.store.backupMethods.isEmpty)
        XCTAssertEqual(r.store.lastCloudBackupMs, 0)
        try upload.upload(to: cloud)
        XCTAssertNotNil(cloud.blob)
        XCTAssertFalse(f.state.iCloudDone)
        XCTAssertTrue(r.store.backupMethods.isEmpty, "worker I/O never mutates repository state")
        f.completeBackup(backupICloud, succeeded: true)
        XCTAssertTrue(f.state.iCloudDone)
        XCTAssertEqual(r.backupGate(), .ok)
    }

    func testFailedPreparedUploadDoesNotUnlockBackupGate() throws {
        let (f, r) = flow()
        f.getStarted(); f.profileNext()
        _ = try f.prepareBackup(backupICloud)
        f.completeBackup(backupICloud, succeeded: false)
        XCTAssertFalse(f.state.iCloudDone)
        XCTAssertEqual(f.state.error, "backup")
        XCTAssertEqual(r.store.lastCloudBackupMs, 0)
        XCTAssertEqual(r.backupGate(), .required)
    }

    func testPreparedUploadRejectsWrongDestination() throws {
        let (f, _) = flow()
        f.getStarted(); f.profileNext()
        let upload = try f.prepareBackup(backupICloud)
        let drive = InMemoryCloudBackupStore(method: backupDrive)
        XCTAssertThrowsError(try upload.upload(to: drive))
        XCTAssertNil(drive.blob)
    }

    func testLateBackupSearchCannotReplaceNewerNavigation() {
        let (f, _) = flow()
        f.getStarted()
        f.acceptBackupSearch([(backupICloud, Data("Kblob".utf8))])
        XCTAssertEqual(f.state.step, .profile)
        XCTAssertNil(f.state.foundBackup)
    }

    func testCancelRestoreClearsTypedSecrets() {
        let (f, _) = flow()
        f.setRestorePassword("private password")
        f.usePhraseInstead()
        XCTAssertTrue(f.state.restorePassword.isEmpty)
        f.setRestorePhrase("private recovery words")
        f.backToWelcome()
        XCTAssertTrue(f.state.restorePhrase.isEmpty)
        f.usePhraseInstead()
        XCTAssertTrue(f.state.restorePhrase.isEmpty)
    }

    func testUnavailableRecoveryWordsCannotEnterQuizOrCrash() {
        let (f, _) = flow()
        f.writeItDown()
        XCTAssertEqual(f.state.step, .welcome)
        XCTAssertEqual(f.state.error, "identity")
        XCTAssertTrue(f.state.words.isEmpty)
        f.startQuiz()
        XCTAssertTrue(f.state.quiz.isEmpty)
        f.setQuizInput(0, "anything")
        XCTAssertFalse(f.submitQuiz())
    }

    func testCancelledWordDisplayClearsWordsAndQuiz() {
        let (f, _) = flow()
        f.getStarted(); f.profileNext(); f.writeItDown(); f.startQuiz()
        f.backToSecure()
        XCTAssertTrue(f.state.words.isEmpty)
        XCTAssertTrue(f.state.quiz.isEmpty)
        f.startQuiz()
        XCTAssertEqual(f.state.step, .secure)
    }

    final class FailingDeleteKeyStore: BackupKeyStore {
        var key: Data?
        func load() throws -> Data? { key }
        func save(_ key: Data) throws { self.key = key }
        func delete() throws { throw CoreError.other("keychain unavailable") }
    }

    func testDeletedCloudBackupIsNotCountedWhenKeyCleanupFails() throws {
        let cloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = FailingDeleteKeyStore()
        let (r, _) = makeRepo()
        try r.saveIdentity(phrase: try r.core.generateRecoveryPhrase(), name: "A")
        let s = BackupSettings(repo: r, stores: [cloud], keyStore: keys)
        try s.backUp(backupICloud, password: nil)
        XCTAssertEqual(s.gate, .ok)
        XCTAssertThrowsError(try s.deleteCloudBackup(backupICloud))
        XCTAssertNil(cloud.blob)
        XCTAssertFalse(s.methods.contains(backupICloud))
        XCTAssertEqual(s.gate, .required)
    }

}
