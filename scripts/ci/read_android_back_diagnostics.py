#!/usr/bin/env python3
"""Offline scalar-only readout of ONE test's threadtime logcat; never a CI verdict.

No downloads, XML, archives, shell commands or Android actions. Unknown telemetry
is rejected rather than echoed. Input/line/event bounds also apply to ignored text.
"""
import argparse
import json
import os
import re
import stat
import sys

MAX_INPUT_BYTES = 8 * 1024 * 1024
MAX_LINE_BYTES = 16 * 1024
MAX_LINES = 100_000
MAX_EVENTS = 28
MAX_SNAPSHOTS = 8
MAX_NODES = MAX_VIEWS = 256
READ_BUDGET_MS = 50
DRAIN_MS = 250
LONG_MAX = 2**63 - 1
TAG = b"OnboardingBackDiag"
THREADTIME = re.compile(
    rb"^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+(\d+)\s+\d+\s+[VDIWEFS]\s+([^:]+):"
)
PREFIX = re.compile(
    r"^\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}\s+\d+\s+\d+ I "
    r"OnboardingBackDiag\s*:\s*(.*)$"
)
PHASES = frozenset(("BEFORE_HIDE", "IME_READY", "BEFORE_SYSTEM_BACK", "AFTER_SYSTEM_BACK",
                    "BACK_OR_READINESS_FAILED", "DESTINATION_WAIT_FAILED", "DESTINATION_ASSERTIONS_PASSED"))
LIFECYCLES = frozenset(("DESTROYED", "INITIALIZED", "CREATED", "STARTED", "RESUMED"))
REQUEST_KEYS = frozenset(("request", "phase", "requestedMs"))
STATE_KEYS = frozenset(("snapshot", "phase", "requestedMs", "observedMs", "state"))
EXPIRED_KEYS = frozenset(("snapshot", "phase", "state"))
BACK_KEYS = frozenset(("backReturnedMs", "result", "durationMs"))
DRAIN_KEYS = frozenset(("drainComplete", "drainLimitMs"))
ERROR_KEYS = frozenset(("diagnosticUnavailable",))
SNAPSHOT_KEYS = frozenset((
    "snapshot", "phase", "requestedMs", "observedMs", "readMs", "activity", "decor", "lifecycle",
    "attached", "windowFocused", "viewFocused", "imeVisible", "observerReady", "animationActive",
    "generation", "probe", "secure", "scope", "complete", "roots", "pendingLayoutRoots", "views",
    "nodes", "welcomeCount", "profileCount", "focusedCount", "fixtureMatches", "identityNull",
))
ALL_KEYS = REQUEST_KEYS | STATE_KEYS | BACK_KEYS | DRAIN_KEYS | ERROR_KEYS | SNAPSHOT_KEYS
BOOL_KEYS = frozenset(("result", "drainComplete", "attached", "windowFocused", "viewFocused",
                       "observerReady", "animationActive", "secure", "complete", "fixtureMatches"))
NULL_BOOL_KEYS = frozenset(("imeVisible", "identityNull"))
COUNT_KEYS = frozenset(("roots", "pendingLayoutRoots", "views", "nodes", "welcomeCount", "profileCount", "focusedCount"))


class InvalidTelemetry(ValueError):
    """Only static error codes are retained. Never attach the input or its filename."""


def fail(code):
    raise InvalidTelemetry(code)


def number(value, minimum=0, maximum=LONG_MAX):
    if len(value) > 20 or re.fullmatch(r"-?(?:0|[1-9][0-9]*)", value) is None:
        fail("INVALID_INTEGER")
    result = int(value)
    if not minimum <= result <= maximum:
        fail("INTEGER_OUT_OF_RANGE")
    return result


def scalar(key, value):
    if key in BOOL_KEYS | NULL_BOOL_KEYS:
        allowed = {"true": True, "false": False}
        if key in NULL_BOOL_KEYS:
            allowed["null"] = None
        if value not in allowed:
            fail("INVALID_BOOLEAN")
        return allowed[value]
    if key == "phase":
        allowed = PHASES
    elif key == "lifecycle":
        allowed = LIFECYCLES
    elif key == "scope":
        allowed = {"ACTIVITY_DECOR_MERGED"}
    elif key == "state":
        allowed = {"NO_UNIQUE_RESUMED_ACTIVITY", "EXPIRED_NO_STATE_PUBLISHED"}
    else:
        if key in {"request", "snapshot"}:
            return number(value, 1, MAX_SNAPSHOTS)
        if key in COUNT_KEYS:
            return number(value, 0, MAX_NODES)
        if key in {"activity", "decor", "probe"}:
            return None if key == "probe" and value == "null" else number(value, -(2**31), 2**31 - 1)
        return number(value, -1 if key == "generation" else 0)
    if value not in allowed:
        fail("INVALID_ENUM")
    return value


def parse_message(message):
    fields = {}
    for pair in message.split():
        if pair.count("=") != 1:
            fail("INVALID_FIELD")
        key, value = pair.split("=", 1)
        if key not in ALL_KEYS or key in fields:
            fail("UNKNOWN_OR_DUPLICATE_FIELD")
        fields[key] = value
    keys = frozenset(fields)
    if keys == ERROR_KEYS:
        # Class names are arbitrary strings too: accept a bounded class token but
        # replace it with a fixed category. Never serialize the original name.
        if re.fullmatch(r"(?:[A-Za-z_$][A-Za-z0-9_$]{0,127})?", fields["diagnosticUnavailable"]) is None:
            fail("INVALID_CAPTURE_ERROR")
        return {"kind": "capture_error"}
    kinds = {REQUEST_KEYS: "request", SNAPSHOT_KEYS: "snapshot", STATE_KEYS: "unavailable",
             EXPIRED_KEYS: "expired", BACK_KEYS: "back_returned", DRAIN_KEYS: "drain"}
    if keys not in kinds:
        fail("INVALID_RECORD_SHAPE")
    result = {key: scalar(key, value) for key, value in fields.items()}
    result["kind"] = kinds[keys]
    if keys == STATE_KEYS and result["state"] != "NO_UNIQUE_RESUMED_ACTIVITY":
        fail("INVALID_STATE_SHAPE")
    if keys == EXPIRED_KEYS and result["state"] != "EXPIRED_NO_STATE_PUBLISHED":
        fail("INVALID_STATE_SHAPE")
    if "observedMs" in result and result["observedMs"] < result["requestedMs"]:
        fail("CLOCK_REGRESSION")
    if keys == DRAIN_KEYS and result["drainLimitMs"] != DRAIN_MS:
        fail("UNREVIEWED_DRAIN_LIMIT")
    if keys == BACK_KEYS and result["durationMs"] > result["backReturnedMs"]:
        fail("IMPOSSIBLE_BACK_DURATION")
    if keys == SNAPSHOT_KEYS:
        if (result["roots"] > result["views"] or result["pendingLayoutRoots"] > result["roots"]
                or any(result[k] > result["nodes"] for k in ("welcomeCount", "profileCount", "focusedCount"))
                or (result["complete"] and result["roots"] == 0)):
            fail("INCONSISTENT_COUNTS")
    return result


def readout(stream):
    """Read bytes only. Return sanitized scalars; invalid target data yields no partial report."""
    events, requests, snapshots = [], {}, set()
    total = line_count = 0
    back = drain = process_id = None
    while True:
        line = stream.readline(MAX_LINE_BYTES + 1)
        if not line:
            break
        total += len(line)
        line_count += 1
        if total > MAX_INPUT_BYTES or len(line) > MAX_LINE_BYTES or line_count > MAX_LINES:
            fail("INPUT_LIMIT")
        # A different tag can mention our class in a stack trace. Inspect the
        # actual log tag, not substrings in arbitrary (possibly private) payloads.
        header = THREADTIME.match(line)
        if header is None or header.group(2).strip() != TAG:
            continue
        current_pid = number(header.group(1).decode("ascii"), 1, 2**31 - 1)
        if process_id is not None and current_pid != process_id:
            fail("MIXED_PROCESSES")
        process_id = current_pid
        try:
            decoded = line.rstrip(b"\r\n").decode("ascii")
        except UnicodeDecodeError:
            fail("NON_ASCII_TARGET")
        match = PREFIX.fullmatch(decoded)
        if match is None:
            fail("INVALID_LOG_PREFIX")
        event = parse_message(match.group(1))
        if len(events) >= MAX_EVENTS:
            fail("EVENT_LIMIT")
        kind = event["kind"]
        if kind == "request":
            sequence = event["request"]
            if sequence != len(requests) + 1:
                fail("DUPLICATE_OR_MISSING_REQUEST")
            if requests and event["requestedMs"] < requests[sequence - 1]["requestedMs"]:
                fail("CLOCK_REGRESSION")
            if back is not None and event["requestedMs"] < back["backReturnedMs"]:
                fail("CLOCK_REGRESSION")
            requests[sequence] = event
        elif kind in {"snapshot", "unavailable", "expired"}:
            sequence = event["snapshot"]
            request = requests.get(sequence)
            if request is None or sequence in snapshots or request["phase"] != event["phase"]:
                fail("UNMATCHED_OR_DUPLICATE_SNAPSHOT")
            if "requestedMs" in event and event["requestedMs"] != request["requestedMs"]:
                fail("REQUEST_TIME_MISMATCH")
            snapshots.add(sequence)
        elif kind == "back_returned":
            if back is not None:
                fail("MULTIPLE_BACK_RESULTS")
            if requests and event["backReturnedMs"] < requests[len(requests)]["requestedMs"]:
                fail("CLOCK_REGRESSION")
            back = event
        elif kind == "drain":
            if drain is not None:
                fail("MULTIPLE_DRAINS")
            drain = event
        events.append(event)
    if not events:
        fail("NO_DIAGNOSTIC_RECORDS")
    warnings = {"ASYNCHRONOUS_OBSERVATIONS_NOT_BACK_ROUTING_PROOF"}
    if set(requests) - snapshots:
        warnings.add("MISSING_SNAPSHOTS")
    if back is None:
        warnings.add("NO_BACK_RETURN_RECORDED")
    if drain is None or not drain["drainComplete"]:
        warnings.add("DRAIN_MISSING_OR_INCOMPLETE")
    for event in events:
        if event["kind"] in {"capture_error", "unavailable", "expired"}:
            warnings.add("OBSERVATION_UNAVAILABLE")
        if event["kind"] == "snapshot":
            if not event["complete"] or event["pendingLayoutRoots"]:
                warnings.add("PARTIAL_OR_LAYOUT_PENDING")
            if event["readMs"] > READ_BUDGET_MS:
                warnings.add("COOPERATIVE_READ_BUDGET_EXCEEDED")
            if not event["secure"]:
                warnings.add("SECURE_FLAG_FALSE")
            if event["identityNull"] is False:
                warnings.add("IDENTITY_NOT_NULL")
    return {"schema": 1, "not_a_test_verdict": True, "events": events,
            "missing_snapshot_sequences": sorted(set(requests) - snapshots), "warnings": sorted(warnings)}


def read_file(path):
    """Read a regular file through its checked descriptor, never a FIFO/device.

    O_NONBLOCK prevents opening a FIFO from waiting before fstat can reject it.
    O_NOFOLLOW rejects a symlink at the final path component. Parent directory
    selection remains the caller's responsibility; this is not a filesystem sandbox.
    """
    if not all(hasattr(os, flag) for flag in ("O_NOFOLLOW", "O_NONBLOCK")):
        fail("SAFE_FILE_OPEN_UNAVAILABLE")
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    try:
        metadata = os.fstat(descriptor)
        if not stat.S_ISREG(metadata.st_mode):
            fail("INPUT_NOT_REGULAR_FILE")
        if metadata.st_size > MAX_INPUT_BYTES:
            fail("INPUT_LIMIT")
        # Keep the original descriptor: do not reopen the pathname after checks.
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            return readout(stream)
    finally:
        os.close(descriptor)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logcat", help="One existing per-test threadtime log; '-' reads stdin")
    args = parser.parse_args(argv)
    try:
        if args.logcat == "-":
            result = readout(sys.stdin.buffer)
        else:
            result = read_file(args.logcat)
    except (InvalidTelemetry, OSError) as error:
        code = str(error) if isinstance(error, InvalidTelemetry) else "INPUT_UNAVAILABLE"
        json.dump({"schema": 1, "not_a_test_verdict": True, "error": code}, sys.stdout)
        sys.stdout.write("\n")
        return 2
    json.dump(result, sys.stdout, sort_keys=True, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
