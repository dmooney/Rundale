# Automated iPhone acceptance

The mobile verifier keeps deterministic tests, opt-in live integration, and
human acceptance separate. Automated device checks support the Phase 1–4
requirements; they do not close the deferred human gates in this document.

## Repeatable matrix

Run from the repository root on a stable source revision:

```sh
./verify --phase all --simulator '<primary-simulator-UDID>'
./verify --phase all --simulator '<small-screen-simulator-UDID>'
./verify --phase all --device '<physical-iPhone-UDID>' \
  --development-team MBPRPZ283R --configuration Release \
  --live-endpoint --soak --performance \
  --report-dir mobile/.verification-device
```

The device run also retains the existing simulator checks. Use a specific
`--simulator` when selecting its simulator companion. Native tests use a
separate test-save directory; ordinary player saves remain authoritative.
Keep the phone connected and unlocked. Trust/pairing, Developer Mode, local
Apple signing credentials, and the ignored Firebase configuration are
prerequisites. No provider key belongs in the app or the report.

For a short wiring check, prefix the device command with
`RUNDALE_SOAK_SMOKE=1`; this never counts as a 20-minute soak.
`RUNDALE_SOAK_DURATION_SECONDS` permits an explicit duration override, recorded
in the timing receipt. Opt-in suites require `--phase 4` or `--phase all`.
A live cancellation race that skips remains incomplete evidence.

The soak is an automated workload using the production native UI, embedded
Rust and SQLite, with deterministic remote transport. Its default duration is
20 minutes. It is not a recorded human play session. Performance results from
UI automation include driver and accessibility-query overhead and are not
measurements of pure engine latency. Mock stream timing is not live network
latency. XCTest metrics are retained in the result bundle. Local-action
measurements cover a round trip (two typed commands); scrolling measurements
cover an older/newest gesture pair over the existing 180-row fixture. First-chunk and final-commit mock streaming
times are recorded separately. Launch timing attachments label the total test
wall time separately from per-launch XCTest samples.

The technical vision's approximately 100 ms feedback and 250 ms local-completion
figures are suggested targets, not accepted device budgets. Record measured
baselines and limitations; do not invent a pass threshold or approve Phase 4
performance solely from a passing XCTest measurement.

## Requirement-to-evidence map

| Requirement                            | Automated evidence                                                              | Remaining limit                                              |
| -------------------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Network failure before/during response | Phase 4 injected transport failures, retry and relaunch assertions              | Actual radio loss deferred                                   |
| Stop/retry without duplicate effects   | Phase 2/4 native tests, controller tests, opt-in live Stop                      | Live race may skip if response finishes first                |
| Background and save/draft recovery     | Phase 4 home/activate and terminate/relaunch tests                              | User force-quit and OS eviction are different events         |
| No late/duplicate committed actions    | Runtime/controller cancellation tests and native transcript identity assertions | Device tests supplement canonical runtime assertions         |
| Accessibility text sizes and controls  | Native geometry, hittability, labels, and core-loop assertions                  | VoiceOver meaning and spoken interaction deferred            |
| Keyboard/focus and scrolling           | Native composer and historical-row/newest-content tests                         | Dictation and manual text selection deferred                 |
| Sustained use and performance          | Automated soak, timings, XCTest performance/memory metrics                      | Human session and device-budget approval deferred            |
| Supported device/OS matrix             | Exact hardware/runtime metadata in receipts                                     | Simulator coverage cannot establish physical minimum support |

## Deferred by user instruction

- Recorded 20-minute human sessions on the primary and a small-screen iPhone.
- Human VoiceOver, dictation, selection, and keyboard/focus assessment.
- Actual radio-network loss and user force-quit gesture checks.
- Small-screen physical and minimum-iOS validation unless tested hardware qualifies.

Do not change the provisional iOS 17 deployment target because an iOS 17
runtime or device is unavailable. Record unavailable infrastructure separately
from failed tests and from intentionally deferred acceptance.

## Results

See [acceptance.md](acceptance.md) for dated execution results. Retain detailed
logs, screenshot attachments, metrics, and `.xcresult` bundles under ignored
`mobile/.verification*` paths. Each reported pass needs a matching receipt;
a build success alone is not a test or a TestFlight delivery.
