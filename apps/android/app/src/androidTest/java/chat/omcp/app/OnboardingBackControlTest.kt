package chat.omcp.app

import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import chat.omcp.app.backup.DriveAppDataBackupStore
import chat.omcp.app.core.RustOmcpCore
import chat.omcp.app.data.InMemoryAppStore
import chat.omcp.app.data.MeshRepository
import chat.omcp.app.security.ScreenSecurity
import chat.omcp.app.ui.screens.OnboardingFlow
import chat.omcp.app.ui.theme.OmcpTheme
import chat.omcp.app.ui.vm.OnboardingStep
import chat.omcp.app.ui.vm.OnboardingViewModel
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * Additional screen/dispatcher control, NOT real-system-Back or recreation coverage.
 * Its isolated in-memory store never reads/clears the real app fixture's identity.
 * No editor is touched, no keyboard is disabled/hidden, no mock BackHandler is installed.
 */
@RunWith(AndroidJUnit4::class)
class OnboardingBackControlTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun profileDispatcherBackWithoutImeReturnsToWelcomeWithoutIdentity() {
        val core = RustOmcpCore()
        val repo = MeshRepository(core, InMemoryAppStore())
        val cloud = DriveAppDataBackupStore("") { error("Unconfigured control must not access cloud") }
        val vm = OnboardingViewModel(repo, cloud)
        compose.setContent {
            OmcpTheme {
                OnboardingFlow(vm, myQr = {}, onFindNearby = {}, secureWindow = {
                    ScreenSecurity.secure(compose.activity.window)
                })
            }
        }
        fun text(id: Int) = compose.activity.getString(id)
        compose.onNodeWithText(text(R.string.onb_get_started)).performScrollTo().performClick()
        compose.onNodeWithText(text(R.string.onb_display_name)).assertExists()
        compose.onAllNodes(isFocused()).assertCountEquals(0)
        compose.runOnIdle {
            assertEquals(OnboardingStep.Profile, vm.state.value.step)
            assertNull(core.identity())
            val window = compose.activity.window
            assertTrue(window.decorView.isAttachedToWindow)
            assertTrue(compose.activity.hasWindowFocus())
            assertEquals(false, ViewCompat.getRootWindowInsets(window.decorView)?.isVisible(WindowInsetsCompat.Type.ime()))
            assertTrue(window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            compose.activity.onBackPressedDispatcher.onBackPressed() // One dispatcher invocation, not a system Back retry.
        }
        compose.waitUntil(10_000) {
            compose.onAllNodesWithText(text(R.string.onb_get_started)).fetchSemanticsNodes().size == 1
        }
        compose.onNodeWithText(text(R.string.onb_display_name)).assertDoesNotExist()
        compose.runOnIdle {
            assertEquals(OnboardingStep.Welcome, vm.state.value.step)
            assertNull(core.identity())
            assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
        }
    }
}
