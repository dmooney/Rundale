# parish-world — Technical Debt

## Open

| ID | Category | Severity | Summary |
|----|----------|----------|---------|
| TD-012 | Stale Tests | P2 | `graph.rs:937,957,965,973,1013,1021` validation-test JSON fixtures still embed `"traversal_minutes": 5` on every connection; the field was removed from `Connection` (TD-008) and is silently ignored by serde, leaving misleading examples and an incomplete cleanup. |
| TD-013 | Stale Docs | P3 | `encounter.rs:7` module doc references `[`EncounterTable`](crate::game_mod::EncounterTable)` but `crate::game_mod` does not exist in `parish-world`; rustdoc intra-doc link is broken. |
| TD-014 | Config/Cargo | P2 | `Cargo.toml:18` declares `tokio = { workspace = true, features = ["sync"] }` but no `src/*.rs` references `tokio::` directly (`grep -rn tokio src/` returns nothing). Either the dep is unused or only transitively required by `parish-types::EventBus` — confirm and drop or document. |
| TD-015 | Dead Code | P3 | `weather.rs:183` already sets `self.last_check_hour = Some(current_hour)` before the early-return guards; line 193 redundantly re-assigns the same value after a successful transition. Remove the duplicate write. |
| TD-016 | Duplication | P3 | The history-pruning `if self.history.len() >= HISTORY_CAPACITY { pop_front } push_back` block is duplicated in `weather.rs:157-160` (`force`) and `weather.rs:195-198` (`tick`). Extract a `record_transition(&mut self, now, weather)` helper. |
| TD-017 | Weak Tests | P2 | `lib.rs:232 increment_tick_generation` is a public method with no unit test in `lib.rs` `mod tests` — neither the +1 increment nor the `wrapping_add` overflow behavior described in the doc comment is exercised. |
| TD-018 | Duplication | P3 | `description.rs:98-115` and `description.rs:175-191` build nearly identical `LocationData` structs by hand (only `name`, `description_template`, `associated_npcs` differ). Extract a small `fn loc(name, template, npcs)` test helper to remove the 17-line repetition. |
| TD-019 | Dead Code | P3 | `wayfarers.rs:502-516` allocates `rng` and `prob`, then does `let _ = prob; let _ = rng;` to swallow them. The variables document intent but never participate in any assertion — drop them or assert against the produced `prob`. |
| TD-020 | Stale Docs | P3 | `movement.rs:167-175` doc comment for `resolve_movement_with_weather` claims to behave like `resolve_movement`, then a second `///` block at line 177 documents `weather_adjusted_travel` directly above the function — the doc for `resolve_movement_with_weather` is orphaned (the function declaration is at line 254, well below the block). The two doc comments need to be split with a blank line / re-attached so the public API doc lands on the right item. |
| TD-021 | Dead Code | P2 | `weather.rs:124 WeatherEngine::history()` getter is `pub` but has zero callers anywhere in the workspace (`rg "weather_engine\.history\(\)"` returns nothing). The internal `history` field is only written, never read. Either drop the getter and the `VecDeque<(DateTime<Utc>, Weather)>` field+capacity (`HISTORY_CAPACITY`, the prune-on-push branches at `weather.rs:157-160` and `weather.rs:195-198` from TD-016) or surface it through a debug snapshot the way `since()` / `min_duration_hours()` / `last_check_hour()` are surfaced via `parish-core/src/debug_snapshot.rs:679-684`. |
| TD-022 | Dead Code | P2 | Three public encounter APIs in `encounter.rs` are unused outside the crate: `check_encounter_with_config` (line 63), `check_encounter_with_table` (line 83), and `EncounterTable` (line 19) / `EncounterEvent` (line 27) types. `parish-core/src/game_session.rs:161` and `parish-cli/tests/world_graph_integration.rs:238` only call `check_encounter`, which uses `EncounterConfig::default()` internally. Mod-supplied tables route through `parish_core::game_mod::EncounterTable` — a separate, unrelated struct in `parish-core/src/game_mod.rs:195`. Either wire the with-table path into the actual mod loader or delete the dead surface (and the encounter-table tests at lines 200-232). |
| TD-023 | Weak Tests | P2 | `weather.rs:153 WeatherEngine::force` is a public method called by `parish-core/src/ipc/commands.rs:748` (`/weather <name>` command) with no unit test in `weather.rs` `mod tests` — none of the documented guarantees (immediate state change, `since` reset, `last_check_hour` arming so the next `tick()` skips the just-forced hour, history-ring entry appended) are exercised. A regression in `force` would silently break the `/weather` cheat command. |
| TD-024 | Weak Tests | P2 | `lib.rs:143 from_parish_file` and `lib.rs:158 from_mod_params` are the two production constructors used by `parish-core` mod loaders, yet neither has a test in `lib.rs` `mod tests` — only `WorldState::new()` (the in-memory crossroads stub) is covered. The RFC 3339 fallback branch at `lib.rs:166-175` (parse error → `Utc::now()` warning) is completely unexercised, so a malformed mod `start_date` ships untested. |
| TD-025 | Stale Docs | P3 | `lib.rs:35-36` `MAX_TEXT_LOG` doc says "matching the frontend cap (`MAX_TEXT_LOG_SIZE` in `apps/ui/src/stores/game.ts`)", but the repo layout is `parish/apps/ui/src/stores/game.ts` (per CLAUDE.md "Frontend: `parish/apps/ui/`"). The dangling `apps/ui/...` path has no `apps/` directory at the repo root — the cross-reference is broken until the prefix is fixed. |

*(none)*

## Done

| ID | Category | Severity | Summary |
|----|----------|----------|---------|
| TD-008 | Dead Code | P3 | Removed unused `traversal_minutes` field from `Connection` struct and the test struct literal. |
| TD-009 | Config/Cargo | P3 | Removed unused `anyhow` and `thiserror` dependencies. |
| TD-010 | Config/Cargo | P2 | Moved `toml` to `[dev-dependencies]` (only used in test code). |
| TD-011 | Stale Docs | P3 | Updated README.md module list: removed `palette`, added `session`, `wayfarers`, `weather_travel`. |
| TD-001 | Duplication | P2 | Replaced `shortest_path` BFS body with delegation to `shortest_path_filtered` (always-true closure). |
| TD-002 | Duplication | P2 | Extracted `WorldState::init()` and `graph_to_legacy_locations()` to eliminate repeated field initialization and graph→locations loop in all three constructors. |
| TD-003 | Duplication | P2 | Extracted `encounter_threshold()` helper to eliminate the duplicated 7-arm `match` in both encounter functions. |
| TD-004 | Duplication | P2 | Extracted `resolve_target()` helper to eliminate the duplicated `find_by_name` + AlreadyHere prefix. |
| TD-005 | Complexity | P2 | Added `MatchLevel` enum with `Ord` derive to replace magic `u8::MAX` sentinel and unnumbered level constants. |
| TD-006 | Complexity | P2 | Extracted `weather_adjusted_travel()` and `blocked_or_fallback()` helpers from `resolve_movement_with_weather`, reducing the function from ~95 lines to ~25. |
| TD-007 | Weak Tests | P1 | Added 5 new unit tests for `shortest_path_filtered`: empty filter, same-location bypass, nonexistent target, only-target-edges filter, always-true matches unfiltered. |
