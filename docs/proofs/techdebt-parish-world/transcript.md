Evidence type: gameplay transcript

## Summary

Resolved all 14 remaining TODO.md items in `parish/crates/parish-world/` (TD-012 through TD-025):

### Stale Tests & Docs (TD-012, TD-013, TD-020, TD-025)
- Removed `"traversal_minutes": 5` from six validation-test JSON fixtures in `graph.rs`
- Fixed broken intra-doc link in `encounter.rs` module doc
- Re-attached orphaned doc comment for `resolve_movement_with_weather` in `movement.rs`
- Fixed broken cross-reference path in `lib.rs` doc comment

### Config/Cargo (TD-014)
- Dropped unused `tokio` dependency from `Cargo.toml`

### Dead Code & Duplication (TD-015, TD-016, TD-021, TD-022)
- Removed unused `WeatherEngine::history()` getter, internal `history` field, `HISTORY_CAPACITY`, and redundant `last_check_hour` re-assignment
- Removed unused encounter APIs (`check_encounter_with_config`, `check_encounter_with_table`, `EncounterTable`, `EncounterEvent`) and simplified `check_encounter` to return `Option<String>`
- Updated caller in `parish-core/src/game_session.rs`

### Weak Tests (TD-017, TD-023, TD-024)
- Added unit tests for `increment_tick_generation` (normal + wrapping overflow)
- Added unit test for `WeatherEngine::force` (state change, `since` reset, `last_check_hour` arming)
- Added unit tests for `from_parish_file` and `from_mod_params` constructors, including RFC 3339 fallback on bad date input

### Test Helpers (TD-018, TD-019)
- Extracted `loc()` test helper in `description.rs` to remove 17-line `LocationData` repetition
- Removed dead `rng` and `prob` variables from `wayfarers.rs` test

## Test Output

```
running 152 tests
test result: ok. 152 passed; 0 failed; 0 ignored
```

## Clippy Output

```
cargo clippy -p parish-world --all-targets --all-features: clean (no warnings)
```
