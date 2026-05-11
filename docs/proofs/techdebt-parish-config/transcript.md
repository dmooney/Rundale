Evidence type: gameplay transcript

## Summary

Follow-up technical debt cleanup for `parish-config` crate. Resolved all remaining open TODO.md items (TD-009 through TD-014):

| ID | Category | Description |
|----|----------|-------------|
| TD-009 | Manifest Hygiene | Removed unused `thiserror` dependency from `Cargo.toml` |
| TD-010 | Dead Code | Removed unused `NpcConfig.two_pass_dialogue` field and default |
| TD-011 | Dead Code | Removed unused `PersistenceConfig.journal_compaction_threshold`; struct is now intentionally empty placeholder |
| TD-012 | Weak Tests | Added TOML deserialization tests for `EncounterConfig`, `PaletteConfig`, `WorldConfig`, `PersistenceConfig`, `InferenceConfig`, `MapConfig` |
| TD-013 | Weak Tests | Added unit tests for `Provider::api_key_env_var`, `Provider::is_configured_in_env`, `ProviderConfig::provider_display`, `InferenceCategory::{name,from_name,env_prefix}` |
| TD-014 | Weak Tests | Added exhaustive test verifying `Provider::ALL` contains all 15 variants |

## Files Changed

- `parish/crates/parish-config/Cargo.toml` — removed thiserror
- `parish/crates/parish-config/src/engine.rs` — removed dead fields, added TOML round-trip tests
- `parish/crates/parish-config/src/provider.rs` — added unit tests for Provider and InferenceCategory methods
- `parish/parish.example.toml` — removed `journal_compaction_threshold` entry
- `parish/crates/parish-config/TODO.md` — moved resolved items to Done

## Verification

### cargo fmt -p parish-config
(no output — formatting clean)

### cargo clippy -p parish-config
Finished `dev` profile, no warnings.

### cargo test -p parish-config
running 109 tests (was 88 before changes)
test result: ok. 109 passed; 0 failed; 0 ignored
