"""Probe sequencing and failure decisions, without launching or changing a device."""

import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "android_readiness", Path(__file__).parents[1] / "ci" / "wait_for_android.py")
android = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(android)
READY = "1\npackage:/system/framework/framework-res.apk\nService input: found\nService window: found\n"


class AndroidReadinessTests(unittest.TestCase):
    def setUp(self):
        self.now = 0
        self.mock("time.monotonic", side_effect=lambda: self.now)
        self.sleep = self.mock("time.sleep", side_effect=self.advance)
        self.alive = self.mock("os.kill")
        self.run = self.mock("subprocess.run", return_value=self.result())
        self.mock("print")

    def mock(self, target, **kwargs):
        mock = patch.object(android, target, create=True, **kwargs) if "." not in target else patch.object(
            getattr(android, target.split(".")[0]), target.split(".")[1], **kwargs)
        value = mock.start()
        self.addCleanup(mock.stop)
        return value

    def advance(self, seconds):
        self.now += seconds

    def result(self, output=READY, code=0, error=""):
        return subprocess.CompletedProcess([], code, output, error)

    def test_requires_three_healthy_read_only_probes(self):
        android.wait_for_android("/sdk/adb", "emulator-5554", 123)
        self.assertEqual(self.run.call_count, 3)
        self.assertEqual(self.now, 4)
        self.alive.assert_called_with(123, 0)
        for call in self.run.call_args_list:
            self.assertEqual(call.args[0], ["/sdk/adb", "-s", "emulator-5554", "shell", android.PROBE])
            self.assertFalse(call.kwargs["check"])
            self.assertEqual(call.kwargs["timeout"], 10)

    def test_offline_transport_resets_stability_count(self):
        self.run.side_effect = [self.result(), self.result(),
                               self.result("", 255, "adb: device offline"),
                               self.result(), self.result(), self.result()]
        android.wait_for_android("adb", "emulator-5554", 123)
        self.assertEqual(self.run.call_count, 6)

    def test_boot_property_without_framework_services_is_not_ready(self):
        for output in ["1\n", READY.replace("package:/", "package:"),
                       READY.replace("Service input: found", "Service input: not found"),
                       READY.replace("Service window: found", "Service window: not found"),
                       READY.replace("1\n", "0\n", 1)]:
            with self.subTest(output=output):
                self.assertFalse(android.framework_ready(output))
        self.assertTrue(android.framework_ready(READY.replace("\n", "\r\n")))

    def test_probe_timeout_resets_stability_count(self):
        self.run.side_effect = [self.result(), subprocess.TimeoutExpired("adb", 10),
                               self.result(), self.result(), self.result()]
        android.wait_for_android("adb", "emulator-5554", 123)
        self.assertEqual(self.run.call_count, 5)

    def test_deadline_bounds_probes_and_reports_last_error(self):
        self.run.return_value = self.result("", 255, "adb: device offline")
        with self.assertRaisesRegex(RuntimeError, "within 3s.*device offline"):
            android.wait_for_android("adb", "emulator-5554", 123, timeout=3)
        self.assertEqual(self.now, 3)
        self.assertEqual([call.kwargs["timeout"] for call in self.run.call_args_list], [3, 1])

    def test_dead_emulator_fails_without_probing(self):
        self.alive.side_effect = ProcessLookupError()
        with self.assertRaisesRegex(RuntimeError, "Emulator exited"):
            android.wait_for_android("adb", "emulator-5554", 123)
        self.run.assert_not_called()

    def test_healthy_probe_finishing_at_deadline_does_not_pass(self):
        def probe(*args, **kwargs):
            if self.run.call_count == 3:
                self.advance(kwargs["timeout"])
            return self.result()

        self.run.side_effect = probe
        with self.assertRaisesRegex(RuntimeError, "within 5s"):
            android.wait_for_android("adb", "emulator-5554", 123, timeout=5)
        self.assertEqual(self.run.call_count, 3)
        self.assertEqual(self.now, 5)

    def test_invalid_pid_or_timeout_is_rejected(self):
        for pid, timeout in [(0, 240), (-1, 240), (123, 0)]:
            with self.subTest(pid=pid, timeout=timeout), self.assertRaises(ValueError):
                android.wait_for_android("adb", "emulator-5554", pid, timeout)
        self.run.assert_not_called()

    def test_missing_adb_is_not_retried(self):
        self.run.side_effect = FileNotFoundError()
        with self.assertRaises(FileNotFoundError):
            android.wait_for_android("adb", "emulator-5554", 123)
        self.assertEqual(self.run.call_count, 1)
