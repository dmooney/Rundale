//! Discrete playtest findings + their de-duplication signatures.
//!
//! The judge does not just score axes; acting as an adversarial human
//! playtester it itemizes concrete bugs (an NPC exiting mid-sentence, a
//! third-person self-reference, a name mix-up, an intent-parse miss, …). Each
//! finding gets a canonical signature so the same class of defect, seen across
//! many runs, collapses to one tracked issue instead of spamming the tracker.

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::hex;

/// Severity of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Low => "low",
            Severity::Medium => "medium",
            Severity::High => "high",
            Severity::Critical => "critical",
        }
    }

    pub fn from_label(s: &str) -> Severity {
        match s.trim().to_ascii_lowercase().as_str() {
            "critical" => Severity::Critical,
            "high" => Severity::High,
            "medium" => Severity::Medium,
            // Unknown severities degrade to Low rather than failing the verdict.
            _ => Severity::Low,
        }
    }
}

/// A single discrete finding emitted by the judge.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Category (e.g. `npc_midsentence_exit`, `third_person_self_ref`,
    /// `identity_mixup`, `intent_parse`, `common_sense`, `if_genre_violation`).
    pub category: String,
    /// Turn the defect occurred at (None = run-level).
    pub turn_index: Option<u32>,
    pub severity: Severity,
    pub description: String,
    /// The exact offending line/quote.
    pub evidence_quote: String,
    /// Canonical de-dup key, computed from the above.
    pub signature: String,
}

/// The judge's raw finding before canonicalization (matches the JSON it emits).
#[derive(Debug, Clone, Deserialize)]
pub struct RawFinding {
    pub category: String,
    #[serde(default)]
    pub turn_index: Option<u32>,
    pub severity: String,
    pub description: String,
    #[serde(default)]
    pub evidence_quote: String,
    /// The judge's suggested dedup phrase; the canonicalizer is authoritative
    /// but seeds from this.
    #[serde(default)]
    pub signature_hint: String,
}

impl RawFinding {
    /// Canonicalize into a [`Finding`] with a stable signature.
    pub fn canonicalize(self) -> Finding {
        let severity = Severity::from_label(&self.severity);
        let signature =
            canonical_signature(&self.category, &self.signature_hint, &self.evidence_quote);
        Finding {
            category: self.category,
            turn_index: self.turn_index,
            severity,
            description: self.description,
            evidence_quote: self.evidence_quote,
            signature,
        }
    }
}

/// Compute the canonical de-dup signature for a finding.
///
/// The signature is `sha256(category | normalized-evidence)` where the
/// normalized evidence is the judge's signature hint when present (it is the
/// class-level phrase), otherwise the evidence quote. Normalization lowercases,
/// collapses whitespace, and strips trailing punctuation so cosmetic variants
/// of the same defect collapse. Deliberately *excludes* the turn index — the
/// same defect on different turns is the same bug.
pub fn canonical_signature(category: &str, hint: &str, evidence: &str) -> String {
    let basis = if hint.trim().is_empty() {
        evidence
    } else {
        hint
    };
    let normalized = normalize(basis);
    let mut hasher = Sha256::new();
    hasher.update(normalize(category).as_bytes());
    hasher.update(b"|");
    hasher.update(normalized.as_bytes());
    hex(&hasher.finalize())
}

/// Lowercase, collapse internal whitespace, trim surrounding whitespace and
/// trailing sentence punctuation.
fn normalize(s: &str) -> String {
    let lowered = s.to_lowercase();
    let collapsed = lowered.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed
        .trim_matches(|c: char| c == '.' || c == '!' || c == '?' || c == ',')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_is_stable_for_cosmetic_variants() {
        let a = canonical_signature("identity_mixup", "calls Maggie by wrong name", "");
        let b = canonical_signature("identity_mixup", "Calls Maggie by wrong name.", "");
        let c = canonical_signature("identity_mixup", "calls   maggie  by wrong name", "");
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn signature_differs_by_category() {
        let a = canonical_signature("identity_mixup", "same phrase", "");
        let b = canonical_signature("common_sense", "same phrase", "");
        assert_ne!(a, b);
    }

    #[test]
    fn signature_ignores_turn_index() {
        // Two findings of the same class on different turns dedup together
        // because the signature never sees the turn.
        let f1 = RawFinding {
            category: "npc_midsentence_exit".into(),
            turn_index: Some(3),
            severity: "high".into(),
            description: "x".into(),
            evidence_quote: "She leaves. She says hello.".into(),
            signature_hint: "npc exits then speaks".into(),
        }
        .canonicalize();
        let f2 = RawFinding {
            category: "npc_midsentence_exit".into(),
            turn_index: Some(40),
            severity: "high".into(),
            description: "y".into(),
            evidence_quote: "different quote entirely".into(),
            signature_hint: "npc exits then speaks".into(),
        }
        .canonicalize();
        assert_eq!(f1.signature, f2.signature);
    }

    #[test]
    fn signature_falls_back_to_evidence_when_no_hint() {
        let s = canonical_signature("other", "", "Some raw evidence line");
        let s2 = canonical_signature("other", "", "some raw evidence line");
        assert_eq!(s, s2);
        assert!(!s.is_empty());
    }

    #[test]
    fn severity_parses_and_degrades() {
        assert_eq!(Severity::from_label("Critical"), Severity::Critical);
        assert_eq!(Severity::from_label("HIGH"), Severity::High);
        assert_eq!(Severity::from_label("weird"), Severity::Low);
    }
}
