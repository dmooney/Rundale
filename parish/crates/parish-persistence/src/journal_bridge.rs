//! Bridge between the game event bus and the persistence journal.
//!
//! Converts [`GameEvent`](parish_types::GameEvent) from the
//! broadcast event bus into [`WorldEvent`](super::journal::WorldEvent)
//! for the persistence journal. This allows the journal to record
//! crash-recoverable mutations from the higher-level game events.

use crate::journal::WorldEvent;
use parish_types::GameEvent;

/// Converts a game event into a persistence journal event, if applicable.
///
/// Not all game events map to journal events. Returns `None` for events
/// that are informational and don't represent a state mutation that needs
/// to be replayed during crash recovery.
pub fn to_journal_event(event: &GameEvent) -> Option<WorldEvent> {
    match event {
        GameEvent::ReactionRecorded {
            npc_id,
            direction,
            emoji,
            context,
            timestamp,
        } => Some(WorldEvent::ReactionRecorded {
            npc_id: *npc_id,
            direction: *direction,
            emoji: emoji.clone(),
            context: context.clone(),
            timestamp: *timestamp,
        }),
        GameEvent::DialogueOccurred {
            npc_id,
            summary,
            player_said,
            npc_said,
            ..
        } => Some(WorldEvent::DialogueOccurred {
            npc_id: *npc_id,
            player_said: player_said.clone().unwrap_or_default(),
            npc_said: npc_said.clone().unwrap_or_else(|| summary.clone()),
        }),
        GameEvent::MoodChanged {
            npc_id, new_mood, ..
        } => Some(WorldEvent::NpcMoodChanged {
            npc_id: *npc_id,
            mood: new_mood.clone(),
        }),
        GameEvent::RelationshipChanged {
            npc_a,
            npc_b,
            delta,
            ..
        } => Some(WorldEvent::RelationshipChanged {
            npc_a: *npc_a,
            npc_b: *npc_b,
            delta: *delta,
        }),
        GameEvent::WeatherChanged { new_weather, .. } => Some(WorldEvent::WeatherChanged {
            new_weather: new_weather.clone(),
        }),
        GameEvent::PlayerTaskAssigned { task, .. }
        | GameEvent::PlayerTaskProgressed { task, .. } => {
            Some(WorldEvent::PlayerTaskStateChanged { task: task.clone() })
        }
        // Festival, life events, NPC arrival/departure, and player movement
        // are informational on the broadcast bus.
        // The persistence journal already has its own `PlayerMoved` event
        // sourced from the movement applier.
        GameEvent::FestivalStarted { .. }
        | GameEvent::LifeEvent { .. }
        | GameEvent::NpcArrived { .. }
        | GameEvent::NpcDeparted { .. }
        | GameEvent::NpcActivity { .. }
        | GameEvent::GossipSpread { .. }
        | GameEvent::AddressedAbsentNpc { .. }
        | GameEvent::PlayerMoved { .. }
        | GameEvent::NpcInteraction { .. } => None,
    }
}

/// Drains a broadcast receiver and converts events to journal entries.
///
/// This is meant to be called periodically (e.g., during snapshot) to
/// flush queued events to persistence. Returns all convertible events.
#[cfg(test)]
pub(crate) fn drain_events(
    rx: &mut tokio::sync::broadcast::Receiver<GameEvent>,
) -> Vec<WorldEvent> {
    let mut journal_events = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let Some(je) = to_journal_event(&event) {
            journal_events.push(je);
        }
    }
    journal_events
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use parish_types::LocationId;
    use parish_types::NpcId;
    use parish_types::{PlayerTask, PlayerTaskId, TaskStatus};

    fn test_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
    }

    #[test]
    fn test_mood_changed_converts() {
        let event = GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "happy".to_string(),
            location: LocationId(1),
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "NpcMoodChanged");
    }

    #[test]
    fn reaction_recorded_preserves_direction_for_recovery() {
        let event = GameEvent::ReactionRecorded {
            npc_id: NpcId(7),
            direction: parish_types::ReactionDirection::NpcToPlayer,
            emoji: "👀".to_string(),
            context: "I have no money".to_string(),
            timestamp: test_time(),
        };
        assert_eq!(
            to_journal_event(&event),
            Some(WorldEvent::ReactionRecorded {
                npc_id: NpcId(7),
                direction: parish_types::ReactionDirection::NpcToPlayer,
                emoji: "👀".to_string(),
                context: "I have no money".to_string(),
                timestamp: test_time(),
            })
        );
    }

    #[test]
    fn test_dialogue_converts() {
        let event = GameEvent::DialogueOccurred {
            npc_id: NpcId(1),
            location: LocationId(1),
            summary: "discussed farming".to_string(),
            player_said: None,
            npc_said: None,
            request_id: None,
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "DialogueOccurred");
    }

    #[test]
    fn test_relationship_converts() {
        let event = GameEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: 0.1,
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "RelationshipChanged");
    }

    #[test]
    fn test_weather_converts() {
        let event = GameEvent::WeatherChanged {
            new_weather: "Storm".to_string(),
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "WeatherChanged");
    }

    #[test]
    fn test_festival_returns_none() {
        let event = GameEvent::FestivalStarted {
            name: "May Day".to_string(),
            timestamp: test_time(),
            location: None,
        };
        assert!(to_journal_event(&event).is_none());
    }

    #[test]
    fn test_life_event_returns_none() {
        let event = GameEvent::LifeEvent {
            npc_id: NpcId(1),
            description: "got married".to_string(),
            location: LocationId(1),
            timestamp: test_time(),
        };
        assert!(to_journal_event(&event).is_none());
    }

    #[test]
    fn player_task_events_capture_canonical_post_mutation_state() {
        let task = PlayerTask {
            id: PlayerTaskId(1),
            description: "Dig over the potato patch.".to_string(),
            assigned_by: NpcId(7),
            location: LocationId(9),
            assigned_at: test_time(),
            status: TaskStatus::Assigned,
            started_at: None,
            completed_at: None,
            last_matching_action: None,
        };
        let assigned = GameEvent::PlayerTaskAssigned {
            task: task.clone(),
            timestamp: test_time(),
        };
        let progressed = GameEvent::PlayerTaskProgressed {
            task: PlayerTask {
                status: TaskStatus::InProgress,
                started_at: Some(test_time()),
                last_matching_action: Some("I dig over the potato patch.".to_string()),
                ..task.clone()
            },
            previous_status: TaskStatus::Assigned,
            action: "I dig over the potato patch.".to_string(),
            timestamp: test_time(),
        };

        assert_eq!(
            to_journal_event(&assigned),
            Some(WorldEvent::PlayerTaskStateChanged { task: task.clone() })
        );
        assert_eq!(
            to_journal_event(&progressed),
            Some(WorldEvent::PlayerTaskStateChanged {
                task: PlayerTask {
                    status: TaskStatus::InProgress,
                    started_at: Some(test_time()),
                    last_matching_action: Some("I dig over the potato patch.".to_string()),
                    ..task
                },
            })
        );
    }

    #[test]
    fn test_drain_events() {
        let bus = parish_types::EventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "happy".to_string(),
            location: LocationId(1),
            timestamp: test_time(),
        });
        bus.publish(GameEvent::FestivalStarted {
            name: "test".to_string(),
            timestamp: test_time(),
            location: None,
        });
        bus.publish(GameEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
            timestamp: test_time(),
        });

        let events = drain_events(&mut rx);
        // Festival is filtered out, so 2 events
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_npc_arrived_returns_none() {
        let event = GameEvent::NpcArrived {
            npc_id: NpcId(5),
            location: LocationId(10),
            timestamp: test_time(),
        };
        assert!(to_journal_event(&event).is_none());
    }

    #[test]
    fn test_npc_departed_returns_none() {
        let event = GameEvent::NpcDeparted {
            npc_id: NpcId(5),
            location: LocationId(10),
            to: LocationId(11),
            timestamp: test_time(),
        };
        assert!(to_journal_event(&event).is_none());
    }
}
