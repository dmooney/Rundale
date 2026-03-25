use anyhow::Result;
use clap::Parser;
use parish::config::{
    CliCategoryOverrides, CliCloudOverrides, CliOverrides, InferenceCategory, Provider,
    ProviderConfig, resolve_category_configs, resolve_cloud_config, resolve_config,
};
use parish::headless;
use parish::inference::openai_client::OpenAiClient;
use parish::inference::setup::{self, StdoutProgress};
use parish::inference::{self, InferenceClients, InferenceQueue};
use parish::input::{Command, InputResult, classify_input, parse_intent};
use parish::loading::LoadingAnimation;
use parish::npc::manager::NpcManager;
use parish::npc::ticks;
use parish::npc::{
    SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary, parse_npc_stream_response,
};
use parish::tui::{self, App};
use parish::world::description::{format_exits, render_description};
use parish::world::movement::{self, MovementResult};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

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

    /// Cloud LLM provider for player dialogue: openrouter (default), custom
    #[arg(long, env = "PARISH_CLOUD_PROVIDER")]
    cloud_provider: Option<String>,

    /// Cloud LLM model name (required when cloud provider is set)
    #[arg(long, env = "PARISH_CLOUD_MODEL")]
    cloud_model: Option<String>,

    /// Cloud LLM API base URL override
    #[arg(long, env = "PARISH_CLOUD_BASE_URL")]
    cloud_base_url: Option<String>,

    /// Cloud LLM API key
    #[arg(long, env = "PARISH_CLOUD_API_KEY")]
    cloud_api_key: Option<String>,

    // --- Per-category provider overrides ---
    /// Dialogue LLM provider override
    #[arg(long, env = "PARISH_DIALOGUE_PROVIDER")]
    dialogue_provider: Option<String>,
    /// Dialogue LLM model override
    #[arg(long, env = "PARISH_DIALOGUE_MODEL")]
    dialogue_model: Option<String>,
    /// Dialogue LLM base URL override
    #[arg(long, env = "PARISH_DIALOGUE_BASE_URL")]
    dialogue_base_url: Option<String>,
    /// Dialogue LLM API key override
    #[arg(long, env = "PARISH_DIALOGUE_API_KEY")]
    dialogue_api_key: Option<String>,

    /// Simulation LLM provider override
    #[arg(long, env = "PARISH_SIMULATION_PROVIDER")]
    simulation_provider: Option<String>,
    /// Simulation LLM model override
    #[arg(long, env = "PARISH_SIMULATION_MODEL")]
    simulation_model: Option<String>,
    /// Simulation LLM base URL override
    #[arg(long, env = "PARISH_SIMULATION_BASE_URL")]
    simulation_base_url: Option<String>,
    /// Simulation LLM API key override
    #[arg(long, env = "PARISH_SIMULATION_API_KEY")]
    simulation_api_key: Option<String>,

    /// Intent parsing LLM provider override
    #[arg(long, env = "PARISH_INTENT_PROVIDER")]
    intent_provider: Option<String>,
    /// Intent parsing LLM model override
    #[arg(long, env = "PARISH_INTENT_MODEL")]
    intent_model: Option<String>,
    /// Intent parsing LLM base URL override
    #[arg(long, env = "PARISH_INTENT_BASE_URL")]
    intent_base_url: Option<String>,
    /// Intent parsing LLM API key override
    #[arg(long, env = "PARISH_INTENT_API_KEY")]
    intent_api_key: Option<String>,

    /// Run in TUI mode (terminal interface) instead of default GUI
    #[arg(long)]
    tui: bool,

    /// Capture GUI screenshots to the given directory (default: docs/screenshots)
    #[arg(long, value_name = "DIR")]
    screenshot: Option<Option<String>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (before anything reads env vars)
    dotenvy::dotenv().ok();

    // Set up logging: file appender (always) + stderr (for non-TUI debugging)
    std::fs::create_dir_all("logs").ok();
    let file_appender = tracing_appender::rolling::daily("logs", "parish.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("parish=info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    tracing::info!("Starting Parish...");

    let cli = Cli::parse();

    // Script mode — no LLM needed, synchronous execution
    if let Some(script_path) = &cli.script {
        return parish::testing::run_script_mode(Path::new(script_path));
    }

    // Screenshot mode — capture GUI screenshots, no LLM needed
    if let Some(dir_opt) = &cli.screenshot {
        let dir = dir_opt.as_deref().unwrap_or("docs/screenshots");
        return parish::gui::screenshot::run_screenshots(Path::new(dir));
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

    // Resolve cloud provider configuration (legacy, for backward compat)
    let cloud_overrides = CliCloudOverrides {
        provider: cli.cloud_provider.clone(),
        base_url: cli.cloud_base_url.clone(),
        api_key: cli.cloud_api_key.clone(),
        model: cli.cloud_model.clone(),
    };
    let cloud_config = resolve_cloud_config(config_path, &cloud_overrides)?;

    // Build per-category CLI overrides
    let cli_category_overrides = build_cli_category_overrides(&cli);

    // Resolve per-category provider configs
    let category_configs = resolve_category_configs(
        config_path,
        &provider_config,
        &cli_category_overrides,
        &cloud_overrides,
    )?;

    // Set up local inference client based on provider
    let (client, model, mut ollama_process) = setup_provider(&cli, &provider_config).await?;

    // Build per-category client routing struct
    let clients = build_inference_clients(&client, &model, &category_configs);

    for (cat, cfg) in &category_configs {
        tracing::info!(
            "{:?} category: {:?} provider at {} with model {}",
            cat,
            cfg.provider,
            cfg.base_url,
            cfg.model.as_deref().unwrap_or("(auto)")
        );
    }

    if cli.headless {
        let result = headless::run_headless(
            clients.clone(),
            &provider_config,
            cloud_config.as_ref(),
            &category_configs,
            cli.improv,
        )
        .await;
        ollama_process.stop();
        return result;
    }

    // TUI mode (opt-in with --tui)
    if cli.tui {
        // Initialize dialogue inference pipeline (uses cloud if configured, else local)
        let (dial_client, dial_model) = clients.dialogue_client();
        let (tx, rx) = mpsc::channel(32);
        let _worker = inference::spawn_inference_worker(dial_client.clone(), rx);
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
        app.client = Some(clients.base.clone());
        app.model_name = clients.base_model.clone();
        app.dialogue_model = dial_model.to_string();
        app.provider_name = format!("{:?}", provider_config.provider).to_lowercase();
        app.base_url = provider_config.base_url.clone();
        app.api_key = provider_config.api_key.clone();
        app.improv_enabled = cli.improv;

        // Set cloud fields if configured (legacy compat + new per-category)
        if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Dialogue) {
            app.cloud_provider_name = Some(format!("{:?}", cat_cfg.provider).to_lowercase());
            app.cloud_model_name = cat_cfg.model.clone();
            let (dial_cl, _) = clients.dialogue_client();
            app.cloud_client = Some(dial_cl.clone());
            app.cloud_api_key = cat_cfg.api_key.clone();
            app.cloud_base_url = Some(cat_cfg.base_url.clone());
        } else if let Some(ref cc) = cloud_config {
            app.cloud_provider_name = Some(format!("{:?}", cc.provider).to_lowercase());
            app.cloud_model_name = Some(cc.model.clone());
            let (dial_cl, _) = clients.dialogue_client();
            app.cloud_client = Some(dial_cl.clone());
            app.cloud_api_key = cc.api_key.clone();
            app.cloud_base_url = Some(cc.base_url.clone());
        }

        // Set intent client/model (may differ from base)
        let (intent_cl, intent_mdl) = clients.intent_client();
        app.intent_client = Some(intent_cl.clone());
        app.intent_model = intent_mdl.to_string();

        // Set simulation client/model (may differ from base)
        let (sim_cl, sim_mdl) = clients.simulation_client();
        app.simulation_client = Some(sim_cl.clone());
        app.simulation_model = sim_mdl.to_string();

        // Initialize per-category provider metadata from config
        if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Intent) {
            app.intent_provider_name = Some(format!("{:?}", cat_cfg.provider).to_lowercase());
            app.intent_api_key = cat_cfg.api_key.clone();
            app.intent_base_url = Some(cat_cfg.base_url.clone());
        }
        if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Simulation) {
            app.simulation_provider_name = Some(format!("{:?}", cat_cfg.provider).to_lowercase());
            app.simulation_api_key = cat_cfg.api_key.clone();
            app.simulation_base_url = Some(cat_cfg.base_url.clone());
        }

        // Load NPCs from data file
        let npcs_path = Path::new("data/npcs.json");
        if npcs_path.exists() {
            match NpcManager::load_from_file(npcs_path) {
                Ok(mgr) => app.npc_manager = mgr,
                Err(e) => tracing::warn!("Failed to load NPC data: {}", e),
            }
        }

        // Initial tier assignment
        app.npc_manager
            .assign_tiers(app.world.player_location, &app.world.graph);

        // Initialize persistence
        let db_path = std::path::Path::new("parish_saves.db");
        if let Ok(db) = parish::persistence::Database::open(db_path) {
            let async_db = Arc::new(parish::persistence::AsyncDatabase::new(db));
            if let Ok(Some(branch)) = async_db.find_branch("main").await {
                app.active_branch_id = branch.id;
                if let Ok(Some((snap_id, snapshot))) =
                    async_db.load_latest_snapshot(branch.id).await
                {
                    let events = async_db
                        .events_since_snapshot(branch.id, snap_id)
                        .await
                        .unwrap_or_default();
                    snapshot.restore(&mut app.world, &mut app.npc_manager);
                    parish::persistence::replay_journal(
                        &mut app.world,
                        &mut app.npc_manager,
                        &events,
                    );
                    app.latest_snapshot_id = snap_id;
                    app.npc_manager
                        .assign_tiers(app.world.player_location, &app.world.graph);
                } else {
                    let snapshot =
                        parish::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    if let Ok(snap_id) = async_db.save_snapshot(branch.id, &snapshot).await {
                        app.latest_snapshot_id = snap_id;
                    }
                }
            }
            app.db = Some(async_db);
            app.last_autosave = Some(std::time::Instant::now());
        }

        // Show initial location description
        show_location_arrival(&mut app);

        // Initialize terminal
        let mut terminal = tui::init_terminal()?;
        let mut request_id: u64 = 0;

        // Shared streaming state for the TUI render loop
        let streaming_buf: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
        let streaming_active: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));

        // Idle simulation: tick NPC schedules if no input for 20 seconds
        let mut last_interaction = std::time::Instant::now();
        let idle_tick_interval = Duration::from_secs(20);
        let mut last_idle_tick = std::time::Instant::now();

        // Main game loop
        loop {
            // Check if streaming tokens have arrived and update the log
            {
                let active = *streaming_active.lock().unwrap();
                if active {
                    let mut buf = streaming_buf.lock().unwrap();
                    if !buf.is_empty() {
                        // Tokens arrived — clear the loading animation
                        app.loading_animation = None;
                        // Update the last log line (the streaming line) with new tokens
                        if let Some(last) = app.world.text_log.last_mut() {
                            last.push_str(&buf);
                        }
                        buf.clear();
                    }
                }
            }

            // Tick loading animation if active
            if let Some(anim) = &mut app.loading_animation {
                anim.tick();
            }

            // Idle simulation tick: advance world when player is idle
            {
                let is_streaming = *streaming_active.lock().unwrap();
                let idle_elapsed = last_interaction.elapsed() >= idle_tick_interval;
                let tick_due = last_idle_tick.elapsed() >= idle_tick_interval;

                if !is_streaming && idle_elapsed && tick_due && !app.world.clock.is_paused() {
                    app.npc_manager
                        .assign_tiers(app.world.player_location, &app.world.graph);
                    let events = app
                        .npc_manager
                        .tick_schedules(&app.world.clock, &app.world.graph);
                    process_schedule_events(&mut app, &events);
                    last_idle_tick = std::time::Instant::now();
                }
            }

            // Periodic autosave
            if let Some(ref db) = app.db {
                let should_autosave = app
                    .last_autosave
                    .map(|t| t.elapsed().as_secs() >= 45)
                    .unwrap_or(true);
                if should_autosave {
                    let snapshot =
                        parish::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    if let Ok(snap_id) = db.save_snapshot(app.active_branch_id, &snapshot).await {
                        let _ = db
                            .clear_journal(app.active_branch_id, app.latest_snapshot_id)
                            .await;
                        app.latest_snapshot_id = snap_id;
                        app.last_autosave = Some(std::time::Instant::now());
                        app.debug_event("Autosave complete".to_string());
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
                last_interaction = std::time::Instant::now();
                app.world.log(format!("> {}", raw_input));

                match classify_input(&raw_input) {
                    InputResult::SystemCommand(cmd) => {
                        if handle_system_command(&mut app, cmd).await {
                            // Rebuild dialogue inference pipeline
                            // Use cloud client if available, otherwise local
                            let dial_client =
                                app.cloud_client.clone().or_else(|| app.client.clone());
                            if let Some(new_client) = dial_client {
                                let (tx, rx) = mpsc::channel(32);
                                let _new_worker = inference::spawn_inference_worker(new_client, rx);
                                app.inference_queue = Some(InferenceQueue::new(tx));
                            }
                        }
                    }
                    InputResult::GameInput(text) => {
                        // Parse intent (uses intent client, may be per-category override or base)
                        let intent_client = app.intent_client.clone().unwrap();
                        let intent_model = app.intent_model.clone();
                        let intent = parse_intent(&intent_client, &text, &intent_model).await?;

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
                                let npcs_here = app.npc_manager.npcs_at(app.world.player_location);
                                let npc = npcs_here.first().cloned().cloned();

                                if let Some(npc) = npc {
                                    let other_npcs: Vec<_> = app
                                        .npc_manager
                                        .npcs_at(app.world.player_location)
                                        .into_iter()
                                        .filter(|n| n.id != npc.id)
                                        .collect();
                                    let system_prompt = ticks::build_enhanced_system_prompt(
                                        &npc,
                                        app.improv_enabled,
                                    );
                                    let context = ticks::build_enhanced_context(
                                        &npc,
                                        &app.world,
                                        &text,
                                        &other_npcs,
                                    );

                                    if let Some(queue) = &app.inference_queue {
                                        request_id += 1;

                                        let (token_tx, mut token_rx) =
                                            mpsc::unbounded_channel::<String>();

                                        app.world.log(format!("{}: ", npc.name));
                                        *streaming_active.lock().unwrap() = true;
                                        app.loading_animation = Some(LoadingAnimation::new());

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
                                                        buf_clone
                                                            .lock()
                                                            .unwrap()
                                                            .push_str(&new_text);
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
                                                    let new_text = accumulated
                                                        [displayed_len..safe_end]
                                                        .to_string();
                                                    buf_clone.lock().unwrap().push_str(&new_text);
                                                    displayed_len = safe_end;
                                                }
                                            }

                                            if !separator_found && displayed_len < accumulated.len()
                                            {
                                                let remaining =
                                                    accumulated[displayed_len..].to_string();
                                                buf_clone.lock().unwrap().push_str(&remaining);
                                            }

                                            *active_clone.lock().unwrap() = false;
                                        });

                                        match queue
                                            .send(
                                                request_id,
                                                app.dialogue_model.clone(),
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
                                                            // Tokens arrived — clear loading animation
                                                            app.loading_animation = None;
                                                            if let Some(last) =
                                                                app.world.text_log.last_mut()
                                                            {
                                                                last.push_str(&buf);
                                                            }
                                                            buf.clear();
                                                        }
                                                    }

                                                    // Tick loading animation if still waiting
                                                    if let Some(anim) = &mut app.loading_animation {
                                                        anim.tick();
                                                    }

                                                    terminal
                                                        .draw(|frame| tui::draw(frame, &mut app))?;

                                                    match rx.try_recv() {
                                                        Ok(resp) => break Some(resp),
                                                        Err(
                                                            oneshot::error::TryRecvError::Empty,
                                                        ) => {
                                                            tokio::time::sleep(
                                                                Duration::from_millis(50),
                                                            )
                                                            .await;
                                                            continue;
                                                        }
                                                        Err(
                                                            oneshot::error::TryRecvError::Closed,
                                                        ) => {
                                                            break None;
                                                        }
                                                    }
                                                };

                                                let _ = stream_handle.await;
                                                app.loading_animation = None;

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
                                                            let parsed = parse_npc_stream_response(
                                                                &resp.text,
                                                            );
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

                // --- Simulation tick after each player action ---
                app.npc_manager
                    .assign_tiers(app.world.player_location, &app.world.graph);
                let schedule_events = app
                    .npc_manager
                    .tick_schedules(&app.world.clock, &app.world.graph);
                process_schedule_events(&mut app, &schedule_events);
            }
        }

        // Restore terminal
        tui::restore_terminal(&mut terminal)?;

        // Stop Ollama if we started it
        ollama_process.stop();

        tracing::info!("Parish exited cleanly.");

        return Ok(());
    }

    // GUI mode (default)
    let result = parish::gui::run_gui(
        clients.clone(),
        &provider_config,
        cloud_config.as_ref(),
        cli.improv,
    );
    ollama_process.stop();
    result
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

/// Builds the per-category inference routing struct from base and category configs.
fn build_inference_clients(
    base_client: &OpenAiClient,
    base_model: &str,
    category_configs: &std::collections::HashMap<InferenceCategory, parish::config::CategoryConfig>,
) -> InferenceClients {
    let mut overrides = std::collections::HashMap::new();
    for (category, cfg) in category_configs {
        let client = OpenAiClient::new(&cfg.base_url, cfg.api_key.as_deref());
        let model = cfg.model.clone().unwrap_or_else(|| base_model.to_string());
        overrides.insert(*category, (client, model));
    }
    InferenceClients::new(base_client.clone(), base_model.to_string(), overrides)
}

/// Builds per-category CLI overrides from the parsed CLI arguments.
fn build_cli_category_overrides(cli: &Cli) -> CliCategoryOverrides {
    let mut categories = std::collections::HashMap::new();

    let dialogue = CliOverrides {
        provider: cli.dialogue_provider.clone(),
        base_url: cli.dialogue_base_url.clone(),
        api_key: cli.dialogue_api_key.clone(),
        model: cli.dialogue_model.clone(),
    };
    if dialogue.provider.is_some()
        || dialogue.base_url.is_some()
        || dialogue.api_key.is_some()
        || dialogue.model.is_some()
    {
        categories.insert("dialogue".to_string(), dialogue);
    }

    let simulation = CliOverrides {
        provider: cli.simulation_provider.clone(),
        base_url: cli.simulation_base_url.clone(),
        api_key: cli.simulation_api_key.clone(),
        model: cli.simulation_model.clone(),
    };
    if simulation.provider.is_some()
        || simulation.base_url.is_some()
        || simulation.api_key.is_some()
        || simulation.model.is_some()
    {
        categories.insert("simulation".to_string(), simulation);
    }

    let intent = CliOverrides {
        provider: cli.intent_provider.clone(),
        base_url: cli.intent_base_url.clone(),
        api_key: cli.intent_api_key.clone(),
        model: cli.intent_model.clone(),
    };
    if intent.provider.is_some()
        || intent.base_url.is_some()
        || intent.api_key.is_some()
        || intent.model.is_some()
    {
        categories.insert("intent".to_string(), intent);
    }

    CliCategoryOverrides { categories }
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
///
/// Returns `true` if the LLM provider config changed and the inference
/// pipeline needs to be rebuilt.
async fn handle_system_command(app: &mut App, cmd: Command) -> bool {
    let mut rebuild_inference = false;
    match cmd {
        Command::Quit => {
            if let Some(ref db) = app.db {
                let snapshot =
                    parish::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                if let Ok(_snap_id) = db.save_snapshot(app.active_branch_id, &snapshot).await {
                    app.world.log("Saved.".to_string());
                }
            }
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
        Command::ShowSpeed => {
            let speed_name = app
                .world
                .clock
                .current_speed()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Custom ({}x)", app.world.clock.speed_factor()));
            app.world
                .log(format!("The parish moves at {} pace.", speed_name));
        }
        Command::SetSpeed(speed) => {
            app.world.clock.set_speed(speed);
            app.world.log(speed.activation_message().to_string());
        }
        Command::InvalidSpeed(name) => {
            app.world.log(format!(
                "Unknown speed '{}'. Try: slow, normal, fast, fastest, ludicrous.",
                name
            ));
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
            app.world.log(
                "  /speed    — Show or change game speed (slow/normal/fast/fastest/ludicrous)"
                    .to_string(),
            );
            app.world.log("  /status   — Where am I?".to_string());
            app.world
                .log("  /irish    — Toggle the Irish words sidebar (or press Tab)".to_string());
            app.world
                .log("  /improv   — Toggle improv craft for NPC dialogue".to_string());
            app.world
                .log("  /provider — Show or change LLM provider".to_string());
            app.world
                .log("  /model    — Show or change model name".to_string());
            app.world
                .log("  /key      — Show or change API key".to_string());
            app.world
                .log("  /cloud    — Show or change cloud dialogue provider".to_string());
            app.world
                .log("  /debug    — Debug commands (try /debug help)".to_string());
            app.world.log("  /help     — Show this help".to_string());
            app.world.log("  /save     — Save game".to_string());
            app.world
                .log("  /fork <n> — Fork a new timeline branch".to_string());
            app.world
                .log("  /load <n> — Load a saved branch".to_string());
            app.world
                .log("  /branches — List save branches".to_string());
            app.world
                .log("  /log      — Show snapshot history".to_string());
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
        Command::ShowProvider => {
            app.world.log(format!("Base: {}", app.provider_name));
            for cat in InferenceCategory::ALL {
                if let Some(provider) = app.category_provider_name(cat) {
                    app.world.log(format!("  {}: {}", cat.name(), provider));
                }
            }
        }
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                app.base_url = provider.default_base_url().to_string();
                app.provider_name = format!("{:?}", provider).to_lowercase();
                app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
                rebuild_inference = true;
                app.world
                    .log(format!("Provider changed to {}.", app.provider_name));
            }
            Err(e) => {
                app.world.log(format!("{}", e));
            }
        },
        Command::ShowModel => {
            if app.model_name.is_empty() {
                app.world.log("Base model: (auto-detect)".to_string());
            } else {
                app.world.log(format!("Base model: {}", app.model_name));
            }
            for cat in InferenceCategory::ALL {
                let model = app.category_model(cat);
                if !model.is_empty() {
                    app.world.log(format!("  {}: {}", cat.name(), model));
                }
            }
        }
        Command::SetModel(name) => {
            app.model_name = name.clone();
            app.world.log(format!("Model changed to {}.", name));
        }
        Command::ShowKey => match &app.api_key {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                app.world.log(format!("API key: {}", masked));
            }
            Some(_) => {
                app.world
                    .log("API key: (set, too short to mask)".to_string());
            }
            None => {
                app.world.log("API key: (not set)".to_string());
            }
        },
        Command::SetKey(value) => {
            app.api_key = Some(value);
            app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
            rebuild_inference = true;
            app.world.log("API key updated.".to_string());
        }
        Command::Save => {
            if let Some(ref db) = app.db {
                let snapshot =
                    parish::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                match db.save_snapshot(app.active_branch_id, &snapshot).await {
                    Ok(snap_id) => {
                        let _ = db
                            .clear_journal(app.active_branch_id, app.latest_snapshot_id)
                            .await;
                        app.latest_snapshot_id = snap_id;
                        app.last_autosave = Some(std::time::Instant::now());
                        app.world.log("Game saved.".to_string());
                    }
                    Err(e) => app.world.log(format!("Failed to save: {}", e)),
                }
            } else {
                app.world.log("Persistence not available.".to_string());
            }
        }
        Command::Fork(name) => {
            if let Some(ref db) = app.db {
                let snapshot =
                    parish::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                match db.create_branch(&name, Some(app.active_branch_id)).await {
                    Ok(new_branch_id) => match db.save_snapshot(new_branch_id, &snapshot).await {
                        Ok(snap_id) => {
                            app.active_branch_id = new_branch_id;
                            app.latest_snapshot_id = snap_id;
                            app.last_autosave = Some(std::time::Instant::now());
                            app.world.log(format!("Forked to branch '{}'.", name));
                        }
                        Err(e) => app
                            .world
                            .log(format!("Failed to save fork snapshot: {}", e)),
                    },
                    Err(e) => app
                        .world
                        .log(format!("Failed to create branch '{}': {}", name, e)),
                }
            } else {
                app.world.log("Persistence not available.".to_string());
            }
        }
        Command::Load(name) => {
            if let Some(ref db) = app.db {
                match db.find_branch(&name).await {
                    Ok(Some(branch)) => {
                        if branch.id != app.active_branch_id {
                            let snapshot = parish::persistence::GameSnapshot::capture(
                                &app.world,
                                &app.npc_manager,
                            );
                            let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                        }
                        match db.load_latest_snapshot(branch.id).await {
                            Ok(Some((snap_id, loaded_snapshot))) => {
                                let events = db
                                    .events_since_snapshot(branch.id, snap_id)
                                    .await
                                    .unwrap_or_default();
                                loaded_snapshot.restore(&mut app.world, &mut app.npc_manager);
                                parish::persistence::replay_journal(
                                    &mut app.world,
                                    &mut app.npc_manager,
                                    &events,
                                );
                                app.active_branch_id = branch.id;
                                app.latest_snapshot_id = snap_id;
                                app.last_autosave = Some(std::time::Instant::now());
                                app.npc_manager
                                    .assign_tiers(app.world.player_location, &app.world.graph);
                                let time = app.world.clock.time_of_day();
                                let season = app.world.clock.season();
                                let loc = app.world.current_location().name.clone();
                                app.world.log(format!(
                                    "Loaded branch '{}'. {} — {}, {}.",
                                    name, loc, season, time
                                ));
                            }
                            Ok(None) => {
                                app.world
                                    .log(format!("Branch '{}' has no saves yet.", name));
                            }
                            Err(e) => app
                                .world
                                .log(format!("Failed to load branch '{}': {}", name, e)),
                        }
                    }
                    Ok(None) => {
                        app.world.log(format!("No branch named '{}' found.", name));
                    }
                    Err(e) => app
                        .world
                        .log(format!("Failed to find branch '{}': {}", name, e)),
                }
            } else {
                app.world.log("Persistence not available.".to_string());
            }
        }
        Command::Branches => {
            if let Some(ref db) = app.db {
                match db.list_branches().await {
                    Ok(branches) => {
                        app.world.log("Save branches:".to_string());
                        for b in &branches {
                            let marker = if b.id == app.active_branch_id {
                                " *"
                            } else {
                                ""
                            };
                            app.world.log(format!(
                                "  {}{} (created {})",
                                b.name,
                                marker,
                                parish::persistence::format_timestamp(&b.created_at)
                            ));
                        }
                    }
                    Err(e) => app.world.log(format!("Failed to list branches: {}", e)),
                }
            } else {
                app.world.log("Persistence not available.".to_string());
            }
        }
        Command::Log => {
            if let Some(ref db) = app.db {
                match db.branch_log(app.active_branch_id).await {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            app.world
                                .log("No snapshots on this branch yet.".to_string());
                        } else {
                            app.world
                                .log("Snapshot history (most recent first):".to_string());
                            for s in &snapshots {
                                app.world.log(format!(
                                    "  #{} — game: {} | saved: {}",
                                    s.id,
                                    s.game_time,
                                    parish::persistence::format_timestamp(&s.real_time)
                                ));
                            }
                        }
                    }
                    Err(e) => app.world.log(format!("Failed to read log: {}", e)),
                }
            } else {
                app.world.log("Persistence not available.".to_string());
            }
        }
        Command::Debug(sub) => {
            // Handle panel toggle specially
            if sub.as_deref() == Some("panel") {
                app.debug_sidebar_visible = !app.debug_sidebar_visible;
                let state = if app.debug_sidebar_visible {
                    "visible"
                } else {
                    "hidden"
                };
                app.world.log(format!("Debug panel {}.", state));
            } else {
                let lines = parish::debug::handle_debug(sub.as_deref(), app);
                for line in lines {
                    app.world.log(line);
                }
            }
        }
        Command::ShowCloud => {
            if let Some(ref provider) = app.cloud_provider_name {
                let model = app.cloud_model_name.as_deref().unwrap_or("(none)");
                app.world
                    .log(format!("Cloud: {} | Model: {}", provider, model));
            } else {
                app.world.log(
                    "No cloud provider configured. Use --cloud-provider or parish.toml [cloud]."
                        .to_string(),
                );
            }
        }
        Command::SetCloudProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let base_url = provider.default_base_url().to_string();
                app.cloud_provider_name = Some(format!("{:?}", provider).to_lowercase());
                app.cloud_base_url = Some(base_url.clone());
                app.cloud_client = Some(OpenAiClient::new(&base_url, app.cloud_api_key.as_deref()));
                rebuild_inference = true;
                app.world.log(format!(
                    "Cloud provider changed to {}.",
                    app.cloud_provider_name.as_deref().unwrap()
                ));
            }
            Err(e) => {
                app.world.log(format!("{}", e));
            }
        },
        Command::ShowCloudModel => {
            if let Some(ref model) = app.cloud_model_name {
                app.world.log(format!("Cloud model: {}", model));
            } else {
                app.world.log("Cloud model: (not set)".to_string());
            }
        }
        Command::SetCloudModel(name) => {
            app.cloud_model_name = Some(name.clone());
            app.dialogue_model = name.clone();
            app.world.log(format!("Cloud model changed to {}.", name));
        }
        Command::ShowCloudKey => match &app.cloud_api_key {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                app.world.log(format!("Cloud API key: {}", masked));
            }
            Some(_) => {
                app.world
                    .log("Cloud API key: (set, too short to mask)".to_string());
            }
            None => {
                app.world.log("Cloud API key: (not set)".to_string());
            }
        },
        Command::SetCloudKey(value) => {
            app.cloud_api_key = Some(value);
            let base_url = app
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api");
            app.cloud_client = Some(OpenAiClient::new(base_url, app.cloud_api_key.as_deref()));
            rebuild_inference = true;
            app.world.log("Cloud API key updated.".to_string());
        }
        Command::ShowCategoryProvider(cat) => {
            if let Some(provider) = app.category_provider_name(cat) {
                app.world
                    .log(format!("{} provider: {}", cat.name(), provider));
            } else {
                app.world.log(format!(
                    "{} provider: (inherits base: {})",
                    cat.name(),
                    app.provider_name
                ));
            }
        }
        Command::SetCategoryProvider(cat, name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let base_url = provider.default_base_url().to_string();
                let provider_name = format!("{:?}", provider).to_lowercase();
                let api_key = app.category_api_key(cat).map(|s| s.to_string());
                app.set_category_provider_name(cat, provider_name.clone());
                app.set_category_base_url(cat, base_url.clone());
                app.set_category_client(cat, OpenAiClient::new(&base_url, api_key.as_deref()));
                rebuild_inference = true;
                app.world.log(format!(
                    "{} provider changed to {}.",
                    cat.name(),
                    provider_name
                ));
            }
            Err(e) => {
                app.world.log(format!("{}", e));
            }
        },
        Command::ShowCategoryModel(cat) => {
            let model = app.category_model(cat);
            if model.is_empty() {
                app.world.log(format!(
                    "{} model: (inherits base: {})",
                    cat.name(),
                    app.model_name
                ));
            } else {
                app.world.log(format!("{} model: {}", cat.name(), model));
            }
        }
        Command::SetCategoryModel(cat, name) => {
            let cat_name = cat.name();
            app.set_category_model(cat, name.clone());
            app.world
                .log(format!("{} model changed to {}.", cat_name, name));
        }
        Command::ShowCategoryKey(cat) => match app.category_api_key(cat) {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                app.world.log(format!("{} API key: {}", cat.name(), masked));
            }
            Some(_) => {
                app.world
                    .log(format!("{} API key: (set, too short to mask)", cat.name()));
            }
            None => {
                app.world.log(format!("{} API key: (not set)", cat.name()));
            }
        },
        Command::SetCategoryKey(cat, value) => {
            let cat_name = cat.name();
            app.set_category_api_key(cat, value);
            let base_url = app
                .category_base_url(cat)
                .unwrap_or(&app.base_url)
                .to_string();
            let api_key = app.category_api_key(cat).map(|s| s.to_string());
            app.set_category_client(cat, OpenAiClient::new(&base_url, api_key.as_deref()));
            rebuild_inference = true;
            app.world.log(format!("{} API key updated.", cat_name));
        }
    }
    app.world.log(String::new());
    rebuild_inference
}

/// Shows the current location with description, NPCs, and exits.
fn show_location_arrival(app: &mut App) {
    let loc_name = app.world.current_location().name.clone();
    app.world.log(format!("— {} —", loc_name));

    // Render dynamic description if graph is loaded, else use static description
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let weather = app.world.weather.to_string();
        let npc_names: Vec<&str> = app
            .npc_manager
            .npcs_at(app.world.player_location)
            .iter()
            .map(|n| n.name.as_str())
            .collect();
        let desc = render_description(loc_data, tod, &weather, &npc_names);
        app.world.log(desc);
    } else {
        let desc = app.world.current_location().description.clone();
        app.world.log(desc);
    }

    // Show NPCs present
    for npc in app.npc_manager.npcs_at(app.world.player_location) {
        app.world.log(format!("{} is here.", npc.name));
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
        let weather = app.world.weather.to_string();
        let npc_names: Vec<&str> = app
            .npc_manager
            .npcs_at(app.world.player_location)
            .iter()
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

/// Processes schedule events: logs to debug panel and shows player-visible
/// messages for arrivals/departures at the player's current location.
fn process_schedule_events(app: &mut App, events: &[parish::npc::manager::ScheduleEvent]) {
    use parish::npc::manager::ScheduleEventKind;

    let player_loc = app.world.player_location;

    for event in events {
        // Always feed to debug log
        app.debug_event(event.debug_string());

        // Show player-visible messages for events at their location
        match &event.kind {
            ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                app.world
                    .log(format!("{} heads off down the road.", event.npc_name));
            }
            ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                app.world.log(format!("{} arrives.", event.npc_name));
            }
            _ => {}
        }
    }
}
