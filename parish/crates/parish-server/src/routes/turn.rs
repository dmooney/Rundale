//! Compact synchronous turn-state endpoint used by MCP agents.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Query};
use parish_core::ipc::{TurnReadParams, TurnReadResult};

use crate::state::AppState;

/// `GET /api/turn?since=<cursor>` — returns bounded canonical conversation
/// exchanges, world-event deltas, and core scene state.
pub async fn get_turn(
    Extension(state): Extension<Arc<AppState>>,
    Query(params): Query<TurnReadParams>,
) -> Json<TurnReadResult> {
    let (events, event_cursor) = {
        let events = state.game_events.lock().await;
        let total = state
            .total_game_events
            .load(std::sync::atomic::Ordering::Relaxed);
        parish_core::ipc::events_since(&events, total, params.since.unwrap_or(0))
    };
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    Json(parish_core::ipc::build_turn_read_result(
        &world,
        &npc_manager,
        events,
        event_cursor,
    ))
}
