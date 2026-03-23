//! GUI mode using egui/eframe.
//!
//! Provides a windowed GUI with an enhanced layout: scrollable chat panel,
//! interactive location map, Irish word sidebar, NPC info, and a text input
//! field. Reuses all shared game logic (WorldState, NpcManager, input parsing,
//! inference streaming).

pub mod chat_panel;
pub mod input_field;
pub mod map_panel;
pub mod sidebar;
pub mod status_bar;
pub mod theme;

pub mod screenshot;

use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use eframe::egui;
use tokio::sync::mpsc;

use crate::config::{CloudConfig, ProviderConfig};
use crate::inference::openai_client::OpenAiClient;
use crate::inference::{self, InferenceClients, InferenceQueue};
use crate::input::{Command, InputResult, classify_input};
use crate::loading::LoadingAnimation;
use crate::npc::manager::{NpcManager, ScheduleEvent, ScheduleEventKind};
use crate::npc::ticks;
use crate::npc::{
    IrishWordHint, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
    parse_npc_stream_response,
};
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use crate::world::{LocationId, WorldState};

use theme::{apply_palette, gui_palette_for_time};

/// Maximum number of debug log entries.
const DEBUG_LOG_CAPACITY: usize = 50;

/// Idle messages shown when no NPC is present.
const IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

/// Main application state for the GUI.
///
/// Wraps shared game state (WorldState, NpcManager, inference) with
/// GUI-specific rendering state. Implements `eframe::App` for the
/// egui event loop.
pub struct GuiApp {
    /// The game world state.
    pub world: WorldState,
    /// Central NPC manager.
    pub npc_manager: NpcManager,
    /// The inference queue for LLM requests.
    pub inference_queue: Option<InferenceQueue>,
    /// The LLM client.
    pub client: Option<OpenAiClient>,
    /// Current model name.
    pub model_name: String,
    /// Provider display name.
    pub provider_name: String,
    /// Provider base URL.
    pub base_url: String,
    /// Provider API key.
    pub api_key: Option<String>,
    /// Cloud provider name for dialogue (None = local only).
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud client for dialogue inference.
    pub cloud_client: Option<OpenAiClient>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// The model name used by the dialogue inference queue.
    pub dialogue_model: String,
    /// Whether improv craft mode is enabled.
    pub improv_enabled: bool,
    /// Irish pronunciation hints from NPC responses.
    pub pronunciation_hints: Vec<IrishWordHint>,
    /// Debug activity log (ring buffer).
    pub debug_log: VecDeque<String>,
    /// Idle message rotation counter.
    pub idle_counter: usize,

    // --- GUI-specific state ---
    /// Current text in the input field.
    pub input_buffer: String,
    /// Whether the map panel is visible.
    pub show_map: bool,
    /// Whether the Irish/NPC sidebar is visible.
    pub show_sidebar: bool,
    /// Whether the debug panel is visible.
    pub show_debug: bool,
    /// Flag to exit the application.
    pub should_quit: bool,

    // --- Async bridge ---
    /// Handle to the tokio runtime for spawning async tasks.
    pub tokio_handle: tokio::runtime::Handle,
    /// Shared buffer for streaming tokens from inference.
    pub streaming_buf: Arc<Mutex<String>>,
    /// Whether streaming is currently active.
    pub streaming_active: Arc<Mutex<bool>>,
    /// Pending Irish word hints from the latest streamed response.
    pub pending_hints: Arc<Mutex<Vec<IrishWordHint>>>,
    /// Monotonic request counter.
    pub request_id: u64,

    /// Loading animation state, active while waiting for LLM inference.
    pub loading_animation: Option<LoadingAnimation>,

    // --- Timing ---
    /// Instant of last player interaction.
    pub last_interaction: std::time::Instant,
    /// Instant of last idle simulation tick.
    pub last_idle_tick: std::time::Instant,

    // --- Screenshot mode ---
    /// When set, captures screenshots and exits.
    pub screenshot_config: Option<screenshot::ScreenshotConfig>,
}

impl GuiApp {
    /// Creates a new GuiApp with default state.
    pub fn new(tokio_handle: tokio::runtime::Handle) -> Self {
        Self {
            world: WorldState::new(),
            npc_manager: NpcManager::new(),
            inference_queue: None,
            client: None,
            model_name: String::new(),
            provider_name: String::from("ollama"),
            base_url: String::new(),
            api_key: None,
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_client: None,
            cloud_api_key: None,
            cloud_base_url: None,
            dialogue_model: String::new(),
            improv_enabled: false,
            pronunciation_hints: Vec::new(),
            debug_log: VecDeque::with_capacity(DEBUG_LOG_CAPACITY),
            idle_counter: 0,
            input_buffer: String::new(),
            show_map: true,
            show_sidebar: true,
            show_debug: false,
            should_quit: false,
            tokio_handle,
            streaming_buf: Arc::new(Mutex::new(String::new())),
            streaming_active: Arc::new(Mutex::new(false)),
            pending_hints: Arc::new(Mutex::new(Vec::new())),
            loading_animation: None,
            request_id: 0,
            last_interaction: std::time::Instant::now(),
            last_idle_tick: std::time::Instant::now(),
            screenshot_config: None,
        }
    }

    /// Pushes an entry to the debug activity log (ring buffer).
    pub fn debug_event(&mut self, msg: String) {
        if self.debug_log.len() >= DEBUG_LOG_CAPACITY {
            self.debug_log.pop_front();
        }
        self.debug_log.push_back(msg);
    }

    /// Drains the streaming buffer into the text log.
    ///
    /// Clears the loading animation once real tokens arrive, or ticks
    /// the animation while still waiting.
    fn drain_streaming_buffer(&mut self) {
        let active = *self.streaming_active.lock().unwrap();
        if active {
            let mut buf = self.streaming_buf.lock().unwrap();
            if !buf.is_empty() {
                // Tokens arrived — clear loading animation
                self.loading_animation = None;
                if let Some(last) = self.world.text_log.last_mut() {
                    last.push_str(&buf);
                }
                buf.clear();
            } else if let Some(anim) = &mut self.loading_animation {
                anim.tick();
            }
        } else {
            // Streaming finished — ensure animation is cleared
            self.loading_animation = None;
        }

        // Drain any pending Irish word hints from completed responses
        let mut pending = self.pending_hints.lock().unwrap();
        if !pending.is_empty() {
            self.pronunciation_hints.splice(0..0, pending.drain(..));
            self.pronunciation_hints.truncate(20);
        }
    }

    /// Runs idle simulation tick if conditions are met.
    fn maybe_idle_tick(&mut self) {
        let idle_interval = std::time::Duration::from_secs(20);
        let is_streaming = *self.streaming_active.lock().unwrap();
        let idle_elapsed = self.last_interaction.elapsed() >= idle_interval;
        let tick_due = self.last_idle_tick.elapsed() >= idle_interval;

        if !is_streaming && idle_elapsed && tick_due && !self.world.clock.is_paused() {
            self.npc_manager
                .assign_tiers(self.world.player_location, &self.world.graph);
            let events = self
                .npc_manager
                .tick_schedules(&self.world.clock, &self.world.graph);
            self.process_schedule_events(&events);
            self.last_idle_tick = std::time::Instant::now();
        }
    }

    /// Processes NPC schedule events (arrivals/departures).
    fn process_schedule_events(&mut self, events: &[ScheduleEvent]) {
        let player_loc = self.world.player_location;
        for event in events {
            self.debug_event(event.debug_string());
            match &event.kind {
                ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                    self.world
                        .log(format!("{} heads off down the road.", event.npc_name));
                }
                ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                    self.world.log(format!("{} arrives.", event.npc_name));
                }
                _ => {}
            }
        }
    }

    /// Shows the location arrival description in the text log.
    fn show_location_arrival(&mut self) {
        let loc_name = self.world.current_location().name.clone();
        self.world.log(format!("— {} —", loc_name));

        if let Some(loc_data) = self.world.current_location_data() {
            let tod = self.world.clock.time_of_day();
            let weather = self.world.weather.clone();
            let npc_names: Vec<&str> = self
                .npc_manager
                .npcs_at(self.world.player_location)
                .iter()
                .map(|n| n.name.as_str())
                .collect();
            let desc = render_description(loc_data, tod, &weather, &npc_names);
            self.world.log(desc);
        } else {
            let desc = self.world.current_location().description.clone();
            self.world.log(desc);
        }

        for npc in self.npc_manager.npcs_at(self.world.player_location) {
            self.world.log(format!("{} is here.", npc.name));
        }

        let exits = format_exits(self.world.player_location, &self.world.graph);
        self.world.log(exits);
        self.world.log(String::new());
    }

    /// Shows location description (for /look).
    fn show_location_description(&mut self) {
        if let Some(loc_data) = self.world.current_location_data() {
            let tod = self.world.clock.time_of_day();
            let weather = self.world.weather.clone();
            let npc_names: Vec<&str> = self
                .npc_manager
                .npcs_at(self.world.player_location)
                .iter()
                .map(|n| n.name.as_str())
                .collect();
            let desc = render_description(loc_data, tod, &weather, &npc_names);
            self.world.log(desc);
        } else {
            let desc = self.world.current_location().description.clone();
            self.world.log(desc);
        }
        let exits = format_exits(self.world.player_location, &self.world.graph);
        self.world.log(exits);
    }

    /// Handles a movement command.
    fn handle_movement(&mut self, target: &str) {
        let result =
            movement::resolve_movement(target, &self.world.graph, self.world.player_location);
        match result {
            MovementResult::Arrived {
                destination,
                minutes,
                narration,
                ..
            } => {
                self.world.log(narration);
                self.world.log(String::new());
                self.world.clock.advance(minutes as i64);
                self.world.player_location = destination;

                if let Some(data) = self.world.graph.get(destination) {
                    self.world.locations.entry(destination).or_insert_with(|| {
                        crate::world::Location {
                            id: destination,
                            name: data.name.clone(),
                            description: data.description_template.clone(),
                            indoor: data.indoor,
                            public: data.public,
                        }
                    });
                }
                self.show_location_arrival();
            }
            MovementResult::AlreadyHere => {
                self.world
                    .log("Sure, you're already standing right here.".to_string());
            }
            MovementResult::NotFound(name) => {
                self.world.log(format!(
                    "You haven't the faintest notion how to reach \"{}\". Try asking about.",
                    name
                ));
                let exits = format_exits(self.world.player_location, &self.world.graph);
                self.world.log(exits);
            }
        }
    }

    /// Handles a system command. Returns true if inference pipeline needs rebuild.
    fn handle_system_command(&mut self, cmd: Command) -> bool {
        let mut rebuild = false;
        match cmd {
            Command::Quit => {
                self.world
                    .log("Safe home to ye. May the road rise to meet you.".to_string());
                self.should_quit = true;
            }
            Command::Pause => {
                self.world.clock.pause();
                self.world
                    .log("The clocks of the parish stand still.".to_string());
            }
            Command::Resume => {
                self.world.clock.resume();
                self.world
                    .log("Time stirs again in the parish.".to_string());
            }
            Command::ShowSpeed => {
                let speed_name = self
                    .world
                    .clock
                    .current_speed()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("Custom ({}x)", self.world.clock.speed_factor()));
                self.world
                    .log(format!("The parish moves at {} pace.", speed_name));
            }
            Command::SetSpeed(speed) => {
                use crate::world::time::GameSpeed;
                self.world.clock.set_speed(speed);
                let msg = match speed {
                    GameSpeed::Slow => "The parish slows to a gentle amble.",
                    GameSpeed::Normal => "The parish settles into its natural stride.",
                    GameSpeed::Fast => "The parish quickens its step.",
                    GameSpeed::Fastest => "The parish fair flies — hold onto your hat!",
                };
                self.world.log(msg.to_string());
            }
            Command::Status => {
                let time = self.world.clock.time_of_day();
                let season = self.world.clock.season();
                let loc = self.world.current_location().name.clone();
                let paused = if self.world.clock.is_paused() {
                    " (paused)"
                } else {
                    ""
                };
                self.world.log(format!(
                    "Location: {} | {} | {} {}",
                    loc, time, season, paused
                ));
            }
            Command::Help => {
                self.world.log("A few things ye might say:".to_string());
                self.world.log("  /quit     — Take your leave".to_string());
                self.world.log("  /pause    — Hold time still".to_string());
                self.world
                    .log("  /resume   — Let time flow again".to_string());
                self.world.log(
                    "  /speed    — Show or change game speed (slow/normal/fast/fastest)"
                        .to_string(),
                );
                self.world.log("  /status   — Where am I?".to_string());
                self.world
                    .log("  /improv   — Toggle improv craft mode".to_string());
                self.world
                    .log("  /provider — Show or change LLM provider".to_string());
                self.world
                    .log("  /model    — Show or change model name".to_string());
                self.world
                    .log("  /key      — Show or change API key".to_string());
                self.world.log("  /help     — Show this help".to_string());
            }
            Command::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
            }
            Command::ToggleImprov => {
                self.improv_enabled = !self.improv_enabled;
                if self.improv_enabled {
                    self.world
                        .log("The characters loosen up — improv craft engaged.".to_string());
                } else {
                    self.world
                        .log("The characters settle back to their usual selves.".to_string());
                }
            }
            Command::ShowProvider => {
                self.world.log(format!("Provider: {}", self.provider_name));
            }
            Command::SetProvider(name) => match crate::config::Provider::from_str_loose(&name) {
                Ok(provider) => {
                    self.base_url = provider.default_base_url().to_string();
                    self.provider_name = format!("{:?}", provider).to_lowercase();
                    self.client = Some(OpenAiClient::new(&self.base_url, self.api_key.as_deref()));
                    rebuild = true;
                    self.world
                        .log(format!("Provider changed to {}.", self.provider_name));
                }
                Err(e) => {
                    self.world.log(format!("{}", e));
                }
            },
            Command::ShowModel => {
                if self.model_name.is_empty() {
                    self.world.log("Model: (auto-detect)".to_string());
                } else {
                    self.world.log(format!("Model: {}", self.model_name));
                }
            }
            Command::SetModel(name) => {
                self.model_name = name.clone();
                self.world.log(format!("Model changed to {}.", name));
            }
            Command::ShowKey => match &self.api_key {
                Some(key) if key.len() > 8 => {
                    let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                    self.world.log(format!("API key: {}", masked));
                }
                Some(_) => {
                    self.world
                        .log("API key: (set, too short to mask)".to_string());
                }
                None => {
                    self.world.log("API key: (not set)".to_string());
                }
            },
            Command::SetKey(value) => {
                self.api_key = Some(value);
                self.client = Some(OpenAiClient::new(&self.base_url, self.api_key.as_deref()));
                rebuild = true;
                self.world.log("API key updated.".to_string());
            }
            Command::Save
            | Command::Fork(_)
            | Command::Load(_)
            | Command::Branches
            | Command::Log => {
                self.world.log(
                    "That particular skill hasn't arrived in the parish yet. Patience now."
                        .to_string(),
                );
            }
            Command::Debug(sub) => {
                if sub.as_deref() == Some("panel") {
                    self.show_debug = !self.show_debug;
                    let state = if self.show_debug { "visible" } else { "hidden" };
                    self.world.log(format!("Debug panel {}.", state));
                } else {
                    self.world
                        .log("Debug commands not yet available in GUI mode.".to_string());
                }
            }
            Command::ShowCloud => {
                if let Some(ref provider) = self.cloud_provider_name {
                    let model = self.cloud_model_name.as_deref().unwrap_or("(none)");
                    self.world
                        .log(format!("Cloud: {} | Model: {}", provider, model));
                } else {
                    self.world.log(
                        "No cloud provider configured. Use --cloud-provider or parish.toml [cloud]."
                            .to_string(),
                    );
                }
            }
            Command::SetCloudProvider(name) => {
                match crate::config::Provider::from_str_loose(&name) {
                    Ok(provider) => {
                        let base_url = provider.default_base_url().to_string();
                        self.cloud_provider_name = Some(format!("{:?}", provider).to_lowercase());
                        self.cloud_base_url = Some(base_url.clone());
                        self.cloud_client =
                            Some(OpenAiClient::new(&base_url, self.cloud_api_key.as_deref()));
                        rebuild = true;
                        self.world.log(format!(
                            "Cloud provider changed to {}.",
                            self.cloud_provider_name.as_deref().unwrap()
                        ));
                    }
                    Err(e) => {
                        self.world.log(format!("{}", e));
                    }
                }
            }
            Command::ShowCloudModel => {
                if let Some(ref model) = self.cloud_model_name {
                    self.world.log(format!("Cloud model: {}", model));
                } else {
                    self.world.log("Cloud model: (not set)".to_string());
                }
            }
            Command::SetCloudModel(name) => {
                self.cloud_model_name = Some(name.clone());
                self.dialogue_model = name.clone();
                self.world.log(format!("Cloud model changed to {}.", name));
            }
            Command::ShowCloudKey => match &self.cloud_api_key {
                Some(key) if key.len() > 8 => {
                    let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                    self.world.log(format!("Cloud API key: {}", masked));
                }
                Some(_) => {
                    self.world
                        .log("Cloud API key: (set, too short to mask)".to_string());
                }
                None => {
                    self.world.log("Cloud API key: (not set)".to_string());
                }
            },
            Command::SetCloudKey(value) => {
                self.cloud_api_key = Some(value);
                let base_url = self
                    .cloud_base_url
                    .as_deref()
                    .unwrap_or("https://openrouter.ai/api");
                self.cloud_client =
                    Some(OpenAiClient::new(base_url, self.cloud_api_key.as_deref()));
                rebuild = true;
                self.world.log("Cloud API key updated.".to_string());
            }
        }
        self.world.log(String::new());
        rebuild
    }

    /// Processes submitted player input (called from the egui update loop).
    fn process_input(&mut self, raw_input: String) {
        self.last_interaction = std::time::Instant::now();
        self.world.log(format!("> {}", raw_input));

        match classify_input(&raw_input) {
            InputResult::SystemCommand(cmd) => {
                let rebuild = self.handle_system_command(cmd);
                if rebuild {
                    // Rebuild dialogue queue: prefer cloud client, fall back to local
                    let dial_client = self.cloud_client.clone().or_else(|| self.client.clone());
                    if let Some(new_client) = dial_client {
                        let (tx, rx) = mpsc::channel(32);
                        let _worker = inference::spawn_inference_worker(new_client, rx);
                        self.inference_queue = Some(InferenceQueue::new(tx));
                    }
                }
            }
            InputResult::GameInput(text) => {
                self.process_game_input(text);
            }
        }

        // Simulation tick after each action
        self.npc_manager
            .assign_tiers(self.world.player_location, &self.world.graph);
        let events = self
            .npc_manager
            .tick_schedules(&self.world.clock, &self.world.graph);
        self.process_schedule_events(&events);
    }

    /// Processes a game input (non-system-command).
    fn process_game_input(&mut self, text: String) {
        // Try local intent parsing first (synchronous for move/look)
        let lower = text.to_lowercase();
        let trimmed = lower.trim();

        // Quick local move detection
        let move_prefixes = ["go ", "walk ", "move ", "travel ", "head "];
        let is_move = move_prefixes.iter().any(|p| trimmed.starts_with(p));

        if is_move {
            let target = move_prefixes
                .iter()
                .find_map(|p| trimmed.strip_prefix(p))
                .unwrap_or(trimmed);
            self.handle_movement(target);
            self.world.log(String::new());
            return;
        }

        if trimmed == "look" || trimmed == "l" || trimmed.starts_with("look around") {
            self.show_location_description();
            self.world.log(String::new());
            return;
        }

        // NPC conversation
        let npcs_here = self.npc_manager.npcs_at(self.world.player_location);
        let npc = npcs_here.first().cloned().cloned();

        if let Some(npc) = npc {
            let other_npcs: Vec<_> = self
                .npc_manager
                .npcs_at(self.world.player_location)
                .into_iter()
                .filter(|n| n.id != npc.id)
                .collect();
            let system_prompt = ticks::build_enhanced_system_prompt(&npc, self.improv_enabled);
            let context = ticks::build_enhanced_context(&npc, &self.world, &text, &other_npcs);

            if let Some(queue) = &self.inference_queue {
                self.request_id += 1;
                let request_id = self.request_id;

                let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

                self.world.log(format!("{}: ", npc.name));
                *self.streaming_active.lock().unwrap() = true;
                self.loading_animation = Some(LoadingAnimation::new());

                let buf_clone = Arc::clone(&self.streaming_buf);
                let active_clone = Arc::clone(&self.streaming_active);
                let hints_clone = Arc::clone(&self.pending_hints);

                // Spawn the token accumulator task
                self.tokio_handle.spawn(async move {
                    let mut accumulated = String::new();
                    let mut displayed_len: usize = 0;
                    let mut separator_found = false;

                    while let Some(token) = token_rx.recv().await {
                        accumulated.push_str(&token);

                        if separator_found {
                            continue;
                        }

                        if let Some((dialogue_end, _meta_start)) =
                            find_response_separator(&accumulated)
                        {
                            if dialogue_end > displayed_len {
                                let new_text = accumulated[displayed_len..dialogue_end].to_string();
                                buf_clone.lock().unwrap().push_str(&new_text);
                            }
                            separator_found = true;
                            continue;
                        }

                        let raw_end = accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                        let safe_end = floor_char_boundary(&accumulated, raw_end);
                        if safe_end > displayed_len {
                            let new_text = accumulated[displayed_len..safe_end].to_string();
                            buf_clone.lock().unwrap().push_str(&new_text);
                            displayed_len = safe_end;
                        }
                    }

                    if !separator_found && displayed_len < accumulated.len() {
                        let remaining = accumulated[displayed_len..].to_string();
                        buf_clone.lock().unwrap().push_str(&remaining);
                    }

                    // Parse metadata for Irish word hints
                    let parsed = parse_npc_stream_response(&accumulated);
                    if let Some(meta) = &parsed.metadata
                        && !meta.irish_words.is_empty()
                    {
                        let mut hints = hints_clone.lock().unwrap();
                        hints.extend(meta.irish_words.clone());
                    }

                    *active_clone.lock().unwrap() = false;
                });

                // Send inference request (fire-and-forget from the GUI thread)
                let queue_clone = queue.clone();
                let model = self.dialogue_model.clone();
                self.tokio_handle.spawn(async move {
                    match queue_clone
                        .send(
                            request_id,
                            model,
                            context,
                            Some(system_prompt),
                            Some(token_tx),
                        )
                        .await
                    {
                        Ok(rx) => {
                            // Wait for the response (consumed by the token stream)
                            let _ = rx.await;
                        }
                        Err(e) => {
                            tracing::error!("Failed to send inference request: {}", e);
                        }
                    }
                });
            } else {
                self.world
                    .log("[No storyteller could be found in the parish today.]".to_string());
            }
        } else {
            let msg = IDLE_MESSAGES[self.idle_counter % IDLE_MESSAGES.len()];
            self.world.log(msg.to_string());
            self.idle_counter += 1;
        }
        self.world.log(String::new());
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Drain streaming buffer
        self.drain_streaming_buffer();

        // Idle tick
        self.maybe_idle_tick();

        // Apply time-of-day palette
        let palette = gui_palette_for_time(&self.world.clock.time_of_day());
        apply_palette(ctx, &palette);

        // Request continuous repaint while streaming
        if *self.streaming_active.lock().unwrap() {
            ctx.request_repaint();
        } else {
            // Repaint every 500ms for clock updates
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }

        // Check quit
        if self.should_quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }

        // --- Layout ---

        // Top status bar
        egui::TopBottomPanel::top("status_bar").show(ctx, |ui| {
            status_bar::draw_status_bar(ui, &self.world, &palette);
        });

        // Bottom input field
        let submitted = {
            let mut submitted_text = None;
            egui::TopBottomPanel::bottom("input_panel").show(ctx, |ui| {
                submitted_text =
                    input_field::draw_input_field(ui, &mut self.input_buffer, &palette);
            });
            submitted_text
        };

        // Right sidebar (map + Irish words + NPC info)
        if self.show_sidebar || self.show_map {
            egui::SidePanel::right("right_panel")
                .min_width(250.0)
                .default_width(320.0)
                .show(ctx, |ui| {
                    if self.show_map {
                        let map_height = ui.available_height() * 0.55;
                        ui.allocate_ui(egui::vec2(ui.available_width(), map_height), |ui| {
                            let clicked = map_panel::draw_map_panel(
                                ui,
                                &self.world.graph,
                                self.world.player_location,
                                &self.npc_manager,
                                &palette,
                            );
                            if let Some(dest) = clicked
                                && let Some(loc_data) = self.world.graph.get(dest)
                            {
                                let target = loc_data.name.clone();
                                self.handle_movement(&target);
                            }
                        });
                        ui.separator();
                    }
                    if self.show_sidebar {
                        sidebar::draw_sidebar(
                            ui,
                            &self.pronunciation_hints,
                            &self.npc_manager,
                            self.world.player_location,
                            &palette,
                        );
                    }
                });
        }

        // Central chat panel (fills remaining space)
        let loading_display = self.loading_animation.as_ref().map(|anim| {
            let (r, g, b) = anim.current_color_rgb();
            chat_panel::LoadingDisplay {
                text: anim.display_text(),
                color: egui::Color32::from_rgb(r, g, b),
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            chat_panel::draw_chat_panel(
                ui,
                &self.world.text_log,
                &palette,
                loading_display.as_ref(),
            );
        });

        // Process submitted input
        if let Some(text) = submitted {
            self.process_input(text);
        }

        // Screenshot mode: request capture or handle result
        if let Some(config) = &mut self.screenshot_config {
            screenshot::handle_screenshot_frame(
                ctx,
                config,
                &mut self.world,
                &mut self.should_quit,
            );
        }
    }
}

/// Launches the GUI mode.
///
/// Initializes game state, inference pipeline, and opens the egui window.
/// This function takes ownership of the tokio runtime handle to bridge
/// async inference calls with the synchronous egui event loop.
/// Runs the game in GUI mode with egui/eframe.
///
/// Uses dual-client routing: cloud client for dialogue, local client for
/// intent parsing and background simulation. Falls back to local if no
/// cloud provider is configured.
pub fn run_gui(
    clients: InferenceClients,
    config: &ProviderConfig,
    cloud_config: Option<&CloudConfig>,
    improv: bool,
) -> Result<()> {
    let rt_handle = tokio::runtime::Handle::current();

    // Initialize dialogue inference pipeline (cloud if configured, else local)
    let (dial_client, dial_model) = clients.dialogue_client();
    let dialogue_model = dial_model.to_string();
    let (tx, rx) = mpsc::channel(32);
    let _worker = {
        let _guard = rt_handle.enter();
        inference::spawn_inference_worker(dial_client.clone(), rx)
    };
    let queue = InferenceQueue::new(tx);

    // Initialize app
    let mut app = GuiApp::new(rt_handle);

    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match WorldState::from_parish_file(parish_path, LocationId(15)) {
            Ok(world) => app.world = world,
            Err(e) => tracing::warn!("Failed to load parish data: {}", e),
        }
    }

    app.inference_queue = Some(queue);
    app.client = Some(clients.local.clone());
    app.model_name = clients.local_model.clone();
    app.dialogue_model = dialogue_model;
    app.provider_name = format!("{:?}", config.provider).to_lowercase();
    app.base_url = config.base_url.clone();
    app.api_key = config.api_key.clone();
    app.improv_enabled = improv;

    // Set cloud fields if configured
    if let Some(cc) = cloud_config {
        app.cloud_provider_name = Some(format!("{:?}", cc.provider).to_lowercase());
        app.cloud_model_name = Some(cc.model.clone());
        app.cloud_client = clients.cloud.clone();
        app.cloud_api_key = cc.api_key.clone();
        app.cloud_base_url = Some(cc.base_url.clone());
    }

    // Load NPCs
    let npcs_path = Path::new("data/npcs.json");
    if npcs_path.exists() {
        match NpcManager::load_from_file(npcs_path) {
            Ok(mgr) => app.npc_manager = mgr,
            Err(e) => tracing::warn!("Failed to load NPC data: {}", e),
        }
    }

    // Initial tier assignment and location description
    app.npc_manager
        .assign_tiers(app.world.player_location, &app.world.graph);
    app.show_location_arrival();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Parish — An Irish Living World Text Adventure")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0]),
        ..Default::default()
    };

    eframe::run_native("Parish", options, Box::new(|_cc| Ok(Box::new(app))))
        .map_err(|e| anyhow::anyhow!("eframe error: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gui_app_new() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let app = GuiApp::new(rt.handle().clone());
        assert!(app.input_buffer.is_empty());
        assert!(app.show_map);
        assert!(app.show_sidebar);
        assert!(!app.show_debug);
        assert!(!app.should_quit);
        assert!(app.inference_queue.is_none());
        assert!(app.client.is_none());
        assert_eq!(app.provider_name, "ollama");
        assert!(app.pronunciation_hints.is_empty());
        assert_eq!(app.idle_counter, 0);
        assert_eq!(app.request_id, 0);
    }

    #[test]
    fn test_gui_app_debug_event() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        app.debug_event("test event".to_string());
        assert_eq!(app.debug_log.len(), 1);
        assert_eq!(app.debug_log[0], "test event");
    }

    #[test]
    fn test_gui_app_debug_log_capacity() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        for i in 0..DEBUG_LOG_CAPACITY + 10 {
            app.debug_event(format!("event {}", i));
        }
        assert_eq!(app.debug_log.len(), DEBUG_LOG_CAPACITY);
    }

    #[test]
    fn test_drain_streaming_buffer_inactive() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        app.world.log("initial".to_string());
        // streaming_active is false, so nothing should happen
        app.drain_streaming_buffer();
        assert_eq!(app.world.text_log.len(), 1);
    }

    #[test]
    fn test_drain_streaming_buffer_active() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        app.world.log("NPC: ".to_string());
        *app.streaming_active.lock().unwrap() = true;
        app.streaming_buf.lock().unwrap().push_str("hello there");
        app.drain_streaming_buffer();
        assert_eq!(app.world.text_log.last().unwrap(), "NPC: hello there");
    }

    #[test]
    fn test_drain_streaming_buffer_drains_pending_hints() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        assert!(app.pronunciation_hints.is_empty());

        // Simulate hints arriving from the token accumulator task
        app.pending_hints.lock().unwrap().push(IrishWordHint {
            word: "Dia dhuit".to_string(),
            pronunciation: "DEE-ah gwit".to_string(),
            meaning: Some("Hello".to_string()),
        });

        app.drain_streaming_buffer();
        assert_eq!(app.pronunciation_hints.len(), 1);
        assert_eq!(app.pronunciation_hints[0].word, "Dia dhuit");
        // Pending should be drained
        assert!(app.pending_hints.lock().unwrap().is_empty());
    }

    #[test]
    fn test_handle_system_command_quit() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        app.handle_system_command(Command::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_system_command_pause_resume() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        assert!(!app.world.clock.is_paused());
        app.handle_system_command(Command::Pause);
        assert!(app.world.clock.is_paused());
        app.handle_system_command(Command::Resume);
        assert!(!app.world.clock.is_paused());
    }

    #[test]
    fn test_handle_system_command_toggle_improv() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        assert!(!app.improv_enabled);
        app.handle_system_command(Command::ToggleImprov);
        assert!(app.improv_enabled);
        app.handle_system_command(Command::ToggleImprov);
        assert!(!app.improv_enabled);
    }

    #[test]
    fn test_handle_system_command_toggle_sidebar() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        assert!(app.show_sidebar);
        app.handle_system_command(Command::ToggleSidebar);
        assert!(!app.show_sidebar);
    }

    #[test]
    fn test_idle_messages_not_empty() {
        assert!(!IDLE_MESSAGES.is_empty());
    }

    #[test]
    fn test_process_game_input_look() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        let initial_len = app.world.text_log.len();
        app.process_game_input("look".to_string());
        assert!(app.world.text_log.len() > initial_len);
    }

    #[test]
    fn test_process_game_input_no_npc() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut app = GuiApp::new(rt.handle().clone());
        let initial_len = app.world.text_log.len();
        app.process_game_input("hello there".to_string());
        // Should get an idle message since no NPCs
        assert!(app.world.text_log.len() > initial_len);
        assert_eq!(app.idle_counter, 1);
    }
}
