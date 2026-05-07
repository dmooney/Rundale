# parish-config — Technical Debt

## Open

*(none — all actionable items resolved)*

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

## Follow-up

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-005 | Config | P2 | `src/engine.rs:26, 33-34` | `load_engine_config(None)` falls back to CWD-relative `Path::new("parish.toml")`, violating Rule 9. Fix requires changing callers in `parish-cli`, `parish-server`, and `parish-tauri` to resolve the path at startup and pass `Some(path)` — out of scope for this crate-only pass. |

## Discovery scan (2026-05-07)

Scanned the entire `parish-config` crate for dead code, duplication, weak tests, stale docs, and brittle patterns. No credible new debt found beyond what was already catalogued.
