package chat.omcp.app

import android.os.SystemClock
import android.view.WindowManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import androidx.test.uiautomator.UiDevice
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
 * Isolated production-screen/IME/system-Back control. Retains a test-owned VM
 * across a real ComponentActivity recreation and explicitly reattaches the UI.
 * NOT MainActivity restoration or persistent-store coverage; AppSmokeTest stays intact.
 */
@RunWith(AndroidJUnit4::class)
class OnboardingImeRoundTripControlTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test fun recreatedProfileWithObservedImeRoundTripNavigatesWithOneSystemBack() {
        val core = RustOmcpCore()
        val repo = MeshRepository(core, InMemoryAppStore())
        val cloud = DriveAppDataBackupStore("") { error("Control must not access cloud") }
        val vm = OnboardingViewModel(repo, cloud)
        val observer = ImeAnimationObserver(compose.activity.application, ComponentActivity::class.java)
        fun attachContent(activity: ComponentActivity) {
            ScreenSecurity.secure(activity)
            activity.setContent {
                OmcpTheme {
                    OnboardingFlow(vm, myQr = {}, onFindNearby = {}, secureWindow = {
                        ScreenSecurity.secure(activity.window)
                    })
                }
            }
        }
        fun text(id: Int) = compose.activity.getString(id)
        fun assertFixture() {
            compose.onNodeWithText(text(R.string.onb_display_name)).assert(
                SemanticsMatcher("Exact synthetic draft retained") {
                    it.config.hasExactEditableText("Round-trip Alice")
                },
            )
            compose.runOnIdle {
                assertEquals(OnboardingStep.Profile, vm.state.value.step)
                assertNull(core.identity())
                assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            }
        }
        compose.activityRule.scenario.onActivity {
            observer.start(it)
            attachContent(it)
        }
        try {
            compose.onNodeWithText(text(R.string.onb_get_started)).performScrollTo().performClick()
            compose.onNodeWithText(text(R.string.onb_display_name)).performTextInput("Round-trip Alice")
            assertFixture()
            val previousActivity = compose.activity
            compose.activityRule.scenario.recreate()
            compose.activityRule.scenario.onActivity {
                assertNotSame(previousActivity, it)
                attachContent(it)
            }
            assertFixture()
            // Establish an actual visible-IME acknowledgement before attempting hide.
            // A request return value or generation zero alone is not acknowledgement.
            compose.onNodeWithText(text(R.string.onb_display_name)).performClick()
            compose.runOnIdle {
                assertTrue(compose.activity.hasWindowFocus())
                WindowCompat.getInsetsController(compose.activity.window, compose.activity.window.decorView)
                    .show(WindowInsetsCompat.Type.ime())
            }
            compose.waitUntil(10_000) {
                compose.runOnIdle {
                    val decor = compose.activity.window.decorView
                    decor.isAttachedToWindow && compose.activity.hasWindowFocus() &&
                        observer.snapshot(compose.activity).ready &&
                        ViewCompat.getRootWindowInsets(decor)?.isVisible(WindowInsetsCompat.Type.ime()) == true
                }
            }
            assertFixture()
            compose.runOnIdle {
                compose.activity.currentFocus?.clearFocus()
                WindowCompat.getInsetsController(compose.activity.window, compose.activity.window.decorView)
                    .hide(WindowInsetsCompat.Type.ime())
            }
            val gate = StableImeDismissal(stableForMs = 500)
            compose.waitUntil(10_000) {
                val sample = compose.runOnIdle {
                    val decor = compose.activity.window.decorView
                    Triple(decor.isAttachedToWindow,
                        ViewCompat.getRootWindowInsets(decor)?.isVisible(WindowInsetsCompat.Type.ime()),
                        observer.snapshot(compose.activity))
                }
                val focused = compose.onAllNodes(isFocused()).fetchSemanticsNodes().isNotEmpty()
                val animation = sample.third
                gate.observe(SystemClock.elapsedRealtime(), sample.first, sample.second, focused,
                    animation.ready, animation.active, animation.generation, animation.window)
            }
            val device = UiDevice.getInstance(InstrumentationRegistry.getInstrumentation())
            assertTrue("System Back injection must succeed", device.pressBack()) // Exactly once.
            compose.waitUntil(10_000) {
                compose.onAllNodesWithText(text(R.string.onb_get_started)).fetchSemanticsNodes().size == 1
            }
            compose.onNodeWithText(text(R.string.onb_display_name)).assertDoesNotExist()
            compose.runOnIdle {
                assertEquals(OnboardingStep.Welcome, vm.state.value.step)
                assertNull(core.identity())
                assertTrue(compose.activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0)
            }
        } finally {
            InstrumentationRegistry.getInstrumentation().runOnMainSync { observer.stop() }
        }
    }
}
