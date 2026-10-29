package chat.omcp.app

import chat.omcp.app.core.CoreLink
import chat.omcp.app.core.LinkResult

/** Simulated OS acceptance after each complete batch; never implies a protocol delivery receipt. */
internal fun drainBatches(
    a: CoreLink,
    b: CoreLink,
    first: LinkResult,
    fromA: Boolean = true,
    apply: (sourceA: Boolean, LinkResult) -> Unit = { _, _ -> },
) {
    val pending = ArrayDeque<Pair<Boolean, LinkResult>>()
    pending.addLast(fromA to first)
    while (pending.isNotEmpty()) {
        val (sourceA, out) = pending.removeFirst()
        val source = if (sourceA) a else b
        val destination = if (sourceA) b else a
        apply(sourceA, out)
        for (pdu in out.outgoing) {
            check(pdu.size <= 512)
            val received = destination.onPdu(pdu)
            if (received.outgoing.isNotEmpty()) pending.addLast(!sourceA to received)
            else apply(!sourceA, received)
        }
        out.batchToken?.let { token ->
            val committed = source.commitOutgoing(token)
            apply(sourceA, committed)
            if (committed.morePending && source.isEstablished()) {
                val next = source.flush()
                if (next.outgoing.isNotEmpty()) pending.addLast(sourceA to next)
                else apply(sourceA, next)
            }
        }
    }
}
