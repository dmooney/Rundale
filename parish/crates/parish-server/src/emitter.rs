//! [`AppStateEmitter`] — [`EventEmitter`] implementation for the axum web server.
//!
//! Wraps `Arc<AppState>` and maps each `(event_name, json_payload)` pair to
//! the correct [`Topic`] before broadcasting to all WebSocket subscribers via
//! `AppState::event_bus`.
//!
//! Using `Arc<AppState>` rather than `Arc<BroadcastEventBus>` lets callers
//! create the emitter from any existing `Arc<AppState>` borrow without
//! constructing a separate reference-counted bus handle.
//!
//! # Topic mapping
//!
//! Delegates to [`Topic::from_event_name`].  Unknown event names fall back to
//! `Topic::TextLog` with a warning log.

use std::sync::Arc;

use parish_core::event_bus::{EventBus as EventBusTrait, ServerEvent, Topic};
use parish_core::ipc::EventEmitter;

use crate::state::AppState;

/// [`EventEmitter`] implementation for the axum web server.
///
/// Clones the `Arc<AppState>` on construction; emit calls are cheap since
/// [`AppState::event_bus`] is a `BroadcastEventBus` backed by a
/// `tokio::sync::broadcast` sender (clone-by-reference).
#[derive(Clone)]
pub struct AppStateEmitter {
    state: Arc<AppState>,
}

impl AppStateEmitter {
    /// Creates a new emitter borrowing the session's AppState.
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl EventEmitter for AppStateEmitter {
    fn emit_event(&self, name: &str, payload: serde_json::Value) {
        let topic = Topic::from_event_name(name).unwrap_or_else(|| {
            tracing::warn!(
                event = %name,
                "AppStateEmitter: unknown event name — falling back to TextLog topic"
            );
            Topic::TextLog
        });
        self.state.event_bus.emit(
            topic,
            ServerEvent {
                event: name.to_string(),
                payload,
            },
        );
    }
}

impl From<Arc<AppState>> for AppStateEmitter {
    fn from(state: Arc<AppState>) -> Self {
        Self::new(state)
    }
}
