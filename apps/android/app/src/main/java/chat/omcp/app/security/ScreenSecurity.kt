package chat.omcp.app.security

import android.app.Activity
import android.app.Application
import android.os.Build
import android.os.Bundle
import android.view.Window
import android.view.WindowManager

/**
 * No screenshots anywhere in Auralis (REQUIREMENTS "Product decisions (2026-10-24)" item 5, SS1–SS2).
 * Every activity window (ours and third-party, e.g. the QR scanner) gets FLAG_SECURE before
 * any content is drawn, so screenshots, screen recording, casting and the recent-apps
 * thumbnail are black. Compose dialogs and bottom sheets use SecureFlagPolicy.Inherit (the
 * default), so they inherit FLAG_SECURE from the activity window. The flag is never cleared.
 */
object ScreenSecurity {
    const val FLAGS = WindowManager.LayoutParams.FLAG_SECURE

    fun secure(window: Window) = window.setFlags(FLAGS, FLAGS)

    fun secure(activity: Activity) {
        secure(activity.window)
        // Android 13+: also opt out of the recents screenshot (belt and braces for SS2).
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) activity.setRecentsScreenshotEnabled(false)
    }

    /** Registered in Application.onCreate; [apply] is injectable for JVM tests. */
    class Callbacks(private val apply: (Activity) -> Unit = ::secure) : Application.ActivityLifecycleCallbacks {
        override fun onActivityPreCreated(activity: Activity, savedInstanceState: Bundle?) = apply(activity)
        override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = apply(activity)
        override fun onActivityStarted(activity: Activity) {}
        override fun onActivityResumed(activity: Activity) {}
        override fun onActivityPaused(activity: Activity) {}
        override fun onActivityStopped(activity: Activity) {}
        override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) {}
        override fun onActivityDestroyed(activity: Activity) {}
    }
}
