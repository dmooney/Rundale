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
--no-cache
```

Set `RUNDALE_IOS_SIMULATOR` to pin the simulator used by release verification;
an explicit `--simulator` takes precedence. This avoids altering unrelated booted
simulators.

The path overrides are useful for fixtures and isolated test projects. The
simulator override accepts an available simulator UDID or name; otherwise the
runner chooses a booted, newest available iPhone simulator. The default report
directory is `mobile/.verification/`.

## Reusing passes

A suite is not rerun when an earlier run passed it with identical inputs. The
cargo, Swift package, unsigned device build, and simulator xcodebuild suites
each have a key built from:

- the working tree, hashed as a git tree in a temporary index (tracked edits
  and untracked, non-ignored files count; committing identical content keeps
  the key; the real index is untouched);
- the ignored private Firebase configuration's digest;
- `xcodebuild -version`, `swift --version`, and the pinned Rust toolchain;
- the suite's own selection: command, test targets, configuration, and the
  simulator's runtime and device type.

Documentation that no gate reads is left out of the tree hash: `docs/`,
Markdown under `mobile/` and `endpoints/`, and the root `README.md`,
`LEARNINGS.md`, and agent guides. Rust crate Markdown stays in because some of
it is compiled with `include_str!`.

Only passes are stored, under `mobile/.verification/cache/`. Failures, skips,
physical-device, soak, and performance suites always run. A reused suite is
reported as passed with `details.cache` naming the run, report, and log that
produced it, and the summary line counts reused suites. Because Phase 4 (and
`--phase all`) includes the earlier phases, it reruns only the suites whose
inputs changed. `--no-cache` reruns everything; if git or a toolchain probe is
unavailable, reuse is disabled for that run and the JSON report says why.

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

## Inspect a streamed reply frame by frame

`stream-frames.swift` reads a recording from `record-ui-test.py` and checks
that streamed text rendered before the final frame. simctl writes a frame only
when the screen changes, so frame timestamps give the rendering cadence. Vision
text recognition reads each changed frame. A frame is provisional while the
speaker's row and the busy indicator are both on screen, and final once the
indicator is gone:

```sh
swift mobile/scripts/stream-frames.swift \
  mobile/.build/recordings/live-stream.mov mobile/.build/recordings/live-stream-frames \
  --speaker "Peig Hannigan" --busy "Having a think"
```

It writes `stream-frames.json` with the provisional text states (time and
character count), how long provisional text was visible, frames per second
while waiting and while streaming, and PNGs of each provisional state and the
first final frame. It exits nonzero unless provisional text preceded the final
frame. Reply text is not stored in the JSON. Simulator cadence reflects the
simulator's compositor and recorder, not a physical display.

The live UI tests also read `uitest.streamTrace`, a UI-test-only element whose
value lists every dialogue-row state the presentation model published
(`row|state|characters|milliseconds`). It lets `test01` assert
provisional-before-committed for a reply that is provisional for only a few
hundred milliseconds, and it is available on a physical device, where simctl
recording is not.

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
