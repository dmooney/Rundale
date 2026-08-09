# Asserted gameplay scenarios

These versioned YAML files are regression tests, not demonstration scripts.
Every step runs through `parish_core::game_loop` via `parish-scenario`, mocks
only inference, and must contain a machine-checkable `expect` block. The Rust
test in `parish-scenario/tests/scenarios.rs` discovers and runs every `*.yaml`
file in this directory.

Run the suite from `parish/`:

```sh
cargo test -p parish-scenario
cargo run -p parish-scenario -- testing/scenarios/real-loop-smoke.yaml
```

Supported event assertions are `name`, payload `contains`, and an exact
`json_pointer` + `equals` pair. State assertions currently include the exact
post-step `location`. `min_events` and `absent_events` make silent/no-op and
panic-marker failures explicit.

Legacy `testing/fixtures/test_*.txt` scripts remain compatibility tests while
they are migrated. One-off `play_*.txt` proof scripts live in
`testing/proofs/` and are not swept by CI as regressions.
