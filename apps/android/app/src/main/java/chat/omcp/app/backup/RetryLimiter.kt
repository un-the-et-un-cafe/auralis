package chat.omcp.app.backup

/** Rate-limits backup password attempts (O4): 3 free tries, then 30 s, 60 s, … up to 15 min. */
class RetryLimiter(private val clock: () -> Long = System::currentTimeMillis) {
    private var failures = 0
    private var lockedUntil = 0L

    fun remainingLockMs(): Long = (lockedUntil - clock()).coerceAtLeast(0)
    fun canTry() = remainingLockMs() == 0L

    fun onFailure() {
        failures++
        if (failures >= FREE_TRIES) {
            val step = failures - FREE_TRIES
            val delay = (BASE_MS shl step.coerceAtMost(5)).coerceAtMost(MAX_MS)
            lockedUntil = clock() + delay
        }
    }

    fun onSuccess() { failures = 0; lockedUntil = 0 }

    companion object {
        const val FREE_TRIES = 3
        const val BASE_MS = 30_000L
        const val MAX_MS = 15 * 60_000L
    }
}
