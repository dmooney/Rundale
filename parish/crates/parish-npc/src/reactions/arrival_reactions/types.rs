//! Shared types for the NPC arrival reaction system.
//!
//! Defines the reaction kind enum, the resolved reaction struct,
//! and the scene context bundle passed to the generation function.

use crate::NpcId;
use parish_config::ReactionConfig;
use parish_world::graph::LocationData;
use parish_world::time::TimeOfDay;

use super::templates::ReactionTemplates;

// ── Core types ───────────────────────────────────────────────────────────────

/// The kind of reaction an NPC has to the player's arrival.
#[derive(Debug, Clone, PartialEq)]
pub enum ReactionKind {
    /// A silent gesture: nod, wave, glance up.
    Gesture,
    /// A short verbal greeting.
    Greeting,
    /// A role-specific welcome at the NPC's workplace.
    Welcome,
    /// The NPC introduces themselves by name.
    Introduction,
}

/// A resolved reaction from one NPC.
#[derive(Debug, Clone)]
pub struct NpcReaction {
    /// Which NPC reacted.
    pub npc_id: NpcId,
    /// Display name (full name or brief description).
    pub npc_display_name: String,
    /// What kind of reaction.
    pub kind: ReactionKind,
    /// The canned fallback text for this reaction.
    pub canned_text: String,
    /// Whether this reaction should mark the NPC as introduced.
    pub introduces: bool,
    /// Whether the caller should attempt an LLM-generated greeting.
    pub use_llm: bool,
}

/// Scene context passed to [`super::selection::generate_arrival_reactions`].
///
/// Bundles the location, time, weather, template bank, and reaction config
/// so the function signature stays manageable as game state grows.
pub struct ArrivalContext<'a> {
    /// The location the player has just entered.
    pub location: &'a LocationData,
    /// Current time of day.
    pub time_of_day: TimeOfDay,
    /// Current weather description (e.g. `"clear"`, `"rainy"`).
    pub weather: &'a str,
    /// Mod-provided (or default) text templates.
    pub templates: &'a ReactionTemplates,
    /// Reaction probability config.
    pub config: &'a ReactionConfig,
}
