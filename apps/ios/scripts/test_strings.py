"""Regression checks for the EN/FR source-of-truth generator; no Apple SDK required."""

import contextlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("auralis_strings", Path(__file__).with_name("strings.py"))
assert SPEC is not None and SPEC.loader is not None
strings = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(strings)


class LocalizationTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.override = patch.object(strings, "IOS", str(self.root))
        self.override.start()
        self.addCleanup(self.override.stop)
        strings.write(self.root / "OpenMeshChat/Resources", list(strings.S))
        strings.write(self.root / "NotificationService", strings.NSE)

    def check_output(self):
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            strings.check()
        return output.getvalue()

    def test_generated_app_and_extension_resources_match_source(self):
        self.assertIn("strings OK:", self.check_output())

    def test_check_rejects_stale_app_resource(self):
        filename = self.root / "OpenMeshChat/Resources/fr.lproj/Localizable.strings"
        filename.write_text(filename.read_text().replace('"app_name" = "Auralis";', '"app_name" = "Old name";'))
        with self.assertRaises(SystemExit), contextlib.redirect_stdout(io.StringIO()):
            strings.check()

    def test_check_rejects_missing_extension_resource(self):
        (self.root / "NotificationService/en.lproj/Localizable.strings").unlink()
        with self.assertRaises(SystemExit), contextlib.redirect_stdout(io.StringIO()):
            strings.check()

    def test_check_rejects_mismatched_placeholder_types(self):
        output = io.StringIO()
        with patch.dict(strings.S, {"auralis_gateway_usage": ("%ld MB", "%@ Mo")}):
            with self.assertRaises(SystemExit), contextlib.redirect_stdout(output):
                strings.check()
        self.assertIn("auralis_gateway_usage (format args)", output.getvalue())

    def test_sent_copy_distinguishes_os_acceptance_from_peer_receipt(self):
        english, french = strings.S["msg_sent"]
        self.assertIn("Bluetooth", english)
        self.assertIn("Receipt unconfirmed", english)
        self.assertIn("Bluetooth", french)
        self.assertIn("Réception non confirmée", french)
        self.assertTrue(all("Bluetooth" in value for value in strings.S["file_state_sent"]))

    def test_headlines_encode_line_breaks_not_literal_backslash_n(self):
        self.assertIn("\n", strings.S["auralis_welcome_title"][0])
        self.assertEqual(strings.esc('first\n"second"'), 'first\\n\\"second\\"')


if __name__ == "__main__":
    unittest.main()
