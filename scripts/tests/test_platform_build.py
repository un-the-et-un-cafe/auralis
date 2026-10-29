"""Build-script isolation regressions; no native compiler is executed."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class AndroidLinkFlagsTests(unittest.TestCase):
    def run_build(self, extra):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "apps/android/scripts"
            scripts.mkdir(parents=True)
            script = scripts / "build-rust.sh"
            shutil.copyfile(ROOT / "apps/android/scripts/build-rust.sh", script)
            bindir = root / "bin"
            bindir.mkdir()
            cargo = bindir / "cargo"
            cargo.write_text('#!/bin/bash\nprintf "cargo:%s\\narm:%s\\nx86:%s\\n" "$*" "$CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS" "$CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS"\n')
            cargo.chmod(0o755)
            env = {k: v for k, v in os.environ.items() if k not in (
                "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS",
                "CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS",
                "CARGO_TARGET_X86_64_LINUX_ANDROID_RUSTFLAGS")}
            env.update(extra)
            env["PATH"] = f"{bindir}:{env['PATH']}"
            return subprocess.run(["bash", str(script)], env=env,
                                  capture_output=True, text=True, check=False)

    def test_present_even_empty_globals_cannot_override_alignment(self):
        for variable in ("RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"):
            for value in ("", "-C opt-level=1"):
                with self.subTest(variable=variable, value=value):
                    result = self.run_build({variable: value})
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Unset global", result.stderr)
                    self.assertNotIn("cargo:", result.stdout)

    def test_target_specific_flags_preserved_and_both_alignments_added(self):
        result = self.run_build({"CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS": "-C opt-level=2"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("arm:-C opt-level=2 -C link-arg=-Wl,-z,max-page-size=16384", result.stdout)
        self.assertIn("x86: -C link-arg=-Wl,-z,max-page-size=16384", result.stdout)
        self.assertIn("-C link-arg=-Wl,-z,common-page-size=16384", result.stdout)
        self.assertIn("cargo:build --locked -p omcp-ffi", result.stdout)


if __name__ == "__main__":
    unittest.main()
