//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::Emitter;
use tokio::sync::mpsc;

use parish_core::debug_snapshot::{self, DebugEvent, DebugSnapshot, InferenceDebug};
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, spawn_inference_worker};
use parish_core::input::{InputResult, classify_input, extract_mention, parse_intent_local};
use parish_core::ipc::{IDLE_MESSAGES, capitalize_first, compute_name_hints, text_log};
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::world::palette::compute_palette;
use parish_core::world::transport::TransportMode;

use crate::events::{
    EVENT_SAVE_PICKER, EVENT_STREAM_END, EVENT_TEXT_LOG, EVENT_TRAVEL_START, EVENT_WORLD_UPDATE,
    NpcReactionPayload, StreamEndPayload, TextLogPayload, spawn_loading_animation,
};
use crate::{AppState, MapData, MapLocation, NpcInfo, SaveState, ThemePalette, WorldSnapshot};

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ── Helper: build a WorldSnapshot from locked world state ────────────────────

/// Builds a [`WorldSnapshot`] from a locked world state reference.
///
/// Used both by the `get_world_snapshot` command and by the background
/// idle-tick task in `lib.rs`. Includes name pronunciation hints when
/// NPC manager and pronunciation data are provided.
pub fn get_world_snapshot_inner(
    world: &parish_core::world::WorldState,
    transport: &TransportMode,
    npc_manager: Option<&parish_core::npc::manager::NpcManager>,
    pronunciations: &[parish_core::game_mod::PronunciationEntry],
) -> WorldSnapshot {
    let mut snapshot = snapshot_from_world(world, transport);
    if let Some(npc_mgr) = npc_manager {
        snapshot.name_hints = compute_name_hints(world, npc_mgr, pronunciations);
    }
    snapshot
}

/// Converts a core [`parish_core::ipc::WorldSnapshot`] into the Tauri-specific
/// [`WorldSnapshot`] (which includes additional fields like `name_hints`).
fn snapshot_from_world(
    world: &parish_core::world::WorldState,
    transport: &TransportMode,
) -> WorldSnapshot {
    let core = parish_core::ipc::snapshot_from_world(world, transport);
    WorldSnapshot {
        location_name: core.location_name,
        location_description: core.location_description,
        time_label: core.time_label,
        hour: core.hour,
        minute: core.minute,
        weather: core.weather,
        season: core.season,
        festival: core.festival,
        paused: core.paused,
        game_epoch_ms: core.game_epoch_ms,
        speed_factor: core.speed_factor,
        name_hints: vec![],
        day_of_week: core.day_of_week,
    }
}

// compute_name_hints is now shared via parish_core::ipc::compute_name_hints

// ── Commands ─────────────────────────────────────────────────────────────────

/// Returns a snapshot of the current world state (location, time, weather, season).
#[tauri::command]
pub async fn get_world_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<WorldSnapshot, String> {
    let world = state.world.lock().await;
    let transport = state.transport.default_mode();
    let npc_manager = state.npc_manager.lock().await;
    let mut snapshot = snapshot_from_world(&world, transport);
    snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
    Ok(snapshot)
}

/// Returns the map data: visited locations with coordinates, edges, and player position.
///
/// Includes visited locations (fully enriched) and the frontier — unvisited
/// locations adjacent to any visited location — so the player can see where
/// to explore next. Frontier locations are marked with `visited: false`.
#[tauri::command]
pub async fn get_map(state: tauri::State<'_, Arc<AppState>>) -> Result<MapData, String> {
    let world = state.world.lock().await;
    let speed = state.transport.default_mode().speed_m_per_s;
    let core_map = parish_core::ipc::build_map_data(&world, speed);

    let player_loc = world.player_location;
    let (player_lat, player_lon) = world
        .graph
        .get(player_loc)
        .map(|data| (data.lat, data.lon))
        .unwrap_or((0.0, 0.0));

    Ok(MapData {
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
    })
}

/// Returns the list of NPCs currently at the player's location.
#[tauri::command]
pub async fn get_npcs_here(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<NpcInfo>, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let npcs = npc_manager.npcs_at(world.player_location);
    Ok(npcs
        .into_iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            NpcInfo {
                name: npc_manager.display_name(npc).to_string(),
                occupation: npc.occupation.clone(),
                mood_emoji: parish_core::npc::mood::mood_emoji(&npc.mood).to_string(),
                mood: npc.mood.clone(),
                introduced,
            }
        })
        .collect())
}

/// Returns the current time-of-day theme palette as CSS hex colours.
#[tauri::command]
pub async fn get_theme(state: tauri::State<'_, Arc<AppState>>) -> Result<ThemePalette, String> {
    use chrono::Timelike;
    let world = state.world.lock().await;
    let now = world.clock.now();
    let raw = compute_palette(
        now.hour(),
        now.minute(),
        world.clock.season(),
        world.weather,
    );
    Ok(ThemePalette::from(raw))
}

/// Returns a debug snapshot of all game state for the debug panel.
///
/// Aggregates clock, world graph, NPC state, events, and inference config
/// into a single serializable [`DebugSnapshot`].
#[tauri::command]
pub async fn get_debug_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DebugSnapshot, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let events = state.debug_events.lock().await;
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
        improv_enabled: config.improv_enabled,
        call_log,
    };

    Ok(debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &inference,
    ))
}

/// Returns the UI configuration from the loaded game mod.
///
/// The frontend uses this to set sidebar labels, accent colours, etc.
#[tauri::command]
pub async fn get_ui_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::UiConfigSnapshot, String> {
    Ok(state.ui_config.clone())
}

/// Processes player text input: classification → movement, look, or NPC conversation.
///
/// Movement and look results are resolved synchronously. NPC conversations
/// submit an inference request and stream tokens back via `stream-token` events.
#[tauri::command]
pub async fn submit_input(
    text: String,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    if text.len() > 2000 {
        return Err("Input too long (max 2000 characters).".to_string());
    }

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, &state, &app).await;
        }
        InputResult::GameInput(raw) => {
            // Emit the player's own text as a dialogue bubble only for actual dialogue
            let player_msg = text_log("player", format!("> {}", raw));
            let player_msg_id = player_msg.id.clone();
            let _ = app.emit(EVENT_TEXT_LOG, player_msg);
            let raw_for_reactions = raw.clone();
            handle_game_input(raw, state.clone(), app.clone()).await;
            // Generate rule-based NPC reactions to the player's message
            emit_npc_reactions(&player_msg_id, &raw_for_reactions, &state, &app).await;
        }
    }

    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Replaces the client and respawns the inference worker so subsequent
/// NPC conversations use the new configuration.
async fn rebuild_inference(state: &Arc<AppState>) {
    let config = state.config.lock().await;
    let new_client = OpenAiClient::new(&config.base_url, config.api_key.as_deref());
    drop(config);

    let mut client_guard = state.client.lock().await;
    *client_guard = Some(new_client.clone());
    drop(client_guard);

    // Respawn inference worker with the new client
    let (tx, rx) = tokio::sync::mpsc::channel(32);
    let _worker = spawn_inference_worker(new_client, rx, state.inference_log.clone());
    let queue = InferenceQueue::new(tx);
    let mut iq = state.inference_queue.lock().await;
    *iq = Some(queue);
}

/// Handles `/command` inputs using the shared command handler.
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use parish_core::ipc::{CommandEffect, handle_command};

    // Run shared handler with all locks held.
    let result = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let mut config = state.config.lock().await;
        handle_command(cmd, &mut world, &mut npc_manager, &mut config)
    };

    // Handle mode-specific side effects.
    let mut extra_response: Option<String> = None;
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
                app.exit(0);
                return;
            }
            CommandEffect::ToggleMap => {
                let _ = app.emit(crate::events::EVENT_TOGGLE_MAP, ());
                return; // No text log for map toggle
            }
            CommandEffect::SaveGame => {
                extra_response = Some(match do_save_game(state).await {
                    Ok(msg) => msg,
                    Err(e) => format!("Save failed: {}", e),
                });
            }
            CommandEffect::ForkBranch(name) => {
                let parent_id = state.current_branch_id.lock().await.unwrap_or(1);
                extra_response = Some(match do_create_branch(state, name, parent_id).await {
                    Ok(msg) => msg,
                    Err(e) => format!("Fork failed: {}", e),
                });
            }
            CommandEffect::LoadBranch(_) => {
                let _ = app.emit(EVENT_SAVE_PICKER, ());
                extra_response = Some("Opening save picker...".to_string());
            }
            CommandEffect::ListBranches => {
                extra_response = Some(match do_list_branches_text(state).await {
                    Ok(text) => text,
                    Err(e) => format!("Failed to list branches: {}", e),
                });
            }
            CommandEffect::ShowLog => {
                extra_response = Some(match do_branch_log_text(state).await {
                    Ok(text) => text,
                    Err(e) => format!("Failed to show log: {}", e),
                });
            }
            CommandEffect::Debug(_) => {
                extra_response = Some("Debug commands are not available in the GUI.".to_string());
            }
            CommandEffect::ShowSpinner(secs) => {
                let app_handle = app.clone();
                let cancel = tokio_util::sync::CancellationToken::new();
                spawn_loading_animation(app_handle, cancel.clone());
                let secs = *secs;
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                    cancel.cancel();
                });
                extra_response = Some(format!("Showing spinner for {} seconds…", secs));
            }
            CommandEffect::NewGame => {
                extra_response = Some("New game is not yet available in the GUI.".to_string());
            }
        }
    }

    // Emit the command response text (shared response or mode-specific override).
    let response = extra_response.unwrap_or(result.response);
    if !response.is_empty() {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                source: "system".to_string(),
                content: response,
            },
        );
    }

    // Emit updated world state for status bar.
    {
        let world = state.world.lock().await;
        let transport = state.transport.default_mode();
        let npc_manager = state.npc_manager.lock().await;
        let mut snapshot = snapshot_from_world(&world, transport);
        snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
        let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    }
}

/// Handles free-form game input: parses intent then dispatches.
async fn handle_game_input(
    raw: String,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
    // Use local keyword-based parser first (no LLM latency)
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
            handle_movement(&target, &state, &app).await;
        } else {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    source: "system".to_string(),
                    content: "And where would ye be off to?".to_string(),
                },
            );
        }
        return;
    }

    if is_look {
        handle_look(&state, &app).await;
        return;
    }

    // Extract @mention for NPC targeting, if present
    let (target_name, dialogue) = match extract_mention(&raw) {
        Some(mention) => (Some(mention.name), mention.remaining),
        None => (None, raw),
    };

    // Try NPC conversation
    handle_npc_conversation(dialogue, target_name, state, app).await;
}

/// Resolves movement to a named location using the shared movement pipeline.
///
/// Delegates all state mutation and message generation to
/// [`parish_core::game_session::apply_movement`], then emits the returned
/// effects to the frontend.
async fn handle_movement(target: &str, state: &Arc<AppState>, app: &tauri::AppHandle) {
    use parish_core::game_session::apply_movement;

    let transport = state.transport.default_mode().clone();

    // Apply all movement state changes within a single lock scope to prevent
    // TOCTOU races.
    let effects = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        apply_movement(
            &mut world,
            &mut npc_manager,
            &state.reaction_templates,
            target,
            &transport,
        )
    };

    // Emit travel-start animation payload first
    if let Some(travel_payload) = &effects.travel_start {
        let _ = app.emit(EVENT_TRAVEL_START, travel_payload);
    }

    // Emit all player-visible messages in order
    for msg in &effects.messages {
        let _ = app.emit(EVENT_TEXT_LOG, text_log(msg.source, &msg.text));
    }

    // Emit NPC arrival reactions — upgrade to LLM text where available
    if !effects.arrival_reactions.is_empty() {
        use parish_core::game_session::resolve_reaction_texts;

        let (all_npcs, current_location_id, loc_name, tod, weather, introduced, model) = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let config = state.config.lock().await;
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
                config.model_name.clone(),
            )
        };

        let client_guard = state.client.lock().await;
        let texts = resolve_reaction_texts(
            &effects.arrival_reactions,
            &all_npcs,
            current_location_id,
            &loc_name,
            tod,
            &weather,
            &introduced,
            client_guard.as_ref(),
            &model,
            Some(&state.inference_log),
        )
        .await;
        drop(client_guard);

        for text in texts {
            let _ = app.emit(EVENT_TEXT_LOG, text_log("npc", &text));
        }
    }

    // Record tier transitions in the debug event log
    if !effects.tier_transitions.is_empty() {
        let mut debug_events = state.debug_events.lock().await;
        for tt in &effects.tier_transitions {
            if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                debug_events.pop_front();
            }
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            debug_events.push_back(DebugEvent {
                timestamp: String::new(),
                category: "tier".to_string(),
                message: format!(
                    "{} {} {:?} → {:?}",
                    tt.npc_name, direction, tt.old_tier, tt.new_tier,
                ),
            });
        }
    }

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let mut snapshot = snapshot_from_world(&world, &transport);
        snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
        let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    }
}

/// Renders the current location description and exits.
async fn handle_look(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let text = parish_core::ipc::render_look_text(
        &world,
        &npc_manager,
        transport.speed_m_per_s,
        &transport.label,
    );
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            source: "system".to_string(),
            content: text,
        },
    );
}

/// Routes input to the NPC at the player's location, or shows idle message.
///
/// If `target_name` is provided (from an `@mention`), the matching NPC
/// is selected. Otherwise falls back to the first NPC at the location.
async fn handle_npc_conversation(
    raw: String,
    target_name: Option<String>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
    let (setup, queue) = {
        let world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;

        let setup = parish_core::ipc::prepare_npc_conversation(
            &world,
            &mut npc_manager,
            &raw,
            target_name.as_deref(),
            config.improv_enabled,
        );
        (setup, queue.clone())
    };

    let (Some(setup), Some(queue)) = (setup, queue) else {
        // No NPC present or no inference queue — show idle message
        let idx = REQUEST_ID.fetch_add(1, Ordering::Relaxed) as usize % IDLE_MESSAGES.len();
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                source: "system".to_string(),
                content: IDLE_MESSAGES[idx].to_string(),
            },
        );
        return;
    };

    let npc_name = setup.display_name;
    let system_prompt = setup.system_prompt;
    let context = setup.context;

    let model = {
        let config = state.config.lock().await;
        config.model_name.clone()
    };
    let req_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);

    // Spawn the animated loading indicator (fun Irish phrases)
    let loading_cancel = tokio_util::sync::CancellationToken::new();
    spawn_loading_animation(app.clone(), loading_cancel.clone());

    let (token_tx, token_rx) = mpsc::unbounded_channel::<String>();

    // Emit NPC name prefix as the start of the streaming entry
    let display_label = capitalize_first(&npc_name);
    let _ = app.emit(EVENT_TEXT_LOG, text_log(display_label, String::new()));

    // Pause the game clock while waiting for the inference response
    // and immediately notify the frontend so it stops interpolating.
    {
        let mut world = state.world.lock().await;
        world.clock.inference_pause();
        let transport = state.transport.default_mode();
        let npc_manager = state.npc_manager.lock().await;
        let mut snapshot = snapshot_from_world(&world, transport);
        snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
        let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    }

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
            let app_clone = app.clone();

            // Stream tokens to the frontend
            let stream_handle = tokio::spawn(async move {
                crate::events::stream_npc_response(app_clone, token_rx).await
            });

            // Wait for the complete response
            let full_response = match response_rx.try_recv() {
                Ok(resp) => {
                    let _ = stream_handle.await;
                    Some(resp)
                }
                Err(_) => {
                    // Poll until done
                    loop {
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
                    }
                }
            };

            // Parse Irish word hints from the complete response
            let hints = if let Some(resp) = full_response {
                if let Some(ref err) = resp.error {
                    tracing::warn!("Inference error: {:?}", err);

                    // Log actual error to the debug events panel
                    let mut events = state.debug_events.lock().await;
                    if events.len() >= crate::DEBUG_EVENT_CAPACITY {
                        events.pop_front();
                    }
                    events.push_back(DebugEvent {
                        timestamp: String::new(),
                        category: "inference".to_string(),
                        message: format!("Dialogue error: {err}"),
                    });

                    // Show a funny canned message to the player
                    let canned = [
                        "A sudden fog rolls in and swallows the conversation whole.",
                        "A crow lands between you, caws loudly, and the moment is lost.",
                        "The wind picks up and carries their words clean away.",
                        "They open their mouth to speak, but a donkey brays so loud neither of ye can hear a thing.",
                        "A clap of thunder rattles the sky and ye both forget what ye were talking about.",
                        "They stare at you blankly, as if the thought simply left their head.",
                        "A strange silence falls over the parish. Even the birds have stopped.",
                    ];
                    let idx = resp.id as usize % canned.len();
                    let _ = app.emit(
                        EVENT_TEXT_LOG,
                        TextLogPayload {
                            id: String::new(),
                            source: "system".to_string(),
                            content: canned[idx].to_string(),
                        },
                    );

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

            let _ = app.emit(EVENT_STREAM_END, StreamEndPayload { hints });
        }
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);

            // Log to debug events
            let mut events = state.debug_events.lock().await;
            if events.len() >= crate::DEBUG_EVENT_CAPACITY {
                events.pop_front();
            }
            events.push_back(DebugEvent {
                timestamp: String::new(),
                category: "inference".to_string(),
                message: format!("Queue submit failed: {e}"),
            });

            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    source: "system".to_string(),
                    content: "The parish storyteller has wandered off. Try again in a moment."
                        .to_string(),
                },
            );
        }
    }

    // Resume the game clock now that inference is complete
    {
        let mut world = state.world.lock().await;
        world.clock.inference_resume();
    }

    // Stop the animated loading indicator (emits active: false)
    loading_cancel.cancel();
}

// ── Persistence commands ────────────────────────────────────────────────────

use parish_core::persistence::Database;
use parish_core::persistence::picker::{
    SaveFileInfo, discover_saves, ensure_saves_dir, new_save_path,
};
use parish_core::persistence::snapshot::GameSnapshot;

/// Resolves the saves directory relative to the data directory.
fn saves_dir() -> std::path::PathBuf {
    let mut p = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    for _ in 0..4 {
        if p.join("data/parish.json").exists() {
            let sd = p.join("saves");
            std::fs::create_dir_all(&sd).ok();
            return sd;
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    // Fallback: use ensure_saves_dir which creates ./saves
    ensure_saves_dir()
}

/// Returns the list of save files with branch metadata.
#[tauri::command]
pub async fn discover_save_files(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileInfo>, String> {
    let world = state.world.lock().await;
    let sd = saves_dir();
    let saves = discover_saves(&sd, &world.graph);
    for s in &saves {
        tracing::info!(
            "Save file: {} — {} branches: {:?}",
            s.filename,
            s.branches.len(),
            s.branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
    }
    Ok(saves)
}

/// Saves the current game state to the active save file and branch.
///
/// If no save file is active, creates a new one.
#[tauri::command]
pub async fn save_game(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    do_save_game(&state).await
}

/// Internal save implementation shared by the command and /save handler.
async fn do_save_game(state: &Arc<AppState>) -> Result<String, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let mut save_path_guard = state.save_path.lock().await;
    let mut branch_id_guard = state.current_branch_id.lock().await;
    let mut branch_name_guard = state.current_branch_name.lock().await;

    let db_path = if let Some(ref path) = *save_path_guard {
        path.clone()
    } else {
        // Create a new save file
        let sd = saves_dir();
        let path = new_save_path(&sd);
        *save_path_guard = Some(path.clone());
        path
    };

    let db = Database::open(&db_path).map_err(|e| e.to_string())?;

    let branch_id = if let Some(id) = *branch_id_guard {
        id
    } else {
        let branch = db.find_branch("main").map_err(|e| e.to_string())?;
        let id = branch.map(|b| b.id).unwrap_or(1);
        *branch_id_guard = Some(id);
        *branch_name_guard = Some("main".to_string());
        id
    };

    db.save_snapshot(branch_id, &snapshot)
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

/// Loads a branch from a save file, restoring world and NPC state.
#[tauri::command]
pub async fn load_branch(
    file_path: String,
    branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let path = std::path::PathBuf::from(&file_path);
    let db = Database::open(&path).map_err(|e| e.to_string())?;

    let (_, snapshot) = db
        .load_latest_snapshot(branch_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No snapshots found on this branch.".to_string())?;

    // Find the branch name
    let branches = db.list_branches().map_err(|e| e.to_string())?;
    let branch_name = branches
        .iter()
        .find(|b| b.id == branch_id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // Restore state
    let mut world = state.world.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    snapshot.restore(&mut world, &mut npc_manager);
    npc_manager.assign_tiers(&world, &[]);

    // Update save tracking
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Emit updated state to frontend (compute name hints before dropping locks)
    let transport = state.transport.default_mode();
    let mut ws = snapshot_from_world(&world, transport);
    ws.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
    drop(npc_manager);
    let _ = app.emit(EVENT_WORLD_UPDATE, ws);
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            source: "system".to_string(),
            content: format!("Loaded {} (branch: {}).", filename, branch_name),
        },
    );

    drop(world);

    // Update persistence tracking
    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some(branch_name);

    Ok(())
}

/// Creates a new branch forked from a specified parent branch.
#[tauri::command]
pub async fn create_branch(
    name: String,
    parent_branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    do_create_branch(&state, &name, parent_branch_id).await
}

/// Internal fork implementation shared by the command and /fork handler.
async fn do_create_branch(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    let save_path_guard = state.save_path.lock().await;

    let db_path = save_path_guard
        .as_ref()
        .ok_or("No active save file. Use /save first.")?;

    let db_path_clone = db_path.clone();
    let db = Database::open(db_path).map_err(|e| e.to_string())?;

    tracing::info!(
        "Creating branch '{}' with parent {} in {:?}",
        name,
        parent_branch_id,
        db_path_clone
    );

    let new_id = db
        .create_branch(name, Some(parent_branch_id))
        .map_err(|e| {
            tracing::error!("create_branch failed: {}", e);
            e.to_string()
        })?;

    tracing::info!("Branch '{}' created with id {}", name, new_id);

    drop(save_path_guard);

    // Save current state to the new branch
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    db.save_snapshot(new_id, &snapshot)
        .map_err(|e| e.to_string())?;

    tracing::info!("Snapshot saved to branch '{}'", name);

    // Switch to the new branch
    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.to_string());

    Ok(format!("Created new branch '{}'.", name))
}

/// Creates a new save file and saves the current state.
#[tauri::command]
pub async fn new_save_file(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let sd = saves_dir();
    let path = new_save_path(&sd);
    let db = Database::open(&path).map_err(|e| e.to_string())?;

    let branch = db
        .find_branch("main")
        .map_err(|e| e.to_string())?
        .ok_or("Failed to create main branch")?;

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    db.save_snapshot(branch.id, &snapshot)
        .map_err(|e| e.to_string())?;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch.id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Starts a brand new game: reloads world and NPCs from data files,
/// creates a new save file, and saves the fresh initial state.
#[tauri::command]
pub async fn new_game(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let data_dir = crate::find_data_dir();

    // Reload fresh world and NPCs from data files
    let fresh_world = parish_core::world::WorldState::from_parish_file(
        &data_dir.join("parish.json"),
        parish_core::world::LocationId(15),
    )
    .map_err(|e| format!("Failed to load parish.json: {}", e))?;

    let mut fresh_npcs =
        parish_core::npc::manager::NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|_| parish_core::npc::manager::NpcManager::new());

    fresh_npcs.assign_tiers(&fresh_world, &[]);

    // Replace live state
    let mut world = state.world.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    *world = fresh_world;
    *npc_manager = fresh_npcs;

    // Create a new save file with the fresh state
    let sd = saves_dir();
    let path = new_save_path(&sd);
    let db = Database::open(&path).map_err(|e| e.to_string())?;
    let branch = db
        .find_branch("main")
        .map_err(|e| e.to_string())?
        .ok_or("Failed to create main branch")?;

    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    db.save_snapshot(branch.id, &snapshot)
        .map_err(|e| e.to_string())?;

    // Emit updated state
    let transport = state.transport.default_mode();
    let mut ws = snapshot_from_world(&world, transport);
    ws.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
    let _ = app.emit(EVENT_WORLD_UPDATE, ws);
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            source: "system".to_string(),
            content: "A new chapter begins in the parish...".to_string(),
        },
    );

    drop(npc_manager);
    drop(world);

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch.id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Returns the current save state for display in the StatusBar.
#[tauri::command]
pub async fn get_save_state(state: tauri::State<'_, Arc<AppState>>) -> Result<SaveState, String> {
    let save_path = state.save_path.lock().await;
    let branch_id = state.current_branch_id.lock().await;
    let branch_name = state.current_branch_name.lock().await;

    Ok(SaveState {
        filename: save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()),
        branch_id: *branch_id,
        branch_name: branch_name.clone(),
    })
}

/// Formats branch list as text for the /branches command.
async fn do_list_branches_text(state: &Arc<AppState>) -> Result<String, String> {
    let save_path = state.save_path.lock().await;
    let db_path = save_path
        .as_ref()
        .ok_or("No active save file. Use /save first.")?;
    let db = Database::open(db_path).map_err(|e| e.to_string())?;
    let branches = db.list_branches().map_err(|e| e.to_string())?;

    let current_id = *state.current_branch_id.lock().await;

    let mut lines = vec!["Branches:".to_string()];
    for b in &branches {
        let marker = if Some(b.id) == current_id { " *" } else { "" };
        let parent = b
            .parent_branch_id
            .and_then(|pid| branches.iter().find(|bb| bb.id == pid))
            .map(|bb| format!(" (from {})", bb.name))
            .unwrap_or_default();
        lines.push(format!("  {}{}{}", b.name, parent, marker));
    }
    Ok(lines.join("\n"))
}

/// Formats branch log as text for the /log command.
async fn do_branch_log_text(state: &Arc<AppState>) -> Result<String, String> {
    let save_path = state.save_path.lock().await;
    let branch_id = state.current_branch_id.lock().await;

    let db_path = save_path
        .as_ref()
        .ok_or("No active save file. Use /save first.")?;
    let bid = branch_id.ok_or("No active branch.")?;

    let db = Database::open(db_path).map_err(|e| e.to_string())?;
    let log = db.branch_log(bid).map_err(|e| e.to_string())?;

    if log.is_empty() {
        return Ok("No snapshots yet on this branch.".to_string());
    }

    let branch_name = state.current_branch_name.lock().await;
    let name = branch_name.as_deref().unwrap_or("unknown");

    let mut lines = vec![format!("Save log for branch '{}':", name)];
    for (i, info) in log.iter().enumerate() {
        let time = parish_core::persistence::format_timestamp(&info.real_time);
        lines.push(format!("  {}. {} (game: {})", i + 1, time, info.game_time));
    }
    Ok(lines.join("\n"))
}

// ── Reaction commands ──────────────────────────────────────────────────────

/// Player reacts to an NPC message with an emoji.
#[tauri::command]
pub async fn react_to_message(
    npc_name: String,
    message_snippet: String,
    emoji: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    // Validate emoji is in the palette
    if reactions::reaction_description(&emoji).is_none() {
        return Err("Unknown reaction emoji.".to_string());
    }

    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log.add(&emoji, &message_snippet, now);
    }

    Ok(())
}

/// Generates rule-based NPC reactions to a player message and emits events.
async fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
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
            let _ = app.emit(
                crate::events::EVENT_NPC_REACTION,
                NpcReactionPayload {
                    message_id: player_msg_id.to_string(),
                    emoji,
                    source: capitalize_first(&name),
                },
            );
        }
    }
}
