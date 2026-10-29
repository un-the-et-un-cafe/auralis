package chat.omcp.app

import org.junit.Assert.*
import org.junit.Test

/** State gate only: it never sends Back, hides the keyboard, or retries an action. */
internal class StableImeDismissal(private val stableForMs: Long) {
    init { require(stableForMs > 0) }
    private var hiddenSince: Long? = null
    private var lastGeneration: Long? = null
    private var lastWindow: Any? = null

    fun observe(
        nowMs: Long, attached: Boolean, imeVisible: Boolean?, inputFocused: Boolean,
        observerReady: Boolean = true, animationActive: Boolean = false,
        animationGeneration: Long = 0, window: Any? = null,
    ): Boolean {
        if (lastGeneration != animationGeneration || lastWindow !== window) hiddenSince = null
        lastGeneration = animationGeneration
        lastWindow = window
        if (!attached || imeVisible != false || inputFocused || !observerReady || animationActive) {
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

    @Test fun preparedAnimationAndOverlappingAnimationsRequireEveryEnd() {
        val state = ImeAnimationState()
        val first = Any(); val second = Any()
        state.begin(first); state.begin(first)
        assertEquals(1L, state.generation)
        state.begin(second); state.end(first)
        assertTrue(state.active)
        state.end(second)
        assertFalse(state.active)
    }

    @Test fun animationEntirelyBetweenPollsRestartsReadiness() {
        val gate = StableImeDismissal(500)
        assertFalse(gate.observe(0, true, false, false, animationGeneration = 0))
        assertFalse(gate.observe(600, true, false, false, animationGeneration = 2))
        assertTrue(gate.observe(1100, true, false, false, animationGeneration = 2))
    }

    @Test fun activeAnimationOrUnregisteredObserverCannotPass() {
        for ((ready, active) in listOf(false to false, true to true)) {
            val gate = StableImeDismissal(500)
            assertFalse(gate.observe(0, true, false, false))
            assertFalse(gate.observe(600, true, false, false, ready, active))
            assertFalse(gate.observe(700, true, false, false))
            assertTrue(gate.observe(1200, true, false, false))
        }
    }

    @Test fun recreatedWindowCannotInheritOldReadiness() {
        val gate = StableImeDismissal(500)
        val old = Any(); val replacement = Any()
        assertFalse(gate.observe(0, true, false, false, window = old))
        assertFalse(gate.observe(600, true, false, false, window = replacement))
        assertTrue(gate.observe(1100, true, false, false, window = replacement))
    }

    @Test fun clockRegressionCannotPassTheGate() {
        val gate = StableImeDismissal(500)
        assertFalse(gate.observe(100, true, false, false))
        assertFalse(gate.observe(50, true, false, false))
        assertFalse(gate.observe(549, true, false, false))
        assertTrue(gate.observe(550, true, false, false))
    }
}

/** Prepared animations remain active until end, including cancellation before start (API 31+). */
internal class ImeAnimationState {
    private val activeAnimations = java.util.Collections.newSetFromMap(java.util.IdentityHashMap<Any, Boolean>())
    var generation: Long = 0
        private set
    val active: Boolean get() = activeAnimations.isNotEmpty()
    fun begin(animation: Any) { if (activeAnimations.add(animation)) generation++ }
    fun end(animation: Any) { activeAnimations.remove(animation); generation++ }
}
