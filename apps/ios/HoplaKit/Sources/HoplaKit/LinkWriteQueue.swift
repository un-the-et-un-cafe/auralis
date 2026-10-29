import Foundation

/// Exactly one ciphertext batch for one Noise session. OS acceptance is not peer delivery.
/// All access is confined to the transport's serial queue. A failed batch cannot be reused.
public struct LinkWriteQueue {
    private var pending: [Data] = []
    public private(set) var batchToken: String?
    public private(set) var inFlight = false
    public private(set) var failed = false
    public var isEmpty: Bool { pending.isEmpty }
    public var readyToken: String? { !failed && !inFlight && isEmpty ? batchToken : nil }

    public init() {}

    /// Empty receive-only results never overwrite the outstanding batch. Reject malformed or
    /// overlapping batches instead of guessing which ciphertext a later callback acknowledges.
    @discardableResult
    public mutating func append(contentsOf pdus: [Data], batchToken token: String?) -> Bool {
        guard !failed else { return false }
        if pdus.isEmpty { return token == nil }
        guard batchToken == nil, let token, !token.isEmpty else { return false }
        batchToken = token
        pending = pdus
        return true
    }

    public mutating func next() -> Data? {
        guard !failed, !inFlight, let first = pending.first else { return nil }
        inFlight = true
        return first
    }

    /// Ignore callbacks when no operation is outstanding (or after shutdown). The owner must
    /// also check connection/characteristic identity before routing a CoreBluetooth callback.
    /// A genuine failure discards the whole remaining nonce stream, never just one PDU.
    @discardableResult
    public mutating func complete(succeeded: Bool) -> Bool {
        guard !failed, inFlight else { return false }
        guard succeeded else { fail(); return false }
        pending.removeFirst()
        inFlight = false
        return true
    }

    /// Only for synchronous `updateValue(false)`: the OS did not accept these bytes. Retain
    /// the same ciphertext and await peripheralManagerIsReady, without resealing or committing.
    public mutating func deferUnaccepted() {
        guard !failed else { return }
        inFlight = false
    }

    /// Release the batch token only after the Rust durable transaction succeeded.
    @discardableResult
    public mutating func committed(_ token: String) -> Bool {
        guard readyToken == token else { return false }
        batchToken = nil
        return true
    }

    public mutating func fail() {
        failed = true
        inFlight = false
        pending.removeAll()
        batchToken = nil
    }
}
