# parish-core — Technical Debt

## Open

*(none)*

## In Progress

*(none)*

## Done

| ID | Date | Summary |
|----|------|---------|
| TD-001 | 2026-05-07 | Removed unused `rand` dependency from Cargo.toml |
| TD-002 | 2026-05-07 | Moved `regex` from `[dependencies]` to `[dev-dependencies]` (only used in tests/architecture_fitness.rs) |
| TD-003 | 2026-05-07 | Eliminated `apply_arrival_reactions_inner` duplication — replaced call site with `apply_arrival_reactions(..., &ReactionConfig::default())` and deleted the private helper |
| TD-004 | 2026-05-07 | Added 5 async tests for `TileCache::get()` covering SSRF guard (empty/unsafe source), unknown source, cache miss→fetch→hit, and upstream HTTP failure |
| TD-005 | 2026-05-07 | Added 16 async integration tests for `DbSessionStore` in `tests/db_session_store.rs` — covers ensure_db, save/load round-trip, branch CRUD, journal append/read, acquire/release lock, save_path resolution, and single-user empty-session-id mode |
| TD-006 | 2026-05-07 | Added 11 integration tests for save.rs in `tests/save_integration.rs` — covers `load_fresh_world_and_npcs` (with/without mod, with/without NPC file), `do_new_game` (state reset, save file creation, conversation cleanup, missing-mod error), `do_save_game` (new save, existing path, multiple snapshots, auto-resolve branch) |
| TD-007 | 2026-05-07 | Added `handle_system_command` tests with mock `SystemCommandHost` — verifies SaveGame dispatches to `save_game()`, Quit early-returns before world update, text response and world update are emitted |
| TD-008 | 2026-05-07 | Added 16 contract tests for `IdentityStore` and `SessionRegistry` traits in `tests/identity_contract.rs` — covers identity link/lookup/get, multi-provider, idempotent register, touch/cleanup/evict no-panic, and session isolation |
| TD-009 | 2026-05-07 | Rewrote no-op `apply_arrival_reactions_empty_location` test — removed dead `mgr.npcs_at()` call and suppressed result, renamed to `apply_arrival_reactions_does_not_panic` |
| TD-010 | 2026-05-07 | Removed dead variable assignments (`let _ = target; let _ = start;`) from `apply_movement_already_here` test |
| TD-013 | 2026-05-07 | Updated `SessionStore` trait doc to acknowledge single-user `session_id = ""` convention alongside multi-user UUID v4 convention |
| TD-014 | 2026-05-07 | Updated `lib.rs` module doc to accurately describe parish-core as orchestration layer that composes leaf crates, not the owner of leaf-crate systems |
| TD-011 | 2026-05-07 | Extracted 8 sub-functions from 434-line `handle_command` match: `handle_time_control_command`, `handle_info_command`, `handle_sidebar_improv_command`, `handle_provider_command`, `handle_cloud_provider_command`, `handle_category_provider_command`, `handle_preset_command`, `handle_flag_command`, `handle_theme_command`. Match reduced to dispatch calls grouped by category. Exported sub-functions remain private and testable via the public `handle_command` entry point. |
| TD-012 | 2026-05-07 | Extracted 6 sub-builders from 184-line `build_npc_debug_list`: `build_npc_schedule_debug`, `build_npc_relationship_debug`, `build_npc_memory_debug`, `build_npc_long_term_memory_debug`, `build_npc_reaction_debug`, `build_npc_deflated_summary_debug`. Each is independently testable. |

## Follow-up

*(none)*

## Discovery note

Discovery scan of `parish/crates/parish-core/src/` found no additional credible debt beyond the items already tracked. The dead-code removal (TD-001, TD-010), doc fixes (TD-013, TD-014), duplication cleanup (TD-003), and test additions (TD-004, TD-007, TD-009) cover the actionable items. Remaining weak-test and complexity items are recorded as Follow-up for separate work since they require integration-level changes or carry behavioral risk.
