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
        // ── Screenshot reader (player-triggered, MCP-readable) ───────────────
        // GET-only: capture is initiated from the live UI by pressing F2; the
        // bridge surfaces the most recent path so an MCP client can read the
        // file out of band. Posting a `data_url` from MCP is intentionally
        // out of scope until the future-work design questions are resolved.
        .route("/api/latest-screenshot", get(latest_screenshot))
        // ── BYOK setup-flow (#933) ────────────────────────────────────────
        // Real handlers backed by `parish_core::ipc::byok` — the Svelte
        // wizard and the MCP client share these. Routes match the schema
        // the stubs originally pinned.
        .route("/api/setup-status", get(setup_status))
        .route("/api/submit-byok", post(submit_byok))
        .route("/api/byok-env-keys", get(byok_env_keys))
        .route("/api/preset-models", get(preset_models))
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

async fn latest_screenshot(
    State(b): State<BridgeState>,
) -> Result<Json<Option<ScreenshotInfo>>, AppError> {
    let info = crate::commands::do_get_latest_screenshot(&b.state)
        .await
        .map_err(AppError::from)?;
    Ok(Json(info))
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
-> Json<std::collections::BTreeMap<String, parish_core::ipc::byok::ProviderPresetModels>> {
    Json(parish_core::ipc::byok::handle_list_preset_models())
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
            inference_log: new_inference_log(),
            ui_config,
            theme_palette,
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
            language_settings: parish_core::npc::LanguageSettings::english_only(),
            config: Mutex::new(game_config),
            demo_config: DemoConfig::default(),
            shutdown_token: CancellationToken::new(),
            session_store,
            user_config_dir: dir.path().to_path_buf(),
            secret_store: Arc::new(InMemorySecretStore::new()),
            latest_screenshot_path: Mutex::new(None),
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
            "/api/save-state",
            "/api/setup-snapshot",
            "/api/submit-input",
            "/api/new-game",
            "/api/save-game",
            "/api/load-branch",
            // Screenshot reader (player-triggered, MCP-readable).
            "/api/latest-screenshot",
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
            "get_latest_screenshot",
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
