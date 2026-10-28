#!/usr/bin/env python3
"""Send one explicitly authorized public Auralis status report; stdlib only.

No command-line credentials, third-party imports, redirects, retries, execution
of repository/PR content, or logging of payloads/HTTP exceptions. See the setup
and trust-boundary checklist in docs/TELEGRAM_STATUS.md before use.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
import http.client
import json
import os
import re
import ssl
import sys
import unicodedata
from urllib.parse import quote, urlencode

REPOSITORY = "un-the-et-un-cafe/auralis"
TIMEOUT_SECONDS = 15
MAX_RESPONSE_BYTES = 2_000_000
MAX_MESSAGE_UNITS = 3_500  # UTF-16 units; below Telegram's 4096-character limit.
MAX_PROGRESS_BYTES = 8_000
ETA_MAX_AGE = timedelta(minutes=30)
SHA_PATTERN = re.compile(r"[0-9a-f]{40}\Z")
TOKEN_PATTERN = re.compile(r"[0-9]{5,20}:[A-Za-z0-9_-]{20,100}\Z")
CHAT_PATTERN = re.compile(r"(?:-?[1-9][0-9]{0,19}|@[A-Za-z][A-Za-z0-9_]{4,31})\Z")


class SafeError(Exception):
    """Only fixed, credential-free error messages belong in this type."""


@dataclass(frozen=True)
class Config:
    bot_token: str = field(repr=False)
    chat_id: str = field(repr=False)
    github_token: str = field(repr=False)
    pr_number: int | None = None
    progress_json: str = field(default="", repr=False)

    @classmethod
    def from_env(cls, env: dict[str, str]) -> Config:
        if env.get("TELEGRAM_STATUS_ENABLED") != "true":
            raise SafeError("Reporting is disabled; no request sent.")
        if env.get("TELEGRAM_SEND_CONFIRMED") != "true":
            raise SafeError("Explicit send confirmation is required; no request sent.")
        if env.get("GITHUB_REPOSITORY") != REPOSITORY:
            raise SafeError("Only the configured Auralis repository is allowed.")
        bot_token = env.get("TELEGRAM_BOT_TOKEN", "")
        chat_id = env.get("TELEGRAM_CHAT_ID", "")
        github_token = env.get("GITHUB_TOKEN", "")
        if not TOKEN_PATTERN.fullmatch(bot_token):
            raise SafeError("A valid Telegram Actions secret is required; no request sent.")
        if not CHAT_PATTERN.fullmatch(chat_id):
            raise SafeError("A confirmed Telegram destination is required; no request sent.")
        if not github_token or not re.fullmatch(r"[A-Za-z0-9_]{20,255}", github_token):
            raise SafeError("A read-only GitHub token is required; no request sent.")
        pr = env.get("STATUS_PR_NUMBER", "").strip()
        if pr and not re.fullmatch(r"[1-9][0-9]{0,8}", pr):
            raise SafeError("The PR number must be a positive integer.")
        progress = env.get("STATUS_PROGRESS_JSON", "")
        if len(progress.encode("utf-8")) > MAX_PROGRESS_BYTES:
            raise SafeError("Progress input exceeds the size limit.")
        return cls(bot_token, chat_id, github_token, int(pr) if pr else None, progress)


class HTTPS:
    """Direct TLS to two fixed services; no proxy inheritance or redirects."""

    def request(
        self, host: str, method: str, path: str, *,
        token: str = "", payload: dict | None = None, allow_missing: bool = False,
    ) -> dict | list | None:
        if host not in {"api.github.com", "api.telegram.org"}:
            raise SafeError("The request host is not allowed.")
        if not path.startswith("/") or "\r" in path or "\n" in path:
            raise SafeError("The request path is invalid.")
        if (host == "api.github.com" and method != "GET") or (
            host == "api.telegram.org" and (method != "POST" or token)
        ):
            raise SafeError("The request method or credential scope is invalid.")
        headers = {"User-Agent": "auralis-public-status", "Accept": "application/json"}
        if host == "api.github.com":
            headers["Authorization"] = f"Bearer {token}"
            headers["Accept"] = "application/vnd.github+json"
            headers["X-GitHub-Api-Version"] = "2022-11-28"
        body = None
        if payload is not None:
            body = json.dumps(payload, ensure_ascii=True).encode("ascii")
            headers["Content-Type"] = "application/json"
        connection = None
        try:
            connection = http.client.HTTPSConnection(
                host, timeout=TIMEOUT_SECONDS, context=ssl.create_default_context()
            )
            connection.set_debuglevel(0)
            connection.request(method, path, body=body, headers=headers)
            response = connection.getresponse()
            if response.status == 404 and allow_missing and host == "api.github.com":
                return None
            if not 200 <= response.status < 300:
                # Never print response bodies, redirect locations, or token-bearing paths.
                raise SafeError("Service rejected the request; no automatic retry was made.")
            raw = response.read(MAX_RESPONSE_BYTES + 1)
            if len(raw) > MAX_RESPONSE_BYTES:
                raise SafeError("Service response exceeded the size limit; no retry was made.")
            result = json.loads(raw)
            if not isinstance(result, (dict, list)):
                raise SafeError("Service returned invalid data; no retry was made.")
            return result
        except SafeError:
            raise
        except Exception:
            # A send may have succeeded before a timeout/parse failure. Never retry it.
            raise SafeError(
                "Request outcome is uncertain; inspect the destination before any manual retry."
            ) from None
        finally:
            if connection is not None:
                try:
                    connection.close()
                except Exception:
                    pass


def object_value(value: object) -> dict:
    if not isinstance(value, dict):
        raise SafeError("Service response has an unexpected structure.")
    return value


def array_value(value: object) -> list:
    if not isinstance(value, list):
        raise SafeError("Service response has an unexpected structure.")
    return value


def valid_sha(value: object) -> str:
    if not isinstance(value, str) or not SHA_PATTERN.fullmatch(value):
        raise SafeError("A valid exact commit SHA is required.")
    return value


def positive_id(value: object) -> int:
    if type(value) is not int or value <= 0:
        raise SafeError("Service returned an invalid identifier.")
    return value


def timestamp(value: object) -> datetime:
    if not isinstance(value, str) or len(value) > 40:
        raise SafeError("A timezone-aware ISO timestamp is required.")
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
        if parsed.tzinfo is None:
            raise ValueError
        return parsed.astimezone(timezone.utc)
    except ValueError:
        raise SafeError("A timezone-aware ISO timestamp is required.") from None


def iso(value: datetime) -> str:
    return value.astimezone(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")


def redact(value: str, config: Config) -> str:
    for secret in (config.bot_token, config.github_token):
        if secret:
            value = value.replace(secret, "[redacted]")
    value = re.sub(r"\b[0-9]{5,20}:[A-Za-z0-9_-]{20,100}\b", "[redacted]", value)
    return re.sub(r"\b(?:gh[pousr]_|github_pat_)[A-Za-z0-9_]+", "[redacted]", value)


def clean_text(value: object, config: Config, limit: int = 160) -> str:
    if not isinstance(value, str):
        raise SafeError("Progress items must be text.")
    value = redact(value, config)
    value = "".join(c if unicodedata.category(c)[0] != "C" else " " for c in value)
    return " ".join(value.split())[:limit]


class GitHub:
    def __init__(self, http: HTTPS, config: Config):
        self.http, self.config = http, config

    def get(self, suffix: str, *, allow_missing: bool = False):
        return self.http.request(
            "api.github.com", "GET", f"/repos/{REPOSITORY}{suffix}",
            token=self.config.github_token, allow_missing=allow_missing,
        )

    def public_metadata(self) -> dict:
        metadata = object_value(self.get(""))
        if metadata.get("private") is not False or metadata.get("full_name") != REPOSITORY:
            raise SafeError("Only public Auralis repository data may be reported.")
        return metadata

    def target(self, metadata: dict) -> tuple[str, str]:
        if self.config.pr_number is not None:
            pr = object_value(self.get(f"/pulls/{self.config.pr_number}"))
            base = object_value(object_value(pr.get("base")).get("repo"))
            head = object_value(pr.get("head"))
            head_repo = object_value(head.get("repo"))
            if base.get("full_name") != REPOSITORY or head_repo.get("private") is not False:
                raise SafeError("The requested PR must have a public head and Auralis base.")
            state = "merged" if pr.get("merged") is True else pr.get("state")
            if state not in {"open", "closed", "merged"}:
                raise SafeError("The PR state is invalid.")
            return valid_sha(head.get("sha")), f"PR #{self.config.pr_number}: {state}"
        branch = metadata.get("default_branch")
        if not isinstance(branch, str) or not branch or len(branch) > 255:
            raise SafeError("The default branch is unavailable.")
        commit = object_value(self.get(f"/commits/{quote(branch, safe='')}"))
        return valid_sha(commit.get("sha")), "Current default-branch head"

    def latest_run(self, workflow: str, sha: str) -> dict | None:
        query = urlencode({"head_sha": sha, "per_page": 100})
        response = self.get(f"/actions/workflows/{workflow}/runs?{query}", allow_missing=True)
        if response is None:
            return None
        runs = array_value(object_value(response).get("workflow_runs"))
        # Recheck head locally; never borrow a green result from an older commit.
        matches = [object_value(run) for run in runs if object_value(run).get("head_sha") == sha]
        if not matches:
            return None
        return max(matches, key=lambda run: (timestamp(run.get("created_at")), positive_id(run.get("id"))))

    def available_artifacts(self, run: dict | None, sha: str, names: set[str], now: datetime) -> set[str]:
        if not run or run.get("status") != "completed" or run.get("conclusion") != "success":
            return set()
        run_id = positive_id(run.get("id"))
        result = object_value(self.get(f"/actions/runs/{run_id}/artifacts?per_page=100"))
        artifacts = array_value(result.get("artifacts"))
        if result.get("total_count") != len(artifacts):
            raise SafeError("Artifact results are incomplete; no availability claim was sent.")
        available = set()
        for item in artifacts:
            artifact = object_value(item)
            name = artifact.get("name")
            if not isinstance(name, str) or name not in names or artifact.get("expired") is not False:
                continue
            origin = object_value(artifact.get("workflow_run"))
            if origin.get("head_sha") != sha or origin.get("id") != run_id:
                continue
            if timestamp(artifact.get("expires_at")) > now:
                available.add(name)
        return available


def run_status(run: dict | None) -> str:
    if run is None:
        return "no exact-head run found"
    status = run.get("status")
    if status in {"queued", "in_progress", "waiting", "pending", "requested"}:
        return status.replace("_", " ")
    if status == "completed":
        conclusion = run.get("conclusion")
        if conclusion in {"success", "failure", "cancelled", "skipped", "timed_out", "action_required", "neutral", "stale", "startup_failure"}:
            return conclusion.replace("_", " ")
    return "unknown (not confirmed successful)"


def progress_lines(config: Config, sha: str, now: datetime) -> list[str]:
    unavailable = ["Completed work: no public operator update supplied (CI is not a work log).",
                   "Next unsigned build ETA: unavailable; no current estimate supplied."]
    if not config.progress_json.strip():
        return unavailable
    try:
        progress = object_value(json.loads(config.progress_json))
    except (ValueError, RecursionError):
        raise SafeError("Progress input must be a small valid JSON object.") from None
    allowed = {"public", "head_sha", "as_of", "completed", "next_build_eta"}
    if set(progress) - allowed or progress.get("public") is not True:
        raise SafeError("Progress input requires public=true and only documented fields.")
    update_sha = valid_sha(progress.get("head_sha"))
    updated = timestamp(progress.get("as_of"))
    if updated > now + timedelta(minutes=5):
        raise SafeError("Progress timestamp is in the future.")
    completed = array_value(progress.get("completed", []))
    if len(completed) > 5:
        raise SafeError("At most five completed-work items are allowed.")
    items = [clean_text(item, config) for item in completed]
    eta = timestamp(progress["next_build_eta"]) if progress.get("next_build_eta") is not None else None
    if update_sha != sha:
        return ["Completed work: supplied update is for a different head; not attributed to this commit.",
                "Next unsigned build ETA: unavailable (update does not match current head)."]
    stale = now - updated > ETA_MAX_AGE
    lines = [f"Completed work (operator-reported as of {iso(updated)}{' — stale update' if stale else ''}):"]
    lines.extend(f"- {item}" for item in items if item)
    if not any(items):
        lines.append("- No completed-work items supplied.")
    if eta is None:
        lines.append(unavailable[1])
    elif stale:
        lines.append("Next unsigned build ETA: unavailable (estimate is older than 30 minutes).")
    elif eta <= now:
        lines.append("Next unsigned build ETA: unavailable (previous estimate has elapsed).")
    else:
        lines.append(f"Next unsigned build ETA: {iso(eta)} (operator estimate, not a guarantee).")
    return lines


def bounded_message(text: str, config: Config) -> str:
    text = redact(text, config)
    if len(text.encode("utf-16-le")) // 2 > MAX_MESSAGE_UNITS:
        # Do not silently truncate release/ETA caveats at the end of a message.
        raise SafeError("Report exceeds the message limit; no report sent.")
    return text


def build_report(github: GitHub, config: Config, now: datetime) -> str:
    metadata = github.public_metadata()
    sha, target = github.target(metadata)
    # Validate operator input before any possible send.
    progress = progress_lines(config, sha, now)
    ci = github.latest_run("ci.yml", sha)
    packaging = github.latest_run("release-unsigned.yml", sha)
    debug_name = f"auralis-debug-apk-{sha}"
    android_name = f"auralis-android-unsigned-{sha}"
    ios_name = f"auralis-ios-unsigned-{sha}"
    debug = github.available_artifacts(ci, sha, {debug_name}, now)
    unsigned = github.available_artifacts(packaging, sha, {android_name, ios_name}, now)
    # Avoid sending a snapshot known to be obsolete after a concurrent push.
    latest_sha, latest_target = github.target(github.public_metadata())
    if latest_sha != sha:
        raise SafeError("The target head changed during collection; no report sent.")
    target = latest_target
    lines = ["Auralis — public progress", f"Observed: {iso(now)}", target, f"Exact head: {sha}",
             f"CI: {run_status(ci)}", f"Unsigned packaging: {run_status(packaging)}"]
    for label, run in (("CI run", ci), ("Packaging run", packaging)):
        if run:
            lines.append(f"{label}: https://github.com/{REPOSITORY}/actions/runs/{positive_id(run.get('id'))}")
    lines += [
        "Android debug build: " + ("artifact available (development only)." if debug else "not confirmed available for this head."),
        "Unsigned Android candidate: " + ("artifact available (review only)." if android_name in unsigned else "not confirmed available for this head."),
        "Unsigned iOS archive: " + ("artifact available (not an installable IPA)." if ios_name in unsigned else "not confirmed available for this head."),
        "Signed/store release: not established by these checks; separate signing and release approval required.",
        *progress,
        "Model token use: unavailable — no reliable token metering is connected.",
    ]
    return bounded_message("\n".join(lines), config)


def verify_destination(http: HTTPS, config: Config) -> int:
    """Resolve only the configured destination; never discover unrelated chats."""
    def telegram(method: str, payload: dict) -> dict:
        response = object_value(http.request(
            "api.telegram.org", "POST", f"/bot{config.bot_token}/{method}", payload=payload,
        ))
        if response.get("ok") is not True:
            raise SafeError("Telegram destination verification failed; no report sent.")
        return object_value(response.get("result"))

    bot = telegram("getMe", {})
    if bot.get("is_bot") is not True:
        raise SafeError("Telegram bot identity verification failed; no report sent.")
    bot_id = positive_id(bot.get("id"))
    chat = telegram("getChat", {"chat_id": config.chat_id})
    chat_id = chat.get("id")
    if chat.get("type") != "channel" or type(chat_id) is not int or chat_id >= 0:
        raise SafeError("The configured destination must resolve to a Telegram channel.")
    if config.chat_id.startswith("@"):
        username = chat.get("username")
        matches = isinstance(username, str) and ("@" + username).casefold() == config.chat_id.casefold()
    else:
        matches = str(chat_id) == config.chat_id
    if not matches:
        raise SafeError("Telegram destination identity did not match; no report sent.")
    membership = telegram("getChatMember", {"chat_id": chat_id, "user_id": bot_id})
    if membership.get("status") != "administrator" or membership.get("can_post_messages") is not True:
        raise SafeError("The bot lacks confirmed channel posting permission; no report sent.")
    if object_value(membership.get("user")).get("id") != bot_id:
        raise SafeError("Telegram bot membership did not match; no report sent.")
    # Pin the send to the verified numeric ID, not a potentially renamed username.
    return chat_id


def send_report(http: HTTPS, config: Config, text: str, verified_chat_id: int) -> None:
    if type(verified_chat_id) is not int or verified_chat_id >= 0:
        raise SafeError("A verified channel identifier is required before sending.")
    text = bounded_message(text, config)
    result = object_value(http.request(
        "api.telegram.org", "POST", f"/bot{config.bot_token}/sendMessage",
        payload={"chat_id": verified_chat_id, "text": text,
                 "link_preview_options": {"is_disabled": True}},
    ))
    if result.get("ok") is not True:
        raise SafeError("Telegram did not confirm delivery; no automatic retry was made.")
    message = object_value(result.get("result"))
    positive_id(message.get("message_id"))
    if object_value(message.get("chat")).get("id") != verified_chat_id:
        raise SafeError("Delivery destination was not confirmed; inspect Telegram before any retry.")


def main() -> int:
    try:
        config = Config.from_env(dict(os.environ))
        http = HTTPS()
        text = build_report(GitHub(http, config), config, datetime.now(timezone.utc))
        chat_id = verify_destination(http, config)
        send_report(http, config, text, chat_id)
    except SafeError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1
    except Exception:
        print("ERROR: Report failed; details withheld to protect credentials. No retry was made.", file=sys.stderr)
        return 1
    print("Auralis report delivery confirmed. No schedule is configured by this script.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
