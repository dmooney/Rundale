//! Snapshot serialization — captures and restores dynamic game state.
//!
//! The [`GameSnapshot`] struct is a fully serializable representation of
//! all dynamic game state. Static data (world graph, location map) is
//! excluded because it's loaded from data files on startup.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::npc::gossip::GossipNetwork;

/// Serde helpers for `edge_traversals: HashMap<(LocationId, LocationId), u32>`.
///
/// JSON map keys must be strings, but `(LocationId, LocationId)` is a tuple.
/// We serialize as a list of `[from, to, count]` arrays instead.
mod edge_traversals_serde {
    use crate::world::LocationId;
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
use crate::npc::memory::{LongTermMemory, ShortTermMemory};
use crate::npc::types::{Intelligence, NpcState, Relationship, SeasonalSchedule};
use crate::npc::{Npc, NpcId};
use crate::world::LocationId;

/// Snapshot of the game clock's logical state.
///
/// Captures the current game time, speed factor, and paused flag.
/// On restore, a new [`GameClock`](crate::world::time::GameClock) is
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
/// Mirrors the fields of [`Npc`] so the struct can be serialized
/// without requiring `Serialize` on `Npc` itself.
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
}

impl NpcSnapshot {
    /// Captures a snapshot from a live NPC.
    pub fn from_npc(npc: &Npc) -> Self {
        Self {
            id: npc.id,
            name: npc.name.clone(),
            brief_description: npc.brief_description.clone(),
            age: npc.age,
            occupation: npc.occupation.clone(),
            personality: npc.personality.clone(),
            intelligence: npc.intelligence,
            location: npc.location,
            mood: npc.mood.clone(),
            home: npc.home,
            workplace: npc.workplace,
            schedule: npc.schedule.clone(),
            relationships: npc.relationships.clone(),
            memory: npc.memory.clone(),
            long_term_memory: npc.long_term_memory.clone(),
            knowledge: npc.knowledge.clone(),
            state: npc.state.clone(),
        }
    }

    /// Restores the snapshot into a live NPC.
    pub fn into_npc(self) -> Npc {
        Npc {
            id: self.id,
            name: self.name,
            brief_description: self.brief_description,
            age: self.age,
            occupation: self.occupation,
            personality: self.personality,
            intelligence: self.intelligence,
            location: self.location,
            mood: self.mood,
            home: self.home,
            workplace: self.workplace,
            schedule: self.schedule,
            relationships: self.relationships,
            memory: self.memory,
            long_term_memory: self.long_term_memory,
            knowledge: self.knowledge,
            state: self.state,
            deflated_summary: None,
            reaction_log: crate::npc::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
        }
    }
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
    /// Set of location IDs the player has visited (fog-of-war map).
    #[serde(default)]
    pub visited_locations: HashSet<LocationId>,
    /// Edge traversal counts for "worn path" footprints on the map.
    #[serde(default, with = "edge_traversals_serde")]
    pub edge_traversals: HashMap<(LocationId, LocationId), u32>,
    /// Gossip network state.
    #[serde(default)]
    pub gossip_network: GossipNetwork,
}

impl GameSnapshot {
    /// Captures a snapshot from live game state.
    pub fn capture(
        world: &crate::world::WorldState,
        npc_manager: &crate::npc::manager::NpcManager,
    ) -> Self {
        let clock = ClockSnapshot {
            game_time: world.clock.now(),
            speed_factor: world.clock.speed_factor(),
            paused: world.clock.is_paused(),
        };

        let npcs: Vec<NpcSnapshot> = npc_manager.all_npcs().map(NpcSnapshot::from_npc).collect();

        Self {
            player_location: world.player_location,
            weather: world.weather.to_string(),
            text_log: world.text_log.clone(),
            clock,
            npcs,
            last_tier2_game_time: npc_manager.last_tier2_game_time(),
            visited_locations: world.visited_locations.clone(),
            edge_traversals: world.edge_traversals.clone(),
            gossip_network: world.gossip_network.clone(),
        }
    }

    /// Restores this snapshot into live game state.
    ///
    /// Replaces the dynamic fields of `world` and rebuilds the `npc_manager`
    /// from the snapshot. The world graph and location map are left untouched
    /// (they are static data loaded from files).
    pub fn restore(
        self,
        world: &mut crate::world::WorldState,
        npc_manager: &mut crate::npc::manager::NpcManager,
    ) {
        use crate::world::time::GameClock;

        // Restore clock
        let mut clock = GameClock::new(self.clock.game_time);
        if self.clock.paused {
            clock.pause();
        }
        // Set speed by finding the matching preset, or use custom factor
        let factor = self.clock.speed_factor;
        use crate::world::time::GameSpeed;
        let speed = [
            GameSpeed::Slow,
            GameSpeed::Normal,
            GameSpeed::Fast,
            GameSpeed::Fastest,
        ]
        .into_iter()
        .find(|s| (s.factor() - factor).abs() < 0.01);
        if let Some(s) = speed {
            clock.set_speed(s);
        }
        // For custom speeds, we need to set it directly — handled by with_speed constructor
        if speed.is_none() {
            clock = GameClock::with_speed(self.clock.game_time, factor);
            if self.clock.paused {
                clock.pause();
            }
        }

        world.clock = clock;
        world.player_location = self.player_location;
        world.weather = self.weather.parse().unwrap_or(crate::world::Weather::Clear);
        world.text_log = self.text_log;

        // Restore visited locations; ensure current position is always visited
        world.visited_locations = self.visited_locations;
        world.visited_locations.insert(self.player_location);

        // Restore edge traversal counts
        world.edge_traversals = self.edge_traversals;

        // Restore NPCs
        *npc_manager = crate::npc::manager::NpcManager::new();
        for npc_snap in self.npcs {
            npc_manager.add_npc(npc_snap.into_npc());
        }
        if let Some(t) = self.last_tier2_game_time {
            npc_manager.record_tier2_tick(t);
        }

        // Restore gossip network
        world.gossip_network = self.gossip_network;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::Npc;
    use crate::npc::manager::NpcManager;
    use crate::npc::memory::{LongTermMemory, ShortTermMemory};
    use crate::npc::types::NpcState;
    use crate::world::WorldState;
    use chrono::{TimeZone, Utc};

    fn make_test_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            intelligence: Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
            deflated_summary: None,
            reaction_log: crate::npc::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
        }
    }

    #[test]
    fn test_npc_snapshot_roundtrip() {
        let npc = make_test_npc(1, 2);
        let snap = NpcSnapshot::from_npc(&npc);
        let restored = snap.into_npc();
        assert_eq!(restored.id, NpcId(1));
        assert_eq!(restored.name, "NPC 1");
        assert_eq!(restored.location, LocationId(2));
        assert_eq!(restored.mood, "calm");
    }

    #[test]
    fn test_clock_snapshot_serialize_roundtrip() {
        let clock = ClockSnapshot {
            game_time: Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap(),
            speed_factor: 36.0,
            paused: false,
        };
        let json = serde_json::to_string(&clock).unwrap();
        let restored: ClockSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(clock, restored);
    }

    #[test]
    fn test_game_snapshot_capture_and_serialize() {
        let world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(make_test_npc(1, 1));
        npc_manager.add_npc(make_test_npc(2, 2));

        let snapshot = GameSnapshot::capture(&world, &npc_manager);

        // Serialize and deserialize
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: GameSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snapshot.player_location, restored.player_location);
        assert_eq!(snapshot.weather, restored.weather);
        assert_eq!(snapshot.npcs.len(), restored.npcs.len());
    }

    #[test]
    fn test_game_snapshot_restore() {
        let mut world = WorldState::new();
        world.weather = crate::world::Weather::LightRain;
        world.log("Test entry".to_string());
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(make_test_npc(1, 1));

        let snapshot = GameSnapshot::capture(&world, &npc_manager);

        // Create a fresh world and restore
        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        snapshot.restore(&mut new_world, &mut new_npcs);

        assert_eq!(new_world.weather, crate::world::Weather::LightRain);
        assert_eq!(new_world.text_log.len(), 1);
        assert_eq!(new_world.text_log[0], "Test entry");
        assert_eq!(new_npcs.npc_count(), 1);
        assert!(new_npcs.get(NpcId(1)).is_some());
    }

    #[test]
    fn test_game_snapshot_paused_clock() {
        let mut world = WorldState::new();
        world.clock.pause();
        world.clock.advance(120); // advance 2 hours while paused
        let npc_manager = NpcManager::new();

        let snapshot = GameSnapshot::capture(&world, &npc_manager);
        assert!(snapshot.clock.paused);

        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        snapshot.restore(&mut new_world, &mut new_npcs);

        assert!(new_world.clock.is_paused());
    }

    #[test]
    fn test_game_snapshot_preserves_tier2_time() {
        let world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        let t = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        npc_manager.record_tier2_tick(t);

        let snapshot = GameSnapshot::capture(&world, &npc_manager);
        assert_eq!(snapshot.last_tier2_game_time, Some(t));

        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        snapshot.restore(&mut new_world, &mut new_npcs);
        assert!(!new_npcs.needs_tier2_tick(t));
    }

    #[test]
    fn test_visited_locations_roundtrip() {
        let mut world = WorldState::new();
        world.mark_visited(LocationId(2));
        world.mark_visited(LocationId(3));
        let npc_manager = NpcManager::new();

        let snapshot = GameSnapshot::capture(&world, &npc_manager);
        assert_eq!(snapshot.visited_locations.len(), 3); // 1 (start) + 2

        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: GameSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.visited_locations.len(), 3);
        assert!(restored.visited_locations.contains(&LocationId(1)));
        assert!(restored.visited_locations.contains(&LocationId(2)));
        assert!(restored.visited_locations.contains(&LocationId(3)));
    }

    #[test]
    fn test_old_save_backward_compat_visited() {
        // Simulate an old save JSON without visited_locations field
        let json = r#"{
            "player_location": 5,
            "weather": "Clear",
            "text_log": [],
            "clock": {"game_time": "1820-03-20T08:00:00Z", "speed_factor": 36.0, "paused": false},
            "npcs": [],
            "last_tier2_game_time": null
        }"#;
        let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
        // serde(default) gives empty set
        assert!(snapshot.visited_locations.is_empty());

        // But restore inserts player location
        let mut world = WorldState::new();
        let mut npcs = NpcManager::new();
        snapshot.restore(&mut world, &mut npcs);
        assert!(world.visited_locations.contains(&LocationId(5)));
    }

    #[test]
    fn test_npc_snapshot_with_relationships() {
        let mut npc = make_test_npc(1, 1);
        use crate::npc::types::{Relationship, RelationshipKind};
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.7));

        let snap = NpcSnapshot::from_npc(&npc);
        let json = serde_json::to_string(&snap).unwrap();
        let restored: NpcSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.relationships.len(), 1);
        let rel = restored.relationships.get(&NpcId(2)).unwrap();
        assert!((rel.strength - 0.7).abs() < f64::EPSILON);
    }
}
