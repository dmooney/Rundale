//! Canonical compact turn projections shared by the desktop MCP bridge and
//! the Axum server.
//!
//! Player/NPC exchanges are projected from [`ConversationExchange`] records in
//! [`WorldState::conversation_log`]. The UI transcript is intentionally not an
//! input: it contains presentation-only player lines and speaker labels, and
//! cannot faithfully reconstruct which player input belongs to an older NPC
//! reply.

use std::collections::VecDeque;

use chrono::Timelike;
use serde::{Deserialize, Serialize};

use crate::npc::manager::NpcManager;
use crate::world::WorldState;
use crate::world::events::GameEvent;
use parish_types::{ConversationCursor, ConversationExchange};

/// Maximum conversation exchanges returned by `GET /api/turn`.
pub const TURN_MAX_EXCHANGES: usize = 10;
/// Maximum world events returned by `GET /api/turn` per call.
pub const TURN_MAX_EVENTS: usize = 20;

/// Request body accepted by `POST /api/submit-input`.
///
/// Browser clients use `addressedTo`; the MCP tool historically sends
/// `addressed_to`, so both spellings are accepted.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitInputRequest {
    /// Player input text.
    pub text: String,
    /// Real names of explicitly addressed NPCs, in chip-first order.
    #[serde(default, alias = "addressed_to")]
    pub addressed_to: Vec<String>,
}

/// Optional cursor query accepted by `GET /api/turn`.
#[derive(Debug, Default, Deserialize)]
pub struct TurnReadParams {
    /// Monotonic world-event cursor returned by the preceding call.
    pub since: Option<usize>,
}

/// A canonical player/NPC exchange returned by the compact turn endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnExchange {
    /// What the player said or did in this exchange.
    pub player_input: String,
    /// What the NPC replied.
    pub npc_dialogue: String,
    /// Display name of the NPC who replied.
    pub speaker_name: String,
    /// Location name where this exchange happened.
    pub location: String,
}

/// Compact game-clock snapshot included in turn responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnClock {
    /// Current game hour (0–23).
    pub hour: u8,
    /// Current game minute (0–59).
    pub minute: u8,
    /// Human-readable time label (for example, `"Morning"`).
    pub time_label: String,
}

/// Compact response returned by `POST /api/submit-input`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitInputResult {
    /// Canonical conversation exchanges added by this turn.
    pub exchanges: Vec<TurnExchange>,
    /// Clock state after the turn completes.
    pub clock: TurnClock,
    /// Player location name after the turn.
    pub location: String,
    /// Number of NPCs at the player's location after the turn.
    pub npcs_here: usize,
    /// Present when the submitted dialogue produced no canonical NPC exchange.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A summarised world event returned by `GET /api/turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnEvent {
    /// Event discriminant (for example, `"NpcArrived"`).
    pub kind: String,
    /// Human-readable event summary.
    pub summary: String,
}

/// Compact response returned by `GET /api/turn`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnReadResult {
    /// Recent canonical exchanges, newest last.
    pub exchanges: Vec<TurnExchange>,
    /// Recent world events after the requested event cursor.
    pub events: Vec<TurnEvent>,
    /// Current clock state.
    pub clock: TurnClock,
    /// Current player location name.
    pub location: String,
    /// Number of NPCs at the player's location.
    pub npcs_here: usize,
    /// Monotonic cursor to pass as `?since=` on the next call.
    pub event_cursor: usize,
}

/// Captures the canonical conversation cursor before dispatching a turn.
pub fn conversation_cursor(world: &WorldState) -> ConversationCursor {
    world.conversation_log.cursor()
}

/// Builds the compact response for a completed input submission.
///
/// Only canonical exchanges added after `before_turn` are included. This
/// excludes presentation-only player transcript lines and remains correct
/// when the conversation ring is already at capacity.
pub fn build_submit_input_result(
    world: &WorldState,
    npc_manager: &NpcManager,
    before_turn: ConversationCursor,
) -> SubmitInputResult {
    let exchanges = world
        .conversation_log
        .exchanges_since(before_turn)
        .into_iter()
        .map(|exchange| project_exchange(world, exchange))
        .collect();
    let (clock, location, npcs_here) = project_current_state(world, npc_manager);

    SubmitInputResult {
        exchanges,
        clock,
        location,
        npcs_here,
        error: None,
    }
}

/// Builds a compact turn read from canonical exchanges and already-projected
/// event state.
pub fn build_turn_read_result(
    world: &WorldState,
    npc_manager: &NpcManager,
    events: Vec<TurnEvent>,
    event_cursor: usize,
) -> TurnReadResult {
    let exchanges = recent_exchanges(world, TURN_MAX_EXCHANGES);
    let (clock, location, npcs_here) = project_current_state(world, npc_manager);

    TurnReadResult {
        exchanges,
        events,
        clock,
        location,
        npcs_here,
        event_cursor,
    }
}

/// Projects recent canonical exchanges, oldest first and newest last.
pub fn recent_exchanges(world: &WorldState, limit: usize) -> Vec<TurnExchange> {
    let mut exchanges: Vec<TurnExchange> = world
        .conversation_log
        .all()
        .rev()
        .take(limit)
        .map(|exchange| project_exchange(world, exchange))
        .collect();
    exchanges.reverse();
    exchanges
}

/// Projects the newest retained world events after `since_cursor`.
///
/// `total_events` is the monotonic lifetime count maintained by the runtime.
/// It is floored at the retained length so test fixtures and legacy states
/// without a populated counter still produce a valid cursor. The returned
/// cursor is always that coherent lifetime total: this is a bounded latest
/// window, not a pageable log. When more than [`TURN_MAX_EVENTS`] unseen
/// events are retained, the oldest excess entries are omitted.
pub fn events_since(
    events: &VecDeque<GameEvent>,
    total_events: usize,
    since_cursor: usize,
) -> (Vec<TurnEvent>, usize) {
    let total = total_events.max(events.len());
    let evicted = total.saturating_sub(events.len());
    let first_unseen = if since_cursor > total {
        // A cursor from a different/restarted runtime cannot name an event in
        // this lifetime. Resynchronise from the retained window.
        evicted
    } else {
        since_cursor.max(evicted)
    };
    let unseen_offset = first_unseen.saturating_sub(evicted);
    let unseen_count = events.len().saturating_sub(unseen_offset);
    let skip = unseen_offset.saturating_add(unseen_count.saturating_sub(TURN_MAX_EVENTS));
    let projected: Vec<_> = events
        .iter()
        .skip(skip)
        .take(TURN_MAX_EVENTS)
        .map(project_event)
        .collect();
    (projected, total)
}

fn project_current_state(
    world: &WorldState,
    npc_manager: &NpcManager,
) -> (TurnClock, String, usize) {
    let now = world.clock.now();
    let clock = TurnClock {
        hour: now.hour() as u8,
        minute: now.minute() as u8,
        time_label: world.clock.time_of_day().to_string(),
    };
    let location = world.current_location().name.clone();
    let npcs_here = npc_manager.npcs_at(world.player_location).len();
    (clock, location, npcs_here)
}

fn project_exchange(world: &WorldState, exchange: &ConversationExchange) -> TurnExchange {
    let location = world
        .graph
        .get(exchange.location)
        .map(|data| data.name.clone())
        .or_else(|| {
            world
                .locations
                .get(&exchange.location)
                .map(|location| location.name.clone())
        })
        .unwrap_or_else(|| format!("Location #{}", exchange.location.0));

    TurnExchange {
        player_input: exchange.player_input.clone(),
        npc_dialogue: exchange.npc_dialogue.clone(),
        speaker_name: exchange.speaker_name.clone(),
        location,
    }
}

fn project_event(event: &GameEvent) -> TurnEvent {
    let summary = match event {
        GameEvent::ReactionRecorded {
            npc_id,
            direction,
            emoji,
            ..
        } => format!("NPC #{} reaction {:?}: {emoji}", npc_id.0, direction),
        GameEvent::DialogueOccurred { summary, .. } => summary.clone(),
        GameEvent::MoodChanged {
            npc_id, new_mood, ..
        } => {
            format!("NPC #{} mood → {new_mood}", npc_id.0)
        }
        GameEvent::RelationshipChanged {
            npc_a,
            npc_b,
            delta,
            ..
        } => {
            format!("Relationship #{}/{} Δ{delta:+.2}", npc_a.0, npc_b.0)
        }
        GameEvent::NpcArrived {
            npc_id, location, ..
        } => {
            format!("NPC #{} arrived at loc #{}", npc_id.0, location.0)
        }
        GameEvent::NpcDeparted {
            npc_id,
            location,
            to,
            ..
        } => {
            format!("NPC #{} departed loc #{} → #{}", npc_id.0, location.0, to.0)
        }
        GameEvent::NpcActivity {
            npc_id, activity, ..
        } => {
            format!("NPC #{}: {activity}", npc_id.0)
        }
        GameEvent::GossipSpread { content, .. } => {
            format!("Gossip: {content}")
        }
        GameEvent::AddressedAbsentNpc { name, .. } => {
            format!("{name} not present")
        }
        GameEvent::WeatherChanged { new_weather, .. } => {
            format!("Weather → {new_weather}")
        }
        GameEvent::FestivalStarted { name, .. } => {
            format!("Festival: {name}")
        }
        GameEvent::PlayerMoved { from, to, .. } => {
            format!("Player moved loc #{} → #{}", from.0, to.0)
        }
        GameEvent::PlayerTaskAssigned { task, .. } => {
            format!(
                "Task #{} assigned: {} (loc #{})",
                task.id.0, task.description, task.location.0
            )
        }
        GameEvent::PlayerTaskProgressed { task, .. } => {
            format!(
                "Task #{} → {:?}: {}",
                task.id.0, task.status, task.description
            )
        }
        GameEvent::LifeEvent {
            npc_id,
            description,
            ..
        } => {
            format!("NPC #{}: {description}", npc_id.0)
        }
        GameEvent::NpcInteraction { summary, .. } => summary.clone(),
    };

    TurnEvent {
        kind: event.event_type().to_string(),
        summary,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use parish_types::{ConversationExchange, Location, LocationId, NpcId};

    use super::*;

    fn exchange(
        minute: u32,
        player_input: &str,
        npc_dialogue: &str,
        speaker: &str,
        location: LocationId,
    ) -> ConversationExchange {
        ConversationExchange {
            timestamp: Utc.with_ymd_and_hms(1820, 3, 20, 8, minute, 0).unwrap(),
            speaker_id: NpcId(minute + 1),
            speaker_name: speaker.to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: npc_dialogue.to_string(),
            location,
        }
    }

    #[test]
    fn recent_turn_projection_preserves_each_historical_player_input() {
        let mut world = WorldState::new();
        world.conversation_log.add(exchange(
            0,
            "first question",
            "first answer",
            "Peig",
            LocationId(1),
        ));
        world.conversation_log.add(exchange(
            1,
            "second question",
            "second answer",
            "Sean",
            LocationId(1),
        ));

        let result = build_turn_read_result(&world, &NpcManager::new(), Vec::new(), 0);

        assert_eq!(result.exchanges.len(), 2);
        assert_eq!(result.exchanges[0].player_input, "first question");
        assert_eq!(result.exchanges[0].npc_dialogue, "first answer");
        assert_eq!(result.exchanges[1].player_input, "second question");
        assert_eq!(result.exchanges[1].npc_dialogue, "second answer");
    }

    #[test]
    fn submit_projection_returns_only_canonical_exchanges_added_after_cursor() {
        let mut world = WorldState::new();
        world.conversation_log.add(exchange(
            0,
            "old question",
            "old answer",
            "Peig",
            LocationId(1),
        ));
        let before = conversation_cursor(&world);
        world.conversation_log.add(exchange(
            1,
            "new question",
            "new answer",
            "Sean",
            LocationId(1),
        ));

        let result = build_submit_input_result(&world, &NpcManager::new(), before);

        assert_eq!(result.exchanges.len(), 1);
        assert_eq!(result.exchanges[0].speaker_name, "Sean");
        assert_eq!(result.exchanges[0].player_input, "new question");
        assert_eq!(result.exchanges[0].npc_dialogue, "new answer");
        assert!(result.error.is_none());
    }

    #[test]
    fn submit_projection_can_distinguish_failed_dialogue_from_a_non_dialogue_turn() {
        let world = WorldState::new();
        let before = conversation_cursor(&world);
        let mut result = build_submit_input_result(&world, &NpcManager::new(), before);
        result.error = Some("That reply failed. Please try again.".to_string());

        let json = serde_json::to_value(result).unwrap();
        assert_eq!(json["exchanges"], serde_json::json!([]));
        assert_eq!(
            json["error"],
            serde_json::json!("That reply failed. Please try again.")
        );
    }

    #[test]
    fn exchange_location_comes_from_exchange_not_current_player_location() {
        let mut world = WorldState::new();
        world.locations.insert(
            LocationId(2),
            Location {
                id: LocationId(2),
                name: "Murphy's Farm".to_string(),
                description: String::new(),
                indoor: false,
                public: true,
                lat: 0.0,
                lon: 0.0,
            },
        );
        world.conversation_log.add(exchange(
            0,
            "Where do I begin?",
            "Start with the potatoes.",
            "Siobhan",
            LocationId(2),
        ));

        let projected = recent_exchanges(&world, 10);

        assert_eq!(world.current_location().name, "The Crossroads");
        assert_eq!(projected[0].location, "Murphy's Farm");
    }

    #[test]
    fn event_projection_uses_monotonic_cursor_after_ring_eviction() {
        let mut events = VecDeque::new();
        events.push_back(GameEvent::WeatherChanged {
            new_weather: "Mist".to_string(),
            timestamp: Utc::now(),
        });
        events.push_back(GameEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
            timestamp: Utc::now(),
        });

        // Five lifetime events means the retained pair have indices 3 and 4.
        let (projected, cursor) = events_since(&events, 5, 4);

        assert_eq!(cursor, 5);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].summary, "Weather → Rain");
    }

    fn weather_events(range: std::ops::Range<usize>) -> VecDeque<GameEvent> {
        range
            .map(|index| GameEvent::WeatherChanged {
                new_weather: format!("Weather {index}"),
                timestamp: Utc::now(),
            })
            .collect()
    }

    fn weather_number(event: &TurnEvent) -> usize {
        event
            .summary
            .strip_prefix("Weather → Weather ")
            .expect("weather event summary")
            .parse()
            .expect("numeric weather test id")
    }

    #[test]
    fn event_projection_returns_newest_bounded_window_and_lifetime_cursor() {
        for (total, expected_first) in [(19, 0), (20, 0), (21, 1), (27, 7)] {
            let events = weather_events(0..total);
            let (projected, cursor) = events_since(&events, total, 0);

            assert_eq!(cursor, total, "total={total}");
            assert_eq!(projected.len(), total.min(TURN_MAX_EVENTS), "total={total}");
            assert_eq!(
                weather_number(&projected[0]),
                expected_first,
                "total={total}"
            );
            assert_eq!(
                weather_number(projected.last().expect("non-empty projection")),
                total - 1,
                "total={total}"
            );
            assert_eq!(
                projected.iter().map(weather_number).collect::<Vec<_>>(),
                (expected_first..total).collect::<Vec<_>>(),
                "events stay chronological for total={total}"
            );
        }
    }

    #[test]
    fn event_projection_bounds_in_range_cursor_from_the_newest_end() {
        let events = weather_events(0..27);

        let (more_than_cap, cursor) = events_since(&events, 27, 3);
        assert_eq!(cursor, 27);
        assert_eq!(weather_number(&more_than_cap[0]), 7);
        assert_eq!(weather_number(more_than_cap.last().unwrap()), 26);

        let (within_cap, cursor) = events_since(&events, 27, 10);
        assert_eq!(cursor, 27);
        assert_eq!(within_cap.len(), 17);
        assert_eq!(weather_number(&within_cap[0]), 10);
        assert_eq!(weather_number(within_cap.last().unwrap()), 26);
    }

    #[test]
    fn stale_cursor_after_ring_eviction_returns_newest_retained_events() {
        // A capacity-100 ring retaining lifetime positions 5..104.
        let events = weather_events(5..105);
        let (projected, cursor) = events_since(&events, 105, 0);

        assert_eq!(cursor, 105);
        assert_eq!(projected.len(), TURN_MAX_EVENTS);
        assert_eq!(weather_number(&projected[0]), 85);
        assert_eq!(weather_number(projected.last().unwrap()), 104);
    }

    #[test]
    fn current_cursor_is_empty_and_future_cursor_resynchronises_to_newest() {
        let events = weather_events(0..27);

        let (current, current_cursor) = events_since(&events, 27, 27);
        assert!(current.is_empty());
        assert_eq!(current_cursor, 27);

        let (future, future_cursor) = events_since(&events, 27, 99);
        assert_eq!(future_cursor, 27);
        assert_eq!(future.len(), TURN_MAX_EVENTS);
        assert_eq!(weather_number(&future[0]), 7);
        assert_eq!(weather_number(future.last().unwrap()), 26);
    }

    #[test]
    fn old_context_cursor_catches_first_event_after_ring_clear() {
        let old_context_cursor = 5;
        let mut new_context_events = VecDeque::new();
        new_context_events.push_back(GameEvent::WeatherChanged {
            new_weather: "Clear".to_string(),
            timestamp: Utc::now(),
        });

        // The lifetime total is deliberately not reset when the old ring is
        // cleared. The first new-context event therefore has offset 5 and is
        // visible to a client holding the old context's terminal cursor.
        let (projected, cursor) = events_since(
            &new_context_events,
            old_context_cursor + 1,
            old_context_cursor,
        );

        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].summary, "Weather → Clear");
        assert_eq!(cursor, old_context_cursor + 1);
    }
}
