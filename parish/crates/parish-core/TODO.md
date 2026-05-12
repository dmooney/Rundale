# parish-core — Technical Debt

## Open

*(none — all items resolved)*

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
| TD-011 | 2026-05-07 | Extracted 8 sub-functions from 434-line `handle_command` match: `handle_time_control_command`, `handle_info_command`, `handle_sidebar_improv_command`, `handle_provider_command`, `handle_cloud_provider_command`, `handle_category_provider_command`, `handle_preset_command`, `handle_flag_command`, `handle_theme_command`. Match reduced to dispatch calls grouped by category. |
| TD-012 | 2026-05-07 | Extracted 6 sub-builders from 184-line `build_npc_debug_list`: `build_npc_schedule_debug`, `build_npc_relationship_debug`, `build_npc_memory_debug`, `build_npc_long_term_memory_debug`, `build_npc_reaction_debug`, `build_npc_deflated_summary_debug`. |
| TD-013 | 2026-05-07 | Updated `SessionStore` trait doc to acknowledge single-user `session_id = ""` convention alongside multi-user UUID v4 convention |
| TD-014 | 2026-05-07 | Updated `lib.rs` module doc to accurately describe parish-core as orchestration layer that composes leaf crates, not the owner of leaf-crate systems |
| TD-015 | 2026-05-12 | Deleted dead `interpolate_template` public API and its 4 tests from `game_mod.rs` |
| TD-016 | 2026-05-12 | Updated stale test comment `kilteevan mod` -> `rundale mod` in `game_mod.rs` |
| TD-017 | 2026-05-12 | Extracted `weekday_name` helper in `ipc/handlers.rs`, deduplicated from `snapshot_from_world` and `build_clock_debug` |
| TD-018 | 2026-05-12 | Moved `peek_mod_id` into `Err` arm in `load_setting_mod_sync`, eliminating `let _ = setting_id;` suppression |
| TD-019 | 2026-05-12 | Dropped dead `_transport` parameter from `snapshot_from_world`; updated all cross-crate call sites in `parish-tauri`, `parish-server`, and internal callers |
| TD-020 | 2026-05-12 | Fixed `prepare_npc_conversation` doc to describe it as active single-target convenience wrapper used by headless CLI |
| TD-021 | 2026-05-12 | Extracted `run_autonomous_chain` helper from Phase 2 chain in `npc_turn.rs`, replacing duplicated loop in `run_idle_banter` |
| TD-022 | 2026-05-12 | Added Rule 9 warning docs to `find_mods_root` and `LocalDiskModSource::new`, noting cwd-walk is dev fallback only |

## Discovery note

Discovery scan of `parish/crates/parish-core/src/` found no additional credible debt beyond the items already tracked.
