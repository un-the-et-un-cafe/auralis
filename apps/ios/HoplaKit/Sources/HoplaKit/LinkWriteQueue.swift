import Foundation

/// Ordered ciphertext writes for one Noise session. Once any ATT write fails, discard the
/// remaining ciphertext and require a fresh session; neither skip nor replay its nonce stream.
/// Owned by the transport's serial queue, not shared across threads.
public struct LinkWriteQueue {
    private var pending: [Data] = []
    public private(set) var inFlight = false
    public private(set) var failed = false
    public var isEmpty: Bool { pending.isEmpty }

    public init() {}

    public mutating func append(contentsOf pdus: [Data]) {
        guard !failed else { return }
        pending.append(contentsOf: pdus)
    }

    public mutating func next() -> Data? {
        guard !failed, !inFlight, !pending.isEmpty else { return nil }
        inFlight = true
        return pending.removeFirst()
    }

    /// False means this session must close, including an unexpected completion with no write.
    public mutating func complete(succeeded: Bool) -> Bool {
        guard !failed, inFlight, succeeded else { fail(); return false }
        inFlight = false
        return true
    }

    public mutating func fail() {
        failed = true
        inFlight = false
        pending.removeAll()
    }
}
