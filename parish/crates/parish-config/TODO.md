# parish-config — Technical Debt

## Open

| ID | Category | Severity | Location | Description |
|----|----------|----------|----------|-------------|
| TD-001 | Dead Code | P2 | `Cargo.toml:14` | Unused dependency `dotenvy` — listed in `[dependencies]` but never imported or referenced in any `.rs` file in this crate. `dotenvy::dotenv()` is called only in sibling crates (`parish-cli`, `parish-server`, `parish-tauri`). Remove from this crate's manifest or move the `.env` loading here. |
| TD-002 | Duplication | P2 | `src/engine.rs` (40+ locations) | Every config struct defines default values twice — once in `impl Default` and again in standalone `default_*()` functions for `#[serde(default = "...")]` attributes. Example: `SessionConfig::default()` sets `idle_banter_after_secs: 25` (line 111) while `default_idle_banter_after_secs()` returns `25` (line 118-120). If one source is updated without the other, the divergence silently corrupts defaults. Refactor `Default` impls to delegate to the `default_*()` functions so each value has a single source of truth. |
| TD-003 | Weak Tests | P1 | `src/engine.rs` | Missing TOML deserialization tests for `SessionConfig`, `CognitiveTierConfig`, `RelationshipLabelConfig`, and `ReactionConfig`. Only `Default`-value tests exist (e.g., `test_session_config_defaults` does not exist at all). Users customize these sections in `parish.toml`; a malformed or partial TOML block should not silently revert to defaults. |
| TD-004 | Weak Tests | P2 | `src/engine.rs:26-44` | `load_engine_config(None)` path untested. The doc at lines 18-25 says this is intended for Tauri/web-server boot (which all call `load_engine_config(None)`), but the `None` path — which defaults to `Path::new("parish.toml")` relative to CWD — is never exercised. Only `Some(&Path)` is tested. |
| TD-005 | Config/Cargo | P2 | `src/engine.rs:26, 33-34` | `load_engine_config` defaults to `Path::new("parish.toml")` (CWD-relative) when `path` is `None`, contradicting Rule 9 ("never call `current_dir()` or parent-walk from request handlers"). All three runtimes pass `None`, relying on this implicit CWD default. The path should be resolved at startup and passed explicitly via `Some`. |
| TD-006 | Stale Docs | P3 | `README.md` | Module listing omits `presets` — README lists `engine`, `provider`, `flags` but `presets` (added later with provider model recommendations) is unmentioned. |
| TD-007 | Stale Docs | P3 | `src/engine.rs:277` | Section-header comment reads `SpeedConfig is defined in parish-types::time` but the actual import path is `parish_types::SpeedConfig`. The `::time` module path appears to have been reorganized without updating the comment. |
| TD-008 | Dead Code | P3 | `src/lib.rs:10`, `src/presets.rs:20` | `pub type PresetModels = [Option<&'static str>; 4]` is re-exported from `lib.rs` but never imported by any downstream crate. Only `Provider::preset_models()`, `preset_model()`, and `has_preset()` are used externally; the type alias is public surface with no consumers. |

## In Progress

*(none)*

## Done

*(none)*
