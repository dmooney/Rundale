//! Structured emotional state for characters.
//!
//! Each NPC carries an [`EmotionState`] alongside stable traits
//! (personality, intelligence, temperament). Emotions are *transient*
//! — they drift toward a trait-derived baseline, respond to events via
//! [`EmotionImpulse`]s, and expose non-linear behaviour [`EmotionGates`]
//! that dialogue and simulation systems consult.
//!
//! The design is informed by Anthropic's April 2026 paper
//! *Emotion Concepts and their Function in a Large Language Model*:
//! emotions are organised like human psychology, context-local, and
//! non-monotonic in intensity (moderate fear → caution; extreme fear
//! → truth-telling). We model three zoom levels:
//!
//! 1. **PAD** — Pleasure / Arousal / Dominance, each in `[-1.0, 1.0]`,
//!    the bulk parameters that decay toward baseline.
//! 2. **Family vector** — eight intensities in `[0.0, 1.0]` for
//!    Ekman-style families plus shame and affection.
//! 3. **Leaf words** (stored separately in [`crate::emotion_leaves`])
//!    — 171 word-level descriptors projected from state at prompt
//!    time, never stored per-NPC.

use serde::{Deserialize, Serialize};

/// Eight top-level emotion families. Similar in spirit to Ekman's basic
/// set, extended with `Shame` (for social-cohesion dynamics) and
/// `Affection` (for the warm/love axis that's fundamental to a parish
/// setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EmotionFamily {
    /// Joy, contentment, elation.
    Joy,
    /// Sadness, grief, despair.
    Sadness,
    /// Fear, anxiety, dread.
    Fear,
    /// Anger, irritation, rage.
    Anger,
    /// Disgust, contempt, aversion.
    Disgust,
    /// Surprise, astonishment.
    Surprise,
    /// Shame, embarrassment, guilt.
    Shame,
    /// Warmth, love, tenderness.
    Affection,
}

impl EmotionFamily {
    /// Returns all families in canonical order.
    pub const ALL: [EmotionFamily; 8] = [
        EmotionFamily::Joy,
        EmotionFamily::Sadness,
        EmotionFamily::Fear,
        EmotionFamily::Anger,
        EmotionFamily::Disgust,
        EmotionFamily::Surprise,
        EmotionFamily::Shame,
        EmotionFamily::Affection,
    ];
}

/// Intensity per family, clamped to `[0.0, 1.0]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct FamilyVec {
    pub joy: f32,
    pub sadness: f32,
    pub fear: f32,
    pub anger: f32,
    pub disgust: f32,
    pub surprise: f32,
    pub shame: f32,
    pub affection: f32,
}

impl FamilyVec {
    /// Returns the intensity for a given family.
    pub fn get(&self, family: EmotionFamily) -> f32 {
        match family {
            EmotionFamily::Joy => self.joy,
            EmotionFamily::Sadness => self.sadness,
            EmotionFamily::Fear => self.fear,
            EmotionFamily::Anger => self.anger,
            EmotionFamily::Disgust => self.disgust,
            EmotionFamily::Surprise => self.surprise,
            EmotionFamily::Shame => self.shame,
            EmotionFamily::Affection => self.affection,
        }
    }

    /// Mutably accesses the intensity for a given family.
    pub fn get_mut(&mut self, family: EmotionFamily) -> &mut f32 {
        match family {
            EmotionFamily::Joy => &mut self.joy,
            EmotionFamily::Sadness => &mut self.sadness,
            EmotionFamily::Fear => &mut self.fear,
            EmotionFamily::Anger => &mut self.anger,
            EmotionFamily::Disgust => &mut self.disgust,
            EmotionFamily::Surprise => &mut self.surprise,
            EmotionFamily::Shame => &mut self.shame,
            EmotionFamily::Affection => &mut self.affection,
        }
    }

    /// Returns the (family, intensity) pair with the highest intensity.
    ///
    /// Ties are broken by the canonical order in [`EmotionFamily::ALL`].
    pub fn dominant(&self) -> (EmotionFamily, f32) {
        let mut best = (EmotionFamily::Joy, self.joy);
        for fam in EmotionFamily::ALL.iter().copied() {
            let v = self.get(fam);
            if v > best.1 {
                best = (fam, v);
            }
        }
        best
    }
}

/// Stable trait inputs that shape how an NPC experiences emotion.
///
/// Temperament is *static* — loaded from mod content or defaulted —
/// whereas [`EmotionState`] is dynamic.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Temperament {
    /// `-1.0` = melancholic baseline, `+1.0` = cheerful. Default `0.0`.
    /// Sets the baseline pleasure toward which PAD decays.
    #[serde(default)]
    pub cheerfulness: f32,
    /// `0.0` = placid (impulses barely register), `1.0` = volatile
    /// (impulses land at full strength). Default `0.5`.
    #[serde(default = "default_half")]
    pub reactivity: f32,
    /// `0.0` = quick to cool (short half-life), `1.0` = slow to cool
    /// (long half-life). Default `0.5`.
    #[serde(default = "default_half")]
    pub persistence: f32,
}

fn default_half() -> f32 {
    0.5
}

impl Default for Temperament {
    fn default() -> Self {
        Self {
            cheerfulness: 0.0,
            reactivity: 0.5,
            persistence: 0.5,
        }
    }
}

impl Temperament {
    /// Validates that all fields are within the documented ranges.
    ///
    /// Returns an error describing the first out-of-range field so
    /// mod authors see a specific message at load time instead of
    /// silently getting clamped values during gameplay.
    pub fn validate(&self) -> Result<(), String> {
        if !(-1.0..=1.0).contains(&self.cheerfulness) {
            return Err(format!(
                "cheerfulness must be in [-1.0, 1.0], got {}",
                self.cheerfulness
            ));
        }
        if !(0.0..=1.0).contains(&self.reactivity) {
            return Err(format!(
                "reactivity must be in [0.0, 1.0], got {}",
                self.reactivity
            ));
        }
        if !(0.0..=1.0).contains(&self.persistence) {
            return Err(format!(
                "persistence must be in [0.0, 1.0], got {}",
                self.persistence
            ));
        }
        Ok(())
    }

    /// Returns the decay half-life (seconds of game time) implied by
    /// `persistence`. Range: 15 game-minutes (persistence=0) to 2
    /// game-hours (persistence=1).
    pub fn half_life_secs(&self) -> f32 {
        let p = self.persistence.clamp(0.0, 1.0);
        // 900s (15 min) .. 7200s (2 h)
        900.0 + p * 6300.0
    }

    /// Returns the baseline pleasure this temperament decays toward.
    pub fn baseline_pleasure(&self) -> f32 {
        self.cheerfulness.clamp(-1.0, 1.0) * 0.3
    }
}

/// The current emotional state of an NPC.
///
/// All fields are plain floats — this type is `Copy`-free only
/// because `FamilyVec` contains eight floats; otherwise cheap to
/// clone and persist.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmotionState {
    /// Pleasure (valence): `-1.0` misery, `+1.0` bliss.
    pub pleasure: f32,
    /// Arousal (activation): `-1.0` torpid, `+1.0` frenzied.
    pub arousal: f32,
    /// Dominance (agency): `-1.0` submissive, `+1.0` commanding.
    pub dominance: f32,
    /// Per-family intensities.
    pub families: FamilyVec,
    /// Baseline this state decays toward (derived from temperament
    /// at construction time and cached).
    pub baseline: Baseline,
}

/// Decay target derived from a [`Temperament`].
///
/// Stored on [`EmotionState`] so decay ticks don't need to walk back
/// to the owning NPC's temperament.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub pleasure: f32,
    pub arousal: f32,
    pub dominance: f32,
    /// Decay half-life (seconds of game time) for PAD and family
    /// intensities.
    pub half_life_secs: f32,
}

impl Default for Baseline {
    fn default() -> Self {
        Self {
            pleasure: 0.0,
            arousal: -0.2, // a resting adult is slightly under-aroused
            dominance: 0.0,
            half_life_secs: 3600.0, // 1 game-hour
        }
    }
}

impl Default for EmotionState {
    /// A neutral, low-arousal state with a default temperament.
    fn default() -> Self {
        Self {
            pleasure: 0.0,
            arousal: -0.2,
            dominance: 0.0,
            families: FamilyVec::default(),
            baseline: Baseline::default(),
        }
    }
}

/// A signed nudge to an emotion family, reported either by the LLM
/// (as part of a Tier 1/2/3 JSON response) or constructed in-engine
/// by Tier 4 rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionImpulse {
    /// Which family to nudge.
    pub family: EmotionFamily,
    /// Signed delta. Clamped to `[-0.5, 0.5]` on apply before
    /// reactivity scaling.
    pub delta: f32,
    /// Optional PAD nudge `(pleasure, arousal, dominance)`. Usually
    /// `None` — PAD is recomputed from the dominant family after apply.
    #[serde(default)]
    pub pad: Option<(f32, f32, f32)>,
    /// Optional one-line reason for the impulse. Useful for the
    /// "triggered by:" line in dialogue preambles.
    #[serde(default)]
    pub cause: Option<String>,
}

/// Non-linear behaviour bits derived from state.
///
/// These are the headline "gates" from the paper's findings: moderate
/// emotion produces one effect, extreme emotion a qualitatively
/// different one.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EmotionGates {
    /// `fear > 0.9`: the NPC will say true things they'd normally
    /// hide — "panic truth-telling".
    pub panic_truth: bool,
    /// `anger ∈ [0.5, 0.85]`: aggressive, tight-lipped, prone to
    /// confrontational choices.
    pub public_outburst: bool,
    /// `sadness > 0.8` OR `shame > 0.8`: withdraws, speaks little.
    pub withdraws_silent: bool,
    /// `joy > 0.85` OR (`affection > 0.7` AND `arousal > 0.5`):
    /// expansive, generous, verbose.
    pub effusive: bool,
}

impl EmotionState {
    /// Constructs an initial state from a [`Temperament`] and the
    /// legacy `mood` string from mod content.
    ///
    /// The mood string is parsed via simple substring heuristics
    /// (reusing the vocabulary from the existing `mood_emoji`
    /// mapping) to bias the family vector sensibly without requiring
    /// mod authors to hand-write PAD coordinates. Unknown strings
    /// produce a neutral state at the temperament's baseline.
    pub fn initial_from(temperament: &Temperament, legacy_mood: &str) -> Self {
        let baseline = Baseline {
            pleasure: temperament.baseline_pleasure(),
            arousal: -0.2,
            dominance: 0.0,
            half_life_secs: temperament.half_life_secs(),
        };

        let m = legacy_mood.to_lowercase();
        let mut families = FamilyVec::default();
        let mut pad = (baseline.pleasure, baseline.arousal, baseline.dominance);

        // Negative / intense
        if has_any(&m, &["angry", "furious", "enraged", "irate"]) {
            families.anger = 0.7;
            pad = (-0.3, 0.6, 0.5);
        } else if has_any(&m, &["afraid", "fearful", "terrified", "scared"]) {
            families.fear = 0.7;
            pad = (-0.5, 0.6, -0.6);
        } else if has_any(&m, &["anxious", "nervous", "worried", "uneasy"]) {
            families.fear = 0.4;
            pad = (-0.2, 0.3, -0.3);
        } else if has_any(&m, &["sad", "grief", "mournful", "sorrowful", "grieving"]) {
            families.sadness = 0.7;
            pad = (-0.6, -0.4, -0.3);
        } else if has_any(&m, &["melanchol", "wistful", "pensive", "nostalgic"]) {
            families.sadness = 0.3;
            pad = (-0.2, -0.3, 0.0);
        } else if has_any(&m, &["irritat", "frustrat", "annoyed", "grumpy"]) {
            families.anger = 0.4;
            pad = (-0.2, 0.3, 0.2);
        } else if has_any(&m, &["suspicious", "wary", "distrustful"]) {
            families.disgust = 0.3;
            families.fear = 0.2;
            pad = (-0.2, 0.1, 0.1);
        } else if has_any(&m, &["ashamed", "embarrass", "mortified", "shamed"]) {
            families.shame = 0.6;
            pad = (-0.4, 0.1, -0.5);
        }
        // Positive
        else if has_any(&m, &["joyful", "joy", "ecstatic", "elated", "delighted"]) {
            families.joy = 0.8;
            pad = (0.7, 0.6, 0.3);
        } else if has_any(&m, &["cheerful", "jovial", "merry", "jolly"]) {
            families.joy = 0.5;
            pad = (0.5, 0.3, 0.2);
        } else if has_any(&m, &["friendly", "welcoming", "hospitable", "warm"]) {
            families.affection = 0.5;
            families.joy = 0.3;
            pad = (0.4, 0.1, 0.1);
        } else if has_any(&m, &["amused", "laughing", "mirthful"]) {
            families.joy = 0.5;
            pad = (0.4, 0.4, 0.1);
        } else if has_any(&m, &["passionate", "fervent", "ardent"]) {
            families.affection = 0.5;
            pad = (0.4, 0.6, 0.3);
        }
        // Neutral / cognitive
        else if has_any(&m, &["contemplat", "thoughtful", "reflective", "ponder"]) {
            pad = (0.0, -0.1, 0.1);
        } else if has_any(&m, &["determined", "resolute", "steadfast"]) {
            pad = (0.1, 0.3, 0.5);
        } else if has_any(&m, &["alert", "watchful", "vigilant", "attentive"]) {
            pad = (0.0, 0.4, 0.1);
        } else if has_any(&m, &["calm", "serene", "peaceful", "tranquil"]) {
            pad = (0.3, -0.3, 0.1);
        } else if has_any(&m, &["content", "satisfied", "pleased"]) {
            // `pleasure` must clear the PAD "content" threshold
            // (0.3) so label() round-trips the string back. Keep
            // `joy` below 0.3 so we stay in the PAD branch rather
            // than jumping up to "cheerful".
            families.joy = 0.25;
            pad = (0.35, -0.1, 0.1);
        } else if has_any(&m, &["restless", "agitated", "fidgety"]) {
            pad = (-0.1, 0.5, 0.0);
            families.fear = 0.2;
        } else if has_any(
            &m,
            &["excited", "thrilled", "energetic", "eager", "boisterous"],
        ) {
            families.joy = 0.25;
            pad = (0.4, 0.6, 0.3);
        } else if has_any(&m, &["indignant", "outraged"]) {
            families.anger = 0.55;
            pad = (-0.4, 0.6, 0.55);
        } else if has_any(&m, &["tired", "weary", "exhausted", "sleepy"]) {
            pad = (-0.1, -0.6, -0.2);
        } else if has_any(&m, &["stoic", "guarded", "reserved", "neutral"]) {
            pad = (0.0, -0.1, 0.1);
        } else if has_any(&m, &["curious", "intrigued", "interested"]) {
            pad = (0.2, 0.2, 0.1);
            families.surprise = 0.2;
        } else if has_any(&m, &["shy", "bashful"]) {
            families.shame = 0.2;
            pad = (-0.1, 0.0, -0.4);
        } else if has_any(&m, &["proud", "smug", "self-satisfied"]) {
            pad = (0.4, 0.2, 0.6);
            families.joy = 0.3;
        } else if has_any(&m, &["surprised", "astonished", "shocked"]) {
            families.surprise = 0.6;
            pad = (0.0, 0.5, -0.1);
        } else if has_any(&m, &["unwell", "ill", "sick"]) {
            pad = (-0.3, -0.4, -0.4);
            families.sadness = 0.2;
        }

        Self {
            pleasure: pad.0,
            arousal: pad.1,
            dominance: pad.2,
            families,
            baseline,
        }
    }

    /// Applies an impulse to the relevant family, scaled by the
    /// caller's reactivity (typically `Temperament::reactivity`).
    ///
    /// The family delta is clamped to `[-0.5, 0.5]`, scaled, then
    /// added and re-clamped to `[0.0, 1.0]`. If the impulse carries
    /// explicit PAD, that is added directly (and clamped); otherwise
    /// PAD is nudged proportionally to the family's typical
    /// valence/arousal signature.
    pub fn apply_impulse(&mut self, impulse: &EmotionImpulse, reactivity: f32) {
        let clamped_delta = impulse.delta.clamp(-0.5, 0.5);
        let scaled = clamped_delta * reactivity.clamp(0.0, 1.0);

        let slot = self.families.get_mut(impulse.family);
        *slot = (*slot + scaled).clamp(0.0, 1.0);

        if let Some((dp, da, dd)) = impulse.pad {
            self.pleasure = (self.pleasure + dp * reactivity).clamp(-1.0, 1.0);
            self.arousal = (self.arousal + da * reactivity).clamp(-1.0, 1.0);
            self.dominance = (self.dominance + dd * reactivity).clamp(-1.0, 1.0);
        } else {
            let (dp, da, dd) = family_pad_signature(impulse.family);
            self.pleasure = (self.pleasure + dp * scaled.abs()).clamp(-1.0, 1.0);
            self.arousal = (self.arousal + da * scaled.abs()).clamp(-1.0, 1.0);
            self.dominance = (self.dominance + dd * scaled.abs()).clamp(-1.0, 1.0);
        }
    }

    /// Exponentially decays PAD toward baseline and family
    /// intensities toward zero over `dt_secs` of game time.
    ///
    /// Uses the baseline's `half_life_secs`: after one half-life, the
    /// distance to baseline halves.
    pub fn decay(&mut self, dt_secs: f32) {
        if dt_secs <= 0.0 || self.baseline.half_life_secs <= 0.0 {
            return;
        }
        let factor = 0.5f32.powf(dt_secs / self.baseline.half_life_secs);

        self.pleasure = lerp(self.baseline.pleasure, self.pleasure, factor);
        self.arousal = lerp(self.baseline.arousal, self.arousal, factor);
        self.dominance = lerp(self.baseline.dominance, self.dominance, factor);

        for fam in EmotionFamily::ALL.iter().copied() {
            let slot = self.families.get_mut(fam);
            *slot = lerp(0.0, *slot, factor);
            if *slot < 0.001 {
                *slot = 0.0;
            }
        }
    }

    /// Returns the dominant (family, intensity) pair.
    pub fn dominant_family(&self) -> (EmotionFamily, f32) {
        self.families.dominant()
    }

    /// Returns non-linear behaviour gates for the current state.
    pub fn gates(&self) -> EmotionGates {
        let f = &self.families;
        EmotionGates {
            panic_truth: f.fear > 0.9,
            public_outburst: f.anger > 0.5 && f.anger <= 0.85,
            withdraws_silent: f.sadness > 0.8 || f.shame > 0.8,
            effusive: f.joy > 0.85 || (f.affection > 0.7 && self.arousal > 0.5),
        }
    }

    /// Returns a one-word label suitable for the legacy `Npc.mood`
    /// field and the emoji-mapping path.
    ///
    /// Prefers extremes over baseline: if a family is above 0.6, its
    /// label wins; otherwise a PAD-derived label is returned.
    pub fn label(&self) -> &'static str {
        let (fam, intensity) = self.dominant_family();
        if intensity > 0.85 {
            return match fam {
                EmotionFamily::Joy => "ecstatic",
                EmotionFamily::Sadness => "grieving",
                EmotionFamily::Fear => "terrified",
                EmotionFamily::Anger => "furious",
                EmotionFamily::Disgust => "repulsed",
                EmotionFamily::Surprise => "astonished",
                EmotionFamily::Shame => "mortified",
                EmotionFamily::Affection => "enraptured",
            };
        }
        if intensity > 0.6 {
            return match fam {
                EmotionFamily::Joy => "joyful",
                EmotionFamily::Sadness => "sorrowful",
                EmotionFamily::Fear => "afraid",
                EmotionFamily::Anger => "angry",
                EmotionFamily::Disgust => "disgusted",
                EmotionFamily::Surprise => "surprised",
                EmotionFamily::Shame => "ashamed",
                EmotionFamily::Affection => "affectionate",
            };
        }
        if intensity > 0.3 {
            return match fam {
                EmotionFamily::Joy => "cheerful",
                EmotionFamily::Sadness => "melancholy",
                EmotionFamily::Fear => "anxious",
                EmotionFamily::Anger => "irritated",
                EmotionFamily::Disgust => "suspicious",
                EmotionFamily::Surprise => "curious",
                EmotionFamily::Shame => "shy",
                EmotionFamily::Affection => "warm",
            };
        }
        // Fall through to PAD-derived label
        if self.pleasure > 0.3 && self.arousal < 0.2 {
            "content"
        } else if self.pleasure > 0.3 {
            "cheerful"
        } else if self.pleasure < -0.3 && self.arousal < -0.2 {
            "weary"
        } else if self.pleasure < -0.3 {
            "troubled"
        } else if self.arousal > 0.3 {
            "alert"
        } else if self.arousal < -0.3 {
            "tired"
        } else {
            "calm"
        }
    }

    /// Short descriptor suitable for Tier 2/3 batch prompts (one
    /// phrase, no preamble).
    pub fn short_descriptor(&self) -> String {
        let gates = self.gates();
        let label = self.label();
        if gates.panic_truth {
            format!("{label}, panic-stricken")
        } else if gates.public_outburst {
            format!("{label}, simmering")
        } else if gates.withdraws_silent {
            format!("{label}, withdrawn")
        } else if gates.effusive {
            format!("{label}, expansive")
        } else {
            label.to_string()
        }
    }

    /// Multi-sentence prose suitable for the Tier 1 system prompt
    /// preamble. Mirrors the shape of
    /// `Intelligence::prompt_guidance()` in `parish-npc`.
    pub fn prompt_guidance(&self) -> String {
        let (fam, intensity) = self.dominant_family();
        let gates = self.gates();

        let mut out = String::new();
        out.push_str("Emotional state: ");
        out.push_str(self.label());
        out.push('.');

        // Family-specific embodied cue.
        if intensity > 0.3 {
            out.push(' ');
            out.push_str(match fam {
                EmotionFamily::Joy => {
                    "There is a lightness in your chest; small things seem easier."
                }
                EmotionFamily::Sadness => {
                    "A leaden heaviness sits in your chest; ordinary talk feels unreal."
                }
                EmotionFamily::Fear => {
                    "Your senses are sharpened; you watch the room's edges without meaning to."
                }
                EmotionFamily::Anger => {
                    "Your jaw is tight and your sentences come shorter than you mean."
                }
                EmotionFamily::Disgust => "You find yourself pulling back, wanting distance.",
                EmotionFamily::Surprise => "Your pulse is quick; the world feels newly strange.",
                EmotionFamily::Shame => {
                    "You cannot meet eyes easily; your voice wants to go small."
                }
                EmotionFamily::Affection => "There is a warmth toward the person in front of you.",
            });
        }

        // Non-linear gates override ordinary behaviour.
        if gates.panic_truth {
            out.push_str(
                " You are so frightened right now that you will say true things \
                          you would normally hide. If asked a direct question, your first \
                          instinct is to answer honestly, even against your interest.",
            );
        } else if gates.public_outburst {
            out.push_str(
                " You will not back down from a challenge right now, even a small one. \
                          You are close to snapping.",
            );
        } else if gates.withdraws_silent {
            out.push_str(
                " You do not want to speak much. Short replies. Long silences feel right.",
            );
        } else if gates.effusive {
            out.push_str(" You feel expansive — words come easily, you want to share.");
        }

        out
    }
}

/// Typical PAD direction for a family. Used to nudge PAD when an
/// impulse doesn't carry explicit PAD coordinates.
fn family_pad_signature(family: EmotionFamily) -> (f32, f32, f32) {
    match family {
        EmotionFamily::Joy => (0.8, 0.4, 0.3),
        EmotionFamily::Sadness => (-0.7, -0.3, -0.3),
        EmotionFamily::Fear => (-0.6, 0.6, -0.6),
        EmotionFamily::Anger => (-0.4, 0.7, 0.5),
        EmotionFamily::Disgust => (-0.5, 0.2, 0.2),
        EmotionFamily::Surprise => (0.0, 0.6, -0.1),
        EmotionFamily::Shame => (-0.5, 0.1, -0.6),
        EmotionFamily::Affection => (0.6, 0.2, 0.1),
    }
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn lerp(from: f32, to: f32, t: f32) -> f32 {
    from + (to - from) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_neutral() {
        let s = EmotionState::default();
        assert_eq!(s.pleasure, 0.0);
        assert_eq!(s.families.dominant().1, 0.0);
        assert_eq!(s.label(), "calm");
    }

    #[test]
    fn initial_from_legacy_mood_content() {
        let t = Temperament::default();
        let s = EmotionState::initial_from(&t, "content");
        assert!(s.pleasure > 0.0, "content should have positive pleasure");
        assert!(s.arousal < 0.2, "content should be low-arousal");
        assert_eq!(s.families.dominant().0, EmotionFamily::Joy);
    }

    #[test]
    fn initial_from_legacy_mood_furious() {
        let t = Temperament::default();
        let s = EmotionState::initial_from(&t, "furious");
        assert_eq!(s.families.dominant().0, EmotionFamily::Anger);
        assert!(s.families.anger > 0.5);
        assert!(s.arousal > 0.3);
    }

    #[test]
    fn initial_from_unknown_mood_uses_baseline() {
        let t = Temperament::default();
        let s = EmotionState::initial_from(&t, "xyzzy");
        assert_eq!(s.pleasure, t.baseline_pleasure());
        assert_eq!(s.families.dominant().1, 0.0);
    }

    #[test]
    fn initial_from_respects_cheerful_temperament() {
        let t = Temperament {
            cheerfulness: 1.0,
            ..Default::default()
        };
        let s = EmotionState::initial_from(&t, "xyzzy");
        assert!(
            s.pleasure > 0.0,
            "cheerful temperament should produce positive baseline pleasure"
        );
    }

    #[test]
    fn apply_impulse_clamps_family_to_one() {
        let mut s = EmotionState::default();
        s.families.anger = 0.9;
        let imp = EmotionImpulse {
            family: EmotionFamily::Anger,
            delta: 10.0, // absurdly large, should clamp before apply
            pad: None,
            cause: None,
        };
        s.apply_impulse(&imp, 1.0);
        assert!(s.families.anger <= 1.0);
        assert!(s.families.anger > 0.9);
    }

    #[test]
    fn apply_impulse_scales_by_reactivity() {
        let mut placid = EmotionState::default();
        let mut volatile = EmotionState::default();
        let imp = EmotionImpulse {
            family: EmotionFamily::Fear,
            delta: 0.4,
            pad: None,
            cause: None,
        };
        placid.apply_impulse(&imp, 0.1);
        volatile.apply_impulse(&imp, 1.0);
        assert!(volatile.families.fear > placid.families.fear * 5.0);
    }

    #[test]
    fn decay_half_life_halves_distance_to_baseline() {
        let baseline = Baseline {
            pleasure: 0.0,
            arousal: -0.2,
            dominance: 0.0,
            half_life_secs: 100.0,
        };
        let mut s = EmotionState {
            pleasure: 0.8,
            arousal: 0.6,
            dominance: 0.4,
            families: FamilyVec {
                anger: 0.8,
                ..Default::default()
            },
            baseline,
        };
        s.decay(100.0);
        // Pleasure: 0.8 -> 0.4 (halfway to 0.0)
        assert!((s.pleasure - 0.4).abs() < 0.001, "got {}", s.pleasure);
        // Anger: 0.8 -> 0.4 (halfway to 0.0)
        assert!(
            (s.families.anger - 0.4).abs() < 0.001,
            "got {}",
            s.families.anger
        );
    }

    #[test]
    fn decay_with_zero_dt_is_noop() {
        let before = EmotionState {
            pleasure: 0.5,
            ..Default::default()
        };
        let mut after = before;
        after.decay(0.0);
        assert_eq!(before, after);
    }

    #[test]
    fn gates_moderate_anger_triggers_outburst_not_truth() {
        let mut s = EmotionState::default();
        s.families.anger = 0.7;
        let g = s.gates();
        assert!(g.public_outburst, "moderate anger should trigger outburst");
        assert!(!g.panic_truth, "anger alone should not trigger panic_truth");
    }

    #[test]
    fn gates_extreme_fear_triggers_panic_truth() {
        let mut s = EmotionState::default();
        s.families.fear = 0.95;
        let g = s.gates();
        assert!(g.panic_truth, "extreme fear should trigger panic_truth");
    }

    #[test]
    fn gates_extreme_anger_does_not_trigger_outburst() {
        // Mirrors the paper's finding: past a threshold, anger
        // qualitatively changes behaviour. Above 0.85 we leave the
        // outburst band — callers should layer a separate "discloses
        // under fury" path if they want to model that.
        let mut s = EmotionState::default();
        s.families.anger = 0.95;
        let g = s.gates();
        assert!(!g.public_outburst);
    }

    #[test]
    fn gates_high_sadness_triggers_withdrawal() {
        let mut s = EmotionState::default();
        s.families.sadness = 0.9;
        let g = s.gates();
        assert!(g.withdraws_silent);
    }

    #[test]
    fn label_tracks_dominant_family_at_high_intensity() {
        let mut s = EmotionState::default();
        s.families.sadness = 0.9;
        assert_eq!(s.label(), "grieving");
        s.families.sadness = 0.7;
        assert_eq!(s.label(), "sorrowful");
        s.families.sadness = 0.4;
        assert_eq!(s.label(), "melancholy");
    }

    #[test]
    fn short_descriptor_includes_gate_hint() {
        let mut s = EmotionState::default();
        s.families.fear = 0.95;
        assert!(s.short_descriptor().contains("panic"));
    }

    #[test]
    fn prompt_guidance_mentions_gate_language_under_panic() {
        let mut s = EmotionState::default();
        s.families.fear = 0.95;
        let g = s.prompt_guidance();
        assert!(g.contains("frightened"));
        assert!(
            g.contains("true things"),
            "panic_truth gate text should appear: {g}"
        );
    }

    #[test]
    fn serde_round_trip_preserves_state() {
        let original = EmotionState {
            pleasure: 0.2,
            arousal: 0.4,
            dominance: -0.1,
            families: FamilyVec {
                joy: 0.3,
                sadness: 0.1,
                fear: 0.0,
                anger: 0.5,
                disgust: 0.0,
                surprise: 0.2,
                shame: 0.0,
                affection: 0.4,
            },
            baseline: Baseline::default(),
        };
        let json = serde_json::to_string(&original).unwrap();
        let roundtripped: EmotionState = serde_json::from_str(&json).unwrap();
        assert_eq!(original, roundtripped);
    }

    #[test]
    fn temperament_defaults_parse_from_empty_json() {
        // Confirms #[serde(default)] covers every field so mod authors
        // can supply a partial temperament block.
        let t: Temperament = serde_json::from_str("{}").unwrap();
        assert_eq!(t, Temperament::default());
    }

    #[test]
    fn temperament_half_life_monotone_in_persistence() {
        let low = Temperament {
            persistence: 0.0,
            ..Default::default()
        };
        let high = Temperament {
            persistence: 1.0,
            ..Default::default()
        };
        assert!(high.half_life_secs() > low.half_life_secs());
    }
}
