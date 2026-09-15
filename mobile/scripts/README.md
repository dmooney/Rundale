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

- `./verify --phase 1` through `--phase 5` runs the selected implemented gate.
- `./verify` and `./verify --phase all` run Phases 1–5 and report Phase 6 as
  non-blocking future work.
- `./verify --phase 6` reports the selected phase as unavailable and exits
  nonzero until it is implemented.

Phase 4 includes the earlier regression suites and native lifecycle/connectivity
recovery tests. Its physical sessions, accessibility judgment, and performance
budgets remain separate recorded gates. Simulator suites share compiled products
within each run while retaining separate result bundles.

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
