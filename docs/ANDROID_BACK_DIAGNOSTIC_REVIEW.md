# Android API 36 Back/IME diagnostic — local review candidate

Prepared 2026-10-29. **Diagnostic experiment, not a navigation or readiness fix.**
No publication, CI dispatch, workflow change, merge or release is authorized by this preparation.

## Base and preserved failure

Based on published iOS review commit `1ebd1931e6dd741984d4470cc387791f676b969c`,
tree `bfe851cfed2c531dab62fdac2033d6d06a4c333d`. This is a separate local branch;
the tested iOS source, review branch and PR20 are unchanged.

CI41 (`40926376710`) and CI42 (`40933077526`) each failed the same API 36-16 KB
`AppSmokeTest.realAppOnboardingDeniedPermissionsContactMessageAndRecreation`:
the original line 147 waited 10 seconds for exactly one Welcome/Get started node
after Activity recreation and one system Back. Each run had 32/33 Android tests pass.
Both logs show generation-zero IME readiness followed by late input-service activity
and IME Back handling. This supports, but does not prove, an outstanding IME
transaction consuming Back. It does not establish final app nodes or handler routing.
The repeated API 36 gate remains failed/unresolved.

Both iOS jobs passed on the base commit. Nothing in this experiment changes their code.

## Exact experiment

1. Keep the original interrupted-profile editor input, recreation and fixture assertion.
2. Keep the 500 ms IME stability requirement, 10-second readiness and destination waits,
   exactly one `UiDevice.pressBack()`, and all original destination, identity and
   secure-window assertions. Do not pre-initialize UiDevice, hide/disable the keyboard
   differently, send another Back, replace the real Back with a dispatcher call,
   alter production navigation or change any security flag.
3. Add fixed, asynchronous snapshot requests before hide, after readiness, before and
   after the real Back, and on destination success/failure. Back/readiness failure also
   requests a snapshot. The result Boolean and elapsed time include the existing lazy
   UiDevice initialization and accessibility-idle wait; they do not identify who consumed Back.
4. Read only the unambiguously resumed Activity already tracked by the test IME observer.
   No ActivityScenario/Compose-idleness wait occurs in failure capture. The original
   exception is rethrown unchanged; diagnostics are best effort and cannot replace it.
5. Add a separate no-IME **screen/dispatcher control** using the production OnboardingFlow,
   an isolated in-memory store, a fresh Rust core with no identity, and unconfigured cloud.
   It verifies no focused editor and explicitly known-hidden IME, then invokes the app
   dispatcher once and checks Profile → Welcome, identity-null and FLAG_SECURE.
   It never uses or clears the real smoke fixture's persistent identity, so test ordering
   does not require a data reset. It does not recreate an Activity and does not test system
   input routing. It is additional coverage, never a replacement for the original smoke.
6. Two pure instrumentation predicate tests cover exact Text/EditableText matching and
   prevent a label match from being mistaken for the committed editor fixture.

## Data and execution bounds

Only monotonic timestamps, phase enums, ephemeral numeric object identities, lifecycle,
attachment/window focus, nullable IME visibility, animation generation/readiness,
secure-flag Boolean, root/node counts, Welcome/Profile/focused-node counts, exact
fictional editor-match Boolean and identity-null Boolean are logged. No text, peer ID,
phrase, node/config/View dump or exception message is added. Existing test assertions
and their normal failure reporting remain unchanged.

Maximum eight snapshots; each visits at most 256 Views and 256 merged semantic nodes,
with a cooperative 50 ms read budget. Counts are explicitly scoped to the current
Activity's decor, not separate Dialog/Popup/system windows. Missing Activity, missing
roots, read errors and partial traversal are distinct from a trustworthy zero count.
`complete` means traversal completed, **not** that layout or external IME work is idle;
pending-layout roots are reported separately.

There is no waiting for snapshots in the Back/readiness/destination path. Request and
observation times are separate: a posted snapshot can observe state after its named
request phase. After the original outcome only, close drains for at most 250 ms and
cancels pending callbacks. A capture that notices expiration publishes no state.
Individual Compose/native getters cannot be preempted, so the count/time limits are
not a hard main-thread execution guarantee. No diagnostic polling or Back retry is added.
Even these observations can perturb timing: an instrumented pass is **not a proven fix**.

## Dumpsys review: deliberately excluded

No shell collection, screenshot, hierarchy dump, extra artifact upload or workflow edit
is included. `dumpsys input_method` can include editor labels/hints/options, clients,
input history and delegated IME/client dumps. `dumpsys window windows` can expose
window titles, package/activity identities and unrelated usage. A clean fictional
emulator reduces risk but does not make free-form output safe; FLAG_SECURE does not
sanitize text dumps. Any future collection needs separate review, bounded execution
and strict scalar allowlisting with no persisted raw fallback.

## How to interpret one run

| Observation | What it supports / does not establish |
|---|---|
| Real smoke fails; complete late snapshot still has Profile=1, Welcome=0, fixture=true, identityNull=true; control passes | Consistent with Back not navigating that recreated editor. Combined with IME Back logs, strengthens the pending-IME explanation. Does not prove the app handler ran or rule out recreation-specific registration problems. |
| Both control and real smoke fail with no IME/focus and secure flags intact | Investigate handler/destination registration and control fixture readiness; do not change IME deadlines to conceal it. |
| Welcome count is greater than one | The original exact-one assertion can fail because of duplicate semantics; not equivalent to staying on Profile. Inspect composition ownership before changing assertions. |
| Snapshot reports wrong/absent resumed Activity, no roots, partial traversal or pending layout | Observation is incomplete/transitional; do not treat counts as authoritative navigation state. |
| Late visibility/animation/window/probe changes after reported readiness | Evidence that the existing gate did not cover the later transition; still no permission to loop Back or raise timeouts. |
| identityNull=false or secure=false | Security regression signal; original assertions must remain failing. Stop and investigate rather than weaken checks. |
| Diagnostic real smoke passes | Timing-sensitive observation only. Preserve CI41/42 failures; do not declare the readiness race fixed. |

## Validation and next gate

- Local source checks: 128 Python helper tests passed (124 existing + 4 diagnostic
  guardrails), audited source invariant check passed (72 files, 14 unchanged reviewed
  exceptions), and `git diff --check` passed. These are **not Kotlin/native tests**.
- Exact Compose BOM `2024.10.01` resolves UI/test artifacts to 1.7.5. Public test APIs
  were checked in official source JARs. An independent read-only source reviewer found
  no obvious Kotlin/API blocker; their late-capture expiration issue was corrected.
- **Not run:** Android Kotlin compilation, the two new predicate tests, dispatcher
  control, original smoke or any emulator/device execution. This executor has a JRE,
  but no Android SDK, Gradle cache/generated native bindings, Kotlin compiler or emulator.
- Root must review the exact diff/tree before publication or any run. The existing
  `.github/workflows/ci.yml` dispatch has no inputs and always starts the full matrix;
  it cannot truthfully be called a focused API 36 dispatch. Do not alter it or create
  a paid runner to obtain a shortcut. Choose an authorized existing emulator environment
  for a focused experiment if available, otherwise explicitly approve one informative
  existing CI run. No blind repeats; retain original failures and inspect scalar evidence.
- This proposal leaves iOS 17/device privacy checks, SQLite privacy reasons, signing,
  Hopla interop and all other independent adoption/release gates unresolved.

## Primary API/exposure references

- [Exact Compose BOM](https://dl.google.com/dl/android/maven2/androidx/compose/compose-bom/2024.10.01/compose-bom-2024.10.01.pom),
  [UI 1.7.5 sources](https://dl.google.com/dl/android/maven2/androidx/compose/ui/ui-android/1.7.5/ui-android-1.7.5-sources.jar),
  [UI test 1.7.5 sources](https://dl.google.com/dl/android/maven2/androidx/compose/ui/ui-test-android/1.7.5/ui-test-android-1.7.5-sources.jar).
- [ViewRootForTest](https://developer.android.com/reference/kotlin/androidx/compose/ui/platform/ViewRootForTest).
  The reserved `onViewCreatedCallback` is untouched; no reflection, accessibility
  forcing or layout invalidation is used.
- [Android keyboard visibility](https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility).
- [AOSP input-method service dump](https://android.googlesource.com/platform/frameworks/base.git/+/4988ed31cce9383b65f4526b493f3164a49f7f99/services/core/java/com/android/server/inputmethod/InputMethodManagerService.java),
  [EditorInfo dump](https://android.googlesource.com/platform/frameworks/base/+/main/core/java/android/view/inputmethod/EditorInfo.java),
  [WindowState dump](https://android.googlesource.com/platform/frameworks/base/+/refs/heads/main/services/core/java/com/android/server/wm/WindowState.java).
  Framework sources explain exposure, not a byte-for-byte assertion about the pinned image.

## Follow-up candidate: normal smoke without diagnostic observation (2026-10-30)

CI43 passed all nine jobs on tree `62ffa235794770ba2c5bef74acfdf0baf91c585d`;
API36 reported 36 tests and zero failures. Its asynchronous destination snapshot
observed Welcome 108 ms after Back returned. The existing exact-one Welcome wait
already accommodates this: no longer deadline, sleep, extra Back or production
navigation change is warranted. CI41/42 failures remain unresolved evidence;
diagnostic main-thread observation can perturb timing.

This follow-up makes only the extra `OnboardingBackDiagnostics` observer opt-in.
Normal CI omits the argument, constructs no diagnostic instance, posts no diagnostic
snapshots and performs no diagnostic close/drain. The existing IME animation observer,
500 ms stability gate, 10-second waits, one system Back, destination/identity and
FLAG_SECURE assertions remain unchanged. This is **not instrumentation-free** and
is not a production fix.

For a separately authorized diagnostic experiment on a disposable test device,
pass the exact instrumentation argument `auralisBackDiagnostics=true`, for example
`-Pandroid.testInstrumentationRunnerArguments.auralisBackDiagnostics=true` to the
existing Gradle instrumentation command. Omitted values, different case and other
strings do not enable observation. Do not enable it in normal CI to obtain a pass.

Local source guards verify default-off construction, nullable request/drain calls,
unchanged one-Back/deadline/assertion contracts and absence of opt-in from the existing
workflow/runner. These guards are not Kotlin compilation or native execution.
A new normal-path native run is pending specific approval. The unchanged existing
workflow starts the full matrix, not an Android-only dispatch. A single unobserved
pass would be useful regression evidence, **not proof the intermittent race is fixed**.
A failure must remain failing and be investigated without retries or weakened gates.

Integration remains on `review/supported-ios-privacy`; PR20 and main are separate.
Earlier status sections describe historical preparation and are not current authority.

## Separate visible-to-hidden recreation control (local, unexecuted)

CI44 reproduced the original destination timeout with extra observations disabled:
API36 35/36 passed; the isolated no-IME dispatcher control passed. The original
AppSmokeTest remains byte-for-byte unchanged by this new experiment.

The additional `OnboardingImeRoundTripControlTest` uses production OnboardingFlow,
a native Rust core and an isolated in-memory repository in ComponentActivity.
It explicitly retains its test-owned view model across actual Activity recreation
and reattaches production UI. This is **not MainActivity or persistent-store restore
coverage**, and cannot replace that failing baseline. It does not clear app data or
rely on test ordering. ImeAnimationObserver accepts an explicit Activity class for
this control; its default MainActivity behavior remains unchanged.

After confirming the exact restored synthetic draft, the control requests IME show
once on the focused window and requires observed IME visibility within 10 seconds.
It then uses the existing hide request and 500 ms stable-hidden gate (10-second
readiness bound), sends exactly one real system Back, and requires exactly one
Welcome node within the original 10-second destination bound. Identity-null and
FLAG_SECURE are asserted throughout. Failure to observe visible IME is a failing
precondition, never a skip. No snapshots, extra Back, fixed sleep, deadline increase,
production navigation change or keyboard-disable flag is added.

Falsifiable interpretation: a passing round-trip control with a failing baseline
supports an input readiness hypothesis but does not establish a fix. Both failing
would refute visibility round-trip as sufficient. A failed visible precondition
redirects investigation to window/input connection setup, not destination assertions.
No hosted run or publication is authorized by this local preparation.

Android's official InputMethodManager reference deprecates ResultReceiver-based
show/hide acknowledgement at API36 as unreliable and recommends observing insets.
Visibility is a concrete observable; it is not proof all future IME work is drained.
- https://developer.android.com/reference/android/view/inputmethod/InputMethodManager
- https://developer.android.com/develop/ui/views/touch-and-input/keyboard-input/visibility

## CI45 outcome and return-semantics correction (local)

CI45 on remote head `1519daf7197760bac736b7c88430b49845eee36c` failed.
Rust, workflow lint, Android build and both iOS jobs passed. Android API31 ran
37 tests with one failure; API35/API36 each ran 37 with two failures. The new
round-trip control failed on all three at its newly added `pressBack()==true`
assertion, before destination assertions. API36 logs show InteractionController's
1000 ms expected-event timeout. Original API36 smoke timed out at its unchanged
10-second Welcome wait. API35's original smoke instead raised StaleObjectException
in UiObject2.getAccessibilityNodeInfo; this increment does not change that failure.

The previous implementation and review incorrectly described the boolean as an
injection-success verdict. Official UiDevice source invokes sendKeyAndWaitForEvent
for Back with TYPE_WINDOW_CONTENT_CHANGED. False can indicate that expected-event
wait timed out; it is not standalone proof that input injection failed. The new
control now records this boolean as a scalar observation and proceeds to the
existing exact destination, identity and secure-window assertions. Exceptions still
propagate. It sends exactly one Back, with no retry or deadline change. The original
AppSmokeTest is byte-identical. This corrects the experimental harness, not production
navigation; the control's actual destination result remains unvalidated.

Official source (implementation semantics, not proof of delivery on any one run):
https://android.googlesource.com/platform/frameworks/support/+/6282021737797e63ee9f2707aa4740edcf24e4e4/test/uiautomator/uiautomator/src/main/java/androidx/test/uiautomator/UiDevice.java
