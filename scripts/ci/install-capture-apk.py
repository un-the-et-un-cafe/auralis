#!/usr/bin/env python3
"""Bounded install recovery for two isolated capture APKs on our owned emulator only."""
import argparse
import hashlib
import os
from pathlib import Path
import re
import subprocess
import sys
import time

from wait_for_android import wait_for_android

PACKAGES = {"chat.auralis.app.uicapture", "chat.auralis.app.uicapture.test"}
SERIAL = "emulator-5554"
TRANSPORT = ("device offline", "device not found", "no devices", "closed", "connection reset", "protocol fault")


class TransportError(RuntimeError):
    pass


def verify_apk(aapt, apk, package):
    result = subprocess.run([aapt, "dump", "badging", str(apk)], capture_output=True, text=True, timeout=15, check=False)
    match = re.search(r"^package: name='([^']+)' versionCode='([0-9]+)'", result.stdout, re.MULTILINE)
    if result.returncode or not match or match.group(1) != package:
        raise RuntimeError("APK manifest does not match the dedicated capture package; refusing install.")
    return match.group(2)


def install_capture(adb, aapt, serial, emulator_pid, package, apk, timeout=180):
    if serial != SERIAL or package not in PACKAGES or emulator_pid <= 0 or timeout <= 0:
        raise ValueError("Only the owned emulator-5554 and dedicated capture packages are allowed.")
    apk = Path(apk)
    version = verify_apk(aapt, apk, package)
    expected = hashlib.sha256(apk.read_bytes()).hexdigest()
    deadline = time.monotonic() + timeout
    reconnected = False

    def remaining():
        seconds = deadline - time.monotonic()
        if seconds <= 0:
            raise RuntimeError("Capture APK install deadline exhausted.")
        os.kill(emulator_pid, 0)
        return seconds

    def run(args, limit=10):
        try:
            result = subprocess.run([adb, "-s", serial, *args], capture_output=True, text=True,
                                    timeout=min(limit, remaining()), check=False)
        except subprocess.TimeoutExpired as error:
            raise TransportError("ADB operation timed out; install outcome may be uncertain.") from error
        # AOSP pm path returns status 1 with no output when this exact package is absent.
        # Do not generalize that exception to installs or any other command/error output.
        if (args == ["shell", "pm", "path", package] and result.returncode == 1
                and not result.stdout.strip() and not result.stderr.strip()):
            return ""
        combined = (result.stdout + result.stderr).lower()
        if result.returncode != 0:
            if "install_failed" in combined:
                raise RuntimeError("Package installation failed; not a transport-retry condition: " + combined[:300])
            if any(marker in combined for marker in TRANSPORT):
                raise TransportError("ADB transport unavailable: " + combined[:200])
            raise RuntimeError("ADB command failed; no automatic retry: " + combined[:300])
        return result.stdout.strip()

    def recover():
        nonlocal reconnected
        if reconnected:
            raise RuntimeError("The single serial-scoped reconnect was already used; refusing another recovery.")
        reconnected = True
        print(f"Recovering transport for {serial} only; no global ADB reset.", flush=True)
        try:
            run(["reconnect"])
        except TransportError:
            # An offline connection can close while the requested reconnect takes effect.
            pass
        wait_for_android(adb, serial, emulator_pid, timeout=min(45, remaining()))

    def state():
        output = run(["shell", "pm", "path", package])
        if not output:
            return "absent"
        lines = output.splitlines()
        if len(lines) != 1 or not re.fullmatch(r"package:/data/app/[A-Za-z0-9_./=+~\-]+/base\.apk", lines[0]):
            raise RuntimeError("Unexpected installed-package path; refusing to guess or reinstall.")
        installed = lines[0].removeprefix("package:")
        digest = run(["shell", "sha256sum", installed]).split()
        if len(digest) != 2 or digest[1] != installed or not re.fullmatch(r"[0-9a-f]{64}", digest[0]):
            raise RuntimeError("Unable to establish installed APK identity; refusing uncertain reinstall.")
        return "exact" if digest[0] == expected else "different"

    def observed_state():
        try:
            first = state()
        except TransportError:
            recover()
            first = state()
        if first == "exact":
            return first
        # Two consistent PM observations after transport health are required before
        # replacing an absent/different dedicated APK after an uncertain attempt.
        time.sleep(min(1, remaining()))
        second = state()
        if second != first:
            if second == "exact":
                return second
            raise RuntimeError("Installed state changed while checking; refusing uncertain reinstall.")
        return second

    wait_for_android(adb, serial, emulator_pid, timeout=min(45, remaining()))
    for attempt in range(1, 3):
        if observed_state() == "exact":
            print(f"Verified {package} versionCode {version} by exact APK SHA-256; no repeated install.", flush=True)
            return
        print(f"Installing only {package}; bounded attempt {attempt}/2.", flush=True)
        try:
            result = run(["install", "-r", str(apk)], limit=90)
            if "Success" not in result.splitlines():
                raise RuntimeError("Installer did not report Success; refusing blind retry.")
        except TransportError:
            recover()
            if observed_state() == "exact":
                print(f"Uncertain install resolved: exact {package} versionCode {version} is installed.", flush=True)
                return
            if attempt == 2:
                raise RuntimeError("Second install did not establish exact APK identity.")
            continue
        if observed_state() != "exact":
            raise RuntimeError("Installer reported success but exact APK identity was not established.")
        print(f"Verified installed {package} versionCode {version} by exact APK SHA-256.", flush=True)
        return
    raise RuntimeError("Capture APK installation did not complete.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("adb")
    parser.add_argument("aapt")
    parser.add_argument("serial")
    parser.add_argument("emulator_pid", type=int)
    parser.add_argument("package")
    parser.add_argument("apk")
    args = parser.parse_args()
    if os.environ.get("GITHUB_ACTIONS") != "true" or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted":
        print("Capture install recovery is restricted to disposable GitHub-hosted runners.", file=sys.stderr)
        return 1
    try:
        install_capture(**vars(args))
    except (OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f"Capture APK installation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
