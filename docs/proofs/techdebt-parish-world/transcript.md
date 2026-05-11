Evidence type: gameplay transcript

## Summary

Resolved 11 TODO.md items in `parish/crates/parish-world/`:

### Dead Code & Config (TD-008, TD-009, TD-010, TD-011)
- Removed unused `traversal_minutes` field from `Connection` struct
- Removed unused `anyhow` and `thiserror` dependencies
- Moved `toml` to `[dev-dependencies]` (test-only usage)
- Fixed README.md module list (removed `palette`, added `session`, `wayfarers`, `weather_travel`)

### Duplication (TD-001, TD-002, TD-003, TD-004)
- `shortest_path` now delegates to `shortest_path_filtered` with always-true closure
- Extracted `WorldState::init()` and `graph_to_legacy_locations()` to share field init across constructors
- Extracted `encounter_threshold()` to share TimeOfDay→threshold mapping
- Extracted `resolve_target()` to share find-by-name + AlreadyHere guard

### Complexity (TD-005, TD-006)
- Replaced magic `u8::MAX` sentinel with a `MatchLevel` enum with `Ord` derive
- Extracted `weather_adjusted_travel()` and `blocked_or_fallback()` from `resolve_movement_with_weather`

### Tests (TD-007)
- Added 5 direct unit tests for `shortest_path_filtered`: empty filter, same-location, nonexistent target, target-only edges, identity with unfiltered

## Test Output

```
running 152 tests
test result: ok. 152 passed; 0 failed; 0 ignored
```

## Clippy Output

```
cargo clippy -p parish-world -- -D warnings: clean (no warnings)
```
