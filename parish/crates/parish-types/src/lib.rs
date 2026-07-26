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
pub mod player_progress;
pub mod theme;
pub mod time;

pub use conversation::{ConversationCursor, ConversationExchange, ConversationLog};
pub use dice::{DiceRoll, fixed_n, roll_n};
pub use error::ParishError;
pub use events::{ContextEventEnvelope, EventBus, GameEvent};
pub use gossip::{GossipItem, GossipNetwork};
pub use ids::{
    DEFAULT_START_LOCATION, LanguageHint, Location, LocationId, NpcId, Weather,
    extract_dialogue_from_partial_json, floor_char_boundary,
};
pub use player_progress::{
    MAX_PLAYER_TASKS, MAX_TASK_ACTION_CHARS, MAX_TASK_DESCRIPTION_CHARS, PlayerProgress,
    PlayerProgressError, PlayerTask, PlayerTaskId, TaskStatus,
};
pub use theme::ThemePalette;
pub use time::{
    DayType, Festival, GameClock, GameSpeed, Season, SpeedConfig, TimeOfDay, minute_word,
};

/// A single anachronism term entry loaded from mod JSON data.
///
/// Shared between `parish-npc` (for detection) and `parish-core` (for mod loading).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anachronism_entry_deserializes_full_contract() {
        let json = r#"{
            "term": "telegram",
            "category": "technology",
            "origin_year": 1837,
            "note": "Commercial telegraph service was not yet available."
        }"#;

        let entry: AnachronismEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.term, "telegram");
        assert_eq!(entry.category.as_deref(), Some("technology"));
        assert_eq!(entry.origin_year, Some(1837));
        assert_eq!(
            entry.note,
            "Commercial telegraph service was not yet available."
        );
    }

    #[test]
    fn anachronism_entry_defaults_optional_contract_fields() {
        let entry: AnachronismEntry = serde_json::from_str(r#"{"term":"motorcar"}"#).unwrap();

        assert_eq!(entry.term, "motorcar");
        assert_eq!(entry.category, None);
        assert_eq!(entry.origin_year, None);
        assert_eq!(entry.note, "");
    }

    #[test]
    fn anachronism_entry_accepts_legacy_reason_alias_for_note() {
        let entry: AnachronismEntry =
            serde_json::from_str(r#"{"term":"okay","reason":"not idiomatic in 1820"}"#).unwrap();

        assert_eq!(entry.note, "not idiomatic in 1820");
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains(r#""note":"not idiomatic in 1820""#));
        assert!(!json.contains("reason"));
    }
}
