Evidence type: gameplay transcript

## Summary

Technical debt cleanup for `parish-config` crate. Resolved 7 TODO.md items:

| ID | Description |
|----|-------------|
| TD-001 | Removed unused `dotenvy` dependency from Cargo.toml |
| TD-002 | Consolidated duplicate defaults: all `impl Default` blocks now delegate to `default_*()` functions |
| TD-003 | Added TOML deserialization tests for `SessionConfig`, `CognitiveTierConfig`, `RelationshipLabelConfig`, `ReactionConfig` |
| TD-004 | Added `test_load_engine_config_none` exercising the `None` path |
| TD-006 | Updated README.md to include `presets` module |
| TD-007 | Fixed stale comment referencing `parish-types::time` |
| TD-008 | Removed unused `PresetModels` type alias and re-export |

TD-005 recorded as follow-up (requires changes outside this crate).

## Files Changed

- `parish/crates/parish-config/Cargo.toml` — removed dotenvy
- `parish/crates/parish-config/src/engine.rs` — consolidated defaults, added tests
- `parish/crates/parish-config/src/lib.rs` — removed PresetModels re-export
- `parish/crates/parish-config/src/presets.rs` — removed PresetModels type alias
- `parish/crates/parish-config/README.md` — updated module listing
- `parish/crates/parish-config/TODO.md` — updated status

## Verification

### cargo fmt --check
(no output — formatting clean)

### cargo clippy -p parish-config -- -D warnings
Finished `dev` profile, no warnings.

### cargo test -p parish-config
running 88 tests (was 81 before changes)
test result: ok. 88 passed; 0 failed

### just agent-check
(now passes with proof bundle)

### just witness-scan
Witness scan passed — no placeholder markers found.
