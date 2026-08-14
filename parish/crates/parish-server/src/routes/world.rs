//! World-query and setup endpoints.
//!
//! Covers:
//! - `GET /api/world-snapshot`
//! - `GET /api/setup-snapshot`
//! - `GET /api/map`
//! - `GET /api/npcs-here`
//! - `GET /api/available-providers`
//! - `GET /api/theme`
//! - `GET /api/ui-config`
//! - `GET /api/app-icon.png` / `GET /api/favicon.png`
//! - `GET /api/debug-snapshot`
//! - `POST /api/submit-bug-report`
//! - `GET /api/health`
//! - [`redact_call_log`] — shared redaction helper

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Extension, Path as AxumPath, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

use parish_core::debug_snapshot::{self, AuthDebug, InferenceDebug};
use parish_core::ipc::{MapData, NpcInfo, ReconnectState, ThemePalette, WorldSnapshot};

use crate::middleware::SessionId;
use crate::session::GlobalState;
use crate::state::{AppState, SetupStatusSnapshot};

use super::admin::{admin_emails, check_admin};

// ── Query endpoints ─────────────────────────────────────────────────────────

/// `GET /api/world-snapshot` — returns the current world snapshot.
pub async fn get_world_snapshot(Extension(state): Extension<Arc<AppState>>) -> Json<WorldSnapshot> {
    let mut snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let mut snapshot = parish_core::ipc::snapshot_from_world(&world);
        snapshot.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        snapshot
    };
    // Surface whether an NPC turn is in flight so the web frontend can
    // re-assert `streamingActive` from authoritative state after a WebSocket
    // reconnect, instead of guessing and re-enabling input mid-turn (#1164).
    // Acquire the conversation lock only after the world/npc_manager locks are
    // released so this hot, reconnect-path endpoint never holds three locks at
    // once.
    snapshot.turn_in_flight = state.conversation.lock().await.conversation_in_progress;
    Json(snapshot)
}

/// `GET /api/reconnect-state` — one source-consistent replacement payload.
pub async fn get_reconnect_state(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<ReconnectState> {
    let _persistence_guard = state.persistence_gate.lock().await;
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let conversation = state.conversation.lock().await;
    let config = state.config.lock().await;
    Json(parish_core::ipc::build_reconnect_state(
        &world,
        &npc_manager,
        state.transport.default_mode(),
        config.reveal_unexplored_locations,
        &state.pronunciations,
        conversation.conversation_in_progress,
    ))
}

/// `GET /api/setup-snapshot` — returns the current setup status.
/// Always returns `done: true` for the web server (no Ollama bootstrap).
pub async fn get_setup_snapshot(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<SetupStatusSnapshot> {
    let status = state
        .setup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    Json(status.clone())
}

/// `GET /api/map` — returns visited locations, edges, and player position.
pub async fn get_map(Extension(state): Extension<Arc<AppState>>) -> Json<MapData> {
    let world = state.world.lock().await;
    let config = state.config.lock().await;
    let transport = state.transport.default_mode();
    Json(parish_core::ipc::build_map_data(
        &world,
        transport,
        config.reveal_unexplored_locations,
    ))
}

/// `GET /api/npcs-here` — returns NPCs at the player's current location.
pub async fn get_npcs_here(Extension(state): Extension<Arc<AppState>>) -> Json<Vec<NpcInfo>> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Json(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

/// `GET /api/engine-state` — returns the canonical deterministic Parish engine
/// state for the MCP automated-QA loop (#1331). Backs the `parish_engine_state`
/// MCP tool. Read-only; gated behind the default-on `engine-state` kill switch
/// (rule #6) so a misbehaving snapshot can be disabled without a redeploy.
pub async fn get_engine_state(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<parish_core::ipc::EngineState>, (StatusCode, String)> {
    if state.config.lock().await.flags.is_disabled("engine-state") {
        return Err((
            StatusCode::FORBIDDEN,
            "the engine-state feature is disabled".to_string(),
        ));
    }
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Ok(Json(parish_core::ipc::build_engine_state(
        &world,
        &npc_manager,
    )))
}

/// `GET /api/available-providers` — featured + other LLM provider lists,
/// sourced from the runtime-loaded `ProviderRegistry` (builtins +
/// `mods/<id>/providers/`). The same payload Tauri exposes via
/// `list_available_providers`; the web UI consumes it for its picker.
#[allow(clippy::unused_async)]
pub async fn get_available_providers()
-> Json<std::collections::HashMap<&'static str, Vec<parish_core::ipc::byok::ProviderInfo>>> {
    Json(parish_core::ipc::byok::handle_list_available_providers())
}

/// `GET /api/theme` — returns the current palette.
///
/// Resolution order:
/// 1. Mod-provided time-of-day keyframes → interpolated for the current game hour.
/// 2. Mod-provided static `[theme.palette]` (no keyframes) → returned as-is.
/// 3. No mod loaded → `neutral_grey_palette()` so the prompt overlay renders.
pub async fn get_theme(Extension(state): Extension<Arc<AppState>>) -> Json<ThemePalette> {
    use chrono::Timelike;
    use parish_core::config::PaletteConfig;
    use parish_palette::{compute_palette_with_keyframes, neutral_grey_palette};
    let raw = if !state.theme_keyframes.is_empty() {
        let world = state.world.lock().await;
        let now = world.clock.now();
        compute_palette_with_keyframes(
            now.hour(),
            now.minute(),
            &state.theme_keyframes,
            &PaletteConfig::default(),
        )
    } else if let Some(p) = state.static_raw_palette {
        p
    } else {
        neutral_grey_palette()
    };
    Json(ThemePalette::from(raw))
}

/// `GET /api/ui-config` — returns UI configuration (splash text, labels, accent).
pub async fn get_ui_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<crate::state::UiConfigSnapshot> {
    Json(state.ui_config.clone())
}

/// `GET /api/app-icon.png` — serves the active mod's browser icon override.
pub async fn get_app_icon(Extension(state): Extension<Arc<AppState>>) -> impl IntoResponse {
    serve_mod_icon(state.game_mod.as_ref().and_then(|gm| gm.app_icon_path())).await
}

/// `GET /api/favicon.png` — serves the active mod's small browser favicon.
pub async fn get_favicon(Extension(state): Extension<Arc<AppState>>) -> impl IntoResponse {
    serve_mod_icon(state.game_mod.as_ref().and_then(|gm| gm.favicon_path())).await
}

pub async fn serve_mod_icon(path: Option<PathBuf>) -> axum::response::Response {
    let Some(path) = path else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match tokio::fs::read(&path).await {
        // Mod branding is constrained to local assets by parish-core, but the
        // asset format itself is mod-owned. Preserve the authored MIME type.
        Ok(bytes) => (
            [
                (
                    CONTENT_TYPE,
                    mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .to_string(),
                ),
                (CACHE_CONTROL, "public, max-age=86400".to_string()),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "failed to read mod app icon");
            StatusCode::NOT_FOUND.into_response()
        }
    }
}

/// Redact an inference call log for web clients (#333).
///
/// Strips `prompt_text`, `response_text`, and `system_prompt` from every entry
/// so that one user's LLM prompts are never exposed to other authenticated
/// visitors.  `prompt_len` / `response_len` and all other metadata are kept so
/// the debug panel remains informative.
///
/// Called by both [`get_debug_snapshot`] (production path) and the
/// `debug_snapshot_call_log_has_prompt_len_not_prompt_text` integration test, so
/// the test exercises the real redaction rather than a hand-rolled copy.
pub fn redact_call_log(
    entries: &[parish_core::debug_snapshot::InferenceLogEntry],
) -> Vec<parish_core::debug_snapshot::InferenceLogEntry> {
    entries
        .iter()
        .map(|e| parish_core::debug_snapshot::InferenceLogEntry {
            // Redacted fields:
            system_prompt: None,
            prompt_text: String::new(),
            response_text: String::new(),
            ..e.clone()
        })
        .collect()
}

/// `GET /api/debug-snapshot` — returns debug state for the debug panel.
///
/// **Admin-only** (#753): gated by `PARISH_ADMIN_EMAILS` via the same
/// [`check_admin`] guard used for provider/key commands.  Non-admin
/// authenticated users receive 403; unauthenticated callers are rejected
/// upstream by `cf_access_guard` with 401.
///
/// The DebugPanel in the UI is an admin-only feature accessed via F12 dev
/// tooling; the endpoint gate makes that intent explicit and enforced.
///
/// The inference call log is **redacted** for web clients (#333): `prompt_text`,
/// `response_text`, `system_prompt`, and `base_url` are stripped so that one
/// user's LLM prompts are never exposed to other authenticated visitors.
pub async fn get_debug_snapshot(
    Extension(state): Extension<Arc<AppState>>,
    Extension(session_id): Extension<SessionId>,
    Extension(cf_auth): Extension<crate::cf_auth::AuthContext>,
    State(global): State<Arc<GlobalState>>,
) -> Result<Json<debug_snapshot::DebugSnapshot>, StatusCode> {
    // #753 — admin gate: only PARISH_ADMIN_EMAILS members may read the snapshot.
    check_admin(&cf_auth.email, "debug-snapshot", admin_emails())?;

    // Snapshot each piece of state with a brief, non-overlapping lock window.
    // This avoids holding all 5+ locks simultaneously (#105, #282), which
    // caused latency spikes on all concurrent game operations and created
    // a latent deadlock risk if lock ordering ever drifted.
    //
    // Lock order respected throughout — see `LOCK_ORDER` in `state.rs`
    // (config precedes the inference group; #483).

    // 1. Peek inference_queue presence (released temporary — the guard does
    //    not outlive this statement, so it holds no slot in the order check).
    let has_inference_queue = state.inference.inference_queue.lock().await.is_some();

    // 2. Clone the fields we need from config — drop the lock immediately.
    let (
        provider_name,
        model_name,
        base_url,
        cloud_provider,
        cloud_model,
        improv_enabled,
        categories,
    ) = {
        let config = state.config.lock().await;
        (
            config.provider_name.clone(),
            config.model_name.clone(),
            config.base_url.clone(),
            config.cloud_provider_name.clone(),
            config.cloud_model_name.clone(),
            config.improv_enabled,
            parish_core::debug_snapshot::build_inference_categories(&*config),
        )
    };

    // 3. Clone debug_events ring buffer — drop the lock immediately.
    let events_snapshot: std::collections::VecDeque<parish_core::debug_snapshot::DebugEvent> =
        state.debug_events.lock().await.iter().cloned().collect();

    // 4. Clone game_events ring buffer — drop the lock immediately.
    let game_events_snapshot: std::collections::VecDeque<parish_core::world::events::GameEvent> =
        state.game_events.lock().await.iter().cloned().collect();

    // 5. Clone inference log — drop the lock immediately.
    let raw_call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
        state.inference_log.lock().await.iter().cloned().collect();

    // Build a full inference debug block from the cloned data (no locks held).
    let inference = InferenceDebug {
        provider_name,
        model_name,
        base_url,
        cloud_provider,
        cloud_model,
        has_queue: has_inference_queue,
        reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
        improv_enabled,
        call_log: raw_call_log.clone(),
        categories,
        configured_providers: parish_core::debug_snapshot::build_configured_providers(),
        tier2_parse_failures_total: parish_core::npc::ticks::tier2_parse_failures_total(),
    };
    let linked = global.identity_store.get_account(&session_id.0);
    let auth = AuthDebug {
        oauth_enabled: global.oauth_config.is_some(),
        logged_in: linked.is_some(),
        provider: linked.as_ref().map(|_| "google".to_string()),
        display_name: linked.map(|(_sub, name)| name),
        session_id: Some(session_id.0.clone()),
    };

    // 6. Acquire world and npc_manager (in canonical order) only for the
    // duration of the pure-read snapshot build, then release immediately.
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    // Build the full snapshot then redact the inference section (#333).
    let mut snapshot = debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events_snapshot,
        &game_events_snapshot,
        &inference,
        &auth,
    );
    drop(npc_manager);
    drop(world);

    // Replace call_log entries with redacted forms (no prompt/response text,
    // no system_prompt, no base_url).
    snapshot.inference.call_log = redact_call_log(&raw_call_log);
    // Also redact base_url from the inference config block.
    snapshot.inference.base_url = String::new();

    Ok(Json(snapshot))
}

// ── Bug reporting ─────────────────────────────────────────────────────────────

/// Builds a full (un-redacted) debug snapshot for embedding in a bug report.
///
/// Unlike [`get_debug_snapshot`], this is not session-scoped or redacted: the
/// reporter is filing their own session for diagnosis, so the prompts/logs are
/// theirs to share. Uses [`AuthDebug::disabled`] since auth detail is not
/// useful in a bug report.
pub async fn build_full_debug_snapshot(state: &Arc<AppState>) -> debug_snapshot::DebugSnapshot {
    let has_inference_queue = state.inference.inference_queue.lock().await.is_some();
    let (
        provider_name,
        model_name,
        base_url,
        cloud_provider,
        cloud_model,
        improv_enabled,
        categories,
    ) = {
        let config = state.config.lock().await;
        (
            config.provider_name.clone(),
            config.model_name.clone(),
            config.base_url.clone(),
            config.cloud_provider_name.clone(),
            config.cloud_model_name.clone(),
            config.improv_enabled,
            parish_core::debug_snapshot::build_inference_categories(&*config),
        )
    };
    let events_snapshot: std::collections::VecDeque<parish_core::debug_snapshot::DebugEvent> =
        state.debug_events.lock().await.iter().cloned().collect();
    let game_events_snapshot: std::collections::VecDeque<parish_core::world::events::GameEvent> =
        state.game_events.lock().await.iter().cloned().collect();
    let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
        state.inference_log.lock().await.iter().cloned().collect();

    let inference = InferenceDebug {
        provider_name,
        model_name,
        base_url,
        cloud_provider,
        cloud_model,
        has_queue: has_inference_queue,
        reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
        improv_enabled,
        call_log,
        categories,
        configured_providers: parish_core::debug_snapshot::build_configured_providers(),
        tier2_parse_failures_total: parish_core::npc::ticks::tier2_parse_failures_total(),
    };

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events_snapshot,
        &game_events_snapshot,
        &inference,
        &AuthDebug::disabled(),
    )
}

/// `POST /api/submit-bug-report` — bundle screenshot + logs + game state into a
/// GitHub issue (or an on-disk bundle in dry-run / no-token mode).
///
/// Shares the orchestration in `parish_core::ipc::bug_report` with the Tauri
/// command and the MCP bridge (rule #12). The browser supplies the screenshot
/// as a data URL; there is no server-side capture round-trip.
pub async fn submit_bug_report(
    Extension(state): Extension<Arc<AppState>>,
    Json(request): Json<parish_core::ipc::BugReportRequest>,
) -> Result<Json<parish_core::ipc::BugReportResult>, (StatusCode, String)> {
    use parish_core::ipc::bug_report;

    if state.config.lock().await.flags.is_disabled("bug-report") {
        return Err((
            StatusCode::FORBIDDEN,
            bug_report::BugReportError::Disabled.to_string(),
        ));
    }

    let world_snapshot = {
        let world = state.world.lock().await;
        parish_core::ipc::snapshot_from_world(&world)
    };
    let debug = build_full_debug_snapshot(&state).await;
    let save_summary = {
        let branch_id = *state.save_identity.current_branch_id.lock().await;
        let branch_name = state.save_identity.current_branch_name.lock().await.clone();
        match (branch_id, branch_name) {
            (Some(id), Some(name)) => Some(format!("branch {id}: {name}")),
            (Some(id), None) => Some(format!("branch {id}")),
            (None, Some(name)) => Some(name),
            (None, None) => None,
        }
    };
    // Capture the canonical engine-state snapshot + last raw player intent so
    // the bug report carries the full "black box" context stack (#1331).
    let engine_state_json = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let engine_state = parish_core::ipc::build_engine_state(&world, &npc_manager);
        serde_json::to_value(&engine_state).unwrap_or(serde_json::Value::Null)
    };
    let last_user_intent = state.conversation.lock().await.last_player_input.clone();

    let report_state =
        bug_report::BugReportState::from_snapshots(&world_snapshot, &debug, save_summary)
            .with_diagnostic(engine_state_json, last_user_intent);

    let screenshot_png: Option<Vec<u8>> = match &request.screenshot_data_url {
        Some(data_url) => Some(
            bug_report::decode_data_url(data_url)
                .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        ),
        None => None,
    };

    let cfg = bug_report::GitHubBugConfig::from_env_async().await;
    let bundle_root = state.saves_dir.join("bug-reports");
    let http = reqwest::Client::new();
    let result = bug_report::create_bug_report(
        &http,
        &cfg,
        &request,
        &report_state,
        screenshot_png.as_deref(),
        &bundle_root,
    )
    .await
    .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Json(result))
}

// ── #373 — Health check (CF-Access exempt) ──────────────────────────────────

/// `GET /api/health` — lightweight liveness probe; no auth required.
pub async fn get_health() -> StatusCode {
    StatusCode::OK
}

const PLAYWRIGHT_BUILD_ID_HEADER: HeaderName =
    HeaderName::from_static("x-parish-playwright-build-id");

pub(crate) fn playwright_readiness_status(
    requested_run_id: &str,
    expected_run_id: Option<&str>,
    compiled_build_id: Option<&str>,
    expected_build_id: Option<&str>,
    marker: Option<&str>,
) -> (StatusCode, bool) {
    let (Some(expected_run_id), Some(compiled_build_id), Some(expected_build_id)) =
        (expected_run_id, compiled_build_id, expected_build_id)
    else {
        return (StatusCode::NOT_FOUND, false);
    };
    if requested_run_id != expected_run_id || compiled_build_id != expected_build_id {
        return (StatusCode::NOT_FOUND, false);
    }

    let expected_marker = format!("{expected_run_id}\n{compiled_build_id}\n");
    if marker == Some(expected_marker.as_str()) {
        (StatusCode::OK, true)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, true)
    }
}

/// Playwright polls this per-run path instead of a generic health endpoint.
/// A server from another concurrent run therefore cannot satisfy readiness,
/// even when both worktrees happen to have identical UI hashes.
pub async fn get_playwright_ready(AxumPath(run_id): AxumPath<String>) -> Response {
    let expected_run_id = std::env::var("PARISH_PLAYWRIGHT_RUN_ID").ok();
    let expected_build_id = std::env::var("PARISH_PLAYWRIGHT_BUILD_ID").ok();
    let marker = std::env::var_os("PARISH_PLAYWRIGHT_READY_FILE")
        .and_then(|path| std::fs::read_to_string(path).ok());
    let (status, reveal_build_id) = playwright_readiness_status(
        &run_id,
        expected_run_id.as_deref(),
        crate::PLAYWRIGHT_BUILD_ID,
        expected_build_id.as_deref(),
        marker.as_deref(),
    );
    let mut response = status.into_response();
    if reveal_build_id
        && let Some(build_id) = crate::PLAYWRIGHT_BUILD_ID
        && let Ok(value) = HeaderValue::from_str(build_id)
    {
        response
            .headers_mut()
            .insert(PLAYWRIGHT_BUILD_ID_HEADER, value);
    }
    response
}
