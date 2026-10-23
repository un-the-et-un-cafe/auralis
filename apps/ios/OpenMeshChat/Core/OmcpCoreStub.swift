import Foundation

/// Placeholder for omcp-core via FFI.
/// M2–M4 are not ready — UI-facing path/queue only; never invents crypto.
enum OmcpCoreStub {
    static let coreLinked = false
    static let suiteLabel = "OMCP-2026-A (stub)"

    enum PathKind: String {
        case mesh, gateway, mixed, queued
    }

    struct MeshStatus {
        var nearbyPeers: Int
        var queuedCount: Int
        var aggregatePath: PathKind
        var note: String
    }

    static func meshStatus() -> MeshStatus {
        MeshStatus(
            nearbyPeers: 0,
            queuedCount: 0,
            aggregatePath: .queued,
            note: "omcp-core FFI not linked — UI shell only"
        )
    }

    static func encryptionSummary() -> String {
        "End-to-end · Noise hybrid (placeholder — no session yet)"
    }
}
