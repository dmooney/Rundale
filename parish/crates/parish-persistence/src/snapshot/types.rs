//! Serializable snapshot data types.
//!
//! This module contains the plain-data structs that make up the
//! persistence schema. Conversion to/from live types lives in
//! [`super::convert`]; restore logic lives in [`super::restore`].

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use parish_npc::memory::{LongTermMemory, ShortTermMemory};
use parish_npc::types::{Intelligence, NpcState, Relationship, SeasonalSchedule};
use parish_types::{ConversationLog, GossipNetwork, LocationId, NpcId, PlayerProgress};

/// Serde helpers for `edge_traversals: HashMap<(LocationId, LocationId), u32>`.
///
/// JSON map keys must be strings, but `(LocationId, LocationId)` is a tuple.
/// We serialize as a list of `[from, to, count]` arrays instead.
pub(super) mod edge_traversals_serde {
    use parish_types::LocationId;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S>(
        map: &HashMap<(LocationId, LocationId), u32>,
        s: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let list: Vec<[u32; 3]> = map
            .iter()
            .map(|((a, b), count)| [a.0, b.0, *count])
            .collect();
        list.serialize(s)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<HashMap<(LocationId, LocationId), u32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let list: Vec<[u32; 3]> = Vec::deserialize(d)?;
        Ok(list
            .into_iter()
            .map(|[a, b, count]| ((LocationId(a), LocationId(b)), count))
            .collect())
    }
}

/// Default pronouns for NPCs saved before the `pronouns` field existed.
pub(super) fn default_pronouns() -> String {
    "they/them".to_string()
}

/// Snapshot of the game clock's logical state.
///
/// Captures the current game time, speed factor, and paused flag.
/// On restore, a new [`GameClock`](parish_types::GameClock) is
/// constructed from these values (the real-time anchor is reset).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClockSnapshot {
    /// The current game time.
    pub game_time: DateTime<Utc>,
    /// Game-time seconds per real-time second.
    pub speed_factor: f64,
    /// Whether the clock is paused.
    pub paused: bool,
}

/// Snapshot of a single NPC's dynamic state.
///
/// Mirrors the fields of [`Npc`](parish_npc::Npc) so the struct can be
/// serialized without requiring `Serialize` on `Npc` itself.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NpcSnapshot {
    /// Unique identifier.
    pub id: NpcId,
    /// Full name.
    pub name: String,
    /// Brief anonymous description shown before the player is introduced.
    #[serde(default)]
    pub brief_description: String,
    /// Age in years.
    pub age: u8,
    /// Occupation or role.
    pub occupation: String,
    /// Personality description.
    pub personality: String,
    /// Narration pronouns. Defaults to `they/them` for saves written before
    /// the field existed (#1026).
    #[serde(default = "default_pronouns")]
    pub pronouns: String,
    /// Multidimensional intelligence profile.
    #[serde(default)]
    pub intelligence: Intelligence,
    /// Current location.
    pub location: LocationId,
    /// Current emotional state.
    pub mood: String,
    /// Home location.
    pub home: Option<LocationId>,
    /// Workplace location.
    pub workplace: Option<LocationId>,
    /// Season-aware schedule.
    pub schedule: Option<SeasonalSchedule>,
    /// Relationships to other NPCs.
    pub relationships: HashMap<NpcId, Relationship>,
    /// Short-term memory ring buffer.
    pub memory: ShortTermMemory,
    /// Persistent long-term memory with keyword-based retrieval.
    #[serde(default)]
    pub long_term_memory: LongTermMemory,
    /// Knowledge entries.
    pub knowledge: Vec<String>,
    /// Present or in-transit state.
    pub state: NpcState,
    /// Last activity summary from Tier 3 batch simulation.
    #[serde(default)]
    pub last_activity: Option<String>,
    /// Whether the NPC is currently ill. Set by Tier 4 rules engine.
    #[serde(default)]
    pub is_ill: bool,
    /// Game-time at which this NPC is fated to die, if set.
    ///
    /// See [`parish_npc::Npc::doom`] for semantics.
    #[serde(default)]
    pub doom: Option<DateTime<Utc>>,
    /// Whether the banshee wail has already been emitted for the current doom.
    #[serde(default)]
    pub banshee_heralded: bool,
    /// Compressed summary written by `NpcManager::assign_tiers` when the
    /// NPC demotes from a higher cognitive tier.
    ///
    /// Serialized so that an autosave + reload doesn't silently erase
    /// Phase 5 cognitive-LOD compression history (#338). The previous
    /// schema had no field for it and [`NpcSnapshot::into_npc`] hard-coded
    /// `None`, so every save/load cycle dropped the demotion summary
    /// on the floor. `#[serde(default)]` keeps older save files
    /// (pre-#338) loadable.
    #[serde(default)]
    pub deflated_summary: Option<parish_npc::transitions::NpcSummary>,
}

/// A complete snapshot of dynamic game state.
///
/// This is the unit of persistence: serialized to JSON and stored in
/// the `snapshots` table. Static data (world graph, locations) is
/// loaded from data files and not included here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameSnapshot {
    /// Player's current location.
    pub player_location: LocationId,
    /// Current weather description.
    pub weather: String,
    /// Scrollback text log.
    pub text_log: Vec<String>,
    /// Clock state.
    pub clock: ClockSnapshot,
    /// All NPC states.
    pub npcs: Vec<NpcSnapshot>,
    /// Game time of the last Tier 2 tick.
    pub last_tier2_game_time: Option<DateTime<Utc>>,
    /// Game time of the last Tier 3 tick.
    #[serde(default)]
    pub last_tier3_game_time: Option<DateTime<Utc>>,
    /// Game time of the last Tier 4 tick.
    #[serde(default)]
    pub last_tier4_game_time: Option<DateTime<Utc>>,
    /// NPCs the player has been introduced to.
    #[serde(default)]
    pub introduced_npcs: HashSet<NpcId>,
    /// Set of location IDs the player has visited (fog-of-war map).
    #[serde(default)]
    pub visited_locations: HashSet<LocationId>,
    /// First-visit order, parallel to `visited_locations`. Used by
    /// `character_log` to render `player.md`'s visited section in
    /// playthrough order instead of by numeric id (#1130). Defaults to
    /// empty for older saves; in that case the player profile falls
    /// back to id order until the next visit appends a fresh entry.
    #[serde(default)]
    pub visited_order: Vec<LocationId>,
    /// Edge traversal counts for "worn path" footprints on the map.
    #[serde(default, with = "edge_traversals_serde")]
    pub edge_traversals: HashMap<(LocationId, LocationId), u32>,
    /// Gossip network state.
    #[serde(default)]
    pub gossip_network: GossipNetwork,
    /// Recent conversation exchanges for scene awareness.
    #[serde(default)]
    pub conversation_log: ConversationLog,
    /// The player's name, learned from dialogue.
    #[serde(default)]
    pub player_name: Option<String>,
    /// Durable player task assignments and progression.
    #[serde(default)]
    pub player_progress: PlayerProgress,
    /// Set of NPC ids that know the player's name.
    #[serde(default)]
    pub npcs_who_know_player_name: HashSet<NpcId>,
}
