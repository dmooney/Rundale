//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::Emitter;
use tokio::sync::mpsc;

use parish_core::input::{InputResult, classify_input, parse_intent_local};
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::ticks;
use parish_core::world::description::{format_exits, render_description};
use parish_core::world::movement::{self, MovementResult};
use parish_core::world::palette::compute_palette;

use crate::events::{
    EVENT_LOADING, EVENT_STREAM_END, EVENT_TEXT_LOG, EVENT_WORLD_UPDATE, LoadingPayload,
    StreamEndPayload, TextLogPayload,
};
use crate::{AppState, MapData, MapLocation, NpcInfo, ThemePalette, WorldSnapshot};

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

// ── Helper: build a WorldSnapshot from locked world state ────────────────────

/// Builds a [`WorldSnapshot`] from a locked world state reference.
///
/// Used both by the `get_world_snapshot` command and by the background
/// idle-tick task in `lib.rs`.
pub fn get_world_snapshot_inner(world: &parish_core::world::WorldState) -> WorldSnapshot {
    snapshot_from_world(world)
}

fn snapshot_from_world(world: &parish_core::world::WorldState) -> WorldSnapshot {
    use chrono::Timelike;
    use parish_core::world::description::{format_exits, render_description};

    let now = world.clock.now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let tod = world.clock.time_of_day();
    let season = world.clock.season();
    let festival = world.clock.check_festival().map(|f| f.to_string());
    let weather_str = world.weather.to_string();

    let loc = world.current_location();
    // Render the description template with current game state + exits
    let description = if let Some(data) = world.current_location_data() {
        let desc = render_description(data, tod, &weather_str, &[]);
        let exits = format_exits(world.player_location, &world.graph);
        format!("{}\n\n{}", desc, exits)
    } else {
        loc.description.clone()
    };

    WorldSnapshot {
        location_name: loc.name.clone(),
        location_description: description,
        time_label: tod.to_string(),
        hour,
        minute,
        weather: weather_str,
        season: season.to_string(),
        festival,
        paused: world.clock.is_paused(),
        game_epoch_ms: now.timestamp_millis() as f64,
        speed_factor: world.clock.speed_factor(),
    }
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Returns a snapshot of the current world state (location, time, weather, season).
#[tauri::command]
pub async fn get_world_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<WorldSnapshot, String> {
    let world = state.world.lock().await;
    Ok(snapshot_from_world(&world))
}

/// Returns the map data: all locations with coordinates, edges, and player position.
#[tauri::command]
pub async fn get_map(state: tauri::State<'_, Arc<AppState>>) -> Result<MapData, String> {
    let world = state.world.lock().await;
    let player_loc = world.player_location;

    // Collect adjacent location IDs
    let adjacent_ids: std::collections::HashSet<parish_core::world::LocationId> = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let locations: Vec<MapLocation> = world
        .graph
        .location_ids()
        .into_iter()
        .filter_map(|id| world.graph.get(id).map(|data| (id, data)))
        .map(|(id, data)| MapLocation {
            id: id.0.to_string(),
            name: data.name.clone(),
            lat: data.lat,
            lon: data.lon,
            adjacent: adjacent_ids.contains(&id) || id == player_loc,
        })
        .collect();

    // Collect edges as (source_id, target_id) string pairs
    let mut edges: Vec<(String, String)> = Vec::new();
    for loc_id in world.graph.location_ids() {
        for (neighbor_id, _conn) in world.graph.neighbors(loc_id) {
            // Only add each edge once (lower id first)
            if loc_id.0 < neighbor_id.0 {
                edges.push((loc_id.0.to_string(), neighbor_id.0.to_string()));
            }
        }
    }

    Ok(MapData {
        locations,
        edges,
        player_location: player_loc.0.to_string(),
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
        .map(|npc| NpcInfo {
            name: npc.name.clone(),
            occupation: npc.occupation.clone(),
            mood: npc.mood.clone(),
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

    // Emit the player's own text as a log entry
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            source: "player".to_string(),
            content: format!("> {}", text),
        },
    );

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, &state, &app).await;
        }
        InputResult::GameInput(raw) => {
            handle_game_input(raw, state, app).await;
        }
    }

    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Handles `/command` inputs (pause, resume, status, help, etc.).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use parish_core::input::Command;

    let response = {
        let mut world = state.world.lock().await;
        match cmd {
            Command::Pause => {
                world.clock.pause();
                "The clocks of the parish stand still.".to_string()
            }
            Command::Resume => {
                world.clock.resume();
                "Time stirs again in the parish.".to_string()
            }
            Command::Status => {
                let tod = world.clock.time_of_day();
                let season = world.clock.season();
                let loc = world.current_location().name.clone();
                let paused = if world.clock.is_paused() {
                    " (paused)"
                } else {
                    ""
                };
                format!("Location: {} | {} | {}{}", loc, tod, season, paused)
            }
            Command::Help => {
                "Commands: /pause /resume /status /help — or just speak naturally.".to_string()
            }
            Command::Quit => {
                app.exit(0);
                return;
            }
            Command::ShowSpeed => {
                let s = world
                    .clock
                    .current_speed()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Custom ({}x)", world.clock.speed_factor()));
                format!("Speed: {}", s)
            }
            Command::SetSpeed(speed) => {
                world.clock.set_speed(speed);
                speed.activation_message().to_string()
            }
            Command::InvalidSpeed(name) => {
                format!(
                    "Unknown speed '{}'. Try: slow, normal, fast, fastest.",
                    name
                )
            }
            _ => "That command isn't available in the Tauri GUI yet.".to_string(),
        }
    };

    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            source: "system".to_string(),
            content: response,
        },
    );

    // Emit updated world state for status bar
    let world = state.world.lock().await;
    let _ = app.emit(EVENT_WORLD_UPDATE, snapshot_from_world(&world));
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

    // Try NPC conversation
    handle_npc_conversation(raw, state, app).await;
}

/// Resolves movement to a named location.
async fn handle_movement(target: &str, state: &Arc<AppState>, app: &tauri::AppHandle) {
    let result = {
        let world = state.world.lock().await;
        movement::resolve_movement(target, &world.graph, world.player_location)
    };

    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            // Advance clock and update player location
            {
                let mut world = state.world.lock().await;
                world.clock.advance(minutes as i64);
                world.player_location = destination;

                // Update legacy locations map (clone data first to avoid borrow conflict)
                let new_loc =
                    world
                        .graph
                        .get(destination)
                        .map(|data| parish_core::world::Location {
                            id: destination,
                            name: data.name.clone(),
                            description: data.description_template.clone(),
                            indoor: data.indoor,
                            public: data.public,
                        });
                if let Some(loc) = new_loc {
                    world.locations.entry(destination).or_insert(loc);
                }
            }

            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    source: "system".to_string(),
                    content: narration,
                },
            );

            // Emit arrival description
            handle_look(state, app).await;

            // Emit updated world snapshot
            let world = state.world.lock().await;
            let _ = app.emit(EVENT_WORLD_UPDATE, snapshot_from_world(&world));
        }
        MovementResult::AlreadyHere => {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    source: "system".to_string(),
                    content: "Sure, you're already standing right here.".to_string(),
                },
            );
        }
        MovementResult::NotFound(name) => {
            let world = state.world.lock().await;
            let exits = format_exits(world.player_location, &world.graph);
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    source: "system".to_string(),
                    content: format!(
                        "You haven't the faintest notion how to reach \"{}\". {}",
                        name, exits
                    ),
                },
            );
        }
    }
}

/// Renders the current location description and exits.
async fn handle_look(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    let desc = if let Some(loc_data) = world.current_location_data() {
        let tod = world.clock.time_of_day();
        let weather = world.weather.to_string();
        let npc_names: Vec<&str> = npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        render_description(loc_data, tod, &weather, &npc_names)
    } else {
        world.current_location().description.clone()
    };

    let exits = format_exits(world.player_location, &world.graph);

    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            source: "system".to_string(),
            content: format!("{}\n{}", desc, exits),
        },
    );
}

/// Routes input to the NPC at the player's location, or shows idle message.
async fn handle_npc_conversation(
    raw: String,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) {
    let (npc_name, system_prompt, context, queue) = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;

        let npcs_here = npc_manager.npcs_at(world.player_location);
        let npc = npcs_here.first().cloned().cloned();

        if let (Some(npc), Some(q)) = (npc, queue.clone()) {
            let other_npcs: Vec<&parish_core::npc::Npc> =
                npcs_here.into_iter().filter(|n| n.id != npc.id).collect();
            let system = ticks::build_enhanced_system_prompt(&npc, false);
            let ctx = ticks::build_enhanced_context(&npc, &world, &raw, &other_npcs);
            (Some(npc.name.clone()), Some(system), Some(ctx), Some(q))
        } else {
            (None, None, None, None)
        }
    };

    let (Some(npc_name), Some(system_prompt), Some(context), Some(queue)) =
        (npc_name, system_prompt, context, queue)
    else {
        // No NPC present or no inference queue — show idle message
        let idle_messages = [
            "The wind stirs, but nothing else.",
            "Only the sound of a distant crow.",
            "A dog barks somewhere beyond the hill.",
            "The clouds shift. The parish carries on.",
        ];
        let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % idle_messages.len();
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                source: "system".to_string(),
                content: idle_messages[idx].to_string(),
            },
        );
        return;
    };

    let model = state.model_name.clone();
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    let _ = app.emit(EVENT_LOADING, LoadingPayload { active: true });

    let (token_tx, token_rx) = mpsc::unbounded_channel::<String>();

    // Emit NPC name prefix as the start of the streaming entry
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            source: npc_name.clone(),
            content: String::new(),
        },
    );

    match queue
        .send(req_id, model, context, Some(system_prompt), Some(token_tx))
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
                if resp.error.is_some() {
                    tracing::warn!("Inference error: {:?}", resp.error);
                    vec![]
                } else {
                    let parsed = parse_npc_stream_response(&resp.text);
                    parsed.metadata.map(|m| m.irish_words).unwrap_or_default()
                }
            } else {
                vec![]
            };

            let _ = app.emit(EVENT_STREAM_END, StreamEndPayload { hints });
        }
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
        }
    }

    let _ = app.emit(EVENT_LOADING, LoadingPayload { active: false });
}
