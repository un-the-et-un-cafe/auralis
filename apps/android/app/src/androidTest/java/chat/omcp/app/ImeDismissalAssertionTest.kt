package chat.omcp.app

import org.junit.Assert.*
import org.junit.Test

/** State gate only: it never sends Back, hides the keyboard, or retries an action. */
internal class StableImeDismissal(private val stableForMs: Long) {
    init { require(stableForMs > 0) }
    private var hiddenSince: Long? = null

    fun observe(nowMs: Long, attached: Boolean, imeVisible: Boolean?, inputFocused: Boolean): Boolean {
        if (!attached || imeVisible != false || inputFocused) {
            hiddenSince = null
            return false
        }
        val previous = hiddenSince
        if (previous == null || nowMs < previous) hiddenSince = nowMs
        return nowMs - hiddenSince!! >= stableForMs
    }
}

class ImeDismissalAssertionTest {
    @Test fun hiddenMustRemainStableNotJustBeSampledOnce() {
        val gate = StableImeDismissal(500)
        assertFalse(gate.observe(0, true, false, false))
        assertFalse(gate.observe(499, true, false, false))
        assertTrue(gate.observe(500, true, false, false))
    }

    @Test fun lateImeShowResetsTheStableWindow() {
        val gate = StableImeDismissal(500)
        assertFalse(gate.observe(0, true, false, false))
        assertFalse(gate.observe(300, true, true, false))
        assertFalse(gate.observe(400, true, false, false))
        assertFalse(gate.observe(899, true, false, false))
        assertTrue(gate.observe(900, true, false, false))
    }

    @Test fun unknownInsetsDetachedViewOrInputFocusCannotPass() {
        for (state in listOf(Triple(false, false, false), Triple(true, null, false), Triple(true, false, true))) {
            val gate = StableImeDismissal(500)
            assertFalse(gate.observe(0, true, false, false))
            assertFalse(gate.observe(600, state.first, state.second, state.third))
            assertFalse(gate.observe(700, true, false, false))
            assertFalse(gate.observe(1199, true, false, false))
            assertTrue(gate.observe(1200, true, false, false))
        }
    }

    @Test fun clockRegressionCannotPassTheGate() {
        val gate = StableImeDismissal(500)
        assertFalse(gate.observe(100, true, false, false))
        assertFalse(gate.observe(50, true, false, false))
        assertFalse(gate.observe(549, true, false, false))
        assertTrue(gate.observe(550, true, false, false))
    }
}
