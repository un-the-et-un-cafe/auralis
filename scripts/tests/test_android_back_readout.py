"""Offline parser/security tests with synthetic scalars, not Android/IME runtime tests."""
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / "scripts/ci/read_android_back_diagnostics.py"
SPEC = importlib.util.spec_from_file_location("back_readout", SCRIPT)
reader = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(reader)


def message(fields):
    def render(value):
        if value is None:
            return "null"
        if isinstance(value, bool):
            return str(value).lower()
        return str(value)
    return " ".join(f"{key}={render(value)}" for key, value in fields.items())


def line(fields):
    return ("10-30 07:30:00.000  123  456 I OnboardingBackDiag: " + message(fields) + "\n").encode()


def request(sequence=1, phase="BEFORE_HIDE", timestamp=100):
    return dict(request=sequence, phase=phase, requestedMs=timestamp)


def snapshot(**overrides):
    fields = dict(snapshot=1, phase="BEFORE_HIDE", requestedMs=100, observedMs=110, readMs=1,
                  activity=10, decor=20, lifecycle="RESUMED", attached=True, windowFocused=True,
                  viewFocused=False, imeVisible=False, observerReady=True, animationActive=False,
                  generation=0, probe=30, secure=True, scope="ACTIVITY_DECOR_MERGED", complete=True,
                  roots=1, pendingLayoutRoots=0, views=5, nodes=10, welcomeCount=0, profileCount=1,
                  focusedCount=0, fixtureMatches=True, identityNull=True)
    fields.update(overrides)
    return fields


def read(*events):
    return reader.readout(io.BytesIO(b"".join(line(event) for event in events)))


class BackReadoutTests(unittest.TestCase):
    def assert_invalid(self, code, *events):
        with self.assertRaisesRegex(reader.InvalidTelemetry, "^" + code + "$"):
            read(*events)

    def test_complete_scalar_observation_is_not_a_test_verdict(self):
        result = read(request(), snapshot(), dict(backReturnedMs=120, result=True, durationMs=10),
                      dict(drainComplete=True, drainLimitMs=250))
        self.assertTrue(result["not_a_test_verdict"])
        self.assertEqual([], result["missing_snapshot_sequences"])
        self.assertNotIn("passed", json.dumps(result))
        self.assertEqual(["ASYNCHRONOUS_OBSERVATIONS_NOT_BACK_ROUTING_PROOF"], result["warnings"])

    def test_null_visibility_and_identity_are_unknown_not_false(self):
        result = read(request(), snapshot(imeVisible=None, identityNull=None, probe=None))
        event = result["events"][1]
        self.assertIsNone(event["imeVisible"])
        self.assertIsNone(event["identityNull"])
        self.assertIsNone(event["probe"])
        self.assertNotIn("IDENTITY_NOT_NULL", result["warnings"])

    def test_security_failure_is_retained_not_normalized_to_success(self):
        result = read(request(), snapshot(secure=False, identityNull=False))
        self.assertIn("SECURE_FLAG_FALSE", result["warnings"])
        self.assertIn("IDENTITY_NOT_NULL", result["warnings"])

    def test_partial_counts_and_pending_layout_are_explicit(self):
        for sample in (snapshot(complete=False), snapshot(pendingLayoutRoots=1)):
            with self.subTest(sample=sample):
                self.assertIn("PARTIAL_OR_LAYOUT_PENDING", read(request(), sample)["warnings"])

    def test_zero_roots_cannot_claim_complete_observation(self):
        self.assert_invalid("INCONSISTENT_COUNTS", request(), snapshot(roots=0))
        result = read(request(), snapshot(roots=0, complete=False))
        self.assertIn("PARTIAL_OR_LAYOUT_PENDING", result["warnings"])

    def test_duplicate_welcome_nodes_are_retained(self):
        self.assertEqual(2, read(request(), snapshot(welcomeCount=2))["events"][1]["welcomeCount"])

    def test_cooperative_budget_overrun_is_reported_not_dropped(self):
        result = read(request(), snapshot(readMs=51))
        self.assertIn("COOPERATIVE_READ_BUDGET_EXCEEDED", result["warnings"])
        self.assertNotIn("COOPERATIVE_READ_BUDGET_EXCEEDED", read(request(), snapshot(readMs=50))["warnings"])

    def test_posted_before_back_snapshot_can_be_observed_after_back_return(self):
        result = read(request(phase="BEFORE_SYSTEM_BACK"), dict(backReturnedMs=105, result=False, durationMs=5),
                      snapshot(phase="BEFORE_SYSTEM_BACK", observedMs=110))
        self.assertFalse(result["events"][1]["result"])
        self.assertEqual(110, result["events"][2]["observedMs"])
        self.assertTrue(result["not_a_test_verdict"])

    def test_missing_snapshot_and_incomplete_drain_do_not_become_zero_counts(self):
        result = read(request(), dict(drainComplete=False, drainLimitMs=250))
        self.assertEqual([1], result["missing_snapshot_sequences"])
        self.assertIn("MISSING_SNAPSHOTS", result["warnings"])
        self.assertIn("DRAIN_MISSING_OR_INCOMPLETE", result["warnings"])
        self.assertNotIn("welcomeCount", json.dumps(result))

    def test_unavailable_activity_has_no_invented_counts(self):
        result = read(request(), dict(snapshot=1, phase="BEFORE_HIDE", requestedMs=100,
                                      observedMs=110, state="NO_UNIQUE_RESUMED_ACTIVITY"))
        self.assertIn("OBSERVATION_UNAVAILABLE", result["warnings"])
        self.assertNotIn("welcomeCount", result["events"][1])

    def test_expired_capture_after_drain_has_no_state(self):
        result = read(request(), dict(drainComplete=False, drainLimitMs=250),
                      dict(snapshot=1, phase="BEFORE_HIDE", state="EXPIRED_NO_STATE_PUBLISHED"))
        self.assertEqual("expired", result["events"][2]["kind"])
        self.assertNotIn("identityNull", result["events"][2])

    def test_capture_error_class_is_removed_from_report(self):
        secret = "SyntheticPrivateClassName"
        result = read(dict(diagnosticUnavailable=secret))
        self.assertEqual([{"kind": "capture_error"}], result["events"])
        self.assertNotIn(secret, json.dumps(result))

    def test_anonymous_throwable_empty_simple_name_is_still_sanitized(self):
        self.assertEqual([{"kind": "capture_error"}], read(dict(diagnosticUnavailable=""))["events"])

    def test_unknown_or_duplicate_fields_are_rejected(self):
        self.assert_invalid("UNKNOWN_OR_DUPLICATE_FIELD", dict(request(), secret="SyntheticPrivateText"))
        raw = line(request()).rstrip() + b" request=2\n"
        with self.assertRaisesRegex(reader.InvalidTelemetry, "UNKNOWN_OR_DUPLICATE_FIELD"):
            reader.readout(io.BytesIO(raw))

    def test_freeform_text_cannot_be_smuggled_into_scalars(self):
        for key in ("activity", "fixtureMatches", "identityNull", "lifecycle", "scope", "phase"):
            with self.subTest(key=key), self.assertRaises(reader.InvalidTelemetry):
                read(request(), snapshot(**{key: "SyntheticPrivateText"}))

    def test_missing_keys_and_wrong_state_shape_fail_closed(self):
        broken = snapshot()
        del broken["secure"]
        self.assert_invalid("INVALID_RECORD_SHAPE", request(), broken)
        self.assert_invalid("INVALID_STATE_SHAPE", request(),
                            dict(snapshot=1, phase="BEFORE_HIDE", state="NO_UNIQUE_RESUMED_ACTIVITY"))

    def test_numeric_boundaries_reject_overflow_negative_counts_and_noncanonical_values(self):
        for value in ("01", "+1", "1.0", "1e3", "-1", str(2**63), "9" * 100):
            with self.subTest(value=value), self.assertRaises(reader.InvalidTelemetry):
                read(dict(request(), requestedMs=value))
        self.assert_invalid("INTEGER_OUT_OF_RANGE", request(), snapshot(nodes=257))
        self.assert_invalid("INTEGER_OUT_OF_RANGE", request(), snapshot(generation=-2))
        result = read(request(), snapshot(activity=-(2**31), generation=-1, views=256, nodes=256))
        self.assertEqual(-1, result["events"][1]["generation"])

    def test_counts_cannot_exceed_observed_population(self):
        for changes in (dict(roots=6), dict(pendingLayoutRoots=2), dict(welcomeCount=11),
                        dict(profileCount=11), dict(focusedCount=11)):
            with self.subTest(changes=changes):
                self.assert_invalid("INCONSISTENT_COUNTS", request(), snapshot(**changes))

    def test_at_most_eight_requests_no_counter_wrap_or_unbounded_events(self):
        events = [request(i, timestamp=100 + i) for i in range(1, 9)]
        self.assertEqual(8, len(read(*events)["events"]))
        self.assert_invalid("INTEGER_OUT_OF_RANGE", *events, request(9))
        self.assert_invalid("EVENT_LIMIT", *[dict(diagnosticUnavailable="Error") for _ in range(29)])

    def test_duplicate_missing_and_unmatched_sequences_fail(self):
        self.assert_invalid("DUPLICATE_OR_MISSING_REQUEST", request(), request())
        self.assert_invalid("DUPLICATE_OR_MISSING_REQUEST", request(2))
        self.assert_invalid("UNMATCHED_OR_DUPLICATE_SNAPSHOT", snapshot())
        self.assert_invalid("UNMATCHED_OR_DUPLICATE_SNAPSHOT", request(), snapshot(), snapshot())
        self.assert_invalid("UNMATCHED_OR_DUPLICATE_SNAPSHOT", request(), snapshot(phase="IME_READY"))

    def test_snapshot_request_time_and_monotonic_order_are_checked(self):
        self.assert_invalid("REQUEST_TIME_MISMATCH", request(), snapshot(requestedMs=101))
        self.assert_invalid("CLOCK_REGRESSION", request(), snapshot(observedMs=99))
        self.assert_invalid("CLOCK_REGRESSION", request(), request(2, timestamp=99))

    def test_back_timeline_cannot_contradict_same_thread_requests(self):
        self.assert_invalid("CLOCK_REGRESSION", request(), dict(backReturnedMs=99, result=True, durationMs=1))
        self.assert_invalid("IMPOSSIBLE_BACK_DURATION", dict(backReturnedMs=1, result=True, durationMs=500))
        self.assert_invalid("CLOCK_REGRESSION", request(), dict(backReturnedMs=120, result=True, durationMs=10),
                            request(2, phase="AFTER_SYSTEM_BACK", timestamp=119))

    def test_repeated_back_or_drain_records_fail_closed(self):
        back = dict(backReturnedMs=120, result=True, durationMs=10)
        drain = dict(drainComplete=True, drainLimitMs=250)
        self.assert_invalid("MULTIPLE_BACK_RESULTS", back, back)
        self.assert_invalid("MULTIPLE_DRAINS", drain, drain)
        self.assert_invalid("UNREVIEWED_DRAIN_LIMIT", dict(drainComplete=True, drainLimitMs=251))

    def test_absent_diagnostics_are_not_a_pass(self):
        with self.assertRaisesRegex(reader.InvalidTelemetry, "NO_DIAGNOSTIC_RECORDS"):
            reader.readout(io.BytesIO(b"Synthetic unrelated log without telemetry\n"))

    def test_ignored_content_and_error_messages_never_appear_in_output(self):
        private = b"SYNTHETIC_PRIVATE_SENTINEL"
        output = reader.readout(io.BytesIO(private + b"\n" + line(request())))
        self.assertNotIn(private.decode(), json.dumps(output))
        self.assert_invalid("INVALID_CAPTURE_ERROR", dict(diagnosticUnavailable="Error:SYNTHETIC_PRIVATE_SENTINEL"))

    def test_non_ascii_and_wrong_tag_prefix_are_rejected(self):
        for raw, code in ((line(request()).replace(b"BEFORE_HIDE", b"\xff"), "NON_ASCII_TARGET"),
                          (line(request()).replace(b" I OnboardingBackDiag:", b" E OnboardingBackDiag:"), "INVALID_LOG_PREFIX")):
            with self.subTest(code=code), self.assertRaisesRegex(reader.InvalidTelemetry, code):
                reader.readout(io.BytesIO(raw))

    def test_other_tags_can_name_diagnostic_class_without_becoming_telemetry(self):
        other = (b"10-30 07:30:00.000  123  456 E AndroidRuntime: "
                 b"at OnboardingBackDiagnostics.kt SYNTHETIC_PRIVATE_SENTINEL \xff\n")
        result = reader.readout(io.BytesIO(other + line(request())))
        self.assertEqual(1, len(result["events"]))
        self.assertNotIn("SYNTHETIC_PRIVATE_SENTINEL", json.dumps(result))

    def test_mixed_processes_fail_but_main_and_instrumentation_threads_can_differ(self):
        other_process = line(snapshot()).replace(b"  123  456 I", b"  999  456 I")
        with self.assertRaisesRegex(reader.InvalidTelemetry, "MIXED_PROCESSES"):
            reader.readout(io.BytesIO(line(request()) + other_process))
        main_thread = line(snapshot()).replace(b"  123  456 I", b"  123  123 I")
        self.assertEqual(2, len(reader.readout(io.BytesIO(line(request()) + main_thread))["events"]))

    def test_input_limits_apply_to_ignored_lines_too(self):
        for setting, value, content in (("MAX_INPUT_BYTES", 8, b"123456789\n"),
                                         ("MAX_LINE_BYTES", 8, b"123456789\n"),
                                         ("MAX_LINES", 2, b"a\nb\nc\n")):
            with self.subTest(setting=setting), patch.object(reader, setting, value):
                with self.assertRaisesRegex(reader.InvalidTelemetry, "INPUT_LIMIT"):
                    reader.readout(io.BytesIO(content))

    def test_cli_invalid_data_returns_only_static_error_not_partial_events(self):
        raw = line(request()) + line(dict(snapshot(), secret="SYNTHETIC_PRIVATE_SENTINEL"))
        result = subprocess.run([sys.executable, str(SCRIPT), "-"], input=raw, capture_output=True, check=False)
        self.assertEqual(2, result.returncode)
        self.assertEqual({"schema": 1, "not_a_test_verdict": True, "error": "UNKNOWN_OR_DUPLICATE_FIELD"},
                         json.loads(result.stdout))
        self.assertEqual(b"", result.stderr)
        self.assertNotIn(b"SYNTHETIC_PRIVATE_SENTINEL", result.stdout)

    def test_cli_acceptance_is_schema_validation_only(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "-"], input=line(request()),
                                capture_output=True, check=False)
        self.assertEqual(0, result.returncode)
        output = json.loads(result.stdout)
        self.assertTrue(output["not_a_test_verdict"])
        self.assertIn("MISSING_SNAPSHOTS", output["warnings"])

    def test_regular_file_read_uses_same_bounded_parser(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(line(request()))
            self.assertEqual(read(request()), reader.read_file(path))

    def test_final_component_symlink_is_not_followed(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(line(request()))
            link = Path(directory) / "link"
            link.symlink_to(path)
            with self.assertRaises(OSError):
                reader.read_file(link)

    def test_fifo_is_rejected_without_waiting_for_a_writer(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "SYNTHETIC_PRIVATE_FIFO"
            os.mkfifo(path)
            result = subprocess.run([sys.executable, str(SCRIPT), str(path)],
                                    capture_output=True, timeout=5, check=False)
            self.assertEqual(2, result.returncode)
            self.assertEqual("INPUT_NOT_REGULAR_FILE", json.loads(result.stdout)["error"])
            self.assertEqual(b"", result.stderr)
            self.assertNotIn(b"SYNTHETIC_PRIVATE_FIFO", result.stdout)

    def test_directory_is_rejected_without_reading(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(reader.InvalidTelemetry, "INPUT_NOT_REGULAR_FILE"):
                reader.read_file(directory)

    def test_known_oversized_file_is_rejected_before_parser(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(line(request()))
            with patch.object(reader, "MAX_INPUT_BYTES", 1), patch.object(reader, "readout") as parser:
                with self.assertRaisesRegex(reader.InvalidTelemetry, "INPUT_LIMIT"):
                    reader.read_file(path)
                parser.assert_not_called()

    def test_checked_descriptor_survives_path_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(line(request()))
            original_fstat = os.fstat
            def replace_after_check(descriptor):
                metadata = original_fstat(descriptor)
                path.unlink()
                path.write_bytes(b"SYNTHETIC_PRIVATE_REPLACEMENT\n")
                return metadata
            with patch.object(reader.os, "fstat", side_effect=replace_after_check):
                self.assertEqual(read(request()), reader.read_file(path))

    def test_file_growth_after_size_check_still_obeys_byte_limit(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(line(request()))
            original_fstat = os.fstat
            def grow_after_check(descriptor):
                metadata = original_fstat(descriptor)
                with path.open("ab") as stream:
                    stream.write(b"x" * 100 + b"\n")
                return metadata
            with patch.object(reader, "MAX_INPUT_BYTES", 100), patch.object(reader.os, "fstat", side_effect=grow_after_check):
                with self.assertRaisesRegex(reader.InvalidTelemetry, "INPUT_LIMIT"):
                    reader.read_file(path)

    def test_descriptor_closed_after_parser_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "log.txt"
            path.write_bytes(b"invalid\n")
            original_close = os.close
            with patch.object(reader.os, "close", wraps=original_close) as close:
                with self.assertRaises(reader.InvalidTelemetry):
                    reader.read_file(path)
                descriptor = close.call_args.args[0]
                close.assert_called_once_with(descriptor)
                with self.assertRaises(OSError):
                    os.fstat(descriptor)

    def test_missing_file_cli_redacts_path_and_os_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "SYNTHETIC_PRIVATE_MISSING"
            result = subprocess.run([sys.executable, str(SCRIPT), str(path)],
                                    capture_output=True, timeout=5, check=False)
            self.assertEqual(2, result.returncode)
            self.assertEqual("INPUT_UNAVAILABLE", json.loads(result.stdout)["error"])
            self.assertEqual(b"", result.stderr)
            self.assertNotIn(b"SYNTHETIC_PRIVATE", result.stdout)

    def test_missing_safe_open_support_fails_closed(self):
        with patch.object(reader, "hasattr", side_effect=lambda obj, key: key != "O_NOFOLLOW", create=True):
            with self.assertRaisesRegex(reader.InvalidTelemetry, "SAFE_FILE_OPEN_UNAVAILABLE"):
                reader.read_file("not-opened")

    def test_schema_tracks_actual_kotlin_fields_phases_and_limits(self):
        source = (ROOT / "apps/android/app/src/androidTest/java/chat/omcp/app/OnboardingBackDiagnostics.kt").read_text()
        literals = re.findall(r'"(?:\\.|[^"\\])*"', source)
        keys = set(re.findall(r"\b([A-Za-z][A-Za-z0-9]*)=", " ".join(literals)))
        self.assertEqual(reader.ALL_KEYS, keys)
        phases = re.search(r"enum class Phase\s*\{([^}]+)\}", source).group(1)
        self.assertEqual(reader.PHASES, set(re.findall(r"\b[A-Z][A-Z_]+\b", phases)))
        for name in ("MAX_SNAPSHOTS", "MAX_VIEWS", "MAX_NODES", "READ_BUDGET_MS", "DRAIN_MS"):
            actual = int(re.search(rf"const val {name} = (\d+)", source).group(1))
            self.assertEqual(getattr(reader, name), actual)


if __name__ == "__main__":
    unittest.main()
