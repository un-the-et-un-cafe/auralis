package chat.omcp.app

import android.view.WindowManager
import android.os.SystemClock
import android.util.Log
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.text.AnnotatedString
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.By
import androidx.test.uiautomator.UiDevice
import androidx.test.uiautomator.Until
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.BackupGate
import chat.omcp.app.mesh.MeshPermissions
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.util.regex.Pattern

/**
 * Fresh-install smoke on a disposable emulator/device. Uses the shipped Application,
 * encrypted local store and native Rust core; no fake core or simulated cloud backup.
 * Run on API 31 plus API 35 at 200% font / dark mode. Never disables FLAG_SECURE.
 */
@RunWith(AndroidJUnit4::class)
class AppSmokeTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    private val device get() = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
    private val app get() = compose.activity.application as OmcpApplication
    private fun text(id: Int) = compose.activity.getString(id)
    private fun clickText(id: Int) = compose.onNodeWithText(text(id)).performScrollTo().performClick()
    private fun tab(id: Int) = compose.onNode(hasText(text(id)) and hasClickAction())
    private fun composer() = compose.onNodeWithTag("message-composer")
    private fun assertDraft(expected: String) = composer().assert(
        SemanticsMatcher.expectValue(SemanticsProperties.EditableText, AnnotatedString(expected)),
    )

    private fun recreateThreadWithDraft(expected: String) {
        // Establish that text input has committed before testing whether recreation retains it.
        assertDraft(expected)
        compose.activityRule.scenario.recreate()
        // recreate() waits for Activity RESUMED, not the restored Compose/NavHost destination.
        // Wait only for the editor to exist: an empty or changed draft MUST still fail below.
        val started = SystemClock.elapsedRealtime()
        compose.waitUntil(10_000) {
            compose.onAllNodesWithTag("message-composer").fetchSemanticsNodes().size == 1
        }
        Log.i("AppSmokeTest", "Restored composer attached after ${SystemClock.elapsedRealtime() - started} ms")
        assertDraft(expected)
    }
    private fun back() { hideKeyboard(); device.pressBack(); compose.waitForIdle() }
    private fun hideKeyboard() {
        compose.runOnIdle {
            WindowCompat.getInsetsController(compose.activity.window, compose.activity.window.decorView)
                .hide(WindowInsetsCompat.Type.ime())
        }
        device.waitForIdle()
    }

    private fun assertSecure() = compose.runOnIdle {
        assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
    }

    /** Both AOSP and Google permission controllers, without relying on translated button text. */
    private fun denyPermissions(required: Boolean) {
        val deny = By.res(Pattern.compile(".*:id/permission_deny(?:_and_dont_ask_again)?_button"))
        var button = device.wait(Until.findObject(deny), if (required) 5_000 else 1_000)
        if (required) assertNotNull("Expected the real Android permission dialog", button)
        repeat(3) {
            button?.click() ?: return@repeat
            button = device.wait(Until.findObject(deny), 1_000)
        }
        assertNull("Permission dialog did not dismiss", button)
        compose.waitForIdle()
        assertFalse(MeshPermissions.hasBle(compose.activity))
    }

    @Test fun realAppOnboardingDeniedPermissionsContactMessageAndRecreation() {
        assertFalse("Smoke must not opt into simulated Drive", BuildConfig.SIMULATED_DRIVE_BACKUP)
        assertTrue(app.repository.core is RustOmcpCore)
        assertNull("Run on a fresh install (adb shell pm clear chat.auralis.app.debug)", app.repository.core.identity())
        assertSecure()

        clickText(R.string.onb_get_started)
        compose.onNodeWithText(text(R.string.onb_display_name)).performTextInput("Smoke Alice")
        compose.activityRule.scenario.recreate()
        compose.onNodeWithText("Smoke Alice").assertExists()
        // Back from an interrupted profile must not create or replace an identity.
        back()
        assertNull(app.repository.core.identity())
        clickText(R.string.onb_get_started)
        clickText(R.string.cta_continue)
        clickText(R.string.backup_remind_later)
        assertNotNull(app.repository.core.identity())
        val peerId = app.repository.core.identity()!!.peerIdHex
        clickText(R.string.onb_start_chatting)
        tab(R.string.tab_chats).assertIsSelected()
        assertEquals(BackupGate.Banner, app.repository.backupGate())

        tab(R.string.tab_mesh).performClick()
        clickText(R.string.onb_find_nearby)
        denyPermissions(required = true)
        tab(R.string.tab_mesh).assertIsSelected()
        assertEquals(0, app.repository.status.value.nearbyPeers)

        tab(R.string.tab_you).performClick()
        clickText(R.string.backup_settings_title)
        compose.onNodeWithText(text(R.string.backup_settings_title)).assertIsDisplayed()
        back()
        tab(R.string.tab_you).assertIsSelected()
        clickText(R.string.security_centre)
        compose.onNodeWithText(text(R.string.auralis_security_empty)).assertIsDisplayed()
        back()
        tab(R.string.tab_you).assertIsSelected()
        assertSecure()

        // A real peer QR exercises parsing, verification preview, contact persistence and queueing.
        val peer = RustOmcpCore().also { it.load(it.generateRecoveryPhrase(), "Smoke Bob") }
        tab(R.string.tab_chats).performClick()
        compose.onNodeWithContentDescription(text(R.string.contact_add_title)).performClick()
        clickText(R.string.contact_add_peer_id)
        compose.onNodeWithText("omcp:1:…").performScrollTo().performTextInput(peer.qrPayload())
        clickText(R.string.cta_continue)
        clickText(R.string.chat_without_verifying)
        denyPermissions(required = false)
        composer().performTextInput("Draft survives rotation")
        recreateThreadWithDraft("Draft survives rotation")
        assertEquals(peerId, app.repository.core.identity()!!.peerIdHex)
        assertSecure()
        compose.onNodeWithContentDescription(text(R.string.file_send)).performClick()
        compose.onNodeWithText("Draft survives rotation").assertExists()
        compose.runOnIdle {
            val message = app.repository.messages.value.values.flatten().single()
            assertEquals("Draft survives rotation", message.text)
            assertEquals(chat.omcp.app.data.ChatMessage.Status.Queued, message.status)
            assertEquals("Smoke Bob", app.repository.contacts.value.single().displayName)
        }
        back()
        tab(R.string.tab_chats).assertIsSelected()
        compose.onNodeWithText("Smoke Bob").assertExists()
        assertSecure()

        // Exercise deadline expiry in the foreground, not just the pure repository helper.
        compose.runOnIdle { app.repository.appStore.backupRemindDeadlineMs = System.currentTimeMillis() + 2_000 }
        compose.activityRule.scenario.recreate()
        compose.waitUntil(10_000) {
            compose.onAllNodesWithText(text(R.string.onb_secure_title)).fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText(text(R.string.backup_remind_later)).assertDoesNotExist()
        assertEquals(BackupGate.Required, app.repository.backupGate())
        assertEquals(peerId, app.repository.core.identity()!!.peerIdHex)
        assertSecure()
        clickText(R.string.backup_written_cta)
        compose.activityRule.scenario.recreate()
        compose.onNodeWithText(text(R.string.onb_words_body)).assertExists()
        clickText(R.string.onb_wrote_it)
        compose.activityRule.scenario.recreate()
        compose.onNodeWithText(text(R.string.onb_quiz_title)).assertExists()
        assertSecure()
    }
}
