Evidence type: gameplay transcript

# TD-006: save.rs integration tests

## What

Added 11 integration tests for `parish-core/src/game_loop/save.rs` covering all three public functions.

## Why

`load_fresh_world_and_npcs`, `do_new_game`, and `do_save_game` had zero test coverage despite being critical correctness paths (new-game and save-game orchestration).

## Changes

- **`parish/crates/parish-core/tests/save_integration.rs`** (new file) — 11 tests:
  - `load_fresh_world_and_npcs_with_mod_loads_world_and_npcs` — loads real Rundale GameMod, verifies WorldState at LocationId(15) and populated NpcManager
  - `load_fresh_world_and_npcs_with_mod_loads_rundale_world_graph` — verifies graph size and start location name "Kilteevan Village"
  - `load_fresh_world_and_npcs_without_mod_uses_data_dir` — loads from world.json in data_dir when no mod given
  - `load_fresh_world_and_npcs_without_mod_returns_empty_npcs_when_file_missing` — copies world.json to temp dir without npcs.json, verifies empty NpcManager fallback
  - `do_new_game_creates_save_file_and_updates_state` — full round-trip: creates save file, sets branch id/name, replaces world/NPC state
  - `do_new_game_resets_conversation_state` — verifies conversation location and transcript are cleared
  - `do_new_game_without_mod_fallback_to_data_dir_errors` — verifies error when no mod and no data files present
  - `do_save_game_without_existing_path_creates_new_save` — creates new save file when none exists
  - `do_save_game_with_existing_path_writes_snapshot` — writes to pre-existing db file
  - `do_save_game_multiple_saves_accumulate_snapshots` — verifies two saves produce two snapshots
  - `do_save_game_without_existing_branch_auto_resolves_main` — auto-finds main branch when branch_id not set

- **`parish/crates/parish-core/TODO.md`** — moved TD-006 to Done, removed from Follow-up

## Commands Run

```
cargo test -p parish-core --test save_integration
cargo test -p parish-core -p parish-world -p parish-npc
cargo clippy -p parish-core -p parish-world -p parish-npc --all-targets -- -D warnings
```

## Test Results

- 11/11 save integration tests passed
- 152/152 crate tests passed
- Clippy clean (0 warnings)
