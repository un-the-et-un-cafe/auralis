#!/usr/bin/env python3
"""Check protocol-bearing files against an explicitly reviewed Git baseline.

This guards a UI-only fork from accidental wire/crypto drift. It is not a
permanent ban on protocol evolution or an interoperability certification: after
an intentional reviewed upgrade, supply its commit with --base.
"""

from __future__ import annotations

import argparse
import hashlib
import io
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


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--repo", type=Path, default=Path(__file__).resolve().parents[1],
        help="checkout to inspect (default: this script's repository)",
    )
    parser.add_argument(
        "--base", default=DEFAULT_BASE,
        help="reviewed baseline commit or ref (default: audited Hopla upstream)",
    )
    args = parser.parse_args()
    repo = args.repo.resolve()
    try:
        commit = git(
            repo, "rev-parse", "--verify", "--end-of-options", f"{args.base}^{{commit}}"
        ).decode("ascii").strip()
        expected = baseline_hashes(repo, commit)
        actual = workspace_hashes(repo)
    except (OSError, RuntimeError, tarfile.TarError) as error:
        print(f"ERROR: protocol baseline could not be checked: {error}", file=sys.stderr)
        return 2

    changed = []
    for name in sorted(expected.keys() | actual.keys()):
        if name not in expected:
            changed.append(("added", name))
        elif name not in actual:
            changed.append(("removed", name))
        elif expected[name] != actual[name]:
            changed.append(("modified", name))
    if changed:
        print(f"FAIL: {len(changed)} protocol-bearing files differ from {commit}:")
        for kind, name in changed:
            print(f"  {kind}: {name}")
        print("Review intentional protocol changes, then select an audited --base.")
        return 1

    print(f"PASS: {len(expected)} protocol-bearing files match {commit} byte for byte.")
    print("This checks source invariants, not physical-device interoperability.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
