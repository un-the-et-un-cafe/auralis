package chat.omcp.app

import android.graphics.Bitmap
import android.os.SystemClock
import android.view.WindowManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.StaleObjectException
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import chat.omcp.app.core.RustOmcpCore
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.regex.Pattern

/** Real shipped screens and Rust core, fictional local data, no production capture switch. */
@RunWith(AndroidJUnit4::class)
class PrivateUiCaptureTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val instrumentation get() = InstrumentationRegistry.getInstrumentation()
    private val device get() = UiDevice.getInstance(instrumentation)
    private val app get() = compose.activity.application as OmcpApplication
    private fun text(id: Int) = compose.activity.getString(id)
    private fun click(id: Int) = compose.onNodeWithText(text(id)).performScrollTo().performClick()
    private fun tab(id: Int) = compose.onNode(hasText(text(id)) and hasClickAction())

    private fun capture(name: String, marker: Int) {
        assertEquals("uiCapture", BuildConfig.BUILD_TYPE)
        assertEquals("chat.auralis.app.uicapture", instrumentation.targetContext.packageName)
        compose.onNodeWithText(text(marker)).assertIsDisplayed()
        compose.waitForIdle()
        val markerObject = device.wait(Until.findObject(By.text(text(marker))), 5000)
        assertNotNull("Expected displayed hero text in the real accessibility hierarchy", markerObject)
        val bounds = markerObject!!.visibleBounds
        assertFalse("Hero text must have visible screen bounds", bounds.isEmpty)
        val output = File(instrumentation.targetContext.getExternalFilesDir(null), "ui-captures")
        assertTrue(output.isDirectory || output.mkdirs())
        compose.runOnIdle {
            assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        try {
            // The secure-flag transition can recreate a compositor surface after Compose
            // becomes semantically idle. Require redraw, accessibility readiness and three
            // identical actual compositor frames, not a single arbitrary early screenshot.
            SystemClock.sleep(1000)
            compose.runOnIdle {
                compose.activity.window.decorView.requestLayout()
                compose.activity.window.decorView.invalidate()
            }
            instrumentation.waitForIdleSync()
            device.waitForIdle(2000)
            val deadline = SystemClock.elapsedRealtime() + 10000
            var previous: Bitmap? = null
            var stableCount = 0
            var captured = false
            while (SystemClock.elapsedRealtime() < deadline) {
                compose.onNodeWithText(text(marker)).assertIsDisplayed()
                val current = requireNotNull(instrumentation.uiAutomation.takeScreenshot())
                stableCount = if (previous?.sameAs(current) == true) stableCount + 1 else 1
                previous?.recycle()
                previous = current
                if (stableCount >= 3) {
                    // A semantically present but visually absent hero is not a valid capture.
                    // Inspect the real screenshot pixels under the accessibility text bounds.
                    var min = 255
                    var max = 0
                    for (y in bounds.top.coerceAtLeast(0) until bounds.bottom.coerceAtMost(current.height)) {
                        for (x in bounds.left.coerceAtLeast(0) until bounds.right.coerceAtMost(current.width)) {
                            val pixel = current.getPixel(x, y)
                            val brightness = (((pixel shr 16) and 255) + ((pixel shr 8) and 255) + (pixel and 255)) / 3
                            min = minOf(min, brightness)
                            max = maxOf(max, brightness)
                        }
                    }
                    assertTrue("Hero text absent from stable compositor pixels", max - min >= 40)
                    File(output, "$name.png").outputStream().use {
                        assertTrue(current.compress(Bitmap.CompressFormat.PNG, 100, it))
                    }
                    captured = true
                    break
                }
                SystemClock.sleep(500)
            }
            previous?.recycle()
            assertTrue("Compositor did not settle within 10 seconds", captured)
        } finally {
            compose.runOnIdle {
                compose.activity.window.addFlags(WindowManager.LayoutParams.FLAG_SECURE)
                assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            }
        }
    }

    private fun hideKeyboard() {
        compose.runOnIdle {
            compose.activity.currentFocus?.clearFocus()
            WindowCompat.getInsetsController(compose.activity.window, compose.activity.window.decorView)
                .hide(WindowInsetsCompat.Type.ime())
        }
        compose.waitForIdle()
        SystemClock.sleep(600)
    }

    private fun denyPermissions(required: Boolean) {
        val deny = By.res(Pattern.compile(".*:id/permission_deny(?:_and_dont_ask_again)?_button"))
        fun denyOne(timeoutMs: Long): Boolean {
            val deadline = SystemClock.elapsedRealtime() + timeoutMs
            while (SystemClock.elapsedRealtime() < deadline) {
                val button = device.wait(Until.findObject(deny), (deadline - SystemClock.elapsedRealtime()).coerceAtLeast(1)) ?: return false
                try { button.click(); return true } catch (_: StaleObjectException) { /* Reacquire within deadline. */ }
            }
            return false
        }
        val firstDenied = denyOne(if (required) 5000 else 1000)
        if (required) assertTrue("Expected the real nearby-device permission dialog", firstDenied)
        if (firstDenied) {
            for (dialog in 1..2) if (!denyOne(1000)) break
        }
        assertNull("Permission dialog must be dismissed", device.wait(Until.findObject(deny), 1000))
        compose.waitForIdle()
        assertFalse(chat.omcp.app.mesh.MeshPermissions.hasBle(compose.activity))
    }

    @Test fun captureMissingWelcomeAndMeshWithFictionalLocalData() {
        assertEquals("uiCapture", BuildConfig.BUILD_TYPE)
        assertEquals("chat.auralis.app.uicapture", instrumentation.targetContext.packageName)
        assertFalse(BuildConfig.SIMULATED_DRIVE_BACKUP)
        assertEquals("", BuildConfig.GOOGLE_OAUTH_CLIENT_ID)
        assertTrue(app.repository.core is RustOmcpCore)
        assertNull("Only a fresh disposable install is allowed", app.repository.core.identity())
        capture("01-welcome", R.string.auralis_welcome_headline)
        click(R.string.onb_get_started)
        compose.onNodeWithText(text(R.string.onb_display_name)).performTextInput("Alex")
        hideKeyboard()
        click(R.string.cta_continue)
        // Never enter or capture recovery-word screens.
        click(R.string.backup_remind_later)
        click(R.string.onb_start_chatting)
        tab(R.string.tab_chats).assertIsSelected()
        tab(R.string.tab_mesh).performClick()
        click(R.string.onb_find_nearby)
        denyPermissions(required = true)
        tab(R.string.tab_mesh).assertIsSelected()
        compose.onNodeWithText(text(R.string.auralis_mesh_eyebrow)).performScrollTo()
        compose.onNodeWithText(text(R.string.auralis_nearby_connections)).assertIsDisplayed()
        compose.onNodeWithText(text(R.string.auralis_waiting_messages)).assertIsDisplayed()
        assertEquals(0, app.repository.status.value.nearbyPeers)
        assertTrue(app.repository.messages.value.values.flatten().isEmpty())
        capture("07-mesh", R.string.auralis_mesh_title)
    }
}
