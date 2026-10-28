#!/usr/bin/env python3
"""Check protocol-bearing files against an explicitly reviewed Git baseline.

Preserve the immutable upstream comparison, allowing only exact per-file hashes
with documented review rationale and regression evidence. This is a drift guard,
not an independent audit or interoperability certification. Never regenerate the
manifest automatically just to make CI pass.
"""

from __future__ import annotations

import argparse
import hashlib
import io
import json
import re
from pathlib import Path
import subprocess
import sys
import tarfile

DEFAULT_BASE = "233210d5b13f443798428d126266c520999ad387"
SCOPES = (
    "PROTOCOL.md",
    "extensions",
    "crates/omcp-core/src",
    "crates/omcp-ffi/src",
    "crates/omcp-sim/src",
)


def git(repo: Path, *args: str) -> bytes:
    result = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, check=False
    )
    if result.returncode:
        detail = result.stderr.decode("utf-8", errors="replace").strip()
        raise RuntimeError(detail or f"git exited with {result.returncode}")
    return result.stdout


def baseline_hashes(repo: Path, commit: str) -> dict[str, str]:
    # Read members in memory: never extract a Git-controlled archive to disk.
    archive = git(repo, "archive", "--format=tar", commit, "--", *SCOPES)
    hashes = {}
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as files:
        for member in files:
            if member.isdir():
                continue
            if not member.isfile():
                raise RuntimeError(f"unsupported baseline file type: {member.name}")
            stream = files.extractfile(member)
            if stream is None:
                raise RuntimeError(f"cannot read baseline file: {member.name}")
            hashes[member.name] = hashlib.sha256(stream.read()).hexdigest()
    return hashes


def workspace_hashes(repo: Path) -> dict[str, str]:
    hashes = {}
    for scope in SCOPES:
        root = repo / scope
        if root.is_symlink():
            raise RuntimeError(f"symlink in protected scope: {scope}")
        paths = root.rglob("*") if root.is_dir() else (root,)
        for path in paths:
            if path.is_symlink():
                raise RuntimeError(f"symlink in protected scope: {path.relative_to(repo)}")
            if path.is_file():
                hashes[path.relative_to(repo).as_posix()] = hashlib.sha256(
                    path.read_bytes()
                ).hexdigest()
    return hashes


def apply_reviewed_changes(expected: dict[str, str], policy: object) -> dict[str, str]:
    """Validate every exception against upstream; never accept globs or a new HEAD."""
    if not isinstance(policy, dict) or set(policy) != {"schema", "upstream_commit", "changes"}:
        raise RuntimeError("review manifest must contain schema, upstream_commit and changes")
    if type(policy["schema"]) is not int or policy["schema"] != 1:
        raise RuntimeError("unsupported review manifest schema")
    if policy["upstream_commit"] != DEFAULT_BASE:
        raise RuntimeError("review manifest must retain the immutable upstream commit")
    if not isinstance(policy["changes"], list):
        raise RuntimeError("review manifest changes must be a list")
    accepted = expected.copy()
    seen = set()
    required = {"path", "upstream_sha256", "accepted_sha256", "rationale", "evidence"}
    for entry in policy["changes"]:
        if not isinstance(entry, dict) or set(entry) != required:
            raise RuntimeError("each reviewed change needs path, upstream/accepted hashes, rationale and evidence")
        path = entry["path"]
        if not isinstance(path, str) or path not in expected or path in seen:
            raise RuntimeError(f"unknown or duplicate protected path: {path!r}")
        seen.add(path)
        for key in ("upstream_sha256", "accepted_sha256"):
            value = entry[key]
            if not isinstance(value, str) or not re.fullmatch(r"[0-9a-f]{64}", value):
                raise RuntimeError(f"invalid {key}: {path}")
        if entry["upstream_sha256"] != expected[path]:
            raise RuntimeError(f"incorrect upstream hash: {path}")
        if entry["accepted_sha256"] == expected[path]:
            raise RuntimeError(f"redundant unchanged-file exception: {path}")
        if not isinstance(entry["rationale"], str) or not entry["rationale"].strip():
            raise RuntimeError(f"missing review rationale: {path}")
        evidence = entry["evidence"]
        if not isinstance(evidence, list) or not evidence or any(
            not isinstance(item, str) or not item.strip() for item in evidence
        ):
            raise RuntimeError(f"missing regression evidence: {path}")
        accepted[path] = entry["accepted_sha256"]
    return accepted


def strict_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result = {}
    for key, value in pairs:
        if key in result:
            raise RuntimeError(f"duplicate manifest key: {key}")
        result[key] = value
    return result


def differences(expected: dict[str, str], actual: dict[str, str]) -> list[tuple[str, str]]:
    changed = []
    for name in sorted(expected.keys() | actual.keys()):
        if name not in expected:
            changed.append(("added", name))
        elif name not in actual:
            changed.append(("removed", name))
        elif expected[name] != actual[name]:
            changed.append(("modified", name))
    return changed


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo", type=Path, default=Path(__file__).resolve().parents[1],
        help="checkout to inspect (default: this script's repository)",
    )
    parser.add_argument(
        "--upstream-only", action="store_true",
        help="diagnostic: ignore reviewed exceptions and compare strictly with upstream",
    )
    args = parser.parse_args()
    repo = args.repo.resolve()
    try:
        commit = git(
            repo, "rev-parse", "--verify", "--end-of-options", f"{DEFAULT_BASE}^{{commit}}"
        ).decode("ascii").strip()
        expected = baseline_hashes(repo, commit)
        upstream = expected.copy()
        if not args.upstream_only:
            manifest = repo / "scripts/protocol-review-manifest.json"
            if manifest.is_symlink():
                raise RuntimeError("review manifest must not be a symlink")
            policy = json.loads(manifest.read_text(encoding="utf-8"), object_pairs_hook=strict_object)
            expected = apply_reviewed_changes(expected, policy)
        actual = workspace_hashes(repo)
    except (OSError, RuntimeError, tarfile.TarError, ValueError) as error:
        print(f"ERROR: protocol baseline could not be checked: {error}", file=sys.stderr)
        return 2

    changed = differences(expected, actual)
    if changed:
        print(f"FAIL: {len(changed)} protected files differ from the explicit accepted hashes:")
        for kind, name in changed:
            print(f"  {kind}: {name}")
        print("Review the source diff, compatibility and regression evidence; do not auto-update hashes.")
        return 1

    exceptions = len(differences(upstream, expected))
    print(f"PASS: {len(expected)} protected files; upstream {commit}; {exceptions} explicit reviewed exceptions.")
    print("This checks source invariants, not review approval or physical-device interoperability.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
