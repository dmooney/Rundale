//! Quality axes and the weighted-mean roll-up.
//!
//! Seven session-level axes, each scored 0–100 by the judge. The weighted mean
//! is the headline `quality_score` (only computed when all gates pass). Weights
//! are a const table: coherence and character fidelity dominate because they
//! are the headline regressions; intent and common-sense are weighted lower
//! because they are *also* surfaced as discrete findings, so weighting them
//! heavily here would double-count.

use std::fmt;

/// The seven quality axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    NarrativeCoherence,
    CharacterFidelity,
    WorldResponsiveness,
    IntentFidelity,
    Immersion,
    Progression,
    CommonSense,
}

impl Axis {
    /// All axes in canonical order (matches the rubric file and DB rows).
    pub const ALL: [Axis; 7] = [
        Axis::NarrativeCoherence,
        Axis::CharacterFidelity,
        Axis::WorldResponsiveness,
        Axis::IntentFidelity,
        Axis::Immersion,
        Axis::Progression,
        Axis::CommonSense,
    ];

    /// Stable snake_case key (DB column value + JSON key from the judge).
    pub fn key(self) -> &'static str {
        match self {
            Axis::NarrativeCoherence => "narrative_coherence",
            Axis::CharacterFidelity => "character_fidelity",
            Axis::WorldResponsiveness => "world_responsiveness",
            Axis::IntentFidelity => "intent_fidelity",
            Axis::Immersion => "immersion",
            Axis::Progression => "progression",
            Axis::CommonSense => "common_sense",
        }
    }

    /// Parse from the judge's JSON key.
    pub fn from_key(key: &str) -> Option<Axis> {
        Axis::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Relative weight in the quality roll-up. Need not sum to 1 — the mean
    /// normalizes by the total weight of the axes actually present.
    pub fn weight(self) -> f64 {
        match self {
            Axis::NarrativeCoherence => 1.5,
            Axis::CharacterFidelity => 1.5,
            Axis::WorldResponsiveness => 1.0,
            Axis::Immersion => 1.0,
            Axis::Progression => 1.0,
            Axis::IntentFidelity => 0.75,
            Axis::CommonSense => 0.75,
        }
    }
}

impl fmt::Display for Axis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// One axis score (0–100) plus the judge's rationale.
#[derive(Debug, Clone)]
pub struct AxisScore {
    pub axis: Axis,
    pub score: u8,
    pub rationale: String,
}

/// Compute the weighted-mean quality score (0–100, rounded to two decimals)
/// over the supplied axis scores. Returns `None` if no axes are present (a
/// gated run carries no quality score).
pub fn weighted_quality(scores: &[AxisScore]) -> Option<f64> {
    if scores.is_empty() {
        return None;
    }
    let mut weighted_sum = 0.0;
    let mut weight_total = 0.0;
    for s in scores {
        let w = s.axis.weight();
        weighted_sum += w * f64::from(s.score);
        weight_total += w;
    }
    if weight_total == 0.0 {
        return None;
    }
    let mean = weighted_sum / weight_total;
    Some((mean * 100.0).round() / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn score(axis: Axis, score: u8) -> AxisScore {
        AxisScore {
            axis,
            score,
            rationale: String::new(),
        }
    }

    #[test]
    fn axes_keys_round_trip() {
        for a in Axis::ALL {
            assert_eq!(Axis::from_key(a.key()), Some(a));
        }
        assert_eq!(Axis::from_key("not_an_axis"), None);
    }

    #[test]
    fn axes_weighted_mean_of_uniform_scores_is_that_score() {
        let scores: Vec<AxisScore> = Axis::ALL.into_iter().map(|a| score(a, 80)).collect();
        assert_eq!(weighted_quality(&scores), Some(80.0));
    }

    #[test]
    fn axes_weighting_favours_coherence_and_character() {
        // Two runs with the same total raw points but concentrated in the
        // heavy axes should out-score one concentrated in the light axes.
        let heavy = vec![
            score(Axis::NarrativeCoherence, 100),
            score(Axis::CharacterFidelity, 100),
            score(Axis::IntentFidelity, 0),
            score(Axis::CommonSense, 0),
        ];
        let light = vec![
            score(Axis::NarrativeCoherence, 0),
            score(Axis::CharacterFidelity, 0),
            score(Axis::IntentFidelity, 100),
            score(Axis::CommonSense, 100),
        ];
        assert!(weighted_quality(&heavy) > weighted_quality(&light));
    }

    #[test]
    fn axes_empty_scores_yield_no_quality() {
        assert_eq!(weighted_quality(&[]), None);
    }
}
