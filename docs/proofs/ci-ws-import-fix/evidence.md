# CI Fix: ws_integration.rs imports

Evidence type: gameplay transcript

## Problem

`cargo check --workspace --all-targets` failed on main with:
```
error[E0432]: unresolved imports `parish_server::ws::WsValidation`, `parish_server::ws::validate_ws_upgrade`
```

The integration test `ws_integration.rs` was added but the corresponding public API
(`WsValidation`, `validate_ws_upgrade`) was never extracted from `ws_handler`'s inline logic.

## Fix

Extracted `validate_ws_upgrade` as a public pure function and `WsValidation` as a public enum
in `parish-server/src/ws.rs`. Refactored `ws_handler` to delegate to the new function.

## Verification

```
$ cargo check --workspace --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s

$ cargo clippy --workspace --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.05s

$ cargo fmt --all --check
(no output = clean)

$ cargo test --workspace --all-targets
test result: ok. (all pass)
```

## Changed files

- `parish/crates/parish-server/src/ws.rs` — added `WsValidation` enum + `validate_ws_upgrade` fn, refactored `ws_handler`
- 14 other files — `cargo fmt` normalization only (no behavior change)
