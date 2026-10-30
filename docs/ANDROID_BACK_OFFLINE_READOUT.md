# Offline scalar diagnostic readout

Prepared 2026-10-30, locally only. The public diagnostic publication remains blocked;
the general instruction to develop projects did not resolve that destination-specific
denial. No new public write, CI run, workflow or Android/iOS runtime change is included.

`scripts/ci/read_android_back_diagnostics.py` safely reads **one existing per-test
threadtime logcat** and emits typed, allowlisted diagnostic records:

```sh
python3 scripts/ci/read_android_back_diagnostics.py path/to/per-test-logcat.txt
```

It does not collect logs, run the app, access a device, download dependencies, process
XML/archives or modify any files. Standard input is accepted with `-`. The output is
stdout only; the caller decides whether to save it privately. Do not use actual user
data to exercise this diagnostic. No workflow integration is added.

File-path input must be a regular file. The CLI uses a nonblocking, no-follow open,
checks the resulting descriptor and retains it while parsing. It rejects a FIFO,
directory, device, final-component symlink or already oversized file rather than
waiting for a writer or reopening a changed pathname. Platforms without these safe
open flags fail explicitly; `-` remains available for caller-managed stdin.
The existing byte/line limits still apply if a regular file grows after its initial
size check. Parent directories and stdin are caller-controlled: this is not a
filesystem sandbox or a wall-clock timeout for a slow filesystem/input stream.

## Safety and interpretation

- Only the actual `OnboardingBackDiag` log tag is selected. Other tags can mention the
  diagnostic class in stack traces without being mistaken for telemetry.
- Exact fields, enums, nullable Booleans, numeric ranges and record shapes are checked.
  Unknown fields, duplicated/missing request sequences, duplicate Back results,
  mixed processes, contradictory same-thread Back timestamps and inconsistent counts
  are rejected. Main/instrumentation threads may differ within one process. Free-form error class
  names become a fixed `capture_error` category, not output text.
- Bounds: 8 MiB input, 16 KiB per line, 100,000 lines, 28 events, eight snapshots and
  256 visited Views/nodes. Invalid data returns only a static error code, never an
  echoed line, input path or partial report. The original Kotlin limits are checked
  against the parser schema in source tests.
- Missing, unavailable, expired or incomplete snapshots are not converted into zero
  node counts. Duplicate Welcome counts and false security/identity Booleans remain
  visible. Unknown nullable values remain unknown.
- Posted snapshots can be observed after their named request phase. The parser accepts
  that legitimate ordering and always labels output `not_a_test_verdict: true`.
  Cooperative read-budget overruns are reported rather than hidden: framework getters
  cannot be preempted by the diagnostic's nominal 50 ms budget.
- Exit 0 means **schema accepted**, not that Android tests passed, Back navigated,
  security holds, or the race is fixed. Missing records fail explicitly. Exit 2 means
  the readout is unavailable/rejected. The real CI result and original failures remain
  independent evidence. Combining multiple tests/runs is unsupported and must not be
  used to manufacture one apparently coherent sequence.

## Local validation

The new parser tests use only controlled synthetic data, including privacy sentinels,
malformed/overflowed fields, partial observations, asynchronous ordering and input
limits. They exercise the actual Python implementation and CLI, not a port of Android
behavior. Source-schema checks compare the existing Kotlin telemetry keys/phases/limits.
Independent source review identified mixed-process merging, anonymous exception names
and contradictory Back timing as negative cases; all are now covered, alongside a
regression for unrelated stack traces that mention the diagnostic class.
File-input regression tests cover regular files, final symlinks, a FIFO with no
writer, directories, oversized files, pathname replacement, descriptor cleanup,
redacted missing-path errors and missing safe-open support.

The original Android diagnostic, the single real Back, the 500 ms readiness interval,
all original 10-second deadlines, FLAG_SECURE and destination/identity assertions are
unchanged. Production Android, iOS, Rust and CI files are unchanged. Android Kotlin
compilation and emulator execution are still not run. The existing JDK compiler was
found and checked without installation, but does not provide the absent Kotlin/Android
SDK dependencies. No runtime replacement, package installation or dependency download
was performed. This is readout/privacy validation, not an IME navigation fix.
