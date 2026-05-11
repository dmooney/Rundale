# parish-persistence — Technical Debt

## Open

| ID | Category | Severity | Summary |
|----|----------|----------|---------|
| TD-015 | Bug / Stale Logic | P2 | `journal_bridge::to_journal_event` (`journal_bridge.rs:41-54`) maps both `GameEvent::NpcArrived` and `GameEvent::NpcDeparted` to `WorldEvent::NpcMoved { from: location, to: location }`. The "best approximation" comment notes the data is lost; the resulting journal row is a no-op on replay (`journal.rs:183-188` sets `npc.location = to`, which equals `from`), so crash recovery silently drops every arrival/departure. Either drop the conversion (return `None`) or extend `WorldEvent` to carry the actual movement, and add a regression test. |
| TD-016 | Rule 9 Violation / API | P2 | `picker::ensure_saves_dir()` (`picker.rs:116-118`) returns a cwd-relative `PathBuf::from("saves")` and is still the entry-point used by `parish-cli/src/headless.rs:290` and `parish-server/src/lib.rs:393` (out of scope to fix, but in-scope to deprecate). Per CLAUDE.md rule 9, callers must resolve runtime paths from explicit config; mark this fn `#[deprecated(note = "use resolve_project_saves_dir")]` and/or remove it once external callers migrate. |
| TD-017 | Dead / Unused Public API | P3 | `journal_bridge::to_journal_event` and `journal_bridge::drain_events` (`journal_bridge.rs:16,67`) have no callers outside the crate's own tests (verified via `rg` across the workspace). Either wire them into the snapshot/save path or downgrade the visibility (`pub(crate)`) / delete. |
| TD-018 | Dead / Unused Public API | P3 | `picker::PickerChoice` (enum, `picker.rs:62`) and `picker::read_picker_choice` (`picker.rs:427`) are only consumed by `run_picker` inside this file. No external callers. Downgrade to `pub(crate)` (or inline) — keeping them `pub` is a misleading API surface. |
| TD-019 | Dead / Unused Public API | P3 | `lock::SaveFileLock::covers_path` (`lock.rs:207`) has no callers outside its own unit test. Either remove it or document the intended consumer. |
| TD-020 | Test Hygiene / Race | P2 | `picker::tests::test_ensure_saves_dir_creates_directory` (`picker.rs:748-757`) mutates the process-wide `current_dir()` without holding the `env_test_lock` used by sibling tests. Cargo runs unit tests in parallel threads → flaky races with any test that reads cwd or env. Either gate it under the same lock or rewrite to call `ensure_saves_dir_at(tmp.path().join("saves"))` and avoid `set_current_dir` entirely. |
| TD-021 | Inconsistent Logging | P3 | `picker::ensure_saves_dir_at` (`picker.rs:101-103`) uses `eprintln!` / `println!` for the legacy save-file migration, while every other warning in the crate uses `tracing::warn!` / `tracing::info!` (e.g. `picker.rs:89,196`, `lock.rs:154,246`, `database.rs:30`, `journal.rs:208,222`). Switch to `tracing` for consistency and so the migration message survives `--quiet` / non-tty runtimes. |
| TD-022 | Weak Tests | P3 | `journal::replay_journal` treats `WorldEvent::DialogueOccurred` as a deliberate no-op (`journal.rs:228`), but no test asserts that the world state is unchanged after replaying one. Add `test_replay_dialogue_occurred_is_noop` so the contract is locked in (otherwise a future "let's persist dialogue too" change silently regresses). |
| TD-023 | Stale Doc Comment | P3 | `picker.rs:84-86` doc-comments `ensure_saves_dir_at` as performing "the one-time migration of the legacy `parish_saves.db`" — the migration is now multi-year-old and the legacy file does not exist in fresh installs. Either delete the migration block (and shorten the doc) or move the migration code behind a feature flag/one-shot-on-upgrade marker so the cwd-side-effect doesn't run on every startup. |



## In Progress

*(none)*

## Done

| ID | Category | Severity | Summary |
|----|----------|----------|---------|
| TD-001 | Dead Code | P2 | Removed unused `anyhow` dependency from `Cargo.toml`. |
| TD-002 | Dead Code | P2 | Removed unused `thiserror` dependency from `Cargo.toml`. |
| TD-003 | Duplication | P3 | Extracted `branch_info_from_row` helper in `database.rs`; used by both `find_branch` and `list_branches`. |
| TD-004 | Duplication | P2 | Added `run_blocking` generic helper on `AsyncDatabase`, eliminating the repeated `Arc::clone` → `spawn_blocking` → `lock_recovered` → `map_err` pattern from all 9 methods (~130 lines saved). |
| TD-005 | Duplication | P3 | Merged `test_journal_sequence_ordering` and `test_append_event_sequences_are_contiguous`; kept `test_journal_sequences_are_contiguous`. |
| TD-006 | Weak Tests | P2 | Added `test_replay_npc_moved_updates_location_and_state` and `test_replay_npc_moved_unknown_npc_skipped` in `journal.rs`. |
| TD-007 | Weak Tests | P2 | Added `test_replay_relationship_changed_adjusts_strength`, `test_replay_relationship_changed_unknown_npc_skipped`, and `test_replay_relationship_changed_missing_relationship_skipped` in `journal.rs`. |
| TD-008 | Weak Tests | P2 | Added `test_replay_memory_added_creates_entry` and `test_replay_memory_added_unknown_npc_skipped` in `journal.rs`. |
| TD-009 | Weak Tests | P2 | Added `test_corrupt_world_state_json_is_recoverable` in `database.rs`. |
| TD-010 | Weak Tests | P2 | Added `test_concurrent_append_events_produce_correct_sequences` (4 tasks x 25 events) in `database.rs`. |
| TD-011 | Weak Tests | P2 | Added `test_restore_custom_speed_factor_fallback` (speed=100.0, no preset match) in `snapshot.rs`. |
| TD-012 | Complexity | P2 | Split `GameSnapshot::restore()` into `restore_clock`, `restore_world_locations`, `restore_npcs` private helpers. |
| TD-013 | Complexity | P2 | Extracted `apply_player_moved` and `apply_memory_added` helpers from `replay_journal`. |
| TD-014 | Stale Docs | P2 | Added `tracing::warn!` on the `Err(_) => continue` path in `discover_saves` in `picker.rs`. |
