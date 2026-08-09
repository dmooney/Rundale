# parish-scenario — agent scope

Developer tool and library for deterministic, versioned gameplay scenarios.
It depends on `parish-engine` solely for `GameTestHarness` state construction,
but every scenario step must call `execute_via_real_loop`; never add a legacy
`GameTestHarness::execute` fallback.

Only inference may be mocked. Assertions must inspect emitted production IPC
events or post-step game state and failures must appear in the JSON report and
the process exit code.

```sh
cargo test -p parish-scenario
cargo run -p parish-scenario -- testing/scenarios/real-loop-smoke.yaml
```
