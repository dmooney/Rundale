# Mobile verification runner

The repository entry point is `./verify`. It runs the native mobile checks
sequentially and writes evidence under `mobile/.verification/`.

## Phases

- `./verify --phase 1`, `--phase 2`, or `--phase 3` runs that implemented gate.
- `./verify` and `./verify --phase all` run Phases 1–3 and report Phases 4–6 as
  non-blocking future work.
- `./verify --phase 4` through `./verify --phase 6` report the selected phase as
  unavailable and exit nonzero until it is implemented.

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
