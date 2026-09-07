# Mobile acceptance record

Status: Phase 1 implementation and automated verification complete; physical
acceptance remains open. No physical-iPhone acceptance has been performed.
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

The embedded Rust runtime, Swift/C boundary, SQLite journal, and one-location,
one-NPC content slice are implemented. Phase 2 acceptance remains open for
live Parish Endpoint integration and physical-iPhone validation. The separate
service task owns the [Endpoint handoff](endpoint/phase2-handoff.md).

The combined `./verify --phase all` run in `mobile/.verification-phase2-final/`
passed 23 RundaleKit tests, 17 Phase 1 native UI tests, six Phase 2 native UI
tests, two bridge tests, seven Endpoint client tests, the unsigned device build,
Rust packaging, and mobile dependency checks. It found two Rust failures:
an obsolete FFI event-count expectation after the complete `/look` event batch
fix, and an intermittent alias bootstrap lock. Those failures were fixed and
are retained in the historical report rather than erased.

The corrected run was:

```sh
./verify --phase 2 --simulator E8B8E33F-91D9-467A-B8E5-7CC1779F78DF --report-dir mobile/.verification-phase2-confirmed
```

It passed all 14 automated checks: 18 runtime tests, 20 persistence tests,
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

Remaining acceptance requires a deployed pinned Endpoint, consumer policy and
quota setup, successful native authentication/App Check, a real streamed reply,
and a signed build exercised on the selected physical iPhones. An unsigned
arm64 build and an iOS 26.5 simulator run do not establish those gates or iOS 17
runtime compatibility. No provider key or shared invocation secret is bundled.

A final targeted native rerun after the fixture overflow-save adjustment passed
both Phase 1 restoration cases: interrupted stream/retry and transcript/draft
relaunch. Its xcresult is
`mobile/.verification-phase2-confirmed/xcresults/phase1-final-restoration.xcresult`.
