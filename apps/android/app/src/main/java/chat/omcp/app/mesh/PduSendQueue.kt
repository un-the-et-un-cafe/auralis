package chat.omcp.app.mesh

/**
 * One in-flight GATT operation per connection. A rejected/failed PDU poisons the session:
 * its nonce-bearing bytes must never be skipped or replayed on that same crypto link.
 * The owner closes the link on failure; only a fresh session may resume application work.
 * All access is confined to the transport's serial executor.
 */
internal class PduSendQueue {
    private val pending = ArrayDeque<ByteArray>()
    private var inFlight = false
    private var failed = false
    val isEmpty: Boolean get() = pending.isEmpty()

    fun addAll(pdus: List<ByteArray>) {
        check(!failed) { "Cannot reuse a failed GATT session" }
        pending.addAll(pdus)
    }

    /** False means the caller must close this session. Never retries a rejected operation. */
    fun submit(send: (ByteArray) -> Boolean): Boolean {
        if (failed) return false
        if (inFlight || pending.isEmpty()) return true
        inFlight = true
        val accepted = try { send(pending.first()) } catch (_: Exception) { false }
        if (!accepted) fail()
        return accepted
    }

    /** Ignore stale/spurious completions. Successful completion consumes exactly one PDU. */
    fun complete(success: Boolean): Boolean {
        if (!inFlight || failed) return false
        if (success) {
            pending.removeFirst()
            inFlight = false
        } else fail()
        return true
    }

    private fun fail() {
        failed = true
        inFlight = false
        pending.clear()
    }
}
