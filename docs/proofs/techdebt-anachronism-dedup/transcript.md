# Phase 2.1: AnachronismEntry dedup (parish-types TD-002)

## Change

Removed the duplicate `AnachronismEntry` struct from `parish-core/src/game_mod.rs` and made `parish-core` import `AnachronismEntry` from `parish-types` instead.

## Files changed

- `parish/crates/parish-core/src/game_mod.rs` — removed `AnachronismEntry` struct definition (20 lines), added `use parish_types::AnachronismEntry;` import
- `parish/crates/parish-types/TODO.md` — updated TD-002 entry to reflect full resolution, removed Follow-up section

## Why

TD-002 was split into two parts: (1) add `Serialize` to `parish-types::AnachronismEntry` (done in PR #911), and (2) remove the identical copy in `parish-core` that existed only because `parish-types` didn't yet derive `Serialize`. Now that both crates' versions are identical (both derive `Serialize + Deserialize`), the `parish-core` copy is pure dead-code duplication.

## Commands run

```sh
cargo test -p parish-core -p parish-types       # 116 passed, 0 failed
cargo clippy -p parish-core -p parish-types      # clean, -D warnings
cargo check -p parish -p parish-server           # downstream crates compile
cargo check -p parish-tauri                      # Tauri binary compiles
```

## Test output

All 116 tests pass. Zero clippy warnings. All downstream crates compile cleanly.
