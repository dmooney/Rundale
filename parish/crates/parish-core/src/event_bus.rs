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

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
/// | `InferenceToken` | `stream-token`, `stream-end`, `stream-turn-end`, `dialogue-corrected`, `dialogue-quality` |
/// | `TravelStart`    | `travel-start`                            |
/// | `Loading`        | `loading`                                 |
/// | `NpcReaction`    | `npc-reaction`                            |
/// | `ClockTick`      | (reserved — no current emitter)           |
/// | `UiControl`      | `toggle-full-map`, `open-designer`, `save-picker`, `theme-switch`, `tiles-switch`, `game-context-reset` |
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
    /// UI state controls: map toggle, designer, save picker, theme/tile switches,
    /// and canonical game-context resets
    /// (`"toggle-full-map"`, `"open-designer"`, `"save-picker"`,
    /// `"theme-switch"`, `"tiles-switch"`, `"game-context-reset"`).
    UiControl,
}

impl Topic {
    /// Returns the topic for a given wire event name, or `None` if unknown.
    pub fn from_event_name(name: &str) -> Option<Self> {
        match name {
            "text-log" => Some(Self::TextLog),
            "world-update" => Some(Self::WorldUpdate),
            "stream-token" | "stream-end" | "stream-turn-end" | "dialogue-corrected"
            | "dialogue-quality" => Some(Self::InferenceToken),
            "travel-start" => Some(Self::TravelStart),
            "loading" => Some(Self::Loading),
            "npc-reaction" => Some(Self::NpcReaction),
            "toggle-full-map" | "open-designer" | "save-picker" | "theme-switch"
            | "tiles-switch" | "game-context-reset" => Some(Self::UiControl),
            _ => None,
        }
    }
}

/// Per-subscriber FIFO queue used by [`DeterministicEventBus`].
///
/// Each deterministic subscriber owns one of these behind an `Arc<Mutex<..>>`;
/// the bus pushes matching events onto the back at emit time, the stream pops
/// them off the front. No tokio runtime and no `broadcast` channel are
/// involved, so the order a subscriber observes is exactly publish order.
type DeterministicQueue = Arc<Mutex<VecDeque<ServerEvent>>>;

/// Transport backing an [`EventStream`].
///
/// Production code always gets [`StreamBackend::Broadcast`] (the
/// `tokio::sync::broadcast` receiver). The test-only
/// [`DeterministicEventBus`] hands out [`StreamBackend::Deterministic`]
/// streams whose ordering is fixed and reproducible.
enum StreamBackend {
    /// Async broadcast receiver — the production transport.
    Broadcast(broadcast::Receiver<(Topic, ServerEvent)>),
    /// Synchronous, in-order queue — the deterministic test transport.
    /// Filtering is applied at emit time, so the queue holds only events
    /// already matching this stream's filter.
    Deterministic(DeterministicQueue),
}

/// A stream of tagged events delivered to a subscriber.
///
/// Returned by [`EventBus::subscribe`].  Call [`EventStream::recv`] to
/// receive the next event, skipping any that don't match the topic filter.
pub struct EventStream {
    backend: StreamBackend,
    /// Topics this stream is interested in.  Empty = firehose (all topics).
    ///
    /// For the broadcast backend, filtering is applied on receive (the
    /// firehose carries every topic). For the deterministic backend, filtering
    /// is applied at emit time, so this field is informational there.
    filter: Vec<Topic>,
}

impl EventStream {
    fn new(rx: broadcast::Receiver<(Topic, ServerEvent)>, filter: Vec<Topic>) -> Self {
        Self {
            backend: StreamBackend::Broadcast(rx),
            filter,
        }
    }

    /// Builds a deterministic stream backed by a pre-filtered FIFO queue.
    fn deterministic(queue: DeterministicQueue, filter: Vec<Topic>) -> Self {
        Self {
            backend: StreamBackend::Deterministic(queue),
            filter,
        }
    }

    /// Receives the next matching event.
    ///
    /// Skips lagged or out-of-filter events. Returns `Ok(event)` on success,
    /// `Err` if the channel is closed.
    ///
    /// For the deterministic backend this resolves immediately from the
    /// in-order queue (no awaiting), returning [`RecvError::Closed`] once the
    /// queue is drained — there is no separate producer task to wait on.
    pub async fn recv(&mut self) -> Result<ServerEvent, RecvError> {
        match &mut self.backend {
            StreamBackend::Broadcast(rx) => loop {
                match rx.recv().await {
                    Ok((topic, event)) => {
                        if self.filter.is_empty() || self.filter.contains(&topic) {
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
            },
            StreamBackend::Deterministic(queue) => queue
                .lock()
                .expect("deterministic event queue mutex poisoned")
                .pop_front()
                .ok_or(RecvError::Closed),
        }
    }

    /// Non-blocking receive — returns `None` if no matching event is ready.
    pub fn try_recv(&mut self) -> Option<ServerEvent> {
        match &mut self.backend {
            StreamBackend::Broadcast(rx) => loop {
                match rx.try_recv() {
                    Ok((topic, event)) => {
                        if self.filter.is_empty() || self.filter.contains(&topic) {
                            return Some(event);
                        }
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        tracing::warn!(dropped = n, "EventStream: try_recv lagged");
                    }
                    Err(_) => return None,
                }
            },
            StreamBackend::Deterministic(queue) => queue
                .lock()
                .expect("deterministic event queue mutex poisoned")
                .pop_front(),
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

/// One deterministic subscriber: its topic filter plus its private FIFO queue.
struct DeterministicSubscriber {
    filter: Vec<Topic>,
    queue: DeterministicQueue,
}

/// Deterministic, synchronous-drain [`EventBus`] for tests (#1176).
///
/// Unlike [`BroadcastEventBus`], this implementation does **not** use a
/// `tokio::sync::broadcast` channel. On [`emit`](EventBus::emit) it
/// **synchronously** appends the event to every matching subscriber's FIFO
/// queue, in the same thread, before returning. A subscriber therefore
/// observes events in exactly the order they were published, with no async
/// scheduling window between publish and delivery — the window that lets
/// ordering-dependent tests flake under the broadcast transport.
///
/// This is a **test-only** type. It is never wired into the production
/// `parish-server` / `parish-tauri` / `parish-engine` push paths, which keep
/// using [`BroadcastEventBus`] so real-time-push semantics (CLAUDE.md rule
/// #11) are unchanged. Tests and the `GameTestHarness` opt in explicitly by
/// constructing a `DeterministicEventBus`.
///
/// Topic filtering is resolved at emit time: each subscriber's queue only ever
/// holds events matching the topics it asked for (empty filter = firehose).
///
/// ```
/// use parish_core::event_bus::{DeterministicEventBus, EventBus, ServerEvent, Topic};
///
/// let bus = DeterministicEventBus::new();
/// let mut a = bus.subscribe(&[]); // firehose
/// let mut b = bus.subscribe(&[Topic::WorldUpdate]); // filtered
///
/// bus.emit_named(Topic::TextLog, "text-log", &serde_json::json!({}));
/// bus.emit_named(Topic::WorldUpdate, "world-update", &serde_json::json!({}));
///
/// // Firehose sees both, in publish order; the filtered stream sees one.
/// assert_eq!(a.try_recv().unwrap().event, "text-log");
/// assert_eq!(a.try_recv().unwrap().event, "world-update");
/// assert_eq!(b.try_recv().unwrap().event, "world-update");
/// assert!(b.try_recv().is_none());
/// ```
#[derive(Default)]
pub struct DeterministicEventBus {
    subscribers: Arc<Mutex<Vec<DeterministicSubscriber>>>,
}

impl DeterministicEventBus {
    /// Creates an empty deterministic bus with no subscribers.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers
            .lock()
            .expect("deterministic subscribers mutex poisoned")
            .len()
    }
}

impl EventBus for DeterministicEventBus {
    fn emit(&self, topic: Topic, event: ServerEvent) {
        let subscribers = self
            .subscribers
            .lock()
            .expect("deterministic subscribers mutex poisoned");
        for sub in subscribers.iter() {
            let matches = sub.filter.is_empty() || sub.filter.contains(&topic);
            if matches {
                sub.queue
                    .lock()
                    .expect("deterministic event queue mutex poisoned")
                    .push_back(event.clone());
            }
        }
    }

    fn subscribe(&self, topics: &[Topic]) -> EventStream {
        let queue: DeterministicQueue = Arc::new(Mutex::new(VecDeque::new()));
        self.subscribers
            .lock()
            .expect("deterministic subscribers mutex poisoned")
            .push(DeterministicSubscriber {
                filter: topics.to_vec(),
                queue: Arc::clone(&queue),
            });
        EventStream::deterministic(queue, topics.to_vec())
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
    fn try_recv_skips_filtered_events_without_blocking() {
        let bus = BroadcastEventBus::new(16);
        let mut stream = bus.subscribe(&[Topic::WorldUpdate]);

        bus.emit(Topic::TextLog, make_event("text-log"));
        assert!(
            stream.try_recv().is_none(),
            "non-matching ready events should be consumed but not returned"
        );

        bus.emit(Topic::TravelStart, make_event("travel-start"));
        bus.emit(Topic::WorldUpdate, make_event("world-update"));

        let ev = stream
            .try_recv()
            .expect("matching event behind filtered events should be returned");
        assert_eq!(ev.event, "world-update");
        assert!(
            stream.try_recv().is_none(),
            "stream should be empty after returning the matching event"
        );
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
            "dialogue-corrected",
            "dialogue-quality",
            "travel-start",
            "loading",
            "npc-reaction",
            "toggle-full-map",
            "open-designer",
            "save-picker",
            "theme-switch",
            "tiles-switch",
            "game-context-reset",
        ];
        for name in known {
            assert!(
                Topic::from_event_name(name).is_some(),
                "Topic::from_event_name(\"{name}\") returned None — add a mapping"
            );
        }
    }

    // --- DeterministicEventBus (#1176) ---------------------------------------

    /// A fixed, deliberately interleaved publish sequence across multiple
    /// topics. The deterministic bus must replay it back to a firehose
    /// subscriber in exactly this order on every run.
    const INTERLEAVED: &[(Topic, &str)] = &[
        (Topic::TextLog, "text-log"),
        (Topic::WorldUpdate, "world-update"),
        (Topic::InferenceToken, "stream-token"),
        (Topic::TextLog, "text-log"),
        (Topic::NpcReaction, "npc-reaction"),
        (Topic::WorldUpdate, "world-update"),
        (Topic::TravelStart, "travel-start"),
        (Topic::InferenceToken, "stream-end"),
        (Topic::TextLog, "text-log"),
    ];

    fn drain_all(stream: &mut EventStream) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(ev) = stream.try_recv() {
            out.push(ev.event);
        }
        out
    }

    #[test]
    fn deterministic_bus_drains_synchronously_in_publish_order() {
        let bus = DeterministicEventBus::new();
        let mut stream = bus.subscribe(&[]); // firehose

        // No async runtime, no spawned producer task: emit() has fully
        // delivered each event before it returns.
        for (topic, name) in INTERLEAVED {
            bus.emit(*topic, make_event(name));
        }

        let received = drain_all(&mut stream);
        let expected: Vec<String> = INTERLEAVED.iter().map(|(_, n)| n.to_string()).collect();
        assert_eq!(received, expected);
    }

    #[test]
    fn deterministic_bus_ordering_is_stable_across_repeated_runs() {
        // The whole point of #1176: replay the same interleaving many times
        // and assert byte-identical subscriber ordering every iteration. If
        // any async scheduling crept in, this would flake.
        let expected: Vec<String> = INTERLEAVED.iter().map(|(_, n)| n.to_string()).collect();
        for iteration in 0..1000 {
            let bus = DeterministicEventBus::new();
            let mut firehose = bus.subscribe(&[]);
            let mut world_only = bus.subscribe(&[Topic::WorldUpdate]);

            for (topic, name) in INTERLEAVED {
                bus.emit(*topic, make_event(name));
            }

            assert_eq!(
                drain_all(&mut firehose),
                expected,
                "firehose ordering diverged on iteration {iteration}"
            );
            // Filtered subscriber sees only its topic, still in publish order.
            assert_eq!(
                drain_all(&mut world_only),
                vec!["world-update".to_string(), "world-update".to_string()],
                "filtered ordering diverged on iteration {iteration}"
            );
        }
    }

    #[test]
    fn deterministic_bus_filters_by_topic_at_emit_time() {
        let bus = DeterministicEventBus::new();
        let mut text_only = bus.subscribe(&[Topic::TextLog]);

        bus.emit(Topic::WorldUpdate, make_event("world-update"));
        bus.emit(Topic::TextLog, make_event("text-log"));
        bus.emit(Topic::TravelStart, make_event("travel-start"));
        bus.emit(Topic::TextLog, make_event("text-log"));

        // Only the two TextLog events land in the queue, in order.
        assert_eq!(
            drain_all(&mut text_only),
            vec!["text-log".to_string(), "text-log".to_string()]
        );
    }

    #[test]
    fn deterministic_bus_multiple_subscribers_each_get_full_stream() {
        let bus = DeterministicEventBus::new();
        let mut a = bus.subscribe(&[]);
        let mut b = bus.subscribe(&[]);
        assert_eq!(bus.subscriber_count(), 2);

        bus.emit(Topic::TextLog, make_event("text-log"));
        bus.emit(Topic::WorldUpdate, make_event("world-update"));

        let expected = vec!["text-log".to_string(), "world-update".to_string()];
        assert_eq!(drain_all(&mut a), expected);
        assert_eq!(drain_all(&mut b), expected);
    }

    #[tokio::test]
    async fn deterministic_bus_recv_returns_closed_when_drained() {
        let bus = DeterministicEventBus::new();
        let mut stream = bus.subscribe(&[]);
        bus.emit(Topic::TextLog, make_event("text-log"));

        assert_eq!(stream.recv().await.unwrap().event, "text-log");
        // Nothing left and no producer to await — deterministic streams report
        // Closed rather than blocking forever.
        assert_eq!(stream.recv().await.unwrap_err(), RecvError::Closed);
    }

    #[test]
    fn deterministic_bus_emit_named_serializes_payload() {
        let bus = DeterministicEventBus::new();
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
    fn deterministic_bus_no_subscribers_does_not_panic() {
        let bus = DeterministicEventBus::new();
        bus.emit(Topic::TextLog, make_event("orphan"));
    }
}
