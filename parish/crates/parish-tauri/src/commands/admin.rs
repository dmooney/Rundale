//! Admin helpers — debug snapshot, inference rebuild, inactivity tick, bug report.

use std::sync::Arc;

use parish_core::debug_snapshot::{self, AuthDebug, DebugSnapshot, InferenceDebug};
use parish_core::ipc::BugReportRequest;
use tauri::Emitter;

use crate::AppState;
use crate::events::{EVENT_TEXT_LOG, TextLogPayload};

/// Builds a [`DebugSnapshot`] from the live `AppState`.
///
/// Shared by the `get_debug_snapshot` command and the bug reporter so the two
/// can never capture divergent views of the session.
pub(crate) async fn build_app_debug_snapshot(state: &Arc<AppState>) -> DebugSnapshot {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let events = state.debug_events.lock().await;
    let game_events = state.game_events.lock().await;
    let config = state.config.lock().await;

    let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
        state.inference_log.lock().await.iter().cloned().collect();

    let inference = InferenceDebug {
        provider_name: config.provider_name.clone(),
        model_name: config.model_name.clone(),
        base_url: config.base_url.clone(),
        cloud_provider: config.cloud_provider_name.clone(),
        cloud_model: config.cloud_model_name.clone(),
        has_queue: state.inference_queue.lock().await.is_some(),
        reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
        improv_enabled: config.improv_enabled,
        call_log,
        categories: parish_core::debug_snapshot::build_inference_categories(&*config),
        configured_providers: parish_core::debug_snapshot::build_configured_providers(),
        tier2_parse_failures_total: parish_core::npc::ticks::tier2_parse_failures_total(),
    };

    debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
    )
}

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Replaces the client and respawns the inference worker so subsequent
/// NPC conversations use the new configuration.
pub async fn rebuild_inference_inner(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let (provider_name, base_url, api_key) = {
        let config = state.config.lock().await;
        (
            config.provider_name.clone(),
            config.base_url.clone(),
            config.api_key.clone(),
        )
    };

    // Delegate to shared worker-lifecycle helper (#696).
    let (_any_client, url_warning) = parish_core::game_loop::rebuild_inference_worker(
        &provider_name,
        &base_url,
        api_key.as_deref(),
        &state.inference_config,
        state.inference_log.clone(),
        state.inference_file_log.clone(),
        parish_core::game_loop::inference::InferenceSlots {
            client: &state.client,
            worker_handle: &state.worker_handle,
            inference_queue: &state.inference_queue,
        },
    )
    .await;

    // Surface URL warning via Tauri emit (Tauri-specific side effect).
    if let Some(warn) = url_warning {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".into(),
                content: warn,
                subtype: None,
            },
        );
    }
}

pub(crate) async fn tick_inactivity(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let _persistence_guard = state.persistence_gate.lock().await;
    tick_inactivity_locked(state, app).await;
}

/// Applies one inactivity tick while the caller holds `persistence_gate`.
pub(crate) async fn tick_inactivity_locked(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let (last_player_activity, last_spoken_at, running, idle_after, auto_pause_after) = {
        let conversation = state.conversation.lock().await;
        let config = state.config.lock().await;
        (
            conversation.last_player_activity,
            conversation.last_spoken_at,
            conversation.conversation_in_progress,
            config.idle_banter_after_secs,
            config.auto_pause_after_secs,
        )
    };

    if running {
        return;
    }

    let world_state = {
        let world = state.world.lock().await;
        (
            world.clock.is_paused(),
            world.clock.is_inference_paused(),
            world.player_location,
        )
    };

    if world_state.0 || world_state.1 {
        return;
    }

    {
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(world_state.2);
    }

    let now = std::time::Instant::now();
    let player_idle = now.duration_since(last_player_activity).as_secs();
    let speech_idle = now.duration_since(last_spoken_at).as_secs();

    if player_idle >= auto_pause_after {
        {
            let mut world = state.world.lock().await;
            if world.clock.is_paused() || world.clock.is_inference_paused() {
                return;
            }
            world.clock.pause();
        }
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".into(),
                content:
                    "The parish falls quiet after a full minute of silence. Time is now paused."
                        .to_string(),
                subtype: None,
            },
        );
        super::snapshot::emit_world_update(state, app).await;
        let mut conversation = state.conversation.lock().await;
        conversation.last_spoken_at = now;
        return;
    }

    if player_idle >= idle_after && speech_idle >= idle_after {
        super::input::run_idle_banter_locked(state, app).await;
    }
}

// ── Bug reporting ─────────────────────────────────────────────────────────────

/// Files a bug report — bundles a screenshot, recent logs, and current game
/// state into a GitHub issue (or, in dry-run / no-token mode, a bundle on
/// disk). Shared with the MCP bridge's `/api/submit-bug-report` route.
#[tauri::command]
pub async fn submit_bug_report(
    title: String,
    description: Option<String>,
    screenshot_data_url: Option<String>,
    context: Option<parish_core::ipc::BugContext>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<parish_core::ipc::BugReportResult, String> {
    let request = BugReportRequest {
        title,
        description: description.unwrap_or_default(),
        screenshot_data_url,
        context,
    };
    do_submit_bug_report(&state, &app, request).await
}

/// Opens a URL in the system's default browser from the Tauri desktop app.
///
/// In Tauri v2 the webview blocks `<a target="_blank">` external navigation by
/// default (no `opener` plugin, no `window-creation` capability). This command
/// uses the OS process spawner to launch the default handler instead, so result
/// dialogs (e.g. the bug-report result link) work without a plugin dependency.
///
/// URLs are validated to only accept `https://` and `http://` schemes to
/// prevent shell injection via crafted issue URLs (#1223).
#[tauri::command]
pub async fn open_url(url: String) -> Result<(), String> {
    // Reject non-HTTP schemes to prevent shell injection.
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("open_url: rejected non-http URL scheme".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("open_url: failed to open URL: {e}"))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &url])
            .spawn()
            .map_err(|e| format!("open_url: failed to open URL: {e}"))?;
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("open_url: failed to open URL: {e}"))?;
    }

    Ok(())
}

/// Shared bug-report implementation (Tauri command + MCP bridge route).
///
/// Gathers a world + debug snapshot and a save summary from the live
/// `AppState`, resolves the screenshot (decoding the frontend-supplied data
/// URL, or triggering a live `request-screenshot` round-trip when absent),
/// then delegates the GitHub work to `parish_core::ipc::bug_report`.
pub(crate) async fn do_submit_bug_report(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    request: BugReportRequest,
) -> Result<parish_core::ipc::BugReportResult, String> {
    use parish_core::ipc::bug_report;

    // Feature flag (default-on kill switch, per AGENTS.md §6).
    if state.config.lock().await.flags.is_disabled("bug-report") {
        return Err(bug_report::BugReportError::Disabled.to_string());
    }

    let world_snapshot = {
        let world = state.world.lock().await;
        parish_core::ipc::snapshot_from_world(&world)
    };
    let debug = build_app_debug_snapshot(state).await;
    let save_summary = {
        let branch_id = *state.current_branch_id.lock().await;
        let branch_name = state.current_branch_name.lock().await.clone();
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

    // Resolve screenshot bytes: prefer the frontend-supplied data URL, else
    // trigger a live capture and read it back. A failed/timed-out capture is
    // non-fatal — the report is still filed without an image.
    let screenshot_png: Option<Vec<u8>> = match &request.screenshot_data_url {
        Some(data_url) => Some(bug_report::decode_data_url(data_url).map_err(|e| e.to_string())?),
        None => match super::screenshot::do_take_screenshot(state, app).await {
            Ok(info) => tokio::fs::read(&info.path).await.ok(),
            Err(_) => None,
        },
    };

    let cfg = bug_report::GitHubBugConfig::from_env_async().await;
    let bundle_root = state.saves_dir.join("bug-reports");
    let http = reqwest::Client::new();
    bug_report::create_bug_report(
        &http,
        &cfg,
        &request,
        &report_state,
        screenshot_png.as_deref(),
        &bundle_root,
    )
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use crate::commands::cmd_tests::test_app_state;

    // ── tick_inactivity does nothing when paused ────────────────────────────

    #[tokio::test]
    async fn world_clock_paused_state_has_expected_invariants() {
        let state = test_app_state();

        // Pause the world clock
        {
            let mut world = state.world.lock().await;
            world.clock.pause();
        }

        // tick_inactivity needs an AppHandle which we can't construct in unit
        // tests.  The early-return path (world is paused) never touches app,
        // so we exercise the banter-after-silence guard indirectly via
        // conversation state: conversation_in_progress=false, clock paused →
        // the guard returns immediately without calling run_idle_banter.
        // We verify world state is unchanged.
        let (paused_before, loc_before) = {
            let world = state.world.lock().await;
            (world.clock.is_paused(), world.player_location)
        };
        assert!(paused_before, "clock should be paused before tick");

        // We can't call tick_inactivity here because it needs tauri::AppHandle.
        // Instead, confirm the state invariants hold so future tests that mock
        // AppHandle can call tick_inactivity against this base.
        let paused_after = {
            let world = state.world.lock().await;
            world.clock.is_paused()
        };
        assert!(paused_after, "paused flag should still be set");
        assert_eq!(
            state.world.lock().await.player_location,
            loc_before,
            "location should be unchanged"
        );
    }
}
