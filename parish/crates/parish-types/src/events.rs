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
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

use crate::ids::{LocationId, NpcId};
use crate::player_progress::{PlayerTask, TaskStatus};

/// Authoritative direction of a nonverbal reaction retained in NPC history.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum ReactionDirection {
    /// The player selected a reaction to something the NPC said.
    #[default]
    PlayerToNpc,
    /// The NPC automatically reacted to something the player said.
    NpcToPlayer,
}

/// Capacity of the broadcast channel.
///
/// Subscribers that fall behind by more than this many events will
/// receive a `RecvError::Lagged` and skip the dropped messages.
const BUS_CAPACITY: usize = 256;

/// A game event stamped with the canonical context epoch at publish time.
///
/// Runtime fan-in consumers use this envelope to discard events that were
/// queued before a successful new-game or branch replacement.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextEventEnvelope {
    /// Process-local context generation active when `event` was published.
    pub context_epoch: u64,
    /// Semantic game event.
    pub event: GameEvent,
}

/// A discrete game event published on the event bus.
///
/// These are semantic, cross-tier events — higher-level than the
/// persistence journal's `WorldEvent` which is purely for crash
/// recovery. `GameEvent` captures "what happened in the story"
/// while `WorldEvent` captures "what state mutation to replay".
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum GameEvent {
    /// A directional nonverbal reaction was added to an NPC's durable history.
    ReactionRecorded {
        /// NPC whose reaction history changed.
        npc_id: NpcId,
        /// Whether the player or NPC performed the reaction.
        direction: ReactionDirection,
        /// Canonical reaction emoji.
        emoji: String,
        /// Dialogue snippet that triggered the reaction.
        context: String,
        /// When the reaction happened.
        timestamp: DateTime<Utc>,
    },
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
    /// The player addressed a named NPC who is not at their current
    /// location.
    ///
    /// Fired by the engine's input router when the absent-aware target
    /// resolver returns a non-empty `absent` list. Logging consumers
    /// (`character_log`, `location_log`) render a journal entry so a
    /// post-session scan captures missed introductions — the existing
    /// system text-log message is UI-only (#1135).
    AddressedAbsentNpc {
        /// The literal name (or alias) the player used.
        name: String,
        /// Where the player was when they made the attempt.
        location: LocationId,
        /// When the attempt happened.
        timestamp: DateTime<Utc>,
    },
    /// Gossip propagated from a Tier-2 group dialogue.
    ///
    /// Fired when `create_gossip_from_tier2_event` would add a new
    /// rumor to `world.gossip_network` — either because a
    /// relationship delta exceeded the threshold or because the
    /// dialogue summary was substantive (> 30 chars). Carries the
    /// originating NPC, the gossip text, and the location where the
    /// conversation took place so subscribers (logs, UI hints) can
    /// render the gossip-spreading moment.
    GossipSpread {
        /// NPC who originated the gossip (the first participant of
        /// the Tier-2 event).
        source: NpcId,
        /// Where the originating conversation happened.
        location: LocationId,
        /// The dialogue summary that became gossip.
        content: String,
        /// When the gossip was minted.
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
        /// Where the festival is centred, if location-specific. `None` for parish-wide festivals.
        #[serde(default)]
        location: Option<LocationId>,
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
    /// An NPC assigned concrete work to the player (#1781).
    ///
    /// `task` is the authoritative record after insertion into the canonical
    /// player-progress ledger. Model output never supplies its id, assigner,
    /// location, timestamps, or lifecycle state.
    PlayerTaskAssigned {
        /// Canonical task record at assignment time.
        task: PlayerTask,
        /// When the assignment was accepted by the engine.
        timestamp: DateTime<Utc>,
    },
    /// A player action advanced an assigned task to in-progress.
    ///
    /// Starting work never implies completion. `task` is the authoritative
    /// post-mutation record and `previous_status` records the transition source.
    PlayerTaskProgressed {
        /// Canonical task record after the action was applied.
        task: PlayerTask,
        /// Lifecycle state before this action.
        previous_status: TaskStatus,
        /// Bounded player action accepted as relevant to the task.
        action: String,
        /// When the action was accepted by the engine.
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
            GameEvent::ReactionRecorded { timestamp, .. }
            | GameEvent::DialogueOccurred { timestamp, .. }
            | GameEvent::MoodChanged { timestamp, .. }
            | GameEvent::RelationshipChanged { timestamp, .. }
            | GameEvent::NpcArrived { timestamp, .. }
            | GameEvent::NpcDeparted { timestamp, .. }
            | GameEvent::NpcActivity { timestamp, .. }
            | GameEvent::GossipSpread { timestamp, .. }
            | GameEvent::AddressedAbsentNpc { timestamp, .. }
            | GameEvent::WeatherChanged { timestamp, .. }
            | GameEvent::FestivalStarted { timestamp, .. }
            | GameEvent::PlayerMoved { timestamp, .. }
            | GameEvent::PlayerTaskAssigned { timestamp, .. }
            | GameEvent::PlayerTaskProgressed { timestamp, .. }
            | GameEvent::LifeEvent { timestamp, .. }
            | GameEvent::NpcInteraction { timestamp, .. } => *timestamp,
        }
    }

    /// Returns the discriminant name for logging/debugging.
    pub fn event_type(&self) -> &str {
        match self {
            GameEvent::ReactionRecorded { .. } => "ReactionRecorded",
            GameEvent::DialogueOccurred { .. } => "DialogueOccurred",
            GameEvent::MoodChanged { .. } => "MoodChanged",
            GameEvent::RelationshipChanged { .. } => "RelationshipChanged",
            GameEvent::NpcArrived { .. } => "NpcArrived",
            GameEvent::NpcDeparted { .. } => "NpcDeparted",
            GameEvent::NpcActivity { .. } => "NpcActivity",
            GameEvent::GossipSpread { .. } => "GossipSpread",
            GameEvent::AddressedAbsentNpc { .. } => "AddressedAbsentNpc",
            GameEvent::WeatherChanged { .. } => "WeatherChanged",
            GameEvent::FestivalStarted { .. } => "FestivalStarted",
            GameEvent::PlayerMoved { .. } => "PlayerMoved",
            GameEvent::PlayerTaskAssigned { .. } => "PlayerTaskAssigned",
            GameEvent::PlayerTaskProgressed { .. } => "PlayerTaskProgressed",
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
#[derive(Clone)]
pub struct EventBus {
    /// The sending half of the broadcast channel.
    tx: broadcast::Sender<GameEvent>,
    /// Parallel channel used by context-aware runtime fan-in.
    context_tx: broadcast::Sender<ContextEventEnvelope>,
    /// Process-monotonic context generation shared by all bus clones.
    context_epoch: Arc<AtomicU64>,
}

impl EventBus {
    /// Creates a new event bus with the default channel capacity.
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(BUS_CAPACITY);
        let (context_tx, _) = broadcast::channel(BUS_CAPACITY);
        Self {
            tx,
            context_tx,
            context_epoch: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Publishes an event to all current subscribers.
    ///
    /// Returns the number of receivers that received the event.
    /// Returns 0 if there are no active subscribers (which is fine —
    /// events are fire-and-forget).
    pub fn publish(&self, event: GameEvent) -> usize {
        tracing::trace!(event_type = event.event_type(), "Publishing game event");
        let context_epoch = self.context_epoch();
        let context_receivers = self
            .context_tx
            .send(ContextEventEnvelope {
                context_epoch,
                event: event.clone(),
            })
            .unwrap_or(0);
        self.tx.send(event).unwrap_or(0).max(context_receivers)
    }

    /// Creates a new subscription to the event bus.
    ///
    /// The returned receiver will see all events published after
    /// this call. If the receiver falls behind by more than
    /// [`BUS_CAPACITY`] events, it will skip the oldest ones.
    pub fn subscribe(&self) -> broadcast::Receiver<GameEvent> {
        self.tx.subscribe()
    }

    /// Creates a publish-time context-stamped subscription.
    pub fn subscribe_contextual(&self) -> broadcast::Receiver<ContextEventEnvelope> {
        self.context_tx.subscribe()
    }

    /// Returns the active process-local context epoch.
    pub fn context_epoch(&self) -> u64 {
        self.context_epoch.load(Ordering::Acquire)
    }

    /// Advances the context epoch after a durable context replacement commits.
    ///
    /// The value is capped at JavaScript's maximum safe integer because it is
    /// included in reconnect IPC responses. Reaching that cap would require
    /// more than nine quadrillion context switches in one process.
    pub fn advance_context_epoch(&self) -> u64 {
        const MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
        self.context_epoch
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                Some(current.saturating_add(1).min(MAX_SAFE_INTEGER))
            })
            .unwrap_or_else(|current| current)
            .saturating_add(1)
            .min(MAX_SAFE_INTEGER)
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
    fn contextual_subscription_preserves_publish_time_epoch() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe_contextual();
        let old = GameEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
            timestamp: test_timestamp(),
        };
        bus.publish(old.clone());
        assert_eq!(bus.advance_context_epoch(), 1);
        let new = GameEvent::WeatherChanged {
            new_weather: "Clear".to_string(),
            timestamp: test_timestamp(),
        };
        bus.publish(new.clone());

        assert_eq!(
            rx.try_recv().unwrap(),
            ContextEventEnvelope {
                context_epoch: 0,
                event: old,
            }
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            ContextEventEnvelope {
                context_epoch: 1,
                event: new,
            }
        );
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
    fn player_task_progress_event_carries_authoritative_post_mutation_record() {
        let ts = test_timestamp();
        let task = PlayerTask {
            id: crate::PlayerTaskId(4),
            description: "Dig over the potato patch.".to_string(),
            assigned_by: NpcId(7),
            location: LocationId(9),
            assigned_at: ts,
            status: TaskStatus::InProgress,
            started_at: Some(ts),
            completed_at: None,
            last_matching_action: Some("I dig over the potato patch.".to_string()),
        };
        let event = GameEvent::PlayerTaskProgressed {
            task: task.clone(),
            previous_status: TaskStatus::Assigned,
            action: "I dig over the potato patch.".to_string(),
            timestamp: ts,
        };

        assert_eq!(event.event_type(), "PlayerTaskProgressed");
        assert_eq!(event.timestamp(), ts);
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["task"]["id"], 4);
        assert_eq!(json["task"]["assigned_by"], 7);
        assert_eq!(json["task"]["location"], 9);
        assert_eq!(json["task"]["status"], "in_progress");
        assert_eq!(json["previous_status"], "assigned");
        assert_eq!(json["action"], "I dig over the potato patch.");
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
                location: None,
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
