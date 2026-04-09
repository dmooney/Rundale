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
        GameEvent::DialogueOccurred {
            npc_id, summary, ..
        } => Some(WorldEvent::DialogueOccurred {
            npc_id: *npc_id,
            player_said: String::new(),
            npc_said: summary.clone(),
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
        GameEvent::NpcArrived {
            npc_id, location, ..
        } => Some(WorldEvent::NpcMoved {
            npc_id: *npc_id,
            from: *location, // best approximation — arrival doesn't track origin
            to: *location,
        }),
        GameEvent::NpcDeparted {
            npc_id, location, ..
        } => Some(WorldEvent::NpcMoved {
            npc_id: *npc_id,
            from: *location,
            to: *location, // departure doesn't track destination
        }),
        GameEvent::WeatherChanged { new_weather, .. } => Some(WorldEvent::WeatherChanged {
            new_weather: new_weather.clone(),
        }),
        // Festival and life events are informational — no state mutation to replay
        GameEvent::FestivalStarted { .. } | GameEvent::LifeEvent { .. } => None,
    }
}

/// Drains a broadcast receiver and converts events to journal entries.
///
/// This is meant to be called periodically (e.g., during snapshot) to
/// flush queued events to persistence. Returns all convertible events.
pub fn drain_events(rx: &mut tokio::sync::broadcast::Receiver<GameEvent>) -> Vec<WorldEvent> {
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

    fn test_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
    }

    #[test]
    fn test_mood_changed_converts() {
        let event = GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "happy".to_string(),
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "NpcMoodChanged");
    }

    #[test]
    fn test_dialogue_converts() {
        let event = GameEvent::DialogueOccurred {
            npc_id: NpcId(1),
            summary: "discussed farming".to_string(),
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
        };
        assert!(to_journal_event(&event).is_none());
    }

    #[test]
    fn test_life_event_returns_none() {
        let event = GameEvent::LifeEvent {
            npc_id: NpcId(1),
            description: "got married".to_string(),
            timestamp: test_time(),
        };
        assert!(to_journal_event(&event).is_none());
    }

    #[test]
    fn test_drain_events() {
        let bus = parish_types::EventBus::new();
        let mut rx = bus.subscribe();

        bus.publish(GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "happy".to_string(),
            timestamp: test_time(),
        });
        bus.publish(GameEvent::FestivalStarted {
            name: "test".to_string(),
            timestamp: test_time(),
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
    fn test_npc_arrived_converts() {
        let event = GameEvent::NpcArrived {
            npc_id: NpcId(5),
            location: LocationId(10),
            timestamp: test_time(),
        };
        let journal = to_journal_event(&event).unwrap();
        assert_eq!(journal.event_type(), "NpcMoved");
    }
}
