//! Judge rubric loading + content pinning.
//!
//! The rubric instruction text is content-addressed: the run pins the sha256 of
//! the rubric text so two runs are only score-comparable if they were judged
//! against byte-identical instructions. Only the rubric *text* is hashed — not
//! the output-envelope schema — so the JSON shape the judge returns can evolve
//! without invalidating historical comparisons (the same rule the Python
//! `verify_judge_rubric` follows).

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::hex;
use crate::error::{HarnessError, Result};

/// The on-disk rubric manifest (`rubrics/judge_session_<version>.json`).
#[derive(Debug, Clone, Deserialize)]
pub struct RubricManifest {
    pub judge_id: String,
    pub rubric_version: String,
    #[serde(default)]
    pub model: Option<String>,
    /// The instruction text the judge is given. This is the hashed payload.
    pub rubric: String,
    /// The axes the rubric scores (for cross-checking the verdict).
    #[serde(default)]
    pub axes: Vec<String>,
}

/// A loaded rubric plus its computed content hash.
#[derive(Debug, Clone)]
pub struct Rubric {
    pub manifest: RubricManifest,
    /// sha256 hex of `manifest.rubric`.
    pub sha256: String,
}

impl Rubric {
    /// Parse a manifest from JSON text and compute its content hash.
    pub fn from_json(text: &str) -> Result<Rubric> {
        let manifest: RubricManifest =
            serde_json::from_str(text).map_err(|source| HarnessError::Json {
                context: "rubric manifest".into(),
                source,
            })?;
        let sha256 = sha256_hex(&manifest.rubric);
        Ok(Rubric { manifest, sha256 })
    }

    /// Load a rubric manifest from a file path.
    pub fn load(path: &std::path::Path) -> Result<Rubric> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| HarnessError::io(path.display().to_string(), e))?;
        Rubric::from_json(&text)
    }

    /// Verify the loaded rubric against the hash the run config pinned.
    ///
    /// - `Some(expected)`: must match the loaded hash, else [`HarnessError::RubricDrift`].
    /// - `None`: "pin to current" — accepts and the caller records `self.sha256`.
    pub fn verify_pin(&self, expected: Option<&str>) -> Result<()> {
        match expected {
            Some(exp) if exp != self.sha256 => Err(HarnessError::RubricDrift {
                expected: exp.to_string(),
                actual: self.sha256.clone(),
            }),
            _ => Ok(()),
        }
    }
}

/// sha256 hex of an arbitrary string.
pub fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hex(&hasher.finalize())
}

/// The bundled default rubric (v1), embedded so the harness always has a
/// working judge rubric even when run outside the crate directory.
pub const EMBEDDED_V1: &str = include_str!("../../rubrics/judge_session_v1.json");

/// Load the rubric for a version label. Currently only the embedded `v1` is
/// shipped; unknown versions are a config error.
pub fn load_version(version: &str) -> Result<Rubric> {
    match version {
        "v1" => Rubric::from_json(EMBEDDED_V1),
        other => Err(HarnessError::Config(format!(
            "unknown rubric version {other:?} (only v1 is bundled)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "judge_id": "judge_session_v1",
        "rubric_version": "v1",
        "model": "claude-sonnet-4-6",
        "rubric": "Score the session.",
        "axes": ["narrative_coherence"]
    }"#;

    #[test]
    fn rubric_hash_is_sha256_of_text_only() {
        let r = Rubric::from_json(SAMPLE).unwrap();
        assert_eq!(r.sha256, sha256_hex("Score the session."));
    }

    #[test]
    fn rubric_verify_pin_matches() {
        let r = Rubric::from_json(SAMPLE).unwrap();
        let pin = r.sha256.clone();
        assert!(r.verify_pin(Some(&pin)).is_ok());
        assert!(r.verify_pin(None).is_ok());
    }

    #[test]
    fn rubric_drift_is_an_error() {
        let r = Rubric::from_json(SAMPLE).unwrap();
        let err = r.verify_pin(Some("deadbeef")).unwrap_err();
        assert!(matches!(err, HarnessError::RubricDrift { .. }));
    }

    #[test]
    fn rubric_hash_ignores_non_text_fields() {
        // Same rubric text, different model field → identical hash.
        let a = Rubric::from_json(SAMPLE).unwrap();
        let b = Rubric::from_json(&SAMPLE.replace("claude-sonnet-4-6", "claude-opus-4-8")).unwrap();
        assert_eq!(a.sha256, b.sha256);
    }

    #[test]
    fn embedded_v1_loads_and_has_seven_axes() {
        let r = load_version("v1").unwrap();
        assert_eq!(r.manifest.axes.len(), 7);
        assert_eq!(r.manifest.rubric_version, "v1");
        assert!(!r.sha256.is_empty());
    }

    #[test]
    fn unknown_version_is_config_error() {
        assert!(matches!(load_version("v99"), Err(HarnessError::Config(_))));
    }
}
