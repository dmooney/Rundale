//! Input processing and game-loop adapter endpoints.
//!
//! Covers:
//! - `POST /api/submit-input` — main player input handler
//! - [`SubmitInputRequest`] — request body type
//! - [`rebuild_inference_inner`] — rebuilds the inference pipeline after provider changes
//! - [`handle_system_command`] / [`handle_game_input`] — game-loop adapters
//! - [`tick_inactivity`] — idle-banter and auto-pause timer
//! - [`spawn_loading_animation`] — loading indicator background task

use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use parish_core::event_bus::{EventBus as EventBusTrait, Topic};
use parish_core::input::{
    InputResult, classify_input_with_addressees, is_player_dialogue_with_addressees,
};
pub use parish_core::ipc::SubmitInputRequest;
use parish_core::ipc::{LoadingPayload, text_log};

use crate::state::AppState;

use super::admin::{admin_emails, check_admin, is_admin_command, validate_addressed_to};
use super::reactions::emit_npc_reactions;

// ── Input endpoint ──────────────────────────────────────────────────────────

/// `POST /api/submit-input` — processes player text input.
pub async fn submit_input(
    Extension(state): Extension<Arc<AppState>>,
    Extension(auth): Extension<crate::cf_auth::AuthContext>,
    Json(body): Json<SubmitInputRequest>,
) -> Response {
    let text = body.text.trim().to_string();
    if text.len() > 2000 {
        return StatusCode::BAD_REQUEST.into_response();
    }
    // #752 — cap addressed_to to prevent unbounded memory/allocation via the
    // NPC-addressing chip list.  Max 10 entries; each name ≤ 100 chars.
    if let Err(status) = validate_addressed_to(&body.addressed_to) {
        return status.into_response();
    }

    // Outermost per-session barrier: cursor capture, dispatch, task journal
    // commit/rollback, and result projection are one request-bound operation.
    let _persistence_guard = state.persistence_gate.lock().await;
    let before_turn = {
        let world = state.world.lock().await;
        parish_core::ipc::conversation_cursor(&world)
    };

    let mut dialogue_failure = None;
    if !text.is_empty() {
        match classify_input_with_addressees(&text, &body.addressed_to) {
            InputResult::SystemCommand(cmd) => {
                touch_player_activity(&state).await;
                // #332 — admin command gate: provider/key/model commands are operator-only.
                if is_admin_command(&cmd)
                    && let Err(status) = check_admin(&auth.email, &text, admin_emails())
                {
                    return status.into_response();
                }
                let _ = handle_system_command(cmd, &state, &text).await;
            }
            InputResult::GameInput(raw) => {
                // #1351 — only surface a player speech bubble + NPC reactions for
                // genuine dialogue. Deterministic non-dialogue actions (a bare
                // `look`, `look around`, movement phrases) must not render as player
                // speech or provoke NPC reactions. `handle_game_input` still runs so
                // the look/move action itself executes.
                let (dispatch, prelude_emissions) =
                    if is_player_dialogue_with_addressees(&raw, &body.addressed_to) {
                        let player_msg = text_log("player", format!("> {}", raw));
                        let player_msg_id = player_msg.id.clone();
                        let payload =
                            serde_json::to_value(player_msg).unwrap_or(serde_json::Value::Null);
                        (
                            Some((player_msg_id, raw.clone())),
                            vec![("text-log".to_string(), payload)],
                        )
                    } else {
                        (None, Vec::new())
                    };
                // Capture location before handle_game_input (which may move the player).
                let reaction_location = state.world.lock().await.player_location;
                let outcome = match handle_game_input(
                    raw,
                    body.addressed_to,
                    &state,
                    prelude_emissions,
                )
                .await
                {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::error!(
                            session_id = %state.session_id,
                            %error,
                            "player task journal append failed"
                        );
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("failed to persist player task: {error}"),
                        )
                            .into_response();
                    }
                };
                dialogue_failure = outcome.dialogue_failure;
                // Generate NPC reactions to the player's message in the background.
                if let Some((player_msg_id, raw_for_reactions)) = dispatch {
                    emit_npc_reactions(
                        &player_msg_id,
                        &raw_for_reactions,
                        reaction_location,
                        &state,
                    );
                }
            }
        }
    }

    let mut result = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        parish_core::ipc::build_submit_input_result(&world, &npc_manager, before_turn)
    };
    result.error = dialogue_failure;
    Json(result).into_response()
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Config is read in a scoped block so the lock is dropped before any other
/// lock is acquired, minimising the race window between concurrent rebuilds.
pub async fn rebuild_inference_inner(state: &Arc<AppState>) {
    // Read config first, then drop the lock before acquiring any other lock.
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
            client: &state.inference.client,
            worker_handle: &state.worker_handle,
            inference_queue: &state.inference.inference_queue,
        },
    )
    .await;

    // Surface URL warning via the server event bus (server-specific side effect).
    if let Some(warn) = url_warning {
        state
            .event_bus
            .emit_named(Topic::TextLog, "text-log", &text_log("system", warn));
    }
}

pub async fn touch_player_activity(state: &Arc<AppState>) {
    let mut conversation = state.conversation.lock().await;
    let now = std::time::Instant::now();
    conversation.last_player_activity = now;
    conversation.last_spoken_at = now;
}

pub async fn emit_world_update(state: &Arc<AppState>) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let mut ws = parish_core::ipc::snapshot_from_world(&world);
    ws.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    state
        .event_bus
        .emit_named(Topic::WorldUpdate, "world-update", &ws);
}

async fn world_update_payload(state: &Arc<AppState>) -> serde_json::Value {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let mut snapshot = parish_core::ipc::snapshot_from_world(&world);
    snapshot.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null)
}

/// Handles `/command` system inputs.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`AppStateCommandHost`] adapter (#696 slice 7).
pub async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    raw_text: &str,
) -> Result<(), String> {
    use crate::command_host::AppStateCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;

    let host = AppStateCommandHost::new(Arc::clone(state));
    shared_handle(&host, cmd, raw_text).await
}

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
///
/// Delegates to [`parish_core::game_loop::handle_game_input`] for all shared
/// logic (#696 slice 4).  Emits a world-update snapshot before and after
/// NPC-conversation paths so the frontend inference-pause indicator stays
/// accurate during long inference calls.
pub async fn handle_game_input(
    raw: String,
    addressed_to: Vec<String>,
    state: &Arc<AppState>,
    mut prelude_emissions: Vec<(String, serde_json::Value)>,
) -> Result<parish_core::game_loop::GameInputOutcome, parish_core::error::ParishError> {
    let must_stage = {
        let world = state.world.lock().await;
        parish_core::game_loop::input_may_mutate_tasks(&world, &raw)
    };
    let before_progress = state.world.lock().await.player_progress.clone();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::emitter::AppStateEmitter::new(Arc::clone(state)));
    let ctx = make_game_loop_ctx(state, Arc::clone(&emitter));
    let transport = state.transport.default_mode().clone();
    let reaction_templates = state
        .game_mod
        .as_ref()
        .map(|gm| gm.reactions.clone())
        .unwrap_or_default();

    if must_stage {
        let task_target = state
            .save_identity
            .task_journal_target(&state.session_id)
            .await;
        prelude_emissions.push((
            "world-update".to_string(),
            world_update_payload(state).await,
        ));
        let commit = parish_core::game_loop::handle_staged_game_input(
            &ctx,
            state.session_store.as_ref(),
            task_target.as_ref(),
            prelude_emissions,
            raw,
            addressed_to,
            &transport,
            &reaction_templates,
        )
        .await?;
        emit_world_update(state).await;
        return Ok(parish_core::game_loop::GameInputOutcome {
            task_mutations: commit.task_mutations,
            dialogue_failure: commit.dialogue_failure,
        });
    }

    touch_player_activity(state).await;
    parish_core::game_loop::flush_staged_emissions(emitter.as_ref(), prelude_emissions);

    let state_for_loading = Arc::clone(state);
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        spawn_loading_animation(Arc::clone(&state_for_loading), cancel.clone());
        Some(cancel)
    };

    // Emit world-update before so the frontend sees the inference-pause flag
    // when NPC conversation starts.
    emit_world_update(state).await;

    let outcome = parish_core::game_loop::handle_game_input(
        &ctx,
        raw,
        addressed_to,
        &transport,
        &reaction_templates,
        spawn_loading,
    )
    .await;
    let task_target = state
        .save_identity
        .task_journal_target(&state.session_id)
        .await;
    persist_task_mutations(
        state,
        task_target.as_ref(),
        before_progress,
        &outcome.task_mutations,
    )
    .await?;

    // Emit world-update after to clear the inference-pause flag.
    emit_world_update(state).await;
    Ok(outcome)
}

async fn persist_task_mutations(
    state: &Arc<AppState>,
    target: Option<&parish_core::session_store::TaskJournalTarget>,
    before: parish_core::session_store::PlayerProgress,
    tasks: &[parish_core::session_store::PlayerTask],
) -> Result<(), parish_core::error::ParishError> {
    parish_core::session_store::append_task_mutations_or_rollback(
        state.session_store.as_ref(),
        target,
        tasks,
        &state.world,
        before,
    )
    .await?;
    Ok(())
}

/// Resolves movement to a named location.
///
/// Delegates all state mutation, event emission, and world-update to
/// [`parish_core::game_loop::handle_movement`] (#696 slice 4).
///
/// Only called from tests; production code delegates via `handle_game_input`.
#[cfg(test)]
pub(crate) async fn handle_movement(target: &str, state: &Arc<AppState>) {
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::emitter::AppStateEmitter::new(Arc::clone(state)));
    let ctx = make_game_loop_ctx(state, emitter);
    let transport = state.transport.default_mode().clone();
    let reaction_templates = state
        .game_mod
        .as_ref()
        .map(|gm| gm.reactions.clone())
        .unwrap_or_default();
    parish_core::game_loop::handle_movement(&ctx, target, &transport, &reaction_templates).await;
}

// ── Shared orchestration helpers ─────────────────────────────────────────────

/// Creates a [`GameLoopContext`] borrowing the current session's `AppState`.
///
/// The emitter is an [`AppStateEmitter`] that routes events through
/// `state.event_bus`.
pub(crate) fn make_game_loop_ctx<'a>(
    state: &'a Arc<AppState>,
    emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter>,
) -> parish_core::game_loop::GameLoopContext<'a> {
    parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference.inference_queue,
        emitter,
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.inference.client,
        cloud_client: &state.inference.cloud_client,
        language: state.language_settings.clone(),
        inference_failure_messages: &state.inference_failure_messages,
        idle_messages: &state.idle_messages,
    }
}

/// Routes input to one or more NPCs at the player's location, or shows idle message.
///
/// Delegates to [`parish_core::game_loop::handle_npc_conversation`] for all
/// shared logic (#696), then emits a world-update snapshot when inference
/// finishes.
///
/// Only called from tests; production code delegates via `handle_game_input`.
#[cfg(test)]
pub(crate) async fn handle_npc_conversation(
    raw: String,
    target_names: Vec<String>,
    state: &Arc<AppState>,
) {
    // Build a shared-orchestration context from this session's AppState.
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::emitter::AppStateEmitter::new(Arc::clone(state)));
    let ctx = make_game_loop_ctx(state, Arc::clone(&emitter));

    // The loading animation is spawned by run_npc_turn inside the shared
    // handle_npc_conversation for each player-initiated turn.
    let state_for_loading = Arc::clone(state);
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        spawn_loading_animation(Arc::clone(&state_for_loading), cancel.clone());
        Some(cancel)
    };

    // Emit world-update before inference to surface the inference-pause flag.
    emit_world_update(state).await;

    // Run the shared conversation pipeline.
    parish_core::game_loop::handle_npc_conversation(&ctx, raw, target_names, spawn_loading).await;

    // Emit world-update after inference completes to clear the inference-pause flag.
    emit_world_update(state).await;
}

/// Generates spontaneous NPC banter when the player has been idle.
///
/// Delegates to [`parish_core::game_loop::run_idle_banter`] for all shared
/// logic (#696), then emits a world-update snapshot when the sequence ends.
/// Runs idle banter while the caller holds `persistence_gate`.
///
/// Inactivity ticks already own the barrier because they may auto-pause the
/// canonical clock. Keeping the locked form explicit prevents Tokio's
/// non-reentrant mutex from being acquired twice on the banter branch.
pub(crate) async fn run_idle_banter_locked(state: &Arc<AppState>) {
    let before_progress = state.world.lock().await.player_progress.clone();
    let task_target = state
        .save_identity
        .task_journal_target(&state.session_id)
        .await;
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::emitter::AppStateEmitter::new(Arc::clone(state)));
    let ctx = make_game_loop_ctx(state, Arc::clone(&emitter));

    // Idle banter does not show loading animation (no player is waiting).
    emit_world_update(state).await;
    let outcome = parish_core::game_loop::run_idle_banter(&ctx, || None).await;
    if let Err(error) = persist_task_mutations(
        state,
        task_target.as_ref(),
        before_progress,
        &outcome.task_mutations,
    )
    .await
    {
        tracing::error!(
            session_id = %state.session_id,
            %error,
            "idle-banter task journal append failed"
        );
    }
    emit_world_update(state).await;
}

pub async fn tick_inactivity(state: &Arc<AppState>) {
    let _persistence_guard = state.persistence_gate.lock().await;
    tick_inactivity_locked(state).await;
}

/// Applies one inactivity tick while the caller holds `persistence_gate`.
pub(crate) async fn tick_inactivity_locked(state: &Arc<AppState>) {
    let (last_player_activity, last_spoken_at, running) = {
        let conversation = state.conversation.lock().await;
        (
            conversation.last_player_activity,
            conversation.last_spoken_at,
            conversation.conversation_in_progress,
        )
    };
    let (idle_after, auto_pause_after) = {
        let config = state.config.lock().await;
        (config.idle_banter_after_secs, config.auto_pause_after_secs)
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
        state.event_bus.emit_named(
            Topic::TextLog,
            "text-log",
            &text_log(
                "system",
                "The parish falls quiet after a full minute of silence. Time is now paused.",
            ),
        );
        emit_world_update(state).await;
        let mut conversation = state.conversation.lock().await;
        conversation.last_spoken_at = now;
        return;
    }

    if player_idle >= idle_after && speech_idle >= idle_after {
        run_idle_banter_locked(state).await;
    }
}

/// Spawns a background task that emits rich [`LoadingPayload`] events with
/// cycling Irish phrases while the player waits for NPC inference.
pub fn spawn_loading_animation(state: Arc<AppState>, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        use parish_core::loading::LoadingAnimation;

        let mut anim = LoadingAnimation::new();

        // Emit an initial frame immediately
        anim.tick();
        let (r, g, b) = anim.current_color_rgb();
        state.event_bus.emit_named(
            Topic::Loading,
            "loading",
            &LoadingPayload {
                active: true,
                spinner: Some(anim.spinner_char().to_string()),
                phrase: Some(anim.phrase().to_string()),
                color: Some([r, g, b]),
            },
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                    anim.tick();
                    let (r, g, b) = anim.current_color_rgb();
                    state.event_bus.emit_named(Topic::Loading, "loading",
                        &LoadingPayload {
                            active: true,
                            spinner: Some(anim.spinner_char().to_string()),
                            phrase: Some(anim.phrase().to_string()),
                            color: Some([r, g, b]),
                        },
                    );
                }
            }
        }

        // Final "off" event
        state.event_bus.emit_named(
            Topic::Loading,
            "loading",
            &LoadingPayload {
                active: false,
                spinner: None,
                phrase: None,
                color: None,
            },
        );
    });
}
