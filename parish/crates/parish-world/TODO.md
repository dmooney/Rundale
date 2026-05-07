# parish-world — Technical Debt

## Open

*(none)*

## In Progress

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
