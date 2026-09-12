//! Server-Sent Events helpers for streaming in-progress `TurnEvent`s.
//!
//! The broadcast channel is created in `AppState`. For Phase 2 there are no
//! publishers (the run loop runs in a separate process); the SSE endpoint is
//! implemented correctly so it streams when a publisher exists.

use axum::response::sse::Event;
use tokio::sync::broadcast;

use crate::score::TurnEvent;

/// Capacity of the broadcast channel (events before slow readers are dropped).
pub const BROADCAST_CAPACITY: usize = 64;

/// Create a broadcast sender/receiver pair for live `TurnEvent`s.
pub fn channel() -> broadcast::Sender<TurnEventMsg> {
    let (tx, _) = broadcast::channel(BROADCAST_CAPACITY);
    tx
}

/// A serializable wrapper around `TurnEvent` for SSE.
#[derive(Debug, Clone)]
pub struct TurnEventMsg {
    pub run_id: i64,
    pub turn_index: u32,
    pub kind: String,
    pub detail: String,
}

impl TurnEventMsg {
    /// Render as an SSE `Event`.
    pub fn to_sse_event(&self) -> Event {
        let data = serde_json::json!({
            "run_id": self.run_id,
            "turn_index": self.turn_index,
            "kind": self.kind,
            "detail": self.detail,
        });
        Event::default().event("turn").data(data.to_string())
    }
}

/// Convert a `TurnEvent` into a `TurnEventMsg` for broadcast.
pub fn from_turn_event(run_id: i64, turn_index: u32, ev: &TurnEvent) -> TurnEventMsg {
    let (kind, detail) = match ev {
        TurnEvent::Response { outcome, state_sig } => (
            "response".to_string(),
            format!(
                "outcome={:?} sig={}",
                outcome,
                state_sig.as_deref().unwrap_or("-")
            ),
        ),
        TurnEvent::Crash(msg) => ("crash".to_string(), msg.clone()),
        TurnEvent::JsonParseFail(ctx) => ("json_parse_fail".to_string(), ctx.clone()),
    };
    TurnEventMsg {
        run_id,
        turn_index,
        kind,
        detail,
    }
}
