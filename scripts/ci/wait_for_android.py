#!/usr/bin/env python3
"""Bounded, read-only transport/framework readiness for a fresh test emulator.

sys.boot_completed alone is insufficient: ADB can briefly go offline while the
emulator finishes post-boot configuration. Never retry instrumentation or mutate
the device here; require consecutive healthy framework probes before setup.
"""

import argparse
import os
import subprocess
import sys
import time


PROBE = ("getprop sys.boot_completed; cmd package path android; "
         "service check input; service check window")


def framework_ready(output):
    lines = output.strip().splitlines()
    return (len(lines) == 4 and lines[0] == "1"
            and lines[1].startswith("package:/") and lines[1].endswith(".apk")
            and lines[2:] == ["Service input: found", "Service window: found"])


def wait_for_android(adb, serial, emulator_pid, timeout=240):
    if emulator_pid <= 0 or timeout <= 0:
        raise ValueError("Expected a positive emulator PID and timeout.")
    start = time.monotonic()
    deadline = start + timeout
    consecutive = 0
    last = "No probe completed"
    while time.monotonic() < deadline:
        try:
            os.kill(emulator_pid, 0)
        except ProcessLookupError as error:
            raise RuntimeError("Emulator exited before Android became ready.") from error
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        try:
            result = subprocess.run([adb, "-s", serial, "shell", PROBE],
                                    capture_output=True, text=True,
                                    timeout=min(10, remaining), check=False)
            ready = result.returncode == 0 and framework_ready(result.stdout)
            last = f"exit={result.returncode}, stdout={result.stdout[:400]!r}, stderr={result.stderr[:400]!r}"
        except subprocess.TimeoutExpired:
            ready = False
            last = "ADB framework probe timed out"
        consecutive = consecutive + 1 if ready else 0
        print(f"Android readiness: {consecutive}/3 consecutive healthy probes; {last}", flush=True)
        if consecutive == 3 and time.monotonic() < deadline:
            print(f"Android transport/framework ready after {time.monotonic() - start:.1f}s", flush=True)
            return
        time.sleep(min(2, max(0, deadline - time.monotonic())))
    raise RuntimeError(f"Android did not become stably ready within {timeout}s; {last}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("adb")
    parser.add_argument("serial")
    parser.add_argument("emulator_pid", type=int)
    args = parser.parse_args()
    try:
        wait_for_android(args.adb, args.serial, args.emulator_pid)
    except (OSError, RuntimeError, ValueError) as error:
        print(f"Android readiness: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
