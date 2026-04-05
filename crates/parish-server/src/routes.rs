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

use parish_core::backend_init::{NpcFallback, load_world_and_npcs};
use parish_core::config::Provider;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::inference::{InferenceQueue, new_inference_log, spawn_inference_worker};
use parish_core::input::{InputResult, classify_input, parse_intent_local};
use parish_core::ipc::{
    LoadingPayload, MapData, NpcInfo, NpcReactionPayload, ReactRequest, StreamEndPayload,
    TextLogPayload, ThemePalette, WorldSnapshot, capitalize_first,
};
use parish_core::npc::parse_npc_stream_response;
use parish_core::npc::reactions;
use parish_core::npc::ticks;
use parish_core::world::description::{format_exits, render_description};

use parish_core::debug_snapshot::{self, DebugSnapshot, InferenceDebug};
use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

use crate::state::{AppState, GameConfig, SaveState};

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

// ── Query endpoints ─────────────────────────────────────────────────────────

/// `GET /api/world-snapshot` — returns the current world snapshot.
pub async fn get_world_snapshot(State(state): State<Arc<AppState>>) -> Json<WorldSnapshot> {
    let world = state.world.lock().await;
    let transport = state.transport.default_mode();
    Json(parish_core::ipc::snapshot_from_world(&world, transport))
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

/// Handles `/command` system inputs.
async fn handle_system_command(cmd: parish_core::input::Command, state: &Arc<AppState>) {
    use parish_core::input::Command;
    use parish_core::ipc::mask_key;

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
            "  /help     — Show this help",
            "  /pause    — Hold time still",
            "  /resume   — Let time flow again",
            "  /speed    — Show or change game speed",
            "  /status   — Where am I?",
        ]
        .join("\n"),
        Command::Quit => {
            "The web server cannot be quit from the game. Close your browser tab.".to_string()
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
                "Unknown speed '{}'. Try: slow, normal, fast, fastest.",
                name
            )
        }
        Command::InvalidBranchName(msg) => msg,
        Command::ToggleSidebar => "The Irish words panel is managed by the sidebar.".to_string(),
        Command::ToggleImprov => {
            let mut config = state.config.lock().await;
            config.improv_enabled = !config.improv_enabled;
            if config.improv_enabled {
                "The characters loosen up — improv craft engaged.".to_string()
            } else {
                "The characters settle back to their usual selves.".to_string()
            }
        }
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
        Command::ShowCloud => {
            let config = state.config.lock().await;
            if let Some(ref provider) = config.cloud_provider_name {
                let model = config.cloud_model_name.as_deref().unwrap_or("(none)");
                format!("Cloud: {} | Model: {}", provider, model)
            } else {
                "No cloud provider configured.".to_string()
            }
        }
        Command::SetCloudProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let mut config = state.config.lock().await;
                let base_url = provider.default_base_url().to_string();
                let provider_name = format!("{:?}", provider).to_lowercase();
                config.cloud_provider_name = Some(provider_name.clone());
                config.cloud_base_url = Some(base_url.clone());
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
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            "Persistence is not yet available in web mode.".to_string()
        }
        Command::ShowCategoryProvider(cat) => {
            let config = state.config.lock().await;
            let idx = GameConfig::cat_idx(cat);
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
                let idx = GameConfig::cat_idx(cat);
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
            let idx = GameConfig::cat_idx(cat);
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
            let idx = GameConfig::cat_idx(cat);
            config.category_model[idx] = Some(name.clone());
            format!("{} model changed to {}.", cat.name(), name)
        }
        Command::ShowCategoryKey(cat) => {
            let config = state.config.lock().await;
            let idx = GameConfig::cat_idx(cat);
            match &config.category_api_key[idx] {
                Some(key) => format!("{} API key: {}", cat.name(), mask_key(key)),
                None => format!("{} API key: (not set)", cat.name()),
            }
        }
        Command::SetCategoryKey(cat, value) => {
            let cat_name = cat.name().to_string();
            let mut config = state.config.lock().await;
            let idx = GameConfig::cat_idx(cat);
            config.category_api_key[idx] = Some(value);
            needs_rebuild = true;
            format!("{} API key updated.", cat_name)
        }
        Command::Debug(_) => "Debug commands are not available in web mode.".to_string(),
        Command::Spinner(_) => "Spinner customization is not available in web mode.".to_string(),
        Command::Map => {
            state.event_bus.emit("toggle-full-map", &());
            String::new()
        }
        Command::About => "Parish — An Irish Living World Text Adventure (web mode).".to_string(),
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
        Command::NewGame => "New game is not yet available in web mode.".to_string(),
    };

    if needs_rebuild {
        rebuild_inference(state).await;
    }

    if !response.is_empty() {
        state
            .event_bus
            .emit("text-log", &text_log("system", response));
    }

    let world = state.world.lock().await;
    let transport = state.transport.default_mode();
    state.event_bus.emit(
        "world-update",
        &parish_core::ipc::snapshot_from_world(&world, transport),
    );
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
        state.event_bus.emit(
            "world-update",
            &parish_core::ipc::snapshot_from_world(&world, &transport),
        );
    }
}

/// Renders the current location description and exits.
async fn handle_look(state: &Arc<AppState>) {
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

    state.event_bus.emit(
        "text-log",
        &text_log("system", format!("{}\n{}", desc, exits)),
    );
}

/// Routes input to the NPC at the player's location, or shows idle message.
async fn handle_npc_conversation(raw: String, state: &Arc<AppState>) {
    let (npc_name, npc_id, system_prompt, context, queue, npc_present) = {
        let world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let queue = state.inference_queue.lock().await;
        let config = state.config.lock().await;

        let npcs_here = npc_manager.npcs_at(world.player_location);
        let npc_present = !npcs_here.is_empty();
        let npc = npcs_here.first().cloned().cloned();

        if let (Some(npc), Some(q)) = (npc, queue.clone()) {
            let display = npc_manager.display_name(&npc).to_string();
            let id = npc.id;
            let other_npcs: Vec<&parish_core::npc::Npc> =
                npcs_here.into_iter().filter(|n| n.id != npc.id).collect();
            let system = ticks::build_enhanced_system_prompt(&npc, config.improv_enabled);
            let ctx = ticks::build_enhanced_context(&npc, &world, &raw, &other_npcs);
            npc_manager.mark_introduced(id);
            (
                Some(display),
                Some(id),
                Some(system),
                Some(ctx),
                Some(q),
                npc_present,
            )
        } else {
            (None, None, None, None, None, npc_present)
        }
    };

    let (Some(npc_name), Some(_npc_id), Some(system_prompt), Some(context), Some(queue)) =
        (npc_name, npc_id, system_prompt, context, queue)
    else {
        // If an NPC is present but inference isn't configured, give a clear message.
        // Otherwise show a generic idle message.
        let content = if npc_present {
            "There's someone here, but the LLM is not configured — set a provider with /provider."
                .to_string()
        } else {
            let idle_messages = [
                "The wind stirs, but nothing else.",
                "Only the sound of a distant crow.",
                "A dog barks somewhere beyond the hill.",
                "The clouds shift. The parish carries on.",
            ];
            let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % idle_messages.len();
            idle_messages[idx].to_string()
        };
        state
            .event_bus
            .emit("text-log", &text_log("system", content));
        return;
    };

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
                    crate::streaming::stream_npc_response(&state_clone.event_bus, token_rx).await
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
        let ws = parish_core::ipc::snapshot_from_world(&world, transport);
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
    let data_dir = state.data_dir.clone();
    let (fresh_world, fresh_npcs) = load_world_and_npcs(
        None,
        &data_dir,
        parish_core::world::LocationId(15),
        NpcFallback::Empty,
    );

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
        let transport = state.transport.default_mode();
        let ws = parish_core::ipc::snapshot_from_world(&world, transport);
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
