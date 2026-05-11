# parish-core — Technical Debt

## Open

| ID | Date | Summary |
|----|------|---------|
| TD-015 | 2026-05-09 | Dead public API: `interpolate_template` at `src/game_mod.rs:654` has zero callers outside its own unit tests anywhere in the workspace (verified via `grep -rn` across `parish/crates/`). The `{key}` single-brace syntax is also distinct from `prompts::substitute`'s `{{key}}`, so it's not even a candidate for consolidation. Delete the function and its four `test_interpolate_template*` tests at `src/game_mod.rs:1113-1136`. |
| TD-016 | 2026-05-09 | Stale comment in `src/game_mod.rs:1267` ("The kilteevan mod should have pronunciation data") — per CLAUDE.md the active setting mod is **Rundale**. Update to "The rundale mod" so the test comment matches `find_default_mod()`'s actual return value. |
| TD-017 | 2026-05-09 | Duplicated `chrono::Weekday` → English-name match at `src/ipc/handlers.rs:40-49` (inside `snapshot_from_world`) and `src/debug_snapshot.rs:630-639` (inside `build_clock_debug`). Both match seven variants to "Monday".."Sunday". Extract a `fn weekday_name(w: Weekday) -> &'static str` helper (handlers.rs is the natural home since it already exposes other small string helpers like `capitalize_first`). |
| TD-018 | 2026-05-09 | Awkward unused-warning suppression at `src/mod_source.rs:198-205`: `setting_id` is computed eagerly via `peek_mod_id` but only consumed inside the `Err` arm at line 209, so the `Ok` arm has to write `let _ = setting_id;` to silence the warning. Move the `peek_mod_id` call into the `Err` arm (or compute lazily with `.unwrap_or_else`) and drop the `let _ =` line. |
| TD-019 | 2026-05-09 | Dead parameter on the public surface: `snapshot_from_world` at `src/ipc/handlers.rs:24` takes `_transport: &TransportMode` and never uses it. 6 call sites pass a `TransportMode` for nothing (`src/game_loop/movement.rs:232`, `src/game_loop/save.rs:191`, `src/ipc/handlers.rs:799/811`, plus three in `parish-server/src/routes.rs` and one in `parish-tauri/src/commands.rs`). Drop the parameter and update callers in one PR (cross-crate but mechanical — pure rename refactor). |
| TD-020 | 2026-05-09 | `prepare_npc_conversation` at `src/ipc/handlers.rs:645` is documented as a "Backward-compatible single-target helper" but has exactly one caller in the workspace: `parish-cli/src/headless.rs:771`. Either (a) inline it into the CLI caller and delete the shim, or (b) drop the "backward-compatible" framing — there is no longer anything backward to be compatible with. |
| TD-021 | 2026-05-09 | Body duplication between `handle_npc_conversation` Phase 2 chain (`src/game_loop/npc_turn.rs:443-481`) and the second loop in `run_idle_banter` (`src/game_loop/npc_turn.rs:587-625`). Both run identical logic: clamp `chain_cap` against `autonomous::MAX_CHAIN_TURNS`, pick next speaker via `autonomous::pick_next_speaker`, call `run_npc_turn` with `player_initiated=false` and a no-op spawn_loading, push the line, update `last_spoken_at`, append to `spoken_this_chain`/`last_speaker`. Extract into a private `async fn run_autonomous_chain(ctx, queue, model, max_turns, spoken_this_chain, last_speaker, targets, transcript, combined_hints, prompt_text)` so a future tweak (timeout policy, prompt wording) updates both sites at once. |
| TD-022 | 2026-05-09 | `find_mods_root` at `src/game_mod.rs:773` and `LocalDiskModSource::new` at `src/mod_source.rs:110` walk up from `std::env::current_dir()` looking for a `mods/` directory. Per CLAUDE.md rule #9 ("resolve runtime paths from explicit config, not the cwd") that pattern is forbidden in handlers; today the helpers leak via `find_default_mod()` into request-time paths (`parish-server/src/editor_routes.rs:75/535/550` and `parish-tauri/src/{commands.rs:1131,editor_commands.rs:25/122/135}`). Audit callers and either (a) require an explicit `mods_dir` resolved once at startup and stored on `AppState`/`GlobalState`, or (b) document at the helper that it is for tests/dev only and add a `#[deprecated]` shim for production callers. |

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

Discovery scan of `parish/crates/parish-core/src/` found no additional credible debt beyond the items already tracked. The dead-code removal (TD-001, TD-010), doc fixes (TD-013, TD-014), duplication cleanup (TD-003), and test additions (TD-004, TD-007, TD-009) cover the actionable items. All in-scope weak-test and complexity items were addressed inline (see TD-001 through TD-014).
