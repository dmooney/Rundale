# parish-engine — Technical Debt

## Open

| ID | Category | Description |
|----|----------|-------------|
| TD-020 | Config/Cargo | Remove unused `tokio-test` dev-dep at `Cargo.toml:33`. No `tokio_test` import or `tokio_test::` macro is used anywhere in `src/` or `tests/`; the existing async tests use `tokio::test` (the `tokio` proc-macro), which is unrelated. |
| TD-021 | CLAUDE.md Rule 9 | `src/main.rs:194` calls `std::env::current_dir()` inside `resolve_configs()` to derive `engine_config_path`. This is the same daemonised/`/tmp` failure mode that already led to deprecating `find_data_dir`/`find_ui_dist_dir` (TD-014). Resolve `engine_config_path` from an explicit CLI flag/env var or a startup-time picker, not from the per-call cwd. |
| TD-022 | Stale Code | `src/testing.rs:892` and `src/testing.rs:904` fall back to `data/parish.json` and `data/npcs.json` inside `handle_new_game_effect`. Game content has moved under `mods/rundale/` (CLAUDE.md "Rundale game content" map); the legacy `data/` paths cannot exist in a fresh checkout. Drop the dead fallback or replace it with a clear error. |
| TD-023 | Stale Docs | `src/command_host.rs:13-17` documents the move-back idiom as `std::mem::replace(app, App::new())` + `Arc::try_unwrap(app_arc).expect("no clone").into_inner()`. The real call site at `src/headless.rs:416,427-429` uses `std::mem::take(app)` and `Arc::into_inner(app_arc).expect(...).into_inner()`. Update the doc example to match. |
| TD-024 | Bug Risk | `src/debug.rs:359` sorts relationships with `b.1.strength.partial_cmp(&a.1.strength).unwrap()`. `Relationship::strength` is `f64`; a `NaN` value (e.g. from a corrupt save) panics the entire `/debug rels <name>` command. Use `unwrap_or(std::cmp::Ordering::Equal)` or `total_cmp`. |
| TD-025 | Code Smell | `src/app.rs:209,219` panic with `panic!("Dialogue has no CategoryOverride")` from `category_override`/`category_override_mut`. The Dialogue branch is reachable through any `set_category_*` call that the public API exposes — only convention prevents misuse. Either narrow the input enum (e.g. `NonDialogueCategory`) so the panic is statically unreachable, or use `unreachable!` with a clearer rationale. |
| TD-026 | Stale Docs | `README.md` (lines 13-17) lists "Key modules" as `main`, `app`, `headless`, `config`, `debug`/`testing`, but mis-describes `app` as "top-level startup and mode routing" (it's the shared `App` state) and omits the entire `command_host`, `emitter`, and `lib` modules. Refresh the module list to match `src/lib.rs`. |
| TD-027 | Duplication | The `GameSnapshot::capture(&world, &npc_manager)` + `db.save_snapshot(branch_id, &snapshot)` pair is repeated 6+ times across `src/command_host.rs:95-96, 195-196, 217-220` and `src/testing.rs:665-666, 694-698, 737-741`, plus `src/headless.rs:391, 446-447, 504-505, 549-550, 1437-1438`. Extract a shared `capture_and_save(db, &mut app)` helper (sync + async variants) so journal-clear and `last_autosave` bookkeeping stay in one place. |
| TD-028 | Duplication | `src/headless.rs:528-560` (`handle_headless_new_game`) and `src/testing.rs:883-917` (`handle_new_game_effect`) both reload the active mod's world + NPCs and call `assign_tiers`. The mod-reload core (`world_state_from_mod` + `NpcManager::load_from_file` + `assign_tiers`) belongs in a shared helper instead of being copy-pasted across the headless and testing modes. |
| TD-029 | Complexity | `stream_headless_npc_dialogue` (`src/headless.rs:566-716`, ~150 lines) reaches brace nesting depth 9 around line 646 (`match queue.send` → `Ok(rx)` → spawned task → `match rx.await` → `Ok(response)` → `else` arm → `if let Some(meta)` → ...). Split the response-handling arm into a `apply_npc_response` helper to flatten the control flow. |
| TD-030 | Dead State | `App::idle_counter` (`src/app.rs:75,155,404`) is initialized to `0` and asserted in one test but never read or mutated anywhere. The headless idle messaging instead uses the file-scoped `HEADLESS_IDLE_COUNTER` `AtomicUsize` (`src/headless.rs:400-401, 788`). Either delete the dead `App` field or migrate the headless counter onto `App` so the state lives in one place. |

## In Progress

*(none)*

## Done

| ID | Category | Description |
|----|----------|-------------|
| TD-001 | Config/Cargo | Removed unused `thiserror` from `Cargo.toml:20`. |
| TD-013 | Dead Code | Removed `ScrollState` struct, its methods, and all associated tests from `src/app.rs`. |
| TD-014 | Dead Code | Added `#[deprecated]` to `find_data_dir` and `find_ui_dist_dir` in `src/main.rs` with `#[allow(deprecated)]` at call sites. |
| TD-016 | Stale Docs | Fixed doc comment in `src/emitter.rs:15`: `parish_engine` → `parish`. |
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
