//! Foundational types for the Limerick game engine.
//!
//! This is the leaf crate — it has zero internal dependencies.
//! All other limerick-* crates depend on this one.

pub mod content;
pub mod conversation;
pub mod dice;
pub mod error;
pub mod events;
pub mod gossip;
pub mod ids;
pub mod player_progress;
pub mod theme;
pub mod time;

pub use content::ContentIdentity;
pub use conversation::{
    ConversationCursor, ConversationExchange, ConversationLog, RememberedObjectAttribute,
    RememberedObjectAttributeKind, RememberedObjectFact,
};
pub use dice::{DiceRoll, fixed_n, seeded_n};
pub use error::LimerickError;
pub use events::{ContextEventEnvelope, EventBus, GameEvent, ReactionDirection};
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
