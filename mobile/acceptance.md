# Mobile acceptance record

Status: Phase 4 reliability implementation and automated verification are
complete; physical acceptance remains open across the implemented phases. No physical-iPhone
acceptance has been performed.
This document records remaining evidence, not a waiver of the product checklist.

Delivery includes the user's [Phase 1 demo](../docs/product-specs/phase-demo-plan.md).
Demonstrate the running fixture prototype and distinguish an interim
implementation preview from completed physical-device acceptance.

Use the [full Phase 1 cases](../docs/test-plans/phase-1-test-cases.md) alongside
the [milestone requirements](../docs/product-specs/product-technical-spec.md).
Record build revision, device model, iOS version, text size, appearance, test
date, tester, observed result, and defects for each session.

## Automated evidence — 2026-09-07

Verified the uncommitted working tree using Xcode 26.6, Swift 6.3.3, and
iOS 26.5 simulators. The final command was:

```sh
./verify --phase 1 --simulator E8B8E33F-91D9-467A-B8E5-7CC1779F78DF --report-dir mobile/.verification-small
```

- Final iPhone SE (3rd generation) simulator: 21 core tests and 17 native UI
  tests passed, plus project generation and unsigned iOS device build.
- The earlier iPhone 17 Pro simulator run passed 17 UI tests. Strengthened
  retry-restoration coverage subsequently exposed a missing interruption
  status; the final SE run verifies that fix and the final scroll changes.
- Verification tooling: 10 regression tests passed; mypy and Ruff passed.
- Documentation path checks, Markdown lint, and formatting checks passed.
- Gate result: six passed, no failures or unavailable gates; one benign skip
  because the simulator was already booted; three physical gates remain
  explicitly not automatable.

Machine-readable JSON, JUnit, command logs, and xcresults are in the ignored
`mobile/.verification-small/` directory. The UI tests exercise actual dark
appearance and accessibility text size, newest-following, historical row
position through streaming/composer resizing/relaunch, multiline drafts,
Stop, retry, clarification, completion, and ordinary session restoration.
These checks do not replace human accessibility judgment.

The user viewed the running iPhone 17 Pro prototype during the
[recorded implementation preview](demo.md#recorded-implementation-preview--2026-09-07).

## Remaining physical acceptance

| Physical acceptance area                                                   | Evidence status            |
| -------------------------------------------------------------------------- | -------------------------- |
| Primary and smallest supported iPhone baseline                             | Not yet selected/validated |
| Several minutes reading, typing, sending, stopping, and recalling commands | Not performed              |
| Keyboard, multiline editing, dictation, selection, focus, safe areas       | Not performed              |
| Streaming follow, history anchor, newest-content control, long transcript  | Not performed              |
| Light/dark appearance and accessibility Dynamic Type                       | Not performed              |
| VoiceOver labels, focus order, announcements, complete core loop           | Not performed              |
| Draft and fixture restoration through lifecycle interruptions              | Not performed              |
| No persistent legacy surfaces or secondary navigation                      | Not performed on device    |

The iOS 17 deployment setting is an initial compatibility target. The available
iOS 26.5 simulators can provide automated evidence but cannot establish either
iOS 17 runtime compatibility or physical-iPhone usability. Those distinctions
must remain visible in delivery reports.

Before signing off, calibrate response and rendering budgets on the selected
phones. The technical vision's approximate 100 ms feedback and 250 ms local
operation targets are proposals, not measurements of this application.

## Phase 2 implementation evidence — 2026-09-07

The embedded Rust runtime, Swift/C boundary, SQLite journal, one-location,
one-NPC content slice, and deployed Parish Endpoint integration are implemented.
Phase 2 acceptance remains open for physical-iPhone validation. Deployment and
live simulator evidence are in the [Endpoint handoff](endpoint/phase2-handoff.md).

The combined `./verify --phase all` run in `mobile/.verification-phase2-final/`
passed 23 RundaleKit tests, 17 Phase 1 native UI tests, six Phase 2 native UI
tests, two bridge tests, seven Endpoint client tests, the unsigned device build,
Rust packaging, and mobile dependency checks. It found two Rust failures:
an obsolete FFI event-count expectation after the complete `/look` event batch
fix, and an intermittent alias bootstrap lock. Those failures were fixed and
are retained in the historical report rather than erased.

The corrected deterministic run was:

```sh
./verify --phase 2 --simulator E8B8E33F-91D9-467A-B8E5-7CC1779F78DF --report-dir mobile/.verification-phase2-confirmed
```

At that point it passed all 14 automated checks: 18 runtime tests, 20 persistence tests,
six FFI tests, one production Endpoint fixture test, 23 presentation tests,
two bridge tests, seven Endpoint client tests, six native UI tests, mobile
packaging/dependency checks, and the unsigned device build. The report has no
failures, one already-booted simulator skip, one unavailable live Endpoint
gate, and two physical gates marked not automatable. Verification-tool
regressions passed 16 tests; documentation path checks and diff whitespace
checks also passed.
The Phase 2 simulator tests cover opening state, offline `/look`, Rust NPC
completions, incremental replacement followed by a different validated final,
Stop/retry, and SQLite restoration of committed dialogue after relaunch.
They select a deterministic Endpoint transport. They do not call a provider.
The [recorded native preview](demo.md#phase-2-native-implementation-preview--2026-09-07)
also shows local `/look`, streamed dialogue, and actual process-relaunch recovery.

## Phase 2 deployed Endpoint evidence — 2026-09-09

Cloud Run revision `parish-server-00006-kew` serves the pinned
`parish-demo/rundale-dialogue@1` contract. After its forward-compatible migration
and no-traffic health/auth preflight, two opt-in native UI tests passed against
the production path: a real Firebase/Auth App Check Vertex Google stream
committed a validated final dialogue, and Stop produced `Interrupted; not
applied` with no late dialogue. The authoritative invocation row for Stop ended
`failed`, `REQUEST_CANCELLED`, with its durable cancellation marker set. The
full immutable image/contract hashes, migration execution, timings, rollback
revision, and review result are in the Endpoint handoff.

The final source tree also passed the normal deterministic Phase 2 gate:

```sh
./verify --phase 2 --simulator E8B8E33F-91D9-467A-B8E5-7CC1779F78DF \
  --report-dir mobile/.verification-endpoint-integration
```

Result: 14 passed, no failures, one already-booted simulator skip, one opt-in
live gate reported separately as unavailable to the deterministic runner, and
two physical gates marked not automatable. Included results were 16
ParishEndpointKit tests, two Rust production-wire fixture tests, and six Phase 2
simulator tests. The opt-in live UI class was then run directly and passed both
tests as recorded above.

Remaining acceptance requires a signed build exercised on the selected physical
iPhones, including App Attest and the device usability/accessibility matrix. An
unsigned arm64 build and an iOS 26.5 simulator run do not establish those gates
or iOS 17 runtime compatibility. No provider key or shared invocation secret is
bundled.

A final targeted native rerun after the fixture overflow-save adjustment passed
both Phase 1 restoration cases: interrupted stream/retry and transcript/draft
relaunch. Its xcresult is
`mobile/.verification-phase2-confirmed/xcresults/phase1-final-restoration.xcresult`.

## Phase 3 implementation evidence — 2026-09-12

The canonical three-location/three-NPC tiny world is implemented in the
embedded Parish path. The authored bundle and independent
[world sheet](content/canonical-world.md) define Kilteevan Village, the Letter
Office, Connolly Cottage, Peig Hannigan, Mícheál Connolly, and Róisín Connolly.
Rust owns graph travel, explicit game-time advancement, scheduled presence,
offline `/look`/`/people`/`/exits`, unavailable-person rejection, bounded local
reference resolution, clarification, and save/resume state. SwiftUI consumes
the resulting scene, time, weather, people, exits, and clarification events.

The final deterministic run was:

```sh
./verify --phase 3 --report-dir mobile/.verification-phase3-confirmed
```

Result: 10 automated gates passed with no failures or unavailable gates. The
already-running simulator produced one benign boot skip. Coverage included 22
portable runtime tests, mobile dependency isolation, arm64 device/simulator
Rust packaging, 23 RundaleKit tests, two bridge tests, an unsigned iOS device
build, and three Phase 3 native UI tests. The UI suite traversed all three
places, observed authoritative presence and schedule movement, selected an
ambiguous Connolly reference before Endpoint work began, and restored travel
and presence after process relaunch.

Phase 3 physical acceptance remains open: the same traversal, schedule,
clarification, and resume cases have not been performed on a signed iPhone.
This evidence does not close the outstanding Phase 1–2 accessibility,
App Attest, keyboard, lifecycle, or device-baseline gates.

## Phase 4 implementation evidence — 2026-09-14

The reliability work covers background interruption, explicit retry, draft
identity during delayed acceptance, serialized foreground recovery, transport
failure before/during output, bounded durable-history paging, restored reading
anchors, and accessibility-size composer layout. The SQLite format remains 1;
the full authoritative request ledger and event journal are preserved. Optional
viewport cursor fields remain compatible with earlier draft projections.

The native iPhone 17 Pro simulator checks exposed and verified fixes for a
256 KiB bridge overflow when reopening a 400-command save, a missing far-back
anchor on relaunch, and a composer extending outside the screen at accessibility
sizes. The final controller run passed 13 tests, including three actual
packaged-Rust/SQLite history and clarification cases. Six native recovery UI cases passed, and the
corrected accessibility case passed separately with fullscreen control bounds.
After the small-screen assertion corrections below, all seven Phase 4 recovery
UI cases passed together on the iPhone SE (3rd generation) simulator.
Earlier failed runs are retained in `mobile/.verification-phase4-smoke/`.

Independent review also required unresolved clarification recovery beyond the
presentation tail. The native regression passed after 800 later commands,
relaunch, and answering the original choice.

The first combined release gate exposed two earlier-phase test issues: the
simulator Return-submit field did not exercise the production multiline composer,
and a lazy UI query reread its pre-scroll identifier after scrolling. Testing the
production control and capturing that identifier exposed a real missing New text
affordance, which was fixed with explicit detached-history state. All 18 Phase 1
UI cases then passed together on the iPhone SE (3rd generation) simulator. The
Phase 2 simulator Return-submit case now explicitly selects the Mac keyboard
field, retaining coverage of that behavior alongside the production multiline
composer. The small-screen recovery suite also required scanning the scrollable
transcript when counting commands: virtualized rows above the keyboard are not
present in a visible-element query. The recorded travel failure showed the
correct Letter Office state; the revised assertion walks to the opening scene
and counts stable command IDs before returning to the tail.

Final Rust coverage passed at 70.07% (33,146/47,304 lines), above the 60.8%
ratchet. Verifier/release tooling passed 27 Python tests. `just check` and
`just verify` passed with `AGENT_CHECK_BASE_REF=origin/ios-port`, including the
existing Rust suite, harness walkthrough, documentation and frontend checks. The
first repository gate attempt lacked the worktree frontend dependencies; `npm ci`
installed the existing lockfile without dependency changes before the passing run.

The final combined Phase 1–4 gate passed on the iPhone SE (3rd generation),
iOS 26.5, with 23 passed gates, zero failures, one already-booted simulator skip,
three nonblocking unavailable gates (opt-in live Endpoint and future Phases 5–6),
and 11 explicitly nonautomated physical gates. All 51 native tests passed:
18 Phase 1 UI, eight Phase 2 UI, five Phase 3 UI, seven Phase 4 UI, and 13 native
controller tests. Receipts are in `mobile/.verification/verify.json`, JUnit, logs
and timestamped xcresults for `20260914T144738259459Z`.

### Beta upload and remaining delivery blocker

`just testflight-update` signed and validated version 0.1.0, local archive build 2,
and Apple accepted the upload at 11:02 EDT on 2026-09-14 (`EXPORT SUCCEEDED`).
The upload package metadata records `cfBundleVersion` 2. Receipts are retained in
`mobile/.build/release/release.log` and `receipt.json`.

App Store Connect remains at its password/passkey sign-in screen. The actual
portal build record, processing/compliance outcome, and **Testing** status in
**Internal Beta** could not be confirmed. This is an uploaded build with an
external delivery blocker, not a verified installable beta. The browser login
has been left for the user to complete; no additional tester invitation is needed.
Deterministic transport tests do not establish real connectivity or App Attest.

### Remaining physical Phase 4 gates

The primary iPhone 16 Pro Max was listed by Xcode but unavailable. No supported
small-screen physical iPhone was available. Neither required 20-minute session
has been performed. VoiceOver, dictation/text selection, lock/unlock, real
connectivity loss, physical force-quit timing, and Instruments measurements
remain open on both device classes. The simulator evidence cannot establish
iOS 17 compatibility or waive those gates. Use the
[Phase 4 cases](../docs/test-plans/phase-4-test-cases.md) for sign-off before Phase 5.

### September 14 iPhone feedback corrections

The user reported intermittent transcript following, an unavailable-Michael
error while talking about him to Peig, awkward symbol-keyboard completion, and
the missing waiting animation. These are existing interaction-scope corrections,
tracked as P4-11–P4-14 in the case matrix and described in [reliability.md](reliability.md).
The phone screenshot is useful defect evidence; it does not close the full
physical session or accessibility gates above.

The corrected native Letter Office regression follows the canonical morning
route so Peig has arrived, sends Hello, then the exact reported Michael-reference
message. It verifies a new Peig reply, one accepted command, no absent-Michael
error and the reply above the keyboard. Fixture interaction tests exercise the
People/Commands controls and knot cleanup; UIKit tests cover delayed row growth,
keyboard resizing and touching the tail without leaving it.

Final `just check`, `just verify`, and the Rust coverage ratchet passed; coverage
was 70.11% (33,215/47,376 lines) against a 60.8% floor. Independent review accepted
the final address-slot parser and native controls. `just testflight-update` ran
the complete mobile gate on SE3/iOS 26.5, finishing at 12:28 EDT: 23 passed gates,
zero failures, 56 native tests passed (20/8/5/8 UI cases by phase and 15 controller
cases). It separately reported one benign boot skip, three opt-in/future
unavailable gates and 11 nonautomated physical gates. Receipts use xcresult
timestamp `20260914T161442749055Z` under `mobile/.verification/`.

These corrections change no save schema or content identity. The user can
continue the existing save; physical VoiceOver, session and device-performance
acceptance remain open.

Apple accepted version **0.1.0 (3)** at 12:30 EDT on September 14. Signed archive
validation and upload passed; ContentDelivery metadata confirms build 3. The
App Store Connect website rejected the saved sign-in on this Mac, so **Testing**
in **Internal Beta** remains unverified. The remote user can finish any compliance
prompt and verify that status from their phone. This records an accepted upload,
not confirmed distribution; retained receipts are in the release log and the
`ios-beta-feedback` proof bundle.
