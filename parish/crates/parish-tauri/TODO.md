# parish-tauri — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-001 | Dead Code | P1 | Added `get_demo_config`, `get_demo_context`, `get_llm_player_action` to `EXPECTED_COMMANDS`. Updated EXPECTED_COUNT from 29 to 32. Added compile-time symbol imports. Updated stale doc comments in `command_logic.rs` (28→32, deferred 25→29). |
| TD-002 | Duplication | P2 | Deleted `do_save_game_inner` (52 lines of reimplemented save logic). `TauriCommandHost::save_game` now delegates to `commands::do_save_game` which calls `parish_core::game_loop::do_save_game`. Removed unused `Database`, `new_save_path`, `GameSnapshot` imports. |
| TD-003 | Complexity | P2 | Extracted 8 helpers from the ~940-line `.setup()` closure into a new `crate::setup` module: `init_screenshot_mode`, `bootstrap_inference_provider`, `init_inference_queue`, `init_persistence`, `spawn_event_bus_fanin`, `spawn_world_tick`, `spawn_inactivity_tick`, `spawn_debug_tick`, `spawn_autosave_tick`. `run()` now bottoms out at sequential helper calls; `lib.rs` shrank 2170 → 1203 lines. Behaviour is byte-for-byte identical (no logic moves between phases). Mirrors the `parish-server::session::spawn_session_ticks` decomposition pattern. |
| TD-004 | Weak Tests | P1 | Added `get_world_snapshot_inner_returns_start_location` test. Updated stale doc comments in `command_logic.rs` reflecting 32 total / 29 deferred commands. |
| TD-005 | Duplication | P3 | Consolidated 5 manual `snapshot_from_world + compute_name_hints` call sites to use `get_world_snapshot_inner`. Removed unused `snapshot_from_world` and `compute_name_hints` imports from `command_host.rs`.

## Follow-up

*(none)*

## Progress Log

- **2026-05-08**: TD-003 closed. Decomposed `.setup()` into 8 helpers in new `setup.rs` module (963 lines extracted). 76 tests pass; clippy clean with `-D warnings`. Closes the final 1/2 deferred items from the techdebt sweep.
