# Supported iOS privacy — review proposal

**Status: owner-delegated development candidate; publication and normal CI authorized,
not approved for merge or release.**
This is a fresh reconstruction after a workspace restart, not recovery of the earlier
unpublished patch. Base: platform draft PR 20,
`c69acf49ea5c66f6a2cd4a1acf688cd6c2a4536b`
(tree `a434508371b232942980c47eed45e80886045359`).
The earlier green PR 20 CI does not validate this proposal.

## Owner-delegated UX/security choice

After receiving the review and the ordinary-screenshot limitation, the owner delegated
choosing the best UX/security approach. The supported option is selected for development
and testing rather than relying on an undocumented UIKit workaround. Native acceptance
still determines adoption; this is not a release-readiness decision.

Replace the unsupported secure-text-field canvas workaround with supported privacy
covers. **Ordinary iOS screenshots remain possible, including displayed burn/view-once
content and recovery words.** No sender screenshot notice is added. This is a change
to the old iOS screenshot policy, not a claim to preserve its prevention guarantee.
Android FLAG_SECURE remains unchanged. Existing CI may validate the isolated candidate
branch; platform PR 20 and main remain unchanged until separately authorized integration.

The supported alternative reduces exposure while the app is inactive/background or
when recording/mirroring is reported. It cannot undo an already captured frame, block
a second camera, guarantee system UI coverage, or promise zero-frame transitions.
If the product requires guaranteed arbitrary screenshot prevention, do not approve
this proposal as satisfying that requirement.

## Apple evidence and API choice

Official Apple documentation was checked on 2026-10-29:

- [UITraitCollection.sceneCaptureState](https://developer.apple.com/documentation/uikit/uitraitcollection/scenecapturestate)
  and [UITraitSceneCaptureState](https://developer.apple.com/documentation/uikit/uitraitscenecapturestate-swift.struct):
  public scene capture traits are available from **iOS 17.0**, matching the deployment
  target. The proposal uses the attached window's trait collection, not a global scene.
- [UIScreen.isCaptured](https://developer.apple.com/documentation/uikit/uiscreen/iscaptured):
  indicates recording/mirroring/AirPlay. Apple directs newer code to scene capture state.
  Only an unspecified scene trait falls back to the attached scene's screen. That
  screen-level fallback may conservatively cover another scene on the same display.
  Unknown future trait values remain covered.
- [Screenshot notification](https://developer.apple.com/documentation/uikit/uiapplication/userdidtakescreenshotnotification):
  posts after a screenshot, so it cannot prevent it. It is intentionally not observed.
- [Background UI preparation](https://developer.apple.com/documentation/uikit/preparing-your-ui-to-run-in-the-background)
  and [scene deactivation](https://developer.apple.com/documentation/uikit/uiscene/willdeactivatenotification):
  the cover responds directly to the lifecycle event, without waiting for SwiftUI state
  propagation or assuming activationState has already changed.
- [NotificationCenter observer delivery](https://developer.apple.com/documentation/foundation/notificationcenter/addobserver(forname:object:queue:using:)):
  a nil queue uses synchronous delivery on the posting thread. These UIKit notifications
  are documented as main-actor events; controller handlers preserve that boundary.
- [UIWindow](https://developer.apple.com/documentation/uikit/uiwindow):
  a separate public window can be associated with the specific UIWindowScene and given a
  level above app-owned presentations. This is not a system-wide capture security boundary.

## Proposed implementation

1. Keep the stable root `.screenSecurity()` attachment. Overlapping probes for the same
   UIWindow share a lease-owned controller; one leaving cannot restore another's flags.
   The registry holds weak keys and weak values, and leases retain their shared entry.
   The SwiftUI root
   starts hidden behind an opaque placeholder. No private view lookup, layer reparenting,
   secure text field trick, screenshot interception, or production capture-test bypass.
2. On attachment create an opaque, non-key UIWindow for that scene, above alert-level
   app UI. Its full-frame background is opaque; safe-area insets constrain only the label.
   Never make this window key. Cover app content before observation and initial sampling.
3. Register scene lifecycle notifications and capture-trait updates, then sample current
   activation/capture state. Recording that began before launch is therefore considered.
4. Cover synchronously on willDeactivate, background entry, and foreground entry.
   Only didActivate plus an inactive capture result may reveal. Capture stopping while
   inactive/background does not reveal the content. Cover visibility changes are not animated.
5. End text editing when covering; preserve draft text, prior interaction and accessibility
   flags. Obscure underlying accessibility content while covered and restore prior flags
   when uncovered. These native behaviors still require execution/VoiceOver verification.
6. Explicitly remove notifications and trait registrations, restore saved flags and destroy
   the cover window on detach/disconnect. Attachment epochs reject stale notification
   callbacks; generation checks reject stale queued SwiftUI reveals. Detach queues a hide.
7. Keep original burn timers, view-once rules, notification policy, crypto, storage, wire
   protocol and Android files untouched. The overlay does not introduce a burn receipt.

No asynchronous callback can retract a frame already taken by the OS. Initial scene
attachment, UIKit notification delivery, compositing, keyboard/system windows, simulator
capture tools and external cameras create limits that must remain visible in product copy.

### Shared ownership and teardown

A per-window weak-to-weak registry returns leases on one entry/controller for the exact
UIWindow object. Each probe has its own UUID subscription and weakly captured listener.
Removing a token is idempotent; only the final removal tears down observers/cover and
restores original interaction/accessibility flags. Normal detach is synchronous. An
abandoned lease holds the entry until main-actor cleanup rather than revealing early.
Stale deinit cleanup cannot remove a newly acquired subscription.

Fanout snapshots tokens and rechecks membership and current cover state before each
listener, tolerating reentrant unsubscribe/rebind or synchronous lifecycle changes.
New subscribers use the controller's existing generation-guarded asynchronous state
replay, avoiding state mutation during SwiftUI updates. Surviving leases retain the same
entry across disconnect; a new subscriber cannot create a second controller alongside
old owners. Actual disconnected/reconnected UIKit scenes still need native acceptance.

## Verification ledger

**Passed in this reconstruction's Linux executor:**

- EN/FR generator freshness and format-key validation.
- 14 iOS Python regression/source checks (not Swift execution).
- 124 repository Python helper tests.
- Protocol source guard: 72 protected files, 14 existing reviewed exceptions; manifest unchanged.
- `git diff --check`.
- Zero diff from PR 20 under Android, Rust crates, protocol/extensions, and the invariant manifest.

**Added, not run locally:** complete nine-combination Swift policy matrix; eighteen native
XCTest fixtures covering already-active capture, lifecycle transitions, distinct controller state,
unrelated notifications, repeated attach, cleanup/rebind/disconnect, queued reveal invalidation,
opaque/full-frame/non-key cover, full-screen modal ordering, fictional keyboard draft preservation,
and shared-owner removal orders, deallocation, rebind, late cleanup and listener suppression.
Fixture notifications/trait overrides test controller wiring, not real recording hardware.

**Local limit:** neither `swift` nor `xcodebuild` is installed on this Linux executor.
No native test pass is inferred from Python checks or PR 20 CI.

### First exact-candidate hosted run — not green

[CI 41, attempt 1](https://github.com/un-the-et-un-cafe/auralis/actions/runs/40926376710)
ran the published commit `250f6ba536aae902abdaa87be94e0276d69027b8`
(tree `de2f191586bae143d36e0f7f17d440ac9da6c026`) using the unchanged normal CI workflow.
Both iOS 18.5 and 26.2 jobs passed SwiftPM, SDK 26.2 compilation, simulator signing/Keychain
entitlement checks and app UI smoke. Each native unit suite ran 40 tests with 3 failures:

- `testAbandonedFinalLeaseEventuallyRestoresFlagsAndReleasesController`
- `testDisconnectInvalidatesRevealsAndReusesSingleControllerForSurvivingLeases`
- `testDetachRestoresExistingFlagsAndRemovesObservers`

All three failed weak-window deallocation assertions. Logs showed an already-hidden window;
controller/cover-reference cleanup assertions passed. That suggests a UIKit lifetime/fixture
timing issue, **not yet proof that retention is harmless**. The next test-only candidate adds
an independent plain-UIWindow control, explicit autorelease scopes, synchronous hidden/non-key/
content-free/disconnected/no-scene-membership assertions, and a bounded main-run-loop drain.
The final weak-nil leak assertions remain mandatory. Abandoned-lease cleanup must release both
controller and window, not stop waiting as soon as only the controller disappears. Production
privacy behavior is unchanged. The control and corrected fixtures still require native execution.

Rust, workflow lint, Android build/lint/unit tests and API 35 instrumentation passed. API 31
failed before test startup when its emulator download encountered an HTTP/2 error; one normal
failed-job retry is running. API 36-16k ran 33 tests with one existing app-smoke timeout; that
failure is not treated as a pass or excused by unchanged Android source. No gates are disabled.

Actual iOS 17 execution and real recording/app-switcher/multiscene/VoiceOver/device acceptance
remain pending. Hosted simulator fixtures do not establish these physical/runtime guarantees.

Independent source review found and prompted fixes for detach visibility, stale notification
ownership and synchronous delivery. It is not an independent security audit or native runtime pass.
The final shared-ownership source was also read by the parent reviewer without an immediate
source blocker; this was not an independent native audit.

## Required acceptance before adoption

Use an erased test simulator or dedicated device with fictional contacts/messages only.
Never display/capture real recovery words, private messages, account details or notifications.

| Scenario | Expected evidence (all pending) |
| --- | --- |
| SDK 26 build; min iOS 17 | Exact candidate compiles and all SwiftPM/native/UI suites pass; physical/runtime iOS 17 gap closed |
| Recording already active before cold launch | Generic cover before app content is usable; inspect capture frame by frame, record any race exposure |
| Start/stop recording while active, inactive, background | Cover shown on reported capture; stop does not uncover inactive/background content |
| App switcher, lock/unlock, Control Center, interrupted permission/auth flows | Opaque snapshot/cover; no stale hidden or prematurely revealed UI after return |
| Sheets, alerts, full-screen viewer, Quick Look, keyboard | App-owned modal content covered; draft retained; keyboard/system-window limits documented |
| Ordinary screenshot | Screenshot may contain visible content; EN/FR copy accurately warns; no sender receipt |
| Independent scenes/displays | Recording/lifecycle in one scene does not mutate another; fallback screen conservatism understood |
| Attach/detach/rebind/disconnect/reconnect | No orphan window/observer, no stale callback reveal, no duplicate owners or stuck input |
| Resize/rotation/safe areas; light/dark; large text | Full-scene opaque coverage, readable label, no geometry gaps |
| VoiceOver and hardware keyboard | Covered underlying content unreachable; focus/input recover without data loss |
| Burn/view-once and recovery UI | Existing expiry/no-copy/no-share semantics unchanged; ordinary screenshot limitation explicitly accepted |

Run the existing `CI` workflow (`.github/workflows/ci.yml`) on the exact published candidate
branch using `workflow_dispatch`. This authorized normal workflow builds with Xcode 26.3 /
SDK 26.2 and tests iOS 18.5 and 26.2; it does not close the iOS 17 runtime gap. It includes
SwiftPM, the native XCTest directory, app UI smoke and the existing Android/Rust/helper gates.
No workflow files, runners, signing scope or paid services are changed. Verify the selected
workflow/ref and actual run SHA; dispatch once. No PR is created by this validation step. Do not use real-user screenshots
as test evidence. Preserve unrelated release blockers (SQLite privacy-reason inventory, physical
Hopla interoperability, signing, and power-loss/provider acceptance).

## Decision boundaries

- Owner delegation selects the supported tradeoff for development; it is not release approval
  or permission to weaken Android protection.
- Adoption still requires successful native acceptance evidence for the exact candidate. The
  ordinary-screenshot tradeoff remains explicit in product copy and review evidence.
- Rejection can retain the existing source while keeping its unsupported workaround a release blocker;
  the proposal does not present the old behavior as a guaranteed or supported alternative.

