# parish-cli — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Category | Description |
|----|----------|-------------|
| TD-001 | Config/Cargo | Removed unused `thiserror` from `Cargo.toml:20`. |
| TD-013 | Dead Code | Removed `ScrollState` struct, its methods, and all associated tests from `src/app.rs`. |
| TD-014 | Dead Code | Added `#[deprecated]` to `find_data_dir` and `find_ui_dist_dir` in `src/main.rs` with `#[allow(deprecated)]` at call sites. |
| TD-016 | Stale Docs | Fixed doc comment in `src/emitter.rs:15`: `parish_cli` → `parish`. |
| TD-017 | Stale Docs | Fixed `strength_bar` doc comment in `src/debug.rs:497` to match implementation. |
| TD-018 | Weak Tests | Refactored `StdoutEmitter` to expose `format_event()` for direct testing of content-extraction logic; added 5 new assertions. |
| TD-019 | Stale Docs | Updated `too_many_arguments` comment in `src/headless.rs` to reference TODO.md; removed stale `#future` reference. |
| TD-015 | Config/Cargo | Removed CWD-relative fallback from `load_toml` in `src/config.rs`; now requires an explicit config path per Rule 9. |
| TD-002 | Complexity | Extracted `print_startup_header`, `setup_inference_queue`, and `run_headless_repl_loop` from `run_headless` (525→97 lines). REPL loop body now delegates to `dispatch_headless_*` helpers. |
| TD-003 | Complexity | Extracted `stream_headless_npc_dialogue` from `handle_headless_game_input` (237→81 lines). Streaming, loading animation, memory pipeline, and witness recording moved to the new helper. |
| TD-004 | Complexity | Split `resolve_category_configs` into 4 functions: outer loop (21 lines), `category_toml_override`, `category_has_overrides`, and `resolve_single_category` (130 lines with layered config). |
| TD-005 | Complexity | Introduced `CategoryOverride` struct to replace repeated 5-field pattern. Removed 12 fields from `App` (74→62) by collapsing intent/simulation/reaction fields into `CategoryOverride` instances. All getter/setter methods updated to use `category_override` helper. |
| TD-006 | Complexity | Refactored `build_cli_category_overrides` to use tuple iteration (33→17 lines). |
| TD-007 | Duplication | Created shared `process_schedule_events_generic` returning `Vec<String>`. Both headless `println!` and testing `world.log()` callers iterate the returned messages. |
| TD-008 | Duplication | `snapshot_config`/`apply_config` indirectly simplified via TD-005 (getter/setter methods now use `category_override` helper), keeping the per-category iteration concise. |
| TD-009 | Duplication | 10 getter/setter methods now delegate to `category_override` / `category_override_mut` helpers, eliminating repeated `match cat { Dialogue=>..., Simulation=>..., ...}` within each method body. |
| TD-010 | Duplication | Extracted `load_and_restore_snapshot` (28 lines) shared by `restore_from_db` and `handle_headless_load` named-branch path. Removes inline duplicate of snapshot-load + replay + tier-assign. |
| TD-011 | Duplication | Extracted `dispatch_headless_tier4_tick`, `dispatch_headless_tier3_tick`, and `dispatch_headless_tier2_tick` from the REPL loop, plus `dispatch_headless_weather`, `dispatch_headless_banshee`, and `dispatch_headless_autosave`. |
| TD-012 | Complexity | Extracted `setup_tracing_and_otel`, `resolve_configs`, and `load_game_mod` from `main()` (172→40 lines). `resolve_configs` returns a single `ResolvedConfigs` struct. |
