package chat.omcp.app

import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.view.WindowManager
import androidx.compose.ui.platform.ViewRootForTest
import androidx.compose.ui.semantics.SemanticsConfiguration
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import java.util.ArrayDeque
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/**
 * One diagnostic experiment, only around the pre-identity interrupted-profile Back.
 * No screenshots, shell dumps, raw semantics, text, identity material or exception messages.
 * Posts do not wait for Compose idleness or initialize UiDevice. Request/observation times
 * are distinct: these are best-effort Activity-decor observations, NOT atomic Back routing.
 * Traversal has count/time budgets; individual framework getters cannot be preempted.
 */
internal class OnboardingBackDiagnostics(private val observer: ImeAnimationObserver) {
    enum class Phase {
        BEFORE_HIDE, IME_READY, BEFORE_SYSTEM_BACK, AFTER_SYSTEM_BACK,
        BACK_OR_READINESS_FAILED, DESTINATION_WAIT_FAILED, DESTINATION_ASSERTIONS_PASSED,
    }

    private val handler = Handler(Looper.getMainLooper())
    private val token = Any()
    private val closed = AtomicBoolean(false)
    private val requests = AtomicInteger(0)

    // Diagnostic methods must never replace the original assertion/Back exception.
    fun request(phase: Phase) = bestEffort {
        if (closed.get()) return@bestEffort
        val sequence = requests.incrementAndGet()
        if (sequence > MAX_SNAPSHOTS) return@bestEffort
        val requestedMs = SystemClock.elapsedRealtime()
        Log.i(TAG, "request=$sequence phase=$phase requestedMs=$requestedMs")
        handler.postAtTime({
            if (!closed.get()) bestEffort { snapshot(sequence, phase, requestedMs) }
        }, token, SystemClock.uptimeMillis())
    }

    fun backReturned(result: Boolean, durationMs: Long) = bestEffort {
        Log.i(TAG, "backReturnedMs=${SystemClock.elapsedRealtime()} result=$result durationMs=$durationMs")
    }

    fun close() = bestEffort {
        val drained = CountDownLatch(1)
        handler.postAtTime({ drained.countDown() }, token, SystemClock.uptimeMillis())
        val complete = try {
            drained.await(DRAIN_MS, TimeUnit.MILLISECONDS)
        } catch (_: InterruptedException) {
            Thread.currentThread().interrupt()
            false
        } finally {
            closed.set(true)
            handler.removeCallbacksAndMessages(token)
        }
        Log.i(TAG, "drainComplete=$complete drainLimitMs=$DRAIN_MS")
    }

    private fun snapshot(sequence: Int, phase: Phase, requestedMs: Long) {
        val observedMs = SystemClock.elapsedRealtime()
        val activity = observer.resumedActivity()
        if (activity == null) {
            Log.i(TAG, "snapshot=$sequence phase=$phase requestedMs=$requestedMs observedMs=$observedMs state=NO_UNIQUE_RESUMED_ACTIVITY")
            return
        }
        val decor = activity.window.decorView
        val animation = observer.snapshot(activity)
        val imeVisible = ViewCompat.getRootWindowInsets(decor)?.isVisible(WindowInsetsCompat.Type.ime())
        val welcome = activity.getString(R.string.onb_get_started)
        val profile = activity.getString(R.string.onb_display_name)
        val views = ArrayDeque<View>().apply { add(decor) }
        val nodes = ArrayDeque<SemanticsNode>()
        var visitedViews = 0
        var visitedNodes = 0
        var roots = 0
        var pendingLayoutRoots = 0
        var welcomeCount = 0
        var profileCount = 0
        var focusedCount = 0
        var fixtureMatches = false
        var complete = true
        val deadline = observedMs + READ_BUDGET_MS
        fun withinBudget() = !closed.get() && SystemClock.elapsedRealtime() < deadline
        while (views.isNotEmpty() && visitedViews < MAX_VIEWS && withinBudget()) {
            val view = views.removeFirst()
            visitedViews++
            if (view is ViewRootForTest && view.isAttachedToWindow) {
                roots++
                if (view.hasPendingMeasureOrLayout) pendingLayoutRoots++
                nodes.add(view.semanticsOwner.rootSemanticsNode) // Merged, like default test queries.
            }
            if (view is ViewGroup) {
                val capacity = MAX_VIEWS - visitedViews - views.size
                if (view.childCount > capacity) complete = false
                for (index in 0 until minOf(view.childCount, capacity)) views.add(view.getChildAt(index))
            }
        }
        if (views.isNotEmpty()) complete = false
        while (nodes.isNotEmpty() && visitedNodes < MAX_NODES && withinBudget()) {
            val node = nodes.removeFirst()
            visitedNodes++
            val config = node.config
            if (config.matchesText(welcome)) welcomeCount++
            if (config.matchesText(profile)) profileCount++
            if (config.getOrNull(SemanticsProperties.Focused) == true) focusedCount++
            if (config.hasExactEditableText("Smoke Alice")) fixtureMatches = true
            val children = node.children
            val capacity = MAX_NODES - visitedNodes - nodes.size
            if (children.size > capacity) complete = false
            children.take(capacity).forEach(nodes::add)
        }
        if (nodes.isNotEmpty() || roots == 0) complete = false
        // This pre-identity fixture should hit RustOmcpCore's null-node fast path.
        // Log only the Boolean; never stringify a returned Identity, even on a defect.
        val identityNull = if (withinBudget()) {
            (activity.application as OmcpApplication).repository.core.identity() == null
        } else null
        if (closed.get()) {
            Log.i(TAG, "snapshot=$sequence phase=$phase state=EXPIRED_NO_STATE_PUBLISHED")
            return
        }
        Log.i(TAG, "snapshot=$sequence phase=$phase requestedMs=$requestedMs observedMs=$observedMs " +
            "readMs=${SystemClock.elapsedRealtime() - observedMs} activity=${System.identityHashCode(activity)} " +
            "decor=${System.identityHashCode(decor)} lifecycle=${activity.lifecycle.currentState.name} " +
            "attached=${decor.isAttachedToWindow} windowFocused=${activity.hasWindowFocus()} " +
            "viewFocused=${activity.currentFocus?.isFocused == true} imeVisible=$imeVisible " +
            "observerReady=${animation.ready} animationActive=${animation.active} generation=${animation.generation} " +
            "probe=${animation.window?.let(System::identityHashCode)} " +
            "secure=${activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0} " +
            "scope=ACTIVITY_DECOR_MERGED complete=$complete roots=$roots pendingLayoutRoots=$pendingLayoutRoots " +
            "views=$visitedViews nodes=$visitedNodes welcomeCount=$welcomeCount profileCount=$profileCount " +
            "focusedCount=$focusedCount fixtureMatches=$fixtureMatches identityNull=$identityNull")
    }

    private fun bestEffort(action: () -> Unit) {
        try { action() } catch (failure: Throwable) {
            // No throwable/stack/message overload: diagnostic errors can contain UI text.
            try { Log.i(TAG, "diagnosticUnavailable=${failure.javaClass.simpleName}") } catch (_: Throwable) { }
        }
    }

    companion object {
        private const val TAG = "OnboardingBackDiag"
        private const val MAX_SNAPSHOTS = 8
        private const val MAX_VIEWS = 256
        private const val MAX_NODES = 256
        private const val READ_BUDGET_MS = 50L
        private const val DRAIN_MS = 250L
    }
}

// Same exact-text predicate as default onAllNodesWithText, with no raw value logging.
internal fun SemanticsConfiguration.matchesText(expected: String): Boolean =
    getOrNull(SemanticsProperties.Text)?.any { it.text == expected } == true ||
        getOrNull(SemanticsProperties.EditableText)?.text == expected
