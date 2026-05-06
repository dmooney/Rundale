# parish-tauri — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P1 | `src/command_registry.rs:11-42`, `../tests/command_registry.rs:43` | Command registry is stale. `EXPECTED_COMMANDS` lists 29 names but `lib.rs:846-879` registers 32 commands. Missing: `get_demo_config`, `get_demo_context`, `get_llm_player_action` (added at `commands.rs:1401,1420,1625`). The count test at `tests/command_registry.rs:43` hardcodes `EXPECTED_COUNT: usize = 29`, so it passes despite the drift. Compile-time symbol imports at `tests/command_registry.rs:28-38` also omit the 3 demo functions, meaning rename/removal of these commands is invisible to CI. |
| TD-002 | Duplication | P2 | `src/command_host.rs:230-281`, `src/commands.rs:922-932` | Two divergent save implementations in the same crate. `commands.rs:922` `do_save_game` delegates to `parish_core::game_loop::do_save_game` (the canonical core path, #696 slice 6). `command_host.rs:230` `do_save_game_inner` reimplements the entire save operation (snapshot capture, DB open, `save_snapshot`, branch-id resolution, status message formatting) from scratch. Both are called from different code paths (`TauriCommandHost::save_game` vs `save_game` Tauri command) but perform the same logical operation. |
| TD-003 | Complexity | P2 | `src/lib.rs:549-1894` (~1345 lines) | `run()` function is oversized. The `.setup()` closure alone spans lines 880–1881 (~940 lines) and contains inline: provider bootstrap, persistence init (autoload/create/locked-handling), 5 background tick loops (idle, debug, autosave, game event bus fan-in, main game tick), plus screenshot mode orchestration. No sub-functions extracted; the entire startup lifecycle is a single monolithic closure. |
| TD-004 | Weak Tests | P1 | `src/commands.rs`, `../tests/command_logic.rs:18-29`, `../tests/command_registry.rs` | Missing behavioral test coverage on Tauri command handlers. The `command_logic.rs` test file explicitly documents "Commands deferred (25 of 28)" — this caption is itself stale (now 29 of 32). Only `submit_input` validation, `react_to_message` emoji/snippet guards, and editor `handle_editor_update_*` validations have behavioral tests. Untested commands include: `get_world_snapshot`, `get_map`, `get_npcs_here`, `get_theme`, `get_ui_config`, `get_debug_snapshot`, `get_setup_snapshot`, `discover_save_files`, `save_game`, `load_branch`, `create_branch`, `new_save_file`, `new_game`, `get_save_state`, `get_demo_config`, `get_demo_context`, `get_llm_player_action`. The `command_registry.rs` test only checks compile-time symbol presence and name well-formedness — no runtime behavior. |
| TD-005 | Duplication | P3 | `src/commands.rs:91-93`, `366-368`, `717-719`, `990-992`; `src/command_host.rs:219-222` | The 3-line pattern `snapshot_from_world + compute_name_hints` is repeated verbatim in 5 functions across `commands.rs` and `command_host.rs`, while a shared helper `get_world_snapshot_inner` (`commands.rs:41-52`) already encapsulates this exact pattern and is used by the background tick (`lib.rs:1216`) and `editor_commands.rs:148`. The 5 manual call sites could delegate to the helper. |

## In Progress

*(none)*

## Done

*(none)*
