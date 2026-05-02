//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::Emitter;
use tokio::sync::{Semaphore, mpsc};

/// Maximum number of NPC LLM inference calls that may run concurrently within
/// a single `emit_npc_reactions` batch (#406).
const NPC_REACTION_CONCURRENCY: usize = 4;

use parish_core::config::InferenceCategory;
use parish_core::debug_snapshot::{self, AuthDebug, DebugEvent, DebugSnapshot, InferenceDebug};
use parish_core::inference::{
    AnyClient, INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferenceQueue,
    await_inference_response, spawn_inference_worker,
};
use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::{
    ConversationLine, IDLE_MESSAGES, INFERENCE_FAILURE_MESSAGES, capitalize_first,
    compute_name_hints, text_log, text_log_for_stream_turn, text_log_typed,
};
use parish_core::npc::NpcId;
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::npc::ticks::apply_tier1_response_with_config;
use parish_core::world::LocationId;
use parish_core::world::transport::TransportMode;

use crate::events::{
    EVENT_SAVE_PICKER, EVENT_STREAM_END, EVENT_STREAM_TOKEN, EVENT_STREAM_TURN_END, EVENT_TEXT_LOG,
    EVENT_TRAVEL_START, EVENT_WORLD_UPDATE, NpcReactionPayload, StreamEndPayload,
    StreamTokenPayload, StreamTurnEndPayload, TextLogPayload, spawn_loading_animation,
};
use crate::{
    AppState, ConversationRuntimeState, MapData, MapLocation, NpcInfo, SaveState, ThemePalette,
    WorldSnapshot,
};

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Returns a formatted game-time string (`HH:MM YYYY-MM-DD`) snapshotted
/// from the shared world clock. Used for debug event timestamps so the
/// Events tab no longer renders blank times.
async fn debug_event_timestamp(state: &Arc<AppState>) -> String {
    let world = state.world.lock().await;
    world.clock.now().format("%H:%M %Y-%m-%d").to_string()
}

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
        inference_paused: core.inference_paused,
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
    let config = state.config.lock().await;
    let transport = state.transport.default_mode();
    let core_map =
        parish_core::ipc::build_map_data(&world, transport, config.reveal_unexplored_locations);

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
        transport_label: core_map.transport_label,
        transport_id: core_map.transport_id,
    })
}

/// Returns the list of NPCs currently at the player's location.
#[tauri::command]
pub async fn get_npcs_here(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<NpcInfo>, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Ok(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

/// Returns the current time-of-day palette as CSS hex colours.
#[tauri::command]
pub async fn get_theme(state: tauri::State<'_, Arc<AppState>>) -> Result<ThemePalette, String> {
    use chrono::Timelike;
    use parish_palette::compute_palette;
    let world = state.world.lock().await;
    let now = world.clock.now();
    let raw = compute_palette(now.hour(), now.minute());
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
        categories: parish_core::debug_snapshot::build_inference_categories(&config),
        configured_providers: parish_core::debug_snapshot::build_configured_providers(),
    };

    Ok(debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
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
    addressed_to: Option<Vec<String>>,
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

    touch_player_activity(&state).await;

    let addressed_to = addressed_to.unwrap_or_default();

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
            // Capture location before handle_game_input (which may move the player).
            let reaction_location = state.world.lock().await.player_location;
            handle_game_input(raw, addressed_to, state.clone(), app.clone()).await;
            // Generate NPC reactions to the player's message in the background.
            emit_npc_reactions(
                &player_msg_id,
                &raw_for_reactions,
                reaction_location,
                &state,
                &app,
            );
        }
    }

    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Replaces the client and respawns the inference worker so subsequent
/// NPC conversations use the new configuration.
async fn rebuild_inference(state: &Arc<AppState>, app: &tauri::AppHandle) {
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
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".to_string(),
                    content: format!(
                        "Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.",
                        base_url
                    ),
                },
            );
        }
        let provider_enum =
            parish_core::config::Provider::from_str_loose(&provider_name).unwrap_or_default();
        let built = parish_core::inference::build_client(
            &provider_enum,
            &base_url,
            api_key.as_deref(),
            &state.inference_config, // (#417) use TOML-configured timeouts
        );
        let mut client_guard = state.client.lock().await;
        *client_guard = Some(built.clone());
        built
    };

    // Abort the old inference worker before spawning a replacement to prevent
    // orphaned tasks from accumulating (each holds an HTTP client and channel).
    {
        let mut wh = state.worker_handle.lock().await;
        if let Some(old) = wh.take() {
            old.abort();
        }
    }

    // Respawn inference worker with the new client and store the handle.
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        any_client,
        interactive_rx,
        background_rx,
        batch_rx,
        state.inference_log.clone(),
        state.inference_config.clone(),
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

async fn emit_world_update(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let transport = state.transport.default_mode();
    let npc_manager = state.npc_manager.lock().await;
    let mut snapshot = snapshot_from_world(&world, transport);
    snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
    let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
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
            CommandEffect::RebuildInference => rebuild_inference(state, app).await,
            CommandEffect::RebuildCloudClient => {
                let config = state.config.lock().await;
                let base_url = config
                    .cloud_base_url
                    .as_deref()
                    .unwrap_or("https://openrouter.ai/api")
                    .to_string();
                let api_key = config.cloud_api_key.clone();
                let provider_enum = config
                    .cloud_provider_name
                    .as_deref()
                    .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
                    .unwrap_or(parish_core::config::Provider::OpenRouter);
                drop(config);
                let mut cloud_guard = state.cloud_client.lock().await;
                *cloud_guard = Some(parish_core::inference::build_client(
                    &provider_enum,
                    &base_url,
                    api_key.as_deref(),
                    &state.inference_config, // (#417) use TOML-configured timeouts
                ));
            }
            CommandEffect::Quit => {
                app.exit(0);
                return;
            }
            CommandEffect::ToggleMap => {
                let _ = app.emit(crate::events::EVENT_TOGGLE_MAP, ());
                return; // No text log for map toggle
            }
            CommandEffect::OpenDesigner => {
                let _ = app.emit(crate::events::EVENT_OPEN_DESIGNER, ());
                return; // No text log — navigation handled by frontend
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
            CommandEffect::NewGame => match do_new_game(state, app).await {
                Ok(()) => {
                    extra_response = Some("A new chapter begins in the parish...".to_string());
                }
                Err(e) => {
                    extra_response = Some(format!("New game failed: {}", e));
                }
            },
            CommandEffect::SaveFlags => {
                let flags = state.config.lock().await.flags.clone();
                let path = state.data_dir.join("parish-flags.json");
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = flags.save_to_file(&path) {
                        tracing::warn!("Failed to save feature flags: {}", e);
                    }
                });
            }
            CommandEffect::ApplyTheme(name, mode) => {
                let _ = app.emit(
                    crate::events::EVENT_THEME_SWITCH,
                    serde_json::json!({ "name": name, "mode": mode }),
                );
            }
            CommandEffect::ApplyTiles(id) => {
                let _ = app.emit(
                    crate::events::EVENT_TILES_SWITCH,
                    serde_json::json!({ "id": id }),
                );
            }
        }
    }

    // Emit the command response text (shared response or mode-specific override).
    let response = extra_response.unwrap_or(result.response);
    if !response.is_empty() {
        // Route through the core helpers so tabular responses (e.g. `/help`)
        // carry `subtype: "tabular"` for the chat UI to render in monospace.
        let payload = match result.presentation {
            parish_core::ipc::TextPresentation::Tabular => {
                text_log_typed("system", response, "tabular")
            }
            parish_core::ipc::TextPresentation::Prose => text_log("system", response),
        };
        let _ = app.emit(EVENT_TEXT_LOG, payload);
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

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
async fn handle_game_input(
    raw: String,
    addressed_to: Vec<String>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
    // Resolve the intent client and model (Intent category override, or base).
    let (client, model) = {
        let config = state.config.lock().await;
        let base_client = state.client.lock().await;
        config.resolve_category_client(InferenceCategory::Intent, base_client.as_ref())
    };

    // Parse intent: tries local keywords first, then LLM for ambiguous input.
    let intent = if let Some(client) = &client {
        // Capture generation before releasing the lock so we can detect TOCTOU
        // races on re-acquire (issue #283).
        let gen_before = {
            let mut world = state.world.lock().await;
            world.clock.inference_pause();
            world.tick_generation
        };
        let result = parse_intent(client, &raw, &model).await;
        {
            let mut world = state.world.lock().await;
            world.clock.inference_resume();
            let gen_after = world.tick_generation;
            if gen_after != gen_before {
                tracing::warn!(
                    gen_before,
                    gen_after,
                    "World advanced during intent parse (TOCTOU #283) — \
                     {} tick(s) elapsed; proceeding with parsed intent",
                    gen_after.wrapping_sub(gen_before),
                );
                let _ = app.emit(
                    crate::events::EVENT_TEXT_LOG,
                    text_log(
                        "system",
                        "The world shifted while your words were in the air.",
                    ),
                );
            }
        }
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
            handle_movement(&target, &state, &app).await;
        } else {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
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
        handle_npc_conversation(String::new(), targets, state, app).await;
        return;
    }

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

    handle_npc_conversation(mentions.remaining, targets, state, app).await;
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
        let payload = match msg.subtype {
            Some(st) => text_log_typed(msg.source, &msg.text, st),
            None => text_log(msg.source, &msg.text),
        };
        let _ = app.emit(EVENT_TEXT_LOG, payload);
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
            Some(&state.inference_log),
            |_turn_id, npc_name| {
                let _ = app.emit(
                    EVENT_TEXT_LOG,
                    text_log(npc_name.to_string(), String::new()),
                );
            },
            |turn_id, source, batch| {
                let _ = app.emit(
                    EVENT_STREAM_TOKEN,
                    StreamTokenPayload {
                        token: batch.to_string(),
                        turn_id,
                        source: source.to_string(),
                    },
                );
            },
        )
        .await;

        // Finalise the streaming state so the frontend marks the last entry done.
        let _ = app.emit(EVENT_STREAM_END, StreamEndPayload { hints: vec![] });
    }

    // Record tier transitions in the debug event log
    if !effects.tier_transitions.is_empty() {
        let ts = debug_event_timestamp(state).await;
        let mut debug_events = state.debug_events.lock().await;
        for tt in &effects.tier_transitions {
            if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
                debug_events.pop_front();
            }
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            debug_events.push_back(DebugEvent {
                timestamp: ts.clone(),
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
        false,
    );
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".to_string(),
            content: text,
        },
    );
}

/// Routes input to the NPC at the player's location, or shows idle message.
///
/// If `target_name` is provided (from an `@mention`), the matching NPC
/// is selected. Otherwise falls back to the first NPC at the location.
struct TurnOutcome {
    line: Option<ConversationLine>,
    hints: Vec<parish_core::npc::IrishWordHint>,
}

#[allow(clippy::too_many_arguments)]
async fn run_npc_turn(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
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
    spawn_loading_animation(app.clone(), loading_cancel.clone());

    let (token_tx, token_rx) = mpsc::channel::<String>(parish_core::ipc::TOKEN_CHANNEL_CAPACITY);
    let display_label = capitalize_first(&setup.display_name);
    let req_id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let _ = app.emit(
        EVENT_TEXT_LOG,
        text_log_for_stream_turn(display_label.clone(), String::new(), req_id),
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
            true,
        )
        .await;

    let response_rx = match send_result {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
            let ts = debug_event_timestamp(state).await;
            let mut events = state.debug_events.lock().await;
            if events.len() >= crate::DEBUG_EVENT_CAPACITY {
                events.pop_front();
            }
            events.push_back(DebugEvent {
                timestamp: ts,
                category: "inference".to_string(),
                message: format!("Queue submit failed: {e}"),
            });
            let _ = app.emit(
                EVENT_STREAM_TURN_END,
                StreamTurnEndPayload { turn_id: req_id },
            );
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".to_string(),
                    content: "The parish storyteller has wandered off. Try again in a moment."
                        .to_string(),
                },
            );
            loading_cancel.cancel();
            return None;
        }
    };

    let stream_handle = tokio::spawn({
        let app_clone = app.clone();
        let source = display_label.clone();
        async move {
            parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
                let _ = app_clone.emit(
                    crate::events::EVENT_STREAM_TOKEN,
                    StreamTokenPayload {
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
    let _ = app.emit(
        EVENT_STREAM_TURN_END,
        StreamTurnEndPayload { turn_id: req_id },
    );

    let response = match outcome {
        InferenceAwaitOutcome::Response(r) => r,
        InferenceAwaitOutcome::Closed => {
            tracing::warn!(
                req_id,
                "NPC inference response channel closed without a reply",
            );
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".to_string(),
                    content: "The storyteller has wandered off mid-tale.".to_string(),
                },
            );
            loading_cancel.cancel();
            return None;
        }
        InferenceAwaitOutcome::TimedOut { secs } => {
            tracing::warn!(req_id, secs, "NPC inference response timed out");
            let ts = debug_event_timestamp(state).await;
            let mut events = state.debug_events.lock().await;
            if events.len() >= crate::DEBUG_EVENT_CAPACITY {
                events.pop_front();
            }
            events.push_back(DebugEvent {
                timestamp: ts,
                category: "inference".to_string(),
                message: format!("Response timed out after {secs}s"),
            });
            drop(events);
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".to_string(),
                    content: "The storyteller is lost in thought. Try again.".to_string(),
                },
            );
            loading_cancel.cancel();
            return None;
        }
    };

    if let Some(ref err) = response.error {
        tracing::warn!("Inference error: {:?}", err);
        let ts = debug_event_timestamp(state).await;
        let mut events = state.debug_events.lock().await;
        if events.len() >= crate::DEBUG_EVENT_CAPACITY {
            events.pop_front();
        }
        events.push_back(DebugEvent {
            timestamp: ts,
            category: "inference".to_string(),
            message: format!("Dialogue error: {err}"),
        });
        let idx = response.id as usize % INFERENCE_FAILURE_MESSAGES.len();
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".to_string(),
                content: INFERENCE_FAILURE_MESSAGES[idx].to_string(),
            },
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

async fn handle_npc_conversation(
    raw: String,
    target_names: Vec<String>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
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
        let idx = REQUEST_ID.fetch_add(1, Ordering::Relaxed) as usize % IDLE_MESSAGES.len();
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".to_string(),
                content: IDLE_MESSAGES[idx].to_string(),
            },
        );
        return;
    }

    if trimmed.is_empty() {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".to_string(),
                content: "There are ears enough for ye here, but say something first.".to_string(),
            },
        );
        return;
    }

    let Some(queue) = queue else {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".to_string(),
                content:
                    "There's someone here, but the LLM is not configured — set a provider with /provider."
                        .to_string(),
            },
        );
        return;
    };

    if targets.is_empty() {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".to_string(),
                content: "No one here answers to that name just now.".to_string(),
            },
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

    set_conversation_running(&state, true).await;
    {
        let mut world = state.world.lock().await;
        world.clock.inference_pause();
    }
    emit_world_update(&state, &app).await;

    let mut combined_hints: Vec<parish_core::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // Phase 1: each addressed NPC takes one turn in the order they were named.
    for speaker_id in &targets {
        let Some(outcome) = run_npc_turn(
            &state,
            &app,
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

    // Phase 2: autonomous chain via the bystander-aware heuristic.
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
            &state,
            &app,
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
    set_conversation_running(&state, false).await;
    emit_world_update(&state, &app).await;

    // Single stream-end after the entire turn so input stays disabled
    // through every NPC's response (matches the user spec).
    let _ = app.emit(
        EVENT_STREAM_END,
        StreamEndPayload {
            hints: combined_hints,
        },
    );
}

async fn run_idle_banter(state: &Arc<AppState>, app: &tauri::AppHandle) {
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
    emit_world_update(state, app).await;

    let mut combined_hints: Vec<parish_core::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // First spontaneous remark: deterministic ordering so a quiet location with
    // calm NPCs still produces a line. The heuristic alone would refuse.
    if let Some(first_speaker) = speakers.first().copied()
        && let Some(outcome) = run_npc_turn(
            state,
            app,
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

    // Follow-up turns: heuristic-based selection.
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
            app,
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
    // Update last_spoken_at regardless of success so a failed banter attempt
    // creates a cooldown and does not spam failure messages on every 1s tick.
    {
        let mut conversation = state.conversation.lock().await;
        conversation.last_spoken_at = std::time::Instant::now();
        conversation.conversation_in_progress = false;
    }
    emit_world_update(state, app).await;

    let _ = app.emit(
        EVENT_STREAM_END,
        StreamEndPayload {
            hints: combined_hints,
        },
    );
}

pub(crate) async fn tick_inactivity(state: &Arc<AppState>, app: &tauri::AppHandle) {
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
                source: "system".to_string(),
                content:
                    "The parish falls quiet after a full minute of silence. Time is now paused."
                        .to_string(),
            },
        );
        emit_world_update(state, app).await;
        let mut conversation = state.conversation.lock().await;
        conversation.last_spoken_at = now;
        return;
    }

    if player_idle >= idle_after && speech_idle >= idle_after {
        run_idle_banter(state, app).await;
    }
}

// ── Persistence commands ────────────────────────────────────────────────────

use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

/// Returns the list of save files with branch metadata.
#[tauri::command]
pub async fn discover_save_files(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileInfo>, String> {
    let world = state.world.lock().await;
    let saves = discover_saves(&state.saves_dir, &world.graph);
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
        // Create a new save file in the resolved saves directory.
        let path = new_save_path(&state.saves_dir);
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
    use parish_core::persistence::SaveFileLock;

    let path = std::path::PathBuf::from(&file_path);

    // If switching to a different save file, acquire a new lock first.
    let current_path = state.save_path.lock().await.clone();
    let switching_files = current_path.as_ref() != Some(&path);

    if switching_files {
        let lock = SaveFileLock::try_acquire(&path)
            .ok_or_else(|| "This save file is in use by another instance.".to_string())?;
        // Release old lock and store new one.
        *state.save_lock.lock().await = Some(lock);
    }

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
            stream_turn_id: None,
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
    use parish_core::persistence::SaveFileLock;

    let path = new_save_path(&state.saves_dir);

    // Acquire lock on the new save file, releasing any previous lock.
    let lock = SaveFileLock::try_acquire(&path)
        .ok_or_else(|| "Could not lock the new save file.".to_string())?;
    *state.save_lock.lock().await = Some(lock);

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

/// Internal helper that reloads world/NPCs and creates a fresh save file.
///
/// Called both by the `new_game` Tauri command and the `CommandEffect::NewGame` handler.
async fn do_new_game(state: &Arc<AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    let game_mod = parish_core::game_mod::find_default_mod()
        .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok());

    // Reload fresh world and NPCs from the active game mod when available,
    // falling back to legacy data files for backward compatibility.
    let (fresh_world, npcs_path) = if let Some(ref gm) = game_mod {
        let world = parish_core::game_mod::world_state_from_mod(gm)
            .map_err(|e| format!("Failed to load world from mod: {}", e))?;
        (world, gm.npcs_path())
    } else {
        let data_dir = state.data_dir.clone();
        let world = parish_core::world::WorldState::from_parish_file(
            &data_dir.join("parish.json"),
            parish_core::world::LocationId(15),
        )
        .map_err(|e| format!("Failed to load parish.json: {}", e))?;
        (world, data_dir.join("npcs.json"))
    };

    let mut fresh_npcs = parish_core::npc::manager::NpcManager::load_from_file(&npcs_path)
        .unwrap_or_else(|_| parish_core::npc::manager::NpcManager::new());

    fresh_npcs.assign_tiers(&fresh_world, &[]);

    // Replace live state
    let mut world = state.world.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    *world = fresh_world;
    *npc_manager = fresh_npcs;

    // Reset conversation transcript so stale dialogue from the previous game
    // does not bleed into NPC conversations in the new game (#281).
    {
        let mut conv = state.conversation.lock().await;
        *conv = ConversationRuntimeState::new();
    }

    // Create a new save file with the fresh state
    let path = new_save_path(&state.saves_dir);
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

    drop(npc_manager);
    drop(world);

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
    do_new_game(&state, &app).await?;
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".to_string(),
            content: "A new chapter begins in the parish...".to_string(),
        },
    );
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
    fn is_snippet_injection_char(c: char) -> bool {
        c == '"' || c == '\\' || c == '\u{2028}' || c == '\u{2029}' || c.is_control()
    }

    // Validate emoji is in the palette
    if reactions::reaction_description(&emoji).is_none() {
        return Err("Unknown reaction emoji.".to_string());
    }

    // Reject snippets that could inject content into NPC system prompts (#687).
    if message_snippet.chars().any(is_snippet_injection_char) {
        return Err("Message snippet contains disallowed characters.".to_string());
    }

    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log.add(&emoji, &message_snippet, now);
    }

    Ok(())
}

/// Generates NPC reactions to a player message and emits events.
///
/// `location` must be the player's location **at the time the message was
/// sent**, captured before any `handle_game_input` call that might move the
/// player. This prevents a race where the player moves between spawn and
/// execution, causing reactions to be attributed to NPCs at the wrong location.
///
/// Runs as a detached background task so player input handling remains
/// responsive. When the `npc-llm-reactions` flag is enabled (default) and an
/// LLM client is configured, each NPC at the player's location gets an
/// inference call; on any failure the function falls back to rule-based
/// keyword matching (#404). Reactions are persisted to the NPC's
/// `reaction_log` for memory continuity (#403).
fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    location: LocationId,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    let state = Arc::clone(state);
    let app = app.clone();
    let player_msg_id = player_msg_id.to_string();
    let player_input = player_input.to_string();

    // #651 — await the task handle and surface any panic to the log so errors
    // are never silently swallowed.
    let handle = tokio::spawn(async move {
        let (npcs_here, llm_enabled, reaction_client, reaction_model) = {
            let npc_manager = state.npc_manager.lock().await;
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;

            // Use the pre-captured location — do not read world.player_location
            // here, as the player may have moved since the message was sent.
            let npcs = npc_manager
                .npcs_at(location)
                .iter()
                .map(|npc| (*npc).clone())
                .collect::<Vec<_>>();

            let (client, model) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            let enabled = !config.flags.is_disabled("npc-llm-reactions");

            (npcs, enabled, client, model)
        };

        if npcs_here.is_empty() {
            return;
        }

        // Run per-NPC inference concurrently, bounded to NPC_REACTION_CONCURRENCY
        // simultaneous calls so a busy location can't exhaust the LLM connection
        // pool (#406).
        let sem = Arc::new(Semaphore::new(NPC_REACTION_CONCURRENCY));
        let mut join_set = tokio::task::JoinSet::new();

        for npc in npcs_here {
            let sem = Arc::clone(&sem);
            let client = reaction_client.clone();
            let model = reaction_model.clone();
            let input = player_input.clone();

            join_set.spawn(async move {
                // Acquire a permit before starting the (potentially slow) LLM call.
                let _permit = sem.acquire().await.ok();

                // Try LLM path first; fall back to rule-based on any failure (#404).
                let emoji = if llm_enabled {
                    if let Some(ref c) = client {
                        reactions::infer_player_message_reaction(
                            c,
                            &model,
                            &npc,
                            &input,
                            std::time::Duration::from_secs(2),
                        )
                        .await
                        .or_else(|| reactions::generate_rule_reaction(&input))
                    } else {
                        reactions::generate_rule_reaction(&input)
                    }
                } else {
                    reactions::generate_rule_reaction(&input)
                };

                (npc.name.clone(), emoji)
            });
        }

        // Collect results as tasks finish, then persist + emit each reaction.
        while let Some(result) = join_set.join_next().await {
            let Ok((npc_name, Some(emoji))) = result else {
                continue;
            };

            // Persist to reaction_log so NPC memory is maintained (#403).
            {
                let mut npc_manager = state.npc_manager.lock().await;
                if let Some(npc_mut) = npc_manager.find_by_name_mut(&npc_name) {
                    npc_mut.reaction_log.add_player_message_reaction(
                        &emoji,
                        &player_input,
                        chrono::Utc::now(),
                    );
                }
            }

            let _ = app.emit(
                crate::events::EVENT_NPC_REACTION,
                NpcReactionPayload {
                    message_id: player_msg_id.clone(),
                    emoji,
                    source: capitalize_first(&npc_name),
                },
            );
        }
    });

    // Spawn a lightweight watcher that logs any panic from the reaction batch
    // (#651). This keeps emit_npc_reactions non-blocking while ensuring panics
    // are visible in the tracing output rather than silently swallowed.
    tokio::spawn(async move {
        if let Err(e) = handle.await {
            tracing::error!(error = %e, "emit_npc_reactions task panicked");
        }
    });
}
