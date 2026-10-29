package chat.omcp.app

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

    private fun capture(name: String) {
        assertEquals("uiCapture", BuildConfig.BUILD_TYPE)
        assertEquals("chat.auralis.app.uicapture", instrumentation.targetContext.packageName)
        compose.waitForIdle()
        val output = File(instrumentation.targetContext.getExternalFilesDir(null), "ui-captures")
        assertTrue(output.isDirectory || output.mkdirs())
        compose.runOnIdle {
            assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            compose.activity.window.clearFlags(WindowManager.LayoutParams.FLAG_SECURE)
        }
        try {
            // Wait for the compositor to apply the test-only window flag transition.
            SystemClock.sleep(300)
            assertTrue(device.takeScreenshot(File(output, "$name.png")))
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

    @Test fun captureImplementedScreensWithFictionalLocalData() {
        assertEquals("uiCapture", BuildConfig.BUILD_TYPE)
        assertEquals("chat.auralis.app.uicapture", instrumentation.targetContext.packageName)
        assertFalse(BuildConfig.SIMULATED_DRIVE_BACKUP)
        assertEquals("", BuildConfig.GOOGLE_OAUTH_CLIENT_ID)
        assertTrue(app.repository.core is RustOmcpCore)
        assertNull("Only a fresh disposable install is allowed", app.repository.core.identity())
        capture("01-welcome")
        click(R.string.onb_get_started)
        compose.onNodeWithText(text(R.string.onb_display_name)).performTextInput("Alex")
        hideKeyboard()
        capture("02-profile")
        click(R.string.cta_continue)
        // Never enter or capture recovery-word screens.
        click(R.string.backup_remind_later)
        click(R.string.onb_start_chatting)
        tab(R.string.tab_chats).assertIsSelected()
        capture("03-empty-chats")
        // Follow the already validated real-app smoke's permission order.
        tab(R.string.tab_mesh).performClick()
        click(R.string.onb_find_nearby)
        denyPermissions(required = true)
        tab(R.string.tab_mesh).assertIsSelected()
        assertFalse(chat.omcp.app.mesh.MeshPermissions.hasBle(compose.activity))
        tab(R.string.tab_chats).performClick()

        val peer = RustOmcpCore().also { it.load(it.generateRecoveryPhrase(), "Sam") }
        compose.onNodeWithContentDescription(text(R.string.contact_add_title)).performClick()
        click(R.string.contact_add_peer_id)
        compose.onNodeWithText("omcp:1:…").performScrollTo().performTextInput(peer.qrPayload())
        click(R.string.cta_continue)
        click(R.string.chat_without_verifying)
        denyPermissions(required = false)
        compose.onNodeWithTag("message-composer").performTextInput("See you at the park at 3!")
        hideKeyboard()
        capture("04-message-draft")
        compose.onNodeWithContentDescription(text(R.string.file_send)).performClick()
        hideKeyboard()
        compose.runOnIdle {
            val message = app.repository.messages.value.values.flatten().single()
            assertEquals("See you at the park at 3!", message.text)
            assertEquals(chat.omcp.app.data.ChatMessage.Status.Queued, message.status)
            assertEquals("Sam", app.repository.contacts.value.single().displayName)
        }
        capture("05-queued-message")
        device.pressBack()
        compose.waitForIdle()
        tab(R.string.tab_chats).assertIsSelected()
        capture("06-chat-list")
        tab(R.string.tab_mesh).performClick()
        capture("07-mesh")
        tab(R.string.tab_you).performClick()
        capture("08-you")
        click(R.string.backup_settings_title)
        capture("09-backup-settings")
    }
}
