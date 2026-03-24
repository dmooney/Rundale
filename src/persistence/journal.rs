//! Journal event types for the real-time event log.
//!
//! Every meaningful state mutation is represented as a [`WorldEvent`] and
//! appended to the journal. On crash recovery, events are replayed from
//! the last snapshot to reconstruct current state.

use serde::{Deserialize, Serialize};

use crate::npc::NpcId;
use crate::world::LocationId;

/// A discrete state mutation in the game world.
///
/// These events form the append-only journal. Each variant captures
/// enough information to replay the mutation during crash recovery.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum WorldEvent {
    /// The player moved between locations.
    PlayerMoved {
        /// Origin location.
        from: LocationId,
        /// Destination location.
        to: LocationId,
    },
    /// An NPC moved between locations.
    NpcMoved {
        /// Which NPC moved.
        npc_id: NpcId,
        /// Origin location.
        from: LocationId,
        /// Destination location.
        to: LocationId,
    },
    /// An NPC's mood changed.
    NpcMoodChanged {
        /// Which NPC's mood changed.
        npc_id: NpcId,
        /// The new mood.
        mood: String,
    },
    /// A relationship strength changed between two NPCs.
    RelationshipChanged {
        /// First NPC in the relationship.
        npc_a: NpcId,
        /// Second NPC in the relationship.
        npc_b: NpcId,
        /// The strength delta applied.
        delta: f64,
    },
    /// A dialogue occurred between the player and an NPC.
    DialogueOccurred {
        /// Which NPC spoke.
        npc_id: NpcId,
        /// What the player said.
        player_said: String,
        /// What the NPC said.
        npc_said: String,
    },
    /// The weather changed.
    WeatherChanged {
        /// The new weather description.
        new_weather: String,
    },
    /// A memory was added to an NPC's short-term memory.
    MemoryAdded {
        /// Which NPC gained the memory.
        npc_id: NpcId,
        /// The memory content.
        content: String,
    },
    /// The game clock was advanced by a number of minutes.
    ClockAdvanced {
        /// Number of game minutes advanced.
        minutes: i64,
    },
}

impl WorldEvent {
    /// Returns the discriminant name for the `event_type` column.
    pub fn event_type(&self) -> &str {
        match self {
            WorldEvent::PlayerMoved { .. } => "PlayerMoved",
            WorldEvent::NpcMoved { .. } => "NpcMoved",
            WorldEvent::NpcMoodChanged { .. } => "NpcMoodChanged",
            WorldEvent::RelationshipChanged { .. } => "RelationshipChanged",
            WorldEvent::DialogueOccurred { .. } => "DialogueOccurred",
            WorldEvent::WeatherChanged { .. } => "WeatherChanged",
            WorldEvent::MemoryAdded { .. } => "MemoryAdded",
            WorldEvent::ClockAdvanced { .. } => "ClockAdvanced",
        }
    }
}

/// Replays a sequence of journal events onto live game state.
///
/// Applies each event in order to bring the world and NPC manager
/// up to date from the last snapshot. Events that reference unknown
/// NPCs are silently skipped (the NPC may have been removed).
pub fn replay_journal(
    world: &mut crate::world::WorldState,
    npc_manager: &mut crate::npc::manager::NpcManager,
    events: &[WorldEvent],
) {
    for event in events {
        match event {
            WorldEvent::PlayerMoved { to, .. } => {
                world.player_location = *to;
            }
            WorldEvent::NpcMoved { npc_id, to, .. } => {
                if let Some(npc) = npc_manager.get_mut(*npc_id) {
                    npc.location = *to;
                    npc.state = crate::npc::types::NpcState::Present;
                }
            }
            WorldEvent::NpcMoodChanged { npc_id, mood } => {
                if let Some(npc) = npc_manager.get_mut(*npc_id) {
                    npc.mood = mood.clone();
                }
            }
            WorldEvent::RelationshipChanged {
                npc_a,
                npc_b,
                delta,
            } => {
                if let Some(npc) = npc_manager.get_mut(*npc_a)
                    && let Some(rel) = npc.relationships.get_mut(npc_b)
                {
                    rel.adjust_strength(*delta);
                }
            }
            WorldEvent::WeatherChanged { new_weather } => {
                world.weather = new_weather.parse().unwrap_or(crate::world::Weather::Clear);
            }
            WorldEvent::MemoryAdded { npc_id, content } => {
                if let Some(npc) = npc_manager.get_mut(*npc_id) {
                    use crate::npc::memory::MemoryEntry;
                    npc.memory.add(MemoryEntry {
                        timestamp: world.clock.now(),
                        content: content.clone(),
                        participants: vec![*npc_id],
                        location: npc.location,
                    });
                }
            }
            WorldEvent::ClockAdvanced { minutes } => {
                world.clock.advance(*minutes);
            }
            WorldEvent::DialogueOccurred { .. } => {
                // Dialogue events are recorded for history but don't
                // mutate game state during replay (the memory and mood
                // changes are recorded as separate events).
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_world_event_type_names() {
        let event = WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
        };
        assert_eq!(event.event_type(), "PlayerMoved");

        let event = WorldEvent::NpcMoodChanged {
            npc_id: NpcId(1),
            mood: "happy".to_string(),
        };
        assert_eq!(event.event_type(), "NpcMoodChanged");

        let event = WorldEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
        };
        assert_eq!(event.event_type(), "WeatherChanged");

        let event = WorldEvent::ClockAdvanced { minutes: 30 };
        assert_eq!(event.event_type(), "ClockAdvanced");
    }

    #[test]
    fn test_world_event_serialize_roundtrip() {
        let event = WorldEvent::DialogueOccurred {
            npc_id: NpcId(3),
            player_said: "Hello".to_string(),
            npc_said: "Good day".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: WorldEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn test_world_event_tagged_serialization() {
        let event = WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"PlayerMoved\""));
    }

    #[test]
    fn test_replay_player_moved() {
        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.player_location, LocationId(2));
    }

    #[test]
    fn test_replay_weather_changed() {
        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        let events = vec![WorldEvent::WeatherChanged {
            new_weather: "Storm".to_string(),
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.weather, crate::world::Weather::Storm);
    }

    #[test]
    fn test_replay_clock_advanced() {
        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::ClockAdvanced { minutes: 60 }];
        replay_journal(&mut world, &mut npcs, &events);
        let time_after = world.clock.now();
        let diff = (time_after - time_before).num_minutes();
        assert_eq!(diff, 60);
    }

    #[test]
    fn test_replay_npc_mood_changed() {
        use crate::npc::memory::ShortTermMemory;
        use crate::npc::types::NpcState;
        use std::collections::HashMap;

        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        npcs.add_npc(crate::npc::Npc {
            id: NpcId(1),
            name: "Test".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test".to_string(),
            location: LocationId(1),
            mood: "calm".to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
        });

        let events = vec![WorldEvent::NpcMoodChanged {
            npc_id: NpcId(1),
            mood: "angry".to_string(),
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(npcs.get(NpcId(1)).unwrap().mood, "angry");
    }

    #[test]
    fn test_replay_unknown_npc_skipped() {
        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        // NPC 99 doesn't exist — should not panic
        let events = vec![WorldEvent::NpcMoodChanged {
            npc_id: NpcId(99),
            mood: "angry".to_string(),
        }];
        replay_journal(&mut world, &mut npcs, &events);
    }

    #[test]
    fn test_replay_multiple_events() {
        let mut world = crate::world::WorldState::new();
        let mut npcs = crate::npc::manager::NpcManager::new();
        let events = vec![
            WorldEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
            },
            WorldEvent::WeatherChanged {
                new_weather: "Fog".to_string(),
            },
            WorldEvent::ClockAdvanced { minutes: 30 },
        ];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.player_location, LocationId(2));
        assert_eq!(world.weather, crate::world::Weather::Fog);
    }

    #[test]
    fn test_all_event_types_covered() {
        // Ensure every variant has an event_type string
        let events = vec![
            WorldEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
            },
            WorldEvent::NpcMoved {
                npc_id: NpcId(1),
                from: LocationId(1),
                to: LocationId(2),
            },
            WorldEvent::NpcMoodChanged {
                npc_id: NpcId(1),
                mood: "happy".to_string(),
            },
            WorldEvent::RelationshipChanged {
                npc_a: NpcId(1),
                npc_b: NpcId(2),
                delta: 0.1,
            },
            WorldEvent::DialogueOccurred {
                npc_id: NpcId(1),
                player_said: "hi".to_string(),
                npc_said: "hello".to_string(),
            },
            WorldEvent::WeatherChanged {
                new_weather: "Clear".to_string(),
            },
            WorldEvent::MemoryAdded {
                npc_id: NpcId(1),
                content: "test".to_string(),
            },
            WorldEvent::ClockAdvanced { minutes: 10 },
        ];
        for event in &events {
            assert!(!event.event_type().is_empty());
        }
    }
}
