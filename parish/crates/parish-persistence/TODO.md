# parish-persistence — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P2 | `Cargo.toml:16` | `anyhow` declared as dependency but never imported or used in any source file. Wastes compile time and bloats lockfile. |
| TD-002 | Dead Code | P2 | `Cargo.toml:17` | `thiserror` declared as dependency but never imported or used. Error conversion is hand-rolled via `IntoParishDbError` trait in `lib.rs:25-33`. |
| TD-003 | Duplication | P3 | `database.rs:229-234` and `database.rs:249-253` | Identical `BranchInfo` row-mapping closure duplicated in `find_branch` and `list_branches`. Extract a private `fn branch_info_from_row(row: &Row) -> Result<BranchInfo>` helper. |
| TD-004 | Duplication | P2 | `database.rs:405-544` | `AsyncDatabase` has 9 methods that repeat the same `Arc::clone` → `spawn_blocking` → `lock_recovered` → `map_err` pattern (each ~13 lines). A macro or generic helper would eliminate ~130 lines of boilerplate. |
| TD-005 | Duplication | P3 | `database.rs:866-888` and `database.rs:1063-1086` | `test_journal_sequence_ordering` (5 events) and `test_append_event_sequences_are_contiguous` (10 events) test the same invariant with no meaningful difference. Merge into a single parameterized test. |
| TD-006 | Weak Tests | P2 | `journal.rs:156-161` | `replay_journal` handles `WorldEvent::NpcMoved` (setting `npc.location` and `npc.state`) but no test exercises this branch. Unknown-NPC skip logic is untested for this variant specifically. |
| TD-007 | Weak Tests | P2 | `journal.rs:167-176` | `replay_journal` handles `WorldEvent::RelationshipChanged` (calling `rel.adjust_strength`) but no test exercises this branch. Both unknown-npc and missing-relationship skip paths are untested. |
| TD-008 | Weak Tests | P2 | `journal.rs:188-198` | `replay_journal` handles `WorldEvent::MemoryAdded` (constructing a `MemoryEntry` and pushing to `npc.memory`) but no test exercises this branch. Memory entry shape (timestamp, participants, location, kind) is never verified via replay. |
| TD-009 | Weak Tests | P2 | `database.rs:196-197` | `load_latest_snapshot` deserializes `world_state` JSON via `serde_json::from_str`. No test verifies that malformed/corrupt JSON produces a recoverable error rather than a panic. Backward-compat tests exist for missing fields but not for syntactically invalid JSON. |
| TD-010 | Weak Tests | P2 | `database.rs:75-78` and `database.rs:280-294` | SQLite is configured for WAL mode (concurrent reads + single writer), and `append_event` uses a subquery for atomic sequence assignment, but there is no concurrent access test — no multi-threaded test exercising simultaneous reads/writes, and no test verifying that concurrent `append_event` calls produce correct non-overlapping sequences. |
| TD-011 | Weak Tests | P2 | `snapshot.rs:380-385` | `GameSnapshot::restore` has a fallback path for `speed_factor` that does not match any `GameSpeed::ALL` preset — it constructs a custom `GameClock::with_speed`. This branch has no test. Only the ludicrous-speed preset path is tested at line 740. |
| TD-012 | Complexity | P2 | `snapshot.rs:357-461` | `GameSnapshot::restore()` is 105 lines and handles clock rehydration, graph-→legacy-locations backfill, fallback placeholder insertion, visited-location merge, edge-traversal restore, NPC rebuild, tier-tick replay, gossip/conversation/player-name restore. Should be split into 3–4 private methods. |
| TD-013 | Complexity | P2 | `journal.rs:125-227` | `replay_journal()` is 102 lines with a single large match on all `WorldEvent` variants plus post-loop `assign_tiers` call. Each non-trivial variant branch (PlayerMoved with clock+edge logic, MemoryAdded with MemoryEntry construction) could be extracted into private helper functions. |
| TD-014 | Stale Docs | P2 | `picker.rs:173-214` | `discover_saves` silently skips corrupt/unreadable save files at line 195 (`Err(_) => continue`). No `tracing::warn` or `tracing::info` log is emitted, making file corruption invisible to operators and debug hard. A warning log should be added on the `Err` path. |

## In Progress

*(none)*

## Done

*(none)*
