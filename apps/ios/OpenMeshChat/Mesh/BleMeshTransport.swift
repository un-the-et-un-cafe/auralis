import CoreBluetooth
import Foundation
import HoplaKit

/// iOS BLE GATT transport, same GATT as Android (`OmcpGatt`, PROTOCOL §8.1). Every phone is both:
/// - **peripheral** (`CBPeripheralManager`): advertises the OMCP service UUID and hosts
///   Inbox (write) / Outbox (notify) / Control (read → 13-byte discovery advertisement);
///   acts as Noise **responder**;
/// - **central** (`CBCentralManager`): scans for the service, connects, reads Control to learn
///   the peer's PeerID prefix, and dials a known contact only if our PeerID sorts lower
///   (same tie-break as Android); acts as Noise **initiator**.
///
/// Opaque PDUs go straight into `CoreLink` (Rust); all crypto and fragmentation is in Rust.
///
/// iOS differences / background notes (see apps/ios/README.md):
/// - iOS peripherals can't advertise manufacturer data, so the 13-byte ad is served on Control
///   and we also advertise `OmcpGatt.localName`. Android centrals don't rely on manufacturer data
///   for iPhones: they match the service UUID / local name, or Apple's background overflow-area
///   data, then connect and do GATT service discovery + read Control (`ScanClassifier.kt`).
/// - In the background iOS moves the service UUID to the overflow area and slows scanning;
///   scanning must name the service UUID. `UIBackgroundModes` has bluetooth-central and
///   bluetooth-peripheral; both managers use state restoration identifiers so iOS can relaunch
///   the app for BLE events. Discovery between two backgrounded iPhones is unreliable; that's
///   why Gateway recommends an Android companion and Nearby uses a Live Activity session.
///
/// Experimental: not run on hardware in this environment (no Mac/iPhone).
final class BleMeshTransport: NSObject, MeshKick {
    private let repo: MeshRepository
    private let q = DispatchQueue(label: "chat.omcp.ble")
    private var central: CBCentralManager!
    private var peripheral: CBPeripheralManager!

    private let service = CBUUID(string: OmcpGatt.service)
    private let inbox = CBUUID(string: OmcpGatt.inbox)
    private let outbox = CBUUID(string: OmcpGatt.outbox)
    private let control = CBUUID(string: OmcpGatt.control)

    // Peripheral side
    private var outboxChar: CBMutableCharacteristic?
    private var notifyBlocked = false
    private final class Responder {
        let central: CBCentral
        let link: CoreLink
        var writes = LinkWriteQueue()
        var receiptRecoveryScheduled = false
        var more = false
        var peer: String?
        init(_ central: CBCentral, _ link: CoreLink) { self.central = central; self.link = link }
    }
    private var responders: [UUID: Responder] = [:]

    // Central side
    private final class Client {
        let p: CBPeripheral
        var contactId: String?
        var link: CoreLink?
        var inbox: CBCharacteristic?
        var outbox: CBCharacteristic?
        var control: CBCharacteristic?
        var writes = LinkWriteQueue()
        /// Rust has more file chunks for this link: flush again when `writes` drains.
        var receiptRecoveryScheduled = false
        var more = false
        var started = false
        init(_ p: CBPeripheral) { self.p = p }
    }
    private var clients: [UUID: Client] = [:]

    init(repo: MeshRepository) {
        self.repo = repo
        super.init()
    }

    func start() {
        q.async { [self] in
            guard central == nil, peripheral == nil else { return }
            central = CBCentralManager(delegate: self, queue: q, options: [CBCentralManagerOptionRestoreIdentifierKey: "omcp.central"])
            peripheral = CBPeripheralManager(delegate: self, queue: q, options: [CBPeripheralManagerOptionRestoreIdentifierKey: "omcp.peripheral"])
        }
    }

    func stop() {
        q.async { [self] in
            central?.stopScan()
            peripheral?.stopAdvertising()
            for c in clients.values { abortClient(c); c.p.delegate = nil }
            clients.removeAll()
            for id in Array(responders.keys) { dropResponder(id) }
            central?.delegate = nil
            peripheral?.delegate = nil
            central = nil
            peripheral = nil
            outboxChar = nil
            notifyBlocked = false
        }
    }

    private func onMain(_ f: @escaping () -> Void) { DispatchQueue.main.async(execute: f) }

    /// Deliver a link step's results to the repository (main thread) and queue outgoing PDUs.
    private func handle(_ r: LinkResult, contact: String?) {
        let cid = r.peerContactId ?? contact
        onMain { [repo] in
            r.received.forEach(repo.onReceived)
            r.opened.forEach(repo.onOpened)
            repo.applyFileEvents(r)
            if let cid { repo.markFlushed(cid, count: r.flushed) }
        }
    }

    private func recoverAfterStorageFailure(_ error: Error) {
        guard case CoreError.storageUnavailable = error else { return }
        onMain { [repo] in repo.retryReceivedFiles() }
    }

    // MARK: MeshKick
    func kick(_ contactId: String) {
        q.async { [self] in
            for c in clients.values where c.contactId == contactId && c.link?.isEstablished == true {
                guard let link = c.link else { continue }
                do { onClientResult(c, try link.flush()) } catch { recoverAfterStorageFailure(error); abortClient(c) }
            }
            for (id, r) in Array(responders) where r.link.isEstablished {
                do { onResponderResult(id, try r.link.flush()) } catch { recoverAfterStorageFailure(error); dropResponder(id) }
            }
            pumpNotify()
        }
    }

    @discardableResult
    private func acceptClientResult(_ c: Client, _ r: LinkResult) -> Bool {
        guard c.writes.append(contentsOf: r.outgoing, batchToken: r.batchToken) else {
            abortClient(c)
            return false
        }
        if r.established && !c.receiptRecoveryScheduled {
            c.receiptRecoveryScheduled = true
            onMain { [repo] in repo.retryReceivedFiles() }
        }
        c.more = r.morePending
        handle(r, contact: c.contactId)
        return true
    }

    private func onClientResult(_ c: Client, _ r: LinkResult) {
        if acceptClientResult(c, r) { pumpWrites(c) }
    }

    private func onResponderResult(_ id: UUID, _ output: LinkResult) {
        guard let r = responders[id] else { return }
        guard r.writes.append(contentsOf: output.outgoing, batchToken: output.batchToken) else {
            dropResponder(id)
            return
        }
        if let peer = output.peerContactId { r.peer = peer }
        if output.established && !r.receiptRecoveryScheduled {
            r.receiptRecoveryScheduled = true
            onMain { [repo] in repo.retryReceivedFiles() }
        }
        r.more = output.morePending
        handle(output, contact: r.peer)
    }

    private func pumpWrites(_ c: Client) {
        guard clients[c.p.identifier] === c, !c.writes.failed, !c.writes.inFlight,
              let ch = c.inbox, let link = c.link else { return }
        do {
            if let token = c.writes.readyToken {
                // Every withResponse callback succeeded. This is OS acceptance, not peer receipt.
                let committed = try link.commitOutgoing(token)
                guard c.writes.committed(token), acceptClientResult(c, committed) else { abortClient(c); return }
            }
            if c.writes.batchToken == nil, c.more, link.isEstablished {
                guard acceptClientResult(c, try link.flush()) else { return }
            }
            guard let pdu = c.writes.next() else { return }
            c.p.writeValue(pdu, for: ch, type: .withResponse)
        } catch { recoverAfterStorageFailure(error); abortClient(c) }
    }

    private func pumpNotify() {
        guard !notifyBlocked, let pm = peripheral, let ch = outboxChar else { return }
        var deferred: Set<UUID> = []
        while let (id, r) = responders.first(where: {
            !deferred.contains($0.key) && (!$0.value.writes.isEmpty || $0.value.writes.readyToken != nil || $0.value.more)
        }) {
            do {
                if let token = r.writes.readyToken {
                    // updateValue(true) only means CoreBluetooth accepted each notification.
                    let committed = try r.link.commitOutgoing(token)
                    guard r.writes.committed(token) else { dropResponder(id); continue }
                    onResponderResult(id, committed)
                }
                guard responders[id] === r else { continue }
                if r.writes.batchToken == nil, r.more, r.link.isEstablished {
                    onResponderResult(id, try r.link.flush())
                }
                guard responders[id] === r else { continue }
                guard let pdu = r.writes.next() else {
                    deferred.insert(id) // No busy-loop on a peer reserved by another link; allow other peers.
                    continue
                }
                if !pm.updateValue(pdu, for: ch, onSubscribedCentrals: [r.central]) {
                    r.writes.deferUnaccepted()
                    notifyBlocked = true
                    return // Retry these identical bytes only when CoreBluetooth signals ready.
                }
                guard r.writes.complete(succeeded: true) else { dropResponder(id); continue }
            } catch { recoverAfterStorageFailure(error); dropResponder(id) }
        }
    }

    private func abortClient(_ c: Client) {
        // Keep the failed client until didDisconnect: old callbacks must not reach a new session.
        if let token = c.writes.batchToken { try? c.link?.abortOutgoing(token) }
        c.writes.fail()
        c.more = false
        c.link?.close()
        c.link = nil
        pause(c.contactId)
        central?.cancelPeripheralConnection(c.p)
    }

    private func dropResponder(_ id: UUID) {
        guard let r = responders.removeValue(forKey: id) else { return }
        if let token = r.writes.batchToken { try? r.link.abortOutgoing(token) }
        r.writes.fail()
        r.link.close()
        pause(r.peer)
    }

    private func pause(_ contactId: String?) {
        guard let contactId else { return }
        onMain { [repo] in repo.pauseTransfers(contactId) }
    }
}

extension BleMeshTransport: CBCentralManagerDelegate, CBPeripheralDelegate {
    func centralManager(_ c: CBCentralManager, willRestoreState dict: [String: Any]) {}

    func centralManagerDidUpdateState(_ c: CBCentralManager) {
        guard c === central else { return }
        guard c.state == .poweredOn else {
            for client in clients.values { abortClient(client) }
            return
        }
        c.scanForPeripherals(withServices: [service], options: [CBCentralManagerScanOptionAllowDuplicatesKey: false])
    }

    func centralManager(_ c: CBCentralManager, didDiscover p: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        guard c === central, clients[p.identifier] == nil else { return }
        let client = Client(p)
        clients[p.identifier] = client
        p.delegate = self
        c.connect(p)
    }

    func centralManager(_ c: CBCentralManager, didConnect p: CBPeripheral) {
        guard c === central, let client = clients[p.identifier], client.p === p, !client.writes.failed else { return }
        p.discoverServices([service])
    }

    func centralManager(_ c: CBCentralManager, didDisconnectPeripheral p: CBPeripheral, error: Error?) {
        guard c === central, let client = clients[p.identifier], client.p === p else { return }
        clients.removeValue(forKey: p.identifier)
        client.writes.fail()
        client.link?.close()
        pause(client.contactId)
    }

    func centralManager(_ c: CBCentralManager, didFailToConnect p: CBPeripheral, error: Error?) {
        guard c === central, let client = clients[p.identifier], client.p === p else { return }
        client.link?.close()
        clients.removeValue(forKey: p.identifier)
    }

    func peripheral(_ p: CBPeripheral, didDiscoverServices error: Error?) {
        guard let c = clients[p.identifier], c.p === p, !c.writes.failed else { return }
        guard error == nil, let s = p.services?.first(where: { $0.uuid == service }) else { abortClient(c); return }
        p.discoverCharacteristics([inbox, outbox, control], for: s)
    }

    func peripheral(_ p: CBPeripheral, didDiscoverCharacteristicsFor s: CBService, error: Error?) {
        guard let c = clients[p.identifier], c.p === p, !c.writes.failed else { return }
        guard error == nil else { abortClient(c); return }
        guard c.inbox == nil else { return }
        c.inbox = s.characteristics?.first { $0.uuid == inbox }
        c.outbox = s.characteristics?.first { $0.uuid == outbox }
        c.control = s.characteristics?.first { $0.uuid == control }
        guard c.inbox != nil, c.outbox != nil, let ctl = c.control else { abortClient(c); return }
        p.readValue(for: ctl)
    }

    func peripheral(_ p: CBPeripheral, didUpdateValueFor ch: CBCharacteristic, error: Error?) {
        guard let c = clients[p.identifier], c.p === p, !c.writes.failed else { return }
        guard ch === c.control || ch === c.outbox else { return }
        guard error == nil, let value = ch.value else { abortClient(c); return }
        if ch === c.control {
            guard c.link == nil else { return } // A duplicate discovery result cannot replace a live session.
            // 13-byte discovery ad → known contact? Tie-break: lower PeerID dials.
            guard let cid = repo.core.contactForAdvertisement(value),
                  let me = repo.core.identity()?.peerIdHex, me < cid,
                  let link = try? repo.core.openInitiatorLink(cid) else {
                central.cancelPeripheralConnection(p)
                return
            }
            c.contactId = cid
            c.link = link
            if let ob = c.outbox { p.setNotifyValue(true, for: ob) }
        } else if ch === c.outbox, let link = c.link {
            do { onClientResult(c, try link.onPdu(value)) } catch { recoverAfterStorageFailure(error); abortClient(c) }
        }
    }

    func peripheral(_ p: CBPeripheral, didUpdateNotificationStateFor ch: CBCharacteristic, error: Error?) {
        guard let c = clients[p.identifier], c.p === p, ch === c.outbox, !c.writes.failed else { return }
        guard error == nil, ch.isNotifying, let link = c.link else { abortClient(c); return }
        guard !c.started else { return }
        c.started = true
        do { onClientResult(c, try link.start()) } catch { recoverAfterStorageFailure(error); abortClient(c) }
    }

    func peripheral(_ p: CBPeripheral, didWriteValueFor ch: CBCharacteristic, error: Error?) {
        guard let c = clients[p.identifier], c.p === p, c.inbox === ch,
              !c.writes.failed, c.writes.inFlight else { return }
        guard c.writes.complete(succeeded: error == nil) else { abortClient(c); return }
        pumpWrites(c)
    }

}

extension BleMeshTransport: CBPeripheralManagerDelegate {
    func peripheralManager(_ pm: CBPeripheralManager, willRestoreState dict: [String: Any]) {}

    func peripheralManagerDidUpdateState(_ pm: CBPeripheralManager) {
        guard pm === peripheral else { return }
        guard pm.state == .poweredOn else {
            for id in Array(responders.keys) { dropResponder(id) }
            outboxChar = nil
            notifyBlocked = false
            return
        }
        notifyBlocked = false
        let inboxC = CBMutableCharacteristic(type: inbox, properties: [.write, .writeWithoutResponse], value: nil, permissions: [.writeable])
        let outboxC = CBMutableCharacteristic(type: outbox, properties: [.notify], value: nil, permissions: [.readable])
        let controlC = CBMutableCharacteristic(type: control, properties: [.read], value: nil, permissions: [.readable])
        let s = CBMutableService(type: service, primary: true)
        s.characteristics = [inboxC, outboxC, controlC]
        outboxChar = outboxC
        pm.removeAllServices()
        pm.add(s)
        pm.startAdvertising([CBAdvertisementDataServiceUUIDsKey: [service], CBAdvertisementDataLocalNameKey: OmcpGatt.localName])
    }

    func peripheralManager(_ pm: CBPeripheralManager, didReceiveRead req: CBATTRequest) {
        guard pm === peripheral else { return }
        guard req.characteristic.uuid == control, let ad = try? repo.core.advertisement() else {
            pm.respond(to: req, withResult: .requestNotSupported)
            return
        }
        guard req.offset <= ad.count else { pm.respond(to: req, withResult: .invalidOffset); return }
        req.value = ad.subdata(in: req.offset..<ad.count)
        pm.respond(to: req, withResult: .success)
    }

    func peripheralManager(_ pm: CBPeripheralManager, central: CBCentral, didSubscribeTo ch: CBCharacteristic) {
        guard pm === peripheral, ch === outboxChar else { return }
        if let r = responders[central.identifier], r.central === central { return }
        dropResponder(central.identifier)
        if let link = try? repo.core.openResponderLink() { responders[central.identifier] = Responder(central, link) }
    }

    func peripheralManager(_ pm: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom ch: CBCharacteristic) {
        guard pm === peripheral, ch === outboxChar, responders[central.identifier]?.central === central else { return }
        dropResponder(central.identifier)
    }

    func peripheralManager(_ pm: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        guard pm === peripheral else { return }
        var result: CBATTError.Code = .success
        for req in requests {
            let id = req.central.identifier
            guard req.characteristic.uuid == inbox, let r = responders[id], r.central === req.central, let value = req.value else {
                result = .requestNotSupported
                break
            }
            do {
                onResponderResult(id, try r.link.onPdu(value))
                if responders[id] == nil { result = .unlikelyError; break }
            } catch {
                recoverAfterStorageFailure(error)
                // Reject rather than acknowledging a rejected/closed Noise session. The peer
                // must establish a new session; queued ciphertext for this one is discarded.
                dropResponder(id)
                result = .unlikelyError
                break
            }
        }
        if let first = requests.first { pm.respond(to: first, withResult: result) }
        pumpNotify()
    }

    func peripheralManagerIsReady(toUpdateSubscribers pm: CBPeripheralManager) {
        guard pm === peripheral else { return }
        notifyBlocked = false
        pumpNotify()
    }
}
