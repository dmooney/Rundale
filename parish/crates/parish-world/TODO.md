# parish-world — Technical Debt

## Open

| ID | Category | Severity | Summary |
|----|----------|----------|---------|

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
| TD-012 | Stale Tests | P2 | Removed `"traversal_minutes": 5` from six validation-test JSON fixtures in `graph.rs`. |
| TD-013 | Stale Docs | P3 | Fixed broken intra-doc link in `encounter.rs` module doc by removing the non-existent `crate::game_mod::EncounterTable` reference. |
| TD-014 | Config/Cargo | P2 | Dropped unused `tokio` dependency from `Cargo.toml`. |
| TD-015 | Dead Code | P3 | Removed redundant `last_check_hour` re-assignment in `weather.rs` (eliminated as part of TD-021). |
| TD-016 | Duplication | P3 | Eliminated duplicated history-pruning block by removing the unused `history` field entirely (TD-021). |
| TD-017 | Weak Tests | P2 | Added unit tests for `increment_tick_generation` covering normal increment and `wrapping_add` overflow. |
| TD-018 | Duplication | P3 | Extracted `loc(name, template, npcs)` test helper in `description.rs` to remove 17-line `LocationData` repetition. |
| TD-019 | Dead Code | P3 | Removed unused `rng` and `prob` variables (and their swallow lines) from `wayfarers.rs` test. |
| TD-020 | Stale Docs | P3 | Re-attached orphaned doc comment for `resolve_movement_with_weather` in `movement.rs` and restored the doc block for `weather_adjusted_travel`. |
| TD-021 | Dead Code | P2 | Dropped unused `WeatherEngine::history()` getter, the internal `history` field, `HISTORY_CAPACITY`, and all prune-on-push branches. |
| TD-022 | Dead Code | P2 | Removed unused encounter APIs (`check_encounter_with_config`, `check_encounter_with_table`, `EncounterTable`, `EncounterEvent`) and their tests; inlined default-config logic into `check_encounter`, changed return type to `Option<String>`, and updated caller in `parish-core`. |
| TD-023 | Weak Tests | P2 | Added unit test for `WeatherEngine::force` verifying immediate state change, `since` reset, and `last_check_hour` arming. |
| TD-024 | Weak Tests | P2 | Added unit tests for `from_parish_file` and `from_mod_params` constructors, including the RFC 3339 fallback branch on malformed date input. |
| TD-025 | Stale Docs | P3 | Fixed broken cross-reference path in `lib.rs` doc comment (`apps/ui/...` → `parish/apps/ui/...`). |

## Progress Log

- **2026-05-11** — Resolved all open items (TD-012 through TD-025). `cargo fmt`, `cargo clippy -p parish-world`, and `cargo test -p parish-world` pass cleanly. Dependent crates `parish-core` and `parish` also pass `cargo check`.
