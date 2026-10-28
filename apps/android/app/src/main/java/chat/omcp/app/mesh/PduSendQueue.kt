package chat.omcp.app.mesh

/**
 * Exactly one Rust batch and one in-flight GATT operation per connection. A batch
 * is committed only after every successful OS callback; this is not peer delivery.
 * Rejected/failed operations poison the session rather than skipping nonce-bearing
 * PDUs. The owner aborts/closes the link, preserving its logical durable work.
 *
 * Mutation is IO-confined. The synchronized callback ticket is also read by Android's
 * callback thread, before dispatch, so already-queued late/duplicate callbacks cannot
 * accidentally acknowledge a newer operation or a replacement queue.
 */
internal class PduSendQueue {
    internal class Ticket internal constructor(val owner: PduSendQueue)

    private val pending = ArrayDeque<ByteArray>()
    private var inFlight: Ticket? = null
    private var failed = false
    private var token: String? = null

    val isEmpty: Boolean @Synchronized get() = pending.isEmpty() && token == null
    val batchToken: String? @Synchronized get() = token
    val callbackTicket: Ticket? @Synchronized get() = inFlight
    val readyToken: String? @Synchronized get() = token.takeIf { !failed && inFlight == null && pending.isEmpty() }

    @Synchronized
    fun addBatch(batchToken: String?, pdus: List<ByteArray>) {
        check(!failed) { "Cannot reuse a failed GATT session" }
        require((batchToken == null) == pdus.isEmpty()) { "Every nonempty PDU batch needs its Rust token" }
        if (pdus.isEmpty()) return // inbound-only results must not erase a pending batch
        check(token == null) { "Previous Rust batch is not committed" }
        token = batchToken
        pending.addAll(pdus)
    }

    /** False means close this session. A synchronous rejection is never retried. */
    @Synchronized
    fun submit(send: (ByteArray) -> Boolean): Boolean {
        if (failed) return false
        if (inFlight != null || pending.isEmpty()) return true
        inFlight = Ticket(this)
        val accepted = try { send(pending.first()) } catch (_: Exception) { false }
        if (!accepted) {
            inFlight = null // Android did not accept this operation; no callback is owed.
            cancel()
        }
        return accepted
    }

    /** Consume only the operation captured at callback ingress, never a newer one. */
    @Synchronized
    fun complete(ticket: Ticket, success: Boolean): Boolean {
        if (ticket.owner !== this || ticket !== inFlight) return false
        inFlight = null
        if (failed) return false // drains a retired callback without touching replacement work
        if (success) pending.removeFirst() else cancel()
        return true
    }

    /** Call only after Rust durably committed this exact token. */
    @Synchronized
    fun committed(batchToken: String) {
        check(readyToken == batchToken) { "Batch has not finished OS acceptance" }
        token = null
    }

    /** Preserve an outstanding callback ticket so the peripheral can fence reconnects. */
    @Synchronized
    fun cancel() {
        failed = true
        pending.clear()
    }
}
