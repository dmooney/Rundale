use anyhow::Result;
use clap::Parser;
use parish::config::{CliOverrides, Provider, ProviderConfig, resolve_config};
use parish::headless;
use parish::inference::openai_client::OpenAiClient;
use parish::inference::setup::{self, StdoutProgress};
use parish::inference::{self, InferenceQueue};
use parish::input::{Command, InputResult, classify_input, parse_intent};
use parish::npc::{
    self, Npc, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
    parse_npc_stream_response,
};
use parish::tui::{self, App};
use parish::world::description::{format_exits, render_description};
use parish::world::movement::{self, MovementResult};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::EnvFilter;

/// Parish — An Irish Living World Text Adventure
#[derive(Parser, Debug)]
#[command(name = "parish", version, about)]
struct Cli {
    /// Run in headless mode (plain stdin/stdout REPL, no TUI)
    #[arg(long)]
    headless: bool,

    /// Run commands from a script file (one per line, JSON output, no LLM needed)
    #[arg(long, value_name = "FILE")]
    script: Option<String>,

    /// LLM provider: ollama (default), lmstudio, openrouter, custom
    #[arg(long, env = "PARISH_PROVIDER")]
    provider: Option<String>,

    /// Override the model name (required for non-Ollama providers)
    #[arg(long, env = "PARISH_MODEL")]
    model: Option<String>,

    /// Override the API base URL
    #[arg(long, env = "PARISH_BASE_URL")]
    base_url: Option<String>,

    /// API key for cloud providers (e.g. OpenRouter)
    #[arg(long, env = "PARISH_API_KEY")]
    api_key: Option<String>,

    /// Path to config file (default: parish.toml)
    #[arg(long)]
    config: Option<String>,

    /// Enable improv craft mode for NPC dialogue
    #[arg(long, env = "PARISH_IMPROV")]
    improv: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Starting Parish...");

    let cli = Cli::parse();

    // Script mode — no LLM needed, synchronous execution
    if let Some(script_path) = &cli.script {
        return parish::testing::run_script_mode(Path::new(script_path));
    }

    // Resolve provider configuration from file + env + CLI
    let config_path = cli.config.as_ref().map(|p| Path::new(p.as_str()));
    let overrides = CliOverrides {
        provider: cli.provider.clone(),
        base_url: cli.base_url.clone(),
        api_key: cli.api_key.clone(),
        model: cli.model.clone(),
    };
    let provider_config = resolve_config(config_path, &overrides)?;

    // Set up inference client based on provider
    let (client, model, mut ollama_process) = setup_provider(&cli, &provider_config).await?;

    if cli.headless {
        // Build OllamaSetup-compatible struct for headless mode
        let setup = setup::OllamaSetup {
            process: ollama_process,
            client,
            model_name: model,
            gpu_info: setup::GpuInfo {
                vendor: setup::GpuVendor::CpuOnly,
                vram_total_mb: 0,
                vram_free_mb: 0,
            },
        };
        return headless::run_headless(setup).await;
    }

    // TUI mode

    // Initialize inference pipeline
    let (tx, rx) = mpsc::channel(32);
    let _worker = inference::spawn_inference_worker(client.clone(), rx);
    let queue = InferenceQueue::new(tx);

    // Initialize app — load parish data if available
    let mut app = App::new();
    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match parish::world::WorldState::from_parish_file(
            parish_path,
            parish::world::LocationId(15),
        ) {
            Ok(world) => app.world = world,
            Err(e) => tracing::warn!("Failed to load parish data: {}", e),
        }
    }
    app.inference_queue = Some(queue);
    app.improv_enabled = cli.improv;
    app.npcs.push(Npc::new_test_npc());

    // Show initial location description
    show_location_arrival(&mut app);

    // Initialize terminal
    let mut terminal = tui::init_terminal()?;
    let mut request_id: u64 = 0;

    // Shared streaming state for the TUI render loop
    let streaming_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let streaming_active: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

    // Main game loop
    loop {
        // Check if streaming tokens have arrived and update the log
        {
            let active = *streaming_active.lock().unwrap();
            if active {
                let mut buf = streaming_buf.lock().unwrap();
                if !buf.is_empty() {
                    // Update the last log line (the streaming line) with new tokens
                    if let Some(last) = app.world.text_log.last_mut() {
                        last.push_str(&buf);
                    }
                    buf.clear();
                }
            }
        }

        // Draw frame
        terminal.draw(|frame| tui::draw(frame, &mut app))?;

        // Check for quit
        if app.should_quit {
            break;
        }

        // Handle input
        if let Some(raw_input) = tui::handle_input(&mut app, Duration::from_millis(100))? {
            app.world.log(format!("> {}", raw_input));

            match classify_input(&raw_input) {
                InputResult::SystemCommand(cmd) => {
                    handle_system_command(&mut app, cmd);
                }
                InputResult::GameInput(text) => {
                    // Always parse intent first so Move/Look work even with NPCs present
                    let intent = parse_intent(&client, &text, &model).await?;

                    match intent.intent {
                        parish::input::IntentKind::Move => {
                            if let Some(target) = &intent.target {
                                handle_movement(&mut app, target);
                            } else {
                                app.world.log("And where would ye be off to?".to_string());
                            }
                        }
                        parish::input::IntentKind::Look => {
                            show_location_description(&mut app);
                        }
                        _ => {
                            // Route to NPC conversation if one is present
                            let npc = app
                                .npcs
                                .iter()
                                .find(|n| n.location == app.world.player_location)
                                .cloned();

                            if let Some(npc) = npc {
                                let system_prompt =
                                    npc::build_tier1_system_prompt(&npc, app.improv_enabled);
                                let context = npc::build_tier1_context(&npc, &app.world, &text);

                                if let Some(queue) = &app.inference_queue {
                                    request_id += 1;

                                    let (token_tx, mut token_rx) =
                                        mpsc::unbounded_channel::<String>();

                                    app.world.log(format!("{}: ", npc.name));
                                    *streaming_active.lock().unwrap() = true;

                                    let buf_clone = Arc::clone(&streaming_buf);
                                    let active_clone = Arc::clone(&streaming_active);
                                    let stream_handle = tokio::spawn(async move {
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
                                                    let new_text = accumulated
                                                        [displayed_len..dialogue_end]
                                                        .to_string();
                                                    buf_clone.lock().unwrap().push_str(&new_text);
                                                }
                                                separator_found = true;
                                                continue;
                                            }

                                            let raw_end = accumulated
                                                .len()
                                                .saturating_sub(SEPARATOR_HOLDBACK);
                                            let safe_end =
                                                floor_char_boundary(&accumulated, raw_end);
                                            if safe_end > displayed_len {
                                                let new_text = accumulated[displayed_len..safe_end]
                                                    .to_string();
                                                buf_clone.lock().unwrap().push_str(&new_text);
                                                displayed_len = safe_end;
                                            }
                                        }

                                        if !separator_found && displayed_len < accumulated.len() {
                                            let remaining =
                                                accumulated[displayed_len..].to_string();
                                            buf_clone.lock().unwrap().push_str(&remaining);
                                        }

                                        *active_clone.lock().unwrap() = false;
                                    });

                                    match queue
                                        .send(
                                            request_id,
                                            model.clone(),
                                            context,
                                            Some(system_prompt),
                                            Some(token_tx),
                                        )
                                        .await
                                    {
                                        Ok(mut rx) => {
                                            let response = loop {
                                                {
                                                    let mut buf = streaming_buf.lock().unwrap();
                                                    if !buf.is_empty() {
                                                        if let Some(last) =
                                                            app.world.text_log.last_mut()
                                                        {
                                                            last.push_str(&buf);
                                                        }
                                                        buf.clear();
                                                    }
                                                }

                                                terminal
                                                    .draw(|frame| tui::draw(frame, &mut app))?;

                                                match rx.try_recv() {
                                                    Ok(resp) => break Some(resp),
                                                    Err(oneshot::error::TryRecvError::Empty) => {
                                                        tokio::time::sleep(Duration::from_millis(
                                                            50,
                                                        ))
                                                        .await;
                                                        continue;
                                                    }
                                                    Err(oneshot::error::TryRecvError::Closed) => {
                                                        break None;
                                                    }
                                                }
                                            };

                                            let _ = stream_handle.await;

                                            {
                                                let mut buf = streaming_buf.lock().unwrap();
                                                if !buf.is_empty() {
                                                    if let Some(last) =
                                                        app.world.text_log.last_mut()
                                                    {
                                                        last.push_str(&buf);
                                                    }
                                                    buf.clear();
                                                }
                                            }

                                            match response {
                                                Some(resp) => {
                                                    if let Some(err) = &resp.error {
                                                        app.world.log(format!(
                                            "[The parish storyteller has lost the thread: {}]",
                                            err
                                        ));
                                                    } else {
                                                        let parsed =
                                                            parse_npc_stream_response(&resp.text);
                                                        if let Some(meta) = &parsed.metadata {
                                                            tracing::debug!(
                                                                "NPC metadata: action={}, mood={}",
                                                                meta.action,
                                                                meta.mood
                                                            );
                                                            if !meta.irish_words.is_empty() {
                                                                // Prepend new hints, keep recent ones
                                                                app.pronunciation_hints.splice(
                                                                    0..0,
                                                                    meta.irish_words.clone(),
                                                                );
                                                                app.pronunciation_hints
                                                                    .truncate(20);
                                                            }
                                                        }
                                                    }
                                                }
                                                None => {
                                                    app.world.log(
                                                "[The storyteller has wandered off mid-tale.]"
                                                    .to_string(),
                                            );
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            *streaming_active.lock().unwrap() = false;
                                            let _ = stream_handle.await;
                                            app.world.log(format!(
                                                "[The storyteller couldn't hear ye: {}]",
                                                e
                                            ));
                                        }
                                    }
                                } else {
                                    app.world.log(
                                        "[No storyteller could be found in the parish today.]"
                                            .to_string(),
                                    );
                                }
                            } else {
                                let msg = IDLE_MESSAGES[app.idle_counter % IDLE_MESSAGES.len()];
                                app.world.log(msg.to_string());
                                app.idle_counter += 1;
                            }
                        }
                    }
                    app.world.log(String::new());
                }
            }
        }
    }

    // Restore terminal
    tui::restore_terminal(&mut terminal)?;

    // Stop Ollama if we started it
    ollama_process.stop();

    tracing::info!("Parish exited cleanly.");

    Ok(())
}

/// Sets up the inference client based on the resolved provider configuration.
///
/// For Ollama: runs the full setup sequence (GPU detect, auto-start, model pull, warmup).
/// For other providers: creates an OpenAI-compatible client directly.
async fn setup_provider(
    _cli: &Cli,
    config: &ProviderConfig,
) -> Result<(
    OpenAiClient,
    String,
    parish::inference::client::OllamaProcess,
)> {
    match config.provider {
        Provider::Ollama => {
            let progress = StdoutProgress;
            let setup =
                setup::setup_ollama(&config.base_url, config.model.as_deref(), &progress).await?;
            let model = setup.model_name.clone();
            let client = setup.client.clone();
            let process = setup.process;
            Ok((client, model, process))
        }
        _ => {
            // Non-Ollama providers: require model name
            let model = config.model.clone().ok_or_else(|| {
                anyhow::anyhow!(
                    "{:?} provider requires a model name. Set --model or PARISH_MODEL.",
                    config.provider
                )
            })?;
            let client = OpenAiClient::new(&config.base_url, config.api_key.as_deref());
            tracing::info!(
                "Using {:?} provider at {} with model {}",
                config.provider,
                config.base_url,
                model
            );

            // No OllamaProcess management for non-Ollama providers
            let dummy_process = parish::inference::client::OllamaProcess::none();
            Ok((client, model, dummy_process))
        }
    }
}

/// Atmospheric idle messages shown when no NPC is present for conversation.
const IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

/// Handles a system command.
fn handle_system_command(app: &mut App, cmd: Command) {
    match cmd {
        Command::Quit => {
            app.world
                .log("Safe home to ye. May the road rise to meet you.".to_string());
            app.should_quit = true;
        }
        Command::Pause => {
            app.world.clock.pause();
            app.world
                .log("The clocks of the parish stand still.".to_string());
        }
        Command::Resume => {
            app.world.clock.resume();
            app.world.log("Time stirs again in the parish.".to_string());
        }
        Command::Status => {
            let time = app.world.clock.time_of_day();
            let season = app.world.clock.season();
            let loc = app.world.current_location().name.clone();
            let paused = if app.world.clock.is_paused() {
                " (paused)"
            } else {
                ""
            };
            app.world.log(format!(
                "Location: {} | {} | {} {}",
                loc, time, season, paused
            ));
        }
        Command::Help => {
            app.world.log("A few things ye might say:".to_string());
            app.world.log("  /quit     — Take your leave".to_string());
            app.world.log("  /pause    — Hold time still".to_string());
            app.world
                .log("  /resume   — Let time flow again".to_string());
            app.world.log("  /status   — Where am I?".to_string());
            app.world
                .log("  /irish    — Toggle the Irish words sidebar (or press Tab)".to_string());
            app.world
                .log("  /improv   — Toggle improv craft for NPC dialogue".to_string());
            app.world.log("  /help     — Show this help".to_string());
            app.world
                .log("  /save     — Save game (not yet arrived)".to_string());
            app.world
                .log("  /fork <n> — Fork save (not yet arrived)".to_string());
            app.world
                .log("  /load <n> — Load save (not yet arrived)".to_string());
        }
        Command::ToggleSidebar => {
            app.sidebar_visible = !app.sidebar_visible;
            if app.sidebar_visible {
                app.world
                    .log("The pronunciation guide opens at your side.".to_string());
            } else {
                app.world
                    .log("The pronunciation guide folds away.".to_string());
            }
        }
        Command::ToggleImprov => {
            app.improv_enabled = !app.improv_enabled;
            if app.improv_enabled {
                app.world
                    .log("The characters loosen up — improv craft engaged.".to_string());
            } else {
                app.world
                    .log("The characters settle back to their usual selves.".to_string());
            }
        }
        Command::Save | Command::Fork(_) | Command::Load(_) | Command::Branches | Command::Log => {
            app.world.log(
                "That particular skill hasn't arrived in the parish yet. Patience now.".to_string(),
            );
        }
    }
    app.world.log(String::new());
}

/// Shows the current location with description, NPCs, and exits.
fn show_location_arrival(app: &mut App) {
    let loc_name = app.world.current_location().name.clone();
    app.world.log(format!("— {} —", loc_name));

    // Render dynamic description if graph is loaded, else use static description
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let weather = app.world.weather.clone();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &weather, &npc_names);
        app.world.log(desc);
    } else {
        let desc = app.world.current_location().description.clone();
        app.world.log(desc);
    }

    // Show NPCs present
    for npc in &app.npcs {
        if npc.location == app.world.player_location {
            app.world.log(format!("{} is here.", npc.name));
        }
    }

    // Show exits
    let exits = format_exits(app.world.player_location, &app.world.graph);
    app.world.log(exits);
    app.world.log(String::new());
}

/// Shows current location description and exits (for /look or IntentKind::Look).
fn show_location_description(app: &mut App) {
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let weather = app.world.weather.clone();
        let npc_names: Vec<&str> = app
            .npcs
            .iter()
            .filter(|n| n.location == app.world.player_location)
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &weather, &npc_names);
        app.world.log(desc);
    } else {
        let desc = app.world.current_location().description.clone();
        app.world.log(desc);
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    app.world.log(exits);
}

/// Handles a movement command: resolve, travel, advance clock, show arrival.
fn handle_movement(app: &mut App, target: &str) {
    let result = movement::resolve_movement(target, &app.world.graph, app.world.player_location);

    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            // Show travel narration
            app.world.log(narration);
            app.world.log(String::new());

            // Advance game clock
            app.world.clock.advance(minutes as i64);

            // Update player location
            app.world.player_location = destination;

            // Update legacy locations map with current position
            if let Some(data) = app.world.graph.get(destination) {
                app.world
                    .locations
                    .entry(destination)
                    .or_insert_with(|| parish::world::Location {
                        id: destination,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                    });
            }

            // Show new location
            show_location_arrival(app);
        }
        MovementResult::AlreadyHere => {
            app.world
                .log("Sure, you're already standing right here.".to_string());
        }
        MovementResult::NotFound(name) => {
            app.world.log(format!(
                "You haven't the faintest notion how to reach \"{}\". Try asking about.",
                name
            ));

            // Show available exits as a hint
            let exits = format_exits(app.world.player_location, &app.world.graph);
            app.world.log(exits);
        }
    }
}
