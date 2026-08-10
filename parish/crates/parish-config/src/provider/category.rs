//! [`InferenceCategory`] — the four independently-configurable inference slots.

/// Inference categories that can each have independent provider/model/key settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InferenceCategory {
    /// Player-facing NPC dialogue (Tier 1, streaming).
    Dialogue,
    /// Background NPC simulation (Tier 2, JSON).
    Simulation,
    /// Player input intent parsing (JSON, low-latency).
    Intent,
    /// NPC arrival reactions/greetings (short timeout, fast model).
    Reaction,
}

impl InferenceCategory {
    /// All defined inference categories.
    pub const ALL: [InferenceCategory; 4] = [
        InferenceCategory::Dialogue,
        InferenceCategory::Simulation,
        InferenceCategory::Intent,
        InferenceCategory::Reaction,
    ];

    /// Array index matching [`InferenceCategory::ALL`] order.
    pub fn idx(self) -> usize {
        match self {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
            InferenceCategory::Reaction => 3,
        }
    }

    /// Returns the lowercase name used in TOML keys, env var prefixes, and CLI flags.
    pub fn name(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "dialogue",
            InferenceCategory::Simulation => "simulation",
            InferenceCategory::Intent => "intent",
            InferenceCategory::Reaction => "reaction",
        }
    }

    /// Parses a category name (case-insensitive).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dialogue" => Some(InferenceCategory::Dialogue),
            "simulation" => Some(InferenceCategory::Simulation),
            "intent" => Some(InferenceCategory::Intent),
            "reaction" => Some(InferenceCategory::Reaction),
            _ => None,
        }
    }

    /// Returns the SCREAMING_CASE prefix used in environment variables.
    pub fn env_prefix(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "PARISH_DIALOGUE",
            InferenceCategory::Simulation => "PARISH_SIMULATION",
            InferenceCategory::Intent => "PARISH_INTENT",
            InferenceCategory::Reaction => "PARISH_REACTION",
        }
    }
}
