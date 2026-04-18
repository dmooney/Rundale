//! HTTP route handlers for the Parish web server.
//!
//! Each route maps to a Tauri command, calling the shared handlers in
//! [`parish_core::ipc`] and returning JSON responses.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::{Extension, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tokio::sync::mpsc;

use parish_core::config::InferenceCategory;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{
    AnyClient, INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferenceQueue,
    await_inference_response, spawn_inference_worker,
};
use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::{
    ConversationLine, IDLE_MESSAGES, INFERENCE_FAILURE_MESSAGES, LoadingPayload, MapData, NpcInfo,
    NpcReactionPayload, ReactRequest, StreamEndPayload, StreamTokenPayload, StreamTurnEndPayload,
    TextPresentation, ThemePalette, WorldSnapshot, capitalize_first, text_log,
    text_log_for_stream_turn, text_log_typed,
};
use parish_core::npc::NpcId;
use parish_core::npc::manager::NpcManager;
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::npc::ticks::apply_tier1_response_with_config;
use parish_core::world::{LocationId, WorldState};

use parish_core::debug_snapshot::{self, AuthDebug, DebugSnapshot, InferenceDebug};
use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

use crate::middleware::SessionId;
use crate::session::GlobalState;
use crate::state::{AppState, ConversationRuntimeState, SaveState};

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ── Query endpoints ─────────────────────────────────────────────────────────

/// `GET /api/world-snapshot` — returns the current world snapshot.
pub async fn get_world_snapshot(Extension(state): Extension<Arc<AppState>>) -> Json<WorldSnapshot> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let mut snapshot = parish_core::ipc::snapshot_from_world(&world, transport);
    snapshot.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    Json(snapshot)
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

/// `GET /api/theme` — returns the current time-of-day palette (weather + season tinted).
pub async fn get_theme(Extension(state): Extension<Arc<AppState>>) -> Json<ThemePalette> {
    use chrono::Timelike;
    use parish_core::world::palette::compute_palette;
    let world = state.world.lock().await;
    let now = world.clock.now();
    let raw = compute_palette(
        now.hour(),
        now.minute(),
        world.clock.season(),
        world.weather,
    );
    Json(ThemePalette::from(raw))
}

/// `GET /api/ui-config` — returns UI configuration (splash text, labels, accent).
pub async fn get_ui_config(
    Extension(state): Extension<Arc<AppState>>,
) -> Json<crate::state::UiConfigSnapshot> {
    Json(state.ui_config.clone())
}

/// `GET /api/debug-snapshot` — returns full debug state for the debug panel.
pub async fn get_debug_snapshot(
    Extension(state): Extension<Arc<AppState>>,
    Extension(session_id): Extension<SessionId>,
    State(global): State<Arc<GlobalState>>,
) -> Json<DebugSnapshot> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let config = state.config.lock().await;
    let events = state.debug_events.lock().await;
    let game_events = state.game_events.lock().await;
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
    };
    let linked = global.sessions.google_account_for_session(&session_id.0);
    let auth = AuthDebug {
        oauth_enabled: global.oauth_config.is_some(),
        logged_in: linked.is_some(),
        provider: linked.as_ref().map(|_| "google".to_string()),
        display_name: linked.map(|(_sub, name)| name),
        session_id: Some(session_id.0.clone()),
    };
    Json(debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &auth,
    ))
}

// ── Input endpoint ──────────────────────────────────────────────────────────

/// Request body for `POST /api/submit-input`.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitInputRequest {
    /// The player's input text.
    pub text: String,
    /// Real names of NPCs explicitly addressed via chip selection (chip-first order).
    #[serde(default)]
    pub addressed_to: Vec<String>,
}

/// `POST /api/submit-input` — processes player text input.
pub async fn submit_input(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SubmitInputRequest>,
) -> impl IntoResponse {
    let text = body.text.trim().to_string();
    if text.is_empty() {
        return StatusCode::OK;
    }
    if text.len() > 2000 {
        return StatusCode::BAD_REQUEST;
    }

    touch_player_activity(&state).await;

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, &state).await;
        }
        InputResult::GameInput(raw) => {
            // Emit the player's own text as a dialogue bubble only for actual dialogue
            let player_msg = text_log("player", format!("> {}", raw));
            let player_msg_id = player_msg.id.clone();
            state.event_bus.emit("text-log", &player_msg);
            let raw_for_reactions = raw.clone();
            handle_game_input(raw, body.addressed_to, &state).await;
            // Generate rule-based NPC reactions to the player's message
            emit_npc_reactions(&player_msg_id, &raw_for_reactions, &state).await;
        }
    }

    StatusCode::OK
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Config is read in a scoped block so the lock is dropped before any other
/// lock is acquired, minimising the race window between concurrent rebuilds.
async fn rebuild_inference(state: &Arc<AppState>) {
    // Read config first, then drop the lock before acquiring any other lock.
    let (provider_name, base_url, api_key) = {
        let config = state.config.lock().await;
        (
            config.provider_name.clone(),
            config.base_url.clone(),
            config.api_key.clone(),
        )
    };

    let any_client = if provider_name == "simulator" {
        AnyClient::simulator()
    } else {
        if !(base_url.starts_with("http://") || base_url.starts_with("https://")) {
            state.event_bus.emit(
                "text-log",
                &text_log(
                    "system",
                    format!(
                        "Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.",
                        base_url
                    ),
                ),
            );
        }
        let oai = OpenAiClient::new(&base_url, api_key.as_deref());
        let mut client_guard = state.client.lock().await;
        *client_guard = Some(oai.clone());
        AnyClient::open_ai(oai)
    };

    // Abort the old inference worker before spawning a replacement to prevent
    // orphaned tasks from accumulating (each holds an HTTP client and channel).
    // Without this, repeated provider/key/model changes leak workers (bug #224).
    {
        let mut wh = state.worker_handle.lock().await;
        if let Some(old) = wh.take() {
            old.abort();
        }
    }

    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        any_client,
        interactive_rx,
        background_rx,
        batch_rx,
        state.inference_log.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
    let mut iq = state.inference_queue.lock().await;
    *iq = Some(queue);
    drop(iq);
    let mut wh = state.worker_handle.lock().await;
    *wh = Some(worker);
}

async fn touch_player_activity(state: &Arc<AppState>) {
    let mut conversation = state.conversation.lock().await;
    let now = std::time::Instant::now();
    conversation.last_player_activity = now;
    conversation.last_spoken_at = now;
}

async fn emit_world_update(state: &Arc<AppState>) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
    ws.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    state.event_bus.emit("world-update", &ws);
}

/// Handles `/command` system inputs using the shared command handler.
async fn handle_system_command(cmd: parish_core::input::Command, state: &Arc<AppState>) {
    use parish_core::ipc::{CommandEffect, handle_command};

    // Acquire all locks, run the shared handler, then release.
    let result = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let mut config = state.config.lock().await;
        handle_command(cmd, &mut world, &mut npc_manager, &mut config)
    };

    // Handle mode-specific side effects.
    for effect in &result.effects {
        match effect {
            CommandEffect::RebuildInference => rebuild_inference(state).await,
            CommandEffect::RebuildCloudClient => {
                let config = state.config.lock().await;
                let base_url = config
                    .cloud_base_url
                    .as_deref()
                    .unwrap_or("https://openrouter.ai/api")
                    .to_string();
                let api_key = config.cloud_api_key.clone();
                drop(config);
                let mut cloud_guard = state.cloud_client.lock().await;
                *cloud_guard = Some(OpenAiClient::new(&base_url, api_key.as_deref()));
            }
            CommandEffect::Quit => {
                // Web server cannot be quit from the game.
                state.event_bus.emit(
                    "text-log",
                    &text_log(
                        "system",
                        "The web server cannot be quit from the game. Close your browser tab.",
                    ),
                );
            }
            CommandEffect::ToggleMap => {
                state.event_bus.emit("toggle-full-map", &());
            }
            CommandEffect::OpenDesigner => {
                state.event_bus.emit("open-designer", &());
            }
            CommandEffect::SaveGame => {
                let msg = match do_save_game_inner(state).await {
                    Ok(msg) => msg,
                    Err(e) => format!("Save failed: {}", e),
                };
                state.event_bus.emit("text-log", &text_log("system", msg));
            }
            CommandEffect::ForkBranch(name) => {
                let parent_id = state.current_branch_id.lock().await.unwrap_or(1);
                let msg = match do_fork_branch_inner(state, name, parent_id).await {
                    Ok(msg) => msg,
                    Err(e) => format!("Fork failed: {}", e),
                };
                state.event_bus.emit("text-log", &text_log("system", msg));
            }
            CommandEffect::LoadBranch(_) => {
                // Open the save picker in the frontend
                state.event_bus.emit("save-picker", &());
            }
            CommandEffect::ListBranches => {
                let msg = match do_list_branches_inner(state).await {
                    Ok(text) => text,
                    Err(e) => format!("Failed to list branches: {}", e),
                };
                state.event_bus.emit("text-log", &text_log("system", msg));
            }
            CommandEffect::ShowLog => {
                let msg = match do_branch_log_inner(state).await {
                    Ok(text) => text,
                    Err(e) => format!("Failed to show log: {}", e),
                };
                state.event_bus.emit("text-log", &text_log("system", msg));
            }
            CommandEffect::Debug(_) => {
                state.event_bus.emit(
                    "text-log",
                    &text_log("system", "Debug commands are not available in web mode."),
                );
            }
            CommandEffect::ShowSpinner(secs) => {
                let secs = *secs;
                let cancel = tokio_util::sync::CancellationToken::new();
                spawn_loading_animation(Arc::clone(state), cancel.clone());
                let msg = format!("Showing spinner for {} seconds...", secs);
                state.event_bus.emit("text-log", &text_log("system", msg));
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    cancel.cancel();
                });
            }
            CommandEffect::NewGame => match do_new_game_inner(state).await {
                Ok(()) => {
                    state.event_bus.emit(
                        "text-log",
                        &text_log("system", "A new chapter begins in the parish..."),
                    );
                }
                Err(e) => {
                    state.event_bus.emit(
                        "text-log",
                        &text_log("system", format!("New game failed: {}", e)),
                    );
                }
            },
            CommandEffect::SaveFlags => {
                let flags = state.config.lock().await.flags.clone();
                let path = state.flags_path.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = flags.save_to_file(&path) {
                        tracing::warn!("Failed to save feature flags: {}", e);
                    }
                });
            }
            CommandEffect::ApplyTheme(name, mode) => {
                state.event_bus.emit(
                    "theme-switch",
                    &serde_json::json!({ "name": name, "mode": mode }),
                );
            }
            CommandEffect::ApplyTiles(id) => {
                state
                    .event_bus
                    .emit("tiles-switch", &serde_json::json!({ "id": id }));
            }
        }
    }

    // Emit the command response text. Tabular responses (e.g. `/help`) carry
    // a `subtype: "tabular"` hint so the chat UI can render them in monospace.
    if !result.response.is_empty() {
        let payload = match result.presentation {
            TextPresentation::Tabular => text_log_typed("system", result.response, "tabular"),
            TextPresentation::Prose => text_log("system", result.response),
        };
        state.event_bus.emit("text-log", &payload);
    }

    // Emit updated world snapshot.
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
    ws.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    state.event_bus.emit("world-update", &ws);
}

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
async fn handle_game_input(raw: String, addressed_to: Vec<String>, state: &Arc<AppState>) {
    // Resolve the intent client and model (Intent category override, or base).
    let (client, model) = {
        let config = state.config.lock().await;
        let base_client = state.client.lock().await;
        config.resolve_category_client(InferenceCategory::Intent, base_client.as_ref())
    };

    // Parse intent: tries local keywords first, then LLM for ambiguous input.
    let intent = if let Some(client) = &client {
        let mut world = state.world.lock().await;
        world.clock.inference_pause();
        drop(world);
        let result = parse_intent(client, &raw, &model).await;
        let mut world = state.world.lock().await;
        world.clock.inference_resume();
        drop(world);
        result.ok()
    } else {
        // No client configured — use local keyword parsing only.
        parish_core::input::parse_intent_local(&raw)
    };

    let is_move = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Move))
        .unwrap_or(false);
    let is_look = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Look))
        .unwrap_or(false);
    let is_talk = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Talk))
        .unwrap_or(false);
    let move_target = intent
        .as_ref()
        .filter(|_i| is_move)
        .and_then(|i| i.target.clone());
    let talk_target = intent
        .as_ref()
        .filter(|_i| is_talk)
        .and_then(|i| i.target.clone());

    if is_move {
        if let Some(target) = move_target {
            handle_movement(&target, state).await;
        } else {
            state.event_bus.emit(
                "text-log",
                &text_log("system", "And where would ye be off to?"),
            );
        }
        return;
    }

    if is_look {
        handle_look(state).await;
        return;
    }

    // `talk to <name>` / `speak to <name>` — bypass @mention parsing and
    // route directly to the multi-target dispatch loop with this single
    // addressee. The chip-selection list still gets prepended below.
    if is_talk && let Some(target) = talk_target {
        let mut targets: Vec<String> = Vec::with_capacity(addressed_to.len() + 1);
        for name in addressed_to {
            if !targets.iter().any(|t| t == &name) {
                targets.push(name);
            }
        }
        if !targets.iter().any(|t| t == &target) {
            targets.push(target);
        }
        handle_npc_conversation(String::new(), targets, state).await;
        return;
    }

    // Resolve ordered NPC recipients from visible local names.
    let mentions = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        parish_core::ipc::extract_npc_mentions(&raw, &world, &npc_manager)
    };

    // Chip selections (real names from the frontend) come first, then any
    // inline @mentions that aren't already in the chip set. Deduping happens
    // in `resolve_npc_targets` via `find_by_name`, which matches both real
    // and display names.
    let mut targets: Vec<String> = Vec::with_capacity(addressed_to.len() + mentions.names.len());
    for name in addressed_to {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }
    for name in mentions.names {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }

    handle_npc_conversation(mentions.remaining, targets, state).await;
}

/// Resolves movement to a named location.
///
/// Delegates all state mutation and message generation to
/// [`parish_core::game_session::apply_movement`], then emits the returned
/// effects over the event bus.
async fn handle_movement(target: &str, state: &Arc<AppState>) {
    use parish_core::game_session::apply_movement;

    let transport = state.transport.default_mode().clone();
    let reaction_templates = state
        .game_mod
        .as_ref()
        .map(|gm| gm.reactions.clone())
        .unwrap_or_default();

    // Apply movement within a single lock scope to prevent TOCTOU races.
    let effects = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        apply_movement(
            &mut world,
            &mut npc_manager,
            &reaction_templates,
            target,
            &transport,
        )
    };

    // Emit travel-start animation payload before text messages
    if let Some(travel_payload) = &effects.travel_start {
        state.event_bus.emit("travel-start", travel_payload);
    }

    // Emit each player-visible message
    for msg in &effects.messages {
        state
            .event_bus
            .emit("text-log", &text_log(msg.source, &msg.text));
    }

    // Emit NPC arrival reactions — stream gradually like normal NPC dialogue
    if !effects.arrival_reactions.is_empty() {
        use parish_core::game_session::stream_reaction_texts;

        let (
            all_npcs,
            current_location_id,
            loc_name,
            tod,
            weather,
            introduced,
            reaction_client,
            reaction_model,
        ) = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;
            let (rc, rm) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            (
                npc_manager.all_npcs().cloned().collect::<Vec<_>>(),
                world.player_location,
                world
                    .current_location_data()
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                world.clock.time_of_day(),
                world.weather.to_string(),
                npc_manager.introduced_set(),
                rc,
                rm,
            )
        };

        stream_reaction_texts(
            &effects.arrival_reactions,
            &all_npcs,
            current_location_id,
            &loc_name,
            tod,
            &weather,
            &introduced,
            reaction_client.as_ref(),
            &reaction_model,
            None,
            |_turn_id, npc_name| {
                state
                    .event_bus
                    .emit("text-log", &text_log(npc_name, String::new()));
            },
            |turn_id, source, batch| {
                state.event_bus.emit(
                    "stream-token",
                    &StreamTokenPayload {
                        token: batch.to_string(),
                        turn_id,
                        source: source.to_string(),
                    },
                );
            },
        )
        .await;

        // Finalise the streaming state so the frontend marks the last entry done.
        state
            .event_bus
            .emit("stream-end", &StreamEndPayload { hints: vec![] });
    }

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
        let current_location = {
            let world = state.world.lock().await;
            world.player_location
        };
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(current_location);
        conversation.last_spoken_at = std::time::Instant::now();
        drop(conversation);

        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let mut ws = parish_core::ipc::snapshot_from_world(&world, &transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        state.event_bus.emit("world-update", &ws);
    }
}

/// Renders the current location description and exits.
async fn handle_look(state: &Arc<AppState>) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let text = parish_core::ipc::render_look_text(
        &world,
        &npc_manager,
        transport.speed_m_per_s,
        &transport.label,
        false,
    );
    state.event_bus.emit("text-log", &text_log("system", text));
}

struct TurnOutcome {
    line: Option<ConversationLine>,
    hints: Vec<parish_core::npc::IrishWordHint>,
}

async fn run_npc_turn(
    state: &Arc<AppState>,
    queue: &InferenceQueue,
    model: &str,
    speaker_id: NpcId,
    prompt_input: &str,
    transcript: &[ConversationLine],
) -> Option<TurnOutcome> {
    let setup = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let config = state.config.lock().await;
        // Detect player self-introduction before building NPC prompt
        parish_core::ipc::detect_and_record_player_name(
            &mut world,
            &mut npc_manager,
            prompt_input,
            speaker_id,
        );
        parish_core::ipc::prepare_npc_conversation_turn(
            &world,
            &mut npc_manager,
            prompt_input,
            speaker_id,
            transcript,
            config.improv_enabled,
        )
    }?;

    let loading_cancel = tokio_util::sync::CancellationToken::new();
    spawn_loading_animation(Arc::clone(state), loading_cancel.clone());

    let (token_tx, token_rx) = mpsc::unbounded_channel::<String>();
    let display_label = capitalize_first(&setup.display_name);
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
    state.event_bus.emit(
        "text-log",
        &text_log_for_stream_turn(display_label.clone(), String::new(), req_id),
    );
    let send_result = queue
        .send(
            req_id,
            model.to_string(),
            setup.context,
            Some(setup.system_prompt),
            Some(token_tx),
            None,
            Some(0.7),
            parish_core::inference::InferencePriority::Interactive,
        )
        .await;

    let response_rx = match send_result {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
            state
                .event_bus
                .emit("stream-turn-end", &StreamTurnEndPayload { turn_id: req_id });
            state.event_bus.emit(
                "text-log",
                &text_log(
                    "system",
                    "The parish storyteller has wandered off. Try again.",
                ),
            );
            loading_cancel.cancel();
            return None;
        }
    };

    let stream_handle = tokio::spawn({
        let state_clone = Arc::clone(state);
        let cancel = loading_cancel.clone();
        let source = display_label.clone();
        async move {
            parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
                cancel.cancel();
                state_clone.event_bus.emit(
                    "stream-token",
                    &StreamTokenPayload {
                        token: batch.to_string(),
                        turn_id: req_id,
                        source: source.clone(),
                    },
                );
            })
            .await
        }
    });

    let timeout_secs = {
        let config = state.config.lock().await;
        if config.flags.is_disabled("inference-response-timeout") {
            None
        } else {
            Some(INFERENCE_RESPONSE_TIMEOUT_SECS)
        }
    };
    let outcome = await_inference_response(
        response_rx,
        timeout_secs.map(std::time::Duration::from_secs),
    )
    .await;
    let _ = stream_handle.await;
    state
        .event_bus
        .emit("stream-turn-end", &StreamTurnEndPayload { turn_id: req_id });

    let response = match outcome {
        InferenceAwaitOutcome::Response(r) => r,
        InferenceAwaitOutcome::Closed => {
            tracing::warn!(
                req_id,
                "NPC inference response channel closed without a reply",
            );
            state.event_bus.emit(
                "text-log",
                &text_log("system", "The storyteller has wandered off mid-tale."),
            );
            loading_cancel.cancel();
            return None;
        }
        InferenceAwaitOutcome::TimedOut { secs } => {
            tracing::warn!(req_id, secs, "NPC inference response timed out",);
            state.event_bus.emit(
                "text-log",
                &text_log("system", "The storyteller is lost in thought. Try again."),
            );
            loading_cancel.cancel();
            return None;
        }
    };

    if response.error.is_some() {
        tracing::warn!("Inference error: {:?}", response.error);
        let idx = response.id as usize % INFERENCE_FAILURE_MESSAGES.len();
        state.event_bus.emit(
            "text-log",
            &text_log("system", INFERENCE_FAILURE_MESSAGES[idx]),
        );
        loading_cancel.cancel();
        return None;
    }

    loading_cancel.cancel();

    let parsed = parse_npc_stream_response(&response.text);
    let hints = parsed
        .metadata
        .as_ref()
        .map(|meta| meta.language_hints.clone())
        .unwrap_or_default();

    {
        let world = state.world.lock().await;
        let game_time = world.clock.now();
        let mut npc_manager = state.npc_manager.lock().await;
        let player_name = if npc_manager.knows_player_name(speaker_id) {
            world.player_name.clone()
        } else {
            None
        };
        if let Some(npc) = npc_manager.get_mut(speaker_id) {
            let _ = apply_tier1_response_with_config(
                npc,
                &parsed,
                prompt_input,
                game_time,
                &Default::default(),
                player_name.as_deref(),
            );
        }
    }

    let line = if parsed.dialogue.trim().is_empty() {
        None
    } else {
        Some(ConversationLine {
            speaker: display_label,
            text: parsed.dialogue,
        })
    };

    Some(TurnOutcome { line, hints })
}

async fn set_conversation_running(state: &Arc<AppState>, running: bool) {
    let mut conversation = state.conversation.lock().await;
    conversation.conversation_in_progress = running;
}

/// Routes input to one or more NPCs at the player's location, or shows idle message.
async fn handle_npc_conversation(raw: String, target_names: Vec<String>, state: &Arc<AppState>) {
    let trimmed = raw.trim().to_string();
    let (npc_present, player_location, queue, model, max_follow_up_turns, targets) = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;
        let npc_present = !npc_manager.npcs_at(world.player_location).is_empty();
        let targets = parish_core::ipc::resolve_npc_targets(&world, &npc_manager, &target_names);
        (
            npc_present,
            world.player_location,
            queue.clone(),
            config.model_name.clone(),
            config.max_follow_up_turns,
            targets,
        )
    };

    if !npc_present {
        let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % IDLE_MESSAGES.len();
        state
            .event_bus
            .emit("text-log", &text_log("system", IDLE_MESSAGES[idx]));
        return;
    }

    if trimmed.is_empty() {
        state.event_bus.emit(
            "text-log",
            &text_log(
                "system",
                "There are ears enough for ye here, but say something first.",
            ),
        );
        return;
    }

    let Some(queue) = queue else {
        state.event_bus.emit(
            "text-log",
            &text_log(
                "system",
                "There's someone here, but the LLM is not configured — set a provider with /provider.",
            ),
        );
        return;
    };

    if targets.is_empty() {
        state.event_bus.emit(
            "text-log",
            &text_log("system", "No one here answers to that name just now."),
        );
        return;
    }

    let mut transcript = {
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(player_location);
        conversation.push_line(ConversationLine {
            speaker: "You".to_string(),
            text: trimmed.clone(),
        });
        conversation.transcript.iter().cloned().collect::<Vec<_>>()
    };

    set_conversation_running(state, true).await;
    {
        let mut world = state.world.lock().await;
        world.clock.inference_pause();
    }
    emit_world_update(state).await;

    let mut combined_hints: Vec<parish_core::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // Phase 1: each addressed NPC takes one turn in the order they were named.
    for speaker_id in &targets {
        let Some(outcome) = run_npc_turn(
            state,
            &queue,
            &model,
            *speaker_id,
            trimmed.as_str(),
            &transcript,
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = state.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(*speaker_id);
        last_speaker = Some(*speaker_id);
    }

    // Phase 2: autonomous chain. Bystanders or already-addressed NPCs may
    // chime in based on the heuristic in `npc::autonomous::pick_next_speaker`.
    // Capped at `max_follow_up_turns` to prevent runaway chatter.
    let chain_cap = max_follow_up_turns.min(parish_core::npc::autonomous::MAX_CHAIN_TURNS);
    for _ in 0..chain_cap {
        let next_speaker_id = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let candidates: Vec<&parish_core::npc::Npc> =
                npc_manager.npcs_at(world.player_location);
            parish_core::npc::autonomous::pick_next_speaker(
                &candidates,
                last_speaker,
                &spoken_this_chain,
                &targets,
            )
            .map(|npc| npc.id)
        };

        let Some(speaker_id) = next_speaker_id else {
            break;
        };

        let Some(outcome) = run_npc_turn(
            state,
            &queue,
            &model,
            speaker_id,
            "listens while the nearby conversation continues",
            &transcript,
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = state.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(speaker_id);
        last_speaker = Some(speaker_id);
    }

    {
        let mut world = state.world.lock().await;
        world.clock.inference_resume();
    }
    set_conversation_running(state, false).await;
    emit_world_update(state).await;

    // Emit a single stream-end at the end of the entire turn so the input
    // field stays disabled through every NPC's response. (PR #222 emitted
    // one per turn, which let the input flicker open between NPCs — that
    // contradicted the explicit user spec.)
    state.event_bus.emit(
        "stream-end",
        &StreamEndPayload {
            hints: combined_hints,
        },
    );
}

async fn run_idle_banter(state: &Arc<AppState>) {
    let (queue, model, player_location, max_follow_up_turns, speakers) = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;

        let mut speakers = npc_manager.npcs_at_ids(world.player_location);
        speakers.sort_by_key(|id| id.0);
        speakers.truncate(2);

        (
            queue.clone(),
            config.model_name.clone(),
            world.player_location,
            config.max_follow_up_turns.min(2),
            speakers,
        )
    };

    let Some(queue) = queue else {
        return;
    };
    if speakers.is_empty() {
        return;
    }

    let mut transcript = {
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(player_location);
        conversation.transcript.iter().cloned().collect::<Vec<_>>()
    };

    set_conversation_running(state, true).await;
    {
        let mut world = state.world.lock().await;
        world.clock.inference_pause();
    }
    emit_world_update(state).await;

    let mut combined_hints: Vec<parish_core::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // First spontaneous remark: deterministic order (sorted by id) so quiet
    // locations with calm NPCs still produce a line. Without this fallback the
    // heuristic would refuse to fire on a peaceful location.
    if let Some(first_speaker) = speakers.first().copied()
        && let Some(outcome) = run_npc_turn(
            state,
            &queue,
            &model,
            first_speaker,
            "breaks the silence with a natural nearby remark",
            &transcript,
        )
        .await
    {
        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = state.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(first_speaker);
        last_speaker = Some(first_speaker);
    }

    // Follow-up turns: heuristic-based selection so a high-energy or
    // closely-related bystander can chime in.
    let chain_cap = max_follow_up_turns.min(parish_core::npc::autonomous::MAX_CHAIN_TURNS);
    for _ in 0..chain_cap {
        let next_speaker_id = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let candidates: Vec<&parish_core::npc::Npc> =
                npc_manager.npcs_at(world.player_location);
            parish_core::npc::autonomous::pick_next_speaker(
                &candidates,
                last_speaker,
                &spoken_this_chain,
                &[],
            )
            .map(|npc| npc.id)
        };

        let Some(speaker_id) = next_speaker_id else {
            break;
        };

        let Some(outcome) = run_npc_turn(
            state,
            &queue,
            &model,
            speaker_id,
            "answers the nearby remark and keeps the local chatter going",
            &transcript,
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = state.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(speaker_id);
        last_speaker = Some(speaker_id);
    }

    {
        let mut world = state.world.lock().await;
        world.clock.inference_resume();
    }
    set_conversation_running(state, false).await;
    emit_world_update(state).await;

    // Single stream-end after the entire idle-banter sequence (see comment
    // in handle_npc_conversation for the rationale).
    state.event_bus.emit(
        "stream-end",
        &StreamEndPayload {
            hints: combined_hints,
        },
    );
}

pub(crate) async fn tick_inactivity(state: &Arc<AppState>) {
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
        state.event_bus.emit(
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
        run_idle_banter(state).await;
    }
}

/// Spawns a background task that emits rich [`LoadingPayload`] events with
/// cycling Irish phrases while the player waits for NPC inference.
fn spawn_loading_animation(state: Arc<AppState>, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        use parish_core::loading::LoadingAnimation;

        let mut anim = LoadingAnimation::new();

        // Emit an initial frame immediately
        anim.tick();
        let (r, g, b) = anim.current_color_rgb();
        state.event_bus.emit(
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
                    state.event_bus.emit(
                        "loading",
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
        state.event_bus.emit(
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

// ── Reaction endpoint ──────────────────────────────────────────────────────

/// `POST /api/react-to-message` — player reacts to an NPC message with an emoji.
pub async fn react_to_message(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<ReactRequest>,
) -> impl IntoResponse {
    // Validate emoji is in the palette
    if reactions::reaction_description(&body.emoji).is_none() {
        return StatusCode::BAD_REQUEST;
    }

    // Store the reaction in the target NPC's reaction log
    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&body.npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log
            .add(&body.emoji, &body.message_snippet, now);
    }

    StatusCode::OK
}

/// Generates rule-based NPC reactions to a player message and emits events.
///
/// Called after processing player input. Each NPC at the player's location
/// has a chance to react with an emoji based on keyword matching.
async fn emit_npc_reactions(player_msg_id: &str, player_input: &str, state: &Arc<AppState>) {
    let npc_names: Vec<String> = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| n.name.clone())
            .collect()
    };

    for name in npc_names {
        if let Some(emoji) = reactions::generate_rule_reaction(player_input) {
            state.event_bus.emit(
                "npc-reaction",
                &NpcReactionPayload {
                    message_id: player_msg_id.to_string(),
                    emoji,
                    source: capitalize_first(&name),
                },
            );
        }
    }
}

// ── Persistence helpers (called by both REST handlers and CommandEffect) ─────

/// Saves the current game state. Returns a human-readable success message.
async fn do_save_game_inner(state: &Arc<AppState>) -> Result<String, String> {
    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let mut save_path_guard = state.save_path.lock().await;
    let mut branch_id_guard = state.current_branch_id.lock().await;
    let mut branch_name_guard = state.current_branch_name.lock().await;
    let saves_dir = state.saves_dir.clone();

    let db_path = if let Some(ref path) = *save_path_guard {
        path.clone()
    } else {
        let path = new_save_path(&saves_dir);
        *save_path_guard = Some(path.clone());
        path
    };

    let branch_id = if let Some(id) = *branch_id_guard {
        id
    } else {
        let db_path_clone = db_path.clone();
        let id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
            let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
            let branch = db.find_branch("main").map_err(|e| e.to_string())?;
            Ok(branch.map(|b| b.id).unwrap_or(1))
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;

        *branch_id_guard = Some(id);
        *branch_name_guard = Some("main".to_string());
        id
    };

    let db_path_clone = db_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        db.save_snapshot(branch_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let filename = db_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let branch_name = branch_name_guard.as_deref().unwrap_or("main");

    Ok(format!(
        "Game saved to {} (branch: {}).",
        filename, branch_name
    ))
}

/// Creates a new branch forked from a parent. Returns a human-readable message.
async fn do_fork_branch_inner(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    let save_path_guard = state.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let name_owned = name.to_string();
    let db_path_clone = db_path.clone();

    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        db.create_branch(&name_owned, Some(parent_branch_id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let db_path_clone2 = db_path;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let db = Database::open(&db_path_clone2).map_err(|e| e.to_string())?;
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.to_string());

    Ok(format!("Created new branch '{}'.", name))
}

/// Lists all branches in the current save file.
async fn do_list_branches_inner(state: &Arc<AppState>) -> Result<String, String> {
    let save_path_guard = state.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let current_branch_id = *state.current_branch_id.lock().await;

    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let branches = db.list_branches().map_err(|e| e.to_string())?;
        if branches.is_empty() {
            return Ok("No branches found.".to_string());
        }
        let mut lines = vec!["Branches:".to_string()];
        for b in &branches {
            let marker = if Some(b.id) == current_branch_id {
                " *"
            } else {
                ""
            };
            let parent = b
                .parent_branch_id
                .and_then(|pid| branches.iter().find(|bb| bb.id == pid))
                .map(|bb| format!(" (from {})", bb.name))
                .unwrap_or_default();
            lines.push(format!("  {}{}{}", b.name, parent, marker));
        }
        Ok(lines.join("\n"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shows the save log for the current branch.
async fn do_branch_log_inner(state: &Arc<AppState>) -> Result<String, String> {
    let save_path_guard = state.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let branch_id = state
        .current_branch_id
        .lock()
        .await
        .ok_or_else(|| "No active branch.".to_string())?;

    let branch_name = state.current_branch_name.lock().await.clone();
    let name = branch_name.as_deref().unwrap_or("unknown").to_string();

    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let log = db.branch_log(branch_id).map_err(|e| e.to_string())?;
        if log.is_empty() {
            return Ok("No snapshots yet on this branch.".to_string());
        }
        let mut lines = vec![format!("Save log for branch '{}':", name)];
        for (i, info) in log.iter().enumerate() {
            let time = parish_core::persistence::format_timestamp(&info.real_time);
            lines.push(format!("  {}. {} (game: {})", i + 1, time, info.game_time));
        }
        Ok(lines.join("\n"))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts a new game (resets world and NPCs from data dir).
async fn do_new_game_inner(state: &Arc<AppState>) -> Result<(), String> {
    let data_dir = state.data_dir.clone();
    let saves_dir = state.saves_dir.clone();

    // Load fresh world and NPCs — prefer the active game mod when available,
    // matching the same logic used by the Tauri backend.
    let (world, npcs_path) = if let Some(ref gm) = state.game_mod {
        let world = parish_core::game_mod::world_state_from_mod(gm)
            .map_err(|e| format!("Failed to load world from mod: {}", e))?;
        (world, gm.npcs_path())
    } else {
        // Legacy fallback: try parish.json first, then world.json.
        let world_path = {
            let parish = data_dir.join("parish.json");
            let world = data_dir.join("world.json");
            if parish.exists() { parish } else { world }
        };
        let world = WorldState::from_parish_file(&world_path, LocationId(15)).unwrap_or_else(|e| {
            tracing::warn!("Failed to load world data: {}. Using default world.", e);
            WorldState::new()
        });
        (world, data_dir.join("npcs.json"))
    };

    let mut npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|e| {
        tracing::warn!("Failed to load npcs.json: {}. No NPCs.", e);
        NpcManager::new()
    });
    npc_manager.assign_tiers(&world, &[]);

    // Replace state atomically (both locks held together to prevent a window
    // where a command handler sees the new world with the old NPC manager).
    {
        let mut w = state.world.lock().await;
        let mut nm = state.npc_manager.lock().await;
        *w = world;
        *nm = npc_manager;
    }

    // Reset conversation transcript so stale dialogue from the previous game
    // does not bleed into NPC conversations in the new game (#281).
    {
        let mut conv = state.conversation.lock().await;
        *conv = ConversationRuntimeState::new();
    }

    // Create a new save file
    let path = new_save_path(&saves_dir);
    let snapshot = {
        let w = state.world.lock().await;
        let nm = state.npc_manager.lock().await;
        GameSnapshot::capture(&w, &nm)
    };

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to create main branch".to_string())?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    // Emit updated world snapshot
    {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let transport = state.transport.default_mode();
        let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        state.event_bus.emit("world-update", &ws);
    }

    Ok(())
}

// ── Persistence endpoints ────────────────────────────────────────────────────

/// `GET /api/discover-save-files` — returns all save files with branch metadata.
pub async fn discover_save_files(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<SaveFileInfo>>, (StatusCode, String)> {
    let graph = {
        let world = state.world.lock().await;
        world.graph.clone()
    };
    let saves_dir = state.saves_dir.clone();

    let saves = tokio::task::spawn_blocking(move || discover_saves(&saves_dir, &graph))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(saves))
}

/// `POST /api/save-game` — saves the current game state to the active save file.
pub async fn save_game(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<String>, (StatusCode, String)> {
    let msg = do_save_game_inner(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(msg))
}

/// Request body for `POST /api/load-branch`.
#[derive(serde::Deserialize)]
pub struct LoadBranchRequest {
    /// Path to the save file.
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// Branch database id to load.
    #[serde(rename = "branchId")]
    pub branch_id: i64,
}

/// `POST /api/load-branch` — loads a branch from a save file.
pub async fn load_branch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<LoadBranchRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = std::path::PathBuf::from(&body.file_path);
    // Validate the path is within the saves directory to prevent path traversal.
    let canonical = path.canonicalize().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid save file path".to_string(),
        )
    })?;
    let saves_canonical = state.saves_dir.canonicalize().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Saves directory error".to_string(),
        )
    })?;
    if !canonical.starts_with(&saves_canonical) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Path is outside saves directory".to_string(),
        ));
    }
    let path = canonical;
    let branch_id = body.branch_id;
    let path_clone = path.clone();

    let (snapshot, branch_name) =
        tokio::task::spawn_blocking(move || -> Result<(GameSnapshot, String), String> {
            let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
            let (_, snapshot) = db
                .load_latest_snapshot(branch_id)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "No snapshots found on this branch.".to_string())?;
            let branches = db.list_branches().map_err(|e| e.to_string())?;
            let branch_name = branches
                .iter()
                .find(|b| b.id == branch_id)
                .map(|b| b.name.clone())
                .unwrap_or_else(|| "unknown".to_string());
            Ok((snapshot, branch_name))
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        snapshot.restore(&mut world, &mut npc_manager);
        npc_manager.assign_tiers(&world, &[]);

        let transport = state.transport.default_mode();
        let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        drop(npc_manager);
        drop(world);
        state.event_bus.emit("world-update", &ws);
    }

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    state.event_bus.emit(
        "text-log",
        &text_log(
            "system",
            format!("Loaded {} (branch: {}).", filename, branch_name),
        ),
    );

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some(branch_name);

    Ok(StatusCode::OK)
}

/// Request body for `POST /api/create-branch`.
#[derive(serde::Deserialize)]
pub struct CreateBranchRequest {
    /// Name for the new branch.
    pub name: String,
    /// Parent branch database id.
    #[serde(rename = "parentBranchId")]
    pub parent_branch_id: i64,
}

/// `POST /api/create-branch` — creates a new branch forked from a parent.
pub async fn create_branch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<CreateBranchRequest>,
) -> Result<Json<String>, (StatusCode, String)> {
    let msg = do_fork_branch_inner(&state, &body.name, body.parent_branch_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(msg))
}

/// `POST /api/new-save-file` — creates a new save file and saves current state.
pub async fn new_save_file(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    let saves_dir = state.saves_dir.clone();
    let path = new_save_path(&saves_dir);

    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to create main branch".to_string())?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(StatusCode::OK)
}

/// `POST /api/new-game` — reloads world/NPCs from data files and saves fresh state.
pub async fn new_game(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    do_new_game_inner(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.event_bus.emit(
        "text-log",
        &text_log("system", "A new chapter begins in the parish..."),
    );

    Ok(StatusCode::OK)
}

/// `GET /api/save-state` — returns the current save state for the StatusBar.
pub async fn get_save_state(Extension(state): Extension<Arc<AppState>>) -> Json<SaveState> {
    let save_path = state.save_path.lock().await;
    let branch_id = state.current_branch_id.lock().await;
    let branch_name = state.current_branch_name.lock().await;

    Json(SaveState {
        filename: save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()),
        branch_id: *branch_id,
        branch_name: branch_name.clone(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex as StdMutex};
    use std::time::{Duration, Instant};

    use parish_core::inference::{InferenceQueue, InferenceRequest, InferenceResponse};
    use parish_core::ipc::TextLogPayload;
    use parish_core::npc::Npc;
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::transport::TransportConfig;
    use parish_core::world::{LocationId, WorldState};

    #[test]
    fn submit_input_request_deserialization() {
        let json = r#"{"text": "go to church"}"#;
        let req: SubmitInputRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.text, "go to church");
        assert!(req.addressed_to.is_empty());
    }

    #[test]
    fn submit_input_request_with_addressed_to() {
        let json = r#"{"text": "hello", "addressedTo": ["Padraig", "Maire"]}"#;
        let req: SubmitInputRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.text, "hello");
        assert_eq!(req.addressed_to, vec!["Padraig", "Maire"]);
    }

    /// Helper to build a minimal AppState from the real game data.
    pub(crate) fn test_app_state() -> Arc<AppState> {
        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/rundale");
        let world =
            WorldState::from_parish_file(&data_dir.join("world.json"), LocationId(15)).unwrap();
        let npc_manager = NpcManager::new();
        let transport = TransportConfig::default();
        let ui_config = crate::state::UiConfigSnapshot {
            hints_label: "test".to_string(),
            default_accent: "#000".to_string(),
            splash_text: String::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
        };
        let theme_palette = parish_core::game_mod::default_theme_palette();
        let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../saves");
        crate::state::build_app_state(
            world,
            npc_manager,
            None,
            crate::state::GameConfig {
                provider_name: String::new(),
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
                category_provider: [None, None, None, None],
                category_model: [None, None, None, None],
                category_api_key: [None, None, None, None],
                category_base_url: [None, None, None, None],
                flags: parish_core::config::FeatureFlags::default(),
                category_rate_limit: [None, None, None, None],
                active_tile_source: String::new(),
                tile_sources: Vec::new(),
                reveal_unexplored_locations: false,
            },
            None,
            transport,
            ui_config,
            theme_palette,
            saves_dir,
            data_dir.clone(),
            None,
            data_dir.join("parish-flags.json"),
        )
    }

    async fn add_introduced_npc(state: &Arc<AppState>, id: u32, name: &str, occupation: &str) {
        let player_location = {
            let world = state.world.lock().await;
            world.player_location
        };

        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.occupation = occupation.to_string();
        npc.brief_description = format!("a {}", occupation.to_lowercase());
        npc.location = player_location;

        let mut npc_manager = state.npc_manager.lock().await;
        npc_manager.add_npc(npc);
        npc_manager.mark_introduced(NpcId(id));
    }

    async fn install_scripted_inference_queue(
        state: &Arc<AppState>,
        responses: Vec<&str>,
    ) -> (Arc<StdMutex<Vec<String>>>, tokio::task::JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel::<InferenceRequest>(8);
        let (bg_tx, _bg_rx) = mpsc::channel::<InferenceRequest>(8);
        let (batch_tx, _batch_rx) = mpsc::channel::<InferenceRequest>(8);
        let prompts = Arc::new(StdMutex::new(Vec::new()));
        let prompt_log = Arc::clone(&prompts);
        let mut scripted: VecDeque<String> = responses.into_iter().map(str::to_string).collect();

        let handle = tokio::spawn(async move {
            while let Some(request) = rx.recv().await {
                prompt_log.lock().unwrap().push(request.prompt.clone());

                let text = scripted.pop_front().unwrap_or_else(|| {
                    "Aye.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}".to_string()
                });

                let _ = request.response_tx.send(InferenceResponse {
                    id: request.id,
                    text,
                    error: None,
                });
            }
        });

        *state.inference_queue.lock().await = Some(InferenceQueue::new(tx, bg_tx, batch_tx));
        (prompts, handle)
    }

    fn drain_text_logs(
        rx: &mut tokio::sync::broadcast::Receiver<crate::state::ServerEvent>,
    ) -> Vec<TextLogPayload> {
        let mut logs = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(event) if event.event == "text-log" => {
                    logs.push(serde_json::from_value(event.payload).unwrap());
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            }
        }
        logs
    }

    /// Verifies that handle_movement resolves and applies movement atomically
    /// (clock advance + player_location update within a single lock scope).
    #[tokio::test]
    async fn handle_movement_updates_location_and_clock() {
        let state = test_app_state();

        let (start_loc, start_time) = {
            let world = state.world.lock().await;
            (world.player_location, world.clock.now())
        };

        // Move to the crossroads (a neighbor of Kilteevan Village, id 15)
        handle_movement("crossroads", &state).await;

        let world = state.world.lock().await;
        assert_ne!(
            world.player_location, start_loc,
            "player_location should change after movement"
        );
        // Clock should have advanced (travel takes > 0 minutes)
        assert!(
            world.clock.now() > start_time,
            "clock should advance during travel"
        );
    }

    /// Verifies that moving to an unknown location does not change world state.
    #[tokio::test]
    async fn handle_movement_unknown_destination_preserves_state() {
        let state = test_app_state();

        let (start_loc, start_time) = {
            let world = state.world.lock().await;
            (world.player_location, world.clock.now())
        };

        handle_movement("nonexistent-place-xyz", &state).await;

        let world = state.world.lock().await;
        assert_eq!(
            world.player_location, start_loc,
            "player_location should not change for unknown destination"
        );
        assert_eq!(
            world.clock.now(),
            start_time,
            "clock should not advance for unknown destination"
        );
    }

    #[test]
    fn text_log_generates_unique_ids() {
        let a = text_log("system", "hello");
        let b = text_log("system", "world");
        assert_ne!(a.id, b.id);
        assert!(a.id.starts_with("msg-"));
    }

    #[test]
    fn react_request_deserialization() {
        let json = r#"{"npcName": "Padraig", "messageSnippet": "Hello", "emoji": "😊"}"#;
        let req: ReactRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.npc_name, "Padraig");
        assert_eq!(req.emoji, "😊");
    }

    /// Verifies that get_save_state returns None fields on fresh AppState.
    #[tokio::test]
    async fn get_save_state_initial_is_empty() {
        let state = test_app_state();
        let result = get_save_state(axum::extract::Extension(state)).await;
        let save_state = result.0;
        assert!(save_state.filename.is_none());
        assert!(save_state.branch_id.is_none());
        assert!(save_state.branch_name.is_none());
    }

    /// Verifies that discover_save_files returns an empty list for a missing saves dir.
    #[tokio::test]
    async fn discover_save_files_empty_dir() {
        let state = test_app_state();
        // saves_dir points to ../../saves which may or may not exist — either way should not panic
        let result = discover_save_files(axum::extract::Extension(state)).await;
        assert!(result.is_ok());
    }

    /// Verifies request body deserialization for load_branch.
    #[test]
    fn load_branch_request_deserialization() {
        let json = r#"{"filePath": "/saves/parish_001.db", "branchId": 1}"#;
        let req: LoadBranchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.file_path, "/saves/parish_001.db");
        assert_eq!(req.branch_id, 1);
    }

    /// Verifies request body deserialization for create_branch.
    #[test]
    fn create_branch_request_deserialization() {
        let json = r#"{"name": "alternate", "parentBranchId": 1}"#;
        let req: CreateBranchRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "alternate");
        assert_eq!(req.parent_branch_id, 1);
    }

    #[tokio::test]
    async fn handle_npc_conversation_preserves_order_and_follow_up_context() {
        let state = test_app_state();
        add_introduced_npc(&state, 1, "Siobhan Murphy", "Teacher").await;
        add_introduced_npc(&state, 2, "Padraig Darcy", "Farmer").await;

        {
            let mut config = state.config.lock().await;
            config.model_name = "test-model".to_string();
            config.max_follow_up_turns = 0;
        }

        // Subscribe BEFORE the dispatch so we can count stream-end events.
        let mut rx = state.event_bus.subscribe();

        let (prompts, worker) = install_scripted_inference_queue(
            &state,
            vec![
                "I heard the fair will be lively.\n---\n{\"action\":\"speaks\",\"mood\":\"curious\"}",
                "If it is, Siobhan, I'll bring the cart.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
            ],
        )
        .await;

        handle_npc_conversation(
            "What news is there?".to_string(),
            vec!["Siobhan Murphy".to_string(), "Padraig Darcy".to_string()],
            &state,
        )
        .await;

        let transcript = {
            let conversation = state.conversation.lock().await;
            conversation.transcript.iter().cloned().collect::<Vec<_>>()
        };
        assert_eq!(
            transcript,
            vec![
                ConversationLine {
                    speaker: "You".to_string(),
                    text: "What news is there?".to_string(),
                },
                ConversationLine {
                    speaker: "Siobhan Murphy".to_string(),
                    text: "I heard the fair will be lively.".to_string(),
                },
                ConversationLine {
                    speaker: "Padraig Darcy".to_string(),
                    text: "If it is, Siobhan, I'll bring the cart.".to_string(),
                },
            ]
        );

        let prompts = prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2);
        // First prompt: the player's current input is excluded from the "Recent
        // conversation" section (it's shown separately as the triggering line via
        // build_named_action_line), so no transcript header appears.
        assert!(prompts[0].contains("The newcomer says: \"What news is there?\""));
        // Second prompt: includes Siobhan's prior response in transcript context.
        assert!(prompts[1].contains("Recent conversation here:"));
        assert!(prompts[1].contains("- Siobhan Murphy: I heard the fair will be lively."));

        // Regression guard: stream-end must fire EXACTLY ONCE for the whole
        // turn (addressed + follow-up), so the input field stays disabled
        // through every NPC's response. PR #222 emitted one per turn, which
        // let the input flicker open between NPCs and contradicted the
        // explicit user spec.
        let mut stream_end_count = 0;
        loop {
            match rx.try_recv() {
                Ok(event) if event.event == "stream-end" => stream_end_count += 1,
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => continue,
            }
        }
        assert_eq!(
            stream_end_count, 1,
            "expected exactly one stream-end after a 2-turn dispatch, got {}",
            stream_end_count
        );

        worker.abort();
    }

    #[tokio::test]
    async fn handle_npc_conversation_bystander_chain_picks_related_npc() {
        use parish_core::npc::types::{Relationship, RelationshipKind};

        let state = test_app_state();
        add_introduced_npc(&state, 1, "Siobhan Murphy", "Teacher").await;
        add_introduced_npc(&state, 2, "Padraig Darcy", "Farmer").await;
        add_introduced_npc(&state, 3, "Sean Brennan", "Smith").await;

        // Sean has a strong friendship with Padraig — when Padraig is the last
        // speaker, the heuristic should pick Sean for the autonomous chain
        // turn (not Siobhan, who has already spoken and is excluded by
        // `recently_spoken`).
        {
            let mut npc_manager = state.npc_manager.lock().await;
            if let Some(sean) = npc_manager.get_mut(NpcId(3)) {
                sean.relationships.insert(
                    NpcId(2),
                    Relationship {
                        kind: RelationshipKind::Friend,
                        strength: 0.7,
                        history: Vec::new(),
                    },
                );
            }
        }

        {
            let mut config = state.config.lock().await;
            config.model_name = "test-model".to_string();
            config.max_follow_up_turns = 1;
        }

        let (_prompts, worker) = install_scripted_inference_queue(
            &state,
            vec![
                "I heard the fair will be lively.\n---\n{\"action\":\"speaks\",\"mood\":\"curious\"}",
                "If it is, Siobhan, I'll bring the cart.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
                "I'd come too if my hand wasn't burnt at the forge.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
            ],
        )
        .await;

        handle_npc_conversation(
            "What news is there?".to_string(),
            vec!["Siobhan Murphy".to_string(), "Padraig Darcy".to_string()],
            &state,
        )
        .await;

        let transcript = {
            let conversation = state.conversation.lock().await;
            conversation.transcript.iter().cloned().collect::<Vec<_>>()
        };
        // Expect: player → Siobhan (addressed) → Padraig (addressed) → Sean (chain).
        assert_eq!(transcript.len(), 4, "transcript = {:?}", transcript);
        assert_eq!(transcript[3].speaker, "Sean Brennan");

        worker.abort();
    }

    #[tokio::test]
    async fn tick_inactivity_runs_idle_banter_before_auto_pause() {
        use parish_core::npc::types::{Relationship, RelationshipKind};

        let state = test_app_state();
        add_introduced_npc(&state, 1, "Siobhan Murphy", "Teacher").await;
        add_introduced_npc(&state, 2, "Padraig Darcy", "Farmer").await;

        // Padraig is friends with Siobhan so the heuristic will pick him for
        // the autonomous follow-up after Siobhan's first remark. Without this
        // relationship the chain would die after the first deterministic turn.
        {
            let mut npc_manager = state.npc_manager.lock().await;
            if let Some(padraig) = npc_manager.get_mut(NpcId(2)) {
                padraig.relationships.insert(
                    NpcId(1),
                    Relationship {
                        kind: RelationshipKind::Friend,
                        strength: 0.5,
                        history: Vec::new(),
                    },
                );
            }
        }

        {
            let mut config = state.config.lock().await;
            config.model_name = "test-model".to_string();
            config.max_follow_up_turns = 1;
            config.idle_banter_after_secs = 1;
            config.auto_pause_after_secs = 60;
        }

        let (prompts, worker) = install_scripted_inference_queue(
            &state,
            vec![
                "Quiet morning for it.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
                "Too quiet. Even the crows have given up.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
            ],
        )
        .await;

        let player_location = {
            let world = state.world.lock().await;
            world.player_location
        };
        {
            let mut conversation = state.conversation.lock().await;
            conversation.sync_location(player_location);
            let inactive_since = Instant::now() - Duration::from_secs(2);
            conversation.last_player_activity = inactive_since;
            conversation.last_spoken_at = inactive_since;
        }

        tick_inactivity(&state).await;

        let transcript = {
            let conversation = state.conversation.lock().await;
            conversation.transcript.iter().cloned().collect::<Vec<_>>()
        };
        assert_eq!(
            transcript,
            vec![
                ConversationLine {
                    speaker: "Siobhan Murphy".to_string(),
                    text: "Quiet morning for it.".to_string(),
                },
                ConversationLine {
                    speaker: "Padraig Darcy".to_string(),
                    text: "Too quiet. Even the crows have given up.".to_string(),
                },
            ]
        );
        assert!(!state.world.lock().await.clock.is_paused());

        let prompts = prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2);
        assert!(prompts[1].contains("Recent conversation here:"));
        assert!(prompts[1].contains("- Siobhan Murphy: Quiet morning for it."));

        worker.abort();
    }

    #[tokio::test]
    async fn tick_inactivity_auto_pauses_after_full_minute_of_silence() {
        let state = test_app_state();
        let mut rx = state.event_bus.subscribe();
        let player_location = {
            let world = state.world.lock().await;
            world.player_location
        };

        {
            let mut conversation = state.conversation.lock().await;
            conversation.sync_location(player_location);
            let inactive_since = Instant::now() - Duration::from_secs(61);
            conversation.last_player_activity = inactive_since;
            conversation.last_spoken_at = inactive_since;
        }

        tick_inactivity(&state).await;

        assert!(state.world.lock().await.clock.is_paused());

        let logs = drain_text_logs(&mut rx);
        assert!(logs.iter().any(|log| {
            log.content
                .contains("The parish falls quiet after a full minute of silence")
        }));
    }

    #[tokio::test]
    async fn load_branch_rejects_path_traversal() {
        let state = test_app_state();
        let body = LoadBranchRequest {
            file_path: "../../etc/passwd".to_string(),
            branch_id: 1,
        };
        let result = load_branch(Extension(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    /// Regression test for #224 / #231: rebuild_inference must abort the
    /// previously-stored inference worker, otherwise each provider/key/model
    /// change leaks a worker holding an HTTP client and channel state.
    #[tokio::test]
    async fn rebuild_inference_aborts_previous_worker() {
        let state = test_app_state();
        // Use the simulator so rebuild_inference doesn't try to talk to a real
        // LLM endpoint.
        {
            let mut config = state.config.lock().await;
            config.provider_name = "simulator".to_string();
        }

        // Spawn a sentinel "worker" that runs forever; mirror the real worker
        // by just sleeping in a loop. Stash an AbortHandle so we can verify
        // from outside whether rebuild_inference cancelled it.
        let sentinel = tokio::spawn(async {
            loop {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        });
        let abort_handle = sentinel.abort_handle();
        *state.worker_handle.lock().await = Some(sentinel);
        assert!(
            !abort_handle.is_finished(),
            "sentinel should be running before rebuild"
        );

        rebuild_inference(&state).await;

        // Yield + brief sleep so the runtime processes the abort.
        for _ in 0..10 {
            if abort_handle.is_finished() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(
            abort_handle.is_finished(),
            "rebuild_inference must abort the previous worker (#224, #231)"
        );

        // And a fresh worker handle must be stored.
        let wh = state.worker_handle.lock().await;
        assert!(
            wh.is_some(),
            "rebuild_inference must install a new worker handle"
        );
    }

    /// Regression test for #224 / #231: rebuild_inference must work (and
    /// install a worker) even when no previous worker was stored — that
    /// matches the case where startup failed to spawn one.
    #[tokio::test]
    async fn rebuild_inference_installs_worker_when_none_stored() {
        let state = test_app_state();
        {
            let mut config = state.config.lock().await;
            config.provider_name = "simulator".to_string();
        }
        assert!(state.worker_handle.lock().await.is_none());

        rebuild_inference(&state).await;

        assert!(
            state.worker_handle.lock().await.is_some(),
            "rebuild_inference must install a worker even if none was stored"
        );
        assert!(
            state.inference_queue.lock().await.is_some(),
            "rebuild_inference must install an inference queue"
        );
    }
}
