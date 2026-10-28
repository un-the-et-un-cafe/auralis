"""The source guard must fail closed when source or review metadata drifts."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "guard", Path(__file__).resolve().parents[1] / "check_protocol_invariants.py"
)
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)


class ReviewedManifestTests(unittest.TestCase):
    def setUp(self):
        self.path = "crates/omcp-core/src/ble/fragment.rs"
        self.original = "a" * 64
        self.accepted = "b" * 64
        self.expected = {self.path: self.original, "PROTOCOL.md": "c" * 64}
        self.policy = {
            "schema": 1,
            "upstream_commit": guard.DEFAULT_BASE,
            "changes": [{
                "path": self.path,
                "upstream_sha256": self.original,
                "accepted_sha256": self.accepted,
                "rationale": "Bound untrusted fragmented input without changing encoding.",
                "evidence": ["test_reassembly_admission_limit"],
            }],
        }

    def test_exact_reviewed_hash_only(self):
        accepted = guard.apply_reviewed_changes(self.expected, self.policy)
        self.assertEqual(accepted[self.path], self.accepted)
        self.assertEqual(accepted["PROTOCOL.md"], self.expected["PROTOCOL.md"])
        self.assertEqual(self.expected[self.path], self.original)
        self.assertEqual(guard.differences(accepted, accepted), [])
        self.assertEqual(guard.differences(accepted, self.expected), [("modified", self.path)])

    def test_no_exceptions_retains_upstream(self):
        self.policy["changes"] = []
        self.assertEqual(guard.apply_reviewed_changes(self.expected, self.policy), self.expected)

    def test_invalid_manifests_fail_closed(self):
        cases = []
        for key, value in [("schema", True), ("schema", 2), ("upstream_commit", "HEAD"), ("changes", {})]:
            policy = copy.deepcopy(self.policy)
            policy[key] = value
            cases.append(policy)
        for key, value in [
            ("path", "../outside"), ("path", "crates/*"),
            ("upstream_sha256", "d" * 64), ("accepted_sha256", "HEAD"),
            ("accepted_sha256", self.original), ("rationale", " "),
            ("evidence", []), ("evidence", [""]), ("evidence", "not-a-list"),
        ]:
            policy = copy.deepcopy(self.policy)
            policy["changes"][0][key] = value
            cases.append(policy)
        duplicate = copy.deepcopy(self.policy)
        duplicate["changes"].append(copy.deepcopy(duplicate["changes"][0]))
        cases.append(duplicate)
        missing = copy.deepcopy(self.policy)
        del missing["changes"][0]["rationale"]
        cases.append(missing)
        extra = copy.deepcopy(self.policy)
        extra["skip"] = True
        cases.append(extra)
        for policy in cases:
            with self.subTest(policy=policy), self.assertRaises(RuntimeError):
                guard.apply_reviewed_changes(self.expected, policy)

    def test_duplicate_json_keys_rejected(self):
        with self.assertRaises(RuntimeError):
            json.loads('{"schema":1,"schema":2}', object_pairs_hook=guard.strict_object)

    def test_unreviewed_modified_added_removed_files_detected(self):
        actual = {self.path: "d" * 64, "extensions/new.md": "e" * 64}
        self.assertEqual(guard.differences(self.expected, actual), [
            ("removed", "PROTOCOL.md"), ("modified", self.path),
            ("added", "extensions/new.md"),
        ])

    def test_unavailable_baseline_fails_with_error_status(self):
        with tempfile.TemporaryDirectory() as directory:
            with patch("sys.argv", ["guard", "--repo", directory]), contextlib.redirect_stderr(io.StringIO()) as errors:
                self.assertEqual(guard.main(), 2)
            self.assertIn("protocol baseline could not be checked", errors.getvalue())

    def test_baseline_archive_rejects_symlink(self):
        archive = io.BytesIO()
        with tarfile.open(fileobj=archive, mode="w") as output:
            member = tarfile.TarInfo("PROTOCOL.md")
            member.type = tarfile.SYMTYPE
            member.linkname = "outside"
            output.addfile(member)
        with patch.object(guard, "git", return_value=archive.getvalue()), self.assertRaises(RuntimeError):
            guard.baseline_hashes(Path("."), guard.DEFAULT_BASE)

    def test_workspace_symlink_in_protected_scope_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "PROTOCOL.md").symlink_to("elsewhere")
            with self.assertRaises(RuntimeError):
                guard.workspace_hashes(repo)

    def test_nested_workspace_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            root = repo / "crates/omcp-core/src"
            root.mkdir(parents=True)
            (root / "link").symlink_to("elsewhere", target_is_directory=True)
            with self.assertRaises(RuntimeError):
                guard.workspace_hashes(repo)


if __name__ == "__main__":
    unittest.main()
