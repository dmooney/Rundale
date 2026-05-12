Evidence type: gameplay transcript

# Techdebt Cleanup: parish-cli

## Summary of Changes

### Fixed Items
- **TD-001** — Removed unused `thiserror` dependency from `Cargo.toml`.
- **TD-013** — Removed dead `ScrollState` struct, its methods, and 7 associated unit tests from `src/app.rs`.
- **TD-014** — Added `#[deprecated]` to `find_data_dir` and `find_ui_dist_dir` in `src/main.rs` with `#[allow(deprecated)]` at all 3 call sites.
- **TD-016** — Fixed doc comment in `src/emitter.rs`: `parish_cli` → `parish`.
- **TD-017** — Fixed `strength_bar` doc comment in `src/debug.rs` to match `#`/`.` implementation.
- **TD-018** — Refactored `StdoutEmitter` to expose `format_event()` for direct testing; added 5 new assertions covering content extraction, empty/missing content filtering, and non-text-log silencing.
- **TD-019** — Updated `#[allow(clippy::too_many_arguments)]` comment to reference TODO.md; removed stale `#future` reference.

### Remaining Open Items
Items TD-002 through TD-012 (complexity/duplication), and TD-015 (cwd-relative load_toml) remain open. These require non-trivial architectural changes (extracting shared setup structs, unifying tier dispatch, deduplicating schedule event processing) and are deferred to a future focused refactor pass.

## Test Results
```
cargo test -p parish: 154 unit tests passed, 9 eval baselines, 29 game harness,
  74 headless script, 10 persistence, 28 world graph = 304 total, 0 failed
cargo clippy -p parish -- -D warnings: 0 warnings, 0 errors
cargo fmt --check: passes
```

## Files Changed
- `parish/crates/parish-cli/Cargo.toml` — removed `thiserror`
- `parish/crates/parish-cli/src/app.rs` — removed `ScrollState`, updated `App` struct doc
- `parish/crates/parish-cli/src/main.rs` — deprecated `find_data_dir`/`find_ui_dist_dir`
- `parish/crates/parish-cli/src/emitter.rs` — added `format_event()`, updated tests
- `parish/crates/parish-cli/src/debug.rs` — fixed doc comment
- `parish/crates/parish-cli/src/headless.rs` — updated comment
- `parish/crates/parish-cli/TODO.md` — moved 7 items to Done
