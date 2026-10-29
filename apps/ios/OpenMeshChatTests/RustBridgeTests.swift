import Foundation
import XCTest
import HoplaKit
#if canImport(HoplaRust)
import HoplaRust // Linux: scripts/verify-linux.sh
#else
@testable import OpenMeshChat // iOS simulator (xcodebuild test)
#endif

/// Real Rust core through the Swift bindings: two nodes, QR add with safety numbers, a Noise
/// link over in-memory "BLE", burn after read with OPENED, and iCloud/Drive backups (O2–O4).
final class RustBridgeTests: XCTestCase {
    private func durableSink(root: URL) -> FileSink {
        #if canImport(HoplaRust)
        return DurableReceivedFiles(root: root)
        #else
        return ReceivedFiles(root: root) // Exercise Apple's backup-exclusion wrapper too.
        #endif
    }

    func testInjectedNativeSinkRootDoesNotResolveChildSymlink() throws {
        let anchor = try DurableReceivedFiles.physicalTrustedDirectory(FileManager.default.temporaryDirectory)
        let root = anchor.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: root) }
        let target = root.appendingPathComponent("target")
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: false)
        let alias = root.appendingPathComponent("untrusted")
        try FileManager.default.createSymbolicLink(at: alias, withDestinationURL: target)
        let sink = durableSink(root: alias)
        XCTAssertNil(sink.save(ReceivedFileInfo(transferId: String(repeating: "a", count: 32),
            contactId: "", name: "file", mime: "application/octet-stream", bytes: Data([1]))))
        XCTAssertTrue(try FileManager.default.contentsOfDirectory(atPath: target.path).isEmpty)
    }

    func node(_ name: String) throws -> (RustOmcpCore, MeshRepository) {
        let core = RustOmcpCore()
        let repo = MeshRepository(core: core, store: InMemoryAppStore())
        try repo.saveIdentity(phrase: try core.generateRecoveryPhrase(), name: name)
        return (core, repo)
    }

    /// Simulate OS acceptance after all PDUs in a batch, including receive-before-callback
    /// interleavings. Commit only retires sender work; this pump separately models peer receipt.
    func pump(_ a: CoreLink, _ b: CoreLink, _ first: LinkResult, into ra: MeshRepository, _ rb: MeshRepository,
              observe: ((Bool, LinkResult) -> Void)? = nil) throws {
        func apply(_ output: LinkResult, toB: Bool) {
            let repo = toB ? rb : ra
            output.received.forEach(repo.onReceived)
            output.opened.forEach(repo.onOpened)
            repo.applyFileEvents(output)
            if let peer = output.peerContactId { repo.markFlushed(peer, count: output.flushed) }
            observe?(toB, output)
        }
        apply(first, toB: false)
        var queue: [(toB: Bool, output: LinkResult)] = first.outgoing.isEmpty ? [] : [(true, first)]
        while !queue.isEmpty {
            let (toB, output) = queue.removeFirst()
            let sender = toB ? a : b
            let receiver = toB ? b : a
            for pdu in output.outgoing {
                let received = try receiver.onPdu(pdu)
                apply(received, toB: toB)
                if !received.outgoing.isEmpty { queue.append((!toB, received)) }
            }
            let token = try XCTUnwrap(output.batchToken)
            let committed = try sender.commitOutgoing(token)
            apply(committed, toB: !toB)
            if committed.morePending {
                let next = try sender.flush()
                apply(next, toB: !toB)
                if !next.outgoing.isEmpty { queue.append((toB, next)) }
            }
        }
    }

    /// Encrypted local store: contacts, verification, chat, queue and an unopened burn message
    /// survive an app restart; once burned it is gone after the next restart and never on disk.
    func testMessagesQueueAndBurnSurviveRestart() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("omcp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appendingPathComponent("omcp-store.db").path
        let store = InMemoryAppStore()
        var t = Int64(Date().timeIntervalSince1970 * 1000)
        func bob() -> (RustOmcpCore, MeshRepository) {
            let c = RustOmcpCore()
            return (c, MeshRepository(core: c, store: store, dbPath: db, clock: { t }))
        }
        let (ca, ra) = try node("Ana")
        var (cb, rb) = bob()
        try rb.saveIdentity(phrase: try cb.generateRecoveryPhrase(), name: "Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .chatWithoutVerifying)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .verifyAndChat)
        XCTAssertTrue(store.contacts.isEmpty) // encrypted store, not UserDefaults
        XCTAssertNotNil(store.storageKey)
        let secret = "burn: meet at gate 7"
        try ra.send(bOnA.id, text: "hello Ben")
        try ra.send(bOnA.id, text: secret, burnOnce: .timer(10))
        var la = try ca.openInitiatorLink(bOnA.id)
        var lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        try rb.send(aOnB.id, text: "queued for Ana")
        la.close(); lb.close()

        (cb, rb) = bob()
        XCTAssertTrue(rb.restore())
        XCTAssertEqual(rb.contacts.first?.verified, true)
        let chat = rb.messages[aOnB.id] ?? []
        XCTAssertEqual(chat.map(\.text), ["hello Ben", "", "queued for Ana"])
        XCTAssertEqual(chat[1].burnState, .unopened)
        XCTAssertEqual(chat[2].status, .queued)
        XCTAssertEqual(cb.queuedFor(aOnB.id), 1)
        XCTAssertEqual(try cb.openBurn(chat[1].id, nowMs: t)?.text, secret)
        t += 10_000
        XCTAssertEqual(cb.tickBurn(nowMs: t), [chat[1].id])

        (cb, rb) = bob()
        XCTAssertTrue(rb.restore())
        XCTAssertEqual((rb.messages[aOnB.id] ?? []).map(\.text), ["hello Ben", "queued for Ana"])
        XCTAssertFalse(cb.burnIsLive(chat[1].id))
        la = try ca.openInitiatorLink(bOnA.id)
        lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        XCTAssertEqual(ra.messages[bOnA.id]?.last?.text, "queued for Ana")
        XCTAssertEqual(cb.queuedFor(aOnB.id), 0)
        la.close(); lb.close()
        for f in try FileManager.default.contentsOfDirectory(atPath: dir.path) {
            let bytes = try Data(contentsOf: dir.appendingPathComponent(f))
            XCTAssertNil(bytes.range(of: Data(secret.utf8)), "burn content on disk in \(f)")
            XCTAssertNil(bytes.range(of: Data("hello Ben".utf8)), "plaintext on disk in \(f)")
        }
    }

    func testUnreadableStoreIsPreservedAndCanRetry() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("omcp-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: dir) }
        let db = dir.appendingPathComponent("omcp-store.db").path
        let store = InMemoryAppStore()
        let c1 = RustOmcpCore()
        try MeshRepository(core: c1, store: store, dbPath: db).saveIdentity(phrase: try c1.generateRecoveryPhrase(), name: "Ben")
        let originalKey = try XCTUnwrap(store.storageKey)
        let originalBytes = try Data(contentsOf: URL(fileURLWithPath: db))
        store.storageKey = Data(repeating: 7, count: 32) // e.g. Keychain reset
        let repo = MeshRepository(core: RustOmcpCore(), store: store, dbPath: db)
        XCTAssertFalse(repo.restore())
        XCTAssertTrue(repo.storageBlocked)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: db)), originalBytes)
        XCTAssertFalse(try FileManager.default.contentsOfDirectory(atPath: dir.path).contains { $0.contains(".unreadable-") })
        store.storageKey = originalKey
        XCTAssertTrue(repo.restore())
        XCTAssertFalse(repo.storageBlocked)
    }

    func testLinkedAndPolicyFromCore() {
        let c = RustOmcpCore()
        XCTAssertTrue(c.linked)
        XCTAssertEqual(c.maxFileBytes(), 50_000_000)
        XCTAssertEqual(c.gatewayDailyCapBytes(), 500_000_000)
        XCTAssertFalse(c.gatewayDecision(wifiOnly: true, network: .cellular, metered: true).bridge)
        XCTAssertTrue(c.gatewayDecision(wifiOnly: false, network: .cellular, metered: true).meteredWarning)
    }

    final class MemSink: FileSink {
        var bytes: Data?
        func save(_ file: ReceivedFileInfo) -> String? { bytes = file.bytes; return "mem:\(file.name)" }
        func finalize(_ receipt: ReceiveReceiptInfo, readBlock: (UInt32) throws -> Data, verifier: ReceiveFileVerifier) throws -> String {
            var all = Data()
            for i in 0..<receipt.blockCount { let block = try readBlock(i); try verifier.update(block); all.append(block) }
            try verifier.finish(); bytes = all; return "mem:\(receipt.name)"
        }
    }

    func testDurableReceiptRestoresFromStoreAndFinalizesWithRealVerifier() throws {
        let root = try DurableReceivedFiles.physicalTrustedDirectory(FileManager.default.temporaryDirectory)
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let canonical = root
        let db = canonical.appendingPathComponent("store.sqlite").path
        let store = InMemoryAppStore()
        let receiver = RustOmcpCore()
        let before = MeshRepository(core: receiver, store: store, dbPath: db)
        try before.saveIdentity(phrase: try receiver.generateRecoveryPhrase(), name: "Receiver")
        let (sender, sent) = try node("Sender")
        let peer = try sent.addContact(qr: try receiver.qrPayload(), action: .chatWithoutVerifying)
        _ = try before.addContact(qr: try sender.qrPayload(), action: .chatWithoutVerifying)
        let a = try sender.openInitiatorLink(peer.id), b = try receiver.openResponderLink()
        try pump(a, b, try a.start(), into: sent, before)
        let bytes = Data((0..<65_537).map { UInt8($0 % 239) })
        let id = try sent.sendFile(peer.id, name: "../photo.bin", mime: "application/octet-stream", bytes: bytes)
        try pump(a, b, try a.flush(), into: sent, before)
        let receipt = try XCTUnwrap(receiver.pendingReceivedFiles().first)
        XCTAssertEqual(receipt.transferId, id)
        XCTAssertEqual(before.messages.values.flatMap { $0 }.first?.file?.state, .finalizing)
        a.close(); b.close()
        let reopened = RustOmcpCore()
        let after = MeshRepository(core: reopened, store: store, dbPath: db)
        after.fileSink = durableSink(root: canonical.appendingPathComponent("received"))
        XCTAssertTrue(after.restore())
        XCTAssertTrue(try reopened.pendingReceivedFiles().isEmpty)
        let received = try XCTUnwrap(after.messages.values.flatMap { $0 }.first { $0.id == id })
        XCTAssertEqual(received.file?.state, .received)
        let path = try XCTUnwrap(received.file?.localPath)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), bytes)
        after.onFileReceived(receipt) // Stale event cannot create another generation.
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: path)), bytes)
    }

    /// File transfer through the real core: 64 KiB chunks drained over several flushes.
    func testFileTransferIn64kChunks() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .chatWithoutVerifying)
        _ = try rb.addContact(qr: try ca.qrPayload(), action: .chatWithoutVerifying)
        let la = try ca.openInitiatorLink(bOnA.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        let data = Data((0..<200_000).map { UInt8($0 % 251) })
        let root = try DurableReceivedFiles.physicalTrustedDirectory(FileManager.default.temporaryDirectory)
            .appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        rb.fileSink = durableSink(root: root)
        let id = try ra.sendFile(bOnA.id, name: "flyer.jpg", mime: "image/jpeg", bytes: data)
        var flushes = 0
        var more = true
        while more {
            let out = try la.flush()
            flushes += 1
            ra.applyFileEvents(out)
            for p in out.outgoing {
                XCTAssertLessThanOrEqual(p.count, 512)
                rb.applyFileEvents(try lb.onPdu(p))
            }
            let committed = try la.commitOutgoing(XCTUnwrap(out.batchToken))
            ra.applyFileEvents(committed)
            more = committed.morePending
        }
        XCTAssertGreaterThanOrEqual(flushes, 2)
        let receivedPath = try XCTUnwrap(rb.messages.values.flatMap { $0 }.first?.file?.localPath)
        XCTAssertEqual(try Data(contentsOf: URL(fileURLWithPath: receivedPath)), data)
        #if !canImport(HoplaRust)
        let values = try URL(fileURLWithPath: receivedPath).deletingLastPathComponent()
            .resourceValues(forKeys: [.isExcludedFromBackupKey])
        XCTAssertEqual(values.isExcludedFromBackup, true)
        #endif
        XCTAssertTrue(try cb.pendingReceivedFiles().isEmpty)
        XCTAssertEqual(rb.messages.values.flatMap { $0 }.first?.file?.state, .received)
        XCTAssertEqual(ra.messages[bOnA.id]?.first { $0.id == id }?.file?.state, .sent)
        XCTAssertThrowsError(try ra.sendFile(bOnA.id, name: "big", mime: "", bytes: Data(count: 50_000_001)))
    }

    func testQrSafetyNumbersAndChatAndBurn() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let pa = try ca.previewContact(try cb.qrPayload())
        XCTAssertEqual(pa.contact.safetyNumber.count, 12)
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .verifyAndChat)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .chatWithoutVerifying)
        XCTAssertEqual(bOnA.safetyNumber, aOnB.safetyNumber)
        XCTAssertTrue(bOnA.showsVerifiedBadge)
        XCTAssertFalse(aOnB.showsVerifiedBadge)

        try ra.send(bOnA.id, text: "hello")
        try ra.send(bOnA.id, text: "burn me", burnOnce: .onOpen())
        let la = try ca.openInitiatorLink(bOnA.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        let got = rb.messages[aOnB.id] ?? []
        XCTAssertEqual(got.map(\.text), ["hello", ""])
        let burnId = got[1].id
        XCTAssertNotNil(got[1].burn)
        XCTAssertTrue(rb.search("burn").isEmpty)

        let opened = try rb.openBurn(aOnB.id, msgId: burnId)
        XCTAssertEqual(opened?.text, "burn me")
        XCTAssertNil(try rb.openBurn(aOnB.id, msgId: burnId), "content only once")
        try rb.closeBurn(aOnB.id, msgId: burnId)
        XCTAssertFalse(cb.burnIsLive(burnId))
        // OPENED receipt flows back and shreds the sender copy.
        try pump(lb, la, try lb.flush(), into: rb, ra)
        XCTAssertFalse(ca.burnIsLive(burnId))
    }

    func testTextBatchAbortAfterPartialTransportAcceptanceReplaysOnNewSession() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let peer = try ra.addContact(qr: cb.qrPayload(), action: .chatWithoutVerifying)
        let sender = try rb.addContact(qr: ca.qrPayload(), action: .chatWithoutVerifying)
        var la = try ca.openInitiatorLink(peer.id)
        var lb = try cb.openResponderLink()
        try pump(la, lb, la.start(), into: ra, rb)
        let text = Array(repeating: "queued until OS acceptance", count: 100).joined(separator: " ")
        try ra.send(peer.id, text: text)
        let batch = try la.flush()
        let token = try XCTUnwrap(batch.batchToken)
        XCTAssertGreaterThan(batch.outgoing.count, 2)
        XCTAssertEqual(batch.flushed, 0)
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
        XCTAssertTrue(try la.flush().outgoing.isEmpty, "pending flush must not reencrypt")
        _ = try lb.onPdu(batch.outgoing[0])
        try la.abortOutgoing(token)
        XCTAssertFalse(la.isEstablished)
        XCTAssertThrowsError(try la.commitOutgoing(token))
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
        lb.close()
        la = try ca.openInitiatorLink(peer.id)
        lb = try cb.openResponderLink()
        try pump(la, lb, la.start(), into: ra, rb)
        XCTAssertEqual(ca.queuedFor(peer.id), 0)
        XCTAssertEqual(rb.messages[sender.id]?.map(\.text), [text])
    }

    func testCommitTokenCannotRetireDifferentBatchAndDoesNotClaimPeerDelivery() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let peer = try ra.addContact(qr: cb.qrPayload(), action: .chatWithoutVerifying)
        _ = try rb.addContact(qr: ca.qrPayload(), action: .chatWithoutVerifying)
        let la = try ca.openInitiatorLink(peer.id)
        let lb = try cb.openResponderLink()
        try pump(la, lb, la.start(), into: ra, rb)
        let first = try la.sendText("OS acceptance is not peer delivery")
        XCTAssertEqual(first.flushed, 0, "sendText also enters the durable queue first")
        let token = try XCTUnwrap(first.batchToken)
        XCTAssertThrowsError(try la.commitOutgoing("wrong-token"))
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
        // A transport may accept bytes that never reach the remote app. No peer receipt is inferred.
        let committed = try la.commitOutgoing(token)
        XCTAssertEqual(committed.flushed, 1)
        XCTAssertEqual(ca.queuedFor(peer.id), 0)
        XCTAssertTrue(rb.messages.isEmpty)
        let duplicate = try la.commitOutgoing(token)
        XCTAssertEqual(duplicate.flushed, 0)
        try ra.send(peer.id, text: "next batch remains queued")
        let next = try la.flush()
        let nextToken = try XCTUnwrap(next.batchToken)
        XCTAssertNotEqual(token, nextToken)
        _ = try? la.commitOutgoing(token) // Either latest-idempotent or rejected; never commits next.
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
        try la.abortOutgoing(nextToken)
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
    }

    func testHandshakeResponseBeforeWriteAcceptanceDoesNotLoseQueuedReply() throws {
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let peer = try ra.addContact(qr: cb.qrPayload(), action: .chatWithoutVerifying)
        let sender = try rb.addContact(qr: ca.qrPayload(), action: .chatWithoutVerifying)
        try ra.send(peer.id, text: "after handshake acceptance")
        let la = try ca.openInitiatorLink(peer.id)
        let lb = try cb.openResponderLink()
        let first = try la.start()
        var reply: LinkResult?
        for pdu in first.outgoing {
            let out = try lb.onPdu(pdu)
            if !out.outgoing.isEmpty { reply = out }
        }
        let response = try XCTUnwrap(reply)
        for pdu in response.outgoing {
            let out = try la.onPdu(pdu)
            XCTAssertTrue(out.outgoing.isEmpty, "start batch still awaits its final OS callback")
        }
        XCTAssertTrue(la.isEstablished)
        XCTAssertTrue(try la.flush().outgoing.isEmpty)
        XCTAssertEqual(ca.queuedFor(peer.id), 1)
        let committed = try la.commitOutgoing(XCTUnwrap(first.batchToken))
        _ = try lb.commitOutgoing(XCTUnwrap(response.batchToken))
        XCTAssertTrue(committed.morePending)
        try pump(la, lb, la.flush(), into: ra, rb)
        XCTAssertEqual(rb.messages[sender.id]?.map(\.text), ["after handshake acceptance"])
        XCTAssertEqual(ca.queuedFor(peer.id), 0)
    }

    /// O2 + O3 + O4 with the real Argon2id / XChaCha20-Poly1305 sealing.
    func testBackupsThroughOnboarding() throws {
        let (core, repo) = try node("Ana")
        _ = try repo.addContact(qr: try RustOmcpCore().qrPayloadFor(name: "Ben"), action: .verifyAndChat)
        let phrase = try core.recoveryPhrase()
        let icloud = InMemoryCloudBackupStore(method: backupICloud)
        let keys = InMemoryBackupKeyStore()
        let settings = BackupSettings(repo: repo, stores: [icloud], keyStore: keys)
        try settings.backUp(backupICloud, password: nil)
        let blob = try XCTUnwrap(icloud.blob)
        XCTAssertNil(blob.range(of: Data(phrase.utf8)), "O2 phrase")
        XCTAssertNil(blob.range(of: try XCTUnwrap(keys.key)), "O2 key")

        // O3: reinstall, restore from iCloud with the Keychain key.
        let core2 = RustOmcpCore()
        let repo2 = MeshRepository(core: core2, store: InMemoryAppStore())
        let f = OnboardingFlow(repo: repo2, iCloud: icloud, drive: nil, keyStore: keys)
        f.detectBackup()
        XCTAssertEqual(f.state.step, .restorePrompt)
        XCTAssertTrue(f.restoreFromCloud())
        XCTAssertEqual(core2.identity()?.peerIdHex, core.identity()?.peerIdHex)
        XCTAssertEqual(repo2.contacts.map(\.verified), [true])

        // O4: password mode, wrong password fails cleanly.
        try settings.changePassword(backupICloud, to: "violet-orbit-71-tundra")
        let f2 = OnboardingFlow(repo: MeshRepository(core: RustOmcpCore(), store: InMemoryAppStore()),
                                iCloud: icloud, drive: nil, keyStore: keys)
        f2.detectBackup()
        XCTAssertEqual(f2.state.foundBackup?.needsPassword, true)
        f2.setRestorePassword("wrong-password-123")
        XCTAssertFalse(f2.restoreFromCloud())
        XCTAssertEqual(f2.state.error, "wrongpw")
        f2.setRestorePassword("violet-orbit-71-tundra")
        XCTAssertTrue(f2.restoreFromCloud())
    }

    /// CB1/CB4 (padded incremental blobs, restore chats) + RS1/RS2 (notice, verified cleared, old QR fails).
    func testChatBackupRestoreNoticeAndReverify() throws {
        let t = Int64(Date().timeIntervalSince1970 * 1000)
        let (ca, ra) = try node("Ana")
        let (cb, rb) = try node("Ben")
        let bOnA = try ra.addContact(qr: try cb.qrPayload(), action: .verifyAndChat)
        let aOnB = try rb.addContact(qr: try ca.qrPayload(), action: .verifyAndChat)
        let oldQr = try ca.qrPayload()
        try ra.send(bOnA.id, text: "see you at the market")
        var la = try ca.openInitiatorLink(bOnA.id)
        var lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: ra, rb)
        la.close(); lb.close()

        let cloud = InMemoryChatBlobStore()
        XCTAssertEqual(try ChatBackupSync(core: ca, blobs: cloud, clock: { t }).runText(onWifi: true, force: true), 1)
        XCTAssertFalse(cloud.blobs.isEmpty)
        for b in cloud.blobs.values { XCTAssertEqual(b.count % 65536, 0) }

        let idBlob = try ca.createBackup(password: "correct horse battery staple", deviceHint: "test", nowMs: t)
        let restored = try ca.openBackup(idBlob, password: "correct horse battery staple")
        let c2 = RustOmcpCore()
        let r2 = MeshRepository(core: c2, store: InMemoryAppStore(), clock: { t + 1000 })
        r2.chatBackup = ChatBackupSync(core: c2, blobs: cloud, clock: { t + 1000 })
        try r2.restoreFromBackup(restored, name: "Ana", method: "icloud")
        let res = try XCTUnwrap(try r2.restoreChats())
        XCTAssertGreaterThanOrEqual(res.messages, 1)
        XCTAssertEqual(r2.messages[bOnA.id]?.last(where: { $0.status != .system })?.text, "see you at the market")
        XCTAssertNotNil(r2.restoredAtMs)

        // RS1: Ben sees the notice on the next link. RS2: verified cleared, old QR fails.
        var notices: [RestoreNoticeInfo] = []
        // v0.1.4a: the restored device has a new X25519 key, so it dials first.
        la = try c2.openInitiatorLink(bOnA.id)
        lb = try cb.openResponderLink()
        try pump(la, lb, try la.start(), into: r2, rb) { toB, output in
            if toB { notices += output.restoreNotices }
        }
        XCTAssertEqual(notices.count, 1)
        let c = try XCTUnwrap(rb.contact(aOnB.id))
        XCTAssertFalse(c.verified)
        XCTAssertTrue(c.restoredUnverified)
        XCTAssertEqual(rb.messages[aOnB.id]?.last?.status, .system)
        XCTAssertThrowsError(try rb.addContact(qr: oldQr, action: .verifyAndChat)) { e in
            XCTAssertEqual(e as? CoreError, .verificationFailed)
        }
        let re = try rb.addContact(qr: try c2.qrPayload(), action: .verifyAndChat)
        XCTAssertTrue(re.verified)
        XCTAssertFalse(re.restoredUnverified)
    }

    /// Keep sent media + one-message Ask to resend over the real core (REQUIREMENTS v0.1.4b).
    func testKeptMediaSettingsRoundTrip() throws {
        let core = RustOmcpCore()
        try MeshRepository(core: core, store: InMemoryAppStore()).saveIdentity(phrase: try core.generateRecoveryPhrase(), name: "Ana")
        XCTAssertEqual(core.keptMediaSettings(), KeptMediaPrefs())
        XCTAssertEqual(try core.setKeptMediaSettings(KeptMediaPrefs(maxDays: 30, maxBytes: 500_000_000), nowMs: 1), 0)
        XCTAssertEqual(core.keptMediaSettings(), KeptMediaPrefs(maxDays: 30, maxBytes: 500_000_000))
        XCTAssertEqual(core.keptMediaUsage(), HoplaKit.KeptMediaUsage(count: 0, bytes: 0))
    }

}

extension RustOmcpCore {
    /// A throwaway identity's QR, for tests.
    func qrPayloadFor(name: String) throws -> String {
        _ = try load(phrase: try generateRecoveryPhrase(), displayName: name)
        return try qrPayload()
    }

}
