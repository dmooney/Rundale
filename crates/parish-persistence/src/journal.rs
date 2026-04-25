//! Journal event types for the real-time event log.
//!
//! Every meaningful state mutation is represented as a [`WorldEvent`] and
//! appended to the journal. On crash recovery, events are replayed from
//! the last snapshot to reconstruct current state.

use serde::{Deserialize, Serialize};

use parish_types::LocationId;
use parish_types::NpcId;

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
        /// Travel time in minutes (used on replay to advance the clock).
        ///
        /// Optional and `#[serde(default)]` for backwards compatibility with
        /// legacy journal rows written before this field was added.
        #[serde(default)]
        minutes: Option<i64>,
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

/// Upper bound (one game-week) on a single `ClockAdvanced` minutes value.
///
/// Journal rows with a value outside `(0, MAX_MINUTES_PER_EVENT)` are skipped
/// on replay to keep monotonic-clock invariants intact even if the journal is
/// corrupted. See [`replay_journal`].
const MAX_MINUTES_PER_EVENT: i64 = 60 * 24 * 7;

/// Replays a sequence of journal events onto live game state.
///
/// Applies each event in order to bring the world and NPC manager
/// up to date from the last snapshot. Events that reference unknown
/// NPCs are silently skipped (the NPC may have been removed).
///
/// `ClockAdvanced` events with non-positive or out-of-range minutes are
/// skipped with a warning so a corrupted journal cannot brick a save by
/// moving the clock backward or jumping weeks into the future.
///
/// `PlayerMoved` events additionally:
/// - record an edge traversal in `world.edge_traversals` when `from` and
///   `to` are direct neighbours in the graph (so fog-of-war "worn paths"
///   survive crash recovery),
/// - advance the clock by `minutes` when the journal row carries it, and
/// - trigger an `NpcManager::assign_tiers` call at the end of the replay
///   so cognitive tiers reflect the player's final position.
pub fn replay_journal(
    world: &mut parish_world::WorldState,
    npc_manager: &mut parish_npc::manager::NpcManager,
    events: &[WorldEvent],
) {
    let mut player_moved = false;
    for event in events {
        match event {
            WorldEvent::PlayerMoved { from, to, minutes } => {
                // Record the edge traversal so fog-of-war "worn paths"
                // survive crash recovery. Only record when `from` and `to`
                // are direct neighbours — otherwise we would fabricate an
                // edge that does not exist in the graph.
                if world
                    .graph
                    .neighbors(*from)
                    .iter()
                    .any(|(id, _)| *id == *to)
                {
                    world.record_path_traversal(&[*from, *to]);
                }
                if let Some(m) = minutes
                    && *m > 0
                    && *m < MAX_MINUTES_PER_EVENT
                {
                    world.clock.advance(*m);
                }
                world.player_location = *to;
                world.visited_locations.insert(*to);
                player_moved = true;
            }
            WorldEvent::NpcMoved { npc_id, to, .. } => {
                if let Some(npc) = npc_manager.get_mut(*npc_id) {
                    npc.location = *to;
                    npc.state = parish_npc::types::NpcState::Present;
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
            WorldEvent::WeatherChanged { new_weather } => match new_weather.parse() {
                Ok(w) => world.weather = w,
                Err(_) => {
                    tracing::warn!(
                        "Invalid weather in journal: '{}', defaulting to Clear",
                        new_weather
                    );
                    world.weather = parish_types::Weather::Clear;
                }
            },
            WorldEvent::MemoryAdded { npc_id, content } => {
                if let Some(npc) = npc_manager.get_mut(*npc_id) {
                    use parish_npc::memory::MemoryEntry;
                    npc.memory.add(MemoryEntry {
                        timestamp: world.clock.now(),
                        content: content.clone(),
                        participants: vec![*npc_id],
                        location: npc.location,
                        kind: None,
                    });
                }
            }
            WorldEvent::ClockAdvanced { minutes } => {
                // Reject non-positive or implausibly large values so a
                // corrupted journal cannot violate the monotonic-clock
                // invariants relied on by tier-transition bookkeeping.
                if *minutes > 0 && *minutes < MAX_MINUTES_PER_EVENT {
                    world.clock.advance(*minutes);
                } else {
                    tracing::warn!(
                        "skipping invalid ClockAdvanced({}) in journal replay",
                        minutes
                    );
                }
            }
            WorldEvent::DialogueOccurred { .. } => {
                // Dialogue events are recorded for history but don't
                // mutate game state during replay (the memory and mood
                // changes are recorded as separate events).
            }
        }
    }

    // After any player movement, reassign cognitive tiers so Tier 1
    // candidates reflect the player's final position rather than the
    // stale snapshot-time location.
    if player_moved {
        let _ = npc_manager.assign_tiers(world, &[]);
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
            minutes: None,
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
            minutes: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"PlayerMoved\""));
    }

    #[test]
    fn test_replay_player_moved() {
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: None,
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.player_location, LocationId(2));
    }

    #[test]
    fn test_replay_player_moved_tracks_visited() {
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        assert!(!world.visited_locations.contains(&LocationId(2)));
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: None,
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert!(world.visited_locations.contains(&LocationId(2)));
    }

    #[test]
    fn test_replay_weather_changed() {
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let events = vec![WorldEvent::WeatherChanged {
            new_weather: "Storm".to_string(),
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.weather, parish_types::Weather::Storm);
    }

    #[test]
    fn test_replay_clock_advanced() {
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::ClockAdvanced { minutes: 60 }];
        replay_journal(&mut world, &mut npcs, &events);
        let time_after = world.clock.now();
        let diff = (time_after - time_before).num_minutes();
        assert_eq!(diff, 60);
    }

    #[test]
    fn test_replay_npc_mood_changed() {
        use parish_npc::memory::{LongTermMemory, ShortTermMemory};
        use parish_npc::types::NpcState;
        use std::collections::HashMap;

        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        npcs.add_npc(parish_npc::Npc {
            id: NpcId(1),
            name: "Test".to_string(),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test".to_string(),
            intelligence: parish_npc::types::Intelligence::default(),
            location: LocationId(1),
            mood: "calm".to_string(),
            home: None,
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
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        // NPC 99 doesn't exist — should not panic
        let events = vec![WorldEvent::NpcMoodChanged {
            npc_id: NpcId(99),
            mood: "angry".to_string(),
        }];
        replay_journal(&mut world, &mut npcs, &events);
    }

    #[test]
    fn test_replay_multiple_events() {
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let events = vec![
            WorldEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
                minutes: None,
            },
            WorldEvent::WeatherChanged {
                new_weather: "Fog".to_string(),
            },
            WorldEvent::ClockAdvanced { minutes: 30 },
        ];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.player_location, LocationId(2));
        assert_eq!(world.weather, parish_types::Weather::Fog);
    }

    #[test]
    fn test_replay_rejects_negative_clock_advance() {
        // Regression: #344 — a corrupted journal row with a negative
        // `minutes` must not move the clock backward.
        let mut world = parish_world::WorldState::new();
        world.clock.pause();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::ClockAdvanced { minutes: -60 }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.clock.now(), time_before);
    }

    #[test]
    fn test_replay_rejects_huge_clock_advance() {
        // Regression: #344 — a value beyond the one-week sanity bound is
        // treated as corrupted and skipped, leaving the clock alone.
        let mut world = parish_world::WorldState::new();
        world.clock.pause();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::ClockAdvanced {
            minutes: MAX_MINUTES_PER_EVENT,
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.clock.now(), time_before);
    }

    #[test]
    fn test_replay_rejects_zero_clock_advance() {
        // Zero is treated the same as negative — not a valid mutation.
        let mut world = parish_world::WorldState::new();
        world.clock.pause();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::ClockAdvanced { minutes: 0 }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.clock.now(), time_before);
    }

    #[test]
    fn test_replay_skips_invalid_clock_but_continues() {
        // An invalid ClockAdvanced must not abort the replay — later
        // events still apply.
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let events = vec![
            WorldEvent::ClockAdvanced { minutes: -5 },
            WorldEvent::WeatherChanged {
                new_weather: "Rain".to_string(),
            },
            WorldEvent::ClockAdvanced { minutes: 15 },
        ];
        let time_before = world.clock.now();
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.weather, parish_types::Weather::LightRain);
        let diff = (world.clock.now() - time_before).num_minutes();
        assert_eq!(diff, 15, "valid advance still applied after invalid one");
    }

    #[test]
    fn test_replay_player_moved_advances_clock_when_minutes_present() {
        // Regression: #345 — journal rows written with travel minutes
        // should advance the clock on replay so crash-recovered state
        // matches uninterrupted state.
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: Some(12),
        }];
        replay_journal(&mut world, &mut npcs, &events);
        let diff = (world.clock.now() - time_before).num_minutes();
        assert_eq!(diff, 12);
    }

    #[test]
    fn test_replay_player_moved_legacy_row_leaves_clock_untouched() {
        // Legacy rows (minutes = None) behave as before — no clock change
        // from PlayerMoved alone.
        let mut world = parish_world::WorldState::new();
        world.clock.pause();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: None,
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.clock.now(), time_before);
    }

    #[test]
    fn test_replay_player_moved_legacy_row_deserializes() {
        // Rows written before `minutes` existed must still deserialize.
        let legacy_json = r#"{"type":"PlayerMoved","from":1,"to":2}"#;
        let event: WorldEvent = serde_json::from_str(legacy_json).unwrap();
        match event {
            WorldEvent::PlayerMoved { from, to, minutes } => {
                assert_eq!(from, LocationId(1));
                assert_eq!(to, LocationId(2));
                assert_eq!(minutes, None);
            }
            _ => panic!("expected PlayerMoved"),
        }
    }

    #[test]
    fn test_replay_player_moved_rejects_out_of_range_minutes() {
        // Guard #344's bounds when minutes arrive via PlayerMoved too.
        let mut world = parish_world::WorldState::new();
        world.clock.pause();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let time_before = world.clock.now();
        let events = vec![
            WorldEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
                minutes: Some(-5),
            },
            WorldEvent::PlayerMoved {
                from: LocationId(2),
                to: LocationId(3),
                minutes: Some(MAX_MINUTES_PER_EVENT),
            },
        ];
        replay_journal(&mut world, &mut npcs, &events);
        assert_eq!(world.clock.now(), time_before);
        // Player still moved — only the clock advance was suppressed.
        assert_eq!(world.player_location, LocationId(3));
    }

    #[test]
    fn test_replay_player_moved_skips_edge_traversal_for_non_neighbours() {
        // If `from` and `to` are not adjacent in the graph, do not
        // fabricate an edge traversal for an edge that does not exist.
        let mut world = parish_world::WorldState::new();
        let mut npcs = parish_npc::manager::NpcManager::new();
        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(42),
            to: LocationId(99),
            minutes: None,
        }];
        replay_journal(&mut world, &mut npcs, &events);
        assert!(world.edge_traversals.is_empty());
    }

    #[test]
    fn test_replay_player_moved_records_edge_traversal_for_direct_neighbours() {
        // Regression: #345 — fog-of-war "worn paths" should survive crash
        // recovery. When `from` and `to` are direct graph neighbours, a
        // traversal is recorded.
        let graph_json = r#"{
            "locations": [
                {"id": 1, "name": "A", "description_template": "a",
                 "indoor": false, "public": true, "lat": 0.0, "lon": 0.0,
                 "connections": [{"target": 2, "path_description": "p"}],
                 "associated_npcs": [], "mythological_significance": null},
                {"id": 2, "name": "B", "description_template": "b",
                 "indoor": false, "public": true, "lat": 0.0, "lon": 0.001,
                 "connections": [{"target": 1, "path_description": "p"}],
                 "associated_npcs": [], "mythological_significance": null}
            ]
        }"#;
        let graph = parish_world::graph::WorldGraph::load_from_str(graph_json).unwrap();

        let mut world = parish_world::WorldState::new();
        world.graph = graph;
        world.player_location = LocationId(1);
        let mut npcs = parish_npc::manager::NpcManager::new();

        let events = vec![WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: Some(5),
        }];
        replay_journal(&mut world, &mut npcs, &events);

        assert_eq!(
            world.edge_traversals.get(&(LocationId(1), LocationId(2))),
            Some(&1),
            "direct neighbour traversal should be recorded"
        );
    }

    #[test]
    fn test_all_event_types_covered() {
        // Ensure every variant has an event_type string
        let events = vec![
            WorldEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
                minutes: None,
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
