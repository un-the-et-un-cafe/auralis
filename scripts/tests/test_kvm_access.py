"""Exercise KVM permission decisions without touching devices or invoking sudo."""

import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "kvm_access", Path(__file__).parents[1] / "ci" / "kvm_access.py")
kvm = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(kvm)


class KvmAccessTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.entries = kvm.BASE_ACL
        self.info = SimpleNamespace(st_mode=stat.S_IFCHR | 0o660, st_uid=0,
                                    st_gid=109, st_dev=5, st_ino=123,
                                    st_rdev=os.makedev(10, 232))
        self.patch("os.environ", {"GITHUB_ACTIONS": "true",
                                  "RUNNER_ENVIRONMENT": "github-hosted",
                                  "RUNNER_OS": "Linux", "RUNNER_TEMP": self.temp.name})
        self.patch("platform.system", return_value="Linux")
        self.patch("platform.machine", return_value="x86_64")
        self.patch("os.getuid", return_value=1001)
        self.patch("os.geteuid", return_value=1001)
        # Only mock the device lstat; tempfile cleanup still needs real file stats.
        real_lstat = os.lstat
        self.patch("os.lstat", side_effect=lambda path, **kwargs:
                   self.info if str(path) == kvm.DEVICE else real_lstat(path, **kwargs))
        self.patch("grp.getgrgid", return_value=SimpleNamespace(gr_name="kvm"))
        self.access = self.patch("os.access", side_effect=lambda *args, **kwargs:
                                 self.entries == kvm.granted_acl(1001))
        self.read_acl = self.patch("subprocess.check_output", side_effect=self.get_acl)
        self.run = self.patch("subprocess.run", side_effect=self.mutate_acl)

    def patch(self, target, *args, **kwargs):
        obj, attribute = target.split(".")
        mock = patch.object(getattr(kvm, obj), attribute, *args, **kwargs)
        value = mock.start()
        self.addCleanup(mock.stop)
        return value

    def get_acl(self, command, **kwargs):
        self.assertEqual(command, ["getfacl", "--omit-header", "--numeric", "--", "/dev/kvm"])
        return self.entries + "\n\n"

    def mutate_acl(self, command, **kwargs):
        self.assertEqual(command[:4], ["sudo", "--non-interactive", "setfacl", "--no-mask"])
        self.assertEqual(command[-2:], ["--", "/dev/kvm"])
        self.assertTrue(kwargs["check"])
        operation, entries = command[4:6]
        if operation == "--modify":
            self.assertEqual(entries, "user:1001:rw-,mask::rw-")
            self.entries = kvm.granted_acl(1001)
        elif entries == "user:1001,mask::":
            self.assertEqual(operation, "--remove")
            self.entries = kvm.BASE_ACL
        else:
            self.assertEqual((operation, entries), ("--remove", "user:1001"))
            self.entries = "\n".join(line for line in self.entries.splitlines()
                                      if not line.startswith("user:1001:"))

    def test_grant_and_cleanup_only_current_uid(self):
        kvm.prepare()
        self.assertEqual(self.entries, kvm.granted_acl(1001))
        self.assertEqual(stat.S_IMODE(kvm.state_path().stat().st_mode), 0o600)
        kvm.cleanup()
        self.assertEqual(self.entries, kvm.BASE_ACL)
        self.assertFalse(kvm.state_path().exists())
        self.assertEqual(self.run.call_count, 2)

    def test_existing_access_does_not_modify_permissions(self):
        self.access.side_effect = None
        self.access.return_value = True
        kvm.prepare()
        self.run.assert_not_called()
        self.read_acl.assert_not_called()
        self.assertFalse(kvm.state_path().exists())

    def test_not_hosted_runner_is_refused(self):
        for key, value in [("GITHUB_ACTIONS", "false"),
                           ("RUNNER_ENVIRONMENT", "self-hosted"), ("RUNNER_OS", "macOS")]:
            with self.subTest(key=key), patch.dict(os.environ, {key: value}):
                with self.assertRaisesRegex(RuntimeError, "restricted"):
                    kvm.prepare()
        self.run.assert_not_called()

    def test_root_or_different_effective_uid_is_refused(self):
        for uid in [0, 1002]:
            with self.subTest(uid=uid), patch.object(kvm.os, "getuid", return_value=uid):
                with self.assertRaisesRegex(RuntimeError, "unprivileged"):
                    kvm.prepare()
        self.run.assert_not_called()

    def test_wrong_device_type_owner_or_number_is_refused(self):
        for field, value in [("st_mode", stat.S_IFLNK | 0o660),
                             ("st_mode", stat.S_IFREG | 0o660), ("st_uid", 1001),
                             ("st_rdev", os.makedev(1, 3))]:
            with self.subTest(field=field, value=value), patch.object(self.info, field, value):
                with self.assertRaisesRegex(RuntimeError, "character device"):
                    kvm.prepare()
        self.run.assert_not_called()

    def test_unexpected_mode_or_group_is_refused(self):
        with patch.object(self.info, "st_mode", stat.S_IFCHR | 0o600):
            with self.assertRaisesRegex(RuntimeError, "Unexpected KVM permissions"):
                kvm.prepare()
        with patch.object(kvm.grp, "getgrgid", return_value=SimpleNamespace(gr_name="other")):
            with self.assertRaisesRegex(RuntimeError, "Unexpected KVM permissions"):
                kvm.prepare()
        self.run.assert_not_called()

    def test_existing_named_acl_is_not_broadened(self):
        self.entries = "user::rw-\nuser:2222:r--\ngroup::rw-\nmask::rw-\nother::---"
        with self.assertRaisesRegex(RuntimeError, "Unexpected KVM permissions"):
            kvm.prepare()
        self.run.assert_not_called()

    def test_sudo_denial_is_not_retried(self):
        self.run.side_effect = subprocess.CalledProcessError(1, ["sudo"])
        with self.assertRaises(subprocess.CalledProcessError):
            kvm.prepare()
        self.assertTrue(kvm.state_path().exists())
        kvm.cleanup()  # Original ACL is unchanged, so no second sudo call.
        self.assertEqual(self.run.call_count, 1)

    def test_verification_failure_keeps_state_for_cleanup(self):
        self.access.side_effect = None
        self.access.return_value = False
        with self.assertRaisesRegex(RuntimeError, "verification failed"):
            kvm.prepare()
        kvm.cleanup()
        self.assertEqual(self.entries, kvm.BASE_ACL)

    def test_drift_cleanup_removes_our_uid_only(self):
        kvm.prepare()
        self.entries = self.entries.replace("group::rw-", "user:2222:r--\ngroup::rw-")
        with self.assertRaisesRegex(RuntimeError, "ACL drift"):
            kvm.cleanup()
        self.assertIn("user:2222:r--", self.entries)
        self.assertNotIn("user:1001:", self.entries)
        self.assertIn("mask::rw-", self.entries)

    def test_drift_without_our_uid_reports_actual_acl_without_mutation(self):
        kvm.prepare()
        self.entries = "user::rw-\ngroup::rw-\nmask::rw-\nother::---"
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            with self.assertRaisesRegex(RuntimeError, "UID entry was already absent"):
                kvm.cleanup()
        self.assertEqual(self.run.call_count, 1)
        self.assertIn("KVM cleanup observed mode: 0660", output.getvalue())
        self.assertIn("KVM cleanup named UID 1001 present: False", output.getvalue())
        self.assertIn(self.entries, output.getvalue())

    def test_drift_cleanup_verifies_uid_entry_was_removed(self):
        kvm.prepare()
        self.entries = self.entries.replace("group::rw-", "user:2222:r--\ngroup::rw-")
        self.run.side_effect = None  # A purported successful command must not hide a retained UID.
        with self.assertRaisesRegex(RuntimeError, "could not verify removal"):
            kvm.cleanup()

    def test_replaced_device_is_not_touched_during_cleanup(self):
        kvm.prepare()
        self.info.st_ino += 1
        with self.assertRaisesRegex(RuntimeError, "device changed"):
            kvm.cleanup()
        self.assertEqual(self.run.call_count, 1)

    def test_other_saved_uid_is_not_touched(self):
        kvm.prepare()
        saved = json.loads(kvm.state_path().read_text())
        saved["uid"] = 2222
        kvm.state_path().write_text(json.dumps(saved))
        with self.assertRaisesRegex(RuntimeError, "state or device changed"):
            kvm.cleanup()
        self.assertEqual(self.run.call_count, 1)

    def test_preexisting_state_is_not_overwritten(self):
        kvm.state_path().write_text("prior state")
        with self.assertRaises(FileExistsError):
            kvm.prepare()
        self.run.assert_not_called()

    def test_cleanup_without_grant_is_noop(self):
        kvm.cleanup()
        self.run.assert_not_called()
        self.read_acl.assert_not_called()


if __name__ == "__main__":
    unittest.main()
