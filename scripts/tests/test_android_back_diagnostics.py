"""Source guardrails for the local diagnostic experiment; NOT Kotlin/runtime tests."""
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[2]
TESTS = ROOT / "apps/android/app/src/androidTest/java/chat/omcp/app"


class AndroidBackDiagnosticSourceTests(unittest.TestCase):
    def test_original_smoke_keeps_single_system_back_and_deadlines(self):
        smoke = (TESTS / "AppSmokeTest.kt").read_text()
        self.assertEqual(smoke.count("device.pressBack()"), 1)
        self.assertIn("StableImeDismissal(stableForMs = 500)", smoke)
        self.assertEqual(smoke.count("compose.waitUntil(10_000)"), 4)
        self.assertIn('compose.onNodeWithText("Smoke Alice").assertExists()', smoke)
        self.assertIn("back(diagnostics)", smoke)
        self.assertRegex(smoke, r"Phase.DESTINATION_WAIT_FAILED\)\s+throw failure")
        self.assertIn("compose.onNodeWithText(text(R.string.onb_display_name)).assertDoesNotExist()", smoke)
        self.assertIn("assertNull(app.repository.core.identity())", smoke)

    def test_normal_smoke_does_not_construct_diagnostics_without_explicit_opt_in(self):
        smoke = (TESTS / "AppSmokeTest.kt").read_text()
        self.assertEqual(smoke.count("OnboardingBackDiagnostics(imeAnimations)"), 1)
        self.assertRegex(smoke, r'val diagnostics = if \(InstrumentationRegistry.getArguments\(\)\s*'
                         r'\.getString\("auralisBackDiagnostics"\) == "true"\) \{\s*'
                         r'OnboardingBackDiagnostics\(imeAnimations\)\s*\} else null')
        # No eager factory, fallback, unconditional request/drain or permissive parser.
        self.assertNotIn("diagnostics.request(", smoke)
        self.assertNotIn("diagnostics.close()", smoke)
        self.assertIn("diagnostics?.close()", smoke)
        self.assertNotIn("toBoolean", smoke)

    def test_existing_ci_does_not_opt_into_diagnostic_observer(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        runner = (ROOT / "scripts/ci/run-android-emulator-tests.sh").read_text()
        self.assertNotIn("auralisBackDiagnostics", workflow)
        self.assertNotIn("auralisBackDiagnostics", runner)
        self.assertIn("connectedDebugAndroidTest", runner)

    def test_round_trip_control_is_isolated_and_preserves_original_observer_default(self):
        source = (TESTS / "OnboardingImeRoundTripControlTest.kt").read_text()
        observer = (TESTS / "ImeAnimationObserver.kt").read_text()
        self.assertIn("observedActivityClass: Class<out Activity> = MainActivity::class.java", observer)
        self.assertIn("!observedActivityClass.isInstance(activity)", observer)
        self.assertIn("MeshRepository(core, InMemoryAppStore())", source)
        self.assertIn("compose.activityRule.scenario.recreate()", source)
        self.assertIn("assertNotSame(previousActivity, it)", source)
        self.assertEqual(source.count("device.pressBack()"), 1)
        self.assertEqual(source.count("compose.waitUntil(10_000)"), 3)
        self.assertIn("StableImeDismissal(stableForMs = 500)", source)
        self.assertIn(".isVisible(WindowInsetsCompat.Type.ime()) == true", source)
        self.assertLess(source.index(".show(WindowInsetsCompat.Type.ime())"),
                        source.index(".hide(WindowInsetsCompat.Type.ime())"))
        self.assertIn('hasExactEditableText("Round-trip Alice")', source)
        self.assertEqual(source.count("assertNull(core.identity())"), 2)
        self.assertEqual(source.count("WindowManager.LayoutParams.FLAG_SECURE != 0"), 2)
        for forbidden in ("Thread.sleep", "assumeTrue", "Assume.", "OnboardingBackDiagnostics(",
                          "onBackPressedDispatcher", "setSoftInputMode", "clearFlags", "clearPackageData"):
            self.assertNotIn(forbidden, source)

    def test_snapshot_has_no_shell_raw_dump_or_idle_wait(self):
        source = (TESTS / "OnboardingBackDiagnostics.kt").read_text()
        for forbidden in ("executeShellCommand", "printToLog", "fetchSemanticsNodes(",
                          "runOnIdle(", "runOnMainSync(", "waitForIdle(",
                          "onViewCreatedCallback", "forceAccessibility", ".toString()"):
            self.assertNotIn(forbidden, source)
        self.assertIn("MAX_SNAPSHOTS = 8", source)
        self.assertIn("MAX_VIEWS = 256", source)
        self.assertIn("MAX_NODES = 256", source)
        self.assertIn("READ_BUDGET_MS = 50L", source)
        self.assertIn("DRAIN_MS = 250L", source)
        self.assertIn("removeCallbacksAndMessages(token)", source)

    def test_output_is_explicit_scalars_not_sensitive_objects(self):
        source = (TESTS / "OnboardingBackDiagnostics.kt").read_text()
        for interpolation in ("$activity", "$decor", "$node", "$config", "$failure",
                              "$welcome ", "$profile ", "$animation"):
            self.assertNotRegex(source, re.escape(interpolation) + r"(?:[\s\"}]|$)")
        self.assertIn("fixtureMatches=$fixtureMatches identityNull=$identityNull", source)
        self.assertIn("scope=ACTIVITY_DECOR_MERGED complete=$complete", source)
        self.assertIn("state=NO_UNIQUE_RESUMED_ACTIVITY", source)

    def test_control_is_additional_isolated_dispatcher_coverage(self):
        source = (TESTS / "OnboardingBackControlTest.kt").read_text()
        self.assertIn("MeshRepository(core, InMemoryAppStore())", source)
        self.assertEqual(source.count("onBackPressedDispatcher.onBackPressed()"), 1)
        self.assertIn("compose.onAllNodes(isFocused()).assertCountEquals(0)", source)
        self.assertIn("assertEquals(false, ViewCompat.getRootWindowInsets", source)
        self.assertEqual(source.count("assertNull(core.identity())"), 2)
        self.assertEqual(source.count("WindowManager.LayoutParams.FLAG_SECURE != 0"), 2)
        for forbidden in ("performTextInput(", "pressBack(", "hideKeyboard(",
                          ".clearFlags(", "setSoftInputMode(", ".hide(WindowInsets"):
            self.assertNotIn(forbidden, source)


if __name__ == "__main__":
    unittest.main()
