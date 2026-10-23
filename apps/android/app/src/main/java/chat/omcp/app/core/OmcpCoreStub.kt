package chat.omcp.app.core

/**
 * Placeholder for omcp-core via JNI.
 *
 * M2–M4 (BLE, MLS, gateway) are not ready. This stub exposes UI-facing
 * path / queue state only — never invents crypto results.
 */
object OmcpCoreStub {
    const val CORE_LINKED: Boolean = false
    const val SUITE_LABEL: String = "OMCP-2026-A (stub)"

    enum class PathKind { Mesh, Gateway, Mixed, Queued }

    data class MeshStatus(
        val nearbyPeers: Int,
        val queuedCount: Int,
        val aggregatePath: PathKind,
        val note: String,
    )

    fun meshStatus(): MeshStatus = MeshStatus(
        nearbyPeers = 0,
        queuedCount = 0,
        aggregatePath = PathKind.Queued,
        note = "omcp-core JNI not linked — UI shell only",
    )

    fun encryptionSummary(): String =
        "End-to-end · Noise hybrid (placeholder — no session yet)"
}
