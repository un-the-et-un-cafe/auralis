# Auralis Telegram status — prepared, manual-only

This optional reporter sends **one** public Auralis progress snapshot per approved
manual dispatch. It is disabled unless `TELEGRAM_STATUS_ENABLED` is `true`, and
there is deliberately **no ten-minute schedule**. Preparing these files does not
publish, enable, or send anything. Recurring delivery needs a separate decision
about activation, cadence, and when to stop.

## Required setup before the first send

1. Revoke any bot token previously pasted into chat or otherwise exposed. Do not
   reuse it. Generate a replacement through the bot owner’s official Telegram
   management flow; never paste the replacement into a conversation, issue, PR,
   command, workflow input, source file, or log.
2. The bot owner must manually store the replacement in this repository’s
   **GitHub Actions secret** named `TELEGRAM_BOT_TOKEN`. The reporter reads it only
   from the send step’s environment. It accepts no command-line token.
3. Have the intended recipient explicitly confirm the exact channel username or
   numeric chat ID and that Auralis status messages may be posted there. Check
   the channel identity and bot posting permission through the Telegram UI.
   Do not guess a destination or query unrelated bot conversations. Only then
   set the Actions **secret** `TELEGRAM_CHAT_ID` to that confirmed value. Keep
   private channel identifiers/invite links out of source and public job logs.
4. Review and publish this workflow, script, and tests on the protected default
   branch. Review changes to all three before allowing secrets to reach them.
   Do not dispatch an unreviewed revision or add PR-triggered secret access.
5. Only after recipient, rotation, and publication are verified, set the Actions
   **variable** `TELEGRAM_STATUS_ENABLED` to `true`. This enables manual delivery,
   not a schedule. `GITHUB_TOKEN` is the workflow’s automatically scoped token;
   no personal access token is needed.
6. In **Actions → Auralis Telegram status (manual only)**, select the default
   branch, optionally select a PR number/provide a public progress update, and
   explicitly check `confirm_send` to send one report. The default is false.
   Verify the actual destination received the expected message before considering
   a separately authorized recurring mechanism.

Delivery remains blocked until these steps are completed. A skipped job means
the repository/default-branch/enablement guard was not satisfied, **not** that a
message was delivered. Missing secrets, destination, or explicit send confirmation
fail before any service request.

## Report contents and accuracy limits

- Current default-branch head, or the latest head and state of the explicitly
  selected Auralis PR. Private repositories and private PR heads are rejected.
- Latest `ci.yml` and `release-unsigned.yml` runs **for that exact SHA**; old green
  runs are not substituted. PR code is never checked out or executed. If the
  workflow API has no exact-head run (including merge-SHA-only checks), the
  report says so rather than inferring success.
- Android debug APK, unsigned Android candidate, and unsigned iOS archive
  availability are reported separately. An artifact must match the expected
  name, run ID, and SHA, be unexpired, and belong to a successful completed run.
  Links point to public GitHub run pages, not credential-bearing download URLs.
  Private or incomplete API results abort the report.
- A successful build is **not** a signed/store release. No signing, publishing,
  installation, device validation, or release approval is inferred.
- Completed work is an explicitly supplied **operator-reported** summary, not a
  fabricated account of agent activity. Without one the report says unavailable.
- Model token use always says **unavailable — no reliable token metering is
  connected**. GitHub API consumption, elapsed time, and guesses are not tokens.
- The next unsigned build ETA is an operator estimate, not a guarantee. It is
  omitted as unavailable if missing, for another SHA, over 30 minutes old, or
  already elapsed. No ETA for a signed release is invented.
- The head/public repository metadata are rechecked before composition. A
  detected concurrent push aborts rather than publishing known-obsolete status.
  API observations are not atomic; the message includes its observation time.

### Optional `progress_json` input

Inputs are visible to repository operators. Include **only already-public Auralis
work**, never credentials, personal data, private messages, hidden reasoning,
nonpublic project information, or unrelated work. `public: true` is the operator’s
attestation, not an automated privacy classifier. JSON is data; it is never
evaluated as code or instructions. No commit/PR bodies, logs, or arbitrary files
are downloaded for the report.

Use the exact current SHA and timezone-aware ISO dates; the following is a
synthetic schema example, not a current progress claim or ETA:

```json
{
  "public": true,
  "head_sha": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
  "as_of": "2026-10-28T11:55:00Z",
  "completed": ["Android build verification completed."],
  "next_build_eta": "2026-10-28T12:20:00Z"
}
```

Only these fields are accepted. Up to five items are allowed; each is normalized
to one line and capped at 160 characters. Known credential values and common
token patterns are redacted defensively; this does **not** make secret-bearing
input safe. Set `next_build_eta` to `null` or omit it when no current estimate is
available. Omit the whole input when there is no public work summary.

## Security and operational boundaries

- Standard-library Python only; no `pip`, shell interpolation of inputs, local
  actions, dynamically downloaded code, or untrusted PR execution in the secret
  step. The pinned checkout action persists no credentials and checks out only
  the reviewed script/test at the dispatched default-branch revision. Python
  isolated mode ignores user site packages and `PYTHONPATH`.
- Workflow permissions are read-only: `contents`, `actions`, `pull-requests`.
  GitHub credentials are sent only to `api.github.com`. The Telegram bot secret
  is used only in the Bot API path at `api.telegram.org`. Both use direct TLS
  with certificate/hostname verification, a 15-second socket timeout, bounded
  responses, and no proxy-environment inheritance.
- Redirects are rejected, not followed. There are **no automatic retries**,
  including HTTP errors, timeouts, malformed responses, or ambiguous outcomes.
  Inspect the destination before manually rerunning a failure: the first send
  might have succeeded despite a missing response. A successful rerun can create
  a duplicate; concurrency serialization does not provide deduplication.
- Before sending, `getMe`, `getChat`, and `getChatMember` check only the configured
  destination: it must be a channel with the exact confirmed ID/username, and
  this bot must have administrator posting permission. The send is pinned to the
  verified numeric channel ID. These responses are kept in memory, not logged.
  Private invite URLs are not accepted; `getUpdates` and chat discovery are never
  called. This verification does not replace the recipient’s prior confirmation.
- Payloads, tokens, response bodies, redirect destinations, and raw exceptions
  are never logged. No Telegram HTML/Markdown parse mode is enabled; link
  previews are disabled. No external links from API content are followed.
- Only public metadata/status and explicitly approved public summaries leave
  GitHub. This does not authorize reading other chats, using the bot for other
  purposes, or sending to any other recipient.
- Disable future manual sends by setting `TELEGRAM_STATUS_ENABLED=false` (or
  deleting it). Revoke/remove the token if compromised. An in-flight request
  cannot be undone by changing a variable; inspect its delivery separately.
- A later ten-minute scheduler must get separate approval, a defined stop
  condition, a trusted revision, and duplicate/rate-limit handling. Do not simply
  add a cron and assume exact timing, indefinite authorization, or fresh ETAs.

## Offline verification

From the repository root:

```sh
python3 -I scripts/test_telegram_status.py
```

Tests use synthetic credentials and mock every transport; a socket guard rejects
accidental live networking. They cover fail-closed setup, TLS/host restrictions,
redirect rejection, no ambiguous retries, credential redaction, bounded content,
exact-head status, artifact expiry/origin, private-repository rejection, concurrent
head changes, and stale/missing ETAs. No token or destination is needed to test.
Also run the repository’s workflow linter before publication. These focused
checks do not establish app build, Telegram delivery, or signed-release readiness.

API references: [Telegram sendMessage](https://core.telegram.org/bots/api#sendmessage),
[GitHub workflow runs](https://docs.github.com/en/rest/actions/workflow-runs),
[GitHub artifacts](https://docs.github.com/en/rest/actions/artifacts).
