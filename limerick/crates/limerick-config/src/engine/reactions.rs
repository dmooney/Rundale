//! NPC arrival-reaction tuning (`[engine.npc.reactions]`).

use serde::Deserialize;

/// Tuning for NPC arrival reactions (greetings, nods, introductions).
#[derive(Debug, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReactionConfig {
    /// Base probability that an NPC reacts when the player arrives.
    #[serde(default = "default_reaction_base_chance")]
    pub base_chance: f64,
    /// Bonus when NPC is at their workplace.
    #[serde(default = "default_reaction_workplace_bonus")]
    pub workplace_bonus: f64,
    /// Bonus when location is indoors.
    #[serde(default = "default_reaction_indoor_bonus")]
    pub indoor_bonus: f64,
    /// Bonus when NPC has high emotional intelligence (≥4).
    #[serde(default = "default_reaction_empathy_bonus")]
    pub empathy_bonus: f64,
    /// Penalty when NPC has a negative mood.
    #[serde(default = "default_reaction_negative_mood_penalty")]
    pub negative_mood_penalty: f64,
    /// Penalty at night or midnight.
    #[serde(default = "default_reaction_night_penalty")]
    pub night_penalty: f64,
    /// LLM timeout for reaction greeting calls (seconds).
    #[serde(default = "default_reaction_llm_timeout_secs")]
    pub llm_timeout_secs: u64,
    /// Maximum number of NPCs that react on a single arrival (0 = no cap).
    #[serde(default = "default_reaction_max_reactions")]
    pub max_reactions: usize,
}

impl Default for ReactionConfig {
    fn default() -> Self {
        Self {
            base_chance: default_reaction_base_chance(),
            workplace_bonus: default_reaction_workplace_bonus(),
            indoor_bonus: default_reaction_indoor_bonus(),
            empathy_bonus: default_reaction_empathy_bonus(),
            negative_mood_penalty: default_reaction_negative_mood_penalty(),
            night_penalty: default_reaction_night_penalty(),
            llm_timeout_secs: default_reaction_llm_timeout_secs(),
            max_reactions: default_reaction_max_reactions(),
        }
    }
}

fn default_reaction_base_chance() -> f64 {
    0.55
}
fn default_reaction_workplace_bonus() -> f64 {
    0.35
}
fn default_reaction_indoor_bonus() -> f64 {
    0.10
}
fn default_reaction_empathy_bonus() -> f64 {
    0.05
}
fn default_reaction_negative_mood_penalty() -> f64 {
    0.20
}
fn default_reaction_night_penalty() -> f64 {
    0.15
}
fn default_reaction_llm_timeout_secs() -> u64 {
    5
}
fn default_reaction_max_reactions() -> usize {
    2
}
