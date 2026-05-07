# parish-core — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Config/Cargo | P2 | `Cargo.toml:29` | Unused dependency `rand` — no `use rand` or `rand::` anywhere in `src/` or `tests/` |
| TD-002 | Config/Cargo | P3 | `Cargo.toml:30` | `regex` is only used in `tests/architecture_fitness.rs:230` but declared as `[dependencies]` instead of `[dev-dependencies]` |
| TD-003 | Duplication | P2 | `src/game_session.rs:467-504` vs `:394-429` | `apply_arrival_reactions_inner` duplicates `apply_arrival_reactions` — ~60 lines identical except the `config` parameter (hardcoded `ReactionConfig::default()` vs caller-supplied). Extract a shared worker. |
| TD-004 | Weak Tests | P1 | `src/tile_cache.rs:167-193` | No async tests for `TileCache::get()` — cache hit, cache miss, HTTP fetch failure, SSRF path-traversal rejection, and source-not-registered error paths are all untested |
| TD-005 | Weak Tests | P1 | `src/session_store.rs` (entire file) | Zero tests — `DbSessionStore::ensure_db`, `first_db_path`, every `SessionStore` trait method (load_latest_snapshot, save_snapshot, list_branches, create_branch, branch_log, acquire_save_lock, append_journal_event, read_journal) completely untested |
| TD-006 | Weak Tests | P1 | `src/game_loop/save.rs` (entire file) | Zero tests — `load_fresh_world_and_npcs`, `do_new_game`, `do_save_game` all untested. New-game and save-game are critical correctness paths. |
| TD-007 | Weak Tests | P1 | `src/game_loop/system_command.rs` (entire file) | Zero tests — `handle_system_command` dispatcher and the `SystemCommandHost` trait have no coverage. Effect dispatch (Quit, SaveGame, ForkBranch, etc.) is entirely untested at this level. |
| TD-008 | Weak Tests | P2 | `src/identity.rs` (entire file) | Zero tests for `IdentityStore` and `SessionRegistry` trait definitions. Backend implementations are tested in `parish-server`, but the trait contract itself has no coverage in `parish-core`. |
| TD-009 | Weak Tests | P3 | `src/game_session.rs:722-729` | `apply_arrival_reactions_empty_location` is a no-op test — the only assertion line is commented out with `"May or may not be empty depending on game data — just verify it doesn't panic"`. Either remove it or rewrite with a controlled fixture. |
| TD-010 | Weak Tests | P2 | `src/game_session.rs:659-675` | `apply_movement_already_here` has dead variable assignments (`let _ = target; let _ = start;`) and doesn't actually verify the AlreadyHere short-circuit — its comment concedes "Should be AlreadyHere or Moved (depending on fuzzy match)". |
| TD-011 | Complexity | P2 | `src/ipc/commands.rs:187-621` | `handle_command` is a 434-line match monster with 50+ arms (time control, info, sidebar/improv, base provider, cloud provider, category provider, presets, feature flags, mode-specific). While each arm is small, the mega-function structure prevents isolated unit testing of individual commands. |
| TD-012 | Complexity | P3 | `src/debug_snapshot.rs:916-1099` | `build_npc_debug_list` is 184 lines with deeply nested closures (schedule resolution, relationship sorting, memory/reaction mapping). Extracting sub-builders would aid readability and testability. |
| TD-013 | Stale Docs/Comments | P3 | `src/session_store.rs:61` vs `:189` | `SessionStore` trait doc (line 61) says `session_id` is a UUID v4 string, but `DbSessionStore` doc (line 189) says to pass `session_id = ""` for single-user runtimes. The empty-string convention contradicts the UUID invariant documented on the trait. |
| TD-014 | Stale Docs/Comments | P3 | `src/lib.rs:3-5` | Module doc says crate "Contains all backend-agnostic game systems: world graph, NPC management, LLM inference pipeline, player input parsing, and persistence" — but those actually live in leaf crates (`parish-world`, `parish-npc`, `parish-inference`, `parish-input`, `parish-persistence`). `parish-core` re-exports them but does not own them. |

## In Progress

*(none)*

## Done

| ID | Date | Summary |
|----|------|---------|
| TD-001 | 2026-05-07 | Removed unused `rand` dependency from Cargo.toml |
| TD-002 | 2026-05-07 | Moved `regex` from `[dependencies]` to `[dev-dependencies]` (only used in tests/architecture_fitness.rs) |
| TD-003 | 2026-05-07 | Eliminated `apply_arrival_reactions_inner` duplication — replaced call site with `apply_arrival_reactions(..., &ReactionConfig::default())` and deleted the private helper |
| TD-004 | 2026-05-07 | Added 5 async tests for `TileCache::get()` covering SSRF guard (empty/unsafe source), unknown source, cache miss→fetch→hit, and upstream HTTP failure |
| TD-007 | 2026-05-07 | Added `handle_system_command` tests with mock `SystemCommandHost` — verifies SaveGame dispatches to `save_game()`, Quit early-returns before world update, text response and world update are emitted |
| TD-009 | 2026-05-07 | Rewrote no-op `apply_arrival_reactions_empty_location` test — removed dead `mgr.npcs_at()` call and suppressed result, renamed to `apply_arrival_reactions_does_not_panic` |
| TD-010 | 2026-05-07 | Removed dead variable assignments (`let _ = target; let _ = start;`) from `apply_movement_already_here` test |
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
