#!/usr/bin/env python3
"""Grant only this disposable CI runner's UID access to its existing KVM device.

Authorized after run 40696797061 proved root:kvm 0660 denied the runner UID.
Never chmod the device, change group membership, or install a persistent rule.
"""

import grp
import json
import os
from pathlib import Path
import platform
import stat
import subprocess
import sys


DEVICE = "/dev/kvm"
BASE_ACL = "user::rw-\ngroup::rw-\nother::---"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def host_uid():
    require(os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
            and os.environ.get("RUNNER_OS") == "Linux"
            and platform.system() == "Linux" and platform.machine() == "x86_64",
            "KVM ACL management is restricted to disposable GitHub-hosted Linux x86_64 runners.")
    uid = os.getuid()
    require(uid != 0 and uid == os.geteuid(), "Expected an unprivileged runner UID.")
    return uid


def device_identity():
    info = os.lstat(DEVICE)  # Do not follow a replacement symlink.
    require(stat.S_ISCHR(info.st_mode) and info.st_uid == 0
            and os.major(info.st_rdev) == 10 and os.minor(info.st_rdev) == 232,
            "Expected the real root-owned Linux KVM character device (10:232).")
    return [info.st_dev, info.st_ino, info.st_rdev, info.st_uid, info.st_gid]


def acl():
    return subprocess.check_output(
        ["getfacl", "--omit-header", "--numeric", "--", DEVICE], text=True).strip()


def granted_acl(uid):
    return f"user::rw-\nuser:{uid}:rw-\ngroup::rw-\nmask::rw-\nother::---"


def set_acl(*arguments):
    # Explicit no-mask prevents expanding any pre-existing effective group rights.
    subprocess.run(["sudo", "--non-interactive", "setfacl", "--no-mask",
                    *arguments, "--", DEVICE], check=True)


def state_path():
    return Path(os.environ["RUNNER_TEMP"]) / "auralis-kvm-acl.json"


def prepare():
    uid = host_uid()
    identity = device_identity()
    if os.access(DEVICE, os.R_OK | os.W_OK, effective_ids=True):
        print("Runner already has KVM read/write access; no ACL changed.")
        return
    info = os.lstat(DEVICE)
    require(stat.S_IMODE(info.st_mode) == 0o660
            and grp.getgrgid(info.st_gid).gr_name == "kvm" and acl() == BASE_ACL,
            "Unexpected KVM permissions/ACL; refusing to broaden or overwrite them.")
    # Exclusive private state is written before mutation, so always() can revoke
    # an ACL even if verification or a later build step fails.
    fd = os.open(state_path(), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        json.dump({"uid": uid, "identity": identity}, stream)
    set_acl("--modify", f"user:{uid}:rw-,mask::rw-")
    require(device_identity() == identity and acl() == granted_acl(uid)
            and os.access(DEVICE, os.R_OK | os.W_OK, effective_ids=True),
            "KVM UID ACL verification failed; cleanup will revoke the grant.")
    print(f"Granted only runner UID {uid} KVM read/write; owner/group/other rights unchanged.")


def cleanup():
    uid = host_uid()
    path = state_path()
    if not path.exists():
        print("No job-created KVM ACL to revoke.")
        return
    with path.open(encoding="utf-8") as stream:
        saved = json.load(stream)
    require(saved.get("uid") == uid and saved.get("identity") == device_identity(),
            "KVM state or device changed; refusing to touch a different UID/device.")
    current = acl()
    # Device ACLs contain only host UID/permission metadata, never app secrets.
    # Preserve the actual drift evidence rather than guessing at its cause.
    print(f"KVM cleanup verified device identity (device, inode, rdev, owner, group): {saved['identity']}", flush=True)
    print(f"KVM cleanup observed mode: {stat.S_IMODE(os.lstat(DEVICE).st_mode):04o}", flush=True)
    print(f"KVM cleanup observed ACL:\n{current}", flush=True)
    uid_present = any(line.startswith(f"user:{uid}:") for line in current.splitlines())
    print(f"KVM cleanup named UID {uid} present: {uid_present}", flush=True)
    if current == granted_acl(uid):
        # No other named entries exist: remove just ours and the now-unneeded mask.
        set_acl("--remove", f"user:{uid},mask::")
        require(acl() == BASE_ACL, "KVM baseline ACL was not restored.")
    elif current != BASE_ACL:
        # Do not overwrite unexpected concurrent ACL changes. Revoke only our UID.
        if uid_present:
            set_acl("--remove", f"user:{uid}")
            remaining = acl()
            print(f"KVM ACL after removing this job's UID:\n{remaining}", flush=True)
            require(not any(line.startswith(f"user:{uid}:") for line in remaining.splitlines()),
                    "KVM cleanup could not verify removal of this job's UID grant.")
            raise RuntimeError("Unexpected KVM ACL drift; removed only this job's UID grant.")
        raise RuntimeError("Unexpected KVM ACL drift; this job's UID entry was already absent, so no ACL was changed.")
    path.unlink()
    print(f"KVM UID {uid} grant revoked; original ACL restored.")


if __name__ == "__main__":
    try:
        require(len(sys.argv) == 2 and sys.argv[1] in {"prepare", "cleanup"},
                "Usage: kvm_access.py prepare|cleanup")
        {"prepare": prepare, "cleanup": cleanup}[sys.argv[1]]()
    except (KeyError, OSError, RuntimeError, ValueError, subprocess.CalledProcessError) as error:
        print(f"KVM access: {error}", file=sys.stderr)
        sys.exit(1)
