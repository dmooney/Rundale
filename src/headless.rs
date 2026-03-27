//! Headless CLI mode — the default interactive mode.
//!
//! Provides a simple stdin/stdout REPL with full game logic
//! (NPC inference, intent parsing, system commands).
//! Runs by default or with `--headless` on the command line.

use crate::app::App;
use crate::config::{CategoryConfig, CloudConfig, InferenceCategory, Provider, ProviderConfig};
use crate::inference::openai_client::OpenAiClient;
use crate::inference::{self, InferenceClients, InferenceQueue};
use crate::input::{Command, InputResult, classify_input, parse_intent};
use crate::loading::LoadingAnimation;
use crate::npc::anachronism;
use crate::npc::manager::NpcManager;
use crate::npc::ticks;
use crate::npc::{
    SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary, parse_npc_stream_response,
};
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use anyhow::Result;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{Notify, mpsc};

/// Interval between autosaves in seconds.
const AUTOSAVE_INTERVAL_SECS: u64 = 45;

/// Runs the game in headless mode with a plain stdin/stdout REPL.
///
/// Sets up the inference pipeline with dual-client routing: cloud client
/// for dialogue, local client for intent parsing. Falls back to local
/// for everything if no cloud provider is configured.
pub async fn run_headless(
    clients: InferenceClients,
    provider_config: &ProviderConfig,
    cloud_config: Option<&CloudConfig>,
    category_configs: &HashMap<InferenceCategory, CategoryConfig>,
    improv: bool,
) -> Result<()> {
    println!("=== Parish — Headless Mode ===");
    println!(
        "Base: {} ({})",
        clients.base_model,
        provider_config.provider_display()
    );
    if clients.has_custom_dialogue() {
        let (_, dial_model) = clients.dialogue_client();
        println!("Dialogue: {} (override)", dial_model);
    }
    println!("Type /help for commands, /quit to exit.");
    println!();

    // Initialize dialogue inference pipeline (cloud if configured, else local)
    let (dial_client, dial_model) = clients.dialogue_client();
    let dialogue_model = dial_model.to_string();
    let (tx, rx) = mpsc::channel(32);
    let _worker = inference::spawn_inference_worker(dial_client.clone(), rx);
    let queue = InferenceQueue::new(tx);

    // Initialize app state — load parish data if available
    let mut app = App::new();
    let parish_path = Path::new("data/parish.json");
    if parish_path.exists() {
        match crate::world::WorldState::from_parish_file(parish_path, crate::world::LocationId(15))
        {
            Ok(world) => app.world = world,
            Err(e) => eprintln!("Warning: Failed to load parish data: {}", e),
        }
    }
    app.inference_queue = Some(queue);
    app.client = Some(clients.base.clone());
    app.model_name = clients.base_model.clone();
    app.dialogue_model = dialogue_model;
    app.provider_name = format!("{:?}", provider_config.provider).to_lowercase();
    app.base_url = provider_config.base_url.clone();
    app.api_key = provider_config.api_key.clone();
    app.improv_enabled = improv;

    // Set intent client/model
    let (intent_cl, intent_mdl) = clients.intent_client();
    app.intent_client = Some(intent_cl.clone());
    app.intent_model = intent_mdl.to_string();

    // Set simulation client/model
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

    // Set cloud/dialogue fields if configured
    if clients.has_custom_dialogue() {
        let (dial_cl, dial_mdl) = clients.dialogue_client();
        app.cloud_client = Some(dial_cl.clone());
        app.cloud_model_name = Some(dial_mdl.to_string());
    } else if let Some(cc) = cloud_config {
        app.cloud_provider_name = Some(format!("{:?}", cc.provider).to_lowercase());
        app.cloud_model_name = Some(cc.model.clone());
        let (dial_cl, _) = clients.dialogue_client();
        app.cloud_client = Some(dial_cl.clone());
        app.cloud_api_key = cc.api_key.clone();
        app.cloud_base_url = Some(cc.base_url.clone());
    }

    // Load NPCs from data file
    let npcs_path = Path::new("data/npcs.json");
    if npcs_path.exists() {
        match NpcManager::load_from_file(npcs_path) {
            Ok(mgr) => app.npc_manager = mgr,
            Err(e) => eprintln!("Warning: Failed to load NPC data: {}", e),
        }
    }

    // Initial tier assignment
    app.npc_manager
        .assign_tiers(app.world.player_location, &app.world.graph);

    // Initialize persistence
    let db_path = std::path::Path::new("parish_saves.db");
    match crate::persistence::Database::open(db_path) {
        Ok(db) => {
            let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));

            // Find the main branch
            if let Ok(Some(branch)) = async_db.find_branch("main").await {
                app.active_branch_id = branch.id;

                // Try to load latest snapshot
                if let Ok(Some((snap_id, snapshot))) =
                    async_db.load_latest_snapshot(branch.id).await
                {
                    // Replay journal events
                    let events = async_db
                        .events_since_snapshot(branch.id, snap_id)
                        .await
                        .unwrap_or_default();
                    snapshot.restore(&mut app.world, &mut app.npc_manager);
                    crate::persistence::replay_journal(
                        &mut app.world,
                        &mut app.npc_manager,
                        &events,
                    );
                    app.latest_snapshot_id = snap_id;
                    app.npc_manager
                        .assign_tiers(app.world.player_location, &app.world.graph);
                    println!("Restored from save.");
                } else {
                    // First run — save initial snapshot
                    let snapshot =
                        crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    if let Ok(snap_id) = async_db.save_snapshot(branch.id, &snapshot).await {
                        app.latest_snapshot_id = snap_id;
                    }
                }
            }

            app.db = Some(async_db);
            app.last_autosave = Some(std::time::Instant::now());
        }
        Err(e) => {
            eprintln!("Warning: Persistence unavailable: {}", e);
        }
    }

    // Show initial location
    print_location_arrival(&app);

    let mut request_id: u64 = 0;
    let stdin = std::io::stdin();
    let reader = stdin.lock();

    for line in reader.lines() {
        let raw_input = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = raw_input.trim().to_string();
        if trimmed.is_empty() {
            print!("> ");
            std::io::stdout().flush().ok();
            continue;
        }

        match classify_input(&trimmed) {
            InputResult::SystemCommand(cmd) => {
                let (quit, rebuild) = handle_headless_command(&mut app, cmd).await;
                if rebuild {
                    // Rebuild dialogue queue: prefer cloud client, fall back to local
                    let dial_client = app.cloud_client.clone().or_else(|| app.client.clone());
                    if let Some(new_client) = dial_client {
                        let (tx, rx) = mpsc::channel(32);
                        let _new_worker = inference::spawn_inference_worker(new_client, rx);
                        app.inference_queue = Some(InferenceQueue::new(tx));
                    }
                }
                if quit {
                    break;
                }
            }
            InputResult::GameInput(text) => {
                let intent_client = match app.intent_client.clone() {
                    Some(client) => client,
                    None => {
                        eprintln!("Intent client unavailable; cannot process input");
                        continue;
                    }
                };
                let intent_model = app.intent_model.clone();
                handle_headless_game_input(
                    &mut app,
                    &intent_client,
                    &intent_model,
                    &text,
                    &mut request_id,
                )
                .await?;
            }
        }

        // Simulation tick after each player action
        app.npc_manager
            .assign_tiers(app.world.player_location, &app.world.graph);
        let schedule_events = app
            .npc_manager
            .tick_schedules(&app.world.clock, &app.world.graph);
        process_headless_schedule_events(&mut app, &schedule_events);

        // Periodic autosave
        if let Some(ref db) = app.db {
            let should_autosave = app
                .last_autosave
                .map(|t| t.elapsed().as_secs() >= AUTOSAVE_INTERVAL_SECS)
                .unwrap_or(true);
            if should_autosave {
                let snapshot =
                    crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                if let Ok(snap_id) = db.save_snapshot(app.active_branch_id, &snapshot).await {
                    let _ = db
                        .clear_journal(app.active_branch_id, app.latest_snapshot_id)
                        .await;
                    app.latest_snapshot_id = snap_id;
                    app.last_autosave = Some(std::time::Instant::now());
                    tracing::debug!("Autosave complete");
                }
            }
        }

        if app.should_quit {
            break;
        }

        print!("> ");
        std::io::stdout().flush().ok();
    }

    println!("Safe home to ye. May the road rise to meet you.");
    Ok(())
}

/// Capitalizes the first character of a string slice.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Handles a system command in headless mode. Returns true if the game should exit.
/// Atmospheric idle messages shown when no NPC is present for conversation.
const HEADLESS_IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

/// Headless idle message counter.
static HEADLESS_IDLE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Handles a system command in headless mode.
///
/// Returns `(should_quit, rebuild_inference)`.
async fn handle_headless_command(app: &mut App, cmd: Command) -> (bool, bool) {
    let mut rebuild = false;
    match cmd {
        Command::Quit => {
            // Autosave before quitting
            if let Some(ref db) = app.db {
                let snapshot =
                    crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                match db.save_snapshot(app.active_branch_id, &snapshot).await {
                    Ok(snap_id) => {
                        app.latest_snapshot_id = snap_id;
                        println!("Saved and farewell.");
                    }
                    Err(e) => eprintln!("Warning: Failed to save on quit: {}", e),
                }
            }
            app.should_quit = true;
            return (true, false);
        }
        Command::Pause => {
            app.world.clock.pause();
            println!("The clocks of the parish stand still.");
        }
        Command::Resume => {
            app.world.clock.resume();
            println!("Time stirs again in the parish.");
        }
        Command::ShowSpeed => {
            let speed_name = app
                .world
                .clock
                .current_speed()
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Custom ({}x)", app.world.clock.speed_factor()));
            println!("The parish moves at {} pace.", speed_name);
        }
        Command::SetSpeed(speed) => {
            app.world.clock.set_speed(speed);
            println!("{}", speed.activation_message());
        }
        Command::InvalidSpeed(name) => {
            println!(
                "Unknown speed '{}'. Try: slow, normal, fast, fastest, ludicrous.",
                name
            );
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
            println!("Location: {} | {} | {}{}", loc, time, season, paused);
        }
        Command::Help => {
            println!("A few things ye might say:");
            println!("  /quit     - Take your leave");
            println!("  /pause    - Hold time still");
            println!("  /resume   - Let time flow again");
            println!(
                "  /speed    - Show or change game speed (slow/normal/fast/fastest/ludicrous)"
            );
            println!("  /status   - Where am I?");
            println!("  /irish    - Toggle Irish words sidebar (TUI only)");
            println!("  /improv   - Toggle improv craft for NPC dialogue");
            println!("  /provider - Show or change base LLM provider");
            println!("  /model    - Show or change base model name");
            println!("  /key      - Show or change base API key");
            println!("  /cloud    - Show or change cloud dialogue provider (legacy)");
            println!(
                "  /model.<cat>    - Show or change model for a category (dialogue/simulation/intent)"
            );
            println!("  /provider.<cat> - Show or change provider for a category");
            println!("  /key.<cat>      - Show or change API key for a category");
            println!("  /help     - Show this help");
            println!("  /save     - Save game");
            println!("  /fork <n> - Fork a new timeline branch");
            println!("  /load <n> - Load a saved branch");
            println!("  /branches - List save branches");
            println!("  /log      - Show snapshot history");
        }
        Command::ToggleSidebar => {
            println!("The pronunciation sidebar is only available in TUI mode.");
        }
        Command::ToggleImprov => {
            app.improv_enabled = !app.improv_enabled;
            if app.improv_enabled {
                println!("The characters loosen up — improv craft engaged.");
            } else {
                println!("The characters settle back to their usual selves.");
            }
        }
        Command::ShowProvider => {
            println!("Base: {}", app.provider_name);
            for cat in InferenceCategory::ALL {
                if let Some(provider) = app.category_provider_name(cat) {
                    println!("  {}: {}", cat.name(), provider);
                }
            }
        }
        Command::SetProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                app.base_url = provider.default_base_url().to_string();
                app.provider_name = format!("{:?}", provider).to_lowercase();
                app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
                rebuild = true;
                println!("Provider changed to {}.", app.provider_name);
            }
            Err(e) => {
                println!("{}", e);
            }
        },
        Command::ShowModel => {
            if app.model_name.is_empty() {
                println!("Base model: (auto-detect)");
            } else {
                println!("Base model: {}", app.model_name);
            }
            for cat in InferenceCategory::ALL {
                let model = app.category_model(cat);
                if !model.is_empty() {
                    println!("  {}: {}", cat.name(), model);
                }
            }
        }
        Command::SetModel(name) => {
            app.model_name = name.clone();
            println!("Model changed to {}.", name);
        }
        Command::ShowKey => match &app.api_key {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                println!("API key: {}", masked);
            }
            Some(_) => {
                println!("API key: (set, too short to mask)");
            }
            None => {
                println!("API key: (not set)");
            }
        },
        Command::SetKey(value) => {
            app.api_key = Some(value);
            app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
            rebuild = true;
            println!("API key updated.");
        }
        Command::Save => {
            if let Some(ref db) = app.db {
                let snapshot =
                    crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                match db.save_snapshot(app.active_branch_id, &snapshot).await {
                    Ok(snap_id) => {
                        // Clear old journal and start fresh from this snapshot
                        let _ = db
                            .clear_journal(app.active_branch_id, app.latest_snapshot_id)
                            .await;
                        app.latest_snapshot_id = snap_id;
                        app.last_autosave = Some(std::time::Instant::now());
                        println!("Game saved.");
                    }
                    Err(e) => eprintln!("Failed to save: {}", e),
                }
            } else {
                println!("Persistence not available.");
            }
        }
        Command::Fork(name) => {
            if let Some(ref db) = app.db {
                // Save current branch first
                let snapshot =
                    crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;

                // Create new branch
                match db.create_branch(&name, Some(app.active_branch_id)).await {
                    Ok(new_branch_id) => {
                        // Save snapshot under new branch
                        match db.save_snapshot(new_branch_id, &snapshot).await {
                            Ok(snap_id) => {
                                app.active_branch_id = new_branch_id;
                                app.latest_snapshot_id = snap_id;
                                app.last_autosave = Some(std::time::Instant::now());
                                println!("Forked to branch '{}'.", name);
                            }
                            Err(e) => eprintln!("Failed to save fork snapshot: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Failed to create branch '{}': {}", name, e),
                }
            } else {
                println!("Persistence not available.");
            }
        }
        Command::Load(name) => {
            if let Some(ref db) = app.db {
                match db.find_branch(&name).await {
                    Ok(Some(branch)) => {
                        // Auto-save current branch only when switching branches
                        if branch.id != app.active_branch_id {
                            let snapshot = crate::persistence::GameSnapshot::capture(
                                &app.world,
                                &app.npc_manager,
                            );
                            let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                        }
                        match db.load_latest_snapshot(branch.id).await {
                            Ok(Some((snap_id, loaded_snapshot))) => {
                                // Replay journal events
                                let events = db
                                    .events_since_snapshot(branch.id, snap_id)
                                    .await
                                    .unwrap_or_default();
                                loaded_snapshot.restore(&mut app.world, &mut app.npc_manager);
                                crate::persistence::replay_journal(
                                    &mut app.world,
                                    &mut app.npc_manager,
                                    &events,
                                );

                                app.active_branch_id = branch.id;
                                app.latest_snapshot_id = snap_id;
                                app.last_autosave = Some(std::time::Instant::now());

                                // Reassign tiers after loading
                                app.npc_manager
                                    .assign_tiers(app.world.player_location, &app.world.graph);

                                let time = app.world.clock.time_of_day();
                                let season = app.world.clock.season();
                                let loc = app.world.current_location().name.clone();
                                println!(
                                    "Loaded branch '{}'. {} — {}, {}.",
                                    name, loc, season, time
                                );
                            }
                            Ok(None) => println!("Branch '{}' has no saves yet.", name),
                            Err(e) => eprintln!("Failed to load branch '{}': {}", name, e),
                        }
                    }
                    Ok(None) => println!("No branch named '{}' found.", name),
                    Err(e) => eprintln!("Failed to find branch '{}': {}", name, e),
                }
            } else {
                println!("Persistence not available.");
            }
        }
        Command::Branches => {
            if let Some(ref db) = app.db {
                match db.list_branches().await {
                    Ok(branches) => {
                        println!("Save branches:");
                        for b in &branches {
                            let marker = if b.id == app.active_branch_id {
                                " *"
                            } else {
                                ""
                            };
                            println!(
                                "  {}{} (created {})",
                                b.name,
                                marker,
                                crate::persistence::format_timestamp(&b.created_at)
                            );
                        }
                    }
                    Err(e) => eprintln!("Failed to list branches: {}", e),
                }
            } else {
                println!("Persistence not available.");
            }
        }
        Command::Log => {
            if let Some(ref db) = app.db {
                match db.branch_log(app.active_branch_id).await {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            println!("No snapshots on this branch yet.");
                        } else {
                            println!("Snapshot history (most recent first):");
                            for s in &snapshots {
                                println!(
                                    "  #{} — game: {} | saved: {}",
                                    s.id,
                                    s.game_time,
                                    crate::persistence::format_timestamp(&s.real_time)
                                );
                            }
                        }
                    }
                    Err(e) => eprintln!("Failed to get branch log: {}", e),
                }
            } else {
                println!("Persistence not available.");
            }
        }
        Command::Debug(sub) => {
            let lines = crate::debug::handle_debug(sub.as_deref(), app);
            for line in lines {
                println!("{}", line);
            }
        }
        Command::ShowCloud => {
            if let Some(ref provider) = app.cloud_provider_name {
                let model = app.cloud_model_name.as_deref().unwrap_or("(none)");
                println!("Cloud: {} | Model: {}", provider, model);
            } else {
                println!(
                    "No cloud provider configured. Use --cloud-provider or parish.toml [cloud]."
                );
            }
        }
        Command::SetCloudProvider(name) => match Provider::from_str_loose(&name) {
            Ok(provider) => {
                let base_url = provider.default_base_url().to_string();
                let provider_name = format!("{:?}", provider).to_lowercase();
                app.cloud_provider_name = Some(provider_name.clone());
                app.cloud_base_url = Some(base_url.clone());
                app.cloud_client = Some(OpenAiClient::new(&base_url, app.cloud_api_key.as_deref()));
                rebuild = true;
                println!("Cloud provider changed to {}.", provider_name);
            }
            Err(e) => {
                println!("{}", e);
            }
        },
        Command::ShowCloudModel => {
            if let Some(ref model) = app.cloud_model_name {
                println!("Cloud model: {}", model);
            } else {
                println!("Cloud model: (not set)");
            }
        }
        Command::SetCloudModel(name) => {
            app.cloud_model_name = Some(name.clone());
            app.dialogue_model = name.clone();
            println!("Cloud model changed to {}.", name);
        }
        Command::ShowCloudKey => match &app.cloud_api_key {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                println!("Cloud API key: {}", masked);
            }
            Some(_) => {
                println!("Cloud API key: (set, too short to mask)");
            }
            None => {
                println!("Cloud API key: (not set)");
            }
        },
        Command::SetCloudKey(value) => {
            app.cloud_api_key = Some(value);
            let base_url = app
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api");
            app.cloud_client = Some(OpenAiClient::new(base_url, app.cloud_api_key.as_deref()));
            rebuild = true;
            println!("Cloud API key updated.");
        }
        Command::ShowCategoryProvider(cat) => {
            let name = cat.name();
            if let Some(provider) = app.category_provider_name(cat) {
                println!("{} provider: {}", name, provider);
            } else {
                println!("{} provider: (inherits base: {})", name, app.provider_name);
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
                rebuild = true;
                println!("{} provider changed to {}.", cat.name(), provider_name);
            }
            Err(e) => {
                println!("{}", e);
            }
        },
        Command::ShowCategoryModel(cat) => {
            let model = app.category_model(cat);
            if model.is_empty() {
                println!("{} model: (inherits base: {})", cat.name(), app.model_name);
            } else {
                println!("{} model: {}", cat.name(), model);
            }
        }
        Command::SetCategoryModel(cat, name) => {
            let cat_name = cat.name();
            app.set_category_model(cat, name.clone());
            println!("{} model changed to {}.", cat_name, name);
        }
        Command::ShowCategoryKey(cat) => match app.category_api_key(cat) {
            Some(key) if key.len() > 8 => {
                let masked = format!("{}...{}", &key[..4], &key[key.len() - 4..]);
                println!("{} API key: {}", cat.name(), masked);
            }
            Some(_) => {
                println!("{} API key: (set, too short to mask)", cat.name());
            }
            None => {
                println!("{} API key: (not set)", cat.name());
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
            rebuild = true;
            println!("{} API key updated.", cat_name);
        }
    }
    (false, rebuild)
}

/// Handles game input (NPC interaction or intent parsing) in headless mode.
async fn handle_headless_game_input(
    app: &mut App,
    client: &OpenAiClient,
    model: &str,
    text: &str,
    request_id: &mut u64,
) -> Result<()> {
    // Always parse intent first so Move/Look work even with NPCs present
    let intent = parse_intent(client, text, model).await?;

    match intent.intent {
        crate::input::IntentKind::Move => {
            if let Some(target) = &intent.target {
                handle_headless_movement(app, target);
            } else {
                println!("And where would ye be off to?");
            }
        }
        crate::input::IntentKind::Look => {
            print_location_description(app);
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
                let system_prompt = ticks::build_enhanced_system_prompt(&npc, app.improv_enabled);
                let mut context =
                    ticks::build_enhanced_context(&npc, &app.world, text, &other_npcs);

                // Check for anachronisms in player input and inject alert
                let anachronisms = anachronism::check_input(text);
                if let Some(alert) = anachronism::format_context_alert(&anachronisms) {
                    context.push_str(&alert);
                }

                if let Some(queue) = &app.inference_queue {
                    *request_id += 1;

                    let (token_tx, mut token_rx) = mpsc::unbounded_channel::<String>();

                    let npc_display_name = app.npc_manager.display_name(&npc).to_string();
                    print!("{}: ", capitalize_first(&npc_display_name));
                    std::io::stdout().flush().ok();

                    // Spawn a loading animation that prints to stdout
                    // until the first token arrives. Uses Notify for
                    // deterministic cancellation instead of a timed sleep.
                    let cancel_notify = Arc::new(Notify::new());
                    let cancel_for_stream = Arc::clone(&cancel_notify);
                    let npc_name_for_anim = npc_display_name.clone();
                    let anim_handle = tokio::spawn(async move {
                        let mut anim = LoadingAnimation::new();
                        loop {
                            let ansi = anim.current_color_ansi();
                            let text = anim.display_text();
                            print!("\r{}: {}{}\x1b[0m\x1b[K", npc_name_for_anim, ansi, text);
                            std::io::stdout().flush().ok();
                            anim.tick();
                            tokio::select! {
                                () = cancel_notify.notified() => break,
                                () = tokio::time::sleep(std::time::Duration::from_millis(100)) => {}
                            }
                        }
                        // Clear the animation line and reprint NPC prefix
                        print!("\r\x1b[K{}: ", npc_name_for_anim);
                        std::io::stdout().flush().ok();
                    });

                    match queue
                        .send(
                            *request_id,
                            app.dialogue_model.clone(),
                            context,
                            Some(system_prompt),
                            Some(token_tx),
                        )
                        .await
                    {
                        Ok(rx) => {
                            let stream_handle = tokio::spawn(async move {
                                let mut accumulated = String::new();
                                let mut displayed_len: usize = 0;
                                let mut separator_found = false;
                                let mut anim_cancelled = false;

                                while let Some(token) = token_rx.recv().await {
                                    accumulated.push_str(&token);

                                    // Cancel loading animation on first displayable content
                                    if !anim_cancelled {
                                        cancel_for_stream.notify_one();
                                        anim_cancelled = true;
                                    }

                                    if separator_found {
                                        continue;
                                    }

                                    if let Some((dialogue_end, _meta_start)) =
                                        find_response_separator(&accumulated)
                                    {
                                        if dialogue_end > displayed_len {
                                            let new_text =
                                                &accumulated[displayed_len..dialogue_end];
                                            print!("{}", new_text);
                                            std::io::stdout().flush().ok();
                                        }
                                        separator_found = true;
                                        continue;
                                    }

                                    let raw_end =
                                        accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                                    let safe_end = floor_char_boundary(&accumulated, raw_end);
                                    if safe_end > displayed_len {
                                        let new_text = &accumulated[displayed_len..safe_end];
                                        print!("{}", new_text);
                                        std::io::stdout().flush().ok();
                                        displayed_len = safe_end;
                                    }
                                }

                                if !separator_found && displayed_len < accumulated.len() {
                                    let remaining = &accumulated[displayed_len..];
                                    print!("{}", remaining);
                                    std::io::stdout().flush().ok();
                                }

                                println!();
                                accumulated
                            });

                            match rx.await {
                                Ok(response) => {
                                    let _streamed = stream_handle.await.unwrap_or_default();
                                    let _ = anim_handle.await;

                                    if let Some(err) = &response.error {
                                        println!(
                                            "[The parish storyteller has lost the thread: {}]",
                                            err
                                        );
                                    } else {
                                        // Mark the NPC as introduced after their first response
                                        app.npc_manager.mark_introduced(npc.id);
                                        let parsed = parse_npc_stream_response(&response.text);
                                        if let Some(meta) = &parsed.metadata {
                                            tracing::debug!(
                                                "NPC metadata: action={}, mood={}",
                                                meta.action,
                                                meta.mood
                                            );
                                        }
                                    }
                                }
                                Err(_) => {
                                    let _ = stream_handle.await;
                                    let _ = anim_handle.await;
                                    println!("[The storyteller has wandered off mid-tale.]");
                                }
                            }
                        }
                        Err(e) => {
                            println!();
                            println!("[The storyteller couldn't hear ye: {}]", e);
                        }
                    }
                } else {
                    println!("[No storyteller could be found in the parish today.]");
                }
            } else {
                let idx = HEADLESS_IDLE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!(
                    "{}",
                    HEADLESS_IDLE_MESSAGES[idx % HEADLESS_IDLE_MESSAGES.len()]
                );
            }
        }
    }

    println!();
    Ok(())
}

/// Prints the current location with description, NPCs, and exits (headless).
fn print_location_arrival(app: &App) {
    let loc_name = app.world.current_location().name.clone();
    println!("--- {} ---", loc_name);

    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let npc_display: Vec<String> = app
            .npc_manager
            .npcs_at(app.world.player_location)
            .iter()
            .map(|n| app.npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        let desc = render_description(loc_data, tod, &app.world.weather.to_string(), &npc_names);
        println!("{}", desc);
    } else {
        println!("{}", app.world.current_location().description);
    }

    for npc in app.npc_manager.npcs_at(app.world.player_location) {
        let display = app.npc_manager.display_name(npc);
        println!("{} is here.", capitalize_first(display));
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    println!("{}", exits);
    println!();
}

/// Prints current location description and exits (headless /look).
fn print_location_description(app: &App) {
    if let Some(loc_data) = app.world.current_location_data() {
        let tod = app.world.clock.time_of_day();
        let npc_display: Vec<String> = app
            .npc_manager
            .npcs_at(app.world.player_location)
            .iter()
            .map(|n| app.npc_manager.display_name(n).to_string())
            .collect();
        let npc_names: Vec<&str> = npc_display.iter().map(|s| s.as_str()).collect();
        let desc = render_description(loc_data, tod, &app.world.weather.to_string(), &npc_names);
        println!("{}", desc);
    } else {
        println!("{}", app.world.current_location().description);
    }

    let exits = format_exits(app.world.player_location, &app.world.graph);
    println!("{}", exits);
}

/// Handles movement in headless mode.
fn handle_headless_movement(app: &mut App, target: &str) {
    let result = movement::resolve_movement(target, &app.world.graph, app.world.player_location);

    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            println!("{}", narration);
            println!();

            app.world.clock.advance(minutes as i64);
            app.world.player_location = destination;

            if let Some(data) = app.world.graph.get(destination) {
                app.world
                    .locations
                    .entry(destination)
                    .or_insert_with(|| crate::world::Location {
                        id: destination,
                        name: data.name.clone(),
                        description: data.description_template.clone(),
                        indoor: data.indoor,
                        public: data.public,
                        lat: data.lat,
                        lon: data.lon,
                    });
            }

            print_location_arrival(app);
        }
        MovementResult::AlreadyHere => {
            println!("Sure, you're already standing right here.");
        }
        MovementResult::NotFound(name) => {
            println!(
                "You haven't the faintest notion how to reach \"{}\". Try asking about.",
                name
            );
            let exits = format_exits(app.world.player_location, &app.world.graph);
            println!("{}", exits);
        }
    }
}

/// Processes schedule events in headless mode: debug log + player-visible println.
fn process_headless_schedule_events(app: &mut App, events: &[crate::npc::manager::ScheduleEvent]) {
    use crate::npc::manager::ScheduleEventKind;

    let player_loc = app.world.player_location;

    for event in events {
        app.debug_event(event.debug_string());

        // Look up the display name (brief description if not yet introduced)
        let display = app
            .npc_manager
            .get(event.npc_id)
            .map(|n| app.npc_manager.display_name(n).to_string())
            .unwrap_or_else(|| event.npc_name.clone());

        match &event.kind {
            ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                println!("{} heads off down the road.", capitalize_first(&display));
            }
            ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                println!("{} arrives.", capitalize_first(&display));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::world::time::GameSpeed;

    #[tokio::test]
    async fn test_handle_headless_command_quit() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Quit).await;
        assert!(quit);
        assert!(app.should_quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_pause() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Pause).await;
        assert!(!quit);
        assert!(app.world.clock.is_paused());
    }

    #[tokio::test]
    async fn test_handle_headless_command_resume() {
        let mut app = App::new();
        app.world.clock.pause();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Resume).await;
        assert!(!quit);
        assert!(!app.world.clock.is_paused());
    }

    #[tokio::test]
    async fn test_handle_headless_command_help() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Help).await;
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_status() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Status).await;
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_save_no_db() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Save).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_save_with_db() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();
        app.db = Some(async_db);
        app.active_branch_id = branch.id;
        app.latest_snapshot_id = snap_id;

        let (quit, rebuild) = handle_headless_command(&mut app, Command::Save).await;
        assert!(!quit);
        assert!(!rebuild);
        assert!(app.latest_snapshot_id > snap_id);
    }

    #[tokio::test]
    async fn test_handle_headless_command_fork_and_branches() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();
        app.db = Some(async_db.clone());
        app.active_branch_id = branch.id;
        app.latest_snapshot_id = snap_id;

        // Fork
        let (quit, _) = handle_headless_command(&mut app, Command::Fork("test".to_string())).await;
        assert!(!quit);
        assert_ne!(app.active_branch_id, branch.id);

        // Branches should show both
        let branches = async_db.list_branches().await.unwrap();
        assert_eq!(branches.len(), 2);
    }

    #[tokio::test]
    async fn test_handle_headless_command_load() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();
        app.db = Some(async_db);
        app.active_branch_id = branch.id;
        app.latest_snapshot_id = snap_id;

        // Load main
        let (quit, _) = handle_headless_command(&mut app, Command::Load("main".to_string())).await;
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_load_nonexistent() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        app.db = Some(async_db);
        let (quit, _) = handle_headless_command(&mut app, Command::Load("bogus".to_string())).await;
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_provider() {
        let mut app = App::new();
        app.provider_name = "openrouter".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowProvider).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_provider() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("openrouter".to_string())).await;
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.provider_name, "openrouter");
        assert!(app.client.is_some());
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_provider_invalid() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("bogus".to_string())).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_model() {
        let mut app = App::new();
        app.model_name = "test-model".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowModel).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_model() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetModel("new-model".to_string())).await;
        assert!(!quit);
        assert!(!rebuild);
        assert_eq!(app.model_name, "new-model");
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_key_none() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_key_masked() {
        let mut app = App::new();
        app.api_key = Some("sk-or-v1-abcdef1234".to_string());
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_key() {
        let mut app = App::new();
        app.base_url = "https://openrouter.ai/api".to_string();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetKey("sk-new-key-12345678".to_string()))
                .await;
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.api_key, Some("sk-new-key-12345678".to_string()));
        assert!(app.client.is_some());
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_speed() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowSpeed).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_speed() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetSpeed(GameSpeed::Fast)).await;
        assert!(!quit);
        assert!(!rebuild);
        assert!(
            (app.world.clock.speed_factor() - 72.0).abs() < f64::EPSILON,
            "Speed should be 72.0 after setting Fast"
        );
    }

    #[tokio::test]
    async fn test_handle_set_category_model_dialogue() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryModel(InferenceCategory::Dialogue, "gpt-4".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
        assert_eq!(app.cloud_model_name.as_deref(), Some("gpt-4"));
        assert_eq!(app.dialogue_model, "gpt-4");
    }

    #[tokio::test]
    async fn test_handle_set_category_model_intent() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryModel(InferenceCategory::Intent, "qwen3:1.5b".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
        assert_eq!(app.intent_model, "qwen3:1.5b");
    }

    #[tokio::test]
    async fn test_handle_set_category_model_simulation() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryModel(InferenceCategory::Simulation, "qwen3:8b".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
        assert_eq!(app.simulation_model, "qwen3:8b");
    }

    #[tokio::test]
    async fn test_handle_set_category_provider_rebuilds() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryProvider(InferenceCategory::Intent, "openrouter".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(
            rebuild,
            "Setting a category provider should trigger rebuild"
        );
        assert_eq!(app.intent_provider_name.as_deref(), Some("openrouter"));
    }

    #[tokio::test]
    async fn test_handle_set_category_key_rebuilds() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryKey(InferenceCategory::Dialogue, "sk-test-key".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(rebuild, "Setting a category key should trigger rebuild");
        assert_eq!(app.cloud_api_key.as_deref(), Some("sk-test-key"));
    }

    /// Verify SetCloudProvider sets cloud_provider_name without panicking (issue #80).
    #[tokio::test]
    async fn test_set_cloud_provider_sets_name_without_panic() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCloudProvider("openrouter".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.cloud_provider_name.as_deref(), Some("openrouter"));
    }

    /// Verify that intent_client being None is observable (guards against regression of issue #79).
    /// The App starts with intent_client unset; this confirms no panic on access.
    #[test]
    fn test_app_intent_client_starts_none() {
        let app = App::new();
        // intent_client is None until initialized by run_headless; accessing it must not panic.
        assert!(app.intent_client.is_none());
    }
}
