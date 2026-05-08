# parish-core — Technical Debt

## Open

*(none — every item from the 2026-05-07 inventory is either resolved or recorded in Follow-up)*

## In Progress

*(none)*

## Done

| ID | Date | Summary |
|----|------|---------|
| TD-001 | 2026-05-07 | Removed unused `rand` dependency from Cargo.toml |
| TD-002 | 2026-05-08 | Moved `regex` out of `[dependencies]` (only used in `tests/architecture_fitness.rs`); the dev-dependency entry stays |
| TD-003 | 2026-05-07 | Eliminated `apply_arrival_reactions_inner` duplication — replaced call site with `apply_arrival_reactions(..., &ReactionConfig::default())` and deleted the private helper |
| TD-004 | 2026-05-07 | Added 5 async tests for `TileCache::get()` covering SSRF guard (empty/unsafe source), unknown source, cache miss→fetch→hit, and upstream HTTP failure |
| TD-007 | 2026-05-07 | Added `handle_system_command` tests with mock `SystemCommandHost` — verifies SaveGame dispatches to `save_game()`, Quit early-returns before world update, text response and world update are emitted |
| TD-009 | 2026-05-08 | Deleted the no-op `apply_arrival_reactions_does_not_panic` test — the empty-location fast-path is properly covered by `apply_arrival_reactions_returns_empty_when_no_location_data`, which uses a controlled `WorldState::new()` fixture and asserts the returned vec is empty |
| TD-010 | 2026-05-08 | Deleted the redundant weak `apply_movement_already_here` test — `apply_movement_already_here_explicit` covers the same path with stronger assertions (player location unchanged, log appended) and an explanatory comment about the fuzzy-match harness behaviour |
| TD-013 | 2026-05-07 | Updated `SessionStore` trait doc to acknowledge single-user `session_id = ""` convention alongside multi-user UUID v4 convention |
| TD-014 | 2026-05-07 | Updated `lib.rs` module doc to accurately describe parish-core as orchestration layer that composes leaf crates, not the owner of leaf-crate systems |

## Follow-up

Items requiring significant effort or changes outside this crate — deferred for separate work:

| ID | Original | Reason |
|----|----------|--------|
| TD-005 | Weak Tests: `DbSessionStore` | Requires real SQLite databases, save files, async infrastructure. Testing would need significant `parish-persistence` integration. |
| TD-006 | Weak Tests: `save.rs` | `load_fresh_world_and_npcs`, `do_new_game`, `do_save_game` require full WorldState/NpcManager setup with real mod data. Integration-level testing that depends on fixture data. |
| TD-008 | Weak Tests: `IdentityStore`/`SessionRegistry` traits | Trait contract tests for traits whose implementations live in `parish-server`. Would need mock implementations and async harness. |
| TD-011 | Complexity: 434-line `handle_command` match | Refactoring a stable dispatch function — risk of behavioral divergence across 50+ arms. Better addressed when the next arm is added. |
| TD-012 | Complexity: 184-line `build_npc_debug_list` | Readability-only refactor. Not causing bugs. |

## Discovery note

Discovery scan of `parish/crates/parish-core/src/` found no additional credible debt beyond the items already tracked. The dead-code removal (TD-001, TD-010), doc fixes (TD-013, TD-014), duplication cleanup (TD-003), and test additions (TD-004, TD-007, TD-009) cover the actionable items. Remaining weak-test and complexity items are recorded as Follow-up for separate work since they require integration-level changes or carry behavioral risk.

## 2026-05-08 reconciliation note

The 2026-05-07 PR (#921) marked TD-001/003/013/014 done in the Done table but never pruned the Open table — the entries were stale. TD-002 was listed Done but `regex` was still in `[dependencies]`; this PR removes it. TD-009 was listed Done but the underlying test had only been renamed (`apply_arrival_reactions_empty_location` → `apply_arrival_reactions_does_not_panic`), still asserting nothing; this PR deletes it because `apply_arrival_reactions_returns_empty_when_no_location_data` supersedes it. TD-010 was listed Done but the weak `apply_movement_already_here` test still produced no useful signal; this PR deletes it because `apply_movement_already_here_explicit` covers the path more thoroughly.
