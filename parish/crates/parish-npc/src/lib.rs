//! NPC system for the Parish game engine.
//!
//! Each NPC has personality traits, a daily schedule, relationships
//! with other NPCs, and short-term memory. Cognition fidelity is determined
//! by the NpcManager based on distance from the player.

pub mod anachronism;
pub mod autonomous;
pub mod banshee;
pub mod data;
pub mod manager;
pub mod memory;
pub mod mood;
pub mod quality;
pub mod reactions;
pub mod schedule;
pub mod ticks;
pub mod tier4;
pub mod tier_assign;
pub mod transitions;
pub mod types;

#[cfg(test)]
mod overhear;

/// Re-export conversation types from parish-types for cross-crate path compatibility.
pub mod conversation {
    pub use parish_types::conversation::*;
}

use std::collections::HashMap;

use serde::Deserialize;

use chrono::{Datelike, Timelike};
use memory::{LongTermMemory, ShortTermMemory};
use parish_types::{DayType, LocationId, Season, TimeOfDay};
use parish_world::WorldState;
use parish_world::description::render_description;
use parish_world::movement::{WeatherEffect, weather_effect};
use parish_world::transport::TransportMode;
use reactions::ReactionLog;
use transitions::NpcSummary;
use types::{Intelligence, NpcState, Relationship, SeasonalSchedule};

// Re-export shared types from parish-types
pub use parish_types::{
    LanguageHint, NpcId, extract_dialogue_from_partial_json, floor_char_boundary,
};

// Re-export the NPC data-file schema so downstream crates (e.g. the Parish
// Designer editor) can round-trip `npcs.json` without duplicating the schema.
pub use data::{
    IntelligenceFileEntry, NpcFile, NpcFileEntry, RelationshipFileEntry, ScheduleFileEntry,
    ScheduleVariantFileEntry,
};

// ── Crate-root logic, split into focused submodules (#1200 TD-011/TD-027).
//    Each `pub use module::*` keeps every public path stable
//    (e.g. `parish_npc::build_tier1_system_prompt`).
mod context;
mod dialogue_validation;
mod language;
mod names;
mod npc;
mod repetition;
mod response;
mod tier1_prompt;

pub use context::*;
pub use dialogue_validation::*;
pub use language::*;
pub use names::*;
pub use npc::*;
pub use repetition::*;
pub use response::*;
pub use tier1_prompt::*;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) mod test_helpers;
