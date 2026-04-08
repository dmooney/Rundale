//! HTTP route handlers for the Parish web server.
//!
//! Each route maps to a Tauri command, calling the shared handlers in
//! [`parish_core::ipc`] and returning JSON responses.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use tokio::sync::mpsc;

use parish_core::config::InferenceCategory;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, spawn_inference_worker};
use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::{
    ConversationLine, IDLE_MESSAGES, INFERENCE_FAILURE_MESSAGES, LoadingPayload, MapData, NpcInfo,
    NpcReactionPayload, ReactRequest, StreamEndPayload, StreamTokenPayload, ThemePalette,
    WorldSnapshot, capitalize_first, text_log,
};
use parish_core::npc::NpcId;
use parish_core::npc::manager::NpcManager;
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::npc::ticks::apply_tier1_response;
use parish_core::world::{LocationId, WorldState};

use parish_core::debug_snapshot::{self, DebugSnapshot, InferenceDebug};
use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

use crate::state::{AppState, SaveState};

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ── Query endpoints ─────────────────────────────────────────────────────────

/// `GET /api/world-snapshot` — returns the current world snapshot.
pub async fn get_world_snapshot(State(state): State<Arc<AppState>>) -> Json<WorldSnapshot> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let mut snapshot = parish_core::ipc::snapshot_from_world(&world, transport);
    snapshot.name_hints =
        parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
    Json(snapshot)
}

/// `GET /api/map` — returns visited locations, edges, and player position.
pub async fn get_map(State(state): State<Arc<AppState>>) -> Json<MapData> {
    let world = state.world.lock().await;
    let transport = state.transport.default_mode();
    Json(parish_core::ipc::build_map_data(&world, transport))
}

/// `GET /api/npcs-here` — returns NPCs at the player's current location.
pub async fn get_npcs_here(State(state): State<Arc<AppState>>) -> Json<Vec<NpcInfo>> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Json(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

/// `GET /api/theme` — returns the current time-of-day theme palette.
pub async fn get_theme(State(state): State<Arc<AppState>>) -> Json<ThemePalette> {
    let world = state.world.lock().await;
    Json(parish_core::ipc::build_theme(&world))
}

/// `GET /api/ui-config` — returns UI configuration (splash text, labels, accent).
pub async fn get_ui_config(
    State(state): State<Arc<AppState>>,
) -> Json<crate::state::UiConfigSnapshot> {
    Json(state.ui_config.clone())
}

/// `GET /api/debug-snapshot` — returns full debug state for the debug panel.
pub async fn get_debug_snapshot(State(state): State<Arc<AppState>>) -> Json<DebugSnapshot> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let config = state.config.lock().await;
    let events = std::collections::VecDeque::new();
    let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
        state.inference_log.lock().await.iter().cloned().collect();
    let inference = InferenceDebug {
        provider_name: config.provider_name.clone(),
        model_name: config.model_name.clone(),
        base_url: config.base_url.clone(),
        cloud_provider: config.cloud_provider_name.clone(),
        cloud_model: config.cloud_model_name.clone(),
        has_queue: state.inference_queue.lock().await.is_some(),
        improv_enabled: config.improv_enabled,
        call_log,
    };
    Json(debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &inference,
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
    State(state): State<Arc<AppState>>,
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
    let new_client = {
        let config = state.config.lock().await;
        OpenAiClient::new(&config.base_url, config.api_key.as_deref())
    };

    {
        let mut client_guard = state.client.lock().await;
        *client_guard = Some(new_client.clone());
    }

    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let _worker = spawn_inference_worker(new_client, rx, state.inference_log.clone());
    let queue = InferenceQueue::new(tx);
    let mut iq = state.inference_queue.lock().await;
    *iq = Some(queue);
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

fn build_turn_order(targets: &[NpcId], max_follow_up_turns: usize) -> Vec<(NpcId, bool)> {
    let mut turns: Vec<(NpcId, bool)> = targets.iter().copied().map(|id| (id, false)).collect();
    if targets.len() >= 2 {
        for idx in 0..max_follow_up_turns {
            turns.push((targets[idx % targets.len()], true));
        }
    }
    turns
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
        }
    }

    // Emit the command response text.
    if !result.response.is_empty() {
        state
            .event_bus
            .emit("text-log", &text_log("system", result.response));
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

    // Emit NPC arrival reactions — upgrade to LLM text where available
    if !effects.arrival_reactions.is_empty() {
        use parish_core::game_session::resolve_reaction_texts;

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

        let texts = resolve_reaction_texts(
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
        )
        .await;

        for text in texts {
            state.event_bus.emit("text-log", &text_log("npc", text));
        }
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
        let world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let config = state.config.lock().await;
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
    state
        .event_bus
        .emit("text-log", &text_log(display_label.clone(), String::new()));

    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
    let send_result = queue
        .send(
            req_id,
            model.to_string(),
            setup.context,
            Some(setup.system_prompt),
            Some(token_tx),
            None,
        )
        .await;

    let response_rx = match send_result {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
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
        async move {
            parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
                cancel.cancel();
                state_clone.event_bus.emit(
                    "stream-token",
                    &StreamTokenPayload {
                        token: batch.to_string(),
                    },
                );
            })
            .await
        }
    });

    let response = response_rx.await.ok();
    let _ = stream_handle.await;
    loading_cancel.cancel();

    let Some(response) = response else {
        state.event_bus.emit(
            "text-log",
            &text_log("system", "The storyteller has wandered off mid-tale."),
        );
        return None;
    };

    if response.error.is_some() {
        tracing::warn!("Inference error: {:?}", response.error);
        let idx = response.id as usize % INFERENCE_FAILURE_MESSAGES.len();
        state.event_bus.emit(
            "text-log",
            &text_log("system", INFERENCE_FAILURE_MESSAGES[idx]),
        );
        return None;
    }

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
        if let Some(npc) = npc_manager.get_mut(speaker_id) {
            let _ = apply_tier1_response(npc, &parsed, prompt_input, game_time);
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

    for (speaker_id, follow_up) in build_turn_order(&targets, max_follow_up_turns) {
        let prompt = if follow_up {
            "listens while the nearby conversation continues"
        } else {
            trimmed.as_str()
        };

        let Some(outcome) =
            run_npc_turn(state, &queue, &model, speaker_id, prompt, &transcript).await
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

    for (speaker_id, follow_up) in build_turn_order(&speakers, max_follow_up_turns) {
        let prompt = if follow_up {
            "answers the nearby remark and keeps the local chatter going"
        } else {
            "breaks the silence with a natural nearby remark"
        };
        let Some(outcome) =
            run_npc_turn(state, &queue, &model, speaker_id, prompt, &transcript).await
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
    State(state): State<Arc<AppState>>,
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

    // Load fresh world and NPCs
    let world = WorldState::from_parish_file(&data_dir.join("parish.json"), LocationId(15))
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to load parish.json: {}. Using default world.", e);
            WorldState::new()
        });

    let mut npc_manager =
        NpcManager::load_from_file(&data_dir.join("npcs.json")).unwrap_or_else(|e| {
            tracing::warn!("Failed to load npcs.json: {}. No NPCs.", e);
            NpcManager::new()
        });
    npc_manager.assign_tiers(&world, &[]);

    // Replace state
    {
        let mut w = state.world.lock().await;
        *w = world;
    }
    {
        let mut nm = state.npc_manager.lock().await;
        *nm = npc_manager;
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
    State(state): State<Arc<AppState>>,
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

/// `GET /api/save-game` — saves the current game state to the active save file.
pub async fn save_game(
    State(state): State<Arc<AppState>>,
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
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoadBranchRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let path = std::path::PathBuf::from(&body.file_path);
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
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateBranchRequest>,
) -> Result<Json<String>, (StatusCode, String)> {
    let msg = do_fork_branch_inner(&state, &body.name, body.parent_branch_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(msg))
}

/// `GET /api/new-save-file` — creates a new save file and saves current state.
pub async fn new_save_file(
    State(state): State<Arc<AppState>>,
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

/// `GET /api/new-game` — reloads world/NPCs from data files and saves fresh state.
pub async fn new_game(
    State(state): State<Arc<AppState>>,
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
pub async fn get_save_state(State(state): State<Arc<AppState>>) -> Json<SaveState> {
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
mod tests {
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
    fn test_app_state() -> Arc<AppState> {
        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mods/kilteevan-1820");
        let world =
            WorldState::from_parish_file(&data_dir.join("world.json"), LocationId(15)).unwrap();
        let npc_manager = NpcManager::new();
        let transport = TransportConfig::default();
        let ui_config = crate::state::UiConfigSnapshot {
            hints_label: "test".to_string(),
            default_accent: "#000".to_string(),
            splash_text: String::new(),
        };
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
            },
            None,
            transport,
            ui_config,
            saves_dir,
            data_dir,
            None,
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

        *state.inference_queue.lock().await = Some(InferenceQueue::new(tx));
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
        let result = get_save_state(axum::extract::State(state)).await;
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
        let result = discover_save_files(axum::extract::State(state)).await;
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
            config.max_follow_up_turns = 1;
        }

        // Subscribe BEFORE the dispatch so we can count stream-end events.
        let mut rx = state.event_bus.subscribe();

        let (prompts, worker) = install_scripted_inference_queue(
            &state,
            vec![
                "I heard the fair will be lively.\n---\n{\"action\":\"speaks\",\"mood\":\"curious\"}",
                "If it is, Siobhan, I'll bring the cart.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
                "Then we'd best leave early, Padraig.\n---\n{\"action\":\"speaks\",\"mood\":\"content\"}",
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
                ConversationLine {
                    speaker: "Siobhan Murphy".to_string(),
                    text: "Then we'd best leave early, Padraig.".to_string(),
                },
            ]
        );

        let prompts = prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3);
        assert!(prompts[0].contains("Recent conversation here:"));
        assert!(prompts[0].contains("- You: What news is there?"));
        assert!(prompts[1].contains("- Siobhan Murphy: I heard the fair will be lively."));
        assert!(prompts[2].contains("- Padraig Darcy: If it is, Siobhan, I'll bring the cart."));

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
            "expected exactly one stream-end after a 3-turn dispatch, got {}",
            stream_end_count
        );

        worker.abort();
    }

    #[tokio::test]
    async fn tick_inactivity_runs_idle_banter_before_auto_pause() {
        let state = test_app_state();
        add_introduced_npc(&state, 1, "Siobhan Murphy", "Teacher").await;
        add_introduced_npc(&state, 2, "Padraig Darcy", "Farmer").await;

        {
            let mut config = state.config.lock().await;
            config.model_name = "test-model".to_string();
            config.max_follow_up_turns = 0;
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
}
