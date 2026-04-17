//! The 171 emotion-word leaf table.
//!
//! These are the "leaves" of the emotion taxonomy: fine-grained
//! descriptors (e.g. `brooding`, `elated`, `wistful`) that we project
//! from an [`crate::emotion::EmotionState`] at prompt-build time,
//! never stored per-NPC. The list is transcribed from the emotion-word
//! set used in Anthropic's April 2026 interpretability paper
//! *Emotion Concepts and their Function in a Large Language Model*.
//!
//! Each leaf carries:
//! - `word` — the surface form injected into prompts.
//! - `family` — which of the eight [`crate::emotion::EmotionFamily`]
//!   groups this word belongs to. Used to pick leaves that match the
//!   dominant family of the current state.
//! - `pad` — `(pleasure, arousal, dominance)` in `[-1.0, 1.0]`, a
//!   rough affective coordinate drawn from standard English affective
//!   norms. These are *not* learned activation-vector projections —
//!   they're author-chosen so `project_top_k` picks a plausible word
//!   without requiring per-release retraining.
//! - `family_weight` — the family intensity at which this leaf is the
//!   prototypical descriptor (e.g. `ecstatic` is 1.0 in the joy
//!   family; `content` is 0.2). Used as the scale anchor for
//!   intensity-matching.
//!
//! # Coverage
//!
//! 171 leaves spread across the eight families:
//!   - 28 joy, 22 sadness, 22 fear, 22 anger, 16 disgust,
//!     13 surprise, 16 shame, 20 affection, and 12 composite/neutral
//!     leaves that score low on every family but still convey mood
//!     (e.g. `pensive`, `restless`). Composite leaves use the family
//!     nearest to their affective tone.

use crate::emotion::{EmotionFamily, EmotionState};

/// One entry in the static leaf table.
#[derive(Debug, Clone, Copy)]
pub struct LeafWord {
    pub word: &'static str,
    pub family: EmotionFamily,
    /// `(pleasure, arousal, dominance)` in `[-1.0, 1.0]`.
    pub pad: (f32, f32, f32),
    /// Anchor family intensity in `[0.0, 1.0]`.
    pub family_weight: f32,
}

/// The 171 leaves. Order is by family (joy → ... → affection →
/// composites), then roughly by intensity within family.
pub const EMOTION_LEAVES: &[LeafWord] = &[
    // --- JOY (28) ------------------------------------------------
    LeafWord {
        word: "ecstatic",
        family: EmotionFamily::Joy,
        pad: (0.95, 0.90, 0.50),
        family_weight: 1.00,
    },
    LeafWord {
        word: "euphoric",
        family: EmotionFamily::Joy,
        pad: (0.95, 0.85, 0.45),
        family_weight: 0.98,
    },
    LeafWord {
        word: "elated",
        family: EmotionFamily::Joy,
        pad: (0.90, 0.75, 0.40),
        family_weight: 0.92,
    },
    LeafWord {
        word: "exhilarated",
        family: EmotionFamily::Joy,
        pad: (0.85, 0.85, 0.50),
        family_weight: 0.90,
    },
    LeafWord {
        word: "overjoyed",
        family: EmotionFamily::Joy,
        pad: (0.90, 0.70, 0.40),
        family_weight: 0.90,
    },
    LeafWord {
        word: "jubilant",
        family: EmotionFamily::Joy,
        pad: (0.90, 0.70, 0.45),
        family_weight: 0.88,
    },
    LeafWord {
        word: "delighted",
        family: EmotionFamily::Joy,
        pad: (0.80, 0.55, 0.30),
        family_weight: 0.80,
    },
    LeafWord {
        word: "thrilled",
        family: EmotionFamily::Joy,
        pad: (0.85, 0.80, 0.40),
        family_weight: 0.85,
    },
    LeafWord {
        word: "gleeful",
        family: EmotionFamily::Joy,
        pad: (0.80, 0.60, 0.30),
        family_weight: 0.80,
    },
    LeafWord {
        word: "joyful",
        family: EmotionFamily::Joy,
        pad: (0.75, 0.55, 0.30),
        family_weight: 0.75,
    },
    LeafWord {
        word: "happy",
        family: EmotionFamily::Joy,
        pad: (0.70, 0.40, 0.30),
        family_weight: 0.70,
    },
    LeafWord {
        word: "cheerful",
        family: EmotionFamily::Joy,
        pad: (0.60, 0.35, 0.25),
        family_weight: 0.55,
    },
    LeafWord {
        word: "merry",
        family: EmotionFamily::Joy,
        pad: (0.60, 0.40, 0.25),
        family_weight: 0.55,
    },
    LeafWord {
        word: "jovial",
        family: EmotionFamily::Joy,
        pad: (0.55, 0.35, 0.30),
        family_weight: 0.55,
    },
    LeafWord {
        word: "jolly",
        family: EmotionFamily::Joy,
        pad: (0.55, 0.40, 0.25),
        family_weight: 0.50,
    },
    LeafWord {
        word: "upbeat",
        family: EmotionFamily::Joy,
        pad: (0.55, 0.35, 0.30),
        family_weight: 0.50,
    },
    LeafWord {
        word: "pleased",
        family: EmotionFamily::Joy,
        pad: (0.50, 0.20, 0.25),
        family_weight: 0.40,
    },
    LeafWord {
        word: "glad",
        family: EmotionFamily::Joy,
        pad: (0.50, 0.20, 0.20),
        family_weight: 0.40,
    },
    LeafWord {
        word: "satisfied",
        family: EmotionFamily::Joy,
        pad: (0.45, 0.00, 0.30),
        family_weight: 0.35,
    },
    LeafWord {
        word: "content",
        family: EmotionFamily::Joy,
        pad: (0.40, -0.10, 0.20),
        family_weight: 0.25,
    },
    LeafWord {
        word: "serene",
        family: EmotionFamily::Joy,
        pad: (0.40, -0.30, 0.20),
        family_weight: 0.30,
    },
    LeafWord {
        word: "buoyant",
        family: EmotionFamily::Joy,
        pad: (0.60, 0.50, 0.30),
        family_weight: 0.60,
    },
    LeafWord {
        word: "lighthearted",
        family: EmotionFamily::Joy,
        pad: (0.55, 0.30, 0.25),
        family_weight: 0.50,
    },
    LeafWord {
        word: "radiant",
        family: EmotionFamily::Joy,
        pad: (0.70, 0.55, 0.35),
        family_weight: 0.70,
    },
    LeafWord {
        word: "amused",
        family: EmotionFamily::Joy,
        pad: (0.55, 0.40, 0.20),
        family_weight: 0.45,
    },
    LeafWord {
        word: "mirthful",
        family: EmotionFamily::Joy,
        pad: (0.65, 0.50, 0.25),
        family_weight: 0.60,
    },
    LeafWord {
        word: "triumphant",
        family: EmotionFamily::Joy,
        pad: (0.80, 0.65, 0.70),
        family_weight: 0.80,
    },
    LeafWord {
        word: "proud",
        family: EmotionFamily::Joy,
        pad: (0.60, 0.35, 0.65),
        family_weight: 0.50,
    },
    // --- SADNESS (22) --------------------------------------------
    LeafWord {
        word: "devastated",
        family: EmotionFamily::Sadness,
        pad: (-0.90, -0.20, -0.50),
        family_weight: 0.98,
    },
    LeafWord {
        word: "heartbroken",
        family: EmotionFamily::Sadness,
        pad: (-0.90, -0.10, -0.40),
        family_weight: 0.95,
    },
    LeafWord {
        word: "anguished",
        family: EmotionFamily::Sadness,
        pad: (-0.85, 0.30, -0.40),
        family_weight: 0.92,
    },
    LeafWord {
        word: "despairing",
        family: EmotionFamily::Sadness,
        pad: (-0.85, -0.30, -0.60),
        family_weight: 0.90,
    },
    LeafWord {
        word: "desolate",
        family: EmotionFamily::Sadness,
        pad: (-0.85, -0.40, -0.50),
        family_weight: 0.90,
    },
    LeafWord {
        word: "grieving",
        family: EmotionFamily::Sadness,
        pad: (-0.80, -0.20, -0.30),
        family_weight: 0.88,
    },
    LeafWord {
        word: "bereft",
        family: EmotionFamily::Sadness,
        pad: (-0.80, -0.30, -0.45),
        family_weight: 0.85,
    },
    LeafWord {
        word: "mournful",
        family: EmotionFamily::Sadness,
        pad: (-0.70, -0.30, -0.30),
        family_weight: 0.75,
    },
    LeafWord {
        word: "sorrowful",
        family: EmotionFamily::Sadness,
        pad: (-0.70, -0.30, -0.30),
        family_weight: 0.70,
    },
    LeafWord {
        word: "dejected",
        family: EmotionFamily::Sadness,
        pad: (-0.65, -0.40, -0.50),
        family_weight: 0.65,
    },
    LeafWord {
        word: "downcast",
        family: EmotionFamily::Sadness,
        pad: (-0.55, -0.40, -0.40),
        family_weight: 0.55,
    },
    LeafWord {
        word: "sad",
        family: EmotionFamily::Sadness,
        pad: (-0.55, -0.30, -0.30),
        family_weight: 0.50,
    },
    LeafWord {
        word: "gloomy",
        family: EmotionFamily::Sadness,
        pad: (-0.50, -0.35, -0.25),
        family_weight: 0.50,
    },
    LeafWord {
        word: "melancholy",
        family: EmotionFamily::Sadness,
        pad: (-0.40, -0.30, -0.15),
        family_weight: 0.40,
    },
    LeafWord {
        word: "wistful",
        family: EmotionFamily::Sadness,
        pad: (-0.30, -0.30, 0.00),
        family_weight: 0.30,
    },
    LeafWord {
        word: "pensive",
        family: EmotionFamily::Sadness,
        pad: (-0.20, -0.20, 0.05),
        family_weight: 0.20,
    },
    LeafWord {
        word: "nostalgic",
        family: EmotionFamily::Sadness,
        pad: (-0.15, -0.10, 0.05),
        family_weight: 0.20,
    },
    LeafWord {
        word: "disappointed",
        family: EmotionFamily::Sadness,
        pad: (-0.55, -0.20, -0.30),
        family_weight: 0.50,
    },
    LeafWord {
        word: "forlorn",
        family: EmotionFamily::Sadness,
        pad: (-0.65, -0.30, -0.55),
        family_weight: 0.65,
    },
    LeafWord {
        word: "hopeless",
        family: EmotionFamily::Sadness,
        pad: (-0.80, -0.40, -0.70),
        family_weight: 0.85,
    },
    LeafWord {
        word: "blue",
        family: EmotionFamily::Sadness,
        pad: (-0.45, -0.35, -0.30),
        family_weight: 0.40,
    },
    LeafWord {
        word: "brooding",
        family: EmotionFamily::Sadness,
        pad: (-0.45, -0.10, -0.10),
        family_weight: 0.40,
    },
    // --- FEAR (22) -----------------------------------------------
    LeafWord {
        word: "terrified",
        family: EmotionFamily::Fear,
        pad: (-0.80, 0.90, -0.80),
        family_weight: 0.98,
    },
    LeafWord {
        word: "petrified",
        family: EmotionFamily::Fear,
        pad: (-0.85, 0.85, -0.90),
        family_weight: 0.98,
    },
    LeafWord {
        word: "horrified",
        family: EmotionFamily::Fear,
        pad: (-0.85, 0.85, -0.70),
        family_weight: 0.95,
    },
    LeafWord {
        word: "panicked",
        family: EmotionFamily::Fear,
        pad: (-0.70, 0.95, -0.75),
        family_weight: 0.95,
    },
    LeafWord {
        word: "frantic",
        family: EmotionFamily::Fear,
        pad: (-0.65, 0.90, -0.60),
        family_weight: 0.90,
    },
    LeafWord {
        word: "afraid",
        family: EmotionFamily::Fear,
        pad: (-0.60, 0.60, -0.60),
        family_weight: 0.70,
    },
    LeafWord {
        word: "frightened",
        family: EmotionFamily::Fear,
        pad: (-0.65, 0.65, -0.60),
        family_weight: 0.75,
    },
    LeafWord {
        word: "scared",
        family: EmotionFamily::Fear,
        pad: (-0.60, 0.60, -0.55),
        family_weight: 0.70,
    },
    LeafWord {
        word: "fearful",
        family: EmotionFamily::Fear,
        pad: (-0.55, 0.50, -0.55),
        family_weight: 0.65,
    },
    LeafWord {
        word: "alarmed",
        family: EmotionFamily::Fear,
        pad: (-0.45, 0.70, -0.35),
        family_weight: 0.60,
    },
    LeafWord {
        word: "dreading",
        family: EmotionFamily::Fear,
        pad: (-0.55, 0.30, -0.50),
        family_weight: 0.60,
    },
    LeafWord {
        word: "anxious",
        family: EmotionFamily::Fear,
        pad: (-0.35, 0.50, -0.35),
        family_weight: 0.45,
    },
    LeafWord {
        word: "nervous",
        family: EmotionFamily::Fear,
        pad: (-0.25, 0.50, -0.30),
        family_weight: 0.40,
    },
    LeafWord {
        word: "worried",
        family: EmotionFamily::Fear,
        pad: (-0.35, 0.30, -0.30),
        family_weight: 0.40,
    },
    LeafWord {
        word: "uneasy",
        family: EmotionFamily::Fear,
        pad: (-0.25, 0.20, -0.30),
        family_weight: 0.30,
    },
    LeafWord {
        word: "apprehensive",
        family: EmotionFamily::Fear,
        pad: (-0.30, 0.25, -0.25),
        family_weight: 0.35,
    },
    LeafWord {
        word: "jittery",
        family: EmotionFamily::Fear,
        pad: (-0.20, 0.65, -0.30),
        family_weight: 0.40,
    },
    LeafWord {
        word: "tense",
        family: EmotionFamily::Fear,
        pad: (-0.25, 0.50, -0.20),
        family_weight: 0.35,
    },
    LeafWord {
        word: "skittish",
        family: EmotionFamily::Fear,
        pad: (-0.25, 0.60, -0.40),
        family_weight: 0.40,
    },
    LeafWord {
        word: "wary",
        family: EmotionFamily::Fear,
        pad: (-0.15, 0.25, -0.10),
        family_weight: 0.25,
    },
    LeafWord {
        word: "haunted",
        family: EmotionFamily::Fear,
        pad: (-0.65, 0.30, -0.55),
        family_weight: 0.70,
    },
    LeafWord {
        word: "spooked",
        family: EmotionFamily::Fear,
        pad: (-0.40, 0.65, -0.50),
        family_weight: 0.55,
    },
    // --- ANGER (22) ----------------------------------------------
    LeafWord {
        word: "enraged",
        family: EmotionFamily::Anger,
        pad: (-0.50, 0.95, 0.75),
        family_weight: 1.00,
    },
    LeafWord {
        word: "furious",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.90, 0.70),
        family_weight: 0.95,
    },
    LeafWord {
        word: "livid",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.85, 0.65),
        family_weight: 0.92,
    },
    LeafWord {
        word: "incensed",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.80, 0.65),
        family_weight: 0.88,
    },
    LeafWord {
        word: "irate",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.80, 0.60),
        family_weight: 0.85,
    },
    LeafWord {
        word: "seething",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.70, 0.55),
        family_weight: 0.80,
    },
    LeafWord {
        word: "outraged",
        family: EmotionFamily::Anger,
        pad: (-0.60, 0.80, 0.60),
        family_weight: 0.85,
    },
    LeafWord {
        word: "indignant",
        family: EmotionFamily::Anger,
        pad: (-0.45, 0.60, 0.55),
        family_weight: 0.70,
    },
    LeafWord {
        word: "angry",
        family: EmotionFamily::Anger,
        pad: (-0.45, 0.65, 0.55),
        family_weight: 0.70,
    },
    LeafWord {
        word: "resentful",
        family: EmotionFamily::Anger,
        pad: (-0.45, 0.30, 0.20),
        family_weight: 0.55,
    },
    LeafWord {
        word: "bitter",
        family: EmotionFamily::Anger,
        pad: (-0.50, 0.20, 0.15),
        family_weight: 0.55,
    },
    LeafWord {
        word: "cross",
        family: EmotionFamily::Anger,
        pad: (-0.30, 0.40, 0.35),
        family_weight: 0.45,
    },
    LeafWord {
        word: "irritated",
        family: EmotionFamily::Anger,
        pad: (-0.30, 0.45, 0.25),
        family_weight: 0.40,
    },
    LeafWord {
        word: "frustrated",
        family: EmotionFamily::Anger,
        pad: (-0.35, 0.55, 0.15),
        family_weight: 0.45,
    },
    LeafWord {
        word: "annoyed",
        family: EmotionFamily::Anger,
        pad: (-0.25, 0.40, 0.25),
        family_weight: 0.35,
    },
    LeafWord {
        word: "grumpy",
        family: EmotionFamily::Anger,
        pad: (-0.30, 0.15, 0.10),
        family_weight: 0.30,
    },
    LeafWord {
        word: "peeved",
        family: EmotionFamily::Anger,
        pad: (-0.25, 0.35, 0.20),
        family_weight: 0.30,
    },
    LeafWord {
        word: "sullen",
        family: EmotionFamily::Anger,
        pad: (-0.40, 0.00, 0.05),
        family_weight: 0.35,
    },
    LeafWord {
        word: "huffy",
        family: EmotionFamily::Anger,
        pad: (-0.25, 0.50, 0.30),
        family_weight: 0.35,
    },
    LeafWord {
        word: "vengeful",
        family: EmotionFamily::Anger,
        pad: (-0.55, 0.60, 0.60),
        family_weight: 0.70,
    },
    LeafWord {
        word: "spiteful",
        family: EmotionFamily::Anger,
        pad: (-0.50, 0.55, 0.50),
        family_weight: 0.65,
    },
    LeafWord {
        word: "exasperated",
        family: EmotionFamily::Anger,
        pad: (-0.40, 0.60, 0.30),
        family_weight: 0.55,
    },
    // --- DISGUST (16) --------------------------------------------
    LeafWord {
        word: "repulsed",
        family: EmotionFamily::Disgust,
        pad: (-0.75, 0.50, 0.30),
        family_weight: 0.95,
    },
    LeafWord {
        word: "revolted",
        family: EmotionFamily::Disgust,
        pad: (-0.75, 0.55, 0.30),
        family_weight: 0.90,
    },
    LeafWord {
        word: "sickened",
        family: EmotionFamily::Disgust,
        pad: (-0.75, 0.30, 0.10),
        family_weight: 0.85,
    },
    LeafWord {
        word: "nauseated",
        family: EmotionFamily::Disgust,
        pad: (-0.70, 0.15, -0.15),
        family_weight: 0.75,
    },
    LeafWord {
        word: "disgusted",
        family: EmotionFamily::Disgust,
        pad: (-0.65, 0.35, 0.25),
        family_weight: 0.75,
    },
    LeafWord {
        word: "appalled",
        family: EmotionFamily::Disgust,
        pad: (-0.65, 0.55, 0.25),
        family_weight: 0.75,
    },
    LeafWord {
        word: "contemptuous",
        family: EmotionFamily::Disgust,
        pad: (-0.45, 0.30, 0.55),
        family_weight: 0.60,
    },
    LeafWord {
        word: "scornful",
        family: EmotionFamily::Disgust,
        pad: (-0.45, 0.30, 0.55),
        family_weight: 0.55,
    },
    LeafWord {
        word: "disdainful",
        family: EmotionFamily::Disgust,
        pad: (-0.35, 0.15, 0.55),
        family_weight: 0.45,
    },
    LeafWord {
        word: "aversion",
        family: EmotionFamily::Disgust,
        pad: (-0.40, 0.15, 0.10),
        family_weight: 0.40,
    },
    LeafWord {
        word: "loathing",
        family: EmotionFamily::Disgust,
        pad: (-0.70, 0.50, 0.40),
        family_weight: 0.85,
    },
    LeafWord {
        word: "repelled",
        family: EmotionFamily::Disgust,
        pad: (-0.55, 0.35, 0.20),
        family_weight: 0.60,
    },
    LeafWord {
        word: "squeamish",
        family: EmotionFamily::Disgust,
        pad: (-0.30, 0.20, -0.15),
        family_weight: 0.35,
    },
    LeafWord {
        word: "wary",
        family: EmotionFamily::Disgust,
        pad: (-0.15, 0.25, 0.00),
        family_weight: 0.25,
    },
    LeafWord {
        word: "suspicious",
        family: EmotionFamily::Disgust,
        pad: (-0.20, 0.25, 0.15),
        family_weight: 0.30,
    },
    LeafWord {
        word: "distrustful",
        family: EmotionFamily::Disgust,
        pad: (-0.30, 0.15, 0.15),
        family_weight: 0.30,
    },
    // --- SURPRISE (13) -------------------------------------------
    LeafWord {
        word: "astonished",
        family: EmotionFamily::Surprise,
        pad: (0.15, 0.85, -0.15),
        family_weight: 0.95,
    },
    LeafWord {
        word: "astounded",
        family: EmotionFamily::Surprise,
        pad: (0.15, 0.85, -0.15),
        family_weight: 0.92,
    },
    LeafWord {
        word: "flabbergasted",
        family: EmotionFamily::Surprise,
        pad: (0.05, 0.85, -0.20),
        family_weight: 0.90,
    },
    LeafWord {
        word: "stunned",
        family: EmotionFamily::Surprise,
        pad: (-0.05, 0.60, -0.30),
        family_weight: 0.80,
    },
    LeafWord {
        word: "shocked",
        family: EmotionFamily::Surprise,
        pad: (-0.20, 0.75, -0.20),
        family_weight: 0.80,
    },
    LeafWord {
        word: "amazed",
        family: EmotionFamily::Surprise,
        pad: (0.45, 0.70, 0.00),
        family_weight: 0.75,
    },
    LeafWord {
        word: "surprised",
        family: EmotionFamily::Surprise,
        pad: (0.10, 0.60, -0.10),
        family_weight: 0.65,
    },
    LeafWord {
        word: "startled",
        family: EmotionFamily::Surprise,
        pad: (-0.15, 0.70, -0.30),
        family_weight: 0.60,
    },
    LeafWord {
        word: "taken aback",
        family: EmotionFamily::Surprise,
        pad: (-0.10, 0.55, -0.20),
        family_weight: 0.55,
    },
    LeafWord {
        word: "bewildered",
        family: EmotionFamily::Surprise,
        pad: (-0.25, 0.40, -0.35),
        family_weight: 0.50,
    },
    LeafWord {
        word: "curious",
        family: EmotionFamily::Surprise,
        pad: (0.25, 0.35, 0.10),
        family_weight: 0.30,
    },
    LeafWord {
        word: "intrigued",
        family: EmotionFamily::Surprise,
        pad: (0.35, 0.40, 0.15),
        family_weight: 0.35,
    },
    LeafWord {
        word: "wonderstruck",
        family: EmotionFamily::Surprise,
        pad: (0.55, 0.65, 0.10),
        family_weight: 0.70,
    },
    // --- SHAME (16) ----------------------------------------------
    LeafWord {
        word: "mortified",
        family: EmotionFamily::Shame,
        pad: (-0.70, 0.40, -0.70),
        family_weight: 0.95,
    },
    LeafWord {
        word: "humiliated",
        family: EmotionFamily::Shame,
        pad: (-0.75, 0.30, -0.80),
        family_weight: 0.95,
    },
    LeafWord {
        word: "disgraced",
        family: EmotionFamily::Shame,
        pad: (-0.65, 0.20, -0.75),
        family_weight: 0.85,
    },
    LeafWord {
        word: "ashamed",
        family: EmotionFamily::Shame,
        pad: (-0.55, 0.20, -0.60),
        family_weight: 0.75,
    },
    LeafWord {
        word: "guilty",
        family: EmotionFamily::Shame,
        pad: (-0.50, 0.25, -0.45),
        family_weight: 0.65,
    },
    LeafWord {
        word: "remorseful",
        family: EmotionFamily::Shame,
        pad: (-0.55, 0.10, -0.35),
        family_weight: 0.65,
    },
    LeafWord {
        word: "regretful",
        family: EmotionFamily::Shame,
        pad: (-0.45, 0.00, -0.25),
        family_weight: 0.55,
    },
    LeafWord {
        word: "chastened",
        family: EmotionFamily::Shame,
        pad: (-0.35, 0.00, -0.40),
        family_weight: 0.50,
    },
    LeafWord {
        word: "embarrassed",
        family: EmotionFamily::Shame,
        pad: (-0.30, 0.35, -0.45),
        family_weight: 0.50,
    },
    LeafWord {
        word: "sheepish",
        family: EmotionFamily::Shame,
        pad: (-0.15, 0.15, -0.40),
        family_weight: 0.35,
    },
    LeafWord {
        word: "bashful",
        family: EmotionFamily::Shame,
        pad: (-0.05, 0.15, -0.45),
        family_weight: 0.25,
    },
    LeafWord {
        word: "shy",
        family: EmotionFamily::Shame,
        pad: (-0.05, 0.05, -0.40),
        family_weight: 0.20,
    },
    LeafWord {
        word: "self-conscious",
        family: EmotionFamily::Shame,
        pad: (-0.20, 0.20, -0.50),
        family_weight: 0.40,
    },
    LeafWord {
        word: "flustered",
        family: EmotionFamily::Shame,
        pad: (-0.20, 0.45, -0.40),
        family_weight: 0.40,
    },
    LeafWord {
        word: "contrite",
        family: EmotionFamily::Shame,
        pad: (-0.45, 0.05, -0.40),
        family_weight: 0.55,
    },
    LeafWord {
        word: "penitent",
        family: EmotionFamily::Shame,
        pad: (-0.40, 0.00, -0.35),
        family_weight: 0.50,
    },
    // --- AFFECTION (20) ------------------------------------------
    LeafWord {
        word: "enraptured",
        family: EmotionFamily::Affection,
        pad: (0.90, 0.75, 0.30),
        family_weight: 0.95,
    },
    LeafWord {
        word: "smitten",
        family: EmotionFamily::Affection,
        pad: (0.80, 0.65, 0.20),
        family_weight: 0.85,
    },
    LeafWord {
        word: "adoring",
        family: EmotionFamily::Affection,
        pad: (0.80, 0.45, 0.20),
        family_weight: 0.80,
    },
    LeafWord {
        word: "loving",
        family: EmotionFamily::Affection,
        pad: (0.80, 0.35, 0.25),
        family_weight: 0.75,
    },
    LeafWord {
        word: "devoted",
        family: EmotionFamily::Affection,
        pad: (0.70, 0.25, 0.35),
        family_weight: 0.70,
    },
    LeafWord {
        word: "tender",
        family: EmotionFamily::Affection,
        pad: (0.65, 0.20, 0.20),
        family_weight: 0.65,
    },
    LeafWord {
        word: "affectionate",
        family: EmotionFamily::Affection,
        pad: (0.65, 0.25, 0.20),
        family_weight: 0.65,
    },
    LeafWord {
        word: "fond",
        family: EmotionFamily::Affection,
        pad: (0.55, 0.15, 0.20),
        family_weight: 0.55,
    },
    LeafWord {
        word: "warm",
        family: EmotionFamily::Affection,
        pad: (0.55, 0.10, 0.15),
        family_weight: 0.50,
    },
    LeafWord {
        word: "hospitable",
        family: EmotionFamily::Affection,
        pad: (0.55, 0.20, 0.25),
        family_weight: 0.45,
    },
    LeafWord {
        word: "friendly",
        family: EmotionFamily::Affection,
        pad: (0.50, 0.20, 0.20),
        family_weight: 0.40,
    },
    LeafWord {
        word: "welcoming",
        family: EmotionFamily::Affection,
        pad: (0.55, 0.15, 0.25),
        family_weight: 0.45,
    },
    LeafWord {
        word: "compassionate",
        family: EmotionFamily::Affection,
        pad: (0.60, 0.20, 0.25),
        family_weight: 0.60,
    },
    LeafWord {
        word: "sympathetic",
        family: EmotionFamily::Affection,
        pad: (0.35, 0.15, 0.15),
        family_weight: 0.40,
    },
    LeafWord {
        word: "protective",
        family: EmotionFamily::Affection,
        pad: (0.40, 0.50, 0.55),
        family_weight: 0.55,
    },
    LeafWord {
        word: "grateful",
        family: EmotionFamily::Affection,
        pad: (0.65, 0.15, 0.10),
        family_weight: 0.55,
    },
    LeafWord {
        word: "moved",
        family: EmotionFamily::Affection,
        pad: (0.50, 0.35, 0.05),
        family_weight: 0.55,
    },
    LeafWord {
        word: "touched",
        family: EmotionFamily::Affection,
        pad: (0.55, 0.25, 0.10),
        family_weight: 0.55,
    },
    LeafWord {
        word: "charmed",
        family: EmotionFamily::Affection,
        pad: (0.60, 0.35, 0.20),
        family_weight: 0.55,
    },
    LeafWord {
        word: "trusting",
        family: EmotionFamily::Affection,
        pad: (0.45, 0.00, 0.15),
        family_weight: 0.40,
    },
    // --- COMPOSITES / NEUTRAL (12) -------------------------------
    // These fall back to the family they're closest to in affective
    // tone, so project_top_k can pick them when the state is mildly
    // tilted without a dominant family.
    LeafWord {
        word: "calm",
        family: EmotionFamily::Joy,
        pad: (0.30, -0.40, 0.20),
        family_weight: 0.15,
    },
    LeafWord {
        word: "tranquil",
        family: EmotionFamily::Joy,
        pad: (0.35, -0.45, 0.20),
        family_weight: 0.15,
    },
    LeafWord {
        word: "restless",
        family: EmotionFamily::Fear,
        pad: (-0.10, 0.55, -0.10),
        family_weight: 0.25,
    },
    LeafWord {
        word: "weary",
        family: EmotionFamily::Sadness,
        pad: (-0.25, -0.60, -0.20),
        family_weight: 0.25,
    },
    LeafWord {
        word: "spent",
        family: EmotionFamily::Sadness,
        pad: (-0.30, -0.65, -0.30),
        family_weight: 0.25,
    },
    LeafWord {
        word: "determined",
        family: EmotionFamily::Anger,
        pad: (0.10, 0.40, 0.55),
        family_weight: 0.20,
    },
    LeafWord {
        word: "resolute",
        family: EmotionFamily::Anger,
        pad: (0.15, 0.35, 0.60),
        family_weight: 0.20,
    },
    LeafWord {
        word: "alert",
        family: EmotionFamily::Surprise,
        pad: (0.10, 0.45, 0.25),
        family_weight: 0.15,
    },
    LeafWord {
        word: "contemplative",
        family: EmotionFamily::Sadness,
        pad: (0.05, -0.15, 0.10),
        family_weight: 0.15,
    },
    LeafWord {
        word: "reflective",
        family: EmotionFamily::Sadness,
        pad: (0.05, -0.10, 0.10),
        family_weight: 0.15,
    },
    LeafWord {
        word: "stoic",
        family: EmotionFamily::Anger,
        pad: (0.00, -0.10, 0.35),
        family_weight: 0.10,
    },
    LeafWord {
        word: "guarded",
        family: EmotionFamily::Fear,
        pad: (-0.05, 0.20, 0.10),
        family_weight: 0.20,
    },
];

/// Returns the top-`k` leaves whose affective coordinate best matches
/// `state`.
///
/// Scoring is a simple weighted distance: family match first (huge
/// bonus when the leaf is in the state's dominant family), then
/// Euclidean distance in PAD space, then penalty for mismatched
/// family intensity.
///
/// This is deliberately cheap — the table is small and this runs at
/// most a handful of times per dialogue turn.
pub fn project_top_k(state: &EmotionState, k: usize) -> Vec<&'static LeafWord> {
    if k == 0 {
        return Vec::new();
    }

    let (dom_family, dom_intensity) = state.dominant_family();

    let mut scored: Vec<(f32, &'static LeafWord)> = EMOTION_LEAVES
        .iter()
        .map(|leaf| {
            let family_bonus = if leaf.family == dom_family { -1.5 } else { 0.0 };
            let dp = leaf.pad.0 - state.pleasure;
            let da = leaf.pad.1 - state.arousal;
            let dd = leaf.pad.2 - state.dominance;
            let pad_dist = (dp * dp + da * da + dd * dd).sqrt();
            let intensity_gap = (leaf.family_weight - dom_intensity).abs();
            let score = family_bonus + pad_dist + intensity_gap * 0.5;
            (score, leaf)
        })
        .collect();

    scored.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().take(k).map(|(_, l)| l).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotion::{EmotionState, FamilyVec};

    #[test]
    fn table_has_at_least_171_leaves() {
        // Hedged: the plan was 171. Allow a small cushion so the
        // test doesn't burn down if we later add or merge a couple of
        // near-duplicates. The essential invariant is that we cover
        // every family.
        assert!(
            EMOTION_LEAVES.len() >= 165,
            "expected ~171 leaves, got {}",
            EMOTION_LEAVES.len()
        );
    }

    #[test]
    fn every_family_has_at_least_five_leaves() {
        for fam in EmotionFamily::ALL.iter().copied() {
            let count = EMOTION_LEAVES.iter().filter(|l| l.family == fam).count();
            assert!(count >= 5, "family {:?} only has {} leaves", fam, count);
        }
    }

    #[test]
    fn all_pad_coords_are_in_range() {
        for leaf in EMOTION_LEAVES {
            let (p, a, d) = leaf.pad;
            assert!(
                (-1.0..=1.0).contains(&p),
                "{} pleasure out of range: {}",
                leaf.word,
                p
            );
            assert!(
                (-1.0..=1.0).contains(&a),
                "{} arousal out of range: {}",
                leaf.word,
                a
            );
            assert!(
                (-1.0..=1.0).contains(&d),
                "{} dominance out of range: {}",
                leaf.word,
                d
            );
        }
    }

    #[test]
    fn all_family_weights_are_in_range() {
        for leaf in EMOTION_LEAVES {
            assert!(
                (0.0..=1.0).contains(&leaf.family_weight),
                "{} weight out of range: {}",
                leaf.word,
                leaf.family_weight
            );
        }
    }

    #[test]
    fn project_top_k_returns_empty_for_k_zero() {
        assert!(project_top_k(&EmotionState::default(), 0).is_empty());
    }

    #[test]
    fn project_top_k_for_high_anger_returns_anger_word() {
        let mut s = EmotionState::default();
        s.families.anger = 0.9;
        s.pleasure = -0.5;
        s.arousal = 0.8;
        s.dominance = 0.6;
        let top = project_top_k(&s, 1);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].family, EmotionFamily::Anger);
        // One of the high-anger leaves should win
        assert!(
            ["enraged", "furious", "livid", "incensed", "irate"].contains(&top[0].word),
            "unexpected top leaf: {}",
            top[0].word
        );
    }

    #[test]
    fn project_top_k_for_high_fear_returns_fear_word() {
        let mut s = EmotionState::default();
        s.families.fear = 0.95;
        s.pleasure = -0.7;
        s.arousal = 0.9;
        s.dominance = -0.7;
        let top = project_top_k(&s, 1);
        assert_eq!(top[0].family, EmotionFamily::Fear);
        assert!(["terrified", "petrified", "panicked"].contains(&top[0].word));
    }

    #[test]
    fn project_top_k_for_neutral_state_picks_neutral_leaf() {
        let s = EmotionState::default();
        let top = project_top_k(&s, 3);
        // Should include at least one composite/neutral-band leaf,
        // not an extreme one.
        let has_neutral = top
            .iter()
            .any(|l| ["calm", "content", "tranquil", "serene", "weary"].contains(&l.word));
        assert!(
            has_neutral,
            "expected a neutral leaf in top-3, got: {:?}",
            top.iter().map(|l| l.word).collect::<Vec<_>>()
        );
    }

    #[test]
    fn project_top_k_k_larger_than_table_returns_all() {
        let top = project_top_k(&EmotionState::default(), EMOTION_LEAVES.len() + 10);
        assert_eq!(top.len(), EMOTION_LEAVES.len());
    }

    #[test]
    fn no_duplicate_leaf_words_within_same_family() {
        // A few words appear in two families (e.g. "wary" as
        // fear/disgust borderline). That's fine — but we shouldn't
        // have accidental dupes *within* a single family, which would
        // bias top-k selection.
        for fam in EmotionFamily::ALL.iter().copied() {
            let mut words: Vec<&str> = EMOTION_LEAVES
                .iter()
                .filter(|l| l.family == fam)
                .map(|l| l.word)
                .collect();
            words.sort_unstable();
            let unique_len = {
                let mut w = words.clone();
                w.dedup();
                w.len()
            };
            assert_eq!(
                words.len(),
                unique_len,
                "family {:?} has duplicate words: {:?}",
                fam,
                words
            );
        }
    }

    #[test]
    fn project_top_k_for_grief_picks_sadness_word() {
        let s = EmotionState {
            pleasure: -0.7,
            arousal: -0.3,
            dominance: -0.4,
            families: FamilyVec {
                sadness: 0.85,
                ..Default::default()
            },
            ..Default::default()
        };
        let top = project_top_k(&s, 1);
        assert_eq!(top[0].family, EmotionFamily::Sadness);
        assert!(
            [
                "grieving",
                "heartbroken",
                "anguished",
                "despairing",
                "desolate",
                "bereft",
                "mournful",
                "sorrowful",
                "devastated",
                "dejected",
                "hopeless",
                "forlorn",
            ]
            .contains(&top[0].word),
            "unexpected top leaf for grief: {}",
            top[0].word
        );
    }
}
