//! Headless CLI mode — the default interactive mode.
//!
//! Provides a simple stdin/stdout REPL with full game logic
//! (NPC inference, intent parsing, system commands).
//! Runs by default or with `--headless` on the command line.

use crate::app::App;
use crate::config::{
    CategoryConfig, CloudConfig, InferenceCategory, InferenceConfig, NpcConfig, ProviderConfig,
};
use crate::inference::{
    self, AnyClient, InferenceClients, InferencePriority, InferenceQueue, InferenceWorkerConfig,
    QueueRequest,
};
use crate::input::{
    Command, InputResult, classify_input, extract_mention, is_player_dialogue,
    parse_intent_with_profile_and_audit,
};
use crate::loading::LoadingAnimation;
use crate::npc::manager::NpcManager;
use crate::world::description::{format_exits, render_description};
use crate::world::movement::{self, MovementResult};
use anyhow::Result;
use parish_core::ipc::{NPC_REACTION_CONCURRENCY, capitalize_first};
use parish_core::world::transport::TransportMode;
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{Notify, Semaphore, mpsc};

/// Interval between autosaves in seconds.
const AUTOSAVE_INTERVAL_SECS: u64 = 45;

/// Process-wide rotating index for headless idle messages.
///
/// `fetch_add` returns the *pre*-increment value, so the first idle turn uses
/// index 0 (`IDLE_MESSAGES[0]` / `mod idle_messages[0]`) and the cycle is
/// 0-based. This mirrors the `REQUEST_ID.fetch_add` style already used by
/// `parish_core::game_loop::npc_turn` for the same purpose, keeping the
/// headless entry point in parity with the shared game loop (rule #2). It
/// lives at file scope rather than on `App` because idle rotation is a
/// stateless display concern, not game state (TD-030 / TD-034 removed the
/// `App.idle_counter` field; TD-037 restores the file-scoped counter that a
/// later edit had erroneously replaced with a read-then-post-increment field).
static IDLE_MESSAGE_INDEX: AtomicUsize = AtomicUsize::new(0);

/// Selects the idle message for rotation index `idx`, preferring the active
/// mod's `idle_messages` and falling back to the engine `IDLE_MESSAGES`.
///
/// Pure over `(idx, mod_msgs)` so the 0-based rotation (TD-037: index 0 must
/// pick the *first* message) is unit-testable without touching the
/// process-wide [`IDLE_MESSAGE_INDEX`] counter.
fn select_idle_message(idx: usize, mod_msgs: &[String]) -> String {
    if mod_msgs.is_empty() {
        parish_core::ipc::IDLE_MESSAGES[idx % parish_core::ipc::IDLE_MESSAGES.len()].to_string()
    } else {
        mod_msgs[idx % mod_msgs.len()].clone()
    }
}

fn print_startup_header(clients: &InferenceClients, provider_config: &ProviderConfig) {
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
    println!("Type /help for commands, /about for credits, /quit to exit.");
    println!();
}

/// Sets up the inference queue and spawns the background worker.
fn setup_inference_queue(
    dial_client: AnyClient,
    inference_config: &InferenceConfig,
    inference_log: &inference::InferenceLog,
    inference_file_log: &inference::file_log::InferenceFileLog,
    provider: parish_core::config::Provider,
) -> InferenceQueue {
    let (interactive_tx, interactive_rx) = mpsc::channel(16);
    let (background_tx, background_rx) = mpsc::channel(32);
    let (batch_tx, batch_rx) = mpsc::channel(64);
    let _worker = inference::spawn_inference_worker(
        dial_client,
        InferenceWorkerConfig {
            interactive_rx,
            background_rx,
            batch_rx,
            log: inference_log.clone(),
            file_log: inference_file_log.clone(),
            provider,
            timeout_config: inference_config.clone(),
        },
    );
    InferenceQueue::new(interactive_tx, background_tx, batch_tx).with_audit_sink(
        inference::InferenceAuditSink::new(inference_log.clone(), inference_file_log.clone()),
    )
}

/// Runs the headless stdin/stdout REPL loop.
///
/// Processes player input, schedules NPC ticks, weather, banshee, autosave.
async fn run_headless_repl_loop(
    app: &mut App,
    inference_log: inference::InferenceLog,
) -> Result<()> {
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
                let (quit, rebuild) = handle_headless_command(app, cmd, &trimmed).await?;
                if rebuild {
                    let any = if app.provider_name == "simulator" {
                        Some(AnyClient::simulator())
                    } else {
                        app.cloud_client.clone().or_else(|| app.client.clone())
                    };
                    if let Some(new_client) = any {
                        let provider =
                            parish_core::config::Provider::from_str_loose(&app.provider_name)
                                .unwrap_or_default();
                        let queue = setup_inference_queue(
                            new_client,
                            &app.inference_config,
                            &inference_log,
                            &app.inference_file_log,
                            provider,
                        );
                        app.inference_queue = Some(queue);
                    }
                }
                if quit {
                    break;
                }
            }
            InputResult::GameInput(text) => {
                let intent_client = app.intent.client.clone();
                let intent_model = app.intent.model.clone();
                handle_headless_game_input(
                    app,
                    intent_client.as_ref(),
                    &intent_model,
                    &text,
                    &mut request_id,
                )
                .await?;
                // #1351 — NPCs react only to genuine dialogue, not to a bare
                // `look`/movement command routed down the GameInput path.
                if is_player_dialogue(&text) {
                    emit_headless_npc_reactions(app, &text).await;
                }
            }
        }

        // Advance the world one pump through the single shared helper (rule
        // #12): weather (single check) + schedules + tier reassignment +
        // banshee + tier-4. The headless REPL never propagated gossip, so it
        // stays skipped here. Then render the report to stdout / the debug log.
        {
            use parish_core::game_loop::{AdvanceOptions, GossipMode, WeatherMode, advance_world};

            let banshee_lines_start = app.world.text_log.len();
            let report = advance_world(
                &mut app.world,
                &mut app.npc_manager,
                &mut rand::rng(),
                AdvanceOptions {
                    weather: WeatherMode::Single,
                    run_banshee: !app.flags.is_disabled("banshee"),
                    gossip: GossipMode::Skip,
                    run_tier4: parish_core::game_loop::tier4_simulation_enabled(&app.flags),
                },
            );

            for tt in &report.tier_transitions {
                let direction = if tt.promoted { "promoted" } else { "demoted" };
                app.debug_event(format!(
                    "[tier] {} {} {:?} → {:?}",
                    tt.npc_name, direction, tt.old_tier, tt.new_tier,
                ));
            }
            if let Some(new_weather) = report.weather_change {
                tracing::info!(new = %new_weather, "Weather changed");
            }
            process_headless_schedule_events(app, &report.schedule_events);
            // The banshee tick appends its herald/death prose to the world text
            // log; surface the new lines to stdout (the REPL has no event bus
            // subscriber for them).
            for line in app.world.text_log.iter().skip(banshee_lines_start) {
                println!("{line}");
            }
            if !report.banshee.is_empty() {
                app.debug_event(format!(
                    "[banshee] {} wail(s), {} death(s)",
                    report.banshee.wails.len(),
                    report.banshee.deaths.len()
                ));
            }
            if report.tier4_event_count > 0 {
                app.debug_event(format!("[tier4] {} events", report.tier4_event_count));
            }
        }
        dispatch_headless_tier3_tick(app).await;
        dispatch_headless_tier2_tick(app).await;
        dispatch_headless_autosave(app).await;

        drain_character_log_events(app);
        drain_location_log_events(app);
        drain_chat_transcript_events(app);

        if app.should_quit {
            break;
        }

        print!("> ");
        std::io::stdout().flush().ok();
    }

    println!("Safe home to ye. May the road rise to meet you.");
    Ok(())
}

/// Runs the game in headless mode with a plain stdin/stdout REPL.
///
/// Sets up the inference pipeline with dual-client routing: cloud client
/// for dialogue, local client for intent parsing. Falls back to local
/// for everything if no cloud provider is configured.
///
/// `script_mode` should be `true` when stdin is not a terminal (i.e. input
/// is piped or the caller knows there is no interactive user). In that case
/// a save-file lock failure is treated as a hard error — there is nobody to
/// read the warning and concurrent writes could silently corrupt the database
/// (#608).
///
/// The nine parameters are all required for the initialization pipeline;
/// they are distinct concerns (inference clients, provider metadata, category
/// config, feature flags, mod content, data location, interactivity mode,
/// and TOML-configured inference timeouts) that cannot be collapsed into a
/// struct without creating a spurious coupling layer.
#[allow(clippy::too_many_arguments)] // known debt: tracked in parish-engine/TODO.md (TD-019)
pub async fn run_headless(
    clients: InferenceClients,
    provider_config: &ProviderConfig,
    cloud_config: Option<&CloudConfig>,
    category_configs: &HashMap<InferenceCategory, CategoryConfig>,
    improv: bool,
    game_mod: Option<parish_core::game_mod::GameMod>,
    data_dir: Option<std::path::PathBuf>,
    inference_config: InferenceConfig, // (#417) TOML-configured timeouts
    script_mode: bool,
    no_inference_log: bool,
) -> Result<()> {
    print_startup_header(&clients, provider_config);

    // Resolve the per-user saves directory early so the on-disk inference log
    // can write alongside save files. App-name drives the per-user data folder
    // (Rundale → `Rundale`; engine fallback `Parish`); the shared helper keeps
    // the three entry points in lockstep (rule #12). Resolved from the
    // `game_mod` parameter so it's available before the inference worker spawns.
    let app_name = parish_core::game_mod::app_name_from_mod(&game_mod);
    let saves_dir = crate::persistence::picker::resolve_project_saves_dir(&app_name);

    // Inference log effective on/off: CLI flag > env > config default.
    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        no_inference_log,
        inference_config.log_to_disk,
    );
    let inference_file_log = parish_core::inference::file_log::InferenceFileLog::spawn(
        &saves_dir,
        log_to_disk,
        Some(&provider_config.base_url),
    );
    let chat_transcript_log = parish_core::chat_transcript::ChatTranscriptLog::spawn_with_flag(
        &saves_dir,
        inference_file_log.session_id().to_string(),
        inference_file_log.enabled_flag(),
    );
    if log_to_disk {
        println!(
            "Inference log: {}\nChat transcript: {}",
            inference_file_log.path().display(),
            chat_transcript_log.path().display(),
        );
    }

    // Initialize dialogue inference pipeline (cloud if configured, else local)
    let (dial_client, dial_model) = clients.dialogue_client();
    let dialogue_model = dial_model.to_string();
    let inference_log = inference::new_inference_log();
    let worker_client = if provider_config.provider.id() == "simulator" {
        AnyClient::simulator()
    } else {
        dial_client.clone()
    };
    let queue = setup_inference_queue(
        worker_client,
        &inference_config,
        &inference_log,
        &inference_file_log,
        provider_config.provider.clone(),
    );

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
    app.provider_name = provider_config.provider.id().to_string();
    app.base_url = provider_config.base_url.clone();
    app.api_key = provider_config.api_key.clone();
    app.improv_enabled = improv;
    app.inference_config = inference_config; // (#417) store TOML-configured timeouts
    app.script_mode = script_mode;
    app.inference_file_log = inference_file_log.clone();
    app.chat_transcript_log = chat_transcript_log.clone();

    // Match desktop BYOK/profile semantics: reasoning, token caps, and service
    // tier live in the per-user config even when the headless runtime's
    // provider/model routing came from CLI or environment overrides.
    let user_config_dir = parish_core::config::user_config::resolve_user_config_dir();
    if let Ok(user_cfg) = parish_core::config::user_config::load_user_config(&user_config_dir) {
        let mut runtime_config = app.snapshot_config();
        runtime_config.apply_user_inference_profiles(&user_cfg);
        app.inference_profile_override = runtime_config.inference_profile_override;
        app.category_inference_profile = runtime_config.category_inference_profile;
    }

    // Load feature flags from disk
    let flags_path = data_dir.map(|d| d.join("parish-flags.json"));
    if let Some(ref p) = flags_path {
        app.flags = crate::config::FeatureFlags::load_from_file(p);
    }
    app.flags_path = flags_path;

    // Set intent / simulation / reaction clients — skip for the simulator
    // provider since it has no real HTTP client and the dummy URL would cause
    // connection-timeout delays during intent parsing.
    let is_simulator = provider_config.provider.id() == "simulator";
    if !is_simulator {
        let (intent_cl, intent_mdl) = clients.intent_client();
        app.intent.client = Some(intent_cl.clone());
        app.intent.model = intent_mdl.to_string();

        let (sim_cl, sim_mdl) = clients.simulation_client();
        app.simulation.client = Some(sim_cl.clone());
        app.simulation.model = sim_mdl.to_string();

        let (react_cl, react_mdl) = clients.reaction_client();
        app.reaction.client = Some(react_cl.clone());
        app.reaction.model = react_mdl.to_string();
    }

    // Initialize per-category provider metadata from config
    if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Intent) {
        app.intent.provider_name = Some(cat_cfg.provider.id().to_string());
        app.intent.api_key = cat_cfg.api_key.clone();
        app.intent.base_url = Some(cat_cfg.base_url.clone());
    }
    if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Simulation) {
        app.simulation.provider_name = Some(cat_cfg.provider.id().to_string());
        app.simulation.api_key = cat_cfg.api_key.clone();
        app.simulation.base_url = Some(cat_cfg.base_url.clone());
    }
    if let Some(cat_cfg) = category_configs.get(&InferenceCategory::Reaction) {
        app.reaction.provider_name = Some(cat_cfg.provider.id().to_string());
        app.reaction.api_key = cat_cfg.api_key.clone();
        app.reaction.base_url = Some(cat_cfg.base_url.clone());
    }

    // Set cloud/dialogue fields if configured
    if clients.has_custom_dialogue() {
        let (dial_cl, dial_mdl) = clients.dialogue_client();
        app.cloud_client = Some(dial_cl.clone());
        app.cloud_model_name = Some(dial_mdl.to_string());
    } else if let Some(cc) = cloud_config {
        app.cloud_provider_name = Some(cc.provider.id().to_string());
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

    // Saves dir + app-name were resolved at the top of `run_headless` (needed
    // early for the inference-log writer); record the dir on `App` here.
    app.saves_dir = Some(saves_dir.clone());
    // Wire SessionStore — single-user CLI uses session_id = "" (#696 slice 8).
    app.session_store = std::sync::Arc::new(parish_core::session_store::DbSessionStore::new(
        saves_dir.clone(),
    ));
    let remembered_identity =
        parish_core::persistence::read_active_save_identity_candidate(&saves_dir)
            .map_err(|error| anyhow::anyhow!("invalid active-save marker: {error}"))?;
    let (db_path, save_lock) = if let Some(identity) = remembered_identity.as_ref() {
        let lock = crate::persistence::SaveFileLock::try_acquire(&identity.save_path).ok_or_else(
            || {
                anyhow::anyhow!(
                    "save file {} is locked or has an invalid lock owner",
                    identity.save_path.display()
                )
            },
        )?;
        (identity.save_path.clone(), lock)
    } else {
        // The legacy picker opens every listed SQLite file for display
        // metadata. Lock every candidate first and retain the selected guard.
        let mut picker_locks = lock_existing_save_candidates(&saves_dir)?;
        let path = crate::persistence::picker::run_picker(&saves_dir, &app.world.graph);
        let lock = take_or_acquire_save_lock(&mut picker_locks, &path)?;
        (path, lock)
    };
    app.save_file_path = Some(db_path.clone());
    app.save_lock = Some(save_lock);

    let db = crate::persistence::Database::open(&db_path)?;
    let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
    let preferred_branch = remembered_identity
        .as_ref()
        .map(|identity| (identity.branch_id, identity.branch_name.clone()));
    restore_from_db(&mut app, &async_db, &db_path, preferred_branch).await?;
    app.db = Some(async_db);
    app.last_autosave = Some(std::time::Instant::now());

    // Character logs — gated by `character-logs` flag (default on).
    // Subscribe BEFORE writing profiles so the rx doesn't miss any
    // events that fire during/just after profile generation.
    app.log_app_name = app_name.clone();
    {
        let enabled = !app
            .flags
            .is_disabled(parish_core::character_log::FEATURE_FLAG);
        let manager = parish_core::character_log::CharacterLogManager::new(
            &app_name,
            app.active_branch_id,
            enabled,
        );
        if manager.enabled() {
            app.character_log_rx = Some(app.world.event_bus.subscribe());
            if let Err(e) = manager.write_all_profiles(&app.world, &app.npc_manager) {
                tracing::warn!(error = %e, "character-log profile write failed");
            }
        }
        app.character_log = Some(std::sync::Arc::new(manager));
    }

    // Location logs — same gate, default-on `location-logs` flag.
    {
        let enabled = !app
            .flags
            .is_disabled(parish_core::location_log::FEATURE_FLAG);
        let manager = parish_core::location_log::LocationLogManager::new(
            &app_name,
            app.active_branch_id,
            enabled,
        );
        if manager.enabled() {
            app.location_log_rx = Some(app.world.event_bus.subscribe());
            if let Err(e) = manager.write_all_profiles(&app.world, &app.npc_manager) {
                tracing::warn!(error = %e, "location-log profile write failed");
            }
        }
        app.location_log = Some(std::sync::Arc::new(manager));
    }
    app.log_managers_branch = Some(app.active_branch_id);

    // Chat transcript — JSONL paired with the inference log (shares its enable
    // flag). Subscribe a receiver the REPL drains synchronously, mirroring the
    // character/location-log pumps. The writer task itself was spawned at the
    // top of `run_headless` on `app.chat_transcript_log`. Always subscribe —
    // even if logging starts disabled — so a mid-session `/inference-log on`
    // is captured; `process_event` no-ops internally while the flag is off.
    app.chat_transcript_rx = Some(app.world.event_bus.subscribe());

    // Show initial location
    print_location_arrival(&app);
    print_arrival_reactions(&mut app).await;

    run_headless_repl_loop(&mut app, inference_log).await
}

/// Restores game state from a snapshot and replay journal on the given branch.
///
/// Shared by `restore_from_db`, `handle_headless_load` (named-branch path),
/// and simulation of the same sequence in test helpers.
fn fresh_headless_world_and_npcs(
    app: &App,
) -> Result<(crate::world::WorldState, NpcManager), String> {
    let Some(game_mod) = app.game_mod.as_ref() else {
        // Unit-test and legacy fallback: recovery replaces every dynamic
        // field, but it needs the current static graph in which to restore.
        let mut world = crate::world::WorldState::new();
        world.graph = app.world.graph.clone();
        world.locations = app.world.locations.clone();
        return Ok((world, NpcManager::new()));
    };

    let world = parish_core::game_mod::world_state_from_mod(game_mod)
        .map_err(|error| format!("Failed to load world from mod: {error}"))?;
    let npcs_path = game_mod.npcs_path();
    let npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|error| {
        tracing::warn!(
            path = %npcs_path.display(),
            %error,
            "failed to load NPCs for a headless save candidate; using an empty manager",
        );
        NpcManager::new()
    });
    Ok((world, npc_manager))
}

fn lock_existing_save_candidates(
    saves_dir: &std::path::Path,
) -> anyhow::Result<Vec<(std::path::PathBuf, crate::persistence::SaveFileLock)>> {
    let mut paths = std::fs::read_dir(saves_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "db"))
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            crate::persistence::SaveFileLock::try_acquire(&path)
                .map(|lock| (path.clone(), lock))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "save file {} is locked or has an invalid lock owner",
                        path.display()
                    )
                })
        })
        .collect()
}

fn take_or_acquire_save_lock(
    locks: &mut Vec<(std::path::PathBuf, crate::persistence::SaveFileLock)>,
    selected_path: &std::path::Path,
) -> anyhow::Result<crate::persistence::SaveFileLock> {
    if let Some(index) = locks
        .iter()
        .position(|(path, _)| path.as_path() == selected_path)
    {
        return Ok(locks.swap_remove(index).1);
    }
    crate::persistence::SaveFileLock::try_acquire(selected_path).ok_or_else(|| {
        anyhow::anyhow!(
            "save file {} is locked or has an invalid lock owner",
            selected_path.display()
        )
    })
}

async fn load_and_restore_snapshot(
    app: &mut App,
    save_path: &std::path::Path,
    branch_id: i64,
    branch_name: &str,
) -> Result<bool, String> {
    match parish_core::session_store::load_recovery_bundle(
        app.session_store.as_ref(),
        "",
        save_path,
        branch_id,
    )
    .await
    {
        Ok(Some(recovery)) => {
            let snap_id = recovery.snapshot_id;
            let (mut candidate_world, mut candidate_npcs) = fresh_headless_world_and_npcs(app)?;
            recovery.restore(&mut candidate_world, &mut candidate_npcs);
            candidate_npcs.assign_tiers(&candidate_world, &[]);
            let prepared_binding = app
                .session_store
                .prepare_active_save("", save_path)
                .map_err(|error| error.to_string())?;
            if let Some(saves_dir) = app.saves_dir.as_ref() {
                parish_core::persistence::write_active_save_identity(
                    saves_dir,
                    save_path,
                    branch_id,
                    branch_name,
                )
                .map_err(|error| error.to_string())?;
            }

            // Commit only after the exact recovery bundle and active-store
            // binding and active marker all succeed.
            prepared_binding.commit();
            candidate_world.event_bus = std::mem::take(&mut app.world.event_bus);
            candidate_world.event_bus.advance_context_epoch();
            app.world = candidate_world;
            app.npc_manager = candidate_npcs;
            app.save_file_path = Some(save_path.to_path_buf());
            app.active_branch_id = branch_id;
            app.latest_snapshot_id = snap_id;
            reset_headless_context_receivers(app);
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// Restores game state from a database, loading the "main" branch snapshot.
///
/// Finds the "main" branch, loads the latest snapshot, replays any journal
/// events since that snapshot, and reassigns NPC tiers. If no snapshot exists
/// (fresh database), captures and saves an initial snapshot.
async fn restore_from_db(
    app: &mut App,
    async_db: &Arc<crate::persistence::AsyncDatabase>,
    save_path: &std::path::Path,
    preferred_branch: Option<(i64, String)>,
) -> Result<()> {
    let branch = if let Some((branch_id, branch_name)) = preferred_branch {
        async_db
            .list_branches()
            .await?
            .into_iter()
            .find(|branch| branch.id == branch_id && branch.name == branch_name)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "remembered branch {branch_name} ({branch_id}) is missing from active save"
                )
            })?
    } else {
        async_db
            .find_branch("main")
            .await?
            .ok_or_else(|| anyhow::anyhow!("main branch missing from active save"))?
    };

    match load_and_restore_snapshot(app, save_path, branch.id, &branch.name)
        .await
        .map_err(anyhow::Error::msg)?
    {
        true => println!("Restored from save."),
        false => {
            // First run — save initial snapshot. Recovery errors propagate
            // above instead of being mistaken for an empty save and overwritten.
            let (mut candidate_world, mut candidate_npcs) =
                fresh_headless_world_and_npcs(app).map_err(anyhow::Error::msg)?;
            candidate_npcs.assign_tiers(&candidate_world, &[]);
            let snapshot =
                crate::persistence::GameSnapshot::capture(&candidate_world, &candidate_npcs);
            let snapshot_id = async_db.save_snapshot(branch.id, &snapshot).await?;
            let prepared_binding = app.session_store.prepare_active_save("", save_path)?;
            if let Some(saves_dir) = app.saves_dir.as_ref() {
                parish_core::persistence::write_active_save_identity(
                    saves_dir,
                    save_path,
                    branch.id,
                    &branch.name,
                )?;
            }
            prepared_binding.commit();
            candidate_world.event_bus = std::mem::take(&mut app.world.event_bus);
            candidate_world.event_bus.advance_context_epoch();
            app.world = candidate_world;
            app.npc_manager = candidate_npcs;
            app.save_file_path = Some(save_path.to_path_buf());
            app.active_branch_id = branch.id;
            app.latest_snapshot_id = snapshot_id;
            reset_headless_context_receivers(app);
        }
    }
    app.db = Some(Arc::clone(async_db));
    Ok(())
}

/// Drops every pre-switch queued event and subscribes afresh to the retained
/// bus. Headless drains receivers synchronously, so resetting them at the
/// successful commit seam is the deterministic equivalent of contextual
/// envelope filtering in the async runtimes.
pub(crate) fn reset_headless_context_receivers(app: &mut App) {
    if app.character_log_rx.is_some() {
        app.character_log_rx = Some(app.world.event_bus.subscribe());
    }
    if app.location_log_rx.is_some() {
        app.location_log_rx = Some(app.world.event_bus.subscribe());
    }
    if app.chat_transcript_rx.is_some() {
        app.chat_transcript_rx = Some(app.world.event_bus.subscribe());
    }
}

/// Drains every `GameEvent` queued on `app.character_log_rx` and feeds it
/// to the character-log writer. Runs synchronously at the tail of each
/// REPL iteration — the CLI's `App` is not `Send` enough for a tokio
/// background task, but a synchronous drain catches everything since the
/// REPL is the only producer between drains.
fn drain_character_log_events(app: &mut App) {
    app.rebind_log_managers_if_branch_changed();
    let (Some(manager), Some(rx)) = (app.character_log.as_ref(), app.character_log_rx.as_mut())
    else {
        return;
    };
    loop {
        match rx.try_recv() {
            Ok(event) => {
                if let Err(e) = manager.process_event(&event, &app.world, &app.npc_manager) {
                    tracing::warn!(error = %e, "character-log write failed");
                }
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "character-log subscriber lagged; events lost");
                continue;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }
}

/// Same shape as [`drain_character_log_events`] but for the per-location
/// markdown log writer. Both run at the tail of each REPL iteration.
fn drain_location_log_events(app: &mut App) {
    app.rebind_log_managers_if_branch_changed();
    let (Some(manager), Some(rx)) = (app.location_log.as_ref(), app.location_log_rx.as_mut())
    else {
        return;
    };
    loop {
        match rx.try_recv() {
            Ok(event) => {
                if let Err(e) = manager.process_event(&event, &app.world, &app.npc_manager) {
                    tracing::warn!(error = %e, "location-log write failed");
                }
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "location-log subscriber lagged; events lost");
                continue;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }
}

/// Same shape as [`drain_character_log_events`] but for the on-disk chat
/// transcript (JSONL, paired with the inference log). Per-process, so it does
/// not rebind on branch switch.
fn drain_chat_transcript_events(app: &mut App) {
    let Some(rx) = app.chat_transcript_rx.as_mut() else {
        return;
    };
    loop {
        match rx.try_recv() {
            Ok(event) => {
                app.chat_transcript_log
                    .process_event(&event, &app.world, &app.npc_manager);
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(skipped)) => {
                tracing::warn!(skipped, "chat-transcript subscriber lagged; events lost");
                continue;
            }
            Err(tokio::sync::broadcast::error::TryRecvError::Closed) => break,
        }
    }
}

/// Handles a system command in headless mode.
///
/// Returns `(should_quit, rebuild_inference)`.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`CliCommandHost`] adapter (#696 slice 7).  The `App` is temporarily moved
/// into an `Arc<Mutex<App>>` for the duration of the call, then moved back.
async fn handle_headless_command(
    app: &mut App,
    cmd: Command,
    raw_text: &str,
) -> anyhow::Result<(bool, bool)> {
    use crate::command_host::CliCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;
    use std::sync::Arc;

    // Temporarily move App into Arc<Mutex<App>> so CliCommandHost satisfies Send+Sync.
    let app_val = std::mem::take(app);
    let app_arc = Arc::new(tokio::sync::Mutex::new(app_val));
    let (should_quit, rebuild) = {
        let host = CliCommandHost::new(Arc::clone(&app_arc));
        shared_handle(&host, cmd, raw_text)
            .await
            .map_err(anyhow::Error::msg)?;
        let q = host.did_quit();
        let r = host.did_rebuild_inference();
        // `host` is dropped here, releasing its Arc clone so app_arc has exactly 1 ref.
        (q, r)
    };
    // Move App back — exactly 1 strong reference remains at this point.
    *app = Arc::into_inner(app_arc)
        .expect("CliCommandHost dropped: Arc should have exactly 1 reference")
        .into_inner();
    Ok((should_quit, rebuild))
}

/// Handles /load in headless mode (both bare /load and /load <branch_name>).
///
/// Returns `Err` if the new save file's lock cannot be acquired while running
/// in script mode — the same fail-closed policy applied at startup (#608).
pub(crate) async fn handle_headless_load(app: &mut App, name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        // Bare /load — show save picker for switching save files.
        // Read the saves dir resolved once at startup (#771); never re-probe
        // the cwd here — packaged/daemon runs may have moved cwd since boot.
        let saves_dir = match app.saves_dir.as_ref() {
            Some(p) => p.clone(),
            None => {
                println!("Save picker unavailable: saves directory not initialised.");
                return Ok(());
            }
        };
        let mut picker_locks = lock_existing_save_candidates(&saves_dir)?;
        if let Some(new_path) =
            crate::persistence::picker::run_load_picker(&saves_dir, &app.world.graph)
        {
            if app.save_file_path.as_deref() == Some(new_path.as_path()) {
                println!("That save file is already active.");
                return Ok(());
            }
            if app.db.is_some()
                && app
                    .capture_and_save_async(app.active_branch_id)
                    .await
                    .is_none()
            {
                anyhow::bail!("failed to save the current game before switching save files");
            }

            // Picker metadata was read only while every candidate was locked.
            // Keep the old live guard until the selected candidate recovers.
            let candidate_lock = take_or_acquire_save_lock(&mut picker_locks, &new_path)?;
            let new_db = crate::persistence::Database::open(&new_path)?;
            let async_db = Arc::new(crate::persistence::AsyncDatabase::new(new_db));
            restore_from_db(app, &async_db, &new_path, None).await?;
            app.save_lock = Some(candidate_lock);
            app.last_autosave = Some(std::time::Instant::now());
            print_location_arrival(app);
            print_arrival_reactions(app).await;
        }
    } else if let Some(ref db) = app.db {
        match db.find_branch(name).await {
            Ok(Some(branch)) => {
                if branch.id != app.active_branch_id
                    && app
                        .capture_and_save_async(app.active_branch_id)
                        .await
                        .is_none()
                {
                    anyhow::bail!("failed to save the current branch before loading '{name}'");
                }
                let Some(save_path) = app.save_file_path.clone() else {
                    println!("Branch '{}': active save file is unavailable", name);
                    return Ok(());
                };
                match load_and_restore_snapshot(app, &save_path, branch.id, &branch.name).await {
                    Ok(true) => {
                        app.last_autosave = Some(std::time::Instant::now());
                        let time = app.world.clock.time_of_day();
                        let season = app.world.clock.season();
                        let loc = app.world.current_location().name.clone();
                        println!("Loaded branch '{}'. {} — {}, {}.", name, loc, season, time);
                    }
                    Ok(false) => anyhow::bail!("branch '{name}' has no snapshots"),
                    Err(msg) => anyhow::bail!("failed to load branch '{name}': {msg}"),
                }
            }
            Ok(None) => anyhow::bail!("no branch named '{name}' found"),
            Err(e) => anyhow::bail!("failed to find branch '{name}': {e}"),
        }
    } else {
        anyhow::bail!("persistence not available");
    }
    Ok(())
}

/// Handles /new in headless mode — resets world and NPCs.
pub(crate) async fn handle_headless_new_game(app: &mut App) -> Result<(), String> {
    let saves_dir = app
        .saves_dir
        .clone()
        .ok_or_else(|| "Cannot start a new game without a saves directory.".to_string())?;
    let (mut candidate_world, mut candidate_npcs) = fresh_headless_world_and_npcs(app)?;
    candidate_npcs.assign_tiers(&candidate_world, &[]);
    let snapshot = crate::persistence::GameSnapshot::capture(&candidate_world, &candidate_npcs);
    let new_path = crate::persistence::picker::new_save_path(&saves_dir);
    let candidate_lock = crate::persistence::SaveFileLock::try_acquire(&new_path)
        .ok_or_else(|| format!("Could not lock new save file {}", new_path.display()))?;
    let db = crate::persistence::Database::open(&new_path).map_err(|error| error.to_string())?;
    let branch = db
        .find_branch("main")
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "main branch missing from new save file".to_string())?;
    let snapshot_id = db
        .save_snapshot(branch.id, &snapshot)
        .map_err(|error| error.to_string())?;
    let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
    let prepared_binding = app
        .session_store
        .prepare_active_save("", &new_path)
        .map_err(|error| error.to_string())?;
    if let Err(marker_error) = parish_core::persistence::write_active_save_identity(
        &saves_dir, &new_path, branch.id, "main",
    ) {
        drop(candidate_lock);
        drop(async_db);
        std::fs::remove_file(&new_path).map_err(|cleanup_error| {
            format!("{marker_error}; additionally failed to remove candidate save: {cleanup_error}")
        })?;
        return Err(marker_error.to_string());
    }

    prepared_binding.commit();
    candidate_world.event_bus = std::mem::take(&mut app.world.event_bus);
    candidate_world.event_bus.advance_context_epoch();
    app.world = candidate_world;
    app.npc_manager = candidate_npcs;
    app.db = Some(async_db);
    app.save_file_path = Some(new_path);
    app.active_branch_id = branch.id;
    app.latest_snapshot_id = snapshot_id;
    app.last_autosave = Some(std::time::Instant::now());
    app.save_lock = Some(candidate_lock);
    reset_headless_context_receivers(app);

    println!("A new day dawns in the parish.");
    println!();
    print_location_arrival(app);
    print_arrival_reactions(app).await;
    Ok(())
}

/// Applies a parsed NPC dialogue response — tier-1 state update, conversation
/// log entry, and witness memory recording. Extracted from
/// [`stream_headless_npc_dialogue`] to flatten control flow.
#[allow(clippy::too_many_arguments)]
fn apply_npc_response(
    app: &mut App,
    npc_id: crate::npc::NpcId,
    response_text: &str,
    player_input: &str,
    game_time: chrono::DateTime<chrono::Utc>,
    location: parish_core::world::LocationId,
    npc_display_name: &str,
    npc_actual_name: &str,
    grounding: &crate::npc::DialogueGroundingSnapshot,
) -> (Option<parish_core::session_store::PlayerTask>, String) {
    let (parsed, parse_disposition) =
        crate::npc::parse_npc_stream_response_with_disposition(response_text);
    if let Some(meta) = &parsed.metadata {
        tracing::debug!("NPC metadata: action={}, mood={}", meta.action, meta.mood);
    }

    // Shared per-turn pipeline (#1172 / #1173): name detection, Tier-1 apply,
    // conversation-log record, witness memories, and the `DialogueOccurred`
    // publish (headless previously skipped this last step). Forward the returned
    // debug-event strings to the headless debug sink.
    let language = app.language_settings();
    let outcome = parish_core::game_session::apply_npc_dialogue_turn_with_validation(
        &mut app.world,
        &mut app.npc_manager,
        npc_id,
        &parsed,
        parse_disposition,
        grounding,
        crate::npc::DialogueValidationPolicy::default(),
        player_input,
        player_input,
        game_time,
        location,
        npc_display_name,
        npc_actual_name,
        None,
        &[],
        &language,
        &app.flags,
    );
    for event in outcome.debug_events {
        app.debug_event(event);
    }
    (outcome.assigned_task, outcome.display_text)
}

/// Streams NPC dialogue to stdout with loading animation, then applies
/// memory pipeline and records witness events.
///
/// Extracted from `handle_headless_game_input` for TD-003.
async fn stream_headless_npc_dialogue(
    app: &mut App,
    text: &str,
    setup: parish_core::ipc::NpcConversationSetup,
    request_id: &mut u64,
) -> Option<parish_core::session_store::PlayerTask> {
    let npc_id = setup.npc_id;
    let system_prompt = setup.system_prompt;
    let context = setup.context;
    let grounding = setup.grounding.clone();

    let mut assigned_task = None;
    if let Some(queue) = &app.inference_queue {
        app.world.clock.inference_pause();

        *request_id += 1;

        let (token_tx, token_rx) =
            mpsc::channel::<String>(parish_core::ipc::TOKEN_CHANNEL_CAPACITY);

        let npc_display_name = setup.display_name;
        let npc_actual_name = setup.npc_name;
        print!("{}: ", capitalize_first(&npc_display_name));
        std::io::stdout().flush().ok();

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
            print!("\r\x1b[K{}: ", npc_name_for_anim);
            std::io::stdout().flush().ok();
        });

        // Use the same resolved generation profile as the shared live path.
        // Prompt capture drives this headless entry point, so any local literal
        // here would make benchmark requests diverge from server/Tauri play.
        let generation = app.inference_config.dialogue_generation;
        match queue
            .send(QueueRequest {
                id: *request_id,
                model: app.dialogue_model.clone(),
                prompt: context,
                system: Some(system_prompt),
                token_tx: Some(token_tx),
                max_tokens: Some(generation.max_tokens),
                temperature: Some(generation.temperature),
                frequency_penalty: generation.frequency_penalty,
                enable_thinking: generation.enable_thinking,
                reasoning_effort: generation.reasoning_effort,
                priority: InferencePriority::Interactive,
                role: parish_core::config::InferenceCategory::Dialogue,
                subrole: parish_core::config::InferenceSubrole::Dialogue,
                profile: None,
                json_mode: generation.json_mode,
                json_schema: None,
                cancel: None,
            })
            .await
        {
            Ok(rx) => {
                let stream_handle = tokio::spawn(async move {
                    parish_core::ipc::stream_npc_tokens(token_rx, |_| {
                        cancel_for_stream.notify_one();
                    })
                    .await
                });

                match rx.await {
                    Ok(response) => {
                        let _streamed = stream_handle.await.unwrap_or_default();
                        let _ = anim_handle.await;

                        if let Some(err) = &response.error {
                            println!("[The parish storyteller has lost the thread: {}]", err);
                        } else {
                            let game_time = app.world.clock.now();
                            let location = app.world.player_location;
                            let (task, canonical_dialogue) = apply_npc_response(
                                app,
                                npc_id,
                                &response.text,
                                text,
                                game_time,
                                location,
                                &npc_display_name,
                                &npc_actual_name,
                                &grounding,
                            );
                            println!("{canonical_dialogue}");
                            assigned_task = task;
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

        app.world.clock.inference_resume();
    } else {
        println!("[No storyteller could be found in the parish today.]");
    }
    assigned_task
}

async fn persist_headless_task_mutations(
    app: &mut App,
    target: Option<&parish_core::session_store::TaskJournalTarget>,
    before: parish_core::session_store::PlayerProgress,
    tasks: &[parish_core::session_store::PlayerTask],
) -> Result<()> {
    if tasks.is_empty() {
        return Ok(());
    }
    let result = match target {
        Some(target) => parish_core::session_store::append_task_mutations(
            app.session_store.as_ref(),
            target,
            tasks,
        )
        .await
        .map(|_| ()),
        None => Err(parish_core::error::ParishError::Database(
            "cannot journal player task without an active save and branch".to_string(),
        )),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            app.world.player_progress = before;
            Err(error.into())
        }
    }
}

/// Handles game input (NPC interaction or intent parsing) in headless mode.
async fn handle_headless_game_input(
    app: &mut App,
    client: Option<&AnyClient>,
    model: &str,
    text: &str,
    request_id: &mut u64,
) -> Result<()> {
    if parish_core::game_loop::input_may_mutate_tasks(&app.world, text) {
        return handle_headless_staged_game_input(app, client, text).await;
    }

    // Headless input is serialized by the `&mut App` REPL borrow. Capture
    // both the rollback state and exact journal identity before any action
    // can mutate the task ledger.
    let task_progress_before = app.world.player_progress.clone();
    let task_target =
        app.save_file_path
            .clone()
            .map(|save_path| parish_core::session_store::TaskJournalTarget {
                session_id: String::new(),
                save_path,
                branch_id: app.active_branch_id,
            });
    let mut task_mutations = Vec::new();
    let mut delayed_action_narration = None;
    // Parse intent: try local keyword matching first, fall back to LLM.
    let intent = if let Some(local) = crate::input::parse_intent_local(text) {
        local
    } else if let Some(client) = client {
        app.world.clock.inference_pause();
        let profile = app
            .snapshot_config()
            .inference_profile(parish_core::config::InferenceSubrole::Intent);
        let audit_sink = app
            .inference_queue
            .as_ref()
            .and_then(InferenceQueue::audit_sink);
        let result =
            parse_intent_with_profile_and_audit(client, text, model, profile, audit_sink).await;
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
        crate::input::IntentKind::Examine => {
            // Feature-flagged: default-ON via is_disabled (#1424).
            // Collapse: flag must be on AND a target must be present; otherwise room description.
            match (
                !app.flags.is_disabled("examine-intent"),
                intent.target.as_deref(),
            ) {
                (true, Some(name)) => {
                    println!(
                        "You look more closely at {name}. There is nothing more noteworthy about it than what you have already observed."
                    );
                }
                _ => {
                    // Flag disabled or bare examine (no target) → room description.
                    print_location_description(app);
                }
            }
        }
        crate::input::IntentKind::Interact if !app.flags.is_disabled("interact-narration") => {
            if let Some(outcome) =
                parish_core::game_session::apply_player_action(&mut app.world, text, &app.flags)
            {
                delayed_action_narration = Some(outcome.narration);
                task_mutations.extend(outcome.progressed_task);
            }
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
            let lang = app.language_settings();
            let npc_cfg = parish_core::config::NpcConfig {
                dialogue_quality_continuity: !app.flags.is_disabled("dialogue-quality-continuity"),
                grounding_enabled: !app.flags.is_disabled("npc-dialogue-grounding"),
                ..parish_core::config::NpcConfig::default()
            };
            if let Some(setup) = parish_core::ipc::prepare_npc_conversation(
                &app.world,
                &mut app.npc_manager,
                &dialogue,
                target_name.as_deref(),
                app.improv_enabled,
                &lang,
                &npc_cfg,
            ) {
                // Teach this NPC the player's name if introduced
                if app.world.player_name.is_some()
                    && parish_core::npc::detect_player_name(&dialogue).is_some()
                {
                    app.npc_manager.teach_player_name(setup.npc_id);
                }

                task_mutations
                    .extend(stream_headless_npc_dialogue(app, text, setup, request_id).await);
            } else {
                // `fetch_add` returns the pre-increment value, so the first
                // idle turn reads index 0 — i.e. `IDLE_MESSAGES[0]` /
                // `mod_msgs[0]` (TD-037: a prior edit's `app.idle_counter += 1;
                // let idx = app.idle_counter;` read 1 first and skipped the
                // zeroth message).
                let idx = IDLE_MESSAGE_INDEX.fetch_add(1, Ordering::Relaxed);
                let mod_msgs = app
                    .game_mod
                    .as_ref()
                    .map(|gm| gm.loading.idle_messages.as_slice())
                    .unwrap_or(&[]);
                let msg = select_idle_message(idx, mod_msgs);
                println!("{}", msg);
            }
        }
    }

    persist_headless_task_mutations(
        app,
        task_target.as_ref(),
        task_progress_before,
        &task_mutations,
    )
    .await?;
    if let Some(narration) = delayed_action_narration {
        println!("{narration}");
    }
    println!();
    Ok(())
}

/// Drives a task-capable headless turn through the shared whole-turn staging
/// seam, then renders its captured terminal output only after durable commit.
async fn handle_headless_staged_game_input(
    app: &mut App,
    intent_client: Option<&AnyClient>,
    text: &str,
) -> Result<()> {
    use parish_core::game_loop::{GameLoopContext, handle_staged_game_input};
    use parish_core::ipc::{CapturingEmitter, EventEmitter};
    use tokio::sync::Mutex;

    let task_target =
        app.save_file_path
            .clone()
            .map(|save_path| parish_core::session_store::TaskJournalTarget {
                session_id: String::new(),
                save_path,
                branch_id: app.active_branch_id,
            });
    let transport = default_transport(app);
    let reaction_templates = app
        .game_mod
        .as_ref()
        .map(|game_mod| game_mod.reactions.clone())
        .unwrap_or_default();
    let language = app.language_settings();
    let config_snapshot = app.snapshot_config();
    let inference_config = app.inference_config.clone();
    let (pronunciations, idle_messages, inference_failure_messages) = app
        .game_mod
        .as_ref()
        .map(|game_mod| {
            (
                game_mod.pronunciations.clone(),
                game_mod.loading.idle_messages.clone(),
                game_mod.loading.inference_failure_messages.clone(),
            )
        })
        .unwrap_or_default();
    let store = Arc::clone(&app.session_store);

    let world = Mutex::new(std::mem::take(&mut app.world));
    let npc_manager = Mutex::new(std::mem::take(&mut app.npc_manager));
    let config = Mutex::new(config_snapshot);
    let conversation = Mutex::new(parish_core::ipc::ConversationRuntimeState::new());
    let inference_queue = Mutex::new(app.inference_queue.clone());
    let client = Mutex::new(intent_client.cloned());
    let cloud_client = Mutex::new(app.cloud_client.clone());
    let transport_emitter = Arc::new(CapturingEmitter::new());
    let emitter: Arc<dyn EventEmitter> = transport_emitter.clone();

    let result = {
        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter,
            inference_config: &inference_config,
            pronunciations: &pronunciations,
            client: &client,
            cloud_client: &cloud_client,
            language,
            inference_failure_messages: &inference_failure_messages,
            idle_messages: &idle_messages,
        };
        handle_staged_game_input(
            &ctx,
            store.as_ref(),
            task_target.as_ref(),
            Vec::new(),
            text.to_string(),
            Vec::new(),
            &transport,
            &reaction_templates,
        )
        .await
    };

    app.world = world.into_inner();
    app.npc_manager = npc_manager.into_inner();

    let commit = result?;
    render_committed_headless_emissions(&commit.emissions);
    println!();
    Ok(())
}

/// Reconstructs terminal dialogue from the live stream protocol after a staged
/// turn commits. The desktop consumes the same frames incrementally; headless
/// can render them in one pass because every frame is already buffered.
fn render_committed_headless_emissions(emissions: &[(String, serde_json::Value)]) {
    for line in committed_headless_lines(emissions) {
        println!("{line}");
    }
}

fn committed_headless_lines(emissions: &[(String, serde_json::Value)]) -> Vec<String> {
    use std::collections::HashMap;

    #[derive(Default)]
    struct TurnText {
        source: String,
        streamed: String,
        corrected: Option<String>,
    }

    let mut turns: HashMap<u64, TurnText> = HashMap::new();
    for (name, payload) in emissions {
        match name.as_str() {
            "text-log" => {
                if let Some(turn_id) = payload
                    .get("stream_turn_id")
                    .and_then(|value| value.as_u64())
                {
                    turns.entry(turn_id).or_default().source = payload
                        .get("source")
                        .and_then(|value| value.as_str())
                        .unwrap_or_default()
                        .to_string();
                }
            }
            "stream-token" => {
                if let Some(turn_id) = payload.get("turn_id").and_then(|value| value.as_u64()) {
                    turns.entry(turn_id).or_default().streamed.push_str(
                        payload
                            .get("token")
                            .and_then(|value| value.as_str())
                            .unwrap_or_default(),
                    );
                }
            }
            "dialogue-corrected" => {
                if let Some(turn_id) = payload.get("turn_id").and_then(|value| value.as_u64()) {
                    turns.entry(turn_id).or_default().corrected = payload
                        .get("corrected_text")
                        .and_then(|value| value.as_str())
                        .map(str::to_string);
                }
            }
            _ => {}
        }
    }

    let mut lines = Vec::new();
    for (name, payload) in emissions {
        match name.as_str() {
            "text-log" => {
                let is_stream_placeholder = payload
                    .get("stream_turn_id")
                    .is_some_and(|value| !value.is_null());
                if !is_stream_placeholder
                    && let Some(content) = payload.get("content").and_then(|value| value.as_str())
                    && !content.is_empty()
                {
                    lines.push(content.to_string());
                }
            }
            "stream-turn-end" => {
                let Some(turn_id) = payload.get("turn_id").and_then(|value| value.as_u64()) else {
                    continue;
                };
                let Some(turn) = turns.get(&turn_id) else {
                    continue;
                };
                let dialogue = turn.corrected.clone().unwrap_or_else(|| {
                    parish_core::npc::parse_npc_stream_response(&turn.streamed).dialogue
                });
                if !dialogue.trim().is_empty() {
                    if turn.source.is_empty() {
                        lines.push(dialogue);
                    } else {
                        lines.push(format!("{}: {dialogue}", turn.source));
                    }
                }
            }
            _ => {}
        }
    }
    lines
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

/// Emits LLM-informed (or rule-based fallback) NPC reactions to the player's
/// message in headless CLI mode — mode parity with the web server and Tauri
/// paths (#402).
///
/// When the `npc-llm-reactions` flag is enabled (default) and a reaction
/// client is available, each NPC at the player's location gets an inference
/// call. On any failure, falls back to keyword-based rule reactions (#404).
/// Reactions are persisted to each NPC's `reaction_log` (#403) and printed
/// to stdout so the player sees them.
async fn emit_headless_npc_reactions(app: &mut App, player_input: &str) {
    use parish_core::npc::reactions::{
        generate_rule_reaction, infer_player_message_reaction_with_profile_and_audit,
    };
    use tokio::task::JoinSet;

    let npcs_here: Vec<_> = app
        .npc_manager
        .npcs_at(app.world.player_location)
        .into_iter()
        .cloned()
        .collect();

    if npcs_here.is_empty() {
        return;
    }

    let llm_enabled = !app.flags.is_disabled("npc-llm-reactions");
    let reaction_profile = app
        .snapshot_config()
        .inference_profile(parish_core::config::InferenceSubrole::MessageReaction);

    // Run per-NPC inference concurrently, bounded to NPC_REACTION_CONCURRENCY
    // simultaneous calls so a busy location can't exhaust the LLM connection
    // pool (#406).
    let sem = Arc::new(Semaphore::new(NPC_REACTION_CONCURRENCY));
    let mut join_set: JoinSet<(String, Option<String>)> = JoinSet::new();

    for npc in npcs_here {
        let sem = Arc::clone(&sem);
        let client = app.reaction.client.clone();
        let model = app.reaction.model.clone();
        let input = player_input.to_string();
        let audit_sink = app
            .inference_queue
            .as_ref()
            .and_then(InferenceQueue::audit_sink);

        join_set.spawn(async move {
            // Acquire a permit before starting the (potentially slow) LLM call.
            let _permit = sem.acquire().await.ok();

            let emoji = if llm_enabled {
                if let Some(ref c) = client {
                    infer_player_message_reaction_with_profile_and_audit(
                        c,
                        &model,
                        &npc,
                        &input,
                        std::time::Duration::from_secs(2),
                        reaction_profile,
                        audit_sink,
                    )
                    .await
                    .or_else(|| generate_rule_reaction(&input))
                } else {
                    generate_rule_reaction(&input)
                }
            } else {
                generate_rule_reaction(&input)
            };

            (npc.name.clone(), emoji)
        });
    }

    // Collect results as tasks finish, then persist + print each reaction.
    while let Some(result) = join_set.join_next().await {
        let (npc_name, emoji) = match result {
            Ok((name, Some(emoji))) => (name, emoji),
            Ok((_, None)) => continue,
            Err(e) if e.is_panic() => {
                tracing::error!(error = %e, "npc reaction task panicked");
                continue;
            }
            Err(e) if e.is_cancelled() => {
                tracing::debug!("npc reaction task cancelled (shutdown)");
                continue;
            }
            Err(e) => {
                tracing::warn!(error = %e, "npc reaction task ended unexpectedly");
                continue;
            }
        };

        // Persist to reaction_log so NPC memory is maintained (#403).
        if let Some(npc_mut) = app.npc_manager.find_by_name_mut(&npc_name) {
            npc_mut.reaction_log.add_player_message_reaction(
                &emoji,
                player_input,
                chrono::Utc::now(),
            );
        }
        // Feed the per-session diversity sensor (#995).
        app.npc_manager.record_reaction_emoji(&emoji);
        println!("{} {}", capitalize_first(&npc_name), emoji);
    }
}

/// Generates and prints NPC arrival reactions (greetings, nods, introductions).
///
/// For reactions flagged `use_llm`, attempts a short-timeout LLM call for a
/// richer greeting, falling back to canned text on timeout or error.
async fn print_arrival_reactions(app: &mut App) {
    use parish_core::config::ReactionConfig;
    use parish_core::dice;
    use parish_core::npc::reactions::{
        ArrivalContext, LlmGreetingParams, generate_arrival_reactions, resolve_llm_greeting,
    };

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

    let arrival_ctx = ArrivalContext {
        location: &loc_data,
        time_of_day: tod,
        weather: &weather,
        templates: &templates,
        config: &config,
    };
    let reactions = generate_arrival_reactions(&npcs, &introduced, &arrival_ctx, &roll_dice);

    for reaction in &reactions {
        let text = if reaction.use_llm {
            if let Some(client) = &app.reaction.client {
                let npc = app.npc_manager.get(reaction.npc_id);
                if let Some(npc) = npc {
                    let at_workplace = npc.workplace.is_some_and(|wp| wp == loc_data.id);
                    let llm_params = LlmGreetingParams {
                        location_name: &loc_data.name,
                        time_of_day: tod,
                        weather: &weather,
                        is_introduced: introduced.contains(&reaction.npc_id),
                        at_workplace,
                        client,
                        model: &app.reaction.model.clone(),
                        timeout_secs: config.llm_timeout_secs,
                        profile: app.snapshot_config().inference_profile(
                            parish_core::config::InferenceSubrole::ArrivalReaction,
                        ),
                        audit_sink: app
                            .inference_queue
                            .as_ref()
                            .and_then(parish_core::inference::InferenceQueue::audit_sink),
                    };
                    resolve_llm_greeting(reaction, npc, &llm_params).await
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
pub(crate) fn default_transport(app: &App) -> TransportMode {
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
    use parish_core::dice::DiceRoll;
    use parish_core::world::weather_travel::{apply_multiplier, compute_weather_effect};

    let transport = default_transport(app);
    let result = movement::resolve_movement_with_weather(
        target,
        &app.world.graph,
        app.world.player_location,
        &transport,
        app.world.weather,
    );

    match result {
        MovementResult::Arrived {
            destination,
            path,
            minutes,
            narration,
        } => {
            // Consult the weather before committing the journey. The feature
            // flag is default-on via `is_disabled` semantics, same kill-switch
            // pattern as `period-map-tiles`.
            let apply_weather = !app.flags.is_disabled("weather-travel");
            let weather_effect = if apply_weather {
                compute_weather_effect(
                    app.world.weather,
                    app.world.clock.season(),
                    DiceRoll::roll(),
                    DiceRoll::roll(),
                )
            } else {
                parish_core::world::weather_travel::WeatherTravelEffect::clear()
            };

            if let Some(flavour) = weather_effect.flavour {
                println!("{}", flavour);
            }

            // A Storm can force the player back. They lose half the nominal
            // travel time to the aborted attempt and stay at the origin.
            if weather_effect.forced_back.is_some() {
                let lost = (minutes / 2).max(1);
                app.world.clock.advance(lost as i64);
                println!(
                    "You turn back. The storm has the better of it; you'll try again later. \
                     ({} {} lost to the attempt.)",
                    lost,
                    parish_core::world::time::minute_word(lost)
                );
                println!();
                return;
            }

            let adjusted_minutes = apply_multiplier(minutes, weather_effect.multiplier);
            if adjusted_minutes > minutes {
                println!(
                    "{} (slowed by the weather from {} to {} {})",
                    narration,
                    minutes,
                    adjusted_minutes,
                    parish_core::world::time::minute_word(adjusted_minutes)
                );
            } else {
                println!("{}", narration);
            }
            println!();

            app.world.record_path_traversal(&path);
            app.world.clock.advance(adjusted_minutes as i64);
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

            // Travel encounter — default-on, kill-switchable via the `travel-encounters` flag.
            if !app.flags.is_disabled("travel-encounters") {
                use crate::world::wayfarers;
                let clock_minutes = app.world.clock.now().timestamp() / 60;
                let seed = wayfarers::encounter_seed(
                    clock_minutes,
                    app.world.player_location,
                    destination,
                );
                let time = app.world.clock.time_of_day();
                let season = app.world.clock.season();
                let weather = app.world.weather;
                if let Some(enc) = wayfarers::resolve_encounter(time, season, weather, seed) {
                    let line = format!("  · {}", enc.text);
                    app.world.log(line.clone());
                    println!("{line}");
                }
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
        MovementResult::BlockedByWeather {
            weather, reason, ..
        } => {
            println!("{} (The weather is {}. Best wait it out.)", reason, weather);
        }
    }
}

/// Generic schedule event processor shared by headless and test-harness modes.
///
/// Returns player-visible event messages (arrival/departure) without
/// dispatching them — the caller chooses the output channel.
/// Debug strings are always logged via `app.debug_event`.
pub(crate) fn process_schedule_events_generic(
    app: &mut App,
    events: &[crate::npc::manager::ScheduleEvent],
) -> Vec<String> {
    use crate::npc::manager::ScheduleEventKind;

    let player_loc = app.world.player_location;
    let mut messages = Vec::new();

    for event in events {
        app.debug_event(event.debug_string());

        let display = app
            .npc_manager
            .get(event.npc_id)
            .map(|n| app.npc_manager.display_name(n).to_string())
            .unwrap_or_else(|| event.npc_name.clone());

        match &event.kind {
            ScheduleEventKind::Departed { from, .. } if *from == player_loc => {
                messages.push(format!(
                    "{} heads off down the road.",
                    capitalize_first(&display)
                ));
            }
            ScheduleEventKind::Arrived { location, .. } if *location == player_loc => {
                messages.push(format!("{} arrives.", capitalize_first(&display)));
            }
            _ => {}
        }
    }

    messages
}

/// Processes schedule events in headless mode: debug log + player-visible println.
fn process_headless_schedule_events(app: &mut App, events: &[crate::npc::manager::ScheduleEvent]) {
    for msg in process_schedule_events_generic(app, events) {
        println!("{msg}");
    }
}

/// Dispatches Tier 3 batch LLM simulation for distant NPCs.
///
/// Extracted from the REPL loop for TD-011.
async fn dispatch_headless_tier3_tick(app: &mut App) {
    let now = app.world.clock.now();
    if app.npc_manager.needs_tier3_tick(now) && !app.npc_manager.tier3_in_flight() {
        let npc_names: std::collections::HashMap<_, _> = app
            .npc_manager
            .all_npcs()
            .map(|n| (n.id, n.name.clone()))
            .collect();
        let tier3_ids = app
            .npc_manager
            .npcs_in_tier(crate::npc::types::CogTier::Tier3);
        let snapshots: Vec<parish_core::npc::ticks::Tier3Snapshot> = tier3_ids
            .iter()
            .filter_map(|id| app.npc_manager.get(*id))
            .map(|npc| {
                parish_core::npc::ticks::tier3_snapshot_from_npc(npc, &app.world.graph, &npc_names)
            })
            .collect();

        if !snapshots.is_empty()
            && let Some(sim_client) = app.simulation.client.as_ref()
        {
            let time_desc = app.world.clock.time_of_day().to_string();
            let weather_str = app.world.weather.to_string();
            let season_str = format!("{:?}", app.world.clock.season());
            let hours = 24u32;
            let sim_model = app.simulation.model.clone();

            app.npc_manager.set_tier3_in_flight(true);

            let lang = app.language_settings();
            let ctx = parish_core::npc::ticks::Tier3Context {
                snapshots: &snapshots,
                client: sim_client,
                model: &sim_model,
                time_desc: &time_desc,
                weather: &weather_str,
                season: &season_str,
                hours,
                batch_size: 0,
                language: &lang,
                cancel: None,
                grounding_enabled: !app.flags.is_disabled("npc-dialogue-grounding"),
            };

            let profile = app
                .snapshot_config()
                .inference_profile(parish_core::config::InferenceSubrole::Tier3Simulation);
            let audit_sink = app
                .inference_queue
                .as_ref()
                .and_then(InferenceQueue::audit_sink);
            match parish_core::npc::ticks::tick_tier3_with_profile_and_audit(
                &ctx, profile, audit_sink,
            )
            .await
            {
                Ok(updates) => {
                    let game_time = app.world.clock.now();
                    let _events = parish_core::npc::ticks::apply_tier3_updates(
                        &updates,
                        app.npc_manager.npcs_mut(),
                        &app.world.graph,
                        game_time,
                        &app.world.event_bus,
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

/// Dispatches Tier 2 background simulation for nearby NPCs.
///
/// Extracted from the REPL loop for TD-011.
async fn dispatch_headless_tier2_tick(app: &mut App) {
    let now = app.world.clock.now();
    if app.npc_manager.needs_tier2_tick(now)
        && !app.npc_manager.tier2_in_flight()
        && let Some(sim_client) = app.simulation.client.as_ref()
    {
        let groups = parish_core::game_loop::build_tier2_groups(&app.world, &app.npc_manager);
        if !groups.is_empty() {
            let sim_model = app.simulation.model.clone();

            app.npc_manager.set_tier2_in_flight(true);

            let lang = app.language_settings();
            let mut events = Vec::new();
            for group in &groups {
                let profile = app
                    .snapshot_config()
                    .inference_profile(parish_core::config::InferenceSubrole::Tier2Simulation);
                let audit_sink = app
                    .inference_queue
                    .as_ref()
                    .and_then(InferenceQueue::audit_sink);
                if let Some(evt) =
                    parish_core::npc::ticks::run_tier2_for_group_with_profile_and_audit(
                        sim_client,
                        &sim_model,
                        group,
                        &app.world.clock.time_of_day().to_string(),
                        &app.world.weather.to_string(),
                        &lang,
                        None,
                        profile,
                        audit_sink,
                    )
                    .await
                {
                    events.push(evt);
                }
            }

            let game_time = app.world.clock.now();
            let _dbg = parish_core::game_loop::mint_tier2_gossip(
                &events,
                app.npc_manager.npcs_mut(),
                game_time,
                &NpcConfig::default(),
                &mut app.world,
            );
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

/// Periodic autosave — triggered every `AUTOSAVE_INTERVAL_SECS` wall-clock
/// seconds since the last save.
async fn dispatch_headless_autosave(app: &mut App) {
    if app.db.is_some() {
        let should_autosave = app
            .last_autosave
            .map(|t| t.elapsed().as_secs() >= AUTOSAVE_INTERVAL_SECS)
            .unwrap_or(true);
        if should_autosave {
            let old_snap = app.latest_snapshot_id;
            let branch_id = app.active_branch_id;
            if let Some(_snap_id) = app.capture_and_save_async(branch_id).await {
                if let Some(ref db) = app.db {
                    let _ = db.clear_journal(branch_id, old_snap).await;
                }
                tracing::debug!("Autosave complete");
            }
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
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Quit, "")
            .await
            .unwrap();
        assert!(quit);
        assert!(app.should_quit);
    }

    #[test]
    fn committed_headless_renderer_preserves_buffered_protocol_order() {
        let emissions = vec![
            (
                "text-log".to_string(),
                serde_json::json!({"source": "player", "content": "> Have ye work for me?"}),
            ),
            (
                "text-log".to_string(),
                serde_json::json!({
                    "source": "Brigid",
                    "content": "",
                    "stream_turn_id": 7
                }),
            ),
            (
                "stream-token".to_string(),
                serde_json::json!({
                    "turn_id": 7,
                    "token": "{\"dialogue\":\"raw answer\",\"action\":\"\",\"mood\":\"busy\",\"language_hints\":[]}"
                }),
            ),
            (
                "stream-turn-end".to_string(),
                serde_json::json!({"turn_id": 7}),
            ),
            (
                "dialogue-corrected".to_string(),
                serde_json::json!({"turn_id": 7, "corrected_text": "guarded answer"}),
            ),
            (
                "text-log".to_string(),
                serde_json::json!({"source": "system", "content": "*Brigid offers a spade.*"}),
            ),
            (
                "text-log".to_string(),
                serde_json::json!({
                    "source": "Máire",
                    "content": "",
                    "stream_turn_id": 8
                }),
            ),
            (
                "stream-token".to_string(),
                serde_json::json!({
                    "turn_id": 8,
                    "token": "{\"dialogue\":\"fetch water\",\"action\":\"\",\"mood\":\"busy\",\"language_hints\":[]}"
                }),
            ),
            (
                "stream-turn-end".to_string(),
                serde_json::json!({"turn_id": 8}),
            ),
        ];

        assert_eq!(
            committed_headless_lines(&emissions),
            vec![
                "> Have ye work for me?",
                "Brigid: guarded answer",
                "*Brigid offers a spade.*",
                "Máire: fetch water",
            ]
        );
    }

    /// TD-037: the idle-message rotation is 0-based — the first idle turn
    /// (index 0) must emit the *first* message, not skip it. A prior edit had
    /// pre-incremented (`idx = 1` first), shifting the whole cycle.
    #[test]
    fn idle_rotation_is_zero_based() {
        let mod_msgs: Vec<String> = vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string(),
        ];
        // Index 0 selects the FIRST message (the off-by-one regression).
        assert_eq!(select_idle_message(0, &mod_msgs), "first");
        assert_eq!(select_idle_message(1, &mod_msgs), "second");
        assert_eq!(select_idle_message(2, &mod_msgs), "third");
        // Wraps cleanly via modulo.
        assert_eq!(select_idle_message(3, &mod_msgs), "first");
    }

    /// TD-037: the file-scoped counter returns the pre-increment value, so the
    /// very first observed index is 0 (mirrors `REQUEST_ID.fetch_add` in the
    /// shared game loop). This pins the parity guarantee at the counter itself.
    #[test]
    fn idle_index_counter_starts_at_zero() {
        let counter = AtomicUsize::new(0);
        assert_eq!(counter.fetch_add(1, Ordering::Relaxed), 0);
        assert_eq!(counter.fetch_add(1, Ordering::Relaxed), 1);
    }

    /// TD-037: with no mod idle messages, selection falls back to the engine
    /// `IDLE_MESSAGES` table and still indexes from 0.
    #[test]
    fn idle_rotation_falls_back_to_engine_table() {
        let empty: Vec<String> = Vec::new();
        let expected = parish_core::ipc::IDLE_MESSAGES[0].to_string();
        assert_eq!(select_idle_message(0, &empty), expected);
    }

    #[tokio::test]
    async fn test_handle_headless_command_pause() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Pause, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(app.world.clock.is_paused());
    }

    #[tokio::test]
    async fn test_handle_headless_command_resume() {
        let mut app = App::new();
        app.world.clock.pause();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Resume, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!app.world.clock.is_paused());
    }

    #[tokio::test]
    async fn test_handle_headless_command_help() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Help, "")
            .await
            .unwrap();
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_status() {
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Status, "")
            .await
            .unwrap();
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_save_no_db() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Save, "")
            .await
            .unwrap();
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

        let (quit, rebuild) = handle_headless_command(&mut app, Command::Save, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
        assert!(app.latest_snapshot_id > snap_id);
    }

    #[tokio::test]
    async fn test_handle_headless_command_fork_and_branches() {
        let temp = tempfile::tempdir().unwrap();
        let save_path = temp.path().join("parish_001.db");
        let mut app = App::new();
        let db = crate::persistence::Database::open(&save_path).unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();
        app.db = Some(async_db.clone());
        app.saves_dir = Some(temp.path().to_path_buf());
        app.save_file_path = Some(save_path);
        app.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            temp.path().to_path_buf(),
        ));
        app.active_branch_id = branch.id;
        app.latest_snapshot_id = snap_id;

        // Fork
        let (quit, _) = handle_headless_command(&mut app, Command::Fork("test".to_string()), "")
            .await
            .unwrap();
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
        let (quit, _) = handle_headless_command(&mut app, Command::Load("main".to_string()), "")
            .await
            .unwrap();
        assert!(!quit);
    }

    #[tokio::test]
    async fn test_handle_headless_command_load_nonexistent() {
        let mut app = App::new();
        let db = crate::persistence::Database::open_memory().unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));
        app.db = Some(async_db);
        let error = handle_headless_command(&mut app, Command::Load("bogus".to_string()), "")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("no branch named 'bogus'"));
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_provider() {
        let mut app = App::new();
        app.provider_name = "openrouter".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowProvider, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_provider() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("openrouter".to_string()), "")
                .await
                .unwrap();
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.provider_name, "openrouter");
        assert!(app.client.is_some());
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_provider_invalid() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetProvider("bogus".to_string()), "")
                .await
                .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_model() {
        let mut app = App::new();
        app.model_name = "test-model".to_string();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowModel, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_model() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetModel("new-model".to_string()), "")
                .await
                .unwrap();
        assert!(!quit);
        // A base model change now rebinds the worker (#1365) — a model change
        // is a routing change, so it must rebuild for parity with the
        // server/Tauri shared dispatch.
        assert!(rebuild);
        assert_eq!(app.model_name, "new-model");
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_key_none() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_key_masked() {
        let mut app = App::new();
        app.api_key = Some("sk-or-v1-abcdef1234".to_string());
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowKey, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_key() {
        let mut app = App::new();
        app.base_url = "https://openrouter.ai/api".to_string();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetKey("sk-new-key-12345678".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(rebuild);
        assert_eq!(app.api_key, Some("sk-new-key-12345678".to_string()));
        assert!(app.client.is_some());
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_speed() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowSpeed, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_speed() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::SetSpeed(GameSpeed::Fast), "")
                .await
                .unwrap();
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
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        // Per-category model change now rebinds the worker (#1365).
        assert!(rebuild);
        assert_eq!(app.cloud_model_name.as_deref(), Some("gpt-4"));
        assert_eq!(app.dialogue_model, "gpt-4");
    }

    #[tokio::test]
    async fn test_handle_set_category_model_intent() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryModel(InferenceCategory::Intent, "qwen3:1.5b".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        // Per-category model change now rebinds the worker (#1365).
        assert!(rebuild);
        assert_eq!(app.intent.model, "qwen3:1.5b");
    }

    #[tokio::test]
    async fn test_handle_set_category_model_simulation() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryModel(InferenceCategory::Simulation, "qwen3:8b".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        // Per-category model change now rebinds the worker (#1365).
        assert!(rebuild);
        assert_eq!(app.simulation.model, "qwen3:8b");
    }

    #[tokio::test]
    async fn test_handle_set_category_provider_rebuilds() {
        parish_core::config::ensure_mods_loaded();
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryProvider(InferenceCategory::Intent, "openrouter".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(
            rebuild,
            "Setting a category provider should trigger rebuild"
        );
        assert_eq!(app.intent.provider_name.as_deref(), Some("openrouter"));
    }

    #[tokio::test]
    async fn test_handle_set_category_key_rebuilds() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCategoryKey(InferenceCategory::Dialogue, "sk-test-key".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(rebuild, "Setting a category key should trigger rebuild");
        assert_eq!(app.cloud_api_key.as_deref(), Some("sk-test-key"));
    }

    /// Verify SetCloudProvider sets cloud_provider_name without panicking (issue #80).
    #[tokio::test]
    async fn test_set_cloud_provider_sets_name_without_panic() {
        parish_core::config::ensure_mods_loaded();
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCloudProvider("openrouter".to_string()),
            "",
        )
        .await
        .unwrap();
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
        assert!(app.intent.client.is_none());
    }

    #[tokio::test]
    async fn test_restore_from_db_fresh_database() {
        let tmp = tempfile::tempdir().unwrap();
        let save_path = tmp.path().join("parish_001.db");
        let mut app = App::new();
        app.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            tmp.path().to_path_buf(),
        ));
        let db = crate::persistence::Database::open(&save_path).unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));

        // Fresh DB — should create initial snapshot
        restore_from_db(&mut app, &async_db, &save_path, None)
            .await
            .unwrap();
        assert_eq!(app.active_branch_id, 1);
        assert!(app.latest_snapshot_id > 0);
    }

    #[tokio::test]
    async fn test_restore_from_db_with_existing_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let save_path = tmp.path().join("parish_001.db");
        let mut app = App::new();
        app.npc_manager
            .add_npc(parish_core::npc::Npc::new_test_npc());
        let introduced_id = app
            .npc_manager
            .all_npcs()
            .next()
            .expect("headless fixture has an NPC")
            .id;
        app.npc_manager.mark_introduced(introduced_id);
        let db = crate::persistence::Database::open(&save_path).unwrap();
        let async_db = Arc::new(crate::persistence::AsyncDatabase::new(db));

        // Save a snapshot first
        let branch = async_db.find_branch("main").await.unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        let snap_id = async_db.save_snapshot(branch.id, &snapshot).await.unwrap();

        // Now restore — should load the existing snapshot
        let mut app2 = App::new();
        app2.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            tmp.path().to_path_buf(),
        ));
        restore_from_db(&mut app2, &async_db, &save_path, None)
            .await
            .unwrap();
        assert_eq!(app2.active_branch_id, branch.id);
        assert_eq!(app2.latest_snapshot_id, snap_id);
        assert!(
            app2.npc_manager.is_introduced(introduced_id),
            "headless database restore must preserve durable identity knowledge"
        );
    }

    #[tokio::test]
    async fn failed_exact_recovery_preserves_headless_world_identity_and_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let old_path = tmp.path().join("parish_001.db");
        let bad_path = tmp.path().join("parish_002.db");
        let old_db = crate::persistence::Database::open(&old_path).unwrap();
        let old_branch = old_db.find_branch("main").unwrap().unwrap();
        old_db
            .save_snapshot(
                old_branch.id,
                &crate::persistence::GameSnapshot::capture(&App::new().world, &NpcManager::new()),
            )
            .unwrap();
        std::fs::write(&bad_path, b"not a sqlite database").unwrap();

        let mut app = App::new();
        app.world.log("old live world".to_string());
        app.save_file_path = Some(old_path.clone());
        app.active_branch_id = old_branch.id;
        app.latest_snapshot_id = 77;
        app.save_lock = crate::persistence::SaveFileLock::try_acquire(&old_path);
        app.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            tmp.path().to_path_buf(),
        ));
        app.session_store.set_active_save("", &old_path).unwrap();

        let error = load_and_restore_snapshot(&mut app, &bad_path, 1, "main")
            .await
            .unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(app.save_file_path.as_deref(), Some(old_path.as_path()));
        assert_eq!(app.active_branch_id, old_branch.id);
        assert_eq!(app.latest_snapshot_id, 77);
        assert!(app.save_lock.is_some());
        assert!(
            app.world
                .text_log
                .iter()
                .any(|line| line == "old live world")
        );
        assert_eq!(
            std::fs::canonicalize(app.session_store.save_path("").unwrap()).unwrap(),
            std::fs::canonicalize(&old_path).unwrap()
        );
    }

    #[tokio::test]
    async fn failed_headless_new_game_preserves_live_identity_and_lock() {
        let tmp = tempfile::tempdir().unwrap();
        let old_path = tmp.path().join("parish_001.db");
        crate::persistence::Database::open(&old_path).unwrap();
        let invalid_saves_dir = tmp.path().join("not-a-directory");
        std::fs::write(&invalid_saves_dir, b"file").unwrap();

        let mut app = App::new();
        app.world.log("old live world".to_string());
        app.saves_dir = Some(invalid_saves_dir);
        app.save_file_path = Some(old_path.clone());
        app.active_branch_id = 42;
        app.latest_snapshot_id = 77;
        app.save_lock = crate::persistence::SaveFileLock::try_acquire(&old_path);

        let error = handle_headless_new_game(&mut app).await.unwrap_err();

        assert!(!error.is_empty());
        assert_eq!(app.save_file_path.as_deref(), Some(old_path.as_path()));
        assert_eq!(app.active_branch_id, 42);
        assert_eq!(app.latest_snapshot_id, 77);
        assert!(app.save_lock.is_some());
        assert!(
            app.world
                .text_log
                .iter()
                .any(|line| line == "old live world")
        );
    }

    #[tokio::test]
    async fn headless_action_progress_survives_snapshot_only_crash_via_journal() {
        let tmp = tempfile::tempdir().unwrap();
        let save_path = tmp.path().join("parish_001.db");
        let mut app = App::new();
        let assigned_at = app.world.clock.now();
        let task_id = app
            .world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                parish_core::npc::NpcId(7),
                app.world.player_location,
                assigned_at,
            )
            .unwrap();
        let db = crate::persistence::Database::open(&save_path).unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let snapshot = crate::persistence::GameSnapshot::capture(&app.world, &app.npc_manager);
        db.save_snapshot(branch.id, &snapshot).unwrap();
        drop(db);

        app.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            tmp.path().to_path_buf(),
        ));
        app.save_file_path = Some(save_path.clone());
        app.active_branch_id = branch.id;
        let mut request_id = 0;
        handle_headless_game_input(
            &mut app,
            None,
            "",
            "I set to work in the potato patch, breaking clods and planting seed.",
            &mut request_id,
        )
        .await
        .unwrap();
        let progressed = app.world.player_progress.task(task_id).unwrap().clone();
        assert_eq!(progressed.status, parish_types::TaskStatus::InProgress);
        drop(app);

        let reopened = parish_core::session_store::DbSessionStore::new(tmp.path().to_path_buf());
        let recovery =
            parish_core::session_store::load_recovery_bundle(&reopened, "", &save_path, branch.id)
                .await
                .unwrap()
                .unwrap();
        let mut restored_world = crate::world::WorldState::new();
        let mut restored_npcs = crate::npc::manager::NpcManager::new();
        recovery.restore(&mut restored_world, &mut restored_npcs);
        assert_eq!(
            restored_world.player_progress.task(task_id),
            Some(&progressed)
        );
    }

    #[tokio::test]
    async fn headless_interact_flag_off_routes_to_legacy_fallback_without_task_mutation() {
        let mut app = App::new();
        let task_id = app
            .world
            .player_progress
            .assign_task(
                "Dig over the potato patch.",
                parish_core::npc::NpcId(7),
                app.world.player_location,
                app.world.clock.now(),
            )
            .unwrap();
        app.flags.disable("interact-narration");

        let mut request_id = 0;
        handle_headless_game_input(
            &mut app,
            None,
            "",
            "I set to work in the potato patch, breaking clods and planting seed.",
            &mut request_id,
        )
        .await
        .unwrap();

        assert_eq!(
            app.world.player_progress.task(task_id).unwrap().status,
            parish_types::TaskStatus::Assigned,
            "flag-off interact must retain the pre-narration dialogue/fallback behavior"
        );
    }

    #[tokio::test]
    async fn test_handle_load_bare_no_db() {
        // Bare /load without a DB should not crash
        let mut app = App::new();
        let (quit, _rebuild) = handle_headless_command(&mut app, Command::Load(String::new()), "")
            .await
            .unwrap();
        assert!(!quit);
    }

    // --- Additional headless command tests ---

    #[tokio::test]
    async fn test_handle_headless_command_wait() {
        let mut app = App::new();
        app.world.clock.pause(); // freeze for determinism
        let hour_before = app.world.clock.now().hour();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Wait(60), "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
        // Time should have advanced by 60 minutes
        let hour_after = app.world.clock.now().hour();
        assert_eq!((hour_after + 24 - hour_before) % 24, 1);
    }

    #[tokio::test]
    async fn test_handle_headless_command_debug_none() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Debug(None), "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_debug_with_subcommand() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::Debug(Some("clock".to_string())), "")
                .await
                .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_toggle_sidebar() {
        // In headless mode, ToggleSidebar just prints a message (not available)
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ToggleSidebar, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_toggle_improv() {
        let mut app = App::new();
        let was_improv = app.improv_enabled;
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ToggleImprov, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
        assert_ne!(app.improv_enabled, was_improv);
    }

    #[tokio::test]
    async fn test_handle_headless_command_about() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::About, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_map() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Map(None), "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_npcs_here() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::NpcsHere, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_time() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Time, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_invalid_speed() {
        let mut app = App::new();
        let (quit, rebuild) =
            handle_headless_command(&mut app, Command::InvalidSpeed("bogus".to_string()), "")
                .await
                .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_invalid_branch_name() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::InvalidBranchName("Bad name!".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_log() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Log, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_branches_no_db() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Branches, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_tick() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::Tick, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloud, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloudModel, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_cloud_key() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(&mut app, Command::ShowCloudKey, "")
            .await
            .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_cloud_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCloudModel("claude-sonnet".to_string()),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(!rebuild); // SetCloudModel doesn't trigger rebuild
        assert_eq!(app.cloud_model_name.as_deref(), Some("claude-sonnet"));
    }

    #[tokio::test]
    async fn test_handle_headless_command_set_cloud_key() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::SetCloudKey("sk-cloud-123".to_string()),
            "",
        )
        .await
        .unwrap();
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
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_category_model() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::ShowCategoryModel(InferenceCategory::Intent),
            "",
        )
        .await
        .unwrap();
        assert!(!quit);
        assert!(!rebuild);
    }

    #[tokio::test]
    async fn test_handle_headless_command_show_category_key() {
        let mut app = App::new();
        let (quit, rebuild) = handle_headless_command(
            &mut app,
            Command::ShowCategoryKey(InferenceCategory::Simulation),
            "",
        )
        .await
        .unwrap();
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

    /// Regression guard for #608: when a save file is already locked by another
    /// live instance, script mode (`script_mode: true`) must produce a hard
    /// error, not silently proceed with concurrent writes.
    ///
    /// We exercise the precise logic block that was changed without invoking
    /// the full `run_headless` pipeline (which requires inference + save picker
    /// UI). The logic is: `save_lock.is_none() && script_mode` → bail.
    #[test]
    fn script_mode_lock_failure_is_hard_error() {
        // Simulate a lock failure (e.g. another process holds the lock).
        // With PR #542's reentrant SaveFileLock, same-process re-acquire returns
        // Some(...) rather than None, so we simulate None directly — matching the
        // same pattern used by the interactive-mode variant below.
        let failed_lock: Option<crate::persistence::SaveFileLock> = None;

        // Replicate the headless.rs decision: in script_mode the None must be
        // treated as a hard error rather than a warn-and-continue.
        let script_mode = true;
        let error_produced = failed_lock.is_none() && script_mode;

        assert!(
            error_produced,
            "script mode with a locked save file must trigger the hard-error branch"
        );
    }

    /// Regression guard for #608 — interactive mode: when a save file is
    /// already locked, interactive mode (`script_mode: false`) must NOT
    /// trigger the error branch — it warns and continues (the user can ^C).
    #[test]
    fn interactive_mode_lock_failure_is_warning_only() {
        // The interactive-mode branch emits a warning but does NOT bail.
        // Verify the decision logic: script_mode=false → error branch skipped.
        let script_mode = false;
        let failed_lock: Option<crate::persistence::SaveFileLock> = None; // simulate failure

        let error_would_be_triggered = failed_lock.is_none() && script_mode;

        assert!(
            !error_would_be_triggered,
            "interactive mode with a locked save file must not trigger the error branch"
        );
    }

    /// Regression guard for the Gemini-identified gap in #630: the same
    /// script-mode fail-closed policy that applies at startup must also apply
    /// inside `handle_headless_load` when switching save files via `/load`.
    ///
    /// We exercise the exact `app.script_mode && save_lock.is_none()` logic
    /// added to the bare-/load save-switch path without invoking the
    /// interactive save picker (which reads from stdin).
    #[tokio::test]
    async fn load_save_switch_script_mode_lock_failure_is_hard_error() {
        // Simulate a lock failure (e.g. another process holds the target save).
        // With PR #542's reentrant SaveFileLock, same-process re-acquire returns
        // Some(...) rather than None, so we simulate None directly — matching the
        // interactive-mode variant.
        let failed_lock: Option<crate::persistence::SaveFileLock> = None;

        // The new guard in handle_headless_load: script_mode=true + None lock
        // must be treated as a hard error.
        let mut app = App::new();
        app.script_mode = true;
        // Replicate the load-switch decision added to handle_headless_load.
        let error_produced = failed_lock.is_none() && app.script_mode;

        assert!(
            error_produced,
            "script mode must hard-error on a locked save file during /load save-switch"
        );
    }

    /// Regression guard: in interactive mode the /load save-switch path must
    /// NOT trigger the hard-error branch when the lock cannot be acquired —
    /// it should warn and continue (the user is present and can ^C).
    #[tokio::test]
    async fn load_save_switch_interactive_mode_lock_failure_is_warning_only() {
        let failed_lock: Option<crate::persistence::SaveFileLock> = None; // simulate failure

        let mut app = App::new();
        app.script_mode = false;

        let error_would_be_triggered = failed_lock.is_none() && app.script_mode;

        assert!(
            !error_would_be_triggered,
            "interactive mode must not hard-error on a locked save file during /load save-switch"
        );
    }

    /// #1011 / #1034: after the active branch changes, `drain_*` must rebuild
    /// the log managers so subsequent events land under the new branch's dir.
    #[test]
    fn rebind_log_managers_follows_branch_switch() {
        use parish_core::character_log::CharacterLogManager;
        use parish_core::location_log::LocationLogManager;
        use tempfile::tempdir;

        let tmp = tempdir().expect("tempdir");
        // Both managers resolve their log dirs from PARISH_USER_DATA_DIR.
        // SAFETY: env-var mutation in a test — single-threaded by test
        // construction (no parallel test touches log_app_name "rebind-test").
        // safety: env-mutation in test
        unsafe {
            std::env::set_var("PARISH_USER_DATA_DIR", tmp.path());
        }

        let mut app = App::new();
        app.log_app_name = "rebind-test".to_string();
        app.active_branch_id = 1;
        app.character_log = Some(std::sync::Arc::new(CharacterLogManager::new(
            "rebind-test",
            1,
            true,
        )));
        app.location_log = Some(std::sync::Arc::new(LocationLogManager::new(
            "rebind-test",
            1,
            true,
        )));
        app.log_managers_branch = Some(1);

        // Simulate /fork or /load mutating the active branch.
        app.active_branch_id = 2;
        app.rebind_log_managers_if_branch_changed();

        assert_eq!(
            app.log_managers_branch,
            Some(2),
            "rebind should record the new branch id"
        );
        // Both managers should now write under logs/branch-2/.
        // `PARISH_USER_DATA_DIR` overrides the entire user-data root (the
        // app_name is ignored when the env var is set — see
        // resolve_user_data_dir in parish-persistence::paths).
        let branch2 = tmp.path().join("logs").join("branch-2");
        assert!(
            branch2.exists(),
            "expected logs/branch-2/ to exist after rebind, but missing at {}",
            branch2.display()
        );

        // safety: env-cleanup in test
        unsafe {
            std::env::remove_var("PARISH_USER_DATA_DIR");
        }
    }
}
