#!/usr/bin/env python3
"""Offline tests. Credentials below are synthetic; all network access is mocked."""

from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import MagicMock, patch

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("telegram_status", Path(__file__).with_name("telegram_status.py"))
status = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = status
SPEC.loader.exec_module(status)

SHA = "a" * 40
OLD_SHA = "b" * 40
NOW = datetime(2026, 10, 28, 12, 0, tzinfo=timezone.utc)
BOT = "123456:" + "synthetic_not_a_real_secret_123456789"
GH = "ghp_" + "synthetic_not_a_real_secret_123456789"
CHAT_ID = -1001234567890
ENV = {
    "TELEGRAM_STATUS_ENABLED": "true",
    "TELEGRAM_SEND_CONFIRMED": "true",
    "GITHUB_REPOSITORY": status.REPOSITORY,
    "TELEGRAM_BOT_TOKEN": BOT,
    "TELEGRAM_CHAT_ID": "@auralis_test_only",
    "GITHUB_TOKEN": GH,
}
CONFIG = status.Config.from_env(ENV)
PUBLIC = {"private": False, "full_name": status.REPOSITORY, "default_branch": "main"}


def run(run_id=101, sha=SHA, conclusion="success", created="2026-10-28T11:45:00Z"):
    return {"id": run_id, "head_sha": sha, "created_at": created,
            "status": "completed", "conclusion": conclusion}


def artifact(name, run_id, *, expired=False, sha=SHA):
    return {"name": name, "expired": expired, "expires_at": "2026-11-01T00:00:00Z",
            "workflow_run": {"id": run_id, "head_sha": sha}}


def progress(**changes):
    data = {"public": True, "head_sha": SHA, "as_of": "2026-10-28T11:55:00Z",
            "completed": ["Android build verification completed."],
            "next_build_eta": "2026-10-28T12:20:00Z"}
    data.update(changes)
    return status.Config(BOT, CONFIG.chat_id, GH, progress_json=json.dumps(data))


class FakeGitHub(status.GitHub):
    def __init__(self, config=CONFIG):
        super().__init__(MagicMock(), config)
        self.calls = []
        self.metadata = deepcopy(PUBLIC)
        self.heads = [SHA, SHA]
        self.ci_runs = [run()]
        self.release_runs = [run(201)]
        self.artifacts = {
            101: [artifact(f"auralis-debug-apk-{SHA}", 101)],
            201: [artifact(f"auralis-android-unsigned-{SHA}", 201),
                  artifact(f"auralis-ios-unsigned-{SHA}", 201)],
        }

    def get(self, suffix, *, allow_missing=False):
        self.calls.append(suffix)
        if suffix == "":
            return self.metadata
        if suffix == "/commits/main":
            return {"sha": self.heads.pop(0)}
        if suffix.startswith("/pulls/"):
            return {"state": "open", "merged": False, "base": {"repo": PUBLIC},
                    "head": {"sha": self.heads.pop(0), "repo": {"private": False}}}
        if suffix.startswith("/actions/workflows/ci.yml/runs?"):
            assert "head_sha=" + SHA in suffix
            return {"workflow_runs": self.ci_runs}
        if suffix.startswith("/actions/workflows/release-unsigned.yml/runs?"):
            assert "head_sha=" + SHA in suffix
            return {"workflow_runs": self.release_runs}
        if suffix.startswith("/actions/runs/"):
            items = self.artifacts[int(suffix.split("/")[3])]
            return {"total_count": len(items), "artifacts": items}
        raise AssertionError("Unexpected endpoint")


class NoLiveNetwork(unittest.TestCase):
    def setUp(self):
        # An accidentally added live request fails locally instead of leaving the process.
        self.socket_guard = patch("socket.socket", side_effect=AssertionError("Live networking is forbidden in tests"))
        self.socket_guard.start()
        self.addCleanup(self.socket_guard.stop)


class ConfigTests(NoLiveNetwork):
    def test_missing_credentials_and_confirmation_fail_before_network(self):
        for name in ("TELEGRAM_BOT_TOKEN", "TELEGRAM_CHAT_ID", "GITHUB_TOKEN",
                     "TELEGRAM_STATUS_ENABLED", "TELEGRAM_SEND_CONFIRMED", "GITHUB_REPOSITORY"):
            env = ENV | {name: ""}
            with self.subTest(name=name), patch.dict(status.os.environ, env, clear=True), \
                    patch.object(status.HTTPS, "request") as request, redirect_stderr(io.StringIO()):
                self.assertEqual(status.main(), 1)
                request.assert_not_called()

    def test_rejects_other_repository_and_unsafe_destinations(self):
        for name, value in (("GITHUB_REPOSITORY", "someone/else"),
                            ("TELEGRAM_CHAT_ID", "https://example.com"),
                            ("TELEGRAM_CHAT_ID", "@dest\nInjected"),
                            ("TELEGRAM_BOT_TOKEN", BOT + "/getUpdates"),
                            ("GITHUB_TOKEN", GH + "\r\nHeader: bad"),
                            ("STATUS_PR_NUMBER", "1; echo unsafe")):
            with self.subTest(name=name, value=value), self.assertRaises(status.SafeError):
                status.Config.from_env(ENV | {name: value})

    def test_config_repr_contains_no_credentials_or_progress(self):
        self.assertNotIn(BOT, repr(CONFIG))
        self.assertNotIn(GH, repr(CONFIG))
        self.assertNotIn(CONFIG.chat_id, repr(CONFIG))

    def test_progress_input_is_bounded_before_parsing(self):
        with self.assertRaises(status.SafeError):
            status.Config.from_env(ENV | {"STATUS_PROGRESS_JSON": "x" * 8001})


class HTTPTests(NoLiveNetwork):
    def response(self, status_code=200, data=None):
        connection = MagicMock()
        response = connection.getresponse.return_value
        response.status = status_code
        response.read.return_value = json.dumps(data if data is not None else {"ok": True}).encode()
        return connection

    def test_tls_host_timeout_and_header_scope(self):
        connection = self.response()
        with patch.object(status.http.client, "HTTPSConnection", return_value=connection) as factory:
            status.HTTPS().request("api.github.com", "GET", "/repos/test/test", token=GH)
        self.assertEqual(factory.call_args.args, ("api.github.com",))
        self.assertEqual(factory.call_args.kwargs["timeout"], 15)
        context = factory.call_args.kwargs["context"]
        self.assertTrue(context.check_hostname)
        self.assertEqual(context.verify_mode, status.ssl.CERT_REQUIRED)
        self.assertEqual(connection.request.call_args.kwargs["headers"]["Authorization"], "Bearer " + GH)
        self.assertNotIn(BOT, str(connection.request.call_args))
        connection.set_debuglevel.assert_called_once_with(0)
        connection.close.assert_called_once()

    def test_disallowed_host_or_method_never_connects(self):
        cases = [("example.com", "POST"), ("api.github.com.evil", "GET"),
                 ("api.github.com", "POST"), ("api.telegram.org", "GET")]
        with patch.object(status.http.client, "HTTPSConnection") as factory:
            for host, method in cases:
                with self.subTest(host=host, method=method), self.assertRaises(status.SafeError):
                    status.HTTPS().request(host, method, "/")
            factory.assert_not_called()

    def test_redirects_and_error_statuses_are_not_followed_or_retried(self):
        for code in (301, 302, 307, 308, 400, 429, 500, 503):
            connection = self.response(code, {"description": BOT})
            connection.getresponse.return_value.getheader.return_value = "https://evil.example/" + BOT
            with self.subTest(code=code), patch.object(status.http.client, "HTTPSConnection", return_value=connection) as factory:
                with self.assertRaises(status.SafeError) as error:
                    status.send_report(status.HTTPS(), CONFIG, "Public report", CHAT_ID)
                self.assertNotIn(BOT, str(error.exception))
                factory.assert_called_once()
                connection.request.assert_called_once()
                connection.getresponse.return_value.read.assert_not_called()
                connection.getresponse.return_value.getheader.assert_not_called()

    def test_ambiguous_timeout_is_redacted_and_never_retried(self):
        connection = self.response()
        connection.getresponse.side_effect = TimeoutError("URL /bot" + BOT + "/sendMessage")
        with patch.object(status.http.client, "HTTPSConnection", return_value=connection) as factory:
            with self.assertRaises(status.SafeError) as error:
                status.send_report(status.HTTPS(), CONFIG, "Public report", CHAT_ID)
        self.assertIn("uncertain", str(error.exception))
        self.assertNotIn(BOT, str(error.exception))
        self.assertTrue(error.exception.__suppress_context__)
        factory.assert_called_once()
        connection.request.assert_called_once()

    def test_invalid_response_is_redacted_and_bounded(self):
        for raw in (BOT.encode(), b"x" * (status.MAX_RESPONSE_BYTES + 1), b'"wrong type"'):
            connection = self.response()
            connection.getresponse.return_value.read.return_value = raw
            with self.subTest(size=len(raw)), patch.object(status.http.client, "HTTPSConnection", return_value=connection):
                with self.assertRaises(status.SafeError) as error:
                    status.send_report(status.HTTPS(), CONFIG, "Public report", CHAT_ID)
                self.assertNotIn(BOT, str(error.exception))
                connection.getresponse.return_value.read.assert_called_once_with(status.MAX_RESPONSE_BYTES + 1)
                connection.request.assert_called_once()

    def test_missing_only_allowed_for_github_not_telegram(self):
        for host, method in (("api.github.com", "GET"), ("api.telegram.org", "POST")):
            with patch.object(status.http.client, "HTTPSConnection", return_value=self.response(404)):
                if host == "api.github.com":
                    self.assertIsNone(status.HTTPS().request(host, method, "/", allow_missing=True))
                else:
                    with self.assertRaises(status.SafeError):
                        status.HTTPS().request(host, method, "/", allow_missing=True)


class ReportTests(NoLiveNetwork):
    def test_exact_head_artifacts_and_release_boundaries(self):
        report = status.build_report(FakeGitHub(), CONFIG, NOW)
        self.assertIn("Exact head: " + SHA, report)
        self.assertIn("CI: success", report)
        self.assertIn("development only", report)
        self.assertIn("review only", report)
        self.assertIn("not an installable IPA", report)
        self.assertIn("Signed/store release: not established", report)
        self.assertIn("token use: unavailable", report)
        self.assertIn("ETA: unavailable", report)

    def test_explicit_pr_uses_current_head_without_reading_pr_content(self):
        config = status.Config(BOT, CONFIG.chat_id, GH, pr_number=7)
        github = FakeGitHub(config)
        report = status.build_report(github, config, NOW)
        self.assertIn("PR #7: open", report)
        self.assertEqual(github.calls.count("/pulls/7"), 2)
        self.assertFalse(any("/contents/" in call or "/comments" in call for call in github.calls))

    def test_private_repository_fails_closed(self):
        github = FakeGitHub()
        github.metadata["private"] = True
        with self.assertRaises(status.SafeError):
            status.build_report(github, CONFIG, NOW)
        self.assertEqual(github.calls, [""])

    def test_changed_head_aborts_instead_of_reporting_old_green(self):
        github = FakeGitHub()
        github.heads = [SHA, OLD_SHA]
        with self.assertRaisesRegex(status.SafeError, "head changed"):
            status.build_report(github, CONFIG, NOW)

    def test_latest_failure_wins_over_old_success_and_other_head(self):
        github = FakeGitHub()
        github.ci_runs = [run(101), run(102, conclusion="failure", created="2026-10-28T11:55:00Z"),
                          run(103, sha=OLD_SHA, created="2026-10-28T11:59:00Z")]
        report = status.build_report(github, CONFIG, NOW)
        self.assertIn("CI: failure", report)
        self.assertIn("Android debug build: not confirmed available", report)
        self.assertNotIn("/actions/runs/101", report)
        self.assertNotIn("/actions/runs/103", report)

    def test_no_exact_head_run_is_not_inferred_from_older_head(self):
        github = FakeGitHub()
        github.ci_runs = [run(sha=OLD_SHA)]
        github.release_runs = []
        report = status.build_report(github, CONFIG, NOW)
        self.assertIn("CI: no exact-head run found", report)
        self.assertNotIn("artifact available", report)

    def test_expired_or_mismatched_artifacts_are_not_available(self):
        github = FakeGitHub()
        github.artifacts[101][0]["expired"] = True
        github.artifacts[201][0]["workflow_run"]["head_sha"] = OLD_SHA
        github.artifacts[201][1]["expires_at"] = "2026-10-01T00:00:00Z"
        report = status.build_report(github, CONFIG, NOW)
        self.assertNotIn("artifact available", report)

    def test_oversized_artifact_page_fails_closed(self):
        github = status.GitHub(MagicMock(), CONFIG)
        github.http.request.return_value = {"total_count": 101, "artifacts": []}
        with self.assertRaisesRegex(status.SafeError, "incomplete"):
            github.available_artifacts(run(), SHA, set(), NOW)

    def test_eta_is_explicit_head_bound_and_fresh(self):
        lines = "\n".join(status.progress_lines(progress(), SHA, NOW))
        self.assertIn("12:20 UTC", lines)
        self.assertIn("operator estimate, not a guarantee", lines)
        cases = [({"as_of": "2026-10-28T11:00:00Z"}, "older than 30 minutes"),
                 ({"next_build_eta": "2026-10-28T11:59:00Z"}, "elapsed"),
                 ({"head_sha": OLD_SHA}, "does not match current head")]
        for changes, expected in cases:
            with self.subTest(expected=expected):
                text = "\n".join(status.progress_lines(progress(**changes), SHA, NOW))
                self.assertIn("ETA: unavailable", text)
                self.assertIn(expected, text)

    def test_progress_rejects_unknown_or_private_fields_and_bad_timestamps(self):
        cases = [{"public": False}, {"tokens": 123}, {"completed": ["x"] * 6},
                 {"as_of": "2026-10-28T14:00:00Z"}, {"as_of": "2026-10-28T11:00:00"},
                 {"next_build_eta": "tomorrow"}]
        for changes in cases:
            with self.subTest(changes=changes), self.assertRaises(status.SafeError):
                status.progress_lines(progress(**changes), SHA, NOW)

    def test_untrusted_text_is_plain_bounded_and_redacted(self):
        config = progress(completed=[BOT + " " + GH + "\nSigned release:\u202eyep", "😀" * 4000])
        report = status.build_report(FakeGitHub(config), config, NOW)
        self.assertNotIn(BOT, report)
        self.assertNotIn(GH, report)
        self.assertNotIn("\u202e", report)
        self.assertNotIn("\nSigned release:", report)
        self.assertIn("[redacted]", report)
        self.assertLessEqual(len(report.encode("utf-16-le")) // 2, status.MAX_MESSAGE_UNITS)
        self.assertIn("Model token use: unavailable", report)

    def test_message_limit_preserves_caveats_by_failing_instead_of_truncating(self):
        with self.assertRaises(status.SafeError):
            status.bounded_message("😀" * 2000, CONFIG)


class SendTests(NoLiveNetwork):
    def verified_responses(self):
        return [
            {"ok": True, "result": {"is_bot": True, "id": 123456}},
            {"ok": True, "result": {"id": CHAT_ID, "type": "channel", "username": "auralis_test_only"}},
            {"ok": True, "result": {"status": "administrator", "can_post_messages": True,
                                    "user": {"id": 123456}}},
        ]

    def test_preflight_pins_confirmed_channel_and_checks_posting_permission(self):
        http = MagicMock()
        http.request.side_effect = self.verified_responses()
        self.assertEqual(status.verify_destination(http, CONFIG), CHAT_ID)
        self.assertEqual(http.request.call_count, 3)
        self.assertEqual([call.args[2].rsplit("/", 1)[-1] for call in http.request.call_args_list],
                         ["getMe", "getChat", "getChatMember"])
        self.assertEqual(http.request.call_args.kwargs["payload"], {"chat_id": CHAT_ID, "user_id": 123456})

    def test_preflight_rejects_wrong_destination_type_identity_or_permission(self):
        for index, changes in ((0, {"is_bot": False}), (1, {"type": "private"}),
                               (1, {"username": "unrelated_channel"}),
                               (2, {"can_post_messages": False}), (2, {"status": "member"}),
                               (2, {"user": {"id": 999}})):
            http = MagicMock()
            responses = self.verified_responses()
            responses[index]["result"].update(changes)
            http.request.side_effect = responses
            with self.subTest(changes=changes), self.assertRaises(status.SafeError):
                status.verify_destination(http, CONFIG)
            self.assertFalse(any(call.args[2].endswith("/sendMessage") for call in http.request.call_args_list))

    def test_numeric_destination_must_match_exactly(self):
        config = status.Config(BOT, str(CHAT_ID), GH)
        http = MagicMock()
        http.request.side_effect = self.verified_responses()
        self.assertEqual(status.verify_destination(http, config), CHAT_ID)
        responses = self.verified_responses()
        responses[1]["result"]["id"] -= 1
        http.request.side_effect = responses
        with self.assertRaises(status.SafeError):
            status.verify_destination(http, config)

    def test_send_has_plain_text_disabled_previews_and_correct_credential_scope(self):
        http = MagicMock()
        http.request.return_value = {"ok": True, "result": {"message_id": 42, "chat": {"id": CHAT_ID}}}
        status.send_report(http, CONFIG, "Public text " + GH, CHAT_ID)
        http.request.assert_called_once()
        args, kwargs = http.request.call_args
        self.assertEqual(args[:2], ("api.telegram.org", "POST"))
        self.assertEqual(args[2], "/bot" + BOT + "/sendMessage")
        self.assertNotIn("token", kwargs)
        self.assertEqual(kwargs["payload"]["chat_id"], CHAT_ID)
        self.assertNotIn("parse_mode", kwargs["payload"])
        self.assertEqual(kwargs["payload"]["link_preview_options"], {"is_disabled": True})
        self.assertNotIn(GH, kwargs["payload"]["text"])

    def test_delivery_error_does_not_echo_telegram_description_or_retry(self):
        http = MagicMock()
        http.request.return_value = {"ok": False, "description": BOT}
        with self.assertRaises(status.SafeError) as error:
            status.send_report(http, CONFIG, "Public text", CHAT_ID)
        self.assertNotIn(BOT, str(error.exception))
        http.request.assert_called_once()

    def test_main_redacts_unexpected_exceptions(self):
        output = io.StringIO()
        with patch.dict(status.os.environ, ENV, clear=True), \
                patch.object(status, "build_report", side_effect=RuntimeError(BOT + GH)), \
                redirect_stderr(output), redirect_stdout(output):
            self.assertEqual(status.main(), 1)
        self.assertNotIn(BOT, output.getvalue())
        self.assertNotIn(GH, output.getvalue())
        self.assertNotIn("Traceback", output.getvalue())

    def test_success_logs_neither_payload_nor_credentials(self):
        output = io.StringIO()
        with patch.dict(status.os.environ, ENV, clear=True), \
                patch.object(status, "build_report", return_value="Public work summary"), \
                patch.object(status, "verify_destination", return_value=CHAT_ID), \
                patch.object(status, "send_report") as send, redirect_stdout(output):
            self.assertEqual(status.main(), 0)
            send.assert_called_once()
        self.assertNotIn("Public work summary", output.getvalue())
        self.assertNotIn(BOT, output.getvalue())
        self.assertNotIn(GH, output.getvalue())


if __name__ == "__main__":
    unittest.main(verbosity=2)
