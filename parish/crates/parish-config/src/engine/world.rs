//! World graph and persistence tuning (`[engine.world]`,
//! `[engine.persistence]`).

use serde::Deserialize;

/// World graph tuning parameters.
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WorldConfig {
    /// Minimum Jaro-Winkler similarity (0.0–1.0) for fuzzy location name matching.
    ///
    /// Higher values reduce false positives but miss more typos.
    #[serde(default = "default_fuzzy_threshold")]
    pub fuzzy_threshold: f64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            fuzzy_threshold: default_fuzzy_threshold(),
        }
    }
}

fn default_fuzzy_threshold() -> f64 {
    0.82
}

/// Persistence / save system tuning parameters.
///
/// Currently empty — reserved for future save-system knobs (e.g. compaction,
/// autosnap interval). The `[engine.persistence]` table is accepted for
/// backward compatibility but has no effect.
#[derive(Debug, Default, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PersistenceConfig {}
