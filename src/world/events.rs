//! Cross-tier event bus for publishing and subscribing to game events.
//!
//! The [`EventBus`] wraps a `tokio::sync::broadcast` channel so that
//! multiple subsystems (persistence journal, UI, debug panel) can
//! independently observe world state mutations without tight coupling.
//!
//! Events are named [`GameEvent`] (not `WorldEvent`) to avoid collision
//! with the persistence journal's [`WorldEvent`](crate::persistence::journal::WorldEvent).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::npc::NpcId;
use crate::world::LocationId;

/// Capacity of the broadcast channel.
///
/// Subscribers that fall behind by more than this many events will
/// receive a `RecvError::Lagged` and skip the dropped messages.
const BUS_CAPACITY: usize = 256;

/// A discrete game event published on the event bus.
///
/// These are semantic, cross-tier events — higher-level than the
/// persistence journal's `WorldEvent` which is purely for crash
/// recovery. `GameEvent` captures "what happened in the story"
/// while `WorldEvent` captures "what state mutation to replay".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum GameEvent {
    /// A dialogue occurred between the player and an NPC.
    DialogueOccurred {
        /// Which NPC spoke.
        npc_id: NpcId,
        /// Summary of what was said.
        summary: String,
        /// When the dialogue happened.
        timestamp: DateTime<Utc>,
    },
    /// An NPC's mood changed.
    MoodChanged {
        /// Which NPC's mood changed.
        npc_id: NpcId,
        /// The new mood.
        new_mood: String,
        /// When the mood changed.
        timestamp: DateTime<Utc>,
    },
    /// A relationship strength changed between two NPCs.
    RelationshipChanged {
        /// First NPC in the relationship.
        npc_a: NpcId,
        /// Second NPC in the relationship.
        npc_b: NpcId,
        /// The strength delta applied.
        delta: f64,
        /// When the change occurred.
        timestamp: DateTime<Utc>,
    },
    /// An NPC arrived at a location (entered player's vicinity).
    NpcArrived {
        /// Which NPC arrived.
        npc_id: NpcId,
        /// Where they arrived.
        location: LocationId,
        /// When they arrived.
        timestamp: DateTime<Utc>,
    },
    /// An NPC departed from a location.
    NpcDeparted {
        /// Which NPC departed.
        npc_id: NpcId,
        /// Where they departed from.
        location: LocationId,
        /// When they departed.
        timestamp: DateTime<Utc>,
    },
    /// The weather changed.
    WeatherChanged {
        /// The new weather description.
        new_weather: String,
        /// When the weather changed.
        timestamp: DateTime<Utc>,
    },
    /// A festival or calendar event started.
    FestivalStarted {
        /// Name of the festival.
        name: String,
        /// When the festival started.
        timestamp: DateTime<Utc>,
    },
    /// A significant life event occurred for an NPC.
    LifeEvent {
        /// Which NPC experienced the event.
        npc_id: NpcId,
        /// Description of the event.
        description: String,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
}

impl GameEvent {
    /// Returns the timestamp of this event.
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            GameEvent::DialogueOccurred { timestamp, .. }
            | GameEvent::MoodChanged { timestamp, .. }
            | GameEvent::RelationshipChanged { timestamp, .. }
            | GameEvent::NpcArrived { timestamp, .. }
            | GameEvent::NpcDeparted { timestamp, .. }
            | GameEvent::WeatherChanged { timestamp, .. }
            | GameEvent::FestivalStarted { timestamp, .. }
            | GameEvent::LifeEvent { timestamp, .. } => *timestamp,
        }
    }

    /// Returns the discriminant name for logging/debugging.
    pub fn event_type(&self) -> &str {
        match self {
            GameEvent::DialogueOccurred { .. } => "DialogueOccurred",
            GameEvent::MoodChanged { .. } => "MoodChanged",
            GameEvent::RelationshipChanged { .. } => "RelationshipChanged",
            GameEvent::NpcArrived { .. } => "NpcArrived",
            GameEvent::NpcDeparted { .. } => "NpcDeparted",
            GameEvent::WeatherChanged { .. } => "WeatherChanged",
            GameEvent::FestivalStarted { .. } => "FestivalStarted",
            GameEvent::LifeEvent { .. } => "LifeEvent",
        }
    }
}

/// A broadcast-based event bus for game events.
///
/// Wraps `tokio::sync::broadcast` to decouple event producers
/// (game logic) from consumers (persistence, UI, debug panel).
/// Multiple subscribers can independently consume the same events.
pub struct EventBus {
    /// The sending half of the broadcast channel.
    tx: broadcast::Sender<GameEvent>,
}

impl EventBus {
    /// Creates a new event bus with the default channel capacity.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        Self { tx }
    }

    /// Publishes an event to all current subscribers.
    ///
    /// Returns the number of receivers that received the event.
    /// Returns 0 if there are no active subscribers (which is fine —
    /// events are fire-and-forget).
    pub fn publish(&self, event: GameEvent) -> usize {
        tracing::trace!(event_type = event.event_type(), "Publishing game event");
        self.tx.send(event).unwrap_or(0)
    }

    /// Creates a new subscription to the event bus.
    ///
    /// The returned receiver will see all events published after
    /// this call. If the receiver falls behind by more than
    /// [`BUS_CAPACITY`] events, it will skip the oldest ones.
    pub fn subscribe(&self) -> broadcast::Receiver<GameEvent> {
        self.tx.subscribe()
    }

    /// Returns the current number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for EventBus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("subscribers", &self.tx.receiver_count())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn test_timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap()
    }

    #[test]
    fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        let event = GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "happy".to_string(),
            timestamp: test_timestamp(),
        };
        let count = bus.publish(event.clone());
        assert_eq!(count, 1);

        let received = rx.try_recv().unwrap();
        assert_eq!(received, event);
    }

    #[test]
    fn test_event_bus_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        let event = GameEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
            timestamp: test_timestamp(),
        };
        let count = bus.publish(event.clone());
        assert_eq!(count, 2);

        assert_eq!(rx1.try_recv().unwrap(), event);
        assert_eq!(rx2.try_recv().unwrap(), event);
    }

    #[test]
    fn test_event_bus_no_subscribers() {
        let bus = EventBus::new();
        let event = GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "angry".to_string(),
            timestamp: test_timestamp(),
        };
        // Should not panic with zero subscribers
        let count = bus.publish(event);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_event_bus_subscriber_count() {
        let bus = EventBus::new();
        assert_eq!(bus.subscriber_count(), 0);

        let _rx1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);

        let _rx2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);

        drop(_rx1);
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn test_game_event_timestamp() {
        let ts = test_timestamp();
        let event = GameEvent::NpcArrived {
            npc_id: NpcId(5),
            location: LocationId(10),
            timestamp: ts,
        };
        assert_eq!(event.timestamp(), ts);
    }

    #[test]
    fn test_game_event_type_names() {
        let ts = test_timestamp();
        assert_eq!(
            GameEvent::DialogueOccurred {
                npc_id: NpcId(1),
                summary: "hi".into(),
                timestamp: ts,
            }
            .event_type(),
            "DialogueOccurred"
        );
        assert_eq!(
            GameEvent::FestivalStarted {
                name: "May Day".into(),
                timestamp: ts,
            }
            .event_type(),
            "FestivalStarted"
        );
    }

    #[test]
    fn test_game_event_serialize_roundtrip() {
        let event = GameEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: 0.15,
            timestamp: test_timestamp(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let restored: GameEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn test_game_event_tagged_serialization() {
        let event = GameEvent::NpcDeparted {
            npc_id: NpcId(3),
            location: LocationId(7),
            timestamp: test_timestamp(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"NpcDeparted\""));
    }
}
