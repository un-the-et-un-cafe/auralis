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
    private var responders: [UUID: CoreLink] = [:]
    private var subscribers: [UUID: CBCentral] = [:]
    private var notifyQueue: [(UUID, Data)] = []
    private var responderMore: [UUID: Bool] = [:]
    private var responderPeer: [UUID: String] = [:]

    // Central side
    private final class Client {
        let p: CBPeripheral
        var contactId: String?
        var link: CoreLink?
        var inbox: CBCharacteristic?
        var writes: [Data] = []
        var writing = false
        /// Rust has more file chunks for this link: flush again when `writes` drains.
        var more = false
        init(_ p: CBPeripheral) { self.p = p }
    }
    private var clients: [UUID: Client] = [:]

    init(repo: MeshRepository) {
        self.repo = repo
        super.init()
    }

    func start() {
        central = CBCentralManager(delegate: self, queue: q, options: [CBCentralManagerOptionRestoreIdentifierKey: "omcp.central"])
        peripheral = CBPeripheralManager(delegate: self, queue: q, options: [CBPeripheralManagerOptionRestoreIdentifierKey: "omcp.peripheral"])
    }

    func stop() {
        central?.stopScan()
        peripheral?.stopAdvertising()
        clients.values.forEach { central.cancelPeripheralConnection($0.p); $0.link?.close() }
        clients = [:]
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

    // MARK: MeshKick
    func kick(_ contactId: String) {
        q.async { [self] in
            for c in clients.values where c.contactId == contactId && c.link?.isEstablished == true {
                guard let r = try? c.link?.flush() else { continue }
                onClientResult(c, r)
            }
            for (id, link) in responders where link.isEstablished {
                guard let r = try? link.flush() else { continue }
                onResponderResult(id, r)
            }
            pumpNotify()
        }
    }

    private func onClientResult(_ c: Client, _ r: LinkResult) {
        c.more = r.morePending
        c.writes += r.outgoing
        handle(r, contact: c.contactId)
        pumpWrites(c)
    }

    private func onResponderResult(_ id: UUID, _ r: LinkResult) {
        if let p = r.peerContactId { responderPeer[id] = p }
        responderMore[id] = r.morePending
        r.outgoing.forEach { notifyQueue.append((id, $0)) }
        handle(r, contact: responderPeer[id])
    }

    private func pumpWrites(_ c: Client) {
        guard !c.writing, let ch = c.inbox else { return }
        if c.writes.isEmpty, c.more, let link = c.link, link.isEstablished, let r = try? link.flush() {
            // Write queue drained: pull the next file chunks from Rust.
            c.more = r.morePending
            c.writes += r.outgoing
            handle(r, contact: c.contactId)
        }
        guard !c.writes.isEmpty else { return }
        c.writing = true
        c.p.writeValue(c.writes.removeFirst(), for: ch, type: .withResponse)
    }

    private func pumpNotify() {
        guard let ch = outboxChar else { return }
        while true {
            if notifyQueue.isEmpty {
                // Notify queue drained: pull the next file chunks for any responder link that has more.
                guard let (id, link) = responders.first(where: { responderMore[$0.key] == true && $0.value.isEstablished }),
                      let r = try? link.flush() else { return }
                onResponderResult(id, r)
                if notifyQueue.isEmpty { return }
            }
            let (id, pdu) = notifyQueue[0]
            guard let sub = subscribers[id] else { notifyQueue.removeFirst(); continue }
            if !peripheral.updateValue(pdu, for: ch, onSubscribedCentrals: [sub]) { return } // wait for ready
            notifyQueue.removeFirst()
        }
    }

    private func pause(_ contactId: String?) {
        guard let contactId else { return }
        onMain { [repo] in repo.pauseTransfers(contactId) }
    }
}

extension BleMeshTransport: CBCentralManagerDelegate, CBPeripheralDelegate {
    func centralManager(_ c: CBCentralManager, willRestoreState dict: [String: Any]) {}

    func centralManagerDidUpdateState(_ c: CBCentralManager) {
        guard c.state == .poweredOn else { return }
        c.scanForPeripherals(withServices: [service], options: [CBCentralManagerScanOptionAllowDuplicatesKey: false])
    }

    func centralManager(_ c: CBCentralManager, didDiscover p: CBPeripheral, advertisementData: [String: Any], rssi: NSNumber) {
        guard clients[p.identifier] == nil else { return }
        let client = Client(p)
        clients[p.identifier] = client
        p.delegate = self
        c.connect(p)
    }

    func centralManager(_ c: CBCentralManager, didConnect p: CBPeripheral) { p.discoverServices([service]) }

    func centralManager(_ c: CBCentralManager, didDisconnectPeripheral p: CBPeripheral, error: Error?) {
        let c = clients.removeValue(forKey: p.identifier)
        c?.link?.close()
        pause(c?.contactId)
    }

    func centralManager(_ c: CBCentralManager, didFailToConnect p: CBPeripheral, error: Error?) {
        clients.removeValue(forKey: p.identifier)
    }

    func peripheral(_ p: CBPeripheral, didDiscoverServices error: Error?) {
        guard let s = p.services?.first(where: { $0.uuid == service }) else { return }
        p.discoverCharacteristics([inbox, outbox, control], for: s)
    }

    func peripheral(_ p: CBPeripheral, didDiscoverCharacteristicsFor s: CBService, error: Error?) {
        guard let c = clients[p.identifier] else { return }
        c.inbox = s.characteristics?.first { $0.uuid == inbox }
        if let ctl = s.characteristics?.first(where: { $0.uuid == control }) { p.readValue(for: ctl) }
    }

    func peripheral(_ p: CBPeripheral, didUpdateValueFor ch: CBCharacteristic, error: Error?) {
        guard let c = clients[p.identifier], let value = ch.value else { return }
        if ch.uuid == control {
            // 13-byte discovery ad → known contact? Tie-break: lower PeerID dials.
            guard let cid = repo.core.contactForAdvertisement(value),
                  let me = repo.core.identity()?.peerIdHex, me < cid,
                  let link = try? repo.core.openInitiatorLink(cid) else {
                central.cancelPeripheralConnection(p)
                return
            }
            c.contactId = cid
            c.link = link
            if let ob = p.services?.first?.characteristics?.first(where: { $0.uuid == outbox }) { p.setNotifyValue(true, for: ob) }
        } else if ch.uuid == outbox, let link = c.link, let r = try? link.onPdu(value) {
            onClientResult(c, r)
        }
    }

    func peripheral(_ p: CBPeripheral, didUpdateNotificationStateFor ch: CBCharacteristic, error: Error?) {
        guard ch.uuid == outbox, ch.isNotifying, let c = clients[p.identifier], let r = try? c.link?.start() else { return }
        onClientResult(c, r)
    }

    func peripheral(_ p: CBPeripheral, didWriteValueFor ch: CBCharacteristic, error: Error?) {
        guard let c = clients[p.identifier] else { return }
        c.writing = false
        if error == nil, let link = c.link, link.isEstablished, c.writes.isEmpty, let r = try? link.flush() {
            onClientResult(c, r)
            return
        }
        pumpWrites(c)
    }
}

extension BleMeshTransport: CBPeripheralManagerDelegate {
    func peripheralManager(_ pm: CBPeripheralManager, willRestoreState dict: [String: Any]) {}

    func peripheralManagerDidUpdateState(_ pm: CBPeripheralManager) {
        guard pm.state == .poweredOn else { return }
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
        guard req.characteristic.uuid == control, let ad = try? repo.core.advertisement() else {
            pm.respond(to: req, withResult: .requestNotSupported)
            return
        }
        guard req.offset <= ad.count else { pm.respond(to: req, withResult: .invalidOffset); return }
        req.value = ad.subdata(in: req.offset..<ad.count)
        pm.respond(to: req, withResult: .success)
    }

    func peripheralManager(_ pm: CBPeripheralManager, central: CBCentral, didSubscribeTo ch: CBCharacteristic) {
        subscribers[central.identifier] = central
        if responders[central.identifier] == nil { responders[central.identifier] = try? repo.core.openResponderLink() }
    }

    func peripheralManager(_ pm: CBPeripheralManager, central: CBCentral, didUnsubscribeFrom ch: CBCharacteristic) {
        subscribers.removeValue(forKey: central.identifier)
        responders.removeValue(forKey: central.identifier)?.close()
        responderMore.removeValue(forKey: central.identifier)
        pause(responderPeer.removeValue(forKey: central.identifier))
    }

    func peripheralManager(_ pm: CBPeripheralManager, didReceiveWrite requests: [CBATTRequest]) {
        for req in requests where req.characteristic.uuid == inbox {
            let id = req.central.identifier
            guard let link = responders[id], let v = req.value, let r = try? link.onPdu(v) else { continue }
            onResponderResult(id, r)
        }
        if let first = requests.first { pm.respond(to: first, withResult: .success) }
        pumpNotify()
    }

    func peripheralManagerIsReady(toUpdateSubscribers pm: CBPeripheralManager) { pumpNotify() }
}
