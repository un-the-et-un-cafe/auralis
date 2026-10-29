"""Mock-only checks: never installs an APK or touches a device."""
import hashlib
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

CI = Path(__file__).parents[1] / "ci"
sys.path.insert(0, str(CI))
SPEC = importlib.util.spec_from_file_location("capture_install", CI / "install-capture-apk.py")
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)
PACKAGE = "chat.auralis.app.uicapture"
PATH = "/data/app/~~synthetic/key==/base.apk"


def result(output="", code=0, error=""):
    return subprocess.CompletedProcess([], code, output, error)


class CaptureInstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.apk = Path(self.temp.name) / "capture.apk"
        self.apk.write_bytes(b"synthetic test APK bytes")
        self.digest = hashlib.sha256(self.apk.read_bytes()).hexdigest()
        self.now = 0
        self.verify = self.mock("verify_apk", return_value="1")
        self.ready = self.mock("wait_for_android")
        self.run = self.mock("subprocess.run")
        self.mock("os.kill")
        self.mock("time.monotonic", side_effect=lambda: self.now)
        self.mock("time.sleep", side_effect=lambda seconds: setattr(self, "now", self.now + seconds))
        self.mock("print")

    def mock(self, target, **kwargs):
        owner, name = (getattr(module, target.split(".")[0]), target.split(".")[1]) if "." in target else (module, target)
        item = patch.object(owner, name, **kwargs)
        value = item.start()
        self.addCleanup(item.stop)
        return value

    def exact(self):
        return [result("package:" + PATH), result(self.digest + "  " + PATH)]

    def call(self, **kwargs):
        args = dict(adb="/sdk/adb", aapt="/sdk/aapt", serial="emulator-5554", emulator_pid=123,
                    package=PACKAGE, apk=self.apk)
        args.update(kwargs)
        return module.install_capture(**args)

    def commands(self):
        commands = [call.args[0] for call in self.run.call_args_list]
        for command in commands:
            self.assertEqual(command[:3], ["/sdk/adb", "-s", "emulator-5554"])
        return [command[3:] for command in commands]

    def test_pm_absence_exception_does_not_hide_error_text(self):
        self.run.return_value = result(code=1, error="Permission denied")
        with self.assertRaisesRegex(RuntimeError, "no automatic retry"):
            self.call()
        self.assertFalse(any(c[0] == "install" for c in self.commands()))

    def test_empty_install_failure_is_not_package_absence(self):
        self.run.side_effect = [result(code=1), result(code=1), result(code=1)]
        with self.assertRaisesRegex(RuntimeError, "no automatic retry"):
            self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 1)

    def test_initial_exact_apk_is_not_reinstalled(self):
        self.run.side_effect = self.exact()
        self.call()
        self.assertFalse(any(c[0] == "install" for c in self.commands()))

    def test_absent_apk_install_success_requires_digest_match(self):
        self.run.side_effect = [result(code=1), result(code=1), result("Success"), *self.exact()]
        self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 1)

    def test_uncertain_install_already_completed_is_not_repeated(self):
        self.run.side_effect = [result(), result(), result(code=1, error="device offline"), result(), *self.exact()]
        self.call()
        commands = self.commands()
        self.assertEqual(sum(c[0] == "install" for c in commands), 1)
        self.assertEqual(commands.count(["reconnect"]), 1)
        self.assertEqual(self.ready.call_count, 2)

    def test_uncertain_absent_install_gets_only_one_retry(self):
        self.run.side_effect = [result(), result(), result(code=1, error="abb_exec: closed"), result(),
                               result(), result(), result(), result(), result("Success"), *self.exact()]
        self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 2)

    def test_install_failure_is_not_retried(self):
        self.run.side_effect = [result(), result(), result(code=1, error="INSTALL_FAILED_INVALID_APK")]
        with self.assertRaisesRegex(RuntimeError, "not a transport-retry"):
            self.call()
        self.assertNotIn(["reconnect"], self.commands())

    def test_second_transport_loss_stops(self):
        self.run.side_effect = [result(), result(), result(code=1, error="device offline"), result(),
                               result(), result(), result(), result(), result(code=1, error="closed")]
        with self.assertRaisesRegex(RuntimeError, "single serial-scoped reconnect"):
            self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 2)

    def test_installer_timeout_checks_identity_before_retry(self):
        self.run.side_effect = [result(), result(), subprocess.TimeoutExpired("adb", 90), result(), *self.exact()]
        self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 1)

    def test_unexpected_package_path_fails_without_install(self):
        self.run.return_value = result("package:/data/app/evil;command/base.apk")
        with self.assertRaisesRegex(RuntimeError, "Unexpected installed-package path"):
            self.call()
        self.assertFalse(any(c[0] == "install" for c in self.commands()))

    def test_unverified_installed_hash_fails_without_install(self):
        self.run.side_effect = [result("package:" + PATH), result("not a hash")]
        with self.assertRaisesRegex(RuntimeError, "Unable to establish"):
            self.call()
        self.assertFalse(any(c[0] == "install" for c in self.commands()))

    def test_success_without_exact_installed_apk_is_failure(self):
        self.run.side_effect = [result(), result(), result("Success"), result(), result()]
        with self.assertRaisesRegex(RuntimeError, "reported success but exact"):
            self.call()
        self.assertEqual(sum(c[0] == "install" for c in self.commands()), 1)

    def test_other_devices_and_production_packages_rejected_before_commands(self):
        for args in [dict(serial="emulator-5556"), dict(package="chat.auralis.app"),
                     dict(package="chat.auralis.app.debug"), dict(emulator_pid=0)]:
            with self.subTest(args=args), self.assertRaises(ValueError):
                self.call(**args)
        self.verify.assert_not_called()
        self.run.assert_not_called()

    def test_deadline_prevents_install(self):
        self.run.side_effect = [result(), result()]
        with self.assertRaisesRegex(RuntimeError, "deadline exhausted"):
            self.call(timeout=0.5)
        self.assertFalse(any(c[0] == "install" for c in self.commands()))

    def test_manifest_verifier_rejects_wrong_package(self):
        # Exercise the original function independently of the install function's patched dependency.
        source = importlib.util.module_from_spec(SPEC)
        SPEC.loader.exec_module(source)
        with patch.object(source.subprocess, "run", return_value=result("package: name='chat.auralis.app' versionCode='1'")):
            with self.assertRaisesRegex(RuntimeError, "does not match"):
                source.verify_apk("aapt", self.apk, PACKAGE)

    def test_manifest_verifier_returns_exact_version(self):
        source = importlib.util.module_from_spec(SPEC)
        SPEC.loader.exec_module(source)
        with patch.object(source.subprocess, "run", return_value=result(f"package: name='{PACKAGE}' versionCode='7'")):
            self.assertEqual(source.verify_apk("aapt", self.apk, PACKAGE), "7")


if __name__ == "__main__":
    unittest.main()
