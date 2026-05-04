//! Event-bus abstraction for server-push events.
//!
//! This module defines the [`EventBus`] trait and [`Topic`] enum that let the
//! web server (and future runtimes such as Redis/NATS) push typed events to
//! connected clients without coupling the emission sites to a concrete
//! broadcast transport.
//!
//! # Design
//!
//! - [`Topic`] classifies every event so subscribers can request a filtered
//!   view.  The wire format is unchanged: clients still receive
//!   [`ServerEvent`] frames whose `event` field is the original string name.
//! - [`BroadcastEventBus`] is the default implementation, backed by a
//!   `tokio::sync::broadcast` channel.  Subscribers without a topic filter
//!   receive the firehose (current behavior); subscribers with a filter
//!   receive only matching events.
//! - All three runtimes (Axum web, Tauri desktop, headless CLI) may depend on
//!   this module because `parish-core` is backend-agnostic (no axum/tauri
//!   deps here).

use tokio::sync::broadcast;

/// Wire-format event pushed to WebSocket clients.
///
/// The `event` field carries the string name the frontend dispatches on
/// (e.g. `"text-log"`, `"world-update"`).  Topic is a server-side routing
/// concept only — it never appears on the wire.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ServerEvent {
    /// Event name (e.g. `"stream-token"`, `"text-log"`).
    pub event: String,
    /// JSON payload for this event.
    pub payload: serde_json::Value,
}

/// Topic classifies every [`ServerEvent`] for topic-aware subscriptions.
///
/// Each variant maps to one or more `event` string names:
///
/// | Variant          | Wire event name(s)                        |
/// |------------------|-------------------------------------------|
/// | `TextLog`        | `text-log`                                |
/// | `WorldUpdate`    | `world-update`                            |
/// | `InferenceToken` | `stream-token`, `stream-end`, `stream-turn-end` |
/// | `TravelStart`    | `travel-start`                            |
/// | `Loading`        | `loading`                                 |
/// | `NpcReaction`    | `npc-reaction`                            |
/// | `ClockTick`      | (reserved — no current emitter)           |
/// | `UiControl`      | `toggle-full-map`, `open-designer`, `save-picker`, `theme-switch`, `tiles-switch` |
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Topic {
    /// Player-visible dialogue and system messages (`"text-log"`).
    TextLog,
    /// World snapshot after any state change (`"world-update"`).
    WorldUpdate,
    /// LLM streaming tokens and stream lifecycle events
    /// (`"stream-token"`, `"stream-end"`, `"stream-turn-end"`).
    InferenceToken,
    /// Travel animation kickoff (`"travel-start"`).
    TravelStart,
    /// Loading/spinner animation frames (`"loading"`).
    Loading,
    /// NPC emoji reactions to player messages (`"npc-reaction"`).
    NpcReaction,
    /// Game-clock tick events (reserved for future use).
    ClockTick,
    /// UI state controls: map toggle, designer, save picker, theme/tile switches
    /// (`"toggle-full-map"`, `"open-designer"`, `"save-picker"`,
    /// `"theme-switch"`, `"tiles-switch"`).
    UiControl,
}

impl Topic {
    /// Returns the topic for a given wire event name, or `None` if unknown.
    pub fn from_event_name(name: &str) -> Option<Self> {
        match name {
            "text-log" => Some(Self::TextLog),
            "world-update" => Some(Self::WorldUpdate),
            "stream-token" | "stream-end" | "stream-turn-end" => Some(Self::InferenceToken),
            "travel-start" => Some(Self::TravelStart),
            "loading" => Some(Self::Loading),
            "npc-reaction" => Some(Self::NpcReaction),
            "toggle-full-map" | "open-designer" | "save-picker" | "theme-switch"
            | "tiles-switch" => Some(Self::UiControl),
            _ => None,
        }
    }
}

/// A stream of tagged events delivered to a subscriber.
///
/// Returned by [`EventBus::subscribe`].  Call [`EventStream::recv`] to
/// receive the next event, skipping any that don't match the topic filter.
pub struct EventStream {
    rx: broadcast::Receiver<(Topic, ServerEvent)>,
    /// Topics this stream is interested in.  Empty = firehose (all topics).
    filter: Vec<Topic>,
}

impl EventStream {
    fn new(rx: broadcast::Receiver<(Topic, ServerEvent)>, filter: Vec<Topic>) -> Self {
        Self { rx, filter }
    }

    /// Returns `true` if the given topic passes this stream's filter.
    fn matches(&self, topic: Topic) -> bool {
        self.filter.is_empty() || self.filter.contains(&topic)
    }

    /// Receives the next matching event.
    ///
    /// Skips lagged or out-of-filter events. Returns `Ok(event)` on success,
    /// `Err` if the channel is closed.
    pub async fn recv(&mut self) -> Result<ServerEvent, RecvError> {
        loop {
            match self.rx.recv().await {
                Ok((topic, event)) => {
                    if self.matches(topic) {
                        return Ok(event);
                    }
                    // Not in our filter — skip without surfacing to caller.
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(dropped = n, "EventStream: lagged, dropped events");
                    // Continue receiving; the caller doesn't need to handle lag.
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return Err(RecvError::Closed);
                }
            }
        }
    }

    /// Non-blocking receive — returns `None` if no matching event is ready.
    pub fn try_recv(&mut self) -> Option<ServerEvent> {
        loop {
            match self.rx.try_recv() {
                Ok((topic, event)) => {
                    if self.matches(topic) {
                        return Some(event);
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    tracing::warn!(dropped = n, "EventStream: try_recv lagged");
                }
                Err(_) => return None,
            }
        }
    }
}

/// Error returned by [`EventStream::recv`].
#[derive(Debug, PartialEq, Eq)]
pub enum RecvError {
    /// The channel has been closed; no more events will arrive.
    Closed,
}

/// Abstraction over the event-bus transport.
///
/// Implement this trait to swap in Redis, NATS, or another transport
/// without touching any emission call sites.
///
/// # Thread safety
///
/// The trait requires `Send + Sync` so implementations can live behind
/// an `Arc<dyn EventBus>` shared across Tokio tasks.
pub trait EventBus: Send + Sync {
    /// Emits `event` on the given `topic`.
    fn emit(&self, topic: Topic, event: ServerEvent);

    /// Emits a named event with a serializable payload on the given `topic`.
    ///
    /// This is the primary ergonomic entry point used by call sites.
    fn emit_named<T: serde::Serialize>(&self, topic: Topic, event_name: &str, payload: &T) {
        match serde_json::to_value(payload) {
            Ok(value) => self.emit(
                topic,
                ServerEvent {
                    event: event_name.to_string(),
                    payload: value,
                },
            ),
            Err(e) => {
                tracing::warn!(
                    event = %event_name,
                    error = %e,
                    "EventBus: failed to serialize event payload"
                );
            }
        }
    }

    /// Creates a new [`EventStream`] that delivers events matching `topics`.
    ///
    /// Pass an empty slice to receive all events (firehose).
    fn subscribe(&self, topics: &[Topic]) -> EventStream;
}

/// Default [`EventBus`] implementation backed by a `tokio::sync::broadcast`
/// channel.
///
/// Wraps the current broadcast transport and applies topic filtering at
/// subscribe-time.  Existing callers that pass `&[]` (empty filter) receive
/// the full event stream with no behavior change.
pub struct BroadcastEventBus {
    tx: broadcast::Sender<(Topic, ServerEvent)>,
}

impl BroadcastEventBus {
    /// Creates a new bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Returns the number of active receivers (0 if none are connected).
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

impl EventBus for BroadcastEventBus {
    fn emit(&self, topic: Topic, event: ServerEvent) {
        if self.tx.send((topic, event)).is_err() {
            tracing::warn!("EventBus: broadcast failed — no active subscribers");
        }
    }

    fn subscribe(&self, topics: &[Topic]) -> EventStream {
        EventStream::new(self.tx.subscribe(), topics.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(name: &str) -> ServerEvent {
        ServerEvent {
            event: name.to_string(),
            payload: serde_json::Value::Null,
        }
    }

    #[tokio::test]
    async fn firehose_receives_all_topics() {
        let bus = BroadcastEventBus::new(16);
        let mut stream = bus.subscribe(&[]);
        bus.emit(Topic::TextLog, make_event("text-log"));
        bus.emit(Topic::WorldUpdate, make_event("world-update"));
        let e1 = stream.recv().await.unwrap();
        let e2 = stream.recv().await.unwrap();
        assert_eq!(e1.event, "text-log");
        assert_eq!(e2.event, "world-update");
    }

    #[tokio::test]
    async fn filtered_stream_skips_non_matching() {
        let bus = BroadcastEventBus::new(16);
        let mut stream = bus.subscribe(&[Topic::WorldUpdate]);
        // Emit TextLog first — should be filtered out.
        bus.emit(Topic::TextLog, make_event("text-log"));
        bus.emit(Topic::WorldUpdate, make_event("world-update"));

        // Only world-update should come through.
        let ev = stream.recv().await.unwrap();
        assert_eq!(ev.event, "world-update");
    }

    #[test]
    fn emit_named_serializes_payload() {
        let bus = BroadcastEventBus::new(16);
        let mut stream = bus.subscribe(&[]);
        bus.emit_named(
            Topic::TextLog,
            "text-log",
            &serde_json::json!({"key": "value"}),
        );
        let ev = stream.try_recv().unwrap();
        assert_eq!(ev.event, "text-log");
        assert_eq!(ev.payload["key"], "value");
    }

    #[test]
    fn no_subscribers_does_not_panic() {
        let bus = BroadcastEventBus::new(16);
        // Should not panic when no subscribers are present.
        bus.emit(
            Topic::TextLog,
            ServerEvent {
                event: "orphan".to_string(),
                payload: serde_json::Value::Null,
            },
        );
    }

    #[test]
    fn topic_from_event_name_covers_all_wire_names() {
        let known = [
            "text-log",
            "world-update",
            "stream-token",
            "stream-end",
            "stream-turn-end",
            "travel-start",
            "loading",
            "npc-reaction",
            "toggle-full-map",
            "open-designer",
            "save-picker",
            "theme-switch",
            "tiles-switch",
        ];
        for name in known {
            assert!(
                Topic::from_event_name(name).is_some(),
                "Topic::from_event_name(\"{name}\") returned None — add a mapping"
            );
        }
    }
}
