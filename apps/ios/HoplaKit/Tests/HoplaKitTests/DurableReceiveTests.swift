import XCTest
#if canImport(Darwin)
import Darwin
#else
import Glibc
#endif
@testable import HoplaKit

final class DurableReceiveTests: XCTestCase {
    private func temporaryRoot() throws -> URL {
        let anchor = try DurableReceivedFiles.physicalTrustedDirectory(FileManager.default.temporaryDirectory)
        let root = anchor.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        return root
    }
    func testTrustedAnchorPhysicalPathAndUntrustedChildAliasStayDistinct() throws {
        #if canImport(Darwin)
        // Foundation may return /var even after resolving symlinks. Preserve the
        // physical system anchor instead; the sink itself must still reject /var.
        let physicalVar = try DurableReceivedFiles.physicalTrustedDirectory(URL(fileURLWithPath: "/var"))
        XCTAssertEqual(physicalVar.path, "/private/var")
        #endif
        let base = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: base) }
        let target = base.appendingPathComponent("target", isDirectory: true)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)
        let alias = base.appendingPathComponent("untrusted-child", isDirectory: true)
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        let (repo, core, r, _) = try fixture(size: 1)
        repo.fileSink = DurableReceivedFiles(root: alias)
        repo.onFileReceived(r)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: target.path).isEmpty)
        repo.fileSink = DurableReceivedFiles(root: target)
        repo.retryReceivedFiles()
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
    }

    private func receipt(_ size: Int, peer: String, id: String = String(repeating: "a", count: 32)) -> ReceiveReceiptInfo {
        ReceiveReceiptInfo(transferId: id, contactId: peer, name: "../../private.bin", mime: "application/octet-stream",
            size: UInt64(size), receiptToken: id + String(repeating: "b", count: 64),
            contentHash: String(repeating: "c", count: 64), blockCount: UInt32((size + 65_535) / 65_536))
    }
    private func fixture(size: Int = 65_537, mine: Bool = false) throws -> (MeshRepository, FakeOmcpCore, ReceiveReceiptInfo, Data) {
        let (repo, core) = makeRepo()
        try repo.saveIdentity(phrase: core.generateRecoveryPhrase(), name: "Me")
        let peer = try repo.addContact(qr: "omcp:1:b0b:Bob", action: .chatWithoutVerifying)
        let bytes = Data((0..<size).map { UInt8($0 % 251) })
        let r = receipt(size, peer: peer.id)
        core.injectReceipt(r, bytes: bytes, mine: mine)
        return (repo, core, r, bytes)
    }
    func testBoundedStreamingAndReadbackAtBlockBoundaries() throws {
        for size in [0, 1, 65_535, 65_536, 65_537] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, expected) = try fixture(size: size, mine: true)
            repo.fileSink = DurableReceivedFiles(root: root)
            repo.onFileReceived(r)
            let message = try XCTUnwrap(repo.messages[r.contactId]?.first)
            XCTAssertTrue(message.mine, "own-sent refetch must retain direction")
            XCTAssertEqual(message.file?.state, .received)
            let path = try XCTUnwrap(message.file?.localPath)
            XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), expected)
            XCTAssertEqual(URL(fileURLWithPath: path).lastPathComponent, r.receiptToken + ".bin")
            XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        }
    }
    func testNilSinkAndEnumerationFailureNeverClaimReceived() throws {
        let (repo, core, r, _) = try fixture()
        repo.onFileReceived(r)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        core.receiptEnumerationError = .storageUnavailable("locked")
        repo.retryReceivedFiles()
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
    }
    func testEveryFilesystemFailureRetainsReceiptUntilRetry() throws {
        for fault in [DurableReceivedFiles.Checkpoint.beforeWrite, .beforeReadback, .beforeFileSync, .beforeRename, .afterRename, .beforeUnlink, .beforeDirectorySync] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, _) = try fixture()
            repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
                if point == fault { throw CoreError.storageUnavailable("injected I/O fault") }
            })
            repo.onFileReceived(r)
            XCTAssertEqual(try core.pendingReceivedFiles(), [r])
            XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
            repo.fileSink = DurableReceivedFiles(root: root)
            repo.retryReceivedFiles()
            XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
            XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .received)
        }
    }
    func testCommitFailureRecoversRenamedFileAtStartupWithoutResavingBytes() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, expected) = try fixture()
        repo.fileSink = DurableReceivedFiles(root: root)
        core.receiptCommitError = .storageUnavailable("commit failed")
        repo.onFileReceived(r)
        let final = root.appendingPathComponent(r.receiptToken + ".bin")
        XCTAssertEqual(try Data(contentsOf: final), expected)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        core.receiptCommitError = nil
        let restarted = MeshRepository(core: core, store: repo.store)
        restarted.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
            if point == .beforeWrite { XCTFail("existing valid final file must be reused") }
        })
        XCTAssertTrue(restarted.restore())
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        XCTAssertEqual(restarted.messages[r.contactId]?.first?.file?.localPath, final.path)
    }
    func testCancellationPersistenceFailurePreservesPendingAndLateEventCannotResurrect() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        repo.onFileReceived(r) // No sink: load Finalizing state.
        core.receiptCancelError = .storageUnavailable("busy")
        repo.cancelFile(r.contactId, transferId: r.transferId)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
        core.receiptCancelError = nil
        repo.cancelFile(r.contactId, transferId: r.transferId)
        repo.fileSink = DurableReceivedFiles(root: root)
        repo.onFileReceived(r)
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .failed)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(r.receiptToken + ".bin").path))
    }
    func testCancelledDuringWriterCannotCommitOrRecreateBubble() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
            if point == .beforeRename { repo.cancelFile(r.contactId, transferId: r.transferId) }
        })
        repo.onFileReceived(r)
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .failed)
    }
    func testSymlinkRootFinalAndMissingAncestorFailClosed() throws {
        let base = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: base) }
        let outside = base.appendingPathComponent("outside")
        try Data("untouched".utf8).write(to: outside)
        let root = base.appendingPathComponent("received", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        let (repo, core, r, _) = try fixture()
        let final = root.appendingPathComponent(r.receiptToken + ".bin")
        try FileManager.default.createSymbolicLink(at: final, withDestinationURL: outside)
        repo.fileSink = DurableReceivedFiles(root: root); repo.onFileReceived(r)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r]); XCTAssertEqual(try String(contentsOf: outside), "untouched")
        XCTAssertThrowsError(try repo.fileSink?.isDefinitelyMissing(final.path))
        let broken = root.appendingPathComponent("broken")
        try FileManager.default.createSymbolicLink(at: broken, withDestinationURL: base.appendingPathComponent("absent"))
        XCTAssertThrowsError(try repo.fileSink?.isDefinitelyMissing(broken.appendingPathComponent("file").path))
        XCTAssertThrowsError(try XCTUnwrap(repo.fileSink).isDefinitelyMissing(root.appendingPathComponent("missing/file").path))
        XCTAssertTrue(try XCTUnwrap(repo.fileSink).isDefinitelyMissing(root.appendingPathComponent("missing").path))
        let alias = base.appendingPathComponent("alias")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: root)
        repo.fileSink = DurableReceivedFiles(root: alias); repo.onFileReceived(r)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
    }
    func testPositiveMissingOnlyRetiresExactCompletedPathAndSymlinkIsNotOpened() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        repo.fileSink = DurableReceivedFiles(root: root); repo.onFileReceived(r)
        let path = try XCTUnwrap(repo.messages[r.contactId]?.first?.file?.localPath)
        try FileManager.default.removeItem(atPath: path)
        try FileManager.default.createSymbolicLink(atPath: path, withDestinationPath: root.appendingPathComponent("absent").path)
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: path))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .received)
        try FileManager.default.removeItem(atPath: path)
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: path))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .missing)
    }
    func testReadbackTamperNeverCommitsAndLeavesVerifiedSpool() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture(size: 8)
        repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
            if point == .beforeReadback { try Data(repeating: 9, count: 8).write(to: root.appendingPathComponent(r.receiptToken + ".part")) }
        })
        repo.onFileReceived(r)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
    }
    func testLegacyBackupUsesSeparateDurableNamespace() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let sink = DurableReceivedFiles(root: root)
        let file = ReceivedFileInfo(transferId: String(repeating: "a", count: 32), contactId: "", name: "x", mime: "a/b", bytes: Data([1,2,3]))
        let path = try XCTUnwrap(sink.save(file))
        XCTAssertTrue(path.hasPrefix(root.appendingPathComponent("legacy").path + "/"))
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), file.bytes)
    }
    func testRacedDestinationNeverOverwrittenAndRetryKeepsSpool() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture(size: 8)
        let final = root.appendingPathComponent(r.receiptToken + ".bin")
        let foreign = Data("foreign!".utf8)
        repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
            if point == .beforeRename { try foreign.write(to: final) }
        })
        repo.onFileReceived(r)
        XCTAssertEqual(try Data(contentsOf: final), foreign)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        repo.fileSink = DurableReceivedFiles(root: root); repo.retryReceivedFiles()
        XCTAssertEqual(try Data(contentsOf: final), foreign)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
    }
    func testMissingContainerNeverRetiresCompletedReceipt() throws {
        let base = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: base) }
        let root = base.appendingPathComponent("received")
        let (repo, core, r, _) = try fixture()
        repo.fileSink = DurableReceivedFiles(root: root); repo.onFileReceived(r)
        let path = try XCTUnwrap(repo.messages[r.contactId]?.first?.file?.localPath)
        try FileManager.default.moveItem(at: root, to: base.appendingPathComponent("moved"))
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: path))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .received)
    }
    func testCompletedHistorySurvivesReciprocalCancelAndStaleFailure() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture(mine: true)
        repo.fileSink = DurableReceivedFiles(root: root); repo.onFileReceived(r)
        let original = try XCTUnwrap(repo.messages[r.contactId]?.first)
        core.cancelPreservingReceived = true
        repo.cancelFile(r.contactId, transferId: r.transferId)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file, original.file)
        repo.onFileFailed(FileFailureInfo(transferId: r.transferId, contactId: r.contactId, reason: "integrity"))
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file, original.file)
    }
    func testRetryCandidateFailureStillFinalizesDurableReceipts() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        core.receiptRetryError = .storageUnavailable("another candidate failed")
        repo.fileSink = DurableReceivedFiles(root: root); repo.retryReceivedFiles()
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .received)
    }
    func testSchedulingDoesNotRunFileIOOnCallerAndLifecycleFailsBusy() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (_, core, r, _) = try fixture(size: 1)
        let store = InMemoryAppStore()
        let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        let finished = expectation(description: "receipt finalizer finishes")
        let queue = DispatchQueue(label: "receipt-test")
        let repo = MeshRepository(core: core, store: store, receiveExecutor: { work in
            queue.async { work(); finished.fulfill() }
        }, receivePublisher: { _ in })
        repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { point in
            if point == .beforeWrite {
                XCTAssertFalse(Thread.isMainThread)
                entered.signal(); _ = release.wait(timeout: .now() + 5)
            }
        })
        repo.onFileReceived(r)
        XCTAssertEqual(entered.wait(timeout: .now() + 5), .success)
        XCTAssertThrowsError(try repo.saveIdentity(phrase: "must not replace", name: "Other"))
        XCTAssertFalse(repo.restore())
        release.signal()
        wait(for: [finished], timeout: 5)
    }

    func testCancelAfterCommitFailureRemovesOnlyCancelledGeneration() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        let unrelated = root.appendingPathComponent("unrelated")
        try Data([7]).write(to: unrelated)
        repo.fileSink = DurableReceivedFiles(root: root)
        core.receiptCommitError = .storageUnavailable("busy")
        repo.onFileReceived(r)
        let final = root.appendingPathComponent(r.receiptToken + ".bin")
        XCTAssertTrue(FileManager.default.fileExists(atPath: final.path))
        repo.cancelFile(r.contactId, transferId: r.transferId)
        XCTAssertFalse(FileManager.default.fileExists(atPath: final.path))
        XCTAssertEqual(try Data(contentsOf: unrelated), Data([7]))
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
    }

    func testCleanupAcknowledgementFailureSurvivesRestartAndRetriesWithoutResurrection() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, _) = try fixture()
        repo.fileSink = DurableReceivedFiles(root: root)
        core.receiptCommitError = .storageUnavailable("commit failed")
        repo.onFileReceived(r)
        core.receiptCleanupAckError = .storageUnavailable("ack failed")
        repo.cancelFile(r.contactId, transferId: r.transferId)
        XCTAssertEqual(try core.cancelledReceivedFiles(), [r])
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent(r.receiptToken + ".bin").path))
        core.receiptCleanupAckError = nil
        core.receiptRetryError = .storageUnavailable("another candidate failed")
        let restarted = MeshRepository(core: core, store: repo.store)
        restarted.fileSink = DurableReceivedFiles(root: root)
        XCTAssertTrue(restarted.restore())
        XCTAssertTrue(try core.cancelledReceivedFiles().isEmpty)
        XCTAssertEqual(restarted.messages[r.contactId]?.first?.file?.state, .failed)
        restarted.onFileReceived(r)
        XCTAssertEqual(restarted.messages[r.contactId]?.first?.file?.state, .failed)
    }

    func testLegacyLexicalRootReadsOpensAndUsesOriginalMissingCAS() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let alias = "/var/mobile/Containers/Data/Application/current/Library/Application Support/received"
        let parent = root.appendingPathComponent("old-transfer")
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: false)
        let physical = parent.appendingPathComponent("photo.jpg")
        let original = alias + "/old-transfer/photo.jpg"
        let (repo, core, r, bytes) = try fixture(size: 3)
        try bytes.write(to: physical)
        try core.commitReceivedFile(r.receiptToken, path: original)
        let sink = DurableReceivedFiles(root: root, trustedLexicalRoot: alias)
        repo.fileSink = sink
        XCTAssertEqual(try sink.readForResend(original), bytes)
        XCTAssertEqual(repo.receivedFileForOpening(r.transferId, path: original), physical.path)
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.localPath, original)
        // An unavailable/non-directory ancestor must not retire the old path.
        let moved = root.appendingPathComponent("moved")
        try FileManager.default.moveItem(at: parent, to: moved)
        try Data([9]).write(to: parent)
        XCTAssertThrowsError(try sink.isDefinitelyMissing(original))
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: original))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .received)
        try FileManager.default.removeItem(at: parent)
        try FileManager.default.moveItem(at: moved, to: parent)
        try FileManager.default.removeItem(at: physical)
        XCTAssertTrue(try sink.isDefinitelyMissing(original))
        XCTAssertNil(try sink.readForResend(original))
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: original))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .missing,
                       "CAS must use original DB spelling, not mapped physical path")
    }

    func testLegacyMappingRejectsForeignTraversalLookalikesAndSymlinkDescendants() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let alias = "/var/mobile/Containers/Data/Application/current/Library/Application Support/received"
        let sink = DurableReceivedFiles(root: root, trustedLexicalRoot: alias)
        for path in [alias, alias + "/", alias + "-other/file", alias + "/../file", alias + "/./file",
                     alias + "//file", alias + "/file\0suffix", alias.replacingOccurrences(of: "/current/", with: "/foreign/") + "/file"] {
            XCTAssertThrowsError(try sink.isDefinitelyMissing(path), path)
            XCTAssertThrowsError(try sink.readForResend(path), path)
            XCTAssertThrowsError(try sink.validatedPathForOpening(path), path)
        }
        let outside = root.appendingPathComponent("outside")
        try Data([1]).write(to: outside)
        let link = root.appendingPathComponent("link")
        try FileManager.default.createSymbolicLink(at: link, withDestinationURL: outside)
        XCTAssertThrowsError(try sink.readForResend(alias + "/link"))
        XCTAssertThrowsError(try sink.validatedPathForOpening(alias + "/link"))
        XCTAssertThrowsError(try sink.isDefinitelyMissing(alias + "/link/child"))
        XCTAssertThrowsError(try DurableReceivedFiles(root: root).readForResend(alias + "/outside"),
                             "arbitrary injected roots must not acquire alias compatibility")
        XCTAssertEqual(try Data(contentsOf: outside), Data([1]))
    }

    func testLegacyPermissionErrorDoesNotBecomeMissing() throws {
        guard getuid() != 0 else { throw XCTSkip("root bypasses directory permission denial") }
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let parent = root.appendingPathComponent("old-transfer")
        try FileManager.default.createDirectory(at: parent, withIntermediateDirectories: false)
        let alias = "/var/mobile/Containers/Data/Application/current/Library/Application Support/received"
        let original = alias + "/old-transfer/photo.jpg"
        let (repo, core, r, bytes) = try fixture(size: 3)
        try bytes.write(to: parent.appendingPathComponent("photo.jpg"))
        try core.commitReceivedFile(r.receiptToken, path: original)
        let sink = DurableReceivedFiles(root: root, trustedLexicalRoot: alias)
        repo.fileSink = sink
        try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: parent.path)
        defer { try? FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: parent.path) }
        XCTAssertThrowsError(try sink.isDefinitelyMissing(original))
        XCTAssertThrowsError(try sink.readForResend(original))
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: original))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .received)
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.localPath, original)
    }

    func testUnsupportedSinkCannotAuthorizeOpeningOrDowngradeStoredPath() throws {
        final class UnsupportedSink: FileSink {
            func save(_ file: ReceivedFileInfo) -> String? { nil }
        }
        let (repo, core, r, _) = try fixture(size: 1)
        let original = "/unverified/path"
        try core.commitReceivedFile(r.receiptToken, path: original)
        let sink = UnsupportedSink()
        repo.fileSink = sink
        XCTAssertFalse(try sink.isDefinitelyMissing(original)) // Unsupported is not proof of presence.
        XCTAssertThrowsError(try sink.validatedPathForOpening(original))
        XCTAssertNil(repo.receivedFileForOpening(r.transferId, path: original))
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.state, .received)
        XCTAssertEqual(core.storedMessages(r.contactId).first?.file?.localPath, original)
    }

    private func corruptCandidate(root: URL) throws -> (MeshRepository, FakeOmcpCore, ReceiveReceiptInfo, Data, URL, LocalReceiptRepairAction) {
        let (repo, core, r, bytes) = try fixture(size: 8)
        let path = root.appendingPathComponent(r.receiptToken + ".bin")
        try Data(repeating: 9, count: 8).write(to: path)
        repo.fileSink = DurableReceivedFiles(root: root)
        repo.onFileReceived(r)
        return (repo, core, r, bytes, path, try XCTUnwrap(repo.localReceiptRepairs[r.transferId]))
    }

    func testRepairRequiresExplicitActionAndNeverExposesCorruptCandidate() throws {
        let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
        let (repo, core, r, bytes, path, action) = try corruptCandidate(root: root)
        repo.retryReceivedFiles()
        XCTAssertEqual(try Data(contentsOf: path), Data(repeating: 9, count: 8))
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .finalizing)
        XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
        XCTAssertEqual(try core.pendingReceivedFiles(), [r])
        repo.repairLocalReceivedFile(action)
        XCTAssertEqual(try Data(contentsOf: path), bytes)
        XCTAssertEqual(repo.messages[r.contactId]?.first?.file?.state, .received)
        XCTAssertTrue(try core.pendingReceivedFiles().isEmpty)
        XCTAssertNil(repo.localReceiptRepairs[r.transferId])
        XCTAssertEqual(core.queuedFor(r.contactId), 0, "repair must not request network retransmission")
        repo.repairLocalReceivedFile(action) // Already completed: no mutation.
        XCTAssertEqual(try Data(contentsOf: path), bytes)
    }

    func testRepairSpoolValidationFailureNeverDeletesCandidate() throws {
        for error in [CoreError.storageUnavailable("sealed storage locked"), .invalid("sealed hash mismatch"), .localReceiveContentMismatch] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, _, path, action) = try corruptCandidate(root: root)
            core.repairValidationError = error
            repo.repairLocalReceivedFile(action)
            XCTAssertEqual(try Data(contentsOf: path), Data(repeating: 9, count: 8))
            XCTAssertEqual(try core.pendingReceivedFiles(), [r])
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
        }
    }

    func testRepairRejectsChangedLengthSymlinkAndSharedPublicationPair() throws {
        for kind in ["length", "symlink", "hardlink"] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, _, path, action) = try corruptCandidate(root: root)
            if kind == "length" { try Data([9]).write(to: path) }
            if kind == "symlink" {
                try FileManager.default.removeItem(at: path)
                try FileManager.default.createSymbolicLink(at: path, withDestinationURL: root.appendingPathComponent("absent"))
            }
            if kind == "hardlink" { try FileManager.default.linkItem(at: path, to: root.appendingPathComponent(r.receiptToken + ".part")) }
            repo.repairLocalReceivedFile(action)
            XCTAssertEqual(try core.pendingReceivedFiles(), [r])
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
            if kind == "symlink" { XCTAssertNoThrow(try FileManager.default.destinationOfSymbolicLink(atPath: path.path)) }
            else { XCTAssertEqual(try Data(contentsOf: path), kind == "length" ? Data([9]) : Data(repeating: 9, count: 8)) }
        }
    }

    func testRepairRejectsIdentityReloadCancellationAndReplacementInode() throws {
        for kind in ["reload", "cancel", "inode"] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, _, path, action) = try corruptCandidate(root: root)
            if kind == "reload" { XCTAssertTrue(repo.restore()) }
            repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { checkpoint in
                if checkpoint == .beforeRepairDelete {
                    if kind == "cancel" { _ = try core.cancelFile(r.transferId) }
                    if kind == "inode" { try Data(repeating: 7, count: 8).write(to: path, options: .atomic) }
                }
            })
            repo.repairLocalReceivedFile(action)
            XCTAssertEqual(try Data(contentsOf: path), Data(repeating: kind == "inode" ? 7 : 9, count: 8))
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
        }
    }

    func testRepairCommitFailureAndInterruptedRemovalRecoverAfterRestart() throws {
        for kind in ["commit", "after-delete"] {
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, bytes, path, action) = try corruptCandidate(root: root)
            if kind == "commit" { core.receiptCommitError = .storageUnavailable("commit interrupted") }
            repo.fileSink = DurableReceivedFiles(root: root, checkpoint: { checkpoint in
                if kind == "after-delete", checkpoint == .afterRepairDelete { throw CoreError.storageUnavailable("interrupted") }
            })
            repo.repairLocalReceivedFile(action)
            XCTAssertEqual(try core.pendingReceivedFiles(), [r])
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
            core.receiptCommitError = nil
            let restarted = MeshRepository(core: core, store: repo.store)
            restarted.fileSink = DurableReceivedFiles(root: root)
            XCTAssertTrue(restarted.restore())
            XCTAssertEqual(try Data(contentsOf: path), bytes)
            XCTAssertEqual(restarted.messages[r.contactId]?.first?.file?.state, .received)
        }
    }

    func testRepairDoesNotDeleteNowValidOrUnreadableCandidate() throws {
        for unreadable in [false, true] {
            if unreadable && getuid() == 0 { continue } // Root cannot model permission denial.
            let root = try temporaryRoot(); defer { try? FileManager.default.removeItem(at: root) }
            let (repo, core, r, bytes, path, action) = try corruptCandidate(root: root)
            if unreadable { try FileManager.default.setAttributes([.posixPermissions: 0], ofItemAtPath: path.path) }
            else { try bytes.write(to: path) }
            repo.repairLocalReceivedFile(action)
            if unreadable { try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path.path) }
            XCTAssertEqual(try Data(contentsOf: path), unreadable ? Data(repeating: 9, count: 8) : bytes)
            XCTAssertEqual(try core.pendingReceivedFiles(), [r])
            XCTAssertNil(repo.messages[r.contactId]?.first?.file?.localPath)
        }
    }

}
