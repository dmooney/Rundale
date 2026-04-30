//! Foundational types for the Parish game engine.
//!
//! This is the leaf crate — it has zero internal dependencies.
//! All other parish-* crates depend on this one.

pub mod conversation;
pub mod dice;
pub mod error;
pub mod events;
pub mod gossip;
pub mod ids;
pub mod time;

pub use conversation::{ConversationExchange, ConversationLog};
pub use dice::{DiceRoll, fixed_n, roll_n};
pub use error::ParishError;
pub use events::{EventBus, GameEvent};
pub use gossip::{GossipItem, GossipNetwork};
pub use ids::{
    IrishWordHint, LanguageHint, Location, LocationId, NpcId, Weather,
    extract_dialogue_from_partial_json, floor_char_boundary,
};
pub use time::{DayType, Festival, GameClock, GameSpeed, Season, SpeedConfig, TimeOfDay};

/// A single anachronism term entry loaded from mod JSON data.
///
/// Shared between `parish-npc` (for detection) and `parish-core` (for mod loading).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AnachronismEntry {
    /// The anachronistic term or phrase.
    pub term: String,
    /// Category of anachronism (e.g. "technology", "slang").
    #[serde(default)]
    pub category: Option<String>,
    /// Earliest year this concept existed.
    #[serde(default)]
    pub origin_year: Option<u32>,
    /// Brief note explaining why the term is anachronistic.
    #[serde(default, alias = "reason")]
    pub note: String,
}
