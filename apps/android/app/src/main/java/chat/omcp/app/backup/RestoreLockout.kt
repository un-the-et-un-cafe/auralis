package chat.omcp.app.backup

import chat.omcp.app.data.AppStore

/**
 * Backup-password lockout for restore (DESIGN_SPEC §5.31 / §5.85, requirement O4).
 *
 * 3 tries per round; after the 3rd wrong password the field is locked for the next entry of
 * [LOCKOUT_MINUTES] (5 → 15 → 60, then 60 for every further round). State lives in [AppStore]
 * so killing the app does not reset the wait. The 24-word path is never affected.
 */
class RestoreLockout(private val store: AppStore, private val clock: () -> Long = System::currentTimeMillis) {
    fun remainingMs(): Long = (store.restoreLockedUntilMs - clock()).coerceAtLeast(0)
    fun locked() = remainingMs() > 0
    /** Whole minutes shown in the UI (rounded up, so "1 min" until the very end). */
    fun remainingMinutes(): Int = ((remainingMs() + 59_999) / 60_000).toInt()
    fun triesLeft(): Int = TRIES_PER_ROUND - store.restoreFailures
    /** Wait (minutes) the *next* lockout will impose. */
    fun nextLockMinutes(): Int = LOCKOUT_MINUTES[store.restoreRound.coerceAtMost(LOCKOUT_MINUTES.lastIndex)]

    /** Records a wrong password. Returns true when this failure started a lockout. */
    fun onFailure(): Boolean {
        val f = store.restoreFailures + 1
        if (f < TRIES_PER_ROUND) {
            store.restoreFailures = f
            return false
        }
        store.restoreLockedUntilMs = clock() + nextLockMinutes() * 60_000L
        store.restoreRound = store.restoreRound + 1
        store.restoreFailures = 0
        return true
    }

    fun onSuccess() {
        store.restoreFailures = 0
        store.restoreRound = 0
        store.restoreLockedUntilMs = 0
    }

    companion object {
        const val TRIES_PER_ROUND = 3

        /**
         * Lockout per round, in minutes. Designy's assumption (§12 Q16), pending product
         * confirmation: change only this constant.
         */
        val LOCKOUT_MINUTES = listOf(5, 15, 60)
    }
}
