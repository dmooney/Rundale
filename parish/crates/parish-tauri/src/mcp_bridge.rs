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

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use parish_core::ipc::{SubmitInputRequest, SubmitInputResult, TurnReadParams, TurnReadResult};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::commands::ScreenshotInfo;
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
        .route("/api/graphical-ready", get(graphical_ready))
        .route("/api/world-snapshot", get(world_snapshot))
        .route("/api/map", get(map))
        .route("/api/npcs-here", get(npcs_here))
        // `/api/engine-state` — canonical deterministic engine state for the
        // MCP QA loop (#1331). Backs the `parish_engine_state` tool.
        .route("/api/engine-state", get(engine_state))
        .route("/api/save-state", get(save_state))
        .route("/api/transcript", get(transcript))
        .route("/api/setup-snapshot", get(setup_snapshot))
        // `/api/turn` — slim per-turn read (#1356 / #1353). Returns last N
        // exchanges + recent world events + core state. Bounded size; accepts
        // an optional `?since=<cursor>` to stream only new events.
        .route("/api/turn", get(turn_read))
        // `/api/debug-snapshot` — same introspection blob `parish-server`
        // exposes. The bridge previously omitted it, so a desktop-launched
        // MCP session got a 404 where the web server returned data (#1207 #16).
        .route("/api/debug-snapshot", get(debug_snapshot))
        // ── writes ───────────────────────────────────────────────────────────
        .route("/api/submit-input", post(submit_input))
        .route("/api/new-game", post(new_game))
        .route("/api/save-game", post(save_game))
        .route("/api/load-branch", post(load_branch))
        // ── Screenshot routes ────────────────────────────────────────────────
        // GET: read the most recently captured screenshot path.
        // POST /api/take-screenshot: agent-triggered capture. Tries a fresh
        // native/window capture first; if the window is not capturable but a
        // previous verified screenshot exists, returns that latest path with a
        // warning instead of surfacing a generic 500 (#1522).
        .route("/api/latest-screenshot", get(latest_screenshot))
        .route("/api/take-screenshot", post(take_screenshot_mcp))
        // ── Bug reporting ─────────────────────────────────────────────────
        // POST /api/submit-bug-report: bundle a screenshot (captured via the
        // same round-trip as take-screenshot) + logs + game state into a
        // GitHub issue. Backs the `parish_file_bug` MCP tool.
        .route("/api/submit-bug-report", post(submit_bug_report))
        // ── BYOK setup-flow (#933) ────────────────────────────────────────
        // Real handlers backed by `parish_core::ipc::byok` — the Svelte
        // wizard and the MCP client share these. Routes match the schema
        // the stubs originally pinned.
        .route("/api/setup-status", get(setup_status))
        .route("/api/submit-byok", post(submit_byok))
        .route("/api/byok-env-keys", get(byok_env_keys))
        .route("/api/preset-models", get(preset_models))
        .route("/api/list-available-providers", get(available_providers))
        // ── Local-inference onboarding (vllm-mlx fork) ────────────────────
        // GET returns the same fork-variant + RAM data the Svelte
        // `LocalInferenceFork` reads on mount. POST drives the same
        // download + provider-config path the desktop button triggers,
        // so an MCP client can run the full first-run flow headless.
        .route("/api/onboarding-options", get(onboarding_options))
        .route("/api/start-local-inference", post(start_local_inference))
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

/// Graphical readiness is deliberately distinct from bridge health: the MCP
/// listener comes up before the Svelte/Pixi surface has mounted so onboarding
/// remains driveable, but screenshot capture requires a presented UI frame.
async fn graphical_ready(
    State(b): State<BridgeState>,
) -> Json<crate::commands::GraphicalReadiness> {
    Json(crate::commands::graphical_readiness(&b.state))
}

async fn debug_snapshot(
    State(b): State<BridgeState>,
) -> Json<parish_core::debug_snapshot::DebugSnapshot> {
    Json(crate::commands::admin::build_app_debug_snapshot(&b.state).await)
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

/// `GET /api/engine-state` — canonical deterministic engine state (#1331).
/// Shares the `parish_core::ipc::build_engine_state` builder with the web
/// server so the desktop and server snapshots can never drift (rule #12).
async fn engine_state(
    State(b): State<BridgeState>,
) -> Result<Json<parish_core::ipc::EngineState>, AppError> {
    if b.state
        .config
        .lock()
        .await
        .flags
        .is_disabled("engine-state")
    {
        return Err(AppError("the engine-state feature is disabled".to_string()));
    }
    let world = b.state.world.lock().await;
    let npc_manager = b.state.npc_manager.lock().await;
    Ok(Json(parish_core::ipc::build_engine_state(
        &world,
        &npc_manager,
    )))
}

// ── Transcript (kept for Tauri UI compatibility) ─────────────────────────────

#[derive(Serialize)]
struct TranscriptLine {
    speaker: String,
    text: String,
}

async fn transcript(State(b): State<BridgeState>) -> Json<Vec<TranscriptLine>> {
    let conv = b.state.conversation.lock().await;
    let lines = conv
        .transcript
        .iter()
        .map(|l| TranscriptLine {
            speaker: l.speaker.clone(),
            text: l.text.clone(),
        })
        .collect();
    Json(lines)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Projects a completed input turn from the canonical conversation log.
async fn build_submit_result(
    state: &Arc<AppState>,
    before_turn: parish_core::npc::conversation::ConversationCursor,
) -> SubmitInputResult {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    parish_core::ipc::build_submit_input_result(&world, &npc_manager, before_turn)
}

/// Projects a compact turn read from canonical exchanges and world events.
async fn build_turn_result(state: &Arc<AppState>, since_cursor: usize) -> TurnReadResult {
    let (events, event_cursor) = {
        let events = state.game_events.lock().await;
        let total = state
            .total_game_events
            .load(std::sync::atomic::Ordering::Relaxed);
        parish_core::ipc::events_since(&events, total, since_cursor)
    };
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    parish_core::ipc::build_turn_read_result(&world, &npc_manager, events, event_cursor)
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

/// `POST /api/submit-input` — bridge handler.
///
/// Keeps `do_submit_input` signature unchanged (the Tauri UI command still
/// returns `Result<(), String>`). The bridge wrapper captures the canonical
/// conversation-log cursor before dispatch and projects exchanges added after
/// it, so player presentation lines can never masquerade as NPC replies
/// (#1353 / #1356 / #1777 / #1778).
async fn submit_input(
    State(b): State<BridgeState>,
    Json(body): Json<SubmitInputRequest>,
) -> Result<Json<SubmitInputResult>, AppError> {
    let _persistence_guard = b.state.persistence_gate.lock().await;
    let before_turn = {
        let world = b.state.world.lock().await;
        parish_core::ipc::conversation_cursor(&world)
    };

    crate::commands::input::do_submit_input_locked(&b.state, &b.app, body.text, body.addressed_to)
        .await
        .map_err(AppError::from)?;

    Ok(Json(build_submit_result(&b.state, before_turn).await))
}

/// `GET /api/turn?since=<cursor>` — slim per-turn read (#1356).
///
/// Returns the bounded canonical exchange/event projection plus core state.
/// Does not require `get_debug_snapshot`.
async fn turn_read(
    State(b): State<BridgeState>,
    Query(params): Query<TurnReadParams>,
) -> Result<Json<TurnReadResult>, AppError> {
    let _persistence_guard = b.state.persistence_gate.lock().await;
    let since_cursor = params.since.unwrap_or(0);
    Ok(Json(build_turn_result(&b.state, since_cursor).await))
}

async fn new_game(State(b): State<BridgeState>) -> Result<StatusCode, AppError> {
    let _persistence_guard = b.state.persistence_gate.lock().await;
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
    let _persistence_guard = b.state.persistence_gate.lock().await;
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
    let _persistence_guard = b.state.persistence_gate.lock().await;
    crate::commands::do_load_branch(&b.state, &b.app, body.file_path, body.branch_id)
        .await
        .map_err(AppError::from)?;
    Ok(StatusCode::OK)
}

async fn latest_screenshot(
    State(b): State<BridgeState>,
) -> Result<Json<Option<ScreenshotInfo>>, AppError> {
    let info = crate::commands::do_get_latest_screenshot(&b.state)
        .await
        .map_err(AppError::from)?;
    Ok(Json(info))
}

async fn take_screenshot_mcp(State(b): State<BridgeState>) -> axum::response::Response {
    match crate::commands::do_take_screenshot(&b.state, &b.app).await {
        Ok(info) => Json(info).into_response(),
        Err(e) => screenshot_unavailable_response(e, None),
    }
}

fn screenshot_unavailable_response(
    capture_error: String,
    latest_error: Option<String>,
) -> axum::response::Response {
    let mut body = serde_json::json!({
        "error": "screenshot capture unavailable",
        "detail": capture_error,
        "hint": "Wait for the graphical UI readiness signal, then retry the fresh capture.",
    });
    if let Some(latest_error) = latest_error {
        body["latest_error"] = serde_json::Value::String(latest_error);
    }
    (StatusCode::SERVICE_UNAVAILABLE, Json(body)).into_response()
}

async fn submit_bug_report(
    State(b): State<BridgeState>,
    Json(body): Json<parish_core::ipc::BugReportRequest>,
) -> Result<Json<parish_core::ipc::BugReportResult>, AppError> {
    let result = crate::commands::do_submit_bug_report(&b.state, &b.app, body)
        .await
        .map_err(AppError::from)?;
    Ok(Json(result))
}

// ── BYOK setup-flow ──────────────────────────────────────────────────────────
//
// Real handlers backed by the shared `parish_core::ipc::byok` orchestration —
// they share the same `AppState`, secret store, and user-config dir as the
// Svelte BYOK wizard, so an MCP client and the desktop UI converge on
// identical effects.

#[derive(Debug, Deserialize)]
pub(crate) struct SubmitByokBody {
    pub(crate) provider: String,
    #[serde(default)]
    pub(crate) api_key: Option<String>,
    #[serde(default)]
    pub(crate) base_url: Option<String>,
    #[serde(default)]
    pub(crate) model: Option<String>,
}

#[allow(clippy::unused_async)]
async fn byok_env_keys() -> Json<std::collections::BTreeMap<String, bool>> {
    Json(parish_core::ipc::byok::handle_list_env_keys())
}

#[allow(clippy::unused_async)]
async fn preset_models()
-> Json<std::collections::BTreeMap<String, Vec<parish_core::ipc::byok::ProviderPresetOption>>> {
    Json(parish_core::ipc::byok::handle_list_preset_models())
}

#[allow(clippy::unused_async)]
async fn available_providers()
-> Json<std::collections::HashMap<&'static str, Vec<parish_core::ipc::byok::ProviderInfo>>> {
    Json(parish_core::ipc::byok::handle_list_available_providers())
}

async fn setup_status(State(b): State<BridgeState>) -> Json<serde_json::Value> {
    Json(do_setup_status(&b.state).await)
}

async fn submit_byok(
    State(b): State<BridgeState>,
    Json(body): Json<SubmitByokBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let response = do_submit_byok(&b.state, body)
        .await
        .map_err(AppError::from)?;

    // Clear the BYOK gate flag and signal completion so the SetupOverlay
    // (if any) dismisses through the same channel the desktop wizard uses.
    {
        let mut s = b
            .state
            .setup_status
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        s.clear_needs_onboarding();
    }
    crate::record_setup_done(&b.state, true, String::new());
    let _ = b.app.emit(
        crate::events::EVENT_SETUP_DONE,
        crate::events::SetupDonePayload {
            success: true,
            error: String::new(),
        },
    );

    Ok(Json(response))
}

/// Internal setup-status builder. Pure (no side effects), no AppHandle —
/// exists so the bridge route AND tests can call it.
pub(crate) async fn do_setup_status(state: &Arc<AppState>) -> serde_json::Value {
    let provider = parish_core::ipc::byok::handle_get_provider_config(&state.config).await;
    let complete = parish_core::config::user_config::onboarding_complete(&state.user_config_dir);
    serde_json::json!({
        "implemented": true,
        "complete": complete,
        "provider": provider.provider,
        "model": provider.model,
        "base_url": provider.base_url,
        "has_api_key": provider.has_api_key,
        "has_env_key": provider.has_env_key,
    })
}

/// Internal submit-byok worker. Persists the key + config and rebuilds the
/// inference worker; the caller emits `setup-done` if it has an AppHandle.
pub(crate) async fn do_submit_byok(
    state: &Arc<AppState>,
    body: SubmitByokBody,
) -> Result<serde_json::Value, String> {
    let args = parish_core::ipc::byok::SetProviderConfigArgs {
        provider: body.provider,
        base_url: body.base_url,
        model: body.model,
        api_key: body.api_key,
        category_overrides: Default::default(),
    };
    let ctx = parish_core::ipc::byok::ByokContext {
        config: &state.config,
        inference_config: &state.inference_config,
        inference_log: state.inference_log.clone(),
        inference_file_log: state.inference_file_log.clone(),
        slots: parish_core::game_loop::inference::InferenceSlots {
            client: &state.client,
            worker_handle: &state.worker_handle,
            inference_queue: &state.inference_queue,
        },
        secrets: std::sync::Arc::clone(&state.secret_store),
        user_config_dir: state.user_config_dir.as_path(),
    };
    parish_core::ipc::byok::handle_set_provider_config(args, ctx)
        .await
        .map_err(|e| e.to_string())?;

    let snapshot = parish_core::ipc::byok::handle_get_provider_config(&state.config).await;
    Ok(serde_json::json!({
        "ok": true,
        "provider": snapshot.provider,
        "model": snapshot.model,
        "base_url": snapshot.base_url,
        "has_api_key": snapshot.has_api_key,
    }))
}

// ── Local-inference onboarding routes ───────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub(crate) struct StartLocalInferenceBody {
    pub(crate) variant: String,
}

async fn onboarding_options(State(b): State<BridgeState>) -> Json<serde_json::Value> {
    let opts = crate::commands::do_get_onboarding_options(&b.state).await;
    Json(serde_json::to_value(&opts).unwrap_or(serde_json::json!({})))
}

async fn start_local_inference(
    State(b): State<BridgeState>,
    Json(body): Json<StartLocalInferenceBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::commands::do_start_local_inference_setup(
        &b.state,
        &b.app,
        crate::commands::LocalSetupArgs {
            variant: body.variant,
        },
    )
    .await
    .map_err(AppError::from)?;

    Ok(Json(serde_json::json!({ "ok": true })))
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
    use super::*;
    use parish_core::secret_store::InMemorySecretStore;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;
    use tokio_util::sync::CancellationToken;

    use crate::{
        AppState, ConversationRuntimeState, DEBUG_EVENT_CAPACITY, DemoConfig, GameConfig,
        UiConfigSnapshot,
    };

    /// Minimal AppState for byok bridge tests. Uses a TempDir for
    /// `user_config_dir` and an `InMemorySecretStore` so nothing escapes the
    /// test sandbox. Intentionally lighter than `commands::cmd_tests::test_app_state`:
    /// loads no world / NPC data, since the byok handlers don't read those.
    fn byok_test_state(dir: &TempDir) -> Arc<AppState> {
        use parish_core::inference::new_inference_log;
        use parish_core::npc::manager::NpcManager;
        use parish_core::world::WorldState;
        use parish_core::world::transport::TransportConfig;

        let world = WorldState::default();
        let npc_manager = NpcManager::new();
        let transport = TransportConfig::default();
        let ui_config = UiConfigSnapshot {
            hints_label: "test".to_string(),
            default_accent: "#000".to_string(),
            splash_text: String::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            auto_pause_timeout_seconds: 300,
            app_icon_url: None,
            favicon_url: None,
            map_overlay: None,
            base_mod_required: false,
        };
        let theme_palette = parish_core::game_mod::default_theme_palette();
        let game_config = GameConfig {
            provider_name: "simulator".to_string(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
        };
        let saves_dir = dir.path().join("saves");
        let session_store: Arc<dyn parish_core::session_store::SessionStore> = Arc::new(
            parish_core::session_store::DbSessionStore::new(saves_dir.clone()),
        );

        Arc::new(AppState {
            persistence_gate: Mutex::new(()),
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            conversation: Mutex::new(ConversationRuntimeState::new()),
            debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
                DEBUG_EVENT_CAPACITY,
            )),
            game_events: Mutex::new(std::collections::VecDeque::with_capacity(
                DEBUG_EVENT_CAPACITY,
            )),
            total_game_events: std::sync::atomic::AtomicUsize::new(0),
            game_mod: None,
            inference_log: new_inference_log(),
            ui_config,
            theme_palette,
            theme_keyframes: Vec::new(),
            static_raw_palette: None,
            inference_failure_messages: Vec::new(),
            idle_messages: Vec::new(),
            pronunciations: Vec::new(),
            reaction_templates: parish_core::npc::reactions::ReactionTemplates::default(),
            save_path: Mutex::new(None),
            current_branch_id: Mutex::new(None),
            current_branch_name: Mutex::new(None),
            transport,
            data_dir: dir.path().to_path_buf(),
            saves_dir,
            worker_handle: Mutex::new(None),
            editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
            save_lock: Mutex::new(None),
            runtime_processes: Mutex::new(
                parish_core::inference::client::RuntimeProcesses::default(),
            ),
            inference_config: parish_core::config::InferenceConfig::default(),
            setup_status: std::sync::Mutex::new(crate::SetupStatusSnapshot::default()),
            wizard_in_flight: std::sync::atomic::AtomicBool::new(false),
            language_settings: parish_core::npc::LanguageSettings::english_only(),
            config: Mutex::new(game_config),
            demo_config: DemoConfig::default(),
            shutdown_token: CancellationToken::new(),
            sim_cancel: Mutex::new(CancellationToken::new()),
            session_store,
            user_config_dir: dir.path().to_path_buf(),
            secret_store: Arc::new(InMemorySecretStore::new()),
            latest_screenshot_path: Mutex::new(None),
            graphical_launch_token: uuid::Uuid::new_v4().to_string(),
            graphical_ready: std::sync::atomic::AtomicBool::new(false),
            graphical_error: std::sync::Mutex::new(None),
            pending_screenshots: Mutex::new(std::collections::HashMap::new()),
            inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
            chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
        })
    }

    #[tokio::test]
    async fn setup_status_reports_incomplete_before_byok() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let res = do_setup_status(&state).await;
        assert_eq!(res["implemented"], serde_json::Value::Bool(true));
        assert_eq!(res["complete"], serde_json::Value::Bool(false));
        assert_eq!(res["has_api_key"], serde_json::Value::Bool(false));
    }

    #[tokio::test]
    async fn submit_byok_anthropic_persists_and_rebuilds() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        let body = SubmitByokBody {
            provider: "anthropic".to_string(),
            api_key: Some("sk-ant-mcp-test".to_string()),
            base_url: None,
            model: Some("claude-opus-4-7".to_string()),
        };
        let response = do_submit_byok(&state, body).await.unwrap();
        assert_eq!(response["ok"], serde_json::Value::Bool(true));
        assert_eq!(response["provider"], "anthropic");
        assert_eq!(response["model"], "claude-opus-4-7");
        assert_eq!(response["has_api_key"], serde_json::Value::Bool(true));

        // Live AppState is updated.
        {
            let cfg = state.config.lock().await;
            assert_eq!(cfg.provider_name, "anthropic");
            assert_eq!(cfg.api_key.as_deref(), Some("sk-ant-mcp-test"));
        }
        // Keychain persists the secret.
        assert_eq!(
            state
                .secret_store
                .get("provider:anthropic")
                .unwrap()
                .as_deref(),
            Some("sk-ant-mcp-test")
        );
        // On-disk parish.toml exists, no api_key field in it.
        let toml_body = std::fs::read_to_string(dir.path().join("parish.toml")).unwrap();
        assert!(toml_body.contains("provider = \"anthropic\""));
        assert!(!toml_body.contains("api_key"));
        // Onboarding sentinel exists — next launch skips the wizard.
        assert!(dir.path().join(".onboarded").exists());
        // Inference worker rebuilt against the new config.
        assert!(state.inference_queue.lock().await.is_some());

        // Subsequent setup_status reflects the change.
        let status = do_setup_status(&state).await;
        assert_eq!(status["complete"], serde_json::Value::Bool(true));
        assert_eq!(status["provider"], "anthropic");
        assert_eq!(status["has_api_key"], serde_json::Value::Bool(true));
    }

    #[tokio::test]
    async fn submit_byok_rejects_cloud_without_key() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let body = SubmitByokBody {
            provider: "openai".to_string(),
            api_key: None,
            base_url: None,
            model: None,
        };
        let err = do_submit_byok(&state, body).await.unwrap_err();
        assert!(err.contains("requires an API key"), "got: {err}");
        // Nothing persisted.
        assert!(!dir.path().join(".onboarded").exists());
        assert!(state.secret_store.get("provider:openai").unwrap().is_none());
    }

    #[tokio::test]
    async fn submit_byok_keyless_local_provider_works() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let body = SubmitByokBody {
            provider: "lmstudio".to_string(),
            api_key: None,
            base_url: None,
            model: None,
        };
        let response = do_submit_byok(&state, body).await.unwrap();
        assert_eq!(response["provider"], "lmstudio");
        assert_eq!(response["has_api_key"], serde_json::Value::Bool(false));
        assert!(dir.path().join(".onboarded").exists());
    }

    #[tokio::test]
    async fn submit_byok_custom_requires_base_url() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let body = SubmitByokBody {
            provider: "custom".to_string(),
            api_key: Some("sk".to_string()),
            base_url: None,
            model: Some("foo".to_string()),
        };
        let err = do_submit_byok(&state, body).await.unwrap_err();
        assert!(err.to_lowercase().contains("base_url"), "got: {err}");
    }

    #[tokio::test]
    async fn submit_byok_unknown_provider_is_structured_error() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let body = SubmitByokBody {
            provider: "not-a-real-provider".to_string(),
            api_key: Some("sk".to_string()),
            base_url: None,
            model: None,
        };
        let err = do_submit_byok(&state, body).await.unwrap_err();
        assert!(err.to_lowercase().contains("provider"), "got: {err}");
    }

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
            "/api/engine-state",
            "/api/save-state",
            "/api/setup-snapshot",
            "/api/debug-snapshot",
            "/api/submit-input",
            "/api/new-game",
            "/api/save-game",
            "/api/load-branch",
            // Slim per-turn read (#1356 / #1353).
            "/api/turn",
            // Screenshot routes.
            "/api/latest-screenshot",
            "/api/take-screenshot",
            // Bug reporting.
            "/api/submit-bug-report",
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
            "get_engine_state",
            "get_save_state",
            "get_setup_snapshot",
            "get_debug_snapshot",
            "submit_input",
            "new_game",
            "save_game",
            "load_branch",
            "get_turn",
            "get_latest_screenshot",
            "take_screenshot",
            "submit_bug_report",
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

    async fn response_json(response: axum::response::Response) -> (StatusCode, serde_json::Value) {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body = serde_json::from_slice(&bytes).unwrap();
        (status, body)
    }

    #[tokio::test]
    async fn screenshot_capture_error_never_returns_a_stale_latest_path() {
        let response =
            screenshot_unavailable_response("capture window: no window".to_string(), None);
        let (status, body) = response_json(response).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert!(
            body.get("path").is_none(),
            "capture errors must never reuse a stale screenshot: {body}"
        );
    }

    #[tokio::test]
    async fn screenshot_capture_error_without_latest_is_structured_503() {
        let response =
            screenshot_unavailable_response("capture window: no window".to_string(), None);
        let (status, body) = response_json(response).await;

        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            body["error"].as_str(),
            Some("screenshot capture unavailable")
        );
        assert_eq!(body["detail"].as_str(), Some("capture window: no window"));
        assert!(
            body["hint"]
                .as_str()
                .is_some_and(|h| h.contains("readiness"))
        );
        assert!(
            body.get("path").is_none(),
            "no latest screenshot should mean no fallback path: {body}",
        );
    }

    // ── submit_input response shape tests (#1353 / #1356 / #1777) ───────────

    async fn add_canonical_exchange(
        state: &Arc<AppState>,
        player_input: &str,
        speaker_name: &str,
        npc_dialogue: &str,
    ) {
        use chrono::Utc;
        use parish_core::npc::NpcId;
        use parish_core::npc::conversation::ConversationExchange;

        let mut world = state.world.lock().await;
        let location = world.player_location;
        world.conversation_log.add(ConversationExchange {
            timestamp: Utc::now(),
            speaker_id: NpcId(1),
            speaker_name: speaker_name.to_string(),
            player_input: player_input.to_string(),
            npc_dialogue: npc_dialogue.to_string(),
            location,
        });
    }

    #[tokio::test]
    async fn submit_input_result_empty_exchanges_when_no_npc_reply() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let before = {
            let world = state.world.lock().await;
            parish_core::ipc::conversation_cursor(&world)
        };

        let result = build_submit_result(&state, before).await;

        assert!(result.exchanges.is_empty());
    }

    /// Regression for #1777: presentation transcript lines include the player
    /// as `"You"`, but the compact result contains only the canonical NPC
    /// exchange.
    #[tokio::test]
    async fn submit_input_result_excludes_player_transcript_line() {
        use parish_core::ipc::ConversationLine;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let before = {
            let world = state.world.lock().await;
            parish_core::ipc::conversation_cursor(&world)
        };

        {
            let mut conv = state.conversation.lock().await;
            conv.push_line(ConversationLine {
                speaker: "You".to_string(),
                text: "hello".to_string(),
            });
            conv.push_line(ConversationLine {
                speaker: "Mary".to_string(),
                text: "The weather is fierce today.".to_string(),
            });
        }
        add_canonical_exchange(&state, "hello", "Mary", "The weather is fierce today.").await;

        let result = build_submit_result(&state, before).await;

        assert_eq!(result.exchanges.len(), 1);
        assert_eq!(result.exchanges[0].speaker_name, "Mary");
        assert_eq!(
            result.exchanges[0].npc_dialogue,
            "The weather is fierce today."
        );
        assert_eq!(result.exchanges[0].player_input, "hello");
    }

    /// #1569: the bridge response must surface the post-guard canonical text
    /// exactly as stored. When the upstream guard preserves a valid place-history
    /// answer, `exchanges[]` must not substitute an unrelated person denial.
    #[tokio::test]
    async fn submit_input_result_preserves_known_place_history_exchange() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let before = {
            let world = state.world.lock().await;
            parish_core::ipc::conversation_cursor(&world)
        };
        let preserved = "Ah, the history of Lough Ree is a tale as grand as the lake itself.";
        add_canonical_exchange(
            &state,
            "Aoife, what is the history of Lough Ree?",
            "Aoife Brennan",
            preserved,
        )
        .await;

        let result = build_submit_result(&state, before).await;

        assert_eq!(result.exchanges.len(), 1);
        assert_eq!(result.exchanges[0].speaker_name, "Aoife Brennan");
        assert_eq!(result.exchanges[0].npc_dialogue, preserved);
        assert!(
            !result.exchanges[0]
                .npc_dialogue
                .to_lowercase()
                .contains("no such person"),
            "exchange text must not contain a person-denial substitution: {result:?}"
        );
    }

    /// Regression for #1778: changing `last_player_input` must not rewrite the
    /// player side of historical canonical exchanges.
    #[tokio::test]
    async fn turn_result_preserves_historical_player_inputs() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        add_canonical_exchange(&state, "first question", "Peig", "first answer").await;
        add_canonical_exchange(&state, "second question", "Sean", "second answer").await;
        state.conversation.lock().await.last_player_input =
            Some("examine the potato patch".to_string());

        let result = build_turn_result(&state, 0).await;

        assert_eq!(result.exchanges.len(), 2);
        assert_eq!(result.exchanges[0].player_input, "first question");
        assert_eq!(result.exchanges[1].player_input, "second question");
        assert!(
            result
                .exchanges
                .iter()
                .all(|exchange| exchange.player_input != "examine the potato patch")
        );
    }

    // ── turn_read helper tests (#1356 / #1389 / #1778) ─────────────────────

    /// Push N test events into `game_events` and increment `total_game_events`
    /// to keep the monotonic counter consistent (simulates what the background
    /// event-bus task does in production).
    async fn push_test_events(
        state: &Arc<AppState>,
        events: Vec<parish_core::world::events::GameEvent>,
    ) {
        let mut buf = state.game_events.lock().await;
        for evt in events {
            if buf.len() >= crate::DEBUG_EVENT_CAPACITY {
                buf.pop_front();
            }
            buf.push_back(evt);
            state
                .total_game_events
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// A turn read with cursor=0 returns all accumulated events.
    #[tokio::test]
    async fn read_events_since_cursor_zero_returns_all() {
        use chrono::Utc;
        use parish_core::npc::NpcId;
        use parish_core::world::LocationId;
        use parish_core::world::events::GameEvent;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        push_test_events(
            &state,
            vec![
                GameEvent::NpcArrived {
                    npc_id: NpcId(1),
                    location: LocationId(1),
                    timestamp: Utc::now(),
                },
                GameEvent::WeatherChanged {
                    new_weather: "Rain".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        )
        .await;

        let result = build_turn_result(&state, 0).await;
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.event_cursor, 2);
        assert_eq!(result.events[0].kind, "NpcArrived");
        assert_eq!(result.events[1].kind, "WeatherChanged");
    }

    /// `read_events_since` with `since=total` returns nothing new.
    #[tokio::test]
    async fn read_events_since_cursor_at_end_returns_empty() {
        use chrono::Utc;
        use parish_core::world::events::GameEvent;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        push_test_events(
            &state,
            vec![GameEvent::WeatherChanged {
                new_weather: "Sun".to_string(),
                timestamp: Utc::now(),
            }],
        )
        .await;

        // Simulate the agent already saw that event.
        let first = build_turn_result(&state, 0).await;
        let second = build_turn_result(&state, first.event_cursor).await;
        assert!(second.events.is_empty(), "no events since cursor");
        assert_eq!(
            second.event_cursor, first.event_cursor,
            "cursor is stable when nothing new"
        );
    }

    /// Regression test for #1389: two sequential reads with advancing `since`
    /// must return DIFFERENT, forward-moving event windows.
    ///
    /// Push 3 events, read (cursor=3), push 2 more, read with since=3 →
    /// must get exactly the 2 new events, not the original 3.
    #[tokio::test]
    async fn read_events_since_returns_only_new_events_after_cursor() {
        use chrono::Utc;
        use parish_core::world::events::GameEvent;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        // Push 3 initial events.
        push_test_events(
            &state,
            vec![
                GameEvent::WeatherChanged {
                    new_weather: "Clear".to_string(),
                    timestamp: Utc::now(),
                },
                GameEvent::WeatherChanged {
                    new_weather: "Mist".to_string(),
                    timestamp: Utc::now(),
                },
                GameEvent::WeatherChanged {
                    new_weather: "Rain".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        )
        .await;

        // First read: caller sees all 3, gets cursor=3.
        let first = build_turn_result(&state, 0).await;
        assert_eq!(first.events.len(), 3);
        assert_eq!(first.event_cursor, 3);

        // Push 2 new events after the first read.
        push_test_events(
            &state,
            vec![
                GameEvent::WeatherChanged {
                    new_weather: "Snow".to_string(),
                    timestamp: Utc::now(),
                },
                GameEvent::WeatherChanged {
                    new_weather: "Hail".to_string(),
                    timestamp: Utc::now(),
                },
            ],
        )
        .await;

        // Second read with since=3 must return ONLY the 2 new events.
        let second = build_turn_result(&state, first.event_cursor).await;
        assert_eq!(second.events.len(), 2);
        assert_eq!(second.events[0].summary, "Weather → Snow");
        assert_eq!(second.events[1].summary, "Weather → Hail");
        assert_eq!(second.event_cursor, 5);
    }

    /// `TurnReadResult` shape: exchanges + events + core state fields present.
    #[tokio::test]
    async fn turn_read_result_shape_is_correct() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        add_canonical_exchange(&state, "evening", "Brigid", "Ah, grand so.").await;
        let result = build_turn_result(&state, 0).await;

        // Validate the fields that must always be present.
        assert!(!result.location.is_empty());
        let _ = result.npcs_here;
        let _ = result.clock.hour;
        let _ = result.clock.minute;
        assert!(!result.clock.time_label.is_empty());
        assert_eq!(result.event_cursor, 0);
        assert!(result.events.is_empty());
        assert_eq!(result.exchanges[0].player_input, "evening");
    }
}
