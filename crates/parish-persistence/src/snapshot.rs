//! Snapshot serialization — captures and restores dynamic game state.
//!
//! The [`GameSnapshot`] struct is a fully serializable representation of
//! all dynamic game state. Static data (world graph, location map) is
//! excluded because it's loaded from data files on startup.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use parish_types::ConversationLog;
use parish_types::GossipNetwork;

/// Serde helpers for `edge_traversals: HashMap<(LocationId, LocationId), u32>`.
///
/// JSON map keys must be strings, but `(LocationId, LocationId)` is a tuple.
/// We serialize as a list of `[from, to, count]` arrays instead.
mod edge_traversals_serde {
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
use parish_npc::Npc;
use parish_npc::memory::{LongTermMemory, ShortTermMemory};
use parish_npc::types::{Intelligence, NpcState, Relationship, SeasonalSchedule};
use parish_types::LocationId;
use parish_types::NpcId;

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
            last_activity: npc.last_activity.clone(),
            is_ill: npc.is_ill,
            doom: npc.doom,
            banshee_heralded: npc.banshee_heralded,
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
            last_activity: self.last_activity,
            is_ill: self.is_ill,
            doom: self.doom,
            banshee_heralded: self.banshee_heralded,
            deflated_summary: None,
            reaction_log: parish_npc::reactions::ReactionLog::default(),
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
    /// Set of NPC ids that know the player's name.
    #[serde(default)]
    pub npcs_who_know_player_name: HashSet<NpcId>,
}

impl GameSnapshot {
    /// Captures a snapshot from live game state.
    pub fn capture(
        world: &parish_world::WorldState,
        npc_manager: &parish_npc::manager::NpcManager,
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
            last_tier3_game_time: npc_manager.last_tier3_game_time(),
            last_tier4_game_time: npc_manager.last_tier4_game_time(),
            introduced_npcs: npc_manager.introduced_set(),
            visited_locations: world.visited_locations.clone(),
            edge_traversals: world.edge_traversals.clone(),
            gossip_network: world.gossip_network.clone(),
            conversation_log: world.conversation_log.clone(),
            player_name: world.player_name.clone(),
            npcs_who_know_player_name: npc_manager.player_name_known_set(),
        }
    }

    /// Restores this snapshot into live game state.
    ///
    /// Replaces the dynamic fields of `world` and rebuilds the `npc_manager`
    /// from the snapshot. The world graph is left untouched (it's static data
    /// loaded from files), but the legacy `locations` map is back-filled from
    /// the graph so that [`parish_world::WorldState::current_location`] never
    /// panics for a player location that's present in the graph.
    pub fn restore(
        self,
        world: &mut parish_world::WorldState,
        npc_manager: &mut parish_npc::manager::NpcManager,
    ) {
        use parish_types::{GameClock, Location};

        // Restore clock
        let mut clock = GameClock::new(self.clock.game_time);
        if self.clock.paused {
            clock.pause();
        }
        // Set speed by finding the matching preset, or use custom factor
        let factor = self.clock.speed_factor;
        use parish_types::GameSpeed;
        let speed = GameSpeed::ALL
            .iter()
            .copied()
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
        world.weather = self.weather.parse().unwrap_or(parish_types::Weather::Clear);
        world.text_log = self.text_log;

        // Back-fill the legacy `locations` map from the graph so that
        // `current_location()` won't panic if the snapshot's player location
        // was never inserted via movement. Mirrors `WorldState::from_parish_file`.
        // Uses `or_insert_with` so any already-populated entries are preserved.
        for loc_id in world.graph.location_ids() {
            if let Some(data) = world.graph.get(loc_id) {
                world.locations.entry(loc_id).or_insert_with(|| Location {
                    id: loc_id,
                    name: data.name.clone(),
                    description: data.description_template.clone(),
                    indoor: data.indoor,
                    public: data.public,
                    lat: data.lat,
                    lon: data.lon,
                });
            }
        }

        // Last-resort guard: if the player's location is absent from both the
        // graph and the legacy map (e.g. an empty graph, or a stale save whose
        // location was removed from the current parish data), insert a
        // placeholder so `current_location()` stays total.
        world
            .locations
            .entry(self.player_location)
            .or_insert_with(|| Location {
                id: self.player_location,
                name: "Unknown location".to_string(),
                description: "The surroundings are hazy and unfamiliar.".to_string(),
                indoor: false,
                public: false,
                lat: 0.0,
                lon: 0.0,
            });

        // Restore visited locations; ensure current position is always visited
        world.visited_locations = self.visited_locations;
        world.visited_locations.insert(self.player_location);

        // Restore edge traversal counts
        world.edge_traversals = self.edge_traversals;

        // Restore NPCs
        *npc_manager = parish_npc::manager::NpcManager::new();
        for npc_snap in self.npcs {
            npc_manager.add_npc(npc_snap.into_npc());
        }
        if let Some(t) = self.last_tier2_game_time {
            npc_manager.record_tier2_tick(t);
        }
        if let Some(t) = self.last_tier3_game_time {
            npc_manager.record_tier3_tick(t);
        }
        if let Some(t) = self.last_tier4_game_time {
            npc_manager.record_tier4_tick(t);
        }
        npc_manager.restore_introduced_set(self.introduced_npcs);

        // Restore gossip network
        world.gossip_network = self.gossip_network;

        // Restore conversation log
        world.conversation_log = self.conversation_log;

        // Restore player name
        world.player_name = self.player_name;

        // Restore NPC knowledge of player name
        npc_manager.restore_player_name_known(self.npcs_who_know_player_name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use parish_npc::Npc;
    use parish_npc::manager::NpcManager;
    use parish_npc::memory::{LongTermMemory, ShortTermMemory};
    use parish_npc::types::NpcState;
    use parish_world::WorldState;

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
            reaction_log: parish_npc::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
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
        world.weather = parish_types::Weather::LightRain;
        world.log("Test entry".to_string());
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(make_test_npc(1, 1));

        let snapshot = GameSnapshot::capture(&world, &npc_manager);

        // Create a fresh world and restore
        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        snapshot.restore(&mut new_world, &mut new_npcs);

        assert_eq!(new_world.weather, parish_types::Weather::LightRain);
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
    fn test_game_snapshot_ludicrous_speed_roundtrip() {
        use parish_types::GameSpeed;

        let mut world = WorldState::new();
        world.clock.set_speed(GameSpeed::Ludicrous);
        let npc_manager = NpcManager::new();

        let snapshot = GameSnapshot::capture(&world, &npc_manager);
        assert!(
            (snapshot.clock.speed_factor - GameSpeed::Ludicrous.factor()).abs() < 0.01,
            "captured factor should match Ludicrous"
        );

        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        snapshot.restore(&mut new_world, &mut new_npcs);

        assert!(
            (new_world.clock.speed_factor() - GameSpeed::Ludicrous.factor()).abs() < 0.01,
            "restored speed factor should match Ludicrous"
        );
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
        use parish_npc::types::{Relationship, RelationshipKind};
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.7));

        let snap = NpcSnapshot::from_npc(&npc);
        let json = serde_json::to_string(&snap).unwrap();
        let restored: NpcSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.relationships.len(), 1);
        let rel = restored.relationships.get(&NpcId(2)).unwrap();
        assert!((rel.strength - 0.7).abs() < f64::EPSILON);
    }

    /// Regression for #277 — the set of introduced NPCs must survive a
    /// capture/restore cycle; otherwise players lose all name-recognition
    /// state across save/load.
    #[test]
    fn test_introduced_npcs_roundtrip() {
        let world = WorldState::new();
        let mut npc_manager = NpcManager::new();
        npc_manager.add_npc(make_test_npc(1, 1));
        npc_manager.add_npc(make_test_npc(2, 1));
        npc_manager.add_npc(make_test_npc(3, 1));
        npc_manager.mark_introduced(NpcId(1));
        npc_manager.mark_introduced(NpcId(3));
        assert!(npc_manager.is_introduced(NpcId(1)));
        assert!(!npc_manager.is_introduced(NpcId(2)));
        assert!(npc_manager.is_introduced(NpcId(3)));

        let snapshot = GameSnapshot::capture(&world, &npc_manager);
        assert_eq!(snapshot.introduced_npcs.len(), 2);
        assert!(snapshot.introduced_npcs.contains(&NpcId(1)));
        assert!(snapshot.introduced_npcs.contains(&NpcId(3)));

        // Round-trip through JSON to simulate a real save/load.
        let json = serde_json::to_string(&snapshot).unwrap();
        let restored: GameSnapshot = serde_json::from_str(&json).unwrap();

        let mut new_world = WorldState::new();
        let mut new_npcs = NpcManager::new();
        restored.restore(&mut new_world, &mut new_npcs);

        assert!(new_npcs.is_introduced(NpcId(1)));
        assert!(!new_npcs.is_introduced(NpcId(2)));
        assert!(new_npcs.is_introduced(NpcId(3)));
    }

    /// Old saves predating the `introduced_npcs` field must deserialize
    /// cleanly with an empty set (serde `default`), so loading a legacy
    /// save doesn't produce a deserialization error.
    #[test]
    fn test_old_save_backward_compat_introduced_npcs() {
        let json = r#"{
            "player_location": 1,
            "weather": "Clear",
            "text_log": [],
            "clock": {"game_time": "1820-03-20T08:00:00Z", "speed_factor": 36.0, "paused": false},
            "npcs": [],
            "last_tier2_game_time": null
        }"#;
        let snapshot: GameSnapshot = serde_json::from_str(json).unwrap();
        assert!(snapshot.introduced_npcs.is_empty());

        let mut world = WorldState::new();
        let mut npcs = NpcManager::new();
        snapshot.restore(&mut world, &mut npcs);
        assert_eq!(npcs.introduced_count(), 0);
    }

    /// Regression for #286 — `current_location()` must not panic after
    /// restoring a snapshot into a `WorldState::new()` (empty graph, only
    /// Crossroads in the legacy map) when the player's saved location
    /// differs from any entry already present. The fallback placeholder
    /// guarantees `current_location()` is total.
    #[test]
    fn test_restore_current_location_does_not_panic_with_empty_graph() {
        let mut world = WorldState::new();
        // `new()` only registers LocationId(1); a saved location of 99 is
        // unknown to both the legacy `locations` map and the empty graph.
        let mut npc_manager = NpcManager::new();
        let mut snapshot = GameSnapshot::capture(&world, &npc_manager);
        snapshot.player_location = LocationId(99);

        snapshot.restore(&mut world, &mut npc_manager);

        // No panic — the fallback placeholder is inserted for id 99.
        let loc = world.current_location();
        assert_eq!(loc.id, LocationId(99));
        assert_eq!(loc.name, "Unknown location");
    }

    /// Regression for #286 — when the graph does contain the player's
    /// saved location but the legacy `locations` map does not (e.g. a
    /// snapshot restored into a fresh `WorldState::new()` before any
    /// movement), `restore()` must back-fill the legacy map so
    /// `current_location()` returns the real location data.
    #[test]
    fn test_restore_populates_legacy_locations_from_graph() {
        use parish_world::graph::WorldGraph;

        // Minimal graph with id 2 ("Darcy's Pub") that's NOT in
        // `WorldState::new()`'s legacy `locations` map. The graph loader
        // rejects orphan locations, so we include a second connected node.
        let graph_json = r#"{
            "locations": [
                {
                    "id": 2,
                    "name": "Darcy's Pub",
                    "description_template": "Warm pub interior.",
                    "indoor": true,
                    "public": true,
                    "lat": 53.6195,
                    "lon": -8.0925,
                    "connections": [
                        {"target": 7, "path_description": "a short lane"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null
                },
                {
                    "id": 7,
                    "name": "The Crossroads",
                    "description_template": "A quiet junction.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.618,
                    "lon": -8.095,
                    "connections": [
                        {"target": 2, "path_description": "a short lane back"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null
                }
            ]
        }"#;
        let graph = WorldGraph::load_from_str(graph_json).unwrap();

        let mut world = WorldState::new();
        world.graph = graph;
        // Deliberately leave `world.locations` as only the Crossroads.
        assert!(!world.locations.contains_key(&LocationId(2)));

        let mut npc_manager = NpcManager::new();
        let mut snapshot = GameSnapshot::capture(&world, &npc_manager);
        snapshot.player_location = LocationId(2);

        snapshot.restore(&mut world, &mut npc_manager);

        // Legacy map is back-filled from the graph, so the real data lands.
        let loc = world.current_location();
        assert_eq!(loc.id, LocationId(2));
        assert_eq!(loc.name, "Darcy's Pub");
        assert!(loc.indoor);
    }
}
