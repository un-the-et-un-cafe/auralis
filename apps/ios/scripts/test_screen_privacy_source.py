"""Source-regression guardrails only; not Swift compilation or iOS runtime evidence."""
from pathlib import Path
import re
import unittest

IOS = Path(__file__).resolve().parents[1]
SOURCE = IOS / "OpenMeshChat/Security/SecureCapture.swift"


def code():
    return re.sub(r"//[^\n]*", "", SOURCE.read_text())


class ScreenPrivacySourceTests(unittest.TestCase):
    def test_no_private_canvas_reparenting_or_global_screen_selection(self):
        source = code()
        for banned in ["secureCanvasLayer", "CanvasView", "addSublayer", "isSecureTextEntry",
                       "UIScreen.main", "connectedScenes", "makeKeyAndVisible", "UIVisualEffectView"]:
            self.assertNotIn(banned, source)

    def test_opaque_non_key_window_and_source_interaction_boundary(self):
        source = code()
        for required in ["UIWindow(windowScene:", "canBecomeKey: Bool { false }",
                         "UIWindow.Level.alert.rawValue + 1", "view.isOpaque = true",
                         "window.endEditing(true)", "window.isUserInteractionEnabled = false",
                         "window.accessibilityElementsHidden = true"]:
            # The concrete window is a UIWindow subclass; accept its explicit initializer.
            if required == "UIWindow(windowScene:":
                self.assertIn("PrivacyCoverWindow(windowScene: scene)", source)
            else:
                self.assertIn(required, source)

    def test_scene_notifications_are_synchronous_and_cleanup_is_explicit(self):
        source = code()
        for required in ["UIScene.willDeactivateNotification, object: scene",
                         "UIScene.didEnterBackgroundNotification, object: scene",
                         "UIScene.willEnterForegroundNotification, object: scene",
                         "UIScene.didActivateNotification, object: scene",
                         "UIScene.didDisconnectNotification, object: scene",
                         "object: object, queue: nil", "unregisterForTraitChanges",
                         "notifications.removeObserver(observer)",
                         "self.attachmentEpoch == epoch", "self.generation == expected"]:
            self.assertIn(required, source)

    def test_startup_and_unknown_state_fail_closed_without_screenshot_receipts(self):
        source = code()
        self.assertIn("@State private var contentVisible = false", source)
        self.assertIn("render(.unavailable)", source)
        self.assertIn("@unknown default: capture = .unknown", source)
        self.assertNotIn("userDidTakeScreenshotNotification", source)
        self.assertNotIn("secureLayerAlwaysOn", (IOS / "HoplaKit/Sources/HoplaKit/ScreenCapturePolicy.swift").read_text())

    def test_app_installs_exactly_one_security_modifier_at_the_stable_root(self):
        sites = []
        for path in (IOS / "OpenMeshChat").rglob("*.swift"):
            if ".screenSecurity()" in path.read_text():
                sites.append(path.name)
        self.assertEqual(sites, ["OpenMeshChatApp.swift"])

    def test_overlapping_probes_share_lease_owned_controller_without_strong_window_registry(self):
        source = code()
        probe = source[source.index("final class Probe:"):]
        self.assertNotIn("ScenePrivacyController()", probe)
        self.assertIn("WindowPrivacyRegistry.shared.acquire", probe)
        self.assertIn("[weak self] visible", probe)
        self.assertIn("keyOptions: [.weakMemory, .objectPointerPersonality]", source)
        self.assertIn("valueOptions: .weakMemory", source)
        self.assertIn("listeners.removeValue(forKey: token) != nil", source)
        self.assertIn("if listeners.isEmpty { controller.detach() }", source)
        self.assertIn("Array(self.listeners.keys)", source)
        self.assertIn("self.listeners[token]?(self.controller.cover == .none)", source)

    def test_existing_observers_precede_initial_capture_sample(self):
        source = code()
        attach = source[source.index("func attach("):source.index("func detach(")]
        self.assertLess(attach.index("registerForTraitChanges"), attach.rindex("refreshCapture()"))
        self.assertLess(attach.index("render(.unavailable)"), attach.index("observe(UIScene"))


if __name__ == "__main__":
    unittest.main()
