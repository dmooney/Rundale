# parish-config — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-009 | Manifest Hygiene | P2 | `Cargo.toml:15` | `thiserror` is declared as a runtime dependency but is never imported — the crate exclusively uses `parish_types::ParishError` for error propagation (see `src/provider.rs:10,114,503,…`). Drop the dep to slim the build graph. |
| TD-010 | Dead Code | P2 | `src/engine.rs:407-409, 426` | `NpcConfig.two_pass_dialogue` field has zero downstream consumers in the workspace (no references outside this file, and not even surfaced in `parish.example.toml`). Either wire it into the dialogue pipeline or remove the field + default. |
| TD-011 | Dead Code | P3 | `src/engine.rs:706-708, 719-721` | `PersistenceConfig.journal_compaction_threshold` is documented as "Reserved for future use — compaction is not yet implemented" with no consumer beyond reading defaults. If compaction has no roadmap, delete the config knob (and the example.toml entry). Otherwise file an issue and link it from the doc comment. |
| TD-012 | Weak Tests | P2 | `src/engine.rs` | TD-003 added TOML round-trip tests for `SessionConfig`, `CognitiveTierConfig`, `RelationshipLabelConfig`, `ReactionConfig`. Still missing: `EncounterConfig`, `PaletteConfig`, `WorldConfig`, `PersistenceConfig`, `InferenceConfig` (only partial via `test_inference_config_parses_rate_limits_from_toml`), and `MapConfig` (only partial via `test_map_config_deserialize_partial_toml`). Mirrors TD-003 for the remaining structs. |
| TD-013 | Weak Tests | P2 | `src/provider.rs:177, 195, 234-262, 357` | Public methods `Provider::api_key_env_var`, `Provider::is_configured_in_env`, `Provider::provider_display`, `InferenceCategory::name`, `InferenceCategory::from_name`, and `InferenceCategory::env_prefix` have no direct unit tests. Notably `provider_display` has a special-case branch for `NvidiaNim` (`"nvidia-nim"` rather than `"nvidianim"`) that is regression-prone. |
| TD-014 | Weak Tests | P3 | `src/provider.rs:78-94` | `Provider::ALL` is asserted to contain 15 entries via the array length, but no test verifies every variant is listed (a new variant added to the enum would compile-error only if the const length is wrong). Add a test that `ALL.len() == std::mem::variant_count` equivalent — e.g. exhaustive `match` over each variant inside the test loop. |

## In Progress

*(none)*

## Done

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P2 | `Cargo.toml:14` | Unused dependency `dotenvy` — removed from manifest. |
| TD-002 | Duplication | P2 | `src/engine.rs` | All `impl Default` blocks now delegate to the standalone `default_*()` functions, eliminating the dual-source-of-truth. |
| TD-003 | Weak Tests | P1 | `src/engine.rs` | Added TOML deserialization tests for `SessionConfig`, `CognitiveTierConfig`, `RelationshipLabelConfig`, and `ReactionConfig`. |
| TD-004 | Weak Tests | P2 | `src/engine.rs:26-44` | Added `test_load_engine_config_none` exercising the `None` path. |
| TD-006 | Stale Docs | P3 | `README.md` | Added `presets` module to the module listing. |
| TD-007 | Stale Docs | P3 | `src/engine.rs:277` | Fixed comment referencing outdated import path `parish-types::time` → `parish_types`. |
| TD-008 | Dead Code | P3 | `src/lib.rs:10`, `src/presets.rs:20` | Removed `pub type PresetModels` type alias and its re-export; inlined return type on `preset_models()`. No downstream consumers existed. |
| TD-005 | Config | P2 | `src/engine.rs:26, 33-34`, `parish-cli/src/main.rs:248`, `parish-server/src/lib.rs:463`, `parish-tauri/src/lib.rs:690` | `load_engine_config` now takes `&Path` (not `Option<&Path>`). Added `resolve_config_path` helper that walks up from a startup-resolved dir. All three call sites resolve the config path at startup. |

## Follow-up

*(none)*

## Discovery scan (2026-05-07)

Scanned the entire `parish-config` crate for dead code, duplication, weak tests, stale docs, and brittle patterns. No credible new debt found beyond what was already catalogued.
