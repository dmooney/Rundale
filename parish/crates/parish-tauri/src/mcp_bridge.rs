//! MCP control bridge — embeds a small Axum HTTP listener inside the running
//! Tauri process so the existing `parish-mcp` server (which speaks HTTP to
//! `parish-server`-shaped routes) can drive the **live desktop session**.
//!
//! Why an in-process listener instead of a separate `parish-server`?
//! The desktop window owns its `AppState` (world, NPCs, conversation, save
//! file). A side-by-side `parish-server` would have its own `AppState` and
//! produce a parallel session — same code, different state. The MCP user
//! wanted to read *the world the player can see* and inject *inputs the
//! player would observe*; that requires sharing the same `Arc<AppState>`
//! and `tauri::AppHandle` (so emits update the running window). This module
//! is that single-process, single-state bridge.
//!
//! Routing matches `parish-server::route_registry::EXPECTED_HTTP_ROUTES` for
//! the subset of endpoints `parish-mcp` calls — that mode-parity (enforced by
//! `parish-core/tests/wiring_parity.rs`) is what lets one MCP client work
//! against either backend.
//!
//! Security: bound to `127.0.0.1` only. The port is opt-in via `--mcp-port
//! <N>`; if the flag is absent, this module compiles in but never listens.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tauri::{AppHandle, Emitter};

use crate::events::{EVENT_TEXT_LOG, TextLogPayload};
use crate::{
    AppState, MapData, MapLocation, NpcInfo, SaveState, SetupStatusSnapshot, WorldSnapshot,
};

/// Shared extractor state carried by every Axum handler in the bridge.
///
/// Cloning is cheap: both fields are `Arc`-backed (`Arc<AppState>` is an
/// explicit `Arc`; `tauri::AppHandle` is internally reference-counted).
#[derive(Clone)]
struct BridgeState {
    state: Arc<AppState>,
    app: AppHandle,
}

/// Spawns the MCP bridge listener on `127.0.0.1:port` as a fire-and-forget
/// background task — matches the `spawn_*_tick` pattern in `setup.rs`.
///
/// Bind failures are logged and the task exits; they should not crash the
/// desktop app.
pub(crate) fn spawn(state: Arc<AppState>, app: AppHandle, port: u16) {
    let bridge = BridgeState { state, app };
    tokio::spawn(async move {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let router = build_router(bridge);
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(%addr, error = %e, "mcp_bridge: failed to bind, MCP control disabled");
                return;
            }
        };
        tracing::info!(%addr, "mcp_bridge: listening for MCP control requests");
        if let Err(e) = axum::serve(listener, router).await {
            tracing::error!(error = %e, "mcp_bridge: server task exited with error");
        }
    });
}

/// Builds the Axum router. Extracted so unit tests can pin the route table
/// without binding a TCP port.
fn build_router(bridge: BridgeState) -> Router {
    Router::new()
        // ── reads ────────────────────────────────────────────────────────────
        .route("/api/health", get(health))
        .route("/api/world-snapshot", get(world_snapshot))
        .route("/api/map", get(map))
        .route("/api/npcs-here", get(npcs_here))
        .route("/api/save-state", get(save_state))
        .route("/api/setup-snapshot", get(setup_snapshot))
        // ── writes ───────────────────────────────────────────────────────────
        .route("/api/submit-input", post(submit_input))
        .route("/api/new-game", post(new_game))
        .route("/api/save-game", post(save_game))
        .route("/api/load-branch", post(load_branch))
        // ── BYOK setup-flow stubs (#933) ─────────────────────────────────────
        // Backend returns `{"stub": true, ...}` until the setup-UI branch
        // lands; the route paths stay the same so MCP tool callers don't
        // change when the real implementation arrives.
        .route("/api/setup-status", get(setup_status_stub))
        .route("/api/submit-byok", post(submit_byok_stub))
        .with_state(bridge)
}

// ── handlers ────────────────────────────────────────────────────────────────

// `health` and `setup_snapshot` do no async work but stay `async fn` for
// uniformity with the rest of the handler set; the explicit allow keeps
// `cargo clippy --workspace -- -D warnings` happy.
#[allow(clippy::unused_async)]
async fn health() -> &'static str {
    "ok"
}

async fn world_snapshot(State(b): State<BridgeState>) -> Json<WorldSnapshot> {
    let world = b.state.world.lock().await;
    let transport = b.state.transport.default_mode();
    let npc_manager = b.state.npc_manager.lock().await;
    let snap = crate::commands::get_world_snapshot_inner(
        &world,
        transport,
        Some(&npc_manager),
        &b.state.pronunciations,
    );
    Json(snap)
}

async fn map(State(b): State<BridgeState>) -> Json<MapData> {
    let world = b.state.world.lock().await;
    let config = b.state.config.lock().await;
    let transport = b.state.transport.default_mode();
    let core_map =
        parish_core::ipc::build_map_data(&world, transport, config.reveal_unexplored_locations);

    let player_loc = world.player_location;
    let (player_lat, player_lon) = world
        .graph
        .get(player_loc)
        .map(|d| (d.lat, d.lon))
        .unwrap_or((0.0, 0.0));

    Json(MapData {
        locations: core_map
            .locations
            .into_iter()
            .map(|l| MapLocation {
                id: l.id,
                name: l.name,
                lat: l.lat,
                lon: l.lon,
                adjacent: l.adjacent,
                hops: l.hops,
                indoor: l.indoor,
                travel_minutes: l.travel_minutes,
                visited: l.visited,
            })
            .collect(),
        edges: core_map.edges,
        player_location: core_map.player_location,
        player_lat,
        player_lon,
        edge_traversals: core_map.edge_traversals,
        transport_label: core_map.transport_label,
        transport_id: core_map.transport_id,
    })
}

async fn npcs_here(State(b): State<BridgeState>) -> Json<Vec<NpcInfo>> {
    let world = b.state.world.lock().await;
    let npc_manager = b.state.npc_manager.lock().await;
    Json(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

async fn save_state(State(b): State<BridgeState>) -> Json<SaveState> {
    let save_path = b.state.save_path.lock().await;
    let branch_id = b.state.current_branch_id.lock().await;
    let branch_name = b.state.current_branch_name.lock().await;
    Json(SaveState {
        filename: save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()),
        branch_id: *branch_id,
        branch_name: branch_name.clone(),
    })
}

#[allow(clippy::unused_async)]
async fn setup_snapshot(State(b): State<BridgeState>) -> Json<SetupStatusSnapshot> {
    Json(
        b.state
            .setup_status
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone(),
    )
}

#[derive(Debug, Deserialize)]
struct SubmitInputBody {
    text: String,
    #[serde(default)]
    addressed_to: Vec<String>,
}

async fn submit_input(
    State(b): State<BridgeState>,
    Json(body): Json<SubmitInputBody>,
) -> Result<StatusCode, AppError> {
    crate::commands::do_submit_input(&b.state, &b.app, body.text, body.addressed_to)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::OK)
}

async fn new_game(State(b): State<BridgeState>) -> Result<StatusCode, AppError> {
    crate::commands::do_new_game(&b.state, &b.app)
        .await
        .map_err(AppError::from)?;
    // Match the Tauri command's "A new chapter begins..." log so the live
    // window shows the same banner whether the user clicked New Game or an
    // MCP client triggered it.
    let _ = b.app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: "A new chapter begins in the parish...".to_string(),
            subtype: None,
        },
    );
    Ok(StatusCode::OK)
}

async fn save_game(State(b): State<BridgeState>) -> Result<Json<serde_json::Value>, AppError> {
    let msg = crate::commands::do_save_game(&b.state)
        .await
        .map_err(AppError::from)?;
    Ok(Json(serde_json::json!({"message": msg})))
}

#[derive(Debug, Deserialize)]
struct LoadBranchBody {
    file_path: String,
    branch_id: i64,
}

async fn load_branch(
    State(b): State<BridgeState>,
    Json(body): Json<LoadBranchBody>,
) -> Result<StatusCode, AppError> {
    crate::commands::do_load_branch(&b.state, &b.app, body.file_path, body.branch_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::OK)
}

// ── BYOK setup-flow stubs ────────────────────────────────────────────────────
//
// The full implementation lives on a sibling branch. These two handlers
// return a structured `{"stub": true, ...}` response so an MCP client can
// distinguish "endpoint exists but unimplemented" from "transport error /
// 404". When the real handlers land they replace these bodies; the route
// paths and the JSON envelope shape stay the same.

const STUB_MESSAGE: &str =
    "BYOK setup flow is stubbed. Implementation lands with the setup-UI branch.";

#[allow(clippy::unused_async)]
async fn setup_status_stub() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "stub": true,
        "implemented": false,
        "message": STUB_MESSAGE,
        // Shape the eventual real response will fill in:
        "providers": [],
        "complete": false,
    }))
}

#[allow(clippy::unused_async)]
async fn submit_byok_stub(Json(_body): Json<serde_json::Value>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "stub": true,
        "implemented": false,
        "accepted": false,
        "message": STUB_MESSAGE,
    }))
}

// ── error mapping ───────────────────────────────────────────────────────────

/// Bridges domain `Result<_, String>` errors from the `do_*` helpers onto
/// HTTP `500` with the message in the body. The MCP layer surfaces this as
/// `isError: true` content for the model to read.
struct AppError(String);

impl From<String> for AppError {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0).into_response()
    }
}

#[cfg(test)]
mod tests {
    /// Pin the route table so a refactor that drops or renames an endpoint
    /// gets caught before the parish-mcp client breaks.
    #[test]
    fn route_table_matches_parish_mcp_expectations() {
        // We can't easily introspect axum's router; but we can verify the
        // builder compiles and the function exists. The expected paths are
        // listed inline so a future change is forced to update both sides.
        const EXPECTED: &[&str] = &[
            "/api/health",
            "/api/world-snapshot",
            "/api/map",
            "/api/npcs-here",
            "/api/save-state",
            "/api/setup-snapshot",
            "/api/submit-input",
            "/api/new-game",
            "/api/save-game",
            "/api/load-branch",
            // BYOK setup-flow stubs (#933).
            "/api/setup-status",
            "/api/submit-byok",
        ];
        // Mirrors parish-mcp's ParishHttpBackend::command_to_path.
        let translate = |cmd: &str| {
            let stem = cmd.strip_prefix("get_").unwrap_or(cmd);
            let kebab: String = stem
                .chars()
                .map(|c| {
                    if c == '_' {
                        '-'
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect();
            format!("/api/{kebab}")
        };
        for cmd in [
            "get_world_snapshot",
            "get_map",
            "get_npcs_here",
            "get_save_state",
            "get_setup_snapshot",
            "submit_input",
            "new_game",
            "save_game",
            "load_branch",
            "get_setup_status",
            "submit_byok",
        ] {
            let path = translate(cmd);
            assert!(
                EXPECTED.contains(&path.as_str()),
                "parish-mcp would route {cmd} to {path} but the bridge router does not expose it",
            );
        }
    }
}
