import Foundation
@testable import HoplaKit

final class Clock {
    var ms: Int64 = 1_000_000
    lazy var fn: () -> Int64 = { self.ms }
}

struct SeededRNG: RandomNumberGenerator {
    var s: UInt64
    mutating func next() -> UInt64 {
        s = s &* 6364136223846793005 &+ 1442695040888963407
        return s
    }
}

func makeRepo(_ clock: Clock = Clock(), store: AppStore = InMemoryAppStore()) -> (MeshRepository, FakeOmcpCore) {
    let core = FakeOmcpCore()
    return (MeshRepository(core: core, store: store, clock: clock.fn), core)
}

final class RecordingNotifier: Notifier {
    var posted: [NotificationContent] = []
    func post(_ content: NotificationContent, id: String) { posted.append(content) }
}
