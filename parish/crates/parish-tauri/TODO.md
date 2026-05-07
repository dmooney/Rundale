# parish-tauri — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-003 | Complexity | P2 | `src/lib.rs:549-1894` (~1345 lines) | `run()` function is oversized. The `.setup()` closure alone spans lines 880–1881 (~940 lines) and contains inline: provider bootstrap, persistence init (autoload/create/locked-handling), 5 background tick loops (idle, debug, autosave, game event bus fan-in, main game tick), plus screenshot mode orchestration. No sub-functions extracted. Skipped per "prefer deleting dead code over refactoring" — no dead code to prune. |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Description |
|----|----------|----------|-------------|
| TD-001 | Dead Code | P1 | Added `get_demo_config`, `get_demo_context`, `get_llm_player_action` to `EXPECTED_COMMANDS`. Updated EXPECTED_COUNT from 29 to 32. Added compile-time symbol imports. Updated stale doc comments in `command_logic.rs` (28→32, deferred 25→29). |
| TD-002 | Duplication | P2 | Deleted `do_save_game_inner` (52 lines of reimplemented save logic). `TauriCommandHost::save_game` now delegates to `commands::do_save_game` which calls `parish_core::game_loop::do_save_game`. Removed unused `Database`, `new_save_path`, `GameSnapshot` imports. |
| TD-004 | Weak Tests | P1 | Added `get_world_snapshot_inner_returns_start_location` test. Updated stale doc comments in `command_logic.rs` reflecting 32 total / 29 deferred commands. |
| TD-005 | Duplication | P3 | Consolidated 5 manual `snapshot_from_world + compute_name_hints` call sites to use `get_world_snapshot_inner`. Removed unused `snapshot_from_world` and `compute_name_hints` imports from `command_host.rs`.

## Follow-up

| Item | Severity | Description |
|------|----------|-------------|
| TD-003 | (deferred) | `run()` function complexity — ~940-line setup closure. No dead code found to prune. Would require extracting sub-functions for: provider bootstrap, persistence init, background tick loops, screenshot orchestration. Low risk of regression if extracted carefully, but pure refactor work. |
