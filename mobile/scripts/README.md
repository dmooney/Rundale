# Mobile verification runner

## Build and TestFlight commands

`just mobile-build` and `just testflight-update` invoke `release.py` to build the
Release iPhone app or verify, sign, and upload an internal beta. See the
[TestFlight runbook](../testflight.md) for prerequisites, dry-run usage, and
the remaining Apple processing/compliance step. Exercise the orchestration
without uploading using `python3 -m unittest discover -s mobile/scripts -p test_release.py`.

## Verification

The repository entry point is `./verify`. It runs the native mobile checks
sequentially and writes evidence under `mobile/.verification/`.

## Phases

- `./verify --phase 1`, `--phase 2`, `--phase 3`, or `--phase 4` runs that implemented gate.
- `./verify` and `./verify --phase all` run Phases 1–4 and report Phases 5–6 as
  non-blocking future work.
- `./verify --phase 5` and `./verify --phase 6` report the selected phase as
  unavailable and exit nonzero until it is implemented.

Phase 4 includes the earlier regression suites and native lifecycle/connectivity
recovery tests. Its physical sessions, accessibility judgment, and performance
budgets remain separate recorded gates. Simulator suites share compiled products
within each run while retaining separate result bundles.

Phase 1 includes the original interaction suite, the focused accessibility and
keyboard audit, automatic fixture streaming, and fixture history volume tests.
Pin the small iPhone simulator for the accessibility regression; a large-screen
pass alone does not cover the SE3 layout defect tracked in #1990.

## Options

The runner accepts these options:

```text
--phase 1-6|all
--project-spec PATH
--project PATH
--scheme NAME
--package-path PATH
--ui-tests-path PATH
--simulator UDID_OR_NAME
--configuration NAME
--report-dir PATH
```

Set `RUNDALE_IOS_SIMULATOR` to pin the simulator used by release verification;
an explicit `--simulator` takes precedence. This avoids altering unrelated booted
simulators.

The path overrides are useful for fixtures and isolated test projects. The
simulator override accepts an available simulator UDID or name; otherwise the
runner chooses a booted, newest available iPhone simulator. The default report
directory is `mobile/.verification/`.

## Record one UI test

`record-ui-test.py` records one named XCUITest method and keeps the video,
Xcode result bundle, and combined log together:

```sh
python3 mobile/scripts/record-ui-test.py \
  --target RundaleUITests --class RundaleUITests \
  --method testReadingHistoryExposesNewTextAndReturnsToNewest \
  --simulator "Rundale Phase 1 Small iPhone" \
  --output mobile/.build/recordings/history-newest.mov
```

The simulator must match exactly one available iPhone UDID or name. The default
run uses `build-for-testing` once and then `test-without-building`; pass
`--reuse-build` to reuse derived data. The command returns the Xcode test status
and writes `history-newest.log` and `history-newest.xcresult` beside the video.
Use the same `--derived-data` directory for the first build and each reuse.
Test identifiers must resolve to one method in the target’s local Swift source.
Recording failures return nonzero; a failing test retains its own exit status.
Videos, logs, and xcresults are retained on failure and never overwritten.

## Evidence and exit status

Each run emits:

- `verify.json`, the complete machine-readable report;
- `verify.junit.xml`, JUnit-compatible suite and case results;
- `summary.txt`, a concise human-readable summary; and
- `logs/<gate-id>.log`, preserving command output and gate reasons.

The JSON report separates `passed`, `failed`, `skipped`, `unavailable`, and
`not_automatable` counts. A required automated failure, required unavailable
dependency or device, or required blocked skip sets `blocking` and returns exit
status 1. Physical iPhone checks are recorded as required
`not_automatable` gates, so they remain visible without being claimed as
automated passes or causing a simulator-only run to fail.
