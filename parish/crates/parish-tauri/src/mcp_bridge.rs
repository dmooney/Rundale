use std::sync::Arc;
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use axum::Json;
use serde_json::json;
use tokio::net::TcpListener;

use crate::state::AppState;
use parish_core::ipc::{
    commands::{MapData, WorldSnapshot},
    handlers,
};

// ── Bridge state ─────────────────────────────────────────────────────────────

#[derive(Clone)]
struct BridgeState {
    state: Arc<AppState>,
}

// ── Router ───────────────────────────────────────────────────────────────────

pub fn bridge_router(state: Arc<AppState>) -> Router {
    let bridge = BridgeState { state };
    Router::new()
        .route("/api/health", get(health))
        .route("/api/world", get(world_snapshot))
        .route("/api/map", get(map))
        .route("/api/npcs-here", get(npcs_here))
        .route("/api/save-state", get(save_state))
        .with_state(bridge)
}

pub async fn run_bridge(state: Arc<AppState>, port: u16) -> anyhow::Result<()> {
    let router = bridge_router(state);
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    tracing::info!(port, "MCP bridge listening");
    axum::serve(listener, router).await?;
    Ok(())
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn health() -> impl IntoResponse {
    Json(json!({"status": "ok"}))
}

async fn world_snapshot(State(b): State<BridgeState>) -> Json<WorldSnapshot> {
    let world = b.state.world.lock().await;
    let npc_manager = b.state.npc_manager.lock().await;
    let snap = crate::commands::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &b.state.pronunciations,
    );
    Json(snap)
}

async fn map(State(b): State<BridgeState>) -> Json<MapData> {
    let world = b.state.world.lock().await;
    let snap = handlers::map_from_world(&world);
    Json(snap)
}

async fn npcs_here(State(b): State<BridgeState>) -> impl IntoResponse {
    let world = b.state.world.lock().await;
    let npc_manager = b.state.npc_manager.lock().await;
    let location = world.current_location();
    let npcs = npc_manager.npcs_at(location);
    Json(json!(npcs.iter().map(|n| n.name.as_str()).collect::<Vec<_>>()))
}

async fn save_state(State(b): State<BridgeState>) -> impl IntoResponse {
    let session = b.state.session.lock().await;
    match serde_json::to_value(&*session) {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            let msg = format!("serialize error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
        }
    }
}
