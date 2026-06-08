//! Run configuration — the full, A/B-able knob set for one harness run, plus
//! content-addressing so identical configs collapse to one `configs` row and an
//! A/B comparison is exact.
//!
//! Knobs (all first-class): engine models per inference category, feature flags
//! applied to the game under test, the player (mode + model + persona +
//! strategy), the judge (mode + model + pinned rubric), and gate tolerances.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{HarnessError, Result};

/// Which implementation drives the player / judge role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorMode {
    /// Deterministic, no API key — used for the loop proof and CI.
    Scripted,
    /// `parish-inference`-backed LLM (Anthropic / OpenAI-compat / local).
    Api,
}

/// Player-role configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerCfg {
    pub mode: ActorMode,
    /// Model id when `mode == Api` (ignored for `Scripted`).
    #[serde(default)]
    pub model: Option<String>,
    /// In-character persona the player adopts.
    #[serde(default)]
    pub persona: String,
    /// High-level play strategy.
    #[serde(default)]
    pub strategy: String,
}

/// Judge-role configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeCfg {
    pub mode: ActorMode,
    /// Model id when `mode == Api`.
    #[serde(default)]
    pub model: Option<String>,
    /// Rubric version label (selects the rubric file).
    #[serde(default = "default_rubric_version")]
    pub rubric_version: String,
    /// Pinned rubric content hash. `None` means "pin to whatever the rubric
    /// file currently hashes to" (recorded on the run); `Some` means "fail the
    /// run if the rubric file drifted from this hash".
    #[serde(default)]
    pub rubric_sha256: Option<String>,
}

fn default_rubric_version() -> String {
    "v1".to_string()
}

/// Deterministic hard-fail tolerances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateCfg {
    /// How many `rejected` outcomes are tolerated before the ParserReject gate
    /// trips.
    #[serde(default = "default_max_rejects")]
    pub max_rejects: u32,
    /// How many consecutive empty / no-delta turns are tolerated before the
    /// EmptyTurnBurn gate trips.
    #[serde(default = "default_empty_burn_limit")]
    pub empty_burn_limit: u32,
}

fn default_max_rejects() -> u32 {
    3
}
fn default_empty_burn_limit() -> u32 {
    5
}

impl Default for GateCfg {
    fn default() -> Self {
        Self {
            max_rejects: default_max_rejects(),
            empty_burn_limit: default_empty_burn_limit(),
        }
    }
}

/// The complete run configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunConfig {
    /// Optional human label / template name.
    #[serde(default)]
    pub label: Option<String>,
    /// Per-inference-category model overrides applied to the game via BYOK,
    /// keyed by category (`dialogue`, `intent`, `reaction`, `simulation`).
    /// A `BTreeMap` so serialization is canonical (stable key order) for
    /// content-addressing.
    #[serde(default)]
    pub engine_models: BTreeMap<String, EngineModel>,
    /// Feature flags applied to the game under test (via the `/flag` command).
    #[serde(default)]
    pub flags: Vec<FlagSetting>,
    pub player: PlayerCfg,
    pub judge: JudgeCfg,
    #[serde(default)]
    pub gate: GateCfg,
}

/// One per-category engine model override (a BYOK target).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineModel {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// One feature-flag setting applied to the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagSetting {
    pub name: String,
    pub on: bool,
}

impl RunConfig {
    /// Load a run config from a JSON file.
    pub fn load(path: &std::path::Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .map_err(|e| HarnessError::io(path.display().to_string(), e))?;
        serde_json::from_str(&body).map_err(|e| HarnessError::Json {
            context: format!("run config {}", path.display()),
            source: e,
        })
    }

    /// Canonical JSON used for content-addressing. `serde_json` emits map keys
    /// in insertion order; we go through `serde_json::Value` and rely on its
    /// `BTreeMap`-backed object ordering so the bytes are stable regardless of
    /// struct field declaration order changing.
    pub fn canonical_json(&self) -> String {
        // Round-trip through Value to get sorted-key object serialization.
        let value: serde_json::Value =
            serde_json::to_value(self).expect("RunConfig always serializes");
        canonicalize(&value)
    }

    /// Content hash (sha256 hex) of the canonical JSON. Two configs with the
    /// same knobs — regardless of field/key ordering or optional-field absence
    /// vs explicit null — hash identically.
    pub fn content_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json().as_bytes());
        hex(&hasher.finalize())
    }
}

/// Serialize a `Value` with object keys sorted recursively, producing stable
/// bytes for hashing.
fn canonicalize(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(&String, &serde_json::Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), canonicalize(v)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        serde_json::Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(canonicalize).collect();
            format!("[{}]", inner.join(","))
        }
        other => serde_json::to_string(other).unwrap(),
    }
}

/// Lowercase hex encoding of a byte slice.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RunConfig {
        RunConfig {
            label: Some("smoke".into()),
            engine_models: BTreeMap::new(),
            flags: vec![],
            player: PlayerCfg {
                mode: ActorMode::Scripted,
                model: None,
                persona: "newcomer".into(),
                strategy: "explore".into(),
            },
            judge: JudgeCfg {
                mode: ActorMode::Scripted,
                model: Some("claude-sonnet-4-6".into()),
                rubric_version: "v1".into(),
                rubric_sha256: None,
            },
            gate: GateCfg::default(),
        }
    }

    #[test]
    fn config_hash_is_stable_across_calls() {
        let c = sample();
        assert_eq!(c.content_hash(), c.content_hash());
    }

    #[test]
    fn config_hash_changes_when_a_knob_changes() {
        let a = sample();
        let mut b = sample();
        b.flags.push(FlagSetting {
            name: "weather".into(),
            on: true,
        });
        assert_ne!(
            a.content_hash(),
            b.content_hash(),
            "a flag change must produce a new config identity"
        );
    }

    #[test]
    fn config_hash_ignores_label_only_when_unchanged() {
        // The label IS part of identity (a renamed template is a new template),
        // so two configs differing only by label must hash differently.
        let a = sample();
        let mut b = sample();
        b.label = Some("other".into());
        assert_ne!(a.content_hash(), b.content_hash());
    }

    #[test]
    fn engine_models_order_does_not_affect_hash() {
        // BTreeMap guarantees canonical ordering; inserting in different orders
        // must still hash identically.
        let mut a = sample();
        let mut b = sample();
        a.engine_models.insert(
            "dialogue".into(),
            EngineModel {
                provider: "anthropic".into(),
                model: "claude".into(),
                base_url: None,
            },
        );
        a.engine_models.insert(
            "intent".into(),
            EngineModel {
                provider: "ollama".into(),
                model: "qwen".into(),
                base_url: None,
            },
        );
        b.engine_models.insert(
            "intent".into(),
            EngineModel {
                provider: "ollama".into(),
                model: "qwen".into(),
                base_url: None,
            },
        );
        b.engine_models.insert(
            "dialogue".into(),
            EngineModel {
                provider: "anthropic".into(),
                model: "claude".into(),
                base_url: None,
            },
        );
        assert_eq!(a.content_hash(), b.content_hash());
    }
}
