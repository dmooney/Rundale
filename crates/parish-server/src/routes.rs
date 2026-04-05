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

use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, new_inference_log, spawn_inference_worker};
use parish_core::input::{InputResult, classify_input, parse_intent_local};
use parish_core::ipc::{
    IDLE_MESSAGES, LoadingPayload, MapData, NpcInfo, NpcReactionPayload, ReactRequest,
    StreamEndPayload, StreamTokenPayload, ThemePalette, WorldSnapshot, capitalize_first, text_log,
};
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;

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
    Json(parish_core::ipc::build_map_data(
        &world,
        transport.speed_m_per_s,
    ))
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
    let inference = InferenceDebug {
        provider_name: config.provider_name.clone(),
        model_name: config.model_name.clone(),
        base_url: config.base_url.clone(),
        cloud_provider: config.cloud_provider_name.clone(),
        cloud_model: config.cloud_model_name.clone(),
        has_queue: state.inference_queue.lock().await.is_some(),
        improv_enabled: config.improv_enabled,
        call_log: Vec::new(),
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
pub struct SubmitInputRequest {
    /// The player's input text.
    pub text: String,
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
            handle_game_input(raw, &state).await;
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
    let _worker = spawn_inference_worker(new_client, rx, new_inference_log());
    let queue = InferenceQueue::new(tx);
    let mut iq = state.inference_queue.lock().await;
    *iq = Some(queue);
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
            CommandEffect::SaveGame
            | CommandEffect::ForkBranch(_)
            | CommandEffect::LoadBranch(_)
            | CommandEffect::ListBranches
            | CommandEffect::ShowLog => {
                state.event_bus.emit(
                    "text-log",
                    &text_log("system", "Persistence is not yet available in web mode."),
                );
            }
            CommandEffect::Debug(_) => {
                state.event_bus.emit(
                    "text-log",
                    &text_log("system", "Debug commands are not available in web mode."),
                );
            }
            CommandEffect::ShowSpinner(_) => {
                state.event_bus.emit(
                    "text-log",
                    &text_log(
                        "system",
                        "Spinner customization is not available in web mode.",
                    ),
                );
            }
            CommandEffect::NewGame => {
                state.event_bus.emit(
                    "text-log",
                    &text_log("system", "New game is not yet available in web mode."),
                );
            }
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

/// Handles free-form game input: parses intent then dispatches.
async fn handle_game_input(raw: String, state: &Arc<AppState>) {
    let intent = parse_intent_local(&raw);

    let is_move = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Move))
        .unwrap_or(false);
    let is_look = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Look))
        .unwrap_or(false);
    let move_target = intent
        .as_ref()
        .filter(|_i| is_move)
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

    handle_npc_conversation(raw, state).await;
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

        let (all_npcs, current_location_id, loc_name, tod, weather, introduced) = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
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
            )
        };

        let reaction_client = state.reaction_client.lock().await;
        let reaction_model = state.reaction_model.lock().await;
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
        drop(reaction_client);
        drop(reaction_model);

        for text in texts {
            state.event_bus.emit("text-log", &text_log("npc", text));
        }
    }

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
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
    );
    state.event_bus.emit("text-log", &text_log("system", text));
}

/// Routes input to the NPC at the player's location, or shows idle message.
async fn handle_npc_conversation(raw: String, state: &Arc<AppState>) {
    let (setup, queue, npc_present) = {
        let world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;

        let npc_present = !npc_manager.npcs_at(world.player_location).is_empty();
        let setup = parish_core::ipc::prepare_npc_conversation(
            &world,
            &mut npc_manager,
            &raw,
            None,
            config.improv_enabled,
        );
        (setup, queue.clone(), npc_present)
    };

    let (Some(setup), Some(queue)) = (setup, queue) else {
        // If an NPC is present but inference isn't configured, give a clear message.
        // Otherwise show a generic idle message.
        let content = if npc_present {
            "There's someone here, but the LLM is not configured — set a provider with /provider."
                .to_string()
        } else {
            let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % IDLE_MESSAGES.len();
            IDLE_MESSAGES[idx].to_string()
        };
        state
            .event_bus
            .emit("text-log", &text_log("system", content));
        return;
    };

    let npc_name = setup.display_name;
    let system_prompt = setup.system_prompt;
    let context = setup.context;

    let model = {
        let config = state.config.lock().await;
        config.model_name.clone()
    };
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    state
        .event_bus
        .emit("loading", &LoadingPayload { active: true });

    let (token_tx, token_rx) = mpsc::unbounded_channel::<String>();

    let display_label = capitalize_first(&npc_name);
    state
        .event_bus
        .emit("text-log", &text_log(display_label, String::new()));

    match queue
        .send(
            req_id,
            model,
            context,
            Some(system_prompt),
            Some(token_tx),
            None,
        )
        .await
    {
        Ok(mut response_rx) => {
            let bus = &state.event_bus;

            let stream_handle = tokio::spawn({
                let state_clone = Arc::clone(state);
                async move {
                    parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
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

            let full_response = loop {
                match response_rx.try_recv() {
                    Ok(resp) => {
                        let _ = stream_handle.await;
                        break Some(resp);
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
                    }
                    Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                        break None;
                    }
                }
            };

            let hints = if let Some(resp) = full_response {
                if resp.error.is_some() {
                    tracing::warn!("Inference error: {:?}", resp.error);
                    vec![]
                } else {
                    let parsed = parse_npc_stream_response(&resp.text);
                    parsed
                        .metadata
                        .map(|m| m.language_hints)
                        .unwrap_or_default()
                }
            } else {
                vec![]
            };

            bus.emit("stream-end", &StreamEndPayload { hints });
        }
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
        }
    }

    state
        .event_bus
        .emit("loading", &LoadingPayload { active: false });
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
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

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
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let filename = db_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let branch_name = branch_name_guard.as_deref().unwrap_or("main");

    Ok(Json(format!(
        "Game saved to {} (branch: {}).",
        filename, branch_name
    )))
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
    let save_path_guard = state.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "No active save file. Use /save first.".to_string(),
            )
        })?
        .clone();
    drop(save_path_guard);

    let name = body.name.clone();
    let parent_branch_id = body.parent_branch_id;
    let db_path_clone = db_path.clone();
    let name_clone = name.clone();

    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        db.create_branch(&name_clone, Some(parent_branch_id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let db_path_clone2 = db_path.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let db = Database::open(&db_path_clone2).map_err(|e| e.to_string())?;
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.clone());

    Ok(Json(format!("Created new branch '{}'.", name)))
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
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::{LocationId, WorldState};

    let data_dir = state.data_dir.clone();

    let fresh_world = WorldState::from_parish_file(&data_dir.join("parish.json"), LocationId(15))
        .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to load world: {}", e),
        )
    })?;

    let fresh_npcs = NpcManager::load_from_file(&data_dir.join("npcs.json"))
        .unwrap_or_else(|_| NpcManager::new());

    let snapshot = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        *world = fresh_world;
        *npc_manager = fresh_npcs;
        npc_manager.assign_tiers(&world, &[]);
        GameSnapshot::capture(&world, &npc_manager)
    };

    let saves_dir = state.saves_dir.clone();
    let path = new_save_path(&saves_dir);
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

    {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let transport = state.transport.default_mode();
        let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        state.event_bus.emit("world-update", &ws);
    }

    state.event_bus.emit(
        "text-log",
        &text_log("system", "A new chapter begins in the parish..."),
    );

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

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
    use parish_core::npc::manager::NpcManager;
    use parish_core::world::transport::TransportConfig;
    use parish_core::world::{LocationId, WorldState};

    #[test]
    fn submit_input_request_deserialization() {
        let json = r#"{"text": "go to church"}"#;
        let req: SubmitInputRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.text, "go to church");
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
                category_provider: [None, None, None],
                category_model: [None, None, None],
                category_api_key: [None, None, None],
                category_base_url: [None, None, None],
            },
            None,
            transport,
            ui_config,
            saves_dir,
            data_dir,
        )
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
}
