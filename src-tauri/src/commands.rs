//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tauri::Emitter;
use tokio::sync::mpsc;

use parish_core::config::Provider;
use parish_core::debug_snapshot::{self, DebugEvent, DebugSnapshot, InferenceDebug};
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, spawn_inference_worker};
use parish_core::input::{InputResult, classify_input, extract_mention, parse_intent_local};
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::npc::ticks;
use parish_core::world::description::{format_exits, render_description};
use parish_core::world::movement::{self, MovementResult};
use parish_core::world::palette::compute_palette;
use parish_core::world::transport::TransportMode;

use crate::events::{
    EVENT_SAVE_PICKER, EVENT_STREAM_END, EVENT_TEXT_LOG, EVENT_WORLD_UPDATE, NpcReactionPayload,
    StreamEndPayload, TextLogPayload, spawn_loading_animation,
};
use crate::{AppState, MapData, MapLocation, NpcInfo, SaveState, ThemePalette, WorldSnapshot};

/// Capitalizes the first character of a string slice.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Monotonically increasing request ID counter for inference requests.
static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Monotonically increasing message ID counter for text-log entries.
static MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Creates a [`TextLogPayload`] with an auto-generated unique message ID.
fn text_log(source: impl Into<String>, content: impl Into<String>) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        source: source.into(),
        content: content.into(),
    }
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

fn snapshot_from_world(
    world: &parish_core::world::WorldState,
    transport: &TransportMode,
) -> WorldSnapshot {
    use chrono::{Datelike, Timelike};
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
        let exits = format_exits(
            world.player_location,
            &world.graph,
            transport.speed_m_per_s,
            &transport.label,
        );
        format!("{}\n\n{}", desc, exits)
    } else {
        loc.description.clone()
    };

    let day_of_week = match now.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string();

    WorldSnapshot {
        location_name: loc.name.clone(),
        location_description: description,
        time_label: tod.to_string(),
        hour,
        minute,
        weather: weather_str,
        season: season.to_string(),
        festival,
        paused: world.clock.is_paused() || world.clock.is_inference_paused(),
        game_epoch_ms: now.timestamp_millis() as f64,
        speed_factor: world.clock.speed_factor(),
        name_hints: vec![],
        day_of_week,
    }
}

/// Computes contextual name pronunciation hints for the current location.
///
/// Matches pronunciation entries against the current location name and
/// any NPC names present at the player's location.
fn compute_name_hints(
    world: &parish_core::world::WorldState,
    npc_manager: &parish_core::npc::manager::NpcManager,
    pronunciations: &[parish_core::game_mod::PronunciationEntry],
) -> Vec<parish_core::npc::LanguageHint> {
    if pronunciations.is_empty() {
        tracing::debug!("compute_name_hints: no pronunciation entries loaded");
        return vec![];
    }
    let loc = world.current_location();
    let mut names: Vec<&str> = vec![&loc.name];
    let npcs = npc_manager.npcs_at(world.player_location);
    let npc_names: Vec<String> = npcs
        .iter()
        .filter(|n| npc_manager.is_introduced(n.id))
        .map(|n| n.name.clone())
        .collect();
    for name in &npc_names {
        names.push(name);
    }
    let hints: Vec<parish_core::npc::LanguageHint> = pronunciations
        .iter()
        .filter(|entry| entry.matches_any(&names))
        .map(|entry| entry.to_hint())
        .collect();
    tracing::debug!(
        location = %loc.name,
        npc_names = ?npc_names,
        pronunciation_count = pronunciations.len(),
        matched_hints = hints.len(),
        "compute_name_hints"
    );
    hints
}

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

    // Emit the player's own text as a log entry
    let player_msg = text_log("player", format!("> {}", text));
    let player_msg_id = player_msg.id.clone();
    let _ = app.emit(EVENT_TEXT_LOG, player_msg);

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, &state, &app).await;
        }
        InputResult::GameInput(raw) => {
            let raw_for_reactions = raw.clone();
            handle_game_input(raw, state.clone(), app.clone()).await;
            // Generate rule-based NPC reactions to the player's message
            emit_npc_reactions(&player_msg_id, &raw_for_reactions, &state, &app).await;
        }
    }

    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Helper to mask an API key for display, hiding length information for short keys.
fn mask_key(key: &str) -> String {
    if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "****".to_string()
    }
}

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

/// Handles `/command` inputs (pause, resume, status, help, etc.).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use parish_core::input::Command;

    let mut needs_rebuild = false;

    let response = match cmd {
        Command::Pause => {
            let mut world = state.world.lock().await;
            world.clock.pause();
            "The clocks of the parish stand still.".to_string()
        }
        Command::Resume => {
            let mut world = state.world.lock().await;
            world.clock.resume();
            "Time stirs again in the parish.".to_string()
        }
        Command::Status => {
            let world = state.world.lock().await;
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
        Command::Help => [
            "A few things ye might say:",
            "  /branches — List save branches",
            "  /cloud    — Show or change cloud dialogue provider",
            "  /fork <n> — Fork a new timeline branch",
            "  /about    — About the game and credits",
            "  /help     — Show this help",
            "  /improv   — Toggle improv craft for NPC dialogue",
            "  /irish    — Toggle Irish words sidebar",
            "  /key      — Show or change base API key",
            "  /key.<cat>      — Show or change API key for a category",
            "  /load <n> — Load a saved branch",
            "  /log      — Show snapshot history",
            "  /map      — Toggle full parish map overlay (or press M)",
            "  /model    — Show or change base model name",
            "  /model.<cat>    — Show or change model for a category",
            "  /pause    — Hold time still",
            "  /provider — Show or change base LLM provider",
            "  /provider.<cat> — Show or change provider for a category",
            "  /quit     — Take your leave",
            "  /resume   — Let time flow again",
            "  /save     — Save game",
            "  /speed    — Show or change game speed (slow/normal/fast/fastest/ludicrous)",
            "  /status   — Where am I?",
            "",
            "  <cat> = dialogue, simulation, or intent",
        ]
        .join("\n"),
        Command::Quit => {
            app.exit(0);
            return;
        }
        Command::ShowSpeed => {
            let world = state.world.lock().await;
            let s = world
                .clock
                .current_speed()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Custom ({}x)", world.clock.speed_factor()));
            format!("Speed: {}", s)
        }
        Command::SetSpeed(speed) => {
            let mut world = state.world.lock().await;
            world.clock.set_speed(speed);
            speed.activation_message().to_string()
        }
        Command::InvalidSpeed(name) => {
            format!(
                "Unknown speed '{}'. Try: slow, normal, fast, fastest, ludicrous.",
                name
            )
        }
        Command::InvalidBranchName(msg) => msg,

        Command::About => [
            "Parish — A text adventure set in 1820s rural Ireland.",
            "Explore a living village powered by AI-driven NPCs.",
            // TODO: add credits (contributors, libraries, etc.)
            "",
            "Type /help for available commands.",
        ]
        .join("\n"),

        // ── Sidebar & Improv ─────────────────────────────────────────────
        Command::ToggleSidebar => {
            // The GUI sidebar is always visible; this is a no-op with a message.
            "The Irish words panel is managed by the sidebar in the GUI.".to_string()
        }
        Command::ToggleImprov => {
            let mut config = state.config.lock().await;
            config.improv_enabled = !config.improv_enabled;
            if config.improv_enabled {
                "The characters loosen up — improv craft engaged.".to_string()
            } else {
                "The characters settle back to their usual selves.".to_string()
            }
        }

        // ── Base provider/model/key ──────────────────────────────────────
        Command::ShowProvider => {
            let config = state.config.lock().await;
            format!("Provider: {}", config.provider_name)
        }
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let mut config = state.config.lock().await;
                config.base_url = provider.default_base_url().to_string();
                config.provider_name = format!("{:?}", provider).to_lowercase();
                needs_rebuild = true;
                format!("Provider changed to {}.", config.provider_name)
            }
            Err(e) => format!("{}", e),
        },
        Command::ShowModel => {
            let config = state.config.lock().await;
            if config.model_name.is_empty() {
                "Model: (auto-detect)".to_string()
            } else {
                format!("Model: {}", config.model_name)
            }
        }
        Command::SetModel(name) => {
            let mut config = state.config.lock().await;
            config.model_name = name.clone();
            format!("Model changed to {}.", name)
        }
        Command::ShowKey => {
            let config = state.config.lock().await;
            match &config.api_key {
                Some(key) => format!("API key: {}", mask_key(key)),
                None => "API key: (not set)".to_string(),
            }
        }
        Command::SetKey(value) => {
            let mut config = state.config.lock().await;
            config.api_key = Some(value);
            needs_rebuild = true;
            "API key updated.".to_string()
        }

        // ── Cloud provider ───────────────────────────────────────────────
        Command::ShowCloud => {
            let config = state.config.lock().await;
            if let Some(ref provider) = config.cloud_provider_name {
                let model = config.cloud_model_name.as_deref().unwrap_or("(none)");
                format!("Cloud: {} | Model: {}", provider, model)
            } else {
                "No cloud provider configured. Set PARISH_CLOUD_* env vars or use /cloud provider <name>.".to_string()
            }
        }
        Command::SetCloudProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let mut config = state.config.lock().await;
                let base_url = provider.default_base_url().to_string();
                let provider_name = format!("{:?}", provider).to_lowercase();
                config.cloud_provider_name = Some(provider_name.clone());
                config.cloud_base_url = Some(base_url.clone());
                // Rebuild cloud client
                let mut cloud_guard = state.cloud_client.lock().await;
                *cloud_guard = Some(OpenAiClient::new(
                    &base_url,
                    config.cloud_api_key.as_deref(),
                ));
                format!("Cloud provider changed to {}.", provider_name)
            }
            Err(e) => format!("{}", e),
        },
        Command::ShowCloudModel => {
            let config = state.config.lock().await;
            match &config.cloud_model_name {
                Some(model) => format!("Cloud model: {}", model),
                None => "Cloud model: (not set)".to_string(),
            }
        }
        Command::SetCloudModel(name) => {
            let mut config = state.config.lock().await;
            config.cloud_model_name = Some(name.clone());
            format!("Cloud model changed to {}.", name)
        }
        Command::ShowCloudKey => {
            let config = state.config.lock().await;
            match &config.cloud_api_key {
                Some(key) => format!("Cloud API key: {}", mask_key(key)),
                None => "Cloud API key: (not set)".to_string(),
            }
        }
        Command::SetCloudKey(value) => {
            let mut config = state.config.lock().await;
            config.cloud_api_key = Some(value);
            let base_url = config
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api")
                .to_string();
            let mut cloud_guard = state.cloud_client.lock().await;
            *cloud_guard = Some(OpenAiClient::new(
                &base_url,
                config.cloud_api_key.as_deref(),
            ));
            "Cloud API key updated.".to_string()
        }

        // ── Persistence ──────────────────────────────────────────────────
        Command::Save => match do_save_game(state).await {
            Ok(msg) => msg,
            Err(e) => format!("Save failed: {}", e),
        },
        Command::Load(_) => {
            // Emit event to open the save picker in the frontend
            let _ = app.emit(EVENT_SAVE_PICKER, ());
            "Opening save picker...".to_string()
        }
        Command::Fork(name) => {
            let parent_id = state.current_branch_id.lock().await.unwrap_or(1);
            match do_create_branch(state, &name, parent_id).await {
                Ok(msg) => msg,
                Err(e) => format!("Fork failed: {}", e),
            }
        }
        Command::Branches => match do_list_branches_text(state).await {
            Ok(text) => text,
            Err(e) => format!("Failed to list branches: {}", e),
        },
        Command::Log => match do_branch_log_text(state).await {
            Ok(text) => text,
            Err(e) => format!("Failed to show log: {}", e),
        },

        // ── Per-category provider/model/key ──────────────────────────────
        Command::ShowCategoryProvider(cat) => {
            let config = state.config.lock().await;
            let idx = crate::GameConfig::cat_idx(cat);
            match &config.category_provider[idx] {
                Some(p) => format!("{} provider: {}", cat.name(), p),
                None => format!(
                    "{} provider: (inherits base: {})",
                    cat.name(),
                    config.provider_name
                ),
            }
        }
        Command::SetCategoryProvider(cat, name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let mut config = state.config.lock().await;
                let idx = crate::GameConfig::cat_idx(cat);
                let provider_name = format!("{:?}", provider).to_lowercase();
                config.category_provider[idx] = Some(provider_name.clone());
                config.category_base_url[idx] = Some(provider.default_base_url().to_string());
                needs_rebuild = true;
                format!("{} provider changed to {}.", cat.name(), provider_name)
            }
            Err(e) => format!("{}", e),
        },
        Command::ShowCategoryModel(cat) => {
            let config = state.config.lock().await;
            let idx = crate::GameConfig::cat_idx(cat);
            match &config.category_model[idx] {
                Some(m) => format!("{} model: {}", cat.name(), m),
                None => format!(
                    "{} model: (inherits base: {})",
                    cat.name(),
                    config.model_name
                ),
            }
        }
        Command::SetCategoryModel(cat, name) => {
            let mut config = state.config.lock().await;
            let idx = crate::GameConfig::cat_idx(cat);
            config.category_model[idx] = Some(name.clone());
            format!("{} model changed to {}.", cat.name(), name)
        }
        Command::ShowCategoryKey(cat) => {
            let config = state.config.lock().await;
            let idx = crate::GameConfig::cat_idx(cat);
            match &config.category_api_key[idx] {
                Some(key) => format!("{} API key: {}", cat.name(), mask_key(key)),
                None => format!("{} API key: (not set)", cat.name()),
            }
        }
        Command::SetCategoryKey(cat, value) => {
            let cat_name = cat.name().to_string();
            let mut config = state.config.lock().await;
            let idx = crate::GameConfig::cat_idx(cat);
            config.category_api_key[idx] = Some(value);
            needs_rebuild = true;
            format!("{} API key updated.", cat_name)
        }

        // ── Spinner (debug/preview) ──────────────────────────────────────
        Command::Spinner(secs) => {
            let app_handle = app.clone();
            let cancel = tokio_util::sync::CancellationToken::new();
            spawn_loading_animation(app_handle.clone(), cancel.clone());
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                cancel.cancel();
            });
            format!("Showing spinner for {} seconds…", secs)
        }

        // ── Map overlay ──────────────────────────────────────────────────
        Command::Map => {
            let _ = app.emit(crate::events::EVENT_TOGGLE_MAP, ());
            return; // No text log for map toggle
        }

        // ── Debug ────────────────────────────────────────────────────────
        Command::Debug(_) => "Debug commands are not available in the GUI.".to_string(),
        Command::NpcsHere => {
            let world = state.world.lock().await;
            let npc_mgr = state.npc_manager.lock().await;
            let npcs = npc_mgr.npcs_at(world.player_location);
            if npcs.is_empty() {
                "No one else is here.".to_string()
            } else {
                let mut lines = vec!["NPCs here:".to_string()];
                for npc in &npcs {
                    let display = npc_mgr.display_name(npc);
                    let intro = if npc_mgr.is_introduced(npc.id) {
                        " [introduced]"
                    } else {
                        ""
                    };
                    lines.push(format!(
                        "  {} — {} ({}){}",
                        display, npc.occupation, npc.mood, intro
                    ));
                }
                lines.join("\n")
            }
        }
        Command::Time => {
            use chrono::Timelike;
            let world = state.world.lock().await;
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            let season = world.clock.season();
            let festival = world
                .clock
                .check_festival()
                .map(|f| f.to_string())
                .unwrap_or_else(|| "none".to_string());
            let paused = if world.clock.is_paused() {
                " (PAUSED)"
            } else {
                ""
            };
            format!(
                "{:02}:{:02} {} — {}{}\nWeather: {}\nSpeed: {}x\nFestival: {}",
                now.hour(),
                now.minute(),
                tod,
                season,
                paused,
                world.weather,
                world.clock.speed_factor(),
                festival
            )
        }
        Command::Wait(minutes) => {
            use chrono::Timelike;
            let mut world = state.world.lock().await;
            let mut npc_mgr = state.npc_manager.lock().await;
            world.clock.advance(minutes as i64);
            npc_mgr.assign_tiers(&world, &[]);
            let _events = npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
            let now = world.clock.now();
            let tod = world.clock.time_of_day();
            format!(
                "You wait for {} minutes...\nIt is now {:02}:{:02} {}.",
                minutes,
                now.hour(),
                now.minute(),
                tod
            )
        }
        Command::Tick => {
            let world = state.world.lock().await;
            let mut npc_mgr = state.npc_manager.lock().await;
            npc_mgr.assign_tiers(&world, &[]);
            let events = npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
            let count = events.len();
            if count == 0 {
                "No NPC activity.".to_string()
            } else {
                format!("{} schedule event(s) processed.", count)
            }
        }
        Command::NewGame => "New game is not yet available in the GUI.".to_string(),
    };

    if needs_rebuild {
        rebuild_inference(state).await;
    }

    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            source: "system".to_string(),
            content: response,
        },
    );

    // Emit updated world state for status bar
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

/// Resolves movement to a named location.
async fn handle_movement(target: &str, state: &Arc<AppState>, app: &tauri::AppHandle) {
    let transport = state.transport.default_mode().clone();

    // Resolve and apply movement within a single lock to prevent TOCTOU races.
    // Without this, another task could modify the world between resolve and apply,
    // potentially putting the player at an invalid destination.
    let result = {
        let mut world = state.world.lock().await;
        let mv =
            movement::resolve_movement(target, &world.graph, world.player_location, &transport);
        if let MovementResult::Arrived {
            destination,
            minutes,
            ..
        } = &mv
        {
            world.clock.advance(*minutes as i64);
            world.player_location = *destination;
            world.mark_visited(*destination);

            // Update legacy locations map (clone data first to avoid borrow conflict)
            let new_loc = world
                .graph
                .get(*destination)
                .map(|data| parish_core::world::Location {
                    id: *destination,
                    name: data.name.clone(),
                    description: data.description_template.clone(),
                    indoor: data.indoor,
                    public: data.public,
                    lat: data.lat,
                    lon: data.lon,
                });
            if let Some(loc) = new_loc {
                world.locations.entry(*destination).or_insert(loc);
            }
        }
        mv
    };

    match result {
        MovementResult::Arrived { narration, .. } => {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    source: "system".to_string(),
                    content: narration,
                },
            );

            // Reassign tiers after movement
            {
                let world = state.world.lock().await;
                let mut npc_manager = state.npc_manager.lock().await;
                let tier_transitions = npc_manager.assign_tiers(&world, &[]);
                if !tier_transitions.is_empty() {
                    let mut debug_events = state.debug_events.lock().await;
                    for tt in &tier_transitions {
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
            }

            // Emit arrival description
            handle_look(state, app).await;

            // Emit updated world snapshot
            {
                let world = state.world.lock().await;
                let npc_manager = state.npc_manager.lock().await;
                let mut snapshot = snapshot_from_world(&world, &transport);
                snapshot.name_hints =
                    compute_name_hints(&world, &npc_manager, &state.pronunciations);
                let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
            }
        }
        MovementResult::AlreadyHere => {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    source: "system".to_string(),
                    content: "Sure, you're already standing right here.".to_string(),
                },
            );
        }
        MovementResult::NotFound(name) => {
            let world = state.world.lock().await;
            let exits = format_exits(
                world.player_location,
                &world.graph,
                transport.speed_m_per_s,
                &transport.label,
            );
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
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
        let npc_display: Vec<String> = npc_manager
            .npcs_at(world.player_location)
            .iter()
            .map(|n| npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        render_description(loc_data, tod, &weather, &npc_names)
    } else {
        world.current_location().description.clone()
    };

    let transport = state.transport.default_mode();
    let exits = format_exits(
        world.player_location,
        &world.graph,
        transport.speed_m_per_s,
        &transport.label,
    );

    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            source: "system".to_string(),
            content: format!("{}\n{}", desc, exits),
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
    let (npc_name, npc_id, system_prompt, context, queue) = {
        let world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;

        let npcs_here = npc_manager.npcs_at(world.player_location);

        // If an @mention was provided, try to find that NPC; otherwise first NPC
        let npc = if let Some(ref name) = target_name {
            npc_manager
                .find_by_name(name, world.player_location)
                .cloned()
                .or_else(|| npcs_here.first().cloned().cloned())
        } else {
            npcs_here.first().cloned().cloned()
        };

        if let (Some(npc), Some(q)) = (npc, queue.clone()) {
            let display = npc_manager.display_name(&npc).to_string();
            let id = npc.id;
            let other_npcs: Vec<&parish_core::npc::Npc> =
                npcs_here.into_iter().filter(|n| n.id != npc.id).collect();
            let system = ticks::build_enhanced_system_prompt(&npc, config.improv_enabled);
            let ctx = ticks::build_enhanced_context(&npc, &world, &raw, &other_npcs);
            // Mark NPC as introduced on first conversation
            npc_manager.mark_introduced(id);
            (Some(display), Some(id), Some(system), Some(ctx), Some(q))
        } else {
            (None, None, None, None, None)
        }
    };

    let (Some(npc_name), Some(_npc_id), Some(system_prompt), Some(context), Some(queue)) =
        (npc_name, npc_id, system_prompt, context, queue)
    else {
        // No NPC present or no inference queue — show idle message
        let idle_messages = [
            "The wind stirs, but nothing else.",
            "Only the sound of a distant crow.",
            "A dog barks somewhere beyond the hill.",
            "The clouds shift. The parish carries on.",
        ];
        let idx = REQUEST_ID.fetch_add(1, Ordering::Relaxed) as usize % idle_messages.len();
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                source: "system".to_string(),
                content: idle_messages[idx].to_string(),
            },
        );
        return;
    };

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
