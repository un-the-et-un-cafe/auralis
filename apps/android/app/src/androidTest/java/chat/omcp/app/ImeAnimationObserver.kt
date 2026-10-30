package chat.omcp.app

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.WindowInsetsAnimation

/**
 * Test-only, main-thread observer. A zero-size, noninteractive decor child receives
 * insets without replacing the app/Compose callbacks or consuming their events.
 * This observes prepared/running animations, NOT queued IME service transactions.
 */
internal class ImeAnimationObserver(private val application: Application) :
    Application.ActivityLifecycleCallbacks {
    private val probes = mutableMapOf<Activity, Probe>()

    fun start(activity: Activity) {
        application.registerActivityLifecycleCallbacks(this)
        install(activity)
    }

    fun stop() {
        application.unregisterActivityLifecycleCallbacks(this)
        probes.keys.toList().forEach(::remove)
    }

    fun snapshot(activity: Activity): Snapshot {
        val probe = probes[activity]
        return Snapshot(
            ready = probe?.isAttachedToWindow == true && probe.receivedInsets,
            active = probe?.animations?.active == true,
            generation = probe?.animations?.generation ?: -1,
            window = probe,
        )
    }

    // Diagnostic-only main-thread lookup: never wait on ActivityScenario or guess
    // a window when recreation temporarily leaves zero/multiple resumed activities.
    fun resumedActivity(): MainActivity? = probes.keys.filterIsInstance<MainActivity>()
        .singleOrNull { it.lifecycle.currentState == androidx.lifecycle.Lifecycle.State.RESUMED }

    data class Snapshot(val ready: Boolean, val active: Boolean, val generation: Long, val window: Any?)

    private fun install(activity: Activity) {
        if (activity !is MainActivity || probes.containsKey(activity)) return
        val decor = activity.window.decorView as ViewGroup
        val probe = Probe(activity)
        probes[activity] = probe
        decor.addView(probe, ViewGroup.LayoutParams(0, 0))
        probe.requestApplyInsets()
    }

    private fun remove(activity: Activity) {
        probes.remove(activity)?.let {
            it.setWindowInsetsAnimationCallback(null)
            (it.parent as? ViewGroup)?.removeView(it)
        }
    }

    // Runs after onCreate (including setContent), before start/resume/first traversal.
    override fun onActivityPostCreated(activity: Activity, savedInstanceState: Bundle?) = install(activity)
    override fun onActivityDestroyed(activity: Activity) = remove(activity)
    override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = Unit
    override fun onActivityStarted(activity: Activity) = Unit
    override fun onActivityResumed(activity: Activity) = Unit
    override fun onActivityPaused(activity: Activity) = Unit
    override fun onActivityStopped(activity: Activity) = Unit
    override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit

    private class Probe(activity: Activity) : View(activity) {
        val animations = ImeAnimationState()
        var receivedInsets = false
        init {
            isFocusable = false
            isClickable = false
            importantForAccessibility = IMPORTANT_FOR_ACCESSIBILITY_NO
            setWindowInsetsAnimationCallback(object : WindowInsetsAnimation.Callback(DISPATCH_MODE_CONTINUE_ON_SUBTREE) {
                private fun record(event: String, animation: WindowInsetsAnimation, ending: Boolean = false) {
                    if (animation.typeMask and WindowInsets.Type.ime() == 0) return
                    if (ending) animations.end(animation) else animations.begin(animation)
                    Log.i("ImeAnimationObserver", "$event window=${System.identityHashCode(this@Probe)} generation=${animations.generation} active=${animations.active}")
                }
                override fun onPrepare(animation: WindowInsetsAnimation) = record("prepare", animation)
                override fun onStart(animation: WindowInsetsAnimation, bounds: WindowInsetsAnimation.Bounds): WindowInsetsAnimation.Bounds {
                    record("start", animation)
                    return bounds
                }
                override fun onProgress(insets: WindowInsets, runningAnimations: MutableList<WindowInsetsAnimation>): WindowInsets {
                    // Add, never replace: a prepared animation may not yet be running.
                    runningAnimations.filter { it.typeMask and WindowInsets.Type.ime() != 0 }
                        .forEach { animations.begin(it) }
                    return insets
                }
                override fun onEnd(animation: WindowInsetsAnimation) = record("end", animation, ending = true)
            })
        }
        override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
            receivedInsets = true
            return insets
        }
    }
}
