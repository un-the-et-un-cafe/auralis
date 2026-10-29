"""The source guard must fail closed when source or review metadata drifts."""
import contextlib
import copy
import importlib.util
import io
import json
import hashlib
import os
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


class ReviewedAdditionTests(ReviewedManifestTests):
    def setUp(self):
        super().setUp()
        self.policy["schema"] = 2
        self.new_path = "crates/omcp-core/src/receipt.rs"
        self.addition = {
            "path": self.new_path, "accepted_sha256": "d" * 64,
            "rationale": "Explicitly reviewed new module.",
            "evidence": ["receipt_regression_test"],
        }
        self.policy["additions"] = [self.addition]

    def test_exact_reviewed_hash_only(self):
        accepted = guard.apply_reviewed_changes(self.expected, self.policy)
        self.assertEqual(accepted[self.path], self.accepted)
        self.assertEqual(accepted[self.new_path], "d" * 64)
        self.assertNotIn(self.new_path, self.expected)
        self.assertEqual(guard.differences(accepted, accepted), [])
        for actual, kind in [
            ({k: v for k, v in accepted.items() if k != self.new_path}, "removed"),
            ({**accepted, self.new_path: "e" * 64}, "modified"),
            ({**accepted, "extensions/unlisted.md": "f" * 64}, "added"),
        ]:
            with self.subTest(kind=kind):
                self.assertEqual(guard.differences(accepted, actual)[0][0], kind)

    def test_no_exceptions_retains_upstream(self):
        self.policy["changes"] = []
        self.policy["additions"] = []
        self.assertEqual(guard.apply_reviewed_changes(self.expected, self.policy), self.expected)

    def test_invalid_manifests_fail_closed(self):
        # Parent schema-1 tests remain exercised separately.
        for field in self.addition:
            policy = copy.deepcopy(self.policy)
            del policy["additions"][0][field]
            with self.subTest(missing=field), self.assertRaises(RuntimeError):
                guard.apply_reviewed_changes(self.expected, policy)
        for field, value in [
            ("rationale", " "), ("rationale", None), ("evidence", []),
            ("evidence", [""]), ("evidence", "test"), ("evidence", [1]),
            ("accepted_sha256", "A" * 64), ("accepted_sha256", "d" * 63),
            ("accepted_sha256", None), ("path", self.path),
            ("path", "PROTOCOL.md"),
        ]:
            policy = copy.deepcopy(self.policy)
            policy["additions"][0][field] = value
            with self.subTest(field=field, value=value), self.assertRaises(RuntimeError):
                guard.apply_reviewed_changes(self.expected, policy)
        policy = copy.deepcopy(self.policy)
        policy["additions"].append(copy.deepcopy(self.addition))
        with self.assertRaises(RuntimeError):
            guard.apply_reviewed_changes(self.expected, policy)

    def test_adversarial_paths_rejected(self):
        for path in [
            None, 1, "", "/extensions/new.md", "../extensions/new.md",
            "extensions/../outside", "extensions/./new", "extensions//new",
            "extensions/new/", "extensions", "extensions-other/new",
            "PROTOCOL.md/new", "crates/omcp-core/src-other/new",
            "crates/omcp-core/tests/new", "extensions/*.rs", "extensions/a?",
            "extensions/[a]", "extensions/a\\b", "extensions/C:foo",
            "extensions/line\nname", "extensions/null\x00name",
        ]:
            self.addition["path"] = path
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                guard.apply_reviewed_changes(self.expected, self.policy)

    def test_all_existing_directory_scopes_supported(self):
        for scope in guard.SCOPES[1:]:
            self.addition["path"] = scope + "/new.rs"
            self.assertIn(self.addition["path"], guard.apply_reviewed_changes(self.expected, self.policy))

    def test_schema_boundaries(self):
        for value in (None, {}, "all"):
            self.policy["additions"] = value
            with self.assertRaises(RuntimeError):
                guard.apply_reviewed_changes(self.expected, self.policy)
        self.policy["additions"] = []
        self.policy["schema"] = 1
        with self.assertRaises(RuntimeError):
            guard.apply_reviewed_changes(self.expected, self.policy)
        self.policy["schema"] = 2
        del self.policy["additions"]
        with self.assertRaises(RuntimeError):
            guard.apply_reviewed_changes(self.expected, self.policy)

    def test_scope_ancestor_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            target = repo / "elsewhere"
            (target / "omcp-core/src").mkdir(parents=True)
            (target / "omcp-core/src/receipt.rs").write_text("data")
            (repo / "crates").symlink_to(target, target_is_directory=True)
            with self.assertRaises(RuntimeError):
                guard.workspace_hashes(repo)


class AdditionWorkspaceIntegrationTests(unittest.TestCase):
    def test_real_files_exact_addition_drift_removal_and_unlisted(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "scripts").mkdir()
            (repo / "extensions").mkdir()
            baseline = {"PROTOCOL.md": hashlib.sha256(b"baseline").hexdigest()}
            (repo / "PROTOCOL.md").write_bytes(b"baseline")
            added = repo / "extensions/reviewed.md"
            added.write_bytes(b"reviewed")
            policy = {
                "schema": 2, "upstream_commit": guard.DEFAULT_BASE, "changes": [],
                "additions": [{"path": "extensions/reviewed.md",
                    "accepted_sha256": hashlib.sha256(b"reviewed").hexdigest(),
                    "rationale": "Reviewed test fixture", "evidence": ["integration"]}],
            }
            (repo / "scripts/protocol-review-manifest.json").write_text(json.dumps(policy))
            def run():
                with patch("sys.argv", ["guard", "--repo", directory]), \
                     patch.object(guard, "git", return_value=guard.DEFAULT_BASE.encode()), \
                     patch.object(guard, "baseline_hashes", return_value=baseline), \
                     contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                    return guard.main()
            self.assertEqual(run(), 0)
            added.write_bytes(b"drift")
            self.assertEqual(run(), 1)
            added.unlink()
            self.assertEqual(run(), 1)
            added.write_bytes(b"reviewed")
            unlisted = repo / "extensions/unlisted.md"
            unlisted.write_bytes(b"other")
            self.assertEqual(run(), 1)
            unlisted.unlink()
            added.unlink()
            added.symlink_to(repo / "PROTOCOL.md")
            self.assertEqual(run(), 2)

    def test_special_file_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            repo = Path(directory)
            (repo / "extensions").mkdir()
            os.mkfifo(repo / "extensions/unlisted.pipe")
            with self.assertRaises(RuntimeError):
                guard.workspace_hashes(repo)


if __name__ == "__main__":
    unittest.main()
