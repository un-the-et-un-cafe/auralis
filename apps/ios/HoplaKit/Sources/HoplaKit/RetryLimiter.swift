/// Rate-limits backup password attempts (O4): 3 free tries, then 30 s, 60 s, … up to 15 min.
public final class RetryLimiter {
    public static let freeTries = 3
    public static let baseMs: Int64 = 30_000
    public static let maxMs: Int64 = 15 * 60_000

    private let clock: () -> Int64
    private var failures = 0
    private var lockedUntil: Int64 = 0

    public init(clock: @escaping () -> Int64) { self.clock = clock }

    public func remainingLockMs() -> Int64 { max(0, lockedUntil - clock()) }
    public var canTry: Bool { remainingLockMs() == 0 }

    public func onFailure() {
        failures += 1
        if failures >= Self.freeTries {
            let step = min(failures - Self.freeTries, 5)
            lockedUntil = clock() + min(Self.baseMs << Int64(step), Self.maxMs)
        }
    }

    public func onSuccess() {
        failures = 0
        lockedUntil = 0
    }
}
