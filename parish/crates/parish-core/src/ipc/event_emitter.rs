//! Abstract event-emission trait shared by all Parish backends.
//!
//! The three backends emit events differently:
//!
//! - `parish-tauri` uses `tauri::AppHandle::emit(name, payload)`.
//! - `parish-server` uses `EventBus::emit_named(topic, name, &payload)` which
//!   serialises via `serde_json::to_value` and broadcasts to WebSocket clients.
//! - `parish-engine` headless mode uses `StdoutEmitter` which logs `text-log`
//!   payloads to stdout and no-ops on all other event types.
//!
//! This trait is **object-safe** (no generic methods) so `Arc<dyn EventEmitter>`
//! can be passed into shared game-loop helpers (`run_npc_turn`,
//! `handle_npc_conversation`, etc.) without duplicating them.  Serialisation is
//! the caller's responsibility: callers convert their typed payload to
//! `serde_json::Value` before calling `emit_event`.
//!
//! # Backend implementations
//!
//! Each backend supplies a thin newtype:
//!
//! - **`parish-server`**: `BroadcastEmitter(Arc<BroadcastEventBus>)` → maps
//!   each `(name, payload)` to the correct [`Topic`] and calls `emit_named`.
//! - **`parish-tauri`**: `TauriEmitter(tauri::AppHandle)` → calls
//!   `app.emit(name, payload)`.
//! - **`parish-engine`**: `StdoutEmitter` → prints `text-log` content to stdout;
//!   no-ops on `world-update`, `stream-token`, `loading`, etc.

/// Lifecycle event emitted before replacing the frontend's canonical game
/// context during new-game and branch-load operations.
pub const EVENT_GAME_CONTEXT_RESET: &str = "game-context-reset";

/// Backend-agnostic event emission.
///
/// Implementors bridge the generic `(name, json_payload)` call to whatever
/// transport their runtime uses (Tauri IPC, WebSocket broadcast, stdout, etc.).
///
/// All implementations must be `Send + Sync` so they can be shared across
/// async tasks via `Arc<dyn EventEmitter>`.
pub trait EventEmitter: Send + Sync {
    /// Emits a named event with a pre-serialised JSON payload.
    ///
    /// `name` must match a frontend event listener (e.g. `"text-log"`,
    /// `"world-update"`).  `payload` is the serialised form of whichever IPC
    /// type corresponds to that event.
    fn emit_event(&self, name: &str, payload: serde_json::Value);
}

/// Emits the canonical lifecycle reset immediately before its world snapshot.
pub fn emit_game_context_reset_then_world_update(
    emitter: &dyn EventEmitter,
    world_update: serde_json::Value,
) {
    emitter.emit_event(EVENT_GAME_CONTEXT_RESET, serde_json::Value::Null);
    emitter.emit_event("world-update", world_update);
}

/// An [`EventEmitter`] that records every emitted `(name, payload)` in order
/// instead of forwarding it to a transport.
///
/// This is the capture seam for differential ("shadow") testing of the game
/// harness against the real `game_loop`: drive the loop with a
/// `CapturingEmitter`, then read back the exact event stream the loop produced
/// for normalization and comparison. Lives next to the trait (not behind
/// `#[cfg(test)]`) because the consumer — `parish-engine`'s harness — is a
/// separate crate and needs it across the crate boundary.
#[derive(Default)]
pub struct CapturingEmitter {
    events: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
}

impl CapturingEmitter {
    /// Creates an empty emitter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a clone of every captured event, in emission order.
    pub fn events(&self) -> Vec<(String, serde_json::Value)> {
        self.events.lock().unwrap().clone()
    }

    /// Removes and returns every captured event, leaving the buffer empty.
    pub fn drain(&self) -> Vec<(String, serde_json::Value)> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }

    /// Number of events captured so far.
    pub fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }

    /// Whether no events have been captured.
    pub fn is_empty(&self) -> bool {
        self.events.lock().unwrap().is_empty()
    }
}

impl EventEmitter for CapturingEmitter {
    fn emit_event(&self, name: &str, payload: serde_json::Value) {
        self.events
            .lock()
            .unwrap()
            .push((name.to_string(), payload));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn captures_events_in_order_with_intact_payloads() {
        let emitter = CapturingEmitter::new();
        assert!(emitter.is_empty());

        emitter.emit_event("text-log", json!({ "text": "You look around." }));
        emitter.emit_event("world-update", json!({ "location": "church" }));

        let events = emitter.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, "text-log");
        assert_eq!(events[0].1, json!({ "text": "You look around." }));
        assert_eq!(events[1].0, "world-update");
        assert_eq!(events[1].1, json!({ "location": "church" }));

        // drain empties the buffer and returns the same sequence.
        let drained = emitter.drain();
        assert_eq!(drained, events);
        assert!(emitter.is_empty());
    }

    #[test]
    fn game_context_reset_precedes_world_update() {
        let emitter = CapturingEmitter::new();
        emit_game_context_reset_then_world_update(&emitter, json!({ "location": "fresh-context" }));

        let events = emitter.events();
        assert_eq!(
            events[0],
            (
                EVENT_GAME_CONTEXT_RESET.to_string(),
                serde_json::Value::Null
            )
        );
        assert_eq!(events[1].0, "world-update");
        assert_eq!(events[1].1["location"], "fresh-context");
    }
}
