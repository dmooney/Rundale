//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;

use parish_core::config::InferenceCategory;
use parish_core::debug_snapshot::{self, AuthDebug, DebugEvent, DebugSnapshot, InferenceDebug};
// AnyClient, InferenceQueue, spawn_inference_worker formerly imported here —
// now handled by parish_core::game_loop::rebuild_inference_worker (#696).
use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::{compute_name_hints, text_log, text_log_typed};
use parish_core::npc::reactions;
use parish_core::world::LocationId;
use parish_core::world::transport::TransportMode;
// DEFAULT_START_LOCATION — no longer used directly; handled by load_fresh_world_and_npcs (#696).
use tauri::Emitter;

use crate::events::{
    EVENT_STREAM_END, EVENT_STREAM_TOKEN, EVENT_TEXT_LOG, EVENT_TRAVEL_START, EVENT_WORLD_UPDATE,
    StreamEndPayload, StreamTokenPayload, TextLogPayload,
};
use crate::{AppState, MapData, MapLocation, NpcInfo, SaveState, ThemePalette, WorldSnapshot};

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

/// Returns the latest provider-bootstrap status for the startup overlay.
#[tauri::command]
pub async fn get_setup_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::SetupStatusSnapshot, String> {
    Ok(state
        .setup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone())
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
    let text = validate_input_text(&text)?;
    if text.is_empty() {
        return Ok(());
    }
    // #752 — cap addressed_to to prevent unbounded memory/allocation via the
    // NPC-addressing chip list.  Max 10 entries; each name ≤ 100 chars.
    let addressed_to = addressed_to.unwrap_or_default();
    validate_addressed_to(&addressed_to)?;

    touch_player_activity(&state).await;

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, &state, &app).await;
        }
        InputResult::GameInput(raw) => {
            tracing::info!(input = %raw, "chat [player]");
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

// ── #752 — addressed_to validation ───────────────────────────────────────────

/// Validates and trims player free-text input for `submit_input`.
///
/// - Trims leading/trailing whitespace.
/// - Returns `Ok(String)` (the trimmed text) for empty input — callers should
///   short-circuit before calling this if they want to silently drop empties.
/// - Returns `Err` when the trimmed length exceeds 2000 characters.
pub fn validate_input_text(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().to_string();
    if trimmed.len() > 2000 {
        return Err("Input too long (max 2000 characters).".to_string());
    }
    Ok(trimmed)
}

/// Validates the `addressed_to` list from the `submit_input` command.
///
/// Rules (mode-parity with the server path in `parish-server`):
/// - At most **10** entries (prevents unbounded NPC-chip spam).
/// - Each name is at most **100** characters.
///
/// Returns `Err(String)` with a user-visible message on any violation.
pub fn validate_addressed_to(addressed_to: &[String]) -> Result<(), String> {
    if addressed_to.len() > 10 {
        return Err("Too many addressees (max 10).".to_string());
    }
    if addressed_to.iter().any(|name| name.len() > 100) {
        return Err("Addressee name too long (max 100 characters).".to_string());
    }
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

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
    // Note: Tauri has no trait-erased inference_client slot (unlike the server),
    // so no additional slot update is needed here.
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

/// Handles `/command` inputs.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`TauriCommandHost`] adapter (#696 slice 7).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use crate::command_host::TauriCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;

    let host = TauriCommandHost::new(Arc::clone(state), app.clone());
    shared_handle(&host, cmd).await;
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
                    source: "system".into(),
                    content: "And where would ye be off to?".to_string(),
                    subtype: None,
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
    //
    // Pass `raw` (the original input) rather than an empty string so that
    // dialogue like "Hello Brigid, good morning!" is not discarded when the
    // intent parser classifies it as Talk. An empty `raw` still produces the
    // "say something first" prompt, which is correct for bare "talk to X".
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
        handle_npc_conversation(raw, targets, state, app).await;
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
    use parish_core::game_session::{
        apply_movement, enrich_travel_encounter, roll_travel_encounter,
    };

    let transport = state.transport.default_mode().clone();

    // Apply all movement state changes within a single lock scope to prevent
    // TOCTOU races.
    let (effects, rolled_encounter) = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let effects = apply_movement(
            &mut world,
            &mut npc_manager,
            &state.reaction_templates,
            target,
            &transport,
        );
        let rolled = if effects.world_changed {
            let config = state.config.lock().await;
            if !config.flags.is_disabled("travel-encounters") {
                roll_travel_encounter(&world, &effects)
            } else {
                None
            }
        } else {
            None
        };
        (effects, rolled)
    };

    // Resolve encounter text — LLM-enriched when a reaction client exists
    // and the `travel-encounters-llm` flag is not explicitly disabled.
    let encounter_line: Option<String> = if let Some(rolled) = rolled_encounter.as_ref() {
        let llm_enabled = {
            let cfg = state.config.lock().await;
            !cfg.flags.is_disabled("travel-encounters-llm")
        };
        let (reaction_client, reaction_model) = if llm_enabled {
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;
            config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref())
        } else {
            (None, String::new())
        };
        let text = if let Some(client) = reaction_client.as_ref() {
            enrich_travel_encounter(rolled, client, &reaction_model, 15).await
        } else {
            rolled.canned.text.clone()
        };
        let formatted = format!("  · {text}");
        {
            let mut world = state.world.lock().await;
            world.log(formatted.clone());
        }
        Some(formatted)
    } else {
        None
    };

    // Emit travel-start animation payload first
    if let Some(travel_payload) = &effects.travel_start {
        let _ = app.emit(EVENT_TRAVEL_START, travel_payload);
    }

    // Emit all player-visible messages in order
    for msg in &effects.messages {
        tracing::info!(source = %msg.source, text = %msg.text.trim(), "chat");
        let payload = match msg.subtype {
            Some(st) => text_log_typed(msg.source, &msg.text, st),
            None => text_log(msg.source, &msg.text),
        };
        let _ = app.emit(EVENT_TEXT_LOG, payload);
    }

    // Emit travel encounter line if one fired
    if let Some(line) = encounter_line {
        let _ = app.emit(EVENT_TEXT_LOG, text_log("system", &line));
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
            &state.language_settings,
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
            source: "system".into(),
            content: text,
            subtype: None,
        },
    );
}

/// Helper: sets `conversation_in_progress` on the conversation mutex.
///
/// Only used in unit tests; production code uses the `GameLoopContext`-based
/// shared orchestration which manages this flag internally.
#[cfg(test)]
async fn set_conversation_running(state: &Arc<AppState>, running: bool) {
    let mut conversation = state.conversation.lock().await;
    conversation.conversation_in_progress = running;
}

/// Routes input to one or more NPCs at the player's location, or shows an idle message.
///
/// Delegates to [`parish_core::game_loop::handle_npc_conversation`] for all
/// shared logic (#696), then emits a world-update snapshot when inference
/// finishes.
async fn handle_npc_conversation(
    raw: String,
    target_names: Vec<String>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
    };

    let app_for_loading = app.clone();
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        crate::events::spawn_loading_animation(app_for_loading.clone(), cancel.clone());
        Some(cancel)
    };

    emit_world_update(&state, &app).await;
    parish_core::game_loop::handle_npc_conversation(&ctx, raw, target_names, spawn_loading).await;
    emit_world_update(&state, &app).await;
}

/// Delegates to [`parish_core::game_loop::run_idle_banter`] for all shared
/// logic (#696), then emits a world-update snapshot when the sequence ends.
async fn run_idle_banter(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
    };

    emit_world_update(state, app).await;
    // Idle banter spawns no loading animation.
    parish_core::game_loop::run_idle_banter(&ctx, || None).await;
    emit_world_update(state, app).await;
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
                source: "system".into(),
                content:
                    "The parish falls quiet after a full minute of silence. Time is now paused."
                        .to_string(),
                subtype: None,
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

/// Internal save implementation — delegates to the shared canonical impl (#696).
async fn do_save_game(state: &Arc<AppState>) -> Result<String, String> {
    parish_core::game_loop::do_save_game(
        &state.world,
        &state.npc_manager,
        &state.save_path,
        &state.current_branch_id,
        &state.current_branch_name,
        &state.saves_dir,
    )
    .await
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
        .map_err(|e| e.to_string())??;

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
            source: "system".into(),
            content: format!("Loaded {} (branch: {}).", filename, branch_name),
            subtype: None,
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
pub async fn do_create_branch(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };

    tracing::info!(
        "Creating branch '{}' with parent {} in {:?}",
        name,
        parent_branch_id,
        db_path
    );

    // Capture snapshot before spawn_blocking so the tokio locks are not held across it.
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let name_owned = name.to_string();
    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let new_id = db
            .create_branch(&name_owned, Some(parent_branch_id))
            .map_err(|e| {
                tracing::error!("create_branch failed: {}", e);
                e.to_string()
            })?;
        tracing::info!("Branch '{}' created with id {}", name_owned, new_id);
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        tracing::info!("Snapshot saved to branch '{}'", name_owned);
        Ok(new_id)
    })
    .await
    .map_err(|e| e.to_string())??;

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

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or("Failed to create main branch")?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())??;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Internal helper that reloads world/NPCs and creates a fresh save file.
///
/// Called both by the `new_game` Tauri command and the `CommandEffect::NewGame`
/// handler.  Delegates to the shared `parish_core::game_loop::do_new_game` (#696).
pub async fn do_new_game(state: &Arc<AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    use parish_core::game_loop::{NewGameParams, do_new_game as core_do_new_game};

    // Rediscover the active game mod (Tauri AppState does not cache it).
    let game_mod = parish_core::game_mod::find_default_mod()
        .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok());

    let emitter = crate::events::TauriEmitter::new(app.clone());
    core_do_new_game(NewGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        conversation: &state.conversation,
        save_path: &state.save_path,
        current_branch_id: &state.current_branch_id,
        current_branch_name: &state.current_branch_name,
        saves_dir: &state.saves_dir,
        game_mod: game_mod.as_ref(),
        data_dir: &state.data_dir,
        pronunciations: &state.pronunciations,
        default_transport: state.transport.default_mode(),
        emitter: &emitter,
    })
    .await
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
            source: "system".into(),
            content: "A new chapter begins in the parish...".to_string(),
            subtype: None,
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
pub async fn do_list_branches_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let current_id = *state.current_branch_id.lock().await;

    let branches = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.list_branches().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

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
pub async fn do_branch_log_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let bid = state
        .current_branch_id
        .lock()
        .await
        .ok_or("No active branch.")?;

    let log = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.branch_log(bid).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

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

// Snippet injection validation is shared via parish_core::game_loop::is_snippet_injection_char
// (#687 security parity). Delegating here guarantees server and Tauri use identical logic.
pub use parish_core::game_loop::is_snippet_injection_char;

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

/// Delegates to [`parish_core::game_loop::emit_npc_reactions`] (#696 slice 5).
///
/// `location` must be the player's location **at the time the message was
/// sent**, captured before any `handle_game_input` call that might move the
/// player. This prevents a race where the player moves between spawn and
/// execution, causing reactions to be attributed to NPCs at the wrong location.
///
/// Pre-captures the NPC list, resolves the reaction client and feature flags,
/// constructs a `TauriEmitter`, and delegates to the shared implementation.
fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    location: LocationId,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    let state_clone = Arc::clone(state);
    let player_msg_id = player_msg_id.to_string();
    let player_input = player_input.to_string();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));

    // Persist callback: closes over Arc<AppState> and locks npc_manager to
    // record each reaction in the NPC's reaction_log (#403).
    let state_for_persist = Arc::clone(state);
    let persist: parish_core::game_loop::PersistReactionFn = std::sync::Arc::new(
        move |npc_name: String, emoji: String, player_input: String| {
            let state = Arc::clone(&state_for_persist);
            tokio::spawn(async move {
                let mut npc_manager = state.npc_manager.lock().await;
                if let Some(npc_mut) = npc_manager.find_by_name_mut(&npc_name) {
                    npc_mut.reaction_log.add_player_message_reaction(
                        &emoji,
                        &player_input,
                        chrono::Utc::now(),
                    );
                }
            });
        },
    );

    tokio::spawn(async move {
        // Pre-capture the NPC list at the given location (the player may have
        // moved by the time the background task runs).
        let (npcs_here, reaction_client, reaction_model, llm_enabled) = {
            let npc_manager = state_clone.npc_manager.lock().await;
            let config = state_clone.config.lock().await;
            let base_client = state_clone.client.lock().await;
            let npcs = npc_manager
                .npcs_at(location)
                .iter()
                .map(|npc| (*npc).clone())
                .collect::<Vec<_>>();
            let (client, model) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            let enabled = !config.flags.is_disabled("npc-llm-reactions");
            (npcs, client, model, enabled)
        };

        parish_core::game_loop::emit_npc_reactions(
            player_msg_id,
            player_input,
            npcs_here,
            reaction_client,
            reaction_model,
            llm_enabled,
            emitter,
            persist,
        );
    });
}

// ── Demo / auto-player commands ──────────────────────────────────────────────

/// A single NPC visible to the demo player at the current location.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoNpcInfo {
    pub name: String,
    pub description: String,
}

/// An adjacent location visible to the demo player.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoAdjacentLocation {
    pub name: String,
    pub travel_minutes: Option<u16>,
    pub visited: bool,
}

/// A snapshot of the game context passed to the LLM player each turn.
///
/// Backend fills all fields except `recent_log`; the frontend appends the
/// last 40 entries from the `textLog` store before calling `get_llm_player_action`.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoContextSnapshot {
    pub location_name: String,
    pub location_description: String,
    pub game_time: String,
    pub season: String,
    pub weather: String,
    pub npcs_here: Vec<DemoNpcInfo>,
    pub adjacent: Vec<DemoAdjacentLocation>,
    pub recent_log: Vec<String>,
    pub extra_prompt: Option<String>,
}

/// Demo configuration returned by `get_demo_config`.
#[derive(serde::Serialize, Clone)]
pub struct DemoConfigPayload {
    pub auto_start: bool,
    pub extra_prompt: Option<String>,
    pub turn_pause_secs: f32,
    pub max_turns: Option<u32>,
}

/// Returns the demo configuration (CLI flags parsed at startup).
#[tauri::command]
pub async fn get_demo_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoConfigPayload, String> {
    let dc = &state.demo_config;
    Ok(DemoConfigPayload {
        auto_start: dc.auto_start,
        extra_prompt: dc.extra_prompt.clone(),
        turn_pause_secs: dc.turn_pause_secs,
        max_turns: dc.max_turns,
    })
}

/// Builds a context snapshot for the LLM demo player.
///
/// Returns location, time, weather, NPCs present, and adjacent locations.
/// The `recent_log` field is empty; the frontend fills it from the text log
/// store before passing the snapshot to `get_llm_player_action`.
#[tauri::command]
pub async fn get_demo_context(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoContextSnapshot, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Lock order: world → npc_manager (matches AppState contract).
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    let player_loc = world.player_location;

    let location_name = world
        .graph
        .get(player_loc)
        .map(|d| d.name.clone())
        .unwrap_or_default();

    let location_description = if let Some(loc_data) = world.current_location_data() {
        parish_core::world::description::render_description(
            loc_data,
            world.clock.time_of_day(),
            &world.weather.to_string(),
            &[],
        )
    } else {
        String::new()
    };

    use chrono::Datelike;
    let now = world.clock.now();
    let time_of_day = world.clock.time_of_day();
    let game_time = format!(
        "{}, {} {} {}, {}",
        now.format("%A"),
        now.day(),
        now.format("%B"),
        now.year(),
        time_of_day,
    );
    let season = format!("{}", world.clock.season());
    let weather = world.weather.to_string();

    let npcs_here = npc_manager
        .npcs_at(player_loc)
        .iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            DemoNpcInfo {
                name: npc.display_name(introduced).to_string(),
                description: npc.occupation.clone(),
            }
        })
        .collect();

    let speed = state.transport.default_mode().speed_m_per_s;
    let adjacent = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(neighbor_id, _conn)| {
            let name = world
                .graph
                .get(neighbor_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("Location {}", neighbor_id.0));
            let travel_minutes = Some(world.graph.edge_travel_minutes(
                player_loc,
                neighbor_id,
                speed,
            ));
            let visited = world.visited_locations.contains(&neighbor_id);
            DemoAdjacentLocation {
                name,
                travel_minutes,
                visited,
            }
        })
        .collect();

    let extra_prompt = state.demo_config.extra_prompt.clone();

    Ok(DemoContextSnapshot {
        location_name,
        location_description,
        game_time,
        season,
        weather,
        npcs_here,
        adjacent,
        recent_log: Vec::new(),
        extra_prompt,
    })
}

/// Extracts the player action from an LLM response.
///
/// Handles three patterns:
/// 1. Completion: model received `{"action": "` and completed it — response is
///    something like `go to the mill"}`. Extract up to the closing quote.
/// 2. Full JSON: model output `{"action": "go to the mill"}` — scan for `{`
///    and JSON-parse from there.
/// 3. Fallback: no JSON at all — strip thinking preamble, take last line.
fn extract_action_from_response(text: &str) -> String {
    // Strip thinking blocks first so all patterns operate on clean text.
    let stripped = strip_thinking_block(text);
    let trimmed = stripped.trim();

    // Pattern 1: fill-in-the-blank completion — response starts with the
    // action text and ends with `"}` or just `"`. The model completed
    // `{"action": "` → `go to the mill"}`.
    // We also handle `go to the mill"` (no closing brace).
    let completion = trimmed
        .trim_end_matches('}')
        .trim_end()
        .trim_end_matches('"')
        .trim();
    // Valid completion: no opening brace in the extracted text (it's pure action).
    if !completion.is_empty() && !completion.starts_with('{') && !completion.contains("action") {
        // Check that the raw response looked like a completion (no full JSON object).
        if !trimmed.contains("{\"action\"") && !trimmed.contains("{ \"action\"") {
            return completion.to_string();
        }
    }

    // Pattern 2: full JSON object anywhere in the response.
    let mut search = trimmed;
    while let Some(start) = search.find('{') {
        let candidate = &search[start..];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(candidate)
            && let Some(action) = val.get("action").and_then(|v| v.as_str())
        {
            let action = action.trim();
            if !action.is_empty() {
                return action.to_string();
            }
        }
        search = &search[start + 1..];
    }

    // Pattern 3: fallback — take last meaningful line from already-stripped text.
    trimmed.trim_matches('"').trim_matches('\'').to_string()
}

/// Strips reasoning preamble from LLM responses so only the action remains.
///
/// Handles two patterns:
/// 1. Tagged blocks: `<thinking>...</thinking>` / `<think>...</think>` from
///    reasoning models (deepseek-r1, qwq). Takes everything after the last
///    closing tag.
/// 2. Plain-text multi-paragraph reasoning: if the response has blank-line-
///    separated paragraphs, takes the last paragraph. This covers models that
///    output reasoning prose before the final action without tags.
///
/// Falls back to the full trimmed text if neither pattern applies.
fn strip_thinking_block(text: &str) -> &str {
    let trimmed = text.trim();

    // Strip tagged thinking blocks first.
    for close_tag in &["</thinking>", "</think>"] {
        if let Some(pos) = trimmed.rfind(close_tag) {
            let after = trimmed[pos + close_tag.len()..].trim();
            if !after.is_empty() {
                return after;
            }
        }
    }

    // If the response has multiple blank-line-separated paragraphs, take the
    // last one — reasoning models often output rationale before the action.
    if let Some(last_para) = trimmed.rsplit("\n\n").find(|p| !p.trim().is_empty()) {
        let candidate = last_para.trim();
        // Only use the last paragraph if it looks like a short action (≤ 3
        // lines), not if the whole thing is one paragraph of prose.
        let line_count = candidate.lines().count();
        if line_count <= 3 && candidate.len() < trimmed.len() {
            return candidate;
        }
    }

    // Last-line fallback: models sometimes separate reasoning from action with
    // a single newline. If the last non-empty line is much shorter than the
    // full response, treat it as the action.
    let lines: Vec<&str> = trimmed.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > 1
        && let Some(&last) = lines.last()
    {
        let last = last.trim();
        if last.len() <= 200 && last.len() < trimmed.len() {
            return last;
        }
    }

    trimmed
}

/// Asks the LLM to choose the next player action given the current game context.
///
/// The frontend fills `ctx.recent_log` from the text log store before calling
/// this command. Returns the trimmed action string.
#[tauri::command]
pub async fn get_llm_player_action(
    ctx: DemoContextSnapshot,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Resolve client and model (base client; no per-category override for demo).
    let (client_opt, model) = {
        let config = state.config.lock().await;
        let client_guard = state.client.lock().await;
        let model = config.model_name.clone();
        let client = client_guard.as_ref().cloned();
        (client, model)
    };

    let Some(client) = client_opt else {
        return Err("No LLM client configured.".to_string());
    };

    let extra_section = ctx
        .extra_prompt
        .as_deref()
        .map(|p| format!("\n\n{}", p))
        .unwrap_or_default();

    let system_prompt = format!(
        "You are playing Rundale, an Irish living-world simulation set in 1820. You are a \
wandering stranger exploring the townlands of east Roscommon. The world is populated by \
historical Irish villagers — farmers, priests, weavers, matchmakers — each living their \
own life.\n\
\n\
Explore naturally: talk to people, learn their stories, travel between locations, and \
respond to whatever you encounter. Act as a curious outsider would.{extra}\n\
\n\
Respond with a JSON object containing a single field \"action\" — the text the player \
would type into the game. Do NOT use meta-commands like \"talk to X\"; write the actual \
words or command directly.\n\
\n\
Examples:\n\
  {{\"action\": \"Good morning! What brings you out at this hour?\"}}\n\
  {{\"action\": \"go to the mill\"}}\n\
  {{\"action\": \"look\"}}\n\
  {{\"action\": \"ask about the harvest\"}}\n\
\n\
Your entire response must be a single JSON object — nothing before or after it.",
        extra = extra_section,
    );

    let mut user_parts: Vec<String> = Vec::new();
    user_parts.push(format!("Location: {}", ctx.location_name));
    if !ctx.location_description.is_empty() {
        user_parts.push(ctx.location_description.clone());
    }
    user_parts.push(format!("Date and time: {} | {}", ctx.game_time, ctx.season));
    user_parts.push(format!("Weather: {}", ctx.weather));

    if ctx.npcs_here.is_empty() {
        user_parts.push("NPCs here: none".to_string());
    } else {
        let npc_lines: Vec<String> = ctx
            .npcs_here
            .iter()
            .map(|n| format!("  - {} ({})", n.name, n.description))
            .collect();
        user_parts.push(format!("NPCs here:\n{}", npc_lines.join("\n")));
    }

    if !ctx.adjacent.is_empty() {
        let adj_lines: Vec<String> = ctx
            .adjacent
            .iter()
            .map(|a| {
                let mins = a
                    .travel_minutes
                    .map(|m| format!("{} min", m))
                    .unwrap_or_else(|| "? min".to_string());
                let vis = if a.visited { "visited" } else { "unvisited" };
                format!("  - {} ({}, {})", a.name, mins, vis)
            })
            .collect();
        user_parts.push(format!("Adjacent locations:\n{}", adj_lines.join("\n")));
    }

    if !ctx.recent_log.is_empty() {
        let log_lines: Vec<String> = ctx.recent_log.iter().map(|l| format!("> {}", l)).collect();
        user_parts.push(format!("Recent events:\n{}", log_lines.join("\n")));
    }

    // Fill-in-the-blank technique: end the prompt with an incomplete JSON
    // object so the model completes the string rather than reasoning about it.
    user_parts.push("Action (complete the JSON):\n{\"action\": \"".to_string());

    let user_prompt = user_parts.join("\n\n");

    let raw = client
        .generate(
            &model,
            &user_prompt,
            Some(&system_prompt),
            Some(200),
            Some(0.9),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Primary: extract the "action" field from JSON output.
    // The system prompt asks for {"action": "..."}, which is robust against
    // any amount of preamble or reasoning text the model emits before it.
    let action_text = extract_action_from_response(&raw);
    tracing::info!(
        location = %ctx.location_name,
        raw_len = raw.len(),
        action = %action_text,
        "demo turn: LLM chose action"
    );
    Ok(action_text)
}

#[cfg(test)]
mod demo_tests {
    use super::{extract_action_from_response, strip_thinking_block};

    #[test]
    fn extracts_action_from_json() {
        let input = r#"{"action": "Good morning! How are you today?"}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Good morning! How are you today?"
        );
    }

    #[test]
    fn extracts_action_from_json_after_preamble() {
        let input = "Let me think... However, we are a wandering stranger.\n{\"action\": \"go to the crossroads\"}";
        assert_eq!(extract_action_from_response(input), "go to the crossroads");
    }

    #[test]
    fn extracts_action_from_json_after_thinking_tags() {
        let input = "<think>reasoning here</think>\n{\"action\": \"look\"}";
        assert_eq!(extract_action_from_response(input), "look");
    }

    #[test]
    fn falls_back_to_stripping_when_no_json() {
        let input = "Some reasoning.\nask about the harvest";
        assert_eq!(extract_action_from_response(input), "ask about the harvest");
    }

    #[test]
    fn strips_thinking_block_before_action() {
        let input =
            "<thinking>\nI should greet the farmer.\n</thinking>\nHello there, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello there, good morning!");
    }

    #[test]
    fn strips_think_tag_variant() {
        let input = "<think>reasoning</think>\ngo to the mill";
        assert_eq!(strip_thinking_block(input), "go to the mill");
    }

    #[test]
    fn no_thinking_block_returns_trimmed() {
        let input = "  ask Brigid about the harvest  ";
        assert_eq!(strip_thinking_block(input), "ask Brigid about the harvest");
    }

    #[test]
    fn only_thinking_block_falls_back_to_full() {
        let input = "<thinking>just thinking, nothing after</thinking>";
        assert_eq!(
            strip_thinking_block(input),
            "<thinking>just thinking, nothing after</thinking>"
        );
    }

    #[test]
    fn uses_last_closing_tag_for_nested() {
        let input = "<thinking>outer <think>inner</think> more</thinking>\nlook around";
        assert_eq!(strip_thinking_block(input), "look around");
    }

    #[test]
    fn strips_plain_text_reasoning_before_action() {
        let input = "Looking at the context, I see Peig is here. I should greet her warmly.\n\nHello Peig, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello Peig, good morning!");
    }

    #[test]
    fn single_paragraph_returned_as_is() {
        let input = "Hello Seamus, how goes the harvest?";
        assert_eq!(strip_thinking_block(input), input);
    }

    #[test]
    fn strips_single_newline_reasoning_before_action() {
        let input = "I need to explore. The crossroads is nearby.\ngo to the crossroads";
        assert_eq!(strip_thinking_block(input), "go to the crossroads");
    }

    #[test]
    fn strips_multi_sentence_reasoning_single_newline() {
        let input = "Based on my previous interaction with Peig, I should explore. The mill is unvisited.\nask about the mill";
        assert_eq!(strip_thinking_block(input), "ask about the mill");
    }
}

#[cfg(test)]
mod cmd_tests {
    use super::*;
    use std::sync::Arc;

    /// Builds a minimal [`AppState`] for unit tests — matches the structure
    /// used in `parish-server` tests (`routes::tests::test_app_state`).
    fn test_app_state() -> Arc<AppState> {
        use crate::{
            AppState, ConversationRuntimeState, DEBUG_EVENT_CAPACITY, DemoConfig, GameConfig,
            UiConfigSnapshot,
        };
        use parish_core::inference::new_inference_log;
        use parish_core::npc::manager::NpcManager;
        use parish_core::world::transport::TransportConfig;
        use parish_core::world::{DEFAULT_START_LOCATION, WorldState};
        use tokio::sync::Mutex;
        use tokio_util::sync::CancellationToken;

        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let world =
            WorldState::from_parish_file(&data_dir.join("world.json"), DEFAULT_START_LOCATION)
                .unwrap();
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
        let pronunciations = Vec::new();
        let reaction_templates = parish_core::npc::reactions::ReactionTemplates::default();
        let game_config = GameConfig {
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
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        };
        let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");
        let shutdown_token = CancellationToken::new();
        let session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore> =
            std::sync::Arc::new(parish_core::session_store::DbSessionStore::new(
                saves_dir.clone(),
            ));

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
            pronunciations,
            reaction_templates,
            save_path: Mutex::new(None),
            current_branch_id: Mutex::new(None),
            current_branch_name: Mutex::new(None),
            transport,
            data_dir: data_dir.clone(),
            saves_dir,
            worker_handle: Mutex::new(None),
            editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
            save_lock: Mutex::new(None),
            ollama_process: Mutex::new(parish_core::inference::client::OllamaProcess::none()),
            inference_config: parish_core::config::InferenceConfig::default(),
            setup_status: std::sync::Mutex::new(crate::SetupStatusSnapshot::default()),
            language_settings: parish_core::npc::LanguageSettings::english_only(),
            config: Mutex::new(game_config),
            demo_config: DemoConfig::default(),
            shutdown_token,
            session_store,
        })
    }

    // ── validate_input_text ─────────────────────────────────────────────────

    #[test]
    fn validate_input_accepts_normal_text() {
        assert!(validate_input_text("ask Brigid about the harvest").is_ok());
    }

    #[test]
    fn validate_input_trims_whitespace() {
        let result = validate_input_text("  hello  ").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn validate_input_allows_empty_after_trim() {
        assert!(validate_input_text("   ").is_ok());
    }

    #[test]
    fn validate_input_rejects_over_2000_chars() {
        let long: String = "a".repeat(2001);
        assert!(validate_input_text(&long).is_err());
    }

    #[test]
    fn validate_input_accepts_exactly_2000_chars() {
        let exactly: String = "a".repeat(2000);
        assert!(validate_input_text(&exactly).is_ok());
    }

    // ── validate_addressed_to ───────────────────────────────────────────────

    #[test]
    fn validate_addressed_to_accepts_empty_list() {
        assert!(validate_addressed_to(&[]).is_ok());
    }

    #[test]
    fn validate_addressed_to_accepts_up_to_10() {
        let names: Vec<String> = (0..10).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_ok());
    }

    #[test]
    fn validate_addressed_to_rejects_11_names() {
        let names: Vec<String> = (0..11).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_err());
    }

    #[test]
    fn validate_addressed_to_rejects_name_over_100_chars() {
        let long_name = "a".repeat(101);
        assert!(validate_addressed_to(&[long_name]).is_err());
    }

    #[test]
    fn validate_addressed_to_accepts_100_char_name() {
        let name = "a".repeat(100);
        assert!(validate_addressed_to(&[name]).is_ok());
    }

    // ── is_snippet_injection_char ───────────────────────────────────────────

    #[test]
    fn snippet_injection_rejects_double_quote() {
        assert!(is_snippet_injection_char('"'));
    }

    #[test]
    fn snippet_injection_rejects_backslash() {
        assert!(is_snippet_injection_char('\\'));
    }

    #[test]
    fn snippet_injection_rejects_line_separator() {
        assert!(is_snippet_injection_char('\u{2028}'));
    }

    #[test]
    fn snippet_injection_rejects_paragraph_separator() {
        assert!(is_snippet_injection_char('\u{2029}'));
    }

    #[test]
    fn snippet_injection_rejects_null_byte() {
        assert!(is_snippet_injection_char('\0'));
    }

    #[test]
    fn snippet_injection_accepts_normal_chars() {
        for c in "abcdefghijklmnopqrstuvwxyz ÁÉÍÓÚ,.!?'".chars() {
            assert!(
                !is_snippet_injection_char(c),
                "char {:?} should be allowed",
                c
            );
        }
    }

    // ── AppState save state on fresh state ─────────────────────────────────

    #[tokio::test]
    async fn save_state_initial_is_empty() {
        let state = test_app_state();
        let save_path = state.save_path.lock().await;
        let branch_id = state.current_branch_id.lock().await;
        let branch_name = state.current_branch_name.lock().await;

        let filename = save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        assert!(filename.is_none(), "fresh state should have no save file");
        assert!(branch_id.is_none(), "fresh state should have no branch id");
        assert!(
            branch_name.is_none(),
            "fresh state should have no branch name"
        );
    }

    // ── conversation state ──────────────────────────────────────────────────

    #[tokio::test]
    async fn set_conversation_running_toggles_flag() {
        let state = test_app_state();

        // Initially not running
        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }

        set_conversation_running(&state, true).await;

        {
            let conv = state.conversation.lock().await;
            assert!(conv.conversation_in_progress);
        }

        set_conversation_running(&state, false).await;

        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }
    }

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

    // ── world snapshot consistency ─────────────────────────────────────────

    #[tokio::test]
    async fn world_state_loads_kilteevan_as_start_location() {
        let state = test_app_state();
        let world = state.world.lock().await;
        let loc_name = world
            .current_location_data()
            .map(|d| d.name.as_str())
            .unwrap_or("unknown");
        // Default start location for Rundale is Kilteevan Village
        assert_eq!(loc_name, "Kilteevan Village");
    }

    #[tokio::test]
    async fn discover_save_files_returns_ok_for_missing_saves_dir() {
        let state = test_app_state();
        let world = state.world.lock().await;
        let saves =
            parish_core::persistence::picker::discover_saves(&state.saves_dir, &world.graph);
        // Missing dir should return empty vec, not panic
        assert!(
            saves.is_empty(),
            "discover_saves should return empty vec for missing directory"
        );
    }
}
