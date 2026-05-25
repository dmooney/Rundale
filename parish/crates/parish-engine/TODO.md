# parish-engine — Technical Debt

## Open

| ID | Category | Description |
|----|----------|-------------|
| TD-019 | Stale Docs | Reopened: `src/headless.rs:190` still says the `too_many_arguments` allow is tracked in `parish-cli/TODO.md`, even though this file previously marked TD-019 done. Update the inline comment to `parish-engine/TODO.md` or drop the tracker reference once the argument-list rationale is documented. |
| TD-020 | Config/Cargo | Remove unused `tokio-test` dev-dep at `Cargo.toml:34`. No `tokio_test` import or `tokio_test::` macro is used anywhere in `src/` or `tests/`; the existing async tests use `tokio::test` (the `tokio` proc-macro), which is unrelated. |
| TD-021 | CLAUDE.md Rule 9 | `src/main.rs:191` calls `std::env::current_dir()` inside `resolve_configs()` to derive `engine_config_path`. This is the same daemonised/`/tmp` failure mode that already led to deprecating `find_data_dir`/`find_ui_dist_dir` (TD-014). Resolve `engine_config_path` from an explicit CLI flag/env var or a startup-time picker, not from the per-call cwd. |
| TD-022 | Stale Code | `src/testing.rs:1037` and `src/testing.rs:1049` fall back to `data/parish.json` and `data/npcs.json` inside `handle_new_game_effect`. Game content has moved under `mods/rundale/` (CLAUDE.md "Rundale game content" map); the legacy `data/` paths cannot exist in a fresh checkout. Drop the dead fallback or replace it with a clear error. |
| TD-023 | Stale Docs | `src/command_host.rs:13-17` documents the move-back idiom as `std::mem::replace(app, App::new())` + `Arc::try_unwrap(app_arc).expect("no clone").into_inner()`. The real call site at `src/headless.rs:599,610-612` uses `std::mem::take(app)` and `Arc::into_inner(app_arc).expect(...).into_inner()`. Update the doc example to match. |
| TD-024 | Bug Risk | `src/debug.rs:377` sorts relationships with `b.1.strength.partial_cmp(&a.1.strength).unwrap()`. `Relationship::strength` is `f64`; a `NaN` value (e.g. from a corrupt save) panics the entire `/debug rels <name>` command. Use `unwrap_or(std::cmp::Ordering::Equal)` or `total_cmp`. |
| TD-025 | Code Smell | `src/app.rs:310,320` panic with `panic!("Dialogue has no CategoryOverride")` from `category_override`/`category_override_mut`. The Dialogue branch is reachable through any `set_category_*` call that the public API exposes — only convention prevents misuse. Either narrow the input enum (e.g. `NonDialogueCategory`) so the panic is statically unreachable, or use `unreachable!` with a clearer rationale. |
| TD-026 | Stale Docs | `README.md` (lines 13-17) lists "Key modules" as `main`, `app`, `headless`, `config`, `debug`/`testing`, but mis-describes `app` as "top-level startup and mode routing" (it's the shared `App` state) and omits the entire `command_host`, `emitter`, and `lib` modules. Refresh the module list to match `src/lib.rs`. |
| TD-027 | Duplication | The `GameSnapshot::capture(&world, &npc_manager)` + `db.save_snapshot(branch_id, &snapshot)` pair is repeated 6+ times across `src/command_host.rs:97-98, 199-200, 221-224` and `src/testing.rs:809-811, 838-843, 882-886`, plus `src/headless.rs:636-638, 694-696, 740-741, 1669-1670`. Extract a shared `capture_and_save(db, &mut app)` helper (sync + async variants) so journal-clear and `last_autosave` bookkeeping stay in one place. |
| TD-028 | Duplication | `src/headless.rs:719-736` (`handle_headless_new_game`) and `src/testing.rs:1028-1057` (`handle_new_game_effect`) both reload the active mod's world + NPCs and call `assign_tiers`. The mod-reload core (`world_state_from_mod` + `NpcManager::load_from_file` + `assign_tiers`) belongs in a shared helper instead of being copy-pasted across the headless and testing modes. |
| TD-029 | Complexity | `stream_headless_npc_dialogue` (`src/headless.rs:757-907`, ~150 lines) reaches deep nesting around `src/headless.rs:800-888` (`match queue.send` → `Ok(rx)` → spawned task → `match rx.await` → `Ok(response)` → `else` arm → `if let Some(meta)` → ...). Split the response-handling arm into a `apply_npc_response` helper to flatten the control flow. |
| TD-030 | Dead State | `App::idle_counter` (`src/app.rs:78,192,502`) is initialized to `0` and asserted in one test but never read or mutated anywhere. The headless idle messaging instead uses the file-scoped `HEADLESS_IDLE_COUNTER` `AtomicUsize` (`src/headless.rs:507,979`). Either delete the dead `App` field or migrate the headless counter onto `App` so the state lives in one place. |
| TD-031 | CLI/Mode Parity | `src/main.rs:239-240` returns directly to `testing::run_script_mode()` before `load_game_mod()` runs, so `--game-mod` / `PARISH_MOD` are ignored in `--script` mode. `src/testing.rs:1678-1682` then builds the harness from `mods/mod-list.toml` instead of the explicit CLI mod. Pass the resolved mod into script mode or move script dispatch after mod resolution. |
| TD-032 | Runtime Paths | `setup_tracing()` writes to cwd-relative `logs/` (`src/main.rs:115-116`). This has the same packaged/daemon `/tmp` failure mode as the deprecated cwd path probes; resolve the log directory from explicit config or the platform user-data root instead. |
| TD-033 | Observability Bug Risk | `setup_tracing()` drops the `tracing_appender::non_blocking` `WorkerGuard` when the function returns (`src/main.rs:117`), so file logging may stop or fail to flush reliably. Return the guard to `main()` or store it for the process lifetime. |
| TD-034 | Dead State / Stale Docs | `App` still carries UI/TUI-era fields with no repo references beyond initialization/tests (`src/app.rs:52,60,68-74,78,102`), while its doc comment says it is shared with Tauri (`src/app.rs:44-47`) even though Tauri has its own `AppState`. Audit `input_buffer`, `sidebar_visible`, `debug_*`, `idle_counter`, and `loading_animation`; delete dead fields or move real CLI-only state into a narrower type. |
| TD-035 | Stale Rename References | Rename leftovers still refer to the old `parish-cli` / `parish` package identity inside this crate: `src/config.rs:381`, `src/headless.rs:190`, `tests/eval_baselines.rs:384`, `README.md:1`, and `README.md:7`. Refresh these alongside TD-026 so the crate docs match `Cargo.toml` (`package = "parish-engine"`, binary `parish-engine`). |

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
