"""Bounded launcher cleanup, with real owned child processes and fake adb."""
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

HELPER = Path(__file__).resolve().parents[1] / "ci/android-emulator-cleanup.sh"


class AndroidCleanupTest(unittest.TestCase):
    def run_cleanup(self, adb_body="exit 0", child="sleep 60", original=0):
        with tempfile.TemporaryDirectory() as directory:
            adb = Path(directory) / "adb"
            adb.write_text("#!/bin/bash\n" + adb_body + "\n")
            adb.chmod(0o700)
            script = r'''
set -eu
source "$1"
cleanup() {
  local result=$?
  trap - EXIT
  if ! auralis_cleanup_android "$2" emulator-5554 "$PID" "" 0.1 5 0.02; then
    echo "cleanup failure; original=$result" >&2
  fi
  exit "$result"
}
# Keep original arguments available inside the trap function.
HELPER=$1; ADB=$2
trap 'cleanup "$HELPER" "$ADB"' EXIT
bash -c "$3" &
PID=$!
# Allow the mock child to install its signal handler before cleanup.
sleep 0.1
exit "$4"
'''
            started = time.monotonic()
            result = subprocess.run(["bash", "-c", script, "test", str(HELPER), str(adb), child, str(original)],
                                    text=True, capture_output=True, timeout=5)
            self.assertLess(time.monotonic() - started, 4)
            return result

    def test_normal_exit_preserves_success(self):
        result = self.run_cleanup()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("cleanup failure", result.stderr)

    def test_hung_adb_is_bounded_and_reported(self):
        result = self.run_cleanup(adb_body="exec sleep 60")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("console shutdown failed", result.stderr)
        self.assertIn("cleanup failure; original=0", result.stderr)

    def test_hung_emulator_is_killed_and_reported(self):
        result = self.run_cleanup(child="trap '' TERM; while :; do sleep 0.02; done")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("escalating owned child", result.stderr)
        self.assertIn("cleanup failure", result.stderr)

    def test_original_failure_is_not_masked(self):
        result = self.run_cleanup(adb_body="exec sleep 60", original=17)
        self.assertEqual(result.returncode, 17, result.stderr)
        self.assertIn("original=17", result.stderr)

    def test_unowned_pid_is_not_signalled(self):
        with subprocess.Popen(["sleep", "60"]) as unrelated:
            try:
                result = subprocess.run(["bash", "-c", 'source "$1"; auralis_stop_child "$2" unrelated 2 0.01',
                                         "test", str(HELPER), str(unrelated.pid)], capture_output=True, timeout=3)
                self.assertEqual(result.returncode, 0)
                self.assertIsNone(unrelated.poll())
            finally:
                unrelated.terminate()
                unrelated.wait(timeout=3)


if __name__ == "__main__":
    unittest.main()
