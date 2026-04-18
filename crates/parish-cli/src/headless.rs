//! Headless CLI mode — the default interactive mode.
//!
//! Provides a simple stdin/stdout REPL with full game logic
//! (NPC inference, intent parsing, system commands).
//! Runs by default or with `--headless` on the command line.

use crate::app::App;
use crate::config::{CategoryConfig, CloudConfig, InferenceCategory, ProviderConfig};
use crate::inference::openai_client::OpenAiClient;
use crate::inference::{self, AnyClient, InferenceClients, InferenceQueue};
use crate::input::{Command, InputResult, classify_input, extract_mention, parse_intent};
use crate::loading::LoadingAnimation;
use crate::npc::manager::NpcManager;
use crate::npc::parse_npc_stream_response;
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use anyhow::Result;
use parish_core::ipc::capitalize_first;
use parish_core::world::transport::TransportMode;
use std::collections::HashMap;
use std::io::{BufRead, Write};
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
    game_mod: Option<parish_core::game_mod::GameMod>,
    data_dir: Option<std::path::PathBuf>,
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
    let (interactive_tx, interactive_rx) = mpsc::channel(16);
    let (background_tx, background_rx) = mpsc::channel(32);
    let (batch_tx, batch_rx) = mpsc::channel(64);
    let inference_log = inference::new_inference_log();
    let worker_client = if provider_config.provider == parish_core::config::Provider::Simulator {
        AnyClient::simulator()
    } else {
        AnyClient::open_ai(dial_client.clone())
    };
    let _worker = inference::spawn_inference_worker(
        worker_client,
        interactive_rx,
        background_rx,
        batch_rx,
        inference_log.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);

    // Initialize app state — load world from active mod
    let mut app = App::new();
    if let Some(ref gm) = game_mod {
        match parish_core::game_mod::world_state_from_mod(gm) {
            Ok(world) => app.world = world,
            Err(e) => eprintln!("Warning: Failed to load world from mod: {}", e),
        }
    }
    app.game_mod = game_mod;
    app.inference_queue = Some(queue);
    app.client = Some(clients.base.clone());
    app.model_name = clients.base_model.clone();
    app.dialogue_model = dialogue_model;
    app.provider_name = format!("{:?}", provider_config.provider).to_lowercase();
    app.base_url = provider_config.base_url.clone();
    app.api_key = provider_config.api_key.clone();
    app.improv_enabled = improv;

    // Load feature flags from disk
    let flags_path = data_dir.map(|d| d.join("parish-flags.json"));
    if let Some(ref p) = flags_path {
        app.flags = crate::config::FeatureFlags::load_from_file(p);
    }
    app.flags_path = flags_path;

    // Set intent / simulation / reaction clients — skip for the simulator
    // provider since it has no real HTTP client and the dummy URL would cause
    // connection-timeout delays during intent parsing.
    let is_simulator = provider_config.provider == parish_core::config::Provider::Simulator;
    if !is_simulator {
        let (intent_cl, intent_mdl) = clients.intent_client();
        app.intent_client = Some(intent_cl.clone());
        app.intent_model = intent_mdl.to_string();

        let (sim_cl, sim_mdl) = clients.simulation_client();
        app.simulation_client = Some(sim_cl.clone());
        app.simulation_model = sim_mdl.to_string();

        let (react_cl, react_mdl) = clients.reaction_client();
        app.reaction_client = Some(react_cl.clone());
        app.reaction_model = react_mdl.to_string();
    }

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
    if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Reaction) {
        app.reaction_provider_name = Some(format!("{:?}", cat_cfg.provider).to_lowercase());
        app.reaction_api_key = cat_cfg.api_key.clone();
        app.reaction_base_url = Some(cat_cfg.base_url.clone());
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

    // Load NPCs from the active mod
    if let Some(ref gm) = app.game_mod {
        let npcs_path = gm.npcs_path();
        if npcs_path.exists() {
            match NpcManager::load_from_file(&npcs_path) {
                Ok(mgr) => app.npc_manager = mgr,
                Err(e) => eprintln!("Warning: Failed to load NPC data: {}", e),
            }
        }
    }

    // Initial tier assignment
    app.npc_manager.assign_tiers(&app.world, &[]);

    // Initialize persistence — Papers Please-style save picker
    let saves_dir = crate::persistence::picker::ensure_saves_dir();
    let db_path = crate::persistence::picker::run_picker(&saves_dir, &app.world.graph);
    app.save_file_path = Some(db_path.clone());

    // Acquire advisory lock so other instances know this save is in use.
    app.save_lock = crate::persistence::SaveFileLock::try_acquire(&db_path);

    match crate::persistence::Database::open(&db_path) {
        Ok(db) => {
            let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
            restore_from_db(&mut app, &async_db).await;
            app.db = Some(async_db);
            app.last_autosave = Some(std::time::Instant::now());
        }
        Err(e) => {
            eprintln!("Warning: Persistence unavailable: {}", e);
        }
    }

    // Show initial location
    print_location_arrival(&app);
    print_arrival_reactions(&mut app).await;

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
                    // Rebuild dialogue queue: simulator, cloud, or local client.
                    let any = if app.provider_name == "simulator" {
                        Some(AnyClient::simulator())
                    } else {
                        app.cloud_client
                            .clone()
                            .or_else(|| app.client.clone())
                            .map(AnyClient::open_ai)
                    };
                    if let Some(new_client) = any {
                        let (interactive_tx, interactive_rx) = mpsc::channel(16);
                        let (background_tx, background_rx) = mpsc::channel(32);
                        let (batch_tx, batch_rx) = mpsc::channel(64);
                        let _new_worker = inference::spawn_inference_worker(
                            new_client,
                            interactive_rx,
                            background_rx,
                            batch_rx,
                            inference_log.clone(),
                        );
                        app.inference_queue =
                            Some(InferenceQueue::new(interactive_tx, background_tx, batch_tx));
                    }
                }
                if quit {
                    break;
                }
            }
            InputResult::GameInput(text) => {
                let intent_client = app.intent_client.clone();
                let intent_model = app.intent_model.clone();
                handle_headless_game_input(
                    &mut app,
                    intent_client.as_ref(),
                    &intent_model,
                    &text,
                    &mut request_id,
                )
                .await?;
            }
        }

        // Simulation tick after each player action
        let tier_transitions = app.npc_manager.assign_tiers(&app.world, &[]);
        for tt in &tier_transitions {
            let direction = if tt.promoted { "promoted" } else { "demoted" };
            app.debug_event(format!(
                "[tier] {} {} {:?} → {:?}",
                tt.npc_name, direction, tt.old_tier, tt.new_tier,
            ));
        }
        // Tick weather engine
        {
            let season = app.world.clock.season();
            let now = app.world.clock.now();
            let mut rng = rand::thread_rng();
            if let Some(new_weather) = app.world.weather_engine.tick(now, season, &mut rng) {
                let old = app.world.weather;
                app.world.weather = new_weather;
                app.world
                    .event_bus
                    .publish(crate::world::events::GameEvent::WeatherChanged {
                        new_weather: new_weather.to_string(),
                        timestamp: app.world.clock.now(),
                    });
                tracing::info!(old = %old, new = %new_weather, "Weather changed");
            }
        }

        let schedule_events =
            app.npc_manager
                .tick_schedules(&app.world.clock, &app.world.graph, app.world.weather);
        process_headless_schedule_events(&mut app, &schedule_events);

        // Dispatch Tier 4 rules engine if enough game time has elapsed.
        // tick_tier4 is sub-ms CPU work; runs inline inside the lock scope.
        {
            let now = app.world.clock.now();
            if app.npc_manager.needs_tier4_tick(now) {
                let tier4_ids: std::collections::HashSet<crate::npc::NpcId> =
                    app.npc_manager.tier4_npcs().into_iter().collect();
                let events = {
                    let mut tier4_refs: Vec<&mut crate::npc::Npc> = app
                        .npc_manager
                        .npcs_mut()
                        .values_mut()
                        .filter(|n| tier4_ids.contains(&n.id))
                        .collect();
                    let season = app.world.clock.season();
                    let game_date = now.date_naive();
                    let mut rng = rand::thread_rng();
                    crate::npc::tier4::tick_tier4(&mut tier4_refs, season, game_date, &mut rng)
                };
                let game_events = app.npc_manager.apply_tier4_events(&events, now);
                for evt in game_events {
                    app.world.event_bus.publish(evt);
                }
                app.npc_manager.record_tier4_tick(now);
                app.debug_event(format!("[tier4] {} events", events.len()));
            }
        }

        // Dispatch Tier 3 batch LLM simulation for distant NPCs.
        // Runs inline (single-threaded async); the LLM await is acceptable here
        // because the user already expects potentially slow I/O after each input.
        {
            let now = app.world.clock.now();
            if app.npc_manager.needs_tier3_tick(now) && !app.npc_manager.tier3_in_flight() {
                let tier3_ids = app.npc_manager.tier3_npcs();
                let snapshots: Vec<parish_core::npc::ticks::Tier3Snapshot> = tier3_ids
                    .iter()
                    .filter_map(|id| app.npc_manager.get(*id))
                    .map(|npc| {
                        parish_core::npc::ticks::tier3_snapshot_from_npc(npc, &app.world.graph)
                    })
                    .collect();

                if !snapshots.is_empty()
                    && let Some(queue) = app.inference_queue.as_ref()
                {
                    let time_desc = app.world.clock.time_of_day().to_string();
                    let weather_str = app.world.weather.to_string();
                    let season_str = format!("{:?}", app.world.clock.season());
                    let hours = 24u32;
                    // NOTE: queue worker uses the dialogue client; per-category
                    // Simulation overrides are not honored for batch inference.
                    let sim_model = app.simulation_model.clone();

                    app.npc_manager.set_tier3_in_flight(true);

                    let ctx = parish_core::npc::ticks::Tier3Context {
                        snapshots: &snapshots,
                        queue,
                        model: &sim_model,
                        time_desc: &time_desc,
                        weather: &weather_str,
                        season: &season_str,
                        hours,
                        batch_size: 0,
                    };

                    match parish_core::npc::ticks::tick_tier3(&ctx).await {
                        Ok(updates) => {
                            let game_time = app.world.clock.now();
                            let _events = parish_core::npc::ticks::apply_tier3_updates(
                                &updates,
                                app.npc_manager.npcs_mut(),
                                &app.world.graph,
                                game_time,
                            );
                            app.npc_manager.record_tier3_tick(game_time);
                            app.debug_event(format!("[tier3] {} updates", updates.len()));
                        }
                        Err(e) => {
                            tracing::warn!("Tier 3 tick failed: {}", e);
                        }
                    }

                    app.npc_manager.set_tier3_in_flight(false);
                }
            }
        }

        // Dispatch Tier 2 background simulation for nearby NPCs.
        // Runs inline (single-threaded async); the LLM await is acceptable here
        // because the user already expects potentially slow I/O after each input.
        {
            let now = app.world.clock.now();
            if app.npc_manager.needs_tier2_tick(now)
                && !app.npc_manager.tier2_in_flight()
                && let Some(queue) = app.inference_queue.as_ref()
            {
                let groups_map = app.npc_manager.tier2_groups();
                if !groups_map.is_empty() {
                    use parish_core::npc::ticks::{Tier2Group, npc_snapshot_from_npc};

                    let groups: Vec<Tier2Group> = groups_map
                        .into_iter()
                        .filter_map(|(loc, npc_ids)| {
                            let location_name = app
                                .world
                                .graph
                                .get(loc)
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| format!("Location {}", loc.0));
                            let npcs: Vec<_> = npc_ids
                                .iter()
                                .filter_map(|id| app.npc_manager.get(*id))
                                .map(npc_snapshot_from_npc)
                                .collect();
                            if npcs.is_empty() {
                                return None;
                            }
                            Some(Tier2Group {
                                location: loc,
                                location_name,
                                npcs,
                            })
                        })
                        .collect();

                    if !groups.is_empty() {
                        // NOTE: queue worker uses the dialogue client; per-category
                        // Simulation overrides are not honored for batch inference.
                        let sim_model = app.simulation_model.clone();

                        app.npc_manager.set_tier2_in_flight(true);

                        let mut events = Vec::new();
                        for group in &groups {
                            if let Some(evt) = parish_core::npc::ticks::run_tier2_for_group(
                                queue,
                                &sim_model,
                                group,
                                &app.world.clock.time_of_day().to_string(),
                                &app.world.weather.to_string(),
                            )
                            .await
                            {
                                events.push(evt);
                            }
                        }

                        let game_time = app.world.clock.now();
                        for event in &events {
                            let _dbg = parish_core::npc::ticks::apply_tier2_event(
                                event,
                                app.npc_manager.npcs_mut(),
                                game_time,
                            );
                            // Push gossip so it can propagate to other NPCs.
                            parish_core::npc::ticks::create_gossip_from_tier2_event(
                                event,
                                &mut app.world.gossip_network,
                                game_time,
                            );
                        }
                        app.npc_manager.record_tier2_tick(game_time);
                        app.debug_event(format!(
                            "[tier2] {} events from {} groups",
                            events.len(),
                            groups.len()
                        ));

                        app.npc_manager.set_tier2_in_flight(false);
                    }
                }
            }
        }

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

/// Restores game state from a database, loading the "main" branch snapshot.
///
/// Finds the "main" branch, loads the latest snapshot, replays any journal
/// events since that snapshot, and reassigns NPC tiers. If no snapshot exists
/// (fresh database), captures and saves an initial snapshot.
async fn restore_from_db(app: &mut App, async_db: &Arc<crate::persistence::AsyncDatabase>) {
    if let Ok(Some(branch)) = async_db.find_branch("main").await {
        app.active_branch_id = branch.id;

        if let Ok(Some((snap_id, snapshot))) = async_db.load_latest_snapshot(branch.id).await {
            let events = async_db
                .events_since_snapshot(branch.id, snap_id)
                .await
                .unwrap_or_default();
            snapshot.restore(&mut app.world, &mut app.npc_manager);
            crate::persistence::replay_journal(&mut app.world, &mut app.npc_manager, &events);
            app.latest_snapshot_id = snap_id;
            app.npc_manager.assign_tiers(&app.world, &[]);
            println!("Restored from save.");
        } else {
            // First run — save initial snapshot
            let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
            if let Ok(snap_id) = async_db.save_snapshot(branch.id, &snapshot).await {
                app.latest_snapshot_id = snap_id;
            }
        }
    }
}

/// Headless idle message counter.
static HEADLESS_IDLE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Handles a system command in headless mode.
///
/// Returns `(should_quit, rebuild_inference)`.
async fn handle_headless_command(app: &mut App, cmd: Command) -> (bool, bool) {
    use parish_core::ipc::{CommandEffect, handle_command};

    // Snapshot config, run shared handler, apply changes back.
    let mut config = app.snapshot_config();
    let result = handle_command(cmd, &mut app.world, &mut app.npc_manager, &mut config);
    app.apply_config(&config);

    let mut rebuild = false;
    let mut should_quit = false;

    // Handle mode-specific side effects.
    for effect in &result.effects {
        match effect {
            CommandEffect::RebuildInference => {
                if app.provider_name != "simulator" {
                    if !(app.base_url.starts_with("http://")
                        || app.base_url.starts_with("https://"))
                    {
                        println!(
                            "[Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.]",
                            app.base_url
                        );
                    }
                    app.client = Some(OpenAiClient::new(&app.base_url, app.api_key.as_deref()));
                }
                rebuild = true;
            }
            CommandEffect::RebuildCloudClient => {
                let base_url = app
                    .cloud_base_url
                    .as_deref()
                    .unwrap_or("https://openrouter.ai/api");
                app.cloud_client = Some(OpenAiClient::new(base_url, app.cloud_api_key.as_deref()));
                rebuild = true;
            }
            CommandEffect::Quit => {
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
                should_quit = true;
            }
            CommandEffect::ToggleMap => {
                println!("=== Parish Map ===");
                let player_loc = app.world.player_location;
                for node_id in app.world.graph.location_ids() {
                    if let Some(data) = app.world.graph.get(node_id) {
                        let marker = if node_id == player_loc { " * " } else { "   " };
                        println!("{}{}", marker, data.name);
                    }
                }
                println!();
                println!("Connections:");
                for node_id in app.world.graph.location_ids() {
                    if let Some(data) = app.world.graph.get(node_id) {
                        for (neighbor_id, _) in app.world.graph.neighbors(node_id) {
                            if node_id.0 < neighbor_id.0 {
                                let neighbor_name = app
                                    .world
                                    .graph
                                    .get(neighbor_id)
                                    .map(|d| d.name.as_str())
                                    .unwrap_or("???");
                                println!("  {} — {}", data.name, neighbor_name);
                            }
                        }
                    }
                }
            }
            CommandEffect::OpenDesigner => {
                println!("The Parish Designer is only available in the GUI.");
            }
            CommandEffect::SaveGame => {
                if let Some(ref db) = app.db {
                    let snapshot =
                        crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    match db.save_snapshot(app.active_branch_id, &snapshot).await {
                        Ok(snap_id) => {
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
            CommandEffect::ForkBranch(name) => {
                if let Some(ref db) = app.db {
                    let snapshot =
                        crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                    match db.create_branch(name, Some(app.active_branch_id)).await {
                        Ok(new_branch_id) => {
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
            CommandEffect::LoadBranch(name) => {
                handle_headless_load(app, name).await;
            }
            CommandEffect::ListBranches => {
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
            CommandEffect::ShowLog => {
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
            CommandEffect::Debug(sub) => {
                let lines = crate::debug::handle_debug(sub.as_deref(), app);
                for line in lines {
                    println!("{}", line);
                }
            }
            CommandEffect::ShowSpinner(secs) => {
                let secs = *secs;
                println!("Showing spinner for {} seconds...", secs);
                let mut anim = LoadingAnimation::new();
                let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
                while std::time::Instant::now() < end {
                    anim.tick();
                    let (r, g, b) = anim.current_color_rgb();
                    print!(
                        "\r  \x1b[38;2;{};{};{}m{} {}\x1b[0m\x1b[K",
                        r,
                        g,
                        b,
                        anim.spinner_char(),
                        anim.phrase()
                    );
                    std::io::stdout().flush().ok();
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }
                println!("\r\x1b[K");
            }
            CommandEffect::NewGame => {
                handle_headless_new_game(app).await;
            }
            CommandEffect::SaveFlags => {
                if let Some(ref p) = app.flags_path
                    && let Err(e) = app.flags.save_to_file(p)
                {
                    eprintln!("Warning: failed to save feature flags: {}", e);
                }
            }
            CommandEffect::ApplyTheme(..) => {
                // No visual theme in headless mode; response text is printed below.
            }
            CommandEffect::ApplyTiles(..) => {
                // No map in headless mode; response text is printed below.
            }
        }
    }

    // Print the shared handler's response text.
    if !result.response.is_empty() {
        // The shared handler returns Help as a fallback; override with headless-specific help.
        if result.effects.is_empty()
            || !result
                .effects
                .iter()
                .any(|e| matches!(e, CommandEffect::Quit))
        {
            println!("{}", result.response);
        }
    }

    (should_quit, rebuild)
}

/// Handles /load in headless mode (both bare /load and /load <branch_name>).
async fn handle_headless_load(app: &mut App, name: &str) {
    if name.is_empty() {
        // Bare /load — show save picker for switching save files
        let saves_dir = std::path::PathBuf::from(crate::persistence::picker::SAVES_DIR);
        if let Some(new_path) =
            crate::persistence::picker::run_load_picker(&saves_dir, &app.world.graph)
        {
            if let Some(ref db) = app.db {
                let snapshot =
                    crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
            }
            if let Some(ref gm) = app.game_mod {
                if let Ok(world) = parish_core::game_mod::world_state_from_mod(gm) {
                    app.world = world;
                }
                let npcs_path = gm.npcs_path();
                if npcs_path.exists()
                    && let Ok(mgr) = NpcManager::load_from_file(&npcs_path)
                {
                    app.npc_manager = mgr;
                }
            }
            // Release old lock and acquire lock on the new save file.
            app.save_lock = crate::persistence::SaveFileLock::try_acquire(&new_path);

            match crate::persistence::Database::open(&new_path) {
                Ok(new_db) => {
                    let async_db = Arc::new(crate::persistence::AsyncDatabase::new(new_db));
                    restore_from_db(app, &async_db).await;
                    app.db = Some(async_db);
                    app.save_file_path = Some(new_path);
                    app.last_autosave = Some(std::time::Instant::now());
                    print_location_arrival(app);
                    print_arrival_reactions(app).await;
                }
                Err(e) => eprintln!("Failed to open save file: {}", e),
            }
        }
    } else if let Some(ref db) = app.db {
        match db.find_branch(name).await {
            Ok(Some(branch)) => {
                if branch.id != app.active_branch_id {
                    let snapshot =
                        crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
                    let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                }
                match db.load_latest_snapshot(branch.id).await {
                    Ok(Some((snap_id, loaded_snapshot))) => {
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
                        app.npc_manager.assign_tiers(&app.world, &[]);
                        let time = app.world.clock.time_of_day();
                        let season = app.world.clock.season();
                        let loc = app.world.current_location().name.clone();
                        println!("Loaded branch '{}'. {} — {}, {}.", name, loc, season, time);
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

/// Handles /new in headless mode — resets world and NPCs.
async fn handle_headless_new_game(app: &mut App) {
    if let Some(ref gm) = app.game_mod {
        match parish_core::game_mod::world_state_from_mod(gm) {
            Ok(world) => app.world = world,
            Err(e) => {
                eprintln!("Failed to reset world: {}", e);
                return;
            }
        }
        let npcs_path = gm.npcs_path();
        if npcs_path.exists() {
            match NpcManager::load_from_file(&npcs_path) {
                Ok(mgr) => app.npc_manager = mgr,
                Err(e) => eprintln!("Warning: Failed to reload NPCs: {}", e),
            }
        }
    }
    app.npc_manager.assign_tiers(&app.world, &[]);
    if let Some(ref db) = app.db
        && let Ok(branch_id) = db.create_branch("main", None).await
    {
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        if let Ok(snap_id) = db.save_snapshot(branch_id, &snapshot).await {
            app.active_branch_id = branch_id;
            app.latest_snapshot_id = snap_id;
            app.last_autosave = Some(std::time::Instant::now());
        }
    }
    println!("A new day dawns in the parish.");
    println!();
    print_location_arrival(app);
    print_arrival_reactions(app).await;
}

/// Handles game input (NPC interaction or intent parsing) in headless mode.
async fn handle_headless_game_input(
    app: &mut App,
    client: Option<&OpenAiClient>,
    model: &str,
    text: &str,
    request_id: &mut u64,
) -> Result<()> {
    // Parse intent: try local keyword matching first, fall back to LLM.
    let intent = if let Some(local) = crate::input::parse_intent_local(text) {
        local
    } else if let Some(client) = client {
        app.world.clock.inference_pause();
        let result = parse_intent(client, text, model).await;
        app.world.clock.inference_resume();
        result?
    } else {
        // No client (e.g. simulator mode) — treat as generic dialogue.
        crate::input::PlayerIntent {
            intent: crate::input::IntentKind::Talk,
            target: None,
            dialogue: Some(text.to_string()),
            raw: text.to_string(),
        }
    };

    match intent.intent {
        crate::input::IntentKind::Move => {
            if let Some(target) = &intent.target {
                handle_headless_movement(app, target).await;
            } else {
                println!("And where would ye be off to?");
            }
        }
        crate::input::IntentKind::Look => {
            print_location_description(app);
        }
        _ => {
            // Extract @mention for NPC targeting, if present
            let (target_name, dialogue) = match extract_mention(text) {
                Some(mention) => (Some(mention.name), mention.remaining),
                None => (None, text.to_string()),
            };

            // Detect player self-introduction before building the NPC prompt
            if app.world.player_name.is_none()
                && let Some(name) = parish_core::npc::detect_player_name(&dialogue)
            {
                app.world.player_name = Some(name);
            }

            // Route to NPC conversation if one is present
            if let Some(setup) = parish_core::ipc::prepare_npc_conversation(
                &app.world,
                &mut app.npc_manager,
                &dialogue,
                target_name.as_deref(),
                app.improv_enabled,
            ) {
                // Teach this NPC the player's name if introduced
                if app.world.player_name.is_some()
                    && parish_core::npc::detect_player_name(&dialogue).is_some()
                {
                    app.npc_manager.teach_player_name(setup.npc_id);
                }
                let npc_id = setup.npc_id;
                let system_prompt = setup.system_prompt;
                let context = setup.context;

                if let Some(queue) = &app.inference_queue {
                    // Pause the game clock during NPC dialogue inference
                    app.world.clock.inference_pause();

                    *request_id += 1;

                    let (token_tx, token_rx) = mpsc::unbounded_channel::<String>();

                    let npc_display_name = setup.display_name;
                    let npc_actual_name = setup.npc_name;
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
                            None,
                            Some(0.7),
                            parish_core::inference::InferencePriority::Interactive,
                        )
                        .await
                    {
                        Ok(rx) => {
                            let stream_handle = tokio::spawn(async move {
                                let accumulated =
                                    parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
                                        // Cancel loading animation on first token
                                        cancel_for_stream.notify_one();
                                        print!("{}", batch);
                                        std::io::stdout().flush().ok();
                                    })
                                    .await;
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
                                        let parsed = parse_npc_stream_response(&response.text);
                                        if let Some(meta) = &parsed.metadata {
                                            tracing::debug!(
                                                "NPC metadata: action={}, mood={}",
                                                meta.action,
                                                meta.mood
                                            );
                                        }

                                        // Update NPC mood and record speaker's own memory
                                        let game_time = app.world.clock.now();
                                        let player_name_for_mem =
                                            if app.npc_manager.knows_player_name(npc_id) {
                                                app.world.player_name.clone()
                                            } else {
                                                None
                                            };
                                        if let Some(npc_mut) = app.npc_manager.get_mut(npc_id) {
                                            let debug_events = parish_core::npc::ticks::apply_tier1_response_with_config(
                                                npc_mut,
                                                &parsed,
                                                text,
                                                game_time,
                                                &Default::default(),
                                                player_name_for_mem.as_deref(),
                                            );
                                            for event in &debug_events {
                                                app.debug_event(event.clone());
                                            }
                                        }

                                        // Record conversation exchange
                                        let game_time = app.world.clock.now();
                                        let location = app.world.player_location;
                                        app.world.conversation_log.add(
                                            parish_core::npc::conversation::ConversationExchange {
                                                timestamp: game_time,
                                                speaker_id: npc_id,
                                                speaker_name: npc_actual_name.clone(),
                                                player_input: text.to_string(),
                                                npc_dialogue: parsed.dialogue.clone(),
                                                location,
                                            },
                                        );

                                        // Record witness memories for bystander NPCs
                                        let witness_events =
                                            parish_core::npc::ticks::record_witness_memories(
                                                app.npc_manager.npcs_mut(),
                                                npc_id,
                                                &npc_display_name,
                                                text,
                                                &parsed.dialogue,
                                                game_time,
                                                location,
                                            );
                                        for event in &witness_events {
                                            app.debug_event(event.clone());
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

                    // Resume the game clock now that inference is complete
                    app.world.clock.inference_resume();
                } else {
                    println!("[No storyteller could be found in the parish today.]");
                }
            } else {
                let idx = HEADLESS_IDLE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                println!(
                    "{}",
                    parish_core::ipc::IDLE_MESSAGES[idx % parish_core::ipc::IDLE_MESSAGES.len()]
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

    let transport = default_transport(app);
    let exits = format_exits(
        app.world.player_location,
        &app.world.graph,
        transport.speed_m_per_s,
        &transport.label,
    );
    println!("{}", exits);
    println!();
}

/// Generates and prints NPC arrival reactions (greetings, nods, introductions).
///
/// For reactions flagged `use_llm`, attempts a short-timeout LLM call for a
/// richer greeting, falling back to canned text on timeout or error.
async fn print_arrival_reactions(app: &mut App) {
    use parish_core::config::ReactionConfig;
    use parish_core::dice;
    use parish_core::npc::reactions::{generate_arrival_reactions, resolve_llm_greeting};

    let npcs = app.npc_manager.npcs_at(app.world.player_location);
    if npcs.is_empty() {
        return;
    }

    let loc_data = match app.world.current_location_data() {
        Some(d) => d.clone(),
        None => return,
    };

    let tod = app.world.clock.time_of_day();
    let weather = app.world.weather.to_string();
    let introduced = app.npc_manager.introduced_set();
    let templates = app
        .game_mod
        .as_ref()
        .map(|gm| &gm.reactions)
        .cloned()
        .unwrap_or_default();
    let config = ReactionConfig::default();
    let roll_dice = dice::roll_n(npcs.len() * 2);

    let reactions = generate_arrival_reactions(
        &npcs,
        &introduced,
        &loc_data,
        tod,
        &weather,
        &templates,
        &config,
        &roll_dice,
    );

    for reaction in &reactions {
        let text = if reaction.use_llm {
            if let Some(client) = &app.reaction_client {
                let npc = app.npc_manager.get(reaction.npc_id);
                if let Some(npc) = npc {
                    let at_workplace = npc.workplace.is_some_and(|wp| wp == loc_data.id);
                    resolve_llm_greeting(
                        reaction,
                        npc,
                        &loc_data.name,
                        tod,
                        &weather,
                        introduced.contains(&reaction.npc_id),
                        at_workplace,
                        client,
                        &app.reaction_model.clone(),
                        config.llm_timeout_secs,
                    )
                    .await
                } else {
                    reaction.canned_text.clone()
                }
            } else {
                reaction.canned_text.clone()
            }
        } else {
            reaction.canned_text.clone()
        };

        println!("{}", text);

        if reaction.introduces {
            app.npc_manager.mark_introduced(reaction.npc_id);
        }
    }
}

/// Returns the default transport mode from the game mod, or walking.
fn default_transport(app: &App) -> TransportMode {
    app.game_mod
        .as_ref()
        .map(|gm| gm.transport.default_mode().clone())
        .unwrap_or_else(TransportMode::walking)
}

/// Prints current location description and exits (headless /look).
fn print_location_description(app: &App) {
    let transport = default_transport(app);
    let text = parish_core::ipc::render_look_text(
        &app.world,
        &app.npc_manager,
        transport.speed_m_per_s,
        &transport.label,
        true,
    );
    println!("{}", text);
}

/// Handles movement in headless mode.
async fn handle_headless_movement(app: &mut App, target: &str) {
    let transport = default_transport(app);
    let result = movement::resolve_movement(
        target,
        &app.world.graph,
        app.world.player_location,
        &transport,
    );

    match result {
        MovementResult::Arrived {
            destination,
            path,
            minutes,
            narration,
        } => {
            println!("{}", narration);
            println!();

            app.world.record_path_traversal(&path);
            app.world.clock.advance(minutes as i64);
            app.world.player_location = destination;
            app.world.mark_visited(destination);

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
            print_arrival_reactions(app).await;
        }
        MovementResult::AlreadyHere => {
            println!("Sure, you're already standing right here.");
        }
        MovementResult::NotFound(name) => {
            println!(
                "You haven't the faintest notion how to reach \"{}\". Try asking about.",
                name
            );
            let exits = format_exits(
                app.world.player_location,
                &app.world.graph,
                transport.speed_m_per_s,
                &transport.label,
            );
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
    use chrono::Timelike;

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

    #[tokio::test]
    async fn test_restore_from_db_fresh_database() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));

        // Fresh DB — should create initial snapshot
        restore_from_db(&mut app, &async_db).await;
        assert_eq!(app.active_branch_id, 1);
        assert!(app.latest_snapshot_id > 0);
    }

    #[tokio::test]
    async fn test_restore_from_db_with_existing_snapshot() {
        let app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));

        // Save a snapshot first
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();

        // Now restore — should load the existing snapshot
        let mut app2 = App::new();
        restore_from_db(&mut app2, &async_db).await;
        assert_eq!(app2.active_branch_id, branch.id);
        assert_eq!(app2.latest_snapshot_id, snap_id);
    }

    #[tokio::test]
    async fn test_handle_load_bare_no_db() {
        // Bare /load without a DB should not crash
        let mut app = App::new();
        let (quit, _rebuild) =
            handle_headless_command(&mut app, Command::Load(String::new())).await;
        assert!(!quit);
    }

    // --- Additional headless command tests ---

    #[tokio::test]
    async fn test_handle_headless_command_wait() {
        let mut app = App::new();
        app.world.clock.pause(); // freeze for determinism
        let hour_before = app.world.clock.now().hour();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Wait(60)).await;
        assert!(!quit);
        assert!(!rebuild);
        // Time should have advanced by 60 minutes
        let hour_after = app.world.clock.now().hour();
        assert_eq!((hour_after + 24 - hour_before) % 24, 1);
    }

    #[tokio::test]
    async fn test_handle_headless_command_debug_none() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Debug(None)).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_debug_with_subcommand() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::Debug(Some("clock".to_string()))).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_toggle_sidebar() {
        // In headless mode, ToggleSidebar just prints a message (not available)
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ToggleSidebar).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_toggle_improv() {
        let mut app = App::new();
        let was_improv = app.improv_enabled;
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ToggleImprov).await;
        assert!(!quit);
        assert!(!rebuild);
        assert_ne!(app.improv_enabled, was_improv);
    }

    #[tokio::test]
    async fn test_handle_headless_command_about() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::About).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_map() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Map(None)).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_npcs_here() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::NpcsHere).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_time() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Time).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_invalid_speed() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::InvalidSpeed("bogus".to_string())).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_invalid_branch_name() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::InvalidBranchName("Bad name!".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_log() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Log).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_branches_no_db() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Branches).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_tick() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Tick).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloud).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloudModel).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud_key() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloudKey).await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_cloud_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCloudModel("claude-sonnet".to_string()),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild); // SetCloudModel doesn't trigger rebuild
        assert_eq!(app.cloud_model_name.as_deref(), Some("claude-sonnet"));
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_cloud_key() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetCloudKey("sk-cloud-123".to_string()))
                .await;
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.cloud_api_key.as_deref(), Some("sk-cloud-123"));
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_category_provider() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::ShowCategoryProvider(InferenceCategory::Dialogue),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_category_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::ShowCategoryModel(InferenceCategory::Intent),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_category_key() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::ShowCategoryKey(InferenceCategory::Simulation),
        )
        .await;
        assert!(!quit);
        assert!(!rebuild);
    }

    #[test]
    fn test_default_transport_no_mod() {
        let app = App::new();
        let transport = default_transport(&app);
        assert_eq!(transport.id, "walking");
        assert!((transport.speed_m_per_s - 1.25).abs() < f64::EPSILON);
    }
}
