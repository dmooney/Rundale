//! Cross-tier event bus for publishing and subscribing to game events.
//!
//! The [`EventBus`] wraps a `tokio::sync::broadcast` channel so that
//! multiple subsystems (persistence journal, UI, debug panel) can
//! independently observe world state mutations without tight coupling.
//!
//! Events are named [`GameEvent`] (not `WorldEvent`) to avoid collision
//! with the persistence journal's `WorldEvent`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::ids::{LocationId, NpcId};

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
        /// Where the dialogue happened, captured at publish time. The
        /// location-log subscriber routes by this field instead of
        /// re-resolving the NPC's (possibly newer) current location — the
        /// bus is async, so a schedule tick can move the NPC between
        /// publish and consume (#1035).
        location: LocationId,
        /// Summary of what was said.
        summary: String,
        /// The player's full utterance, if available. Populated by the
        /// player→NPC turn handler; left `None` for synthetic / replay
        /// events that don't have the original text.
        #[serde(default)]
        player_said: Option<String>,
        /// The NPC's full reply, if available. Same caveats as
        /// `player_said` — `None` for non-live event sources.
        #[serde(default)]
        npc_said: Option<String>,
        /// The inference request id that produced this reply, if it came from
        /// a live LLM turn. Lets the on-disk chat transcript correlate a
        /// dialogue line with the matching inference-log entry via
        /// `parish.request_id`. `None` for synthetic / replay events.
        #[serde(default)]
        request_id: Option<u64>,
        /// When the dialogue happened.
        timestamp: DateTime<Utc>,
    },
    /// An NPC's mood changed.
    MoodChanged {
        /// Which NPC's mood changed.
        npc_id: NpcId,
        /// The new mood.
        new_mood: String,
        /// Where the mood change happened, captured at publish time. The
        /// location-log subscriber routes by this field instead of
        /// re-resolving the NPC's (possibly newer) current location — the
        /// bus is async, so a schedule tick can move the NPC between
        /// publish and consume (#1077/#1079, same race as #1035).
        location: LocationId,
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
        /// Where they are heading.
        to: LocationId,
        /// When they departed.
        timestamp: DateTime<Utc>,
    },
    /// An NPC's authored schedule activity at a location.
    ///
    /// Distinct from tier-3 LLM-summarised `last_activity`: this carries
    /// the authored `ScheduleEntry::activity` text (e.g. "tending bar",
    /// "praying", "cuaird visiting") fired deterministically when the
    /// NPC arrives at the location for that schedule window.
    NpcActivity {
        /// Which NPC.
        npc_id: NpcId,
        /// Where they are.
        location: LocationId,
        /// Authored activity text from the NPC's schedule.
        activity: String,
        /// When the activity began.
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
    /// The player successfully moved between two locations.
    ///
    /// Published once per `MovementResult::Arrived` from the
    /// shared movement handler. Used by the character-log writer
    /// to record the journey in `player.md`.
    PlayerMoved {
        /// The location the player departed from.
        from: LocationId,
        /// The location the player arrived at.
        to: LocationId,
        /// When the arrival happened (game-time).
        timestamp: DateTime<Utc>,
    },
    /// A significant life event occurred for an NPC.
    LifeEvent {
        /// Which NPC experienced the event.
        npc_id: NpcId,
        /// Description of the event.
        description: String,
        /// Where the event happened, captured at publish time. The
        /// location-log subscriber routes by this field instead of
        /// re-resolving the NPC's (possibly newer) current location — the
        /// bus is async, so a schedule tick can move the NPC between
        /// publish and consume (#1077/#1079, same race as #1035).
        location: LocationId,
        /// When the event occurred.
        timestamp: DateTime<Utc>,
    },
    /// A Tier 2 simulation tick produced a narrative interaction
    /// between two or more NPCs at a shared location. Carries the
    /// LLM-generated `summary` describing what happened so per-location
    /// and per-character logs record the story beat, not just the
    /// mechanical mood / relationship deltas.
    NpcInteraction {
        /// NPCs who participated. First entry is the prompt's "lead"
        /// NPC; remainder are the others present.
        participants: Vec<NpcId>,
        /// Location where the interaction occurred.
        location: LocationId,
        /// LLM-generated description (verbatim from `Tier2Event.summary`).
        summary: String,
        /// In-fiction game time when the tick fired.
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
            | GameEvent::NpcActivity { timestamp, .. }
            | GameEvent::WeatherChanged { timestamp, .. }
            | GameEvent::FestivalStarted { timestamp, .. }
            | GameEvent::PlayerMoved { timestamp, .. }
            | GameEvent::LifeEvent { timestamp, .. }
            | GameEvent::NpcInteraction { timestamp, .. } => *timestamp,
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
            GameEvent::NpcActivity { .. } => "NpcActivity",
            GameEvent::WeatherChanged { .. } => "WeatherChanged",
            GameEvent::FestivalStarted { .. } => "FestivalStarted",
            GameEvent::PlayerMoved { .. } => "PlayerMoved",
            GameEvent::LifeEvent { .. } => "LifeEvent",
            GameEvent::NpcInteraction { .. } => "NpcInteraction",
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
            location: LocationId(1),
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
            location: LocationId(1),
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
                location: LocationId(1),
                summary: "hi".into(),
                player_said: None,
                npc_said: None,
                request_id: None,
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
            to: LocationId(8),
            timestamp: test_timestamp(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"type\":\"NpcDeparted\""));
    }

    #[test]
    fn test_event_bus_overflow() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        for i in 0..BUS_CAPACITY + 10 {
            bus.publish(GameEvent::MoodChanged {
                npc_id: NpcId(1),
                new_mood: format!("mood {}", i),
                location: LocationId(1),
                timestamp: test_timestamp(),
            });
        }

        let mut count = 0;
        loop {
            match rx.try_recv() {
                Ok(_) => count += 1,
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        assert_eq!(count, BUS_CAPACITY);
    }

    #[test]
    fn test_event_bus_lag() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let bus = EventBus::new();
            let mut rx = bus.subscribe();

            bus.publish(GameEvent::MoodChanged {
                npc_id: NpcId(1),
                new_mood: "first".to_string(),
                location: LocationId(1),
                timestamp: test_timestamp(),
            });
            let _ = rx.recv().await.unwrap();

            for i in 0..BUS_CAPACITY + 10 {
                bus.publish(GameEvent::MoodChanged {
                    npc_id: NpcId(1),
                    new_mood: format!("mood {}", i),
                    location: LocationId(1),
                    timestamp: test_timestamp(),
                });
            }

            let result = rx.recv().await;
            assert!(
                matches!(result, Err(broadcast::error::RecvError::Lagged(_))),
                "expected Lagged error, got {:?}",
                result
            );
        });
    }
}
