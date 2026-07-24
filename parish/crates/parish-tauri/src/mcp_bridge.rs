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
use chrono::Timelike;
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

// ── Shared response types ────────────────────────────────────────────────────

/// A single conversation exchange (player + NPC) returned by
/// `submit_input` and `GET /api/turn` (#1356 / #1353).
#[derive(Debug, Clone, Serialize)]
pub(crate) struct TurnExchange {
    /// What the player said.
    pub player_input: String,
    /// What the NPC replied.
    pub npc_dialogue: String,
    /// Display name of the NPC who replied.
    pub speaker_name: String,
    /// Location name where the exchange happened.
    pub location: String,
}

/// Compact game-clock snapshot included in per-turn responses.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct TurnClock {
    /// Current game hour (0–23).
    pub hour: u8,
    /// Current game minute (0–59).
    pub minute: u8,
    /// Human-readable time label (e.g. "Morning").
    pub time_label: String,
}

/// The compact response body returned by `POST /api/submit-input` (#1353).
///
/// Carries only the delta produced by this turn so the MCP agent never needs
/// a second round-trip to read the NPC reply.
#[derive(Debug, Serialize)]
pub(crate) struct SubmitInputResult {
    /// New conversation exchange(s) added by this turn. Empty if the turn
    /// produced no NPC dialogue (movement, look, system command, etc.).
    pub exchanges: Vec<TurnExchange>,
    /// Clock state after the turn completes.
    pub clock: TurnClock,
    /// Player location name after the turn.
    pub location: String,
    /// Number of NPCs at the player's location after the turn.
    pub npcs_here: usize,
}

/// A summarised world event for `GET /api/turn` (#1356).
#[derive(Debug, Serialize)]
pub(crate) struct TurnEvent {
    /// Event discriminant (e.g. "NpcArrived", "WeatherChanged").
    pub kind: String,
    /// Human-readable summary of the event.
    pub summary: String,
}

/// The response body returned by `GET /api/turn` (#1356).
///
/// Bounded to at most `TURN_MAX_EXCHANGES` exchanges + `TURN_MAX_EVENTS`
/// events so the token cost is constant regardless of session length.
#[derive(Debug, Serialize)]
pub(crate) struct TurnReadResult {
    /// Recent conversation exchanges (newest last, capped).
    pub exchanges: Vec<TurnExchange>,
    /// Recent world events since `since_cursor`, capped at `TURN_MAX_EVENTS`.
    pub events: Vec<TurnEvent>,
    /// Current clock state.
    pub clock: TurnClock,
    /// Current player location name.
    pub location: String,
    /// Number of NPCs at the player's location.
    pub npcs_here: usize,
    /// Monotonic cursor for the next `?since=` call. Equals the total number
    /// of game events accumulated in `AppState::game_events` at the time this
    /// response was built. Pass this value as `?since=<cursor>` on the next
    /// call to receive only events that arrived after this snapshot.
    pub event_cursor: usize,
}

/// Maximum conversation exchanges returned by `GET /api/turn`.
const TURN_MAX_EXCHANGES: usize = 10;
/// Maximum world events returned by `GET /api/turn` (per call).
const TURN_MAX_EVENTS: usize = 20;

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

/// Builds a [`TurnClock`] from the live world state without holding the lock.
async fn read_turn_clock(state: &Arc<AppState>) -> TurnClock {
    let world = state.world.lock().await;
    let now = world.clock.now();
    TurnClock {
        hour: now.hour() as u8,
        minute: now.minute() as u8,
        time_label: world.clock.time_of_day().to_string(),
    }
}

/// Reads the current location name and NPC-here count from the live world.
async fn read_location_and_npcs(state: &Arc<AppState>) -> (String, usize) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let location_name = world.current_location().name.clone();
    let npcs_here = npc_manager.npcs_at(world.player_location).len();
    (location_name, npcs_here)
}

/// Converts the `game_events` ring buffer to summarised [`TurnEvent`]s.
///
/// `since_cursor` is the monotonic lifetime event count returned by a
/// previous call (i.e. `AppState::total_game_events` at that point in time).
/// Returns only events enqueued AFTER `since_cursor`, capped at
/// [`TURN_MAX_EVENTS`]. Also returns the new cursor value (current
/// `total_game_events`) so the caller can page forward (#1389).
async fn read_events_since(state: &Arc<AppState>, since_cursor: usize) -> (Vec<TurnEvent>, usize) {
    // Read the monotonic total BEFORE locking the ring so the cursor
    // reflects all events that existed at query time, even if a concurrent
    // push races this read (it would merely appear on the next call).
    let total = state
        .total_game_events
        .load(std::sync::atomic::Ordering::Relaxed);
    let events = state.game_events.lock().await;
    let ring_len = events.len();
    // `total - ring_len` is the number of events evicted from the front of
    // the ring. The first entry in the deque has lifetime index `evicted`.
    // We want events with lifetime index >= since_cursor, so we skip
    // `since_cursor - evicted` entries from the front (clamped to 0).
    let evicted = total.saturating_sub(ring_len);
    let skip = since_cursor.saturating_sub(evicted);
    let new_entries: Vec<TurnEvent> = events
        .iter()
        .skip(skip)
        .take(TURN_MAX_EVENTS)
        .map(|e| TurnEvent {
            kind: e.event_type().to_string(),
            summary: summarise_game_event(e),
        })
        .collect();
    (new_entries, total)
}

/// Builds a one-line human-readable summary for a [`GameEvent`].
fn summarise_game_event(event: &parish_core::world::events::GameEvent) -> String {
    use parish_core::world::events::GameEvent;
    match event {
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
        GameEvent::LifeEvent {
            npc_id,
            description,
            ..
        } => {
            format!("NPC #{}: {description}", npc_id.0)
        }
        GameEvent::NpcInteraction { summary, .. } => summary.clone(),
    }
}

/// Builds the `exchanges` delta from the conversation transcript.
///
/// Reads `transcript` (length up to 12) and returns only the entries that
/// were added after `len_before`. The location field comes from
/// `ConversationRuntimeState::location` (converted via the world graph).
async fn read_transcript_delta(
    state: &Arc<AppState>,
    len_before: usize,
    player_input: &str,
) -> Vec<TurnExchange> {
    let conv = state.conversation.lock().await;
    let world = state.world.lock().await;
    let transcript = &conv.transcript;
    let len_after = transcript.len();
    if len_after <= len_before {
        return Vec::new();
    }
    // The transcript is a ring buffer capped at 12. Determine the location
    // name from the world at current player position.
    let location_name = world.current_location().name.clone();
    // Entries that appear after len_before in the ring buffer. Because the
    // ring buffer may have wrapped, we take from the tail.
    let new_count = len_after - len_before;
    transcript
        .iter()
        .rev()
        .take(new_count)
        .rev()
        .filter(|l| l.speaker != "player")
        .map(|l| TurnExchange {
            player_input: player_input.to_string(),
            npc_dialogue: l.text.clone(),
            speaker_name: l.speaker.clone(),
            location: location_name.clone(),
        })
        .collect()
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

/// `POST /api/submit-input` — bridge handler.
///
/// Keeps `do_submit_input` signature unchanged (the Tauri UI command still
/// returns `Result<(), String>`). The bridge wrapper reads the transcript
/// length before and after the call to extract the delta, then assembles a
/// compact JSON body so the MCP agent never needs a second round-trip to
/// read the NPC reply (#1353 / #1356).
async fn submit_input(
    State(b): State<BridgeState>,
    Json(body): Json<SubmitInputBody>,
) -> Result<Json<SubmitInputResult>, AppError> {
    let text = body.text.clone();

    // Snapshot the transcript length before the turn so we can diff after.
    let len_before = {
        let conv = b.state.conversation.lock().await;
        conv.transcript.len()
    };

    crate::commands::do_submit_input(&b.state, &b.app, body.text, body.addressed_to)
        .await
        .map_err(AppError::from)?;

    let exchanges = read_transcript_delta(&b.state, len_before, &text).await;
    let clock = read_turn_clock(&b.state).await;
    let (location, npcs_here) = read_location_and_npcs(&b.state).await;

    Ok(Json(SubmitInputResult {
        exchanges,
        clock,
        location,
        npcs_here,
    }))
}

/// `GET /api/turn?since=<cursor>` — slim per-turn read (#1356).
///
/// Returns last [`TURN_MAX_EXCHANGES`] exchanges + up to [`TURN_MAX_EVENTS`]
/// world events since `since` cursor + core state. Bounded size; does not
/// require `get_debug_snapshot`.
async fn turn_read(
    State(b): State<BridgeState>,
    Query(params): Query<TurnReadParams>,
) -> Result<Json<TurnReadResult>, AppError> {
    let since_cursor = params.since.unwrap_or(0);

    let (events, event_cursor) = read_events_since(&b.state, since_cursor).await;
    let clock = read_turn_clock(&b.state).await;
    let (location, npcs_here) = read_location_and_npcs(&b.state).await;

    // Read last N exchanges from the conversation transcript.
    let exchanges: Vec<TurnExchange> = {
        let conv = b.state.conversation.lock().await;
        let world = b.state.world.lock().await;
        let location_name = world.current_location().name.clone();
        // Pair transcript lines: player line followed by NPC line(s). Walk in
        // reverse pairs to find the exchanges. The transcript stores speaker +
        // text lines; we look for NPC (non-"player") lines and pair with the
        // most recent player input from ConversationRuntimeState.
        let last_player = conv.last_player_input.clone().unwrap_or_default();
        let entries: Vec<TurnExchange> = conv
            .transcript
            .iter()
            .rev()
            .take(TURN_MAX_EXCHANGES * 2)
            .filter(|l| l.speaker != "player")
            .map(|l| TurnExchange {
                player_input: last_player.clone(),
                npc_dialogue: l.text.clone(),
                speaker_name: l.speaker.clone(),
                location: location_name.clone(),
            })
            .take(TURN_MAX_EXCHANGES)
            .collect();
        // Re-reverse so newest is last.
        entries.into_iter().rev().collect()
    };

    Ok(Json(TurnReadResult {
        exchanges,
        events,
        clock,
        location,
        npcs_here,
        event_cursor,
    }))
}

#[derive(Debug, Deserialize)]
struct TurnReadParams {
    since: Option<usize>,
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

    // ── submit_input response shape tests (#1353 / #1356) ───────────────────

    /// `read_transcript_delta` returns empty when no new entries were added.
    #[tokio::test]
    async fn submit_input_result_empty_exchanges_when_no_npc_reply() {
        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        // Transcript starts empty; delta from len_before=0 to len_after=0 is
        // still empty.
        let delta = read_transcript_delta(&state, 0, "look").await;
        assert!(
            delta.is_empty(),
            "expected no exchanges when transcript is unchanged"
        );
    }

    /// `read_transcript_delta` returns only entries added after len_before.
    #[tokio::test]
    async fn submit_input_result_carries_new_exchange() {
        use parish_core::ipc::ConversationLine;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        // Pre-populate transcript with one line to simulate a prior exchange.
        {
            let mut conv = state.conversation.lock().await;
            conv.push_line(ConversationLine {
                speaker: "Seán".to_string(),
                text: "Good evening to ye.".to_string(),
            });
        }
        let len_before = {
            let conv = state.conversation.lock().await;
            conv.transcript.len()
        };
        // Simulate a new NPC reply appended during this turn.
        {
            let mut conv = state.conversation.lock().await;
            conv.push_line(ConversationLine {
                speaker: "Mary".to_string(),
                text: "The weather is fierce today.".to_string(),
            });
        }

        let delta = read_transcript_delta(&state, len_before, "hello").await;
        assert_eq!(delta.len(), 1, "expected exactly one new exchange");
        assert_eq!(delta[0].speaker_name, "Mary");
        assert_eq!(delta[0].npc_dialogue, "The weather is fierce today.");
        assert_eq!(delta[0].player_input, "hello");
    }

    /// #1569: the bridge response must surface the post-guard transcript text
    /// exactly as stored. When the upstream guard preserves a valid place-history
    /// answer, `exchanges[]` must not substitute an unrelated person denial.
    #[tokio::test]
    async fn submit_input_result_preserves_known_place_history_exchange() {
        use parish_core::ipc::ConversationLine;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);
        let len_before = {
            let conv = state.conversation.lock().await;
            conv.transcript.len()
        };
        let preserved = "Ah, the history of Lough Ree is a tale as grand as the lake itself.";
        {
            let mut conv = state.conversation.lock().await;
            conv.push_line(ConversationLine {
                speaker: "Aoife Brennan".to_string(),
                text: preserved.to_string(),
            });
        }

        let delta = read_transcript_delta(
            &state,
            len_before,
            "Aoife, what is the history of Lough Ree?",
        )
        .await;

        assert_eq!(delta.len(), 1, "expected exactly one exchange");
        assert_eq!(delta[0].speaker_name, "Aoife Brennan");
        assert_eq!(delta[0].npc_dialogue, preserved);
        assert!(
            !delta[0]
                .npc_dialogue
                .to_lowercase()
                .contains("no such person"),
            "exchange text must not contain a person-denial substitution: {delta:?}"
        );
    }

    // ── turn_read helper tests (#1356 / #1389) ───────────────────────────────

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

    /// `read_events_since` with cursor=0 returns all accumulated events.
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

        let (events, cursor) = read_events_since(&state, 0).await;
        assert_eq!(events.len(), 2);
        assert_eq!(cursor, 2);
        assert_eq!(events[0].kind, "NpcArrived");
        assert_eq!(events[1].kind, "WeatherChanged");
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
        let (_, cursor) = read_events_since(&state, 0).await;
        let (new_events, new_cursor) = read_events_since(&state, cursor).await;
        assert!(new_events.is_empty(), "no events since cursor");
        assert_eq!(new_cursor, cursor, "cursor is stable when nothing new");
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
        let (first_events, cursor_after_first) = read_events_since(&state, 0).await;
        assert_eq!(
            first_events.len(),
            3,
            "initial read should return all 3 events"
        );
        assert_eq!(cursor_after_first, 3);

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
        let (second_events, cursor_after_second) =
            read_events_since(&state, cursor_after_first).await;
        assert_eq!(
            second_events.len(),
            2,
            "since=3 must return only the 2 new events, not the original 3"
        );
        assert_eq!(second_events[0].summary, "Weather → Snow");
        assert_eq!(second_events[1].summary, "Weather → Hail");
        assert_eq!(cursor_after_second, 5, "cursor must advance to total=5");
    }

    /// `TurnReadResult` shape: exchanges + events + core state fields present.
    #[tokio::test]
    async fn turn_read_result_shape_is_correct() {
        use parish_core::ipc::ConversationLine;

        let dir = TempDir::new().unwrap();
        let state = byok_test_state(&dir);

        // Add an NPC line to the transcript.
        {
            let mut conv = state.conversation.lock().await;
            conv.last_player_input = Some("evening".to_string());
            conv.push_line(ConversationLine {
                speaker: "Brigid".to_string(),
                text: "Ah, grand so.".to_string(),
            });
        }

        let clock = read_turn_clock(&state).await;
        let (location, npcs_here) = read_location_and_npcs(&state).await;
        let (events, event_cursor) = read_events_since(&state, 0).await;

        // Validate the fields that must always be present.
        assert!(!location.is_empty(), "location must be non-empty");
        let _ = npcs_here; // usize, always valid
        let _ = clock.hour;
        let _ = clock.minute;
        assert!(!clock.time_label.is_empty());
        assert!(event_cursor == 0, "no events added");
        assert!(events.is_empty());
    }
}
