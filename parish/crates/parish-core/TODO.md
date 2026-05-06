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

*(none)*
