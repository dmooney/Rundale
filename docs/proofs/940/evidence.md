# Proof Evidence — PR #940: raise default idle-banter threshold to 2 minutes

Evidence type: gameplay transcript
Date: 2026-05-10
Branch: claude/npc-dialogue-timer-79FEX

## Requirement

Spontaneous NPC banter (`run_idle_banter`) was firing after only 25 seconds
of combined player + speech silence, which felt too chatty during normal
pauses. The PR raises the default to 120 seconds (2 minutes), preserving the
ability to override it via `parish.toml`.

The trigger site at `parish/crates/parish-server/src/routes.rs:604` is
unchanged:

```rust
if player_idle >= idle_after && speech_idle >= idle_after {
    run_idle_banter(state).await;
}
```

`idle_after` is sourced from `config.idle_banter_after_secs`, which the
server overlays from `engine_config.session.idle_banter_after_secs`
(`parish/crates/parish-server/src/lib.rs:782`). Tauri uses the same field
(`parish/crates/parish-tauri/src/lib.rs:808`). The behavioral change is
therefore entirely captured by the default value.

## Defaults updated

| File | Symbol | Before | After |
| --- | --- | ---: | ---: |
| `parish/crates/parish-config/src/engine.rs:138` | `default_idle_banter_after_secs()` | 25 | 120 |
| `parish/crates/parish-core/src/ipc/config.rs:257` | `GameConfig::default()` | 25 | 120 |
| `parish/crates/parish-cli/src/app.rs:323` | CLI hardcoded init | 25 | 120 |

Test fixtures elsewhere that pin a specific value for unrelated assertions
(`auth.rs`, `middleware.rs`, `routes.rs` tests, integration tests) are left
at their existing values — they don't exercise the default and changing
them would obscure their intent.

## parish-core: default_config test

The test in `parish-core/src/ipc/config.rs` asserts the new default value:

```rust
assert_eq!(c.idle_banter_after_secs, 120);
```

Command:

```sh
cargo test -p parish-core --lib ipc::config::tests::default_config
```

Result:

```
running 1 test
test ipc::config::tests::default_config ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 316 filtered out; finished in 0.00s
```

## parish-config full lib test suite

The TOML-deserialization tests in `parish-config/src/engine.rs` exercise the
override path (explicit `idle_banter_after_secs = 60`), which the default
change does not touch.

Command:

```sh
cargo test -p parish-config --lib
```

Result:

```
test result: ok. 87 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

## parish-core + parish-cli unit tests

Full unit suites for the two crates whose `Default` impls changed plus the
CLI binary that re-uses the value:

```sh
cargo test -p parish-config -p parish-core --lib
# test result: ok. 316 passed; 0 failed; 1 ignored; 0 measured.

cargo test -p parish --lib
# test result: ok. 147 passed; 0 failed; 0 ignored; 0 measured.
```

All pre-existing tests continue to pass; no regressions introduced by the
default-value bump.

## What the player sees

Before this change, a player who paused for ~25 s (e.g., to read the log
or check the map) would trigger spontaneous co-located NPC banter. After
this change, the same pause is silent until 2 minutes elapse with no
player input *and* no in-session speech, at which point `run_idle_banter`
fires its normal chain (initial remark + up to 3 autonomous follow-ups,
capped by `autonomous::MAX_CHAIN_TURNS`).
