# parish-persistence — Technical Debt

## Open

*(none — all items resolved)*

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
