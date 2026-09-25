# Mobile acceptance record

Status: Phase 1–4 automated testing now includes a signed physical iPhone run.
Human acceptance remains deferred by user instruction; automated device evidence
does not complete that acceptance.
This document records remaining evidence, not a waiver of the product checklist.

Delivery includes the user's [Phase 1 demo](../docs/product-specs/phase-demo-plan.md).
Demonstrate the running fixture prototype and distinguish an interim
implementation preview from completed physical-device acceptance.

Use the [full Phase 1 cases](../docs/test-plans/phase-1-test-cases.md) alongside
the [milestone requirements](../docs/product-specs/product-technical-spec.md).
Record build revision, device model, iOS version, text size, appearance, test
date, tester, observed result, and defects for each session.

## Evidence provenance (audit #1992, P2-F12)

Dated sections below are historical records. Their verifier report
directories (for example `mobile/.verification-phase2-final/`,
`mobile/.verification-phase2-confirmed/`,
`mobile/.verification-endpoint-integration/` and
`mobile/.verification-small/`) are ignored local output and were not retained.
Their totals are claims tied to the named revision and date. They cannot be
re-verified from this repository, and they do not show which assertions ran.
Do not cite them as current proof.

Current Phase 2 evidence is the reproducible record in
[Phase 2 audit resolution evidence](#phase-2-audit-resolution-evidence--2026-09-25).
The `--phase2` launch argument now opens the canonical three-location world
under the 2026-09-16 scope amendment, so older instructions that expected a
one-location world describe a retired configuration.

## Phase 2 audit resolution evidence — 2026-09-25

Source: branch `claude/issue-1993-hofh7p` (PR #2005) at `61854817e`, based on
`ios-port` `1ed492355`. The only uncommitted change during the runs was this
document. Simulator: iOS 26.5, an already-booted simulator. All Endpoint
traffic in these runs uses the in-app mock transport, and none of this is
physical-device evidence.

```sh
./verify --phase 2 --report-dir mobile/.verification-1993-final-p2
./verify --phase 3 --report-dir mobile/.verification-1993-final-p3
```

- Phase 2: 14 automated gates passed, 0 failed. One simulator-boot skip
  (already booted). The opt-in live Endpoint gate is unavailable because it was
  not configured. Two physical gates are not automatable. The Phase 2
  simulator suite ran 16 tests and all 16 passed: 12
  `RundalePhase2UITests` and 4 app-hosted `RundaleSemanticBridgeTests`.
- Phase 3: 10 automated gates passed, 0 failed. One simulator-boot skip. Three
  physical gates are not automatable. All 8 Phase 3 UI tests passed,
  including inferred movement and inferred conversation through the composer.
- Signed Release archive (`python3 mobile/scripts/release.py archive`,
  development-signed, not uploaded): the code signature verified, the bundled
  Firebase project, app, and bundle IDs matched, and the credential scan found
  no Endpoint consumer key, provider API key, private key, or provider secret
  variable in any bundle file. The exact TestFlight `.ipa` was not inspected;
  `release.py testflight` runs the same scan on its archive before export.

What these runs assert, by finding:

| Finding | Assertion                                                                                                                                                                                                                              |
| ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| P2-F02  | Inferred input requests the Intent role before any action; the receipt names the executed action; travel changes the header exactly once.                                                                                              |
| P2-F04  | A provisional frame is replaced in the same row by a differing validated final, both natively and through the real FFI and reducer.                                                                                                    |
| P2-F05  | Stop, a late candidate, a transport failure, retry, and process loss keep one logical request, a new attempt each time, and no committed dialogue. A delayed mock completion is not committed after Stop, even after reopening SQLite. |
| P2-F06  | Repeated relaunch restores the same command and dialogue rows with exact counts; termination mid-stream restores an interrupted request with no late commit.                                                                           |
| P2-F07  | With every Endpoint request failing, `/look`, `look`, `where am i`, `/people`, `/exits`, `/help`, and exit travel each produce a new local result without Endpoint work.                                                               |
| P2-F09  | Every Phase 2 test except the return-key test drives the device multiline composer, proven by Return inserting a line break.                                                                                                           |
| P2-F10  | Production Rust events for success, intent receipt, stream replacement, clarification, cancellation, failure, and interruption decode through the Swift decoder into the reducer, and replay identically after reopening.              |

Remaining, not claimed here: the authenticated live Intent run, live
incremental delivery, and server-side cancellation (P2-F02/F03/F04/F05), and
all manual and physical-iPhone gates (P2-F11).

## Automated device evidence — 2026-09-14

Source: `ios-port` revision `b06eade45` plus the reviewed automated-acceptance
working-tree changes. Verifier receipts retain source fingerprints, build
identity, command logs and result bundles. Test build: **0.1.0 (6)**,
Xcode 26.6 (17F113). Phone: **iPhone 16 Pro Max, iOS 26.6.1**.

- **Passed:** 68 device-compatible Phase 1–4 native tests (21 Phase 1,
  seven Phase 2, six Phase 3, eight Phase 4 UI, 26 controller/native).
- **Passed:** the full deterministic phone soak: 1,207.71 seconds of workload,
  53 travel cycles, 53 streamed dialogues, and 18 background recoveries.
  The final three-test live/soak bundle had zero failures.
- **Passed:** both live production Endpoint tests: validated streamed dialogue
  and in-flight Stop without a committed late dialogue.
- **Passed:** all 69 native tests on both the iPhone SE (3rd generation)
  Release simulator and iPhone 17 Pro Debug simulator, both iOS 26.5, including
  simulator-specific Return-key behavior. The primary run was the release gate.
- **Passed:** 35 verifier/release regression tests; the small-screen full gate
  also passed Rust mobile/persistence/FFI and Swift package checks.
- **Skipped:** already-booted simulator setup. The simulator-only Return-key
  test is excluded from the physical suite.
- **Unavailable:** iOS 17 runtime/device. The full verifier also reports
  unimplemented Phase 5/6 gates and its opt-in live gate separately; the latter
  was exercised by the successful dedicated phone run above.
- **Deferred:** human acceptance listed below, including a small-screen
  physical phone and minimum-supported-iOS physical validation.

A fresh-install defect was found and fixed: runtime startup now creates the
save's parent directory before SQLite opens it. A native test starts from a
nonexistent nested directory and verifies successful creation and startup.
UI tests use isolated save/draft paths. Existing personal-save paths are unchanged.
After testing, the production-configured app was reinstalled without uninstalling;
all five files in the normal save directory matched byte-for-byte before and
after that reinstall (`save-preservation.json`). No initial pre-test snapshot
was taken, so this comparison specifically establishes reinstall preservation.

Earlier failed runs remain in the evidence: Release simulator builds attempted
an unsupported x86_64 Rust slice, physical empty composer values differed from
simulator values, and the initial soak incorrectly expected identical repeated
NPC prose. Corrected builds use the active architecture and device-compatible
assertions; the soak follows the current response's stable identity and accepts
canonical repeat normalization.

### Internal beta delivery

**0.1.0 (7)** was uploaded and verified as **Testing** in **Internal Beta**
through the in-app App Store Connect browser on September 14. It includes the
fresh-install save-directory fix. The archive uses the tested app source with
the incremented build number and production Endpoint configuration. The phone
was left running a development-signed build of the same fix with normal
production settings; TestFlight offers build 7 through its usual Update flow.

### Release performance baselines

| Measurement                                  | Phone result       | Interpretation                                                |
| -------------------------------------------- | ------------------ | ------------------------------------------------------------- |
| Launch, five samples                         | Mean 0.283 s       | XCTest application-launch metric                              |
| Local east/west round trip                   | Mean 5.600 s       | Two commands; includes typing, driver and rendering           |
| Older/newest scroll pair, 180-row fixture    | Mean 19.578 s      | Includes queries, gestures and automation overhead            |
| Peak physical memory during local round trip | Maximum 32.5 MB    | Sampled XCTest physical-memory metric                         |
| Live Endpoint send to final UI               | 6.918 s            | One sample; includes tap, auth, network, model and UI polling |
| Deterministic stream first chunk / final UI  | 4.700 s / 10.981 s | End-to-end mock sample, includes typing/driver                |

These are baselines, not approved responsiveness budgets. They do not isolate
engine work, measure frame hitches, or equate mock delay with live network
latency. The live timing receipt separately labels authentication, network,
model and UI overhead; it cannot isolate provider latency.

Detailed receipts are local ignored artifacts in
`mobile/.verification-device-acceptance/`,
`mobile/.verification-small-screen/` and
`mobile/.verification-automated-acceptance/`. The initial aggregate device
report contains the historical failures above; individual successful device
bundles and corrected reruns are the authoritative evidence, not an overall
pass claim for that initial report. See the
[repeatable procedure and requirement map](automated-acceptance.md).

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

## Historical physical acceptance checklist — 2026-09-07

The statuses below describe the original record. The September 14 automated
device evidence above supersedes automated coverage; human checks remain deferred.

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

This dated implementation record describes the then-current embedded Rust
runtime, Swift/C boundary, SQLite journal, one-location/one-NPC content slice,
and deployed Parish Endpoint integration. The owner amended scope on
2026-09-16 (issue #1992): the canonical three-location/three-NPC world is
permitted in Phase 2, and world size is not an acceptance blocker. Historical
evidence below remains scoped to the runs and build described at the time.
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

This section is a historical receipt for the named 2026-09-09 deployment. The
current 2026-09-24 release configuration is
`https://limerick-server-24861210203.us-east1.run.app`, organization
`limerick-demo`; see the [Endpoint handoff](endpoint/phase2-handoff.md). Do not
reuse the historical `parish-*` identity as current release configuration.

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

## Export declaration follow-up — September 14, 2026

Build **0.1.0 (4)** includes the same interaction fixes plus the reviewed
`ITSAppUsesNonExemptEncryption=false` declaration. The signed archive contains
the Boolean value, and upload metadata confirms Apple received build 4.
`just testflight-update` passed the complete mobile gate again at 12:55 EDT:
23 passed gates, zero failures, one boot skip, three opt-in/future unavailable
gates and 11 nonautomated physical gates. Native receipts use timestamp
`20260914T164229380370Z`. Release validation, declaration rejection cases,
plist lint and Python lint/format checks also passed.

The declaration rationale and reassessment requirements are in
[testflight.md](testflight.md#encryption-declaration). Build 4 should avoid the
repeated questionnaire for the current internal distribution. Website sign-in
remains unavailable, so **Testing in Internal Beta is not yet verified**.
This metadata change does not modify gameplay, dependencies or save formats,
and it does not close the outstanding physical-device acceptance gates.

## Follow-up phone feedback — September 14, 2026

The user confirmed the other interaction fixes are good, but supplied another
screenshot showing **New text** during an active reply with the keyboard visible.
They also identified Clear paired with a rain icon. This keeps transcript-follow
acceptance open and adds a header-icon correction to the next beta. The report
is limited physical feedback, not the full Phase 4 device/session acceptance.

The follow-recovery regression now fails on the old scroller for both held-touch
layout changes and programmatic animation-end callbacks, and passes with the
correction. Focused verification passed 26 native tests: 22 controller/model/
launch/weather cases and four UI cases covering repeated conversation, accepted
Send from history, deliberate history reading during a reply, composer resizing
and the native Letter Office conversation. Root inspected screenshots showing
the latest Peig reply above the keyboard, no spurious New text, and a sun beside
Clear. See the `ios-follow-recovery` proof bundle for red/green receipts.

`AGENT_CHECK_BASE_REF=origin/ios-port just verify`, repository formatting and
Markdown lint passed. No Rust implementation, dependencies, save schema or
content identity changed. The existing export declaration remains in place.

The complete release gate passed at 14:10 EDT: 23 automated gates, zero
failures, and all 64 native tests (21/8/5/8 UI cases plus 22 controller/model/
launch/weather cases). The runner separately reports one benign boot skip,
three opt-in/future unavailable gates and 11 physical gates. Receipts use
`20260914T175635309510Z`. The signed **0.1.0 (5)** archive passed metadata
and signature validation and retains the Boolean export exemption declaration.

Apple accepted **0.1.0 (5)**; upload completed successfully and ContentDelivery
metadata confirms build 5. **Testing in Internal Beta remains unverified**
because the website sign-in is unavailable. This records successful upload,
not confirmed distribution or physical acceptance of these two corrections.

## September 14 room-arrival feedback

First visits show the opening description followed by the current occupants;
repeat visits retain only the current occupants. Empty arrivals have no invented
presence line. `/look` still describes familiar rooms. Visit history uses the
existing saved world state; no save schema or content identity changed.

Native production-path proof used the iPhone SE (3rd generation) / iOS 26.5
simulator, SwiftUI, embedded Rust, and SQLite. It followed cottage → village →
office → terminate/relaunch → village → cottage → `/look`. The focused test
passed, and visual inspection confirmed singular/plural text and no repeated
opening on return. The full release suite also passed the stronger return-presence assertion on
an iPhone 17 Pro / iOS 26.5 simulator: the row must have a new identity, preventing
a historical row from satisfying it.

[First arrival](../docs/screenshots/ios-first-arrival.png) and
[return after save/resume](../docs/screenshots/ios-repeat-arrival.png) show the
actual native transcript. These are simulator receipts, not physical-iPhone
acceptance.

`just verify` and the coverage ratchet passed (70.12%, 33,235 / 47,397 lines;
60.8% floor). The initial `just testflight-update` verification ran 65 native
tests with one failure: the accessibility test compared a computed
`43.99999999999994`-point frame against exact 44. The assertion now tolerates
`1e-9` points of arithmetic noise, while retaining containment and hittability
checks. Independent review accepted this test-only correction. All eight
Phase 4 native tests then passed on the same simulator, completing the 65-test
verification across the original run and focused recovery. The final repository
gate passed again; no production source changed during recovery.

The original failed full-run report remains intact. The affected-suite rerun
and its result summary supplement it; they do not relabel that command as a
pass. Archive/upload follows the documented manual TestFlight recovery using
the already-verified production framework. Apple accepted **0.1.0 (6)**; exact
numeric ContentDelivery metadata confirms the received version/build. The signed
archive retains Boolean `ITSAppUsesNonExemptEncryption=false`. **Testing in
Internal Beta remains unverified** because App Store Connect website sign-in is
unavailable. Upload success does not close that delivery or physical-device gate.
