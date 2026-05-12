//! Helpers extracted from [`crate::run`]'s `.setup()` closure (TD-003).
//!
//! Each function here corresponds to one phase of Tauri startup:
//! provider bootstrap, persistence init, screenshot mode, and the
//! background tick loops (event-bus fan-in, world tick, inactivity,
//! debug snapshot, autosave). They mirror the decomposition pattern
//! used by `parish-server::session::spawn_session_ticks` so the two
//! runtimes are easy to compare side-by-side.
//!
//! All functions take `&AppHandle` / `&Arc<AppState>` and emit via the
//! existing `tauri::Emitter` impl on the handle. Behaviour is byte-for-byte
//! identical to the inline closure that lived at lib.rs:898-1900.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use parish_core::config::{InferenceCategory, ProviderConfig};
use parish_core::debug_snapshot::{DebugEvent, InferenceDebug};
use parish_core::inference::{AnyClient, InferenceQueue, spawn_inference_worker};

use crate::{
    AUTOSAVE_INTERVAL_SECS, AppState, DEBUG_EVENT_CAPACITY, TauriProgress, ThemePalette,
    bootstrap_provider, dispatch_screenshot, events, record_setup_done,
};

// ── Screenshot mode ──────────────────────────────────────────────────────────

/// Spawns the screenshot-capture loop. Captures the UI at four times of day
/// and exits the app. No background ticks are started in this mode.
pub(crate) fn init_screenshot_mode(handle: AppHandle, state: Arc<AppState>, dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        // Give the WebView time to fully load the frontend.
        // In Xvfb + WebKit2 software rendering the JS bundle takes
        // ~15-20 s to parse, JIT, and complete the initial IPC round-trip
        // before onMount data is rendered into the DOM.
        tokio::time::sleep(Duration::from_secs(20)).await;

        // Emit the configured theme once so the frontend has a palette
        // painted before the first capture.
        {
            let palette = state.theme_palette.clone();
            let _ = handle.emit(events::EVENT_THEME_UPDATE, palette);
        }
        tokio::time::sleep(Duration::from_secs(3)).await;

        let times: &[(&str, u32)] = &[("morning", 7), ("midday", 12), ("dusk", 18), ("night", 22)];

        if let Err(e) = std::fs::create_dir_all(&dir) {
            tracing::warn!(path = %dir.display(), error = %e, "failed to create screenshot dir");
        }

        for (name, target_hour) in times {
            // Advance clock to target hour
            {
                use chrono::Timelike;
                let mut world = state.world.lock().await;
                let current_hour = world.clock.now().hour() as i64;
                let delta = ((*target_hour as i64) - current_hour).rem_euclid(24) * 60;
                world.clock.advance(delta);
            }

            // Wait for Svelte to re-render and WebKit to commit the frame
            tokio::time::sleep(Duration::from_secs(5)).await;

            // GDK must be called from the GTK main thread; dispatch and await.
            let path = dir.join(format!("gui-{}.png", name));
            if let Err(e) = dispatch_screenshot(path).await {
                tracing::error!(name = %name, error = %e, "screenshot capture failed");
            }
        }

        println!("screenshot: all done, exiting");
        handle.exit(0);
    });
}

// ── Provider bootstrap ───────────────────────────────────────────────────────

/// True iff the user has never completed BYOK onboarding AND nothing else has
/// already pinned a provider.
///
/// "Already pinned" means: a `--provider` CLI flag, `PARISH_PROVIDER`, a
/// standard provider key env var (`ANTHROPIC_API_KEY` etc.), a `parish.toml`
/// at the project root, or a saved `parish.toml` in the user-config dir.
///
/// `provider_config` carries whatever `provider_config_from_env` already
/// resolved — if it's anything other than the defaulted Simulator, the user
/// has explicit intent and we should skip the wizard.
/// First-run outcome computed from platform + provider config.
///
/// Drives the SetupOverlay fork: macOS hosts with sufficient unified
/// memory see a local-inference recommendation, smaller Macs see a
/// limited-quality local option alongside BYOK, and everything else
/// falls straight to BYOK (Linux defaults to Ollama and so never reaches
/// the fork in practice — the `LocalUnavailable` variant is reserved
/// for the rare case where no provider is wired and we're not on Mac).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnboardingChoice {
    /// Provider already pinned by env / CLI / saved config — skip the wizard.
    Configured,
    /// macOS Apple Silicon with ≥ 16 GB unified memory. Show local-recommended
    /// card; allow BYOK as a side option.
    LocalRecommended,
    /// macOS Apple Silicon below 16 GB. Show small-slot local + BYOK; warn on
    /// degraded quality.
    LocalLowMem,
    /// Non-Mac (or local-inference unavailable on this host) — BYOK only.
    LocalUnavailable,
}

impl OnboardingChoice {
    /// True when the wizard should render any kind of fork UI.
    pub fn needs_user_choice(self) -> bool {
        !matches!(self, OnboardingChoice::Configured)
    }
}

/// Inputs to [`resolve_onboarding_choice`]. Exposed as a struct so the
/// wrapper can read OS-y bits (env vars, user-config dir, sysctl) once
/// and then dispatch into the pure decision function — which keeps the
/// platform-fork logic unit-testable without needing a real AppState.
pub(crate) struct OnboardingInputs {
    /// User-config sentinel: a prior successful onboarding wrote this.
    pub onboarding_complete: bool,
    /// `PARISH_PROVIDER` env var was set (non-empty).
    pub parish_provider_env_set: bool,
    /// The provider's standard API-key env var (`ANTHROPIC_API_KEY`,
    /// `OPENAI_API_KEY`, …) was set non-empty.
    pub api_key_env_set: bool,
    /// Provider already picked explicitly (anything non-Simulator).
    pub explicit_provider_set: bool,
    /// `cfg!(target_os = "macos")` evaluated at call-site.
    pub is_macos: bool,
    /// What [`Provider::recommended_for_platform`] returned. Only
    /// consulted on macOS.
    pub recommended: parish_core::config::Provider,
}

/// Pure decision function for the first-run flow.
///
/// "Configured" means any of: prior-onboarding sentinel, `PARISH_PROVIDER`,
/// a provider API-key env var, or an explicit non-Simulator provider.
/// Otherwise, the variant is informed by `recommended`:
///
/// - macOS + recommended=VllmMlx → [`OnboardingChoice::LocalRecommended`]
/// - macOS + recommended=Simulator → [`OnboardingChoice::LocalLowMem`]
///   (the platform-recommender returns Simulator for Mac < 16 GB)
/// - everywhere else → [`OnboardingChoice::LocalUnavailable`]
pub(crate) fn resolve_onboarding_choice(inputs: OnboardingInputs) -> OnboardingChoice {
    use parish_core::config::Provider;

    if inputs.onboarding_complete
        || inputs.parish_provider_env_set
        || inputs.api_key_env_set
        || inputs.explicit_provider_set
    {
        return OnboardingChoice::Configured;
    }

    if inputs.is_macos {
        match inputs.recommended {
            Provider::VllmMlx => OnboardingChoice::LocalRecommended,
            Provider::Simulator => OnboardingChoice::LocalLowMem,
            _ => OnboardingChoice::LocalUnavailable,
        }
    } else {
        OnboardingChoice::LocalUnavailable
    }
}

/// Resolves the first-run flow for the current host.
///
/// Reads the OS-y inputs (user-config dir sentinel, env vars, sysctl
/// memory probe) and dispatches into [`resolve_onboarding_choice`].
pub(crate) fn onboarding_choice_for_platform(
    state: &Arc<AppState>,
    provider_config: &ProviderConfig,
) -> OnboardingChoice {
    use parish_core::config::Provider;

    let api_key_env_set = provider_config
        .provider
        .api_key_env_var()
        .and_then(|v| std::env::var(v).ok())
        .filter(|s| !s.is_empty())
        .is_some();

    resolve_onboarding_choice(OnboardingInputs {
        onboarding_complete: parish_core::config::user_config::onboarding_complete(
            &state.user_config_dir,
        ),
        parish_provider_env_set: std::env::var("PARISH_PROVIDER")
            .ok()
            .filter(|s| !s.is_empty())
            .is_some(),
        api_key_env_set,
        explicit_provider_set: !matches!(provider_config.provider, Provider::Simulator),
        is_macos: cfg!(target_os = "macos"),
        recommended: Provider::recommended_for_platform(),
    })
}

/// Runs the inference-provider bootstrap (Ollama install/start, model pull,
/// or remote-client construction) inside the async setup spawn, so the Tauri
/// window is open and can receive setup-status events while bootstrap runs.
///
/// Returns `false` when bootstrap fails — the caller should bail out of the
/// async setup spawn so background ticks are not started against an unconfigured
/// provider. The error is reported through the SetupStatus / setup-done events
/// before this returns.
pub(crate) async fn bootstrap_inference_provider(
    handle: &AppHandle,
    state: &Arc<AppState>,
    provider_config: &ProviderConfig,
    inference_config: &parish_core::config::InferenceConfig,
) -> bool {
    // BYOK gate: if the user has never picked a provider AND no PARISH_/CLI
    // override is in effect AND no standard env-var key is set, render the
    // onboarding fork instead of running the (Ollama-shaped) bootstrap path.
    //
    // AGENTS.md rule #6 — gated by `local-inference-onboarding`. Default-on;
    // explicit-disable falls back to the legacy Ollama-shaped bootstrap so
    // operators can ship a build that suppresses the wizard without code
    // changes (e.g. when running the engine pointed at a managed server).
    let onboarding_enabled = {
        let cfg = state.config.lock().await;
        !cfg.flags.is_disabled("local-inference-onboarding")
    };
    let choice = onboarding_choice_for_platform(state, provider_config);
    if onboarding_enabled && choice.needs_user_choice() {
        let progress = TauriProgress {
            app: handle.clone(),
            state: Arc::clone(state),
        };
        // Persist on the snapshot so SetupOverlay can render the right
        // fork even if it mounts after the event fires (race avoidance
        // — the EVENT_SETUP_NEEDS_ONBOARDING listener might not be
        // installed before the gate fires on a slow first frontend
        // mount). The choice variant tells the UI whether to show the
        // local-recommended card, the low-mem fork, or pure BYOK.
        progress.with_setup_status(|status| status.record_onboarding_choice(choice));
        let _ = handle.emit(
            events::EVENT_SETUP_NEEDS_ONBOARDING,
            events::SetupStatusPayload {
                message: "Awaiting provider choice".to_string(),
            },
        );
        // Bail out of bootstrap; the user's set_provider_config or
        // start_local_inference_setup call will populate the worker and
        // emit a fresh setup-done.
        return false;
    }

    let progress = TauriProgress {
        app: handle.clone(),
        state: Arc::clone(state),
    };
    progress
        .with_setup_status(|status| status.record_status("Starting inference provider setup..."));
    tracing::info!("Starting inference provider setup...");
    let _ = handle.emit(
        events::EVENT_SETUP_STATUS,
        events::SetupStatusPayload {
            message: "Starting inference provider setup...".to_string(),
        },
    );
    let extra_vllm_slots = {
        let cfg = state.config.lock().await;
        cfg.vllm_mlx_extra_slots()
    };
    match bootstrap_provider(
        provider_config,
        &extra_vllm_slots,
        inference_config,
        &progress,
    )
    .await
    {
        Ok((client, model_name, runtime_processes)) => {
            *state.client.lock().await = client;
            *state.runtime_processes.lock().await = runtime_processes;
            {
                let mut config = state.config.lock().await;
                if matches!(
                    provider_config.provider,
                    parish_core::config::Provider::Ollama
                ) {
                    // Auto-setup pulled exactly one model. Pin it across
                    // all four per-category slots so every role uses the
                    // model that is on disk, instead of the static qwen3
                    // preset list (which assumes models the user has not
                    // pulled).
                    config.pin_setup_model(model_name);
                } else {
                    config.model_name = model_name;
                }
                // No-op for Ollama after `pin_setup_model` filled every slot;
                // for cloud providers fills per-role tier mapping
                // (Opus/Sonnet/Haiku).
                config.fill_missing_models_from_presets();
            }
            record_setup_done(state, true, String::new());
            let _ = handle.emit(
                events::EVENT_SETUP_DONE,
                events::SetupDonePayload {
                    success: true,
                    error: String::new(),
                },
            );
            true
        }
        Err(e) => {
            let error = e.to_string();
            record_setup_done(state, false, error.clone());
            tracing::error!("Failed to initialise inference provider: {}", error);
            let _ = handle.emit(
                events::EVENT_SETUP_DONE,
                events::SetupDonePayload {
                    success: false,
                    error,
                },
            );
            // Do not call process::exit — leave the window open so
            // the user can read the error in the setup overlay.
            false
        }
    }
}

// ── Inference queue ──────────────────────────────────────────────────────────

/// Initialises the priority inference queue and worker handle on `AppState`.
/// Must run after [`bootstrap_inference_provider`] succeeds (or after the
/// `simulator` provider is selected) so a client is available.
pub(crate) async fn init_inference_queue(state: &Arc<AppState>) {
    let provider_name = {
        let config = state.config.lock().await;
        config.provider_name.clone()
    };
    let any_client: Option<AnyClient> = if provider_name == "simulator" {
        Some(AnyClient::simulator())
    } else {
        let client_guard = state.client.lock().await;
        client_guard.as_ref().cloned()
    };
    let Some(ac) = any_client else {
        return;
    };

    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        ac,
        interactive_rx,
        background_rx,
        batch_rx,
        state.inference_log.clone(),
        state.inference_config.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
    *state.inference_queue.lock().await = Some(queue);
    *state.worker_handle.lock().await = Some(worker);
}

// ── Persistence: auto-load or create save file ──────────────────────────────

/// Either loads the most-recent unlocked save (and acquires its lock), creates
/// a brand-new save file if none are present, or emits `EVENT_SAVE_PICKER` when
/// every save on disk is locked by another instance.
pub(crate) async fn init_persistence(handle: &AppHandle, state: &Arc<AppState>) {
    use parish_core::persistence::Database;
    use parish_core::persistence::SaveFileLock;
    use parish_core::persistence::picker::{discover_saves, new_save_path};
    use parish_core::persistence::snapshot::GameSnapshot;

    let saves_dir = state.saves_dir.clone();

    let world = state.world.lock().await;
    let saves = discover_saves(&saves_dir, &world.graph);
    drop(world);

    // Find the most recent unlocked save (iterate in reverse).
    let unlocked_save = saves.iter().rev().find(|s| !s.locked);

    if let Some(save) = unlocked_save {
        // Acquire the advisory lock before loading.
        let lock = SaveFileLock::try_acquire(&save.path);
        if lock.is_some() {
            *state.save_lock.lock().await = lock;
        }

        // Load the most recent unlocked save file
        match Database::open(&save.path) {
            Ok(db) => {
                // Find the "main" branch or first branch
                let branch = db
                    .find_branch("main")
                    .ok()
                    .flatten()
                    .or_else(|| db.list_branches().ok().and_then(|b| b.into_iter().next()));

                if let Some(branch) = branch {
                    if let Ok(Some((_snap_id, snapshot))) = db.load_latest_snapshot(branch.id) {
                        let mut world = state.world.lock().await;
                        let mut npc_mgr = state.npc_manager.lock().await;
                        snapshot.restore(&mut world, &mut npc_mgr);
                        npc_mgr.assign_tiers(&world, &[]);
                        drop(npc_mgr);
                        drop(world);

                        *state.save_path.lock().await = Some(save.path.clone());
                        *state.current_branch_id.lock().await = Some(branch.id);
                        *state.current_branch_name.lock().await = Some(branch.name.clone());
                        tracing::info!("Restored from {} (branch: {})", save.filename, branch.name);
                    } else {
                        // Save file exists but no snapshots — save initial state
                        let world = state.world.lock().await;
                        let npc_mgr = state.npc_manager.lock().await;
                        let snap = GameSnapshot::capture(&world, &npc_mgr);
                        drop(npc_mgr);
                        drop(world);
                        let _ = db.save_snapshot(branch.id, &snap);

                        *state.save_path.lock().await = Some(save.path.clone());
                        *state.current_branch_id.lock().await = Some(branch.id);
                        *state.current_branch_name.lock().await = Some(branch.name);
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to open save file {}: {}", save.filename, e);
            }
        }
    } else if saves.is_empty() {
        // No saves exist — create a new save file
        let path = new_save_path(&saves_dir);
        let lock = SaveFileLock::try_acquire(&path);
        if lock.is_some() {
            *state.save_lock.lock().await = lock;
        }
        match Database::open(&path) {
            Ok(db) => {
                if let Ok(Some(branch)) = db.find_branch("main") {
                    let world = state.world.lock().await;
                    let npc_mgr = state.npc_manager.lock().await;
                    let snap = GameSnapshot::capture(&world, &npc_mgr);
                    drop(npc_mgr);
                    drop(world);
                    let _ = db.save_snapshot(branch.id, &snap);

                    *state.save_path.lock().await = Some(path);
                    *state.current_branch_id.lock().await = Some(branch.id);
                    *state.current_branch_name.lock().await = Some("main".to_string());
                    tracing::info!("Created new save file");
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create save file: {}", e);
            }
        }
    } else {
        // All saves are locked by other instances.
        // Show the save picker so the user can choose or create a new ledger.
        tracing::info!(
            "All {} save file(s) are locked by other instances — opening save picker",
            saves.len()
        );
        let _ = handle.emit(events::EVENT_SAVE_PICKER, ());
    }
}

// ── Background ticks ────────────────────────────────────────────────────────

/// Subscribes to `world.event_bus` and buffers the last `DEBUG_EVENT_CAPACITY`
/// events into `state.game_events` for the debug panel.
///
/// `async` because subscribing requires locking `state.world`.
pub(crate) async fn spawn_event_bus_fanin(state: &Arc<AppState>) {
    let state_events = Arc::clone(state);
    let token_events = state.shutdown_token.clone();
    let mut rx = {
        let world = state_events.world.lock().await;
        world.event_bus.subscribe()
    };
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token_events.cancelled() => break,
                result = rx.recv() => {
                    match result {
                        Ok(evt) => {
                            let mut buf = state_events.game_events.lock().await;
                            if buf.len() >= DEBUG_EVENT_CAPACITY {
                                buf.pop_front();
                            }
                            buf.push_back(evt);
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });
}

/// Main world tick: emits a fresh world snapshot, ticks weather, runs Tier 4
/// rules, dispatches Tier 3 batch LLM, dispatches Tier 2 background LLM,
/// propagates gossip, and increments the tick generation counter.
///
/// Lock ordering: `world` -> `npc_manager` -> `debug_events`. Both `world` and
/// `npc_manager` are acquired once at the top of each iteration and held
/// through the entire body to avoid any window where a command handler could
/// sneak in between them and race the tick (see the AppState lock-ordering
/// contract).
pub(crate) fn spawn_world_tick(handle: AppHandle, state: Arc<AppState>) {
    let token = state.shutdown_token.clone();
    tokio::spawn(async move {
        let mut last_palette: Option<parish_palette::RawPalette> = None;
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }

            let mut world = state.world.lock().await;
            let mut npc_mgr = state.npc_manager.lock().await;

            // Emit a fresh world snapshot to the frontend.
            {
                let transport = state.transport.default_mode();
                let snapshot = crate::commands::get_world_snapshot_inner(
                    &world,
                    transport,
                    Some(&npc_mgr),
                    &state.pronunciations,
                );
                let _ = handle.emit(events::EVENT_WORLD_UPDATE, snapshot);
                // Emit current time-of-day palette
                {
                    use chrono::Timelike;
                    use parish_palette::compute_palette;
                    let now = world.clock.now();
                    let raw = compute_palette(now.hour(), now.minute());
                    if last_palette != Some(raw) {
                        let _ = handle.emit(events::EVENT_THEME_UPDATE, ThemePalette::from(raw));
                        last_palette = Some(raw);
                    }
                }
            }
            {
                // Tick weather engine
                let season = world.clock.season();
                let now = world.clock.now();
                // Scope thread_rng tightly so it is dropped before any await.
                let new_weather_opt = {
                    let mut rng = rand::rng();
                    world.weather_engine.tick(now, season, &mut rng)
                };
                {
                    if let Some(new_weather) = new_weather_opt {
                        let old = world.weather;
                        world.weather = new_weather;
                        world.event_bus.publish(
                            parish_core::world::events::GameEvent::WeatherChanged {
                                new_weather: new_weather.to_string(),
                                timestamp: world.clock.now(),
                            },
                        );
                        tracing::info!(old = %old, new = %new_weather, "Weather changed");
                        // Emit weather debug event
                        let mut debug_events = state.debug_events.lock().await;
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "weather".to_string(),
                            message: format!("Weather: {} → {}", old, new_weather),
                        });
                    }
                }

                let schedule_events =
                    npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                let tier_transitions = npc_mgr.assign_tiers(&world, &[]);

                // Banshee tick — herald and finalise doomed NPCs.
                // Default-on; kill-switched by the `banshee` feature flag.
                let banshee_enabled = {
                    let cfg = state.config.lock().await;
                    !cfg.flags.is_disabled("banshee")
                };
                let banshee_report = if banshee_enabled {
                    let world_ref = &mut *world;
                    npc_mgr.tick_banshee(
                        &world_ref.clock,
                        &world_ref.graph,
                        &mut world_ref.text_log,
                        &world_ref.event_bus,
                        world_ref.player_location,
                    )
                } else {
                    parish_core::npc::banshee::BansheeReport::default()
                };
                if !banshee_report.is_empty() {
                    let mut debug_events = state.debug_events.lock().await;
                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(DebugEvent {
                        timestamp: world.clock.now().format("%H:%M %Y-%m-%d").to_string(),
                        category: "banshee".to_string(),
                        message: format!(
                            "{} wail(s), {} death(s)",
                            banshee_report.wails.len(),
                            banshee_report.deaths.len()
                        ),
                    });
                }

                // Log schedule events and tier transitions to debug panel
                if !schedule_events.is_empty() || !tier_transitions.is_empty() {
                    let ts = world.clock.now().format("%H:%M %Y-%m-%d").to_string();
                    let mut debug_events = state.debug_events.lock().await;
                    for evt in &schedule_events {
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: ts.clone(),
                            category: "schedule".to_string(),
                            message: evt.debug_string(),
                        });
                    }
                    for tt in &tier_transitions {
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
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

                // Propagate gossip between co-located Tier 2 NPCs
                // Scope thread_rng tightly so it is dropped before any await.
                let total_gossip = if !world.gossip_network.is_empty() {
                    let groups = npc_mgr.tier2_groups();
                    let mut rng = rand::rng();
                    let mut total = 0usize;
                    for npc_ids in groups.values() {
                        if npc_ids.len() >= 2 {
                            total += parish_core::npc::ticks::propagate_gossip_at_location(
                                npc_ids,
                                &mut world.gossip_network,
                                &mut rng,
                            );
                        }
                    }
                    total
                } else {
                    0
                };
                {
                    if total_gossip > 0 {
                        let mut debug_events = state.debug_events.lock().await;
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "gossip".to_string(),
                            message: format!(
                                "{} rumor(s) spread among co-located NPCs",
                                total_gossip
                            ),
                        });
                    }
                }

                // Dispatch Tier 4 rules engine if enough game time has elapsed.
                // tick_tier4 is sub-ms CPU work; runs inline inside the lock scope.
                if npc_mgr.needs_tier4_tick(now) {
                    let tier4_ids: std::collections::HashSet<parish_core::npc::NpcId> =
                        npc_mgr.tier4_npcs().into_iter().collect();
                    let events = {
                        let mut tier4_refs: Vec<&mut parish_core::npc::Npc> = npc_mgr
                            .npcs_mut()
                            .values_mut()
                            .filter(|n| tier4_ids.contains(&n.id))
                            .collect();
                        let game_date = now.date_naive();
                        let mut rng = rand::rng();
                        parish_core::npc::tier4::tick_tier4(
                            &mut tier4_refs,
                            season,
                            game_date,
                            &mut rng,
                        )
                    };
                    let game_events = npc_mgr.apply_tier4_events(&events, now, banshee_enabled);
                    // Collect per-event descriptions before publishing.
                    let life_descriptions: Vec<String> = game_events
                        .iter()
                        .filter_map(|ge| {
                            if let parish_core::world::events::GameEvent::LifeEvent {
                                description,
                                ..
                            } = ge
                            {
                                Some(description.clone())
                            } else {
                                None
                            }
                        })
                        .collect();
                    for evt in game_events {
                        world.event_bus.publish(evt);
                    }
                    npc_mgr.record_tier4_tick(now);
                    let mut debug_events = state.debug_events.lock().await;
                    // Per-event life_event entries
                    for desc in &life_descriptions {
                        if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                            debug_events.pop_front();
                        }
                        debug_events.push_back(DebugEvent {
                            timestamp: String::new(),
                            category: "life_event".to_string(),
                            message: desc.clone(),
                        });
                    }
                    // Aggregate tier4 entry
                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                        debug_events.pop_front();
                    }
                    debug_events.push_back(DebugEvent {
                        timestamp: String::new(),
                        category: "tier4".to_string(),
                        message: format!("Tier 4 tick: {} events", events.len()),
                    });
                }

                // Dispatch Tier 3 batch LLM simulation for distant NPCs.
                // The LLM call can take 10-30 s, so we spawn a detached task
                // and release the world/npc_mgr locks before awaiting.
                if npc_mgr.needs_tier3_tick(now) && !npc_mgr.tier3_in_flight() {
                    use parish_core::npc::ticks::Tier3Snapshot;
                    use parish_core::npc::ticks::tier3_snapshot_from_npc;

                    let npc_names: std::collections::HashMap<_, _> =
                        npc_mgr.all_npcs().map(|n| (n.id, n.name.clone())).collect();
                    let tier3_ids = npc_mgr.tier3_npcs();
                    let snapshots: Vec<Tier3Snapshot> = tier3_ids
                        .iter()
                        .filter_map(|id| npc_mgr.get(*id))
                        .map(|npc| tier3_snapshot_from_npc(npc, &world.graph, &npc_names))
                        .collect();

                    if !snapshots.is_empty() {
                        let time_desc = world.clock.time_of_day().to_string();
                        let weather_str = world.weather.to_string();
                        let season_str = format!("{:?}", world.clock.season());
                        let hours = 24u32;

                        npc_mgr.set_tier3_in_flight(true);

                        let state_t3 = Arc::clone(&state);
                        // #9 — snapshot the sim-preemption token BEFORE spawn
                        // so player input arriving mid-decode can cancel this
                        // call. Cancelling replaces the token with a fresh one
                        // for the *next* sim cycle.
                        let cancel_t3 = state_t3.sim_cancel.lock().await.clone();
                        tokio::spawn(async move {
                            // Resolve the per-category Simulation client +
                            // model. Direct-client dispatch (not the queue)
                            // is what makes per-category routing actually
                            // hit the small slot on the two-slot loadout —
                            // the queue's worker only knows about the base
                            // client (#?). Mirrors `emit_npc_reactions`.
                            let (client_opt, model) = {
                                let cfg = state_t3.config.lock().await;
                                let base_client = state_t3.client.lock().await;
                                cfg.resolve_category_client(
                                    InferenceCategory::Simulation,
                                    base_client.as_ref(),
                                )
                            };
                            let Some(sim_client) = client_opt else {
                                state_t3.npc_manager.lock().await.set_tier3_in_flight(false);
                                return;
                            };

                            let ctx = parish_core::npc::ticks::Tier3Context {
                                snapshots: &snapshots,
                                client: &sim_client,
                                model: &model,
                                time_desc: &time_desc,
                                weather: &weather_str,
                                season: &season_str,
                                hours,
                                batch_size: 0,
                                language: &state_t3.language_settings,
                                cancel: Some(cancel_t3),
                            };

                            let result = parish_core::npc::ticks::tick_tier3(&ctx).await;

                            // Re-acquire locks to apply updates.
                            // Lock ordering: `world` → `npc_manager`
                            // (matches the documented contract and the
                            // main tick at lib.rs:955-956).  Acquiring
                            // npc_manager first while a concurrent main
                            // tick holds world would deadlock (#337).
                            let world = state_t3.world.lock().await;
                            let mut npc_mgr = state_t3.npc_manager.lock().await;
                            let game_time = world.clock.now();

                            match result {
                                Ok(updates) => {
                                    let _events = parish_core::npc::ticks::apply_tier3_updates(
                                        &updates,
                                        npc_mgr.npcs_mut(),
                                        &world.graph,
                                        game_time,
                                    );
                                    npc_mgr.record_tier3_tick(game_time);
                                    tracing::debug!(
                                        "Tier 3 tick: {} updates applied",
                                        updates.len()
                                    );

                                    let mut debug_events = state_t3.debug_events.lock().await;
                                    if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                                        debug_events.pop_front();
                                    }
                                    debug_events.push_back(DebugEvent {
                                        timestamp: String::new(),
                                        category: "tier3".to_string(),
                                        message: format!("Tier 3 tick: {} updates", updates.len()),
                                    });
                                }
                                Err(e) => {
                                    tracing::warn!("Tier 3 tick failed: {}", e);
                                }
                            }

                            npc_mgr.set_tier3_in_flight(false);
                        });
                    }
                }

                // Dispatch Tier 2 background simulation for nearby NPCs.
                // Submits one LLM call per location group via the priority queue
                // (Background lane, yields to Tier 1 dialogue).
                if npc_mgr.needs_tier2_tick(now) && !npc_mgr.tier2_in_flight() {
                    use parish_core::npc::ticks::{Tier2Group, npc_snapshot_from_npc};

                    let groups_map = npc_mgr.tier2_groups();
                    if !groups_map.is_empty() {
                        let npc_names: std::collections::HashMap<_, _> =
                            npc_mgr.all_npcs().map(|n| (n.id, n.name.clone())).collect();
                        // Build owned snapshots inside the lock scope.
                        let groups: Vec<Tier2Group> = groups_map
                            .into_iter()
                            .filter_map(|(loc, npc_ids)| {
                                let location_name = world
                                    .graph
                                    .get(loc)
                                    .map(|d| d.name.clone())
                                    .unwrap_or_else(|| format!("Location {}", loc.0));
                                let npcs: Vec<_> = npc_ids
                                    .iter()
                                    .filter_map(|id| npc_mgr.get(*id))
                                    .map(|npc| npc_snapshot_from_npc(npc, &npc_names))
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
                            let time_desc = world.clock.time_of_day().to_string();
                            let weather_str = world.weather.to_string();

                            npc_mgr.set_tier2_in_flight(true);

                            let state_t2 = Arc::clone(&state);
                            // #9 — snapshot the sim-preemption token (same
                            // semantics as the Tier 3 site above).
                            let cancel_t2 = state_t2.sim_cancel.lock().await.clone();
                            tokio::spawn(async move {
                                // Resolve the per-category Simulation client +
                                // model. Direct-client dispatch is what makes
                                // the two-slot loadout actually hit the small
                                // slot for Simulation. Matches the Tier 3 site
                                // above.
                                let (client_opt, model) = {
                                    let cfg = state_t2.config.lock().await;
                                    let base_client = state_t2.client.lock().await;
                                    cfg.resolve_category_client(
                                        InferenceCategory::Simulation,
                                        base_client.as_ref(),
                                    )
                                };
                                let Some(sim_client) = client_opt else {
                                    state_t2.npc_manager.lock().await.set_tier2_in_flight(false);
                                    return;
                                };

                                // Submit each group sequentially (one LLM call
                                // per group, single connection).
                                let mut events = Vec::new();
                                for group in &groups {
                                    if let Some(evt) = parish_core::npc::ticks::run_tier2_for_group(
                                        &sim_client,
                                        &model,
                                        group,
                                        &time_desc,
                                        &weather_str,
                                        &state_t2.language_settings,
                                        Some(cancel_t2.clone()),
                                    )
                                    .await
                                    {
                                        events.push(evt);
                                    }
                                    // If the player turn fired during the
                                    // previous group's call, the snapshotted
                                    // token is now cancelled — bail rather
                                    // than burn the queue running every
                                    // remaining group through a cancellation
                                    // error.
                                    if cancel_t2.is_cancelled() {
                                        break;
                                    }
                                }

                                // Re-acquire locks to apply events.
                                // Lock ordering: `world` → `npc_manager`
                                // (matches the documented contract and the
                                // main tick at lib.rs:955-956).  Acquiring
                                // npc_manager first while a concurrent main
                                // tick holds world would deadlock (#337).
                                let mut world = state_t2.world.lock().await;
                                let mut npc_mgr = state_t2.npc_manager.lock().await;
                                let game_time = world.clock.now();

                                for event in &events {
                                    let _dbg = parish_core::npc::ticks::apply_tier2_event(
                                        event,
                                        npc_mgr.npcs_mut(),
                                        game_time,
                                    );
                                    // Push gossip so it can propagate to other NPCs.
                                    parish_core::npc::ticks::create_gossip_from_tier2_event(
                                        event,
                                        &mut world.gossip_network,
                                        game_time,
                                    );
                                }
                                npc_mgr.record_tier2_tick(game_time);
                                npc_mgr.set_tier2_in_flight(false);

                                let mut debug_events = state_t2.debug_events.lock().await;
                                if debug_events.len() >= DEBUG_EVENT_CAPACITY {
                                    debug_events.pop_front();
                                }
                                debug_events.push_back(DebugEvent {
                                    timestamp: String::new(),
                                    category: "tier2".to_string(),
                                    message: format!(
                                        "Tier 2 tick: {} events from {} groups",
                                        events.len(),
                                        groups.len()
                                    ),
                                });
                            });
                        }
                    }
                }
            }

            // Advance the generation counter so handle_game_input can
            // detect TOCTOU races (see issue #283).
            world.increment_tick_generation();
        }
    });
}

/// Inactivity tick: drives idle banter and auto-pause every 1 s.
pub(crate) fn spawn_inactivity_tick(handle: AppHandle, state: Arc<AppState>) {
    let token = state.shutdown_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
            crate::commands::tick_inactivity(&state, &handle).await;
        }
    });
}

/// Debug tick: emits a debug snapshot every 2 s.
///
/// Snapshot each piece of state with a brief, non-overlapping
/// lock window to avoid holding all 5+ locks simultaneously
/// (#105, #282). Lock order: `world` -> `npc_manager` ->
/// `inference_queue` -> `config` -> `debug_events` -> `game_events` ->
/// `inference_log` (#483).
pub(crate) fn spawn_debug_tick(handle: AppHandle, state: Arc<AppState>) {
    let token = state.shutdown_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
            }

            // 1. Peek inference_queue presence first (#483).
            let has_inference_queue = state.inference_queue.lock().await.is_some();

            // 2. Clone config fields — drop the lock immediately.
            let (
                provider_name,
                model_name,
                base_url,
                cloud_provider,
                cloud_model,
                improv_enabled,
                categories,
            ) = {
                let config = state.config.lock().await;
                (
                    config.provider_name.clone(),
                    config.model_name.clone(),
                    config.base_url.clone(),
                    config.cloud_provider_name.clone(),
                    config.cloud_model_name.clone(),
                    config.improv_enabled,
                    parish_core::debug_snapshot::build_inference_categories(&config),
                )
            };

            // 3. Clone debug_events ring buffer — drop immediately.
            let debug_events_snapshot: std::collections::VecDeque<
                parish_core::debug_snapshot::DebugEvent,
            > = state.debug_events.lock().await.iter().cloned().collect();

            // 4. Clone game_events ring buffer — drop immediately.
            let game_events_snapshot: std::collections::VecDeque<
                parish_core::world::events::GameEvent,
            > = state.game_events.lock().await.iter().cloned().collect();

            // 5. Clone inference log — drop immediately.
            let call_log: Vec<parish_core::debug_snapshot::InferenceLogEntry> =
                state.inference_log.lock().await.iter().cloned().collect();

            // Build InferenceDebug from cloned data (no locks held).
            let inference = InferenceDebug {
                provider_name,
                model_name,
                base_url,
                cloud_provider,
                cloud_model,
                has_queue: has_inference_queue,
                reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
                improv_enabled,
                call_log,
                categories,
                configured_providers: parish_core::debug_snapshot::build_configured_providers(),
            };

            // 6. Acquire world and npc_manager (canonical order)
            // only for the pure-read snapshot build, then release.
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let snapshot = parish_core::debug_snapshot::build_debug_snapshot(
                &world,
                &npc_manager,
                &debug_events_snapshot,
                &game_events_snapshot,
                &inference,
                &parish_core::debug_snapshot::AuthDebug::disabled(),
            );
            drop(npc_manager);
            drop(world);

            let _ = handle.emit(events::EVENT_DEBUG_UPDATE, snapshot);
        }
    });
}

/// Autosave tick: writes a snapshot every [`AUTOSAVE_INTERVAL_SECS`] when a
/// save file and branch are active.
pub(crate) fn spawn_autosave_tick(state: Arc<AppState>) {
    let token = state.shutdown_token.clone();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(Duration::from_secs(AUTOSAVE_INTERVAL_SECS)) => {}
            }

            // Only autosave if a save file and branch are active
            let save_path = state.save_path.lock().await.clone();
            let branch_id = *state.current_branch_id.lock().await;

            if let (Some(path), Some(bid)) = (save_path, branch_id) {
                let world = state.world.lock().await;
                let npc_manager = state.npc_manager.lock().await;
                let snapshot =
                    parish_core::persistence::snapshot::GameSnapshot::capture(&world, &npc_manager);
                drop(npc_manager);
                drop(world);

                match parish_core::persistence::Database::open(&path) {
                    Ok(db) => match db.save_snapshot(bid, &snapshot) {
                        Ok(_) => tracing::debug!("Autosave complete"),
                        Err(e) => tracing::warn!("Autosave failed: {}", e),
                    },
                    Err(e) => tracing::warn!("Autosave DB open failed: {}", e),
                }
            }
        }
    });
}

#[cfg(test)]
mod onboarding_choice_tests {
    use super::{OnboardingChoice, OnboardingInputs, resolve_onboarding_choice};
    use parish_core::config::Provider;

    fn unconfigured() -> OnboardingInputs {
        OnboardingInputs {
            onboarding_complete: false,
            parish_provider_env_set: false,
            api_key_env_set: false,
            explicit_provider_set: false,
            is_macos: false,
            recommended: Provider::Simulator,
        }
    }

    #[test]
    fn prior_onboarding_short_circuits() {
        let mut inputs = unconfigured();
        inputs.onboarding_complete = true;
        // Even if every other "you're in a wizard" signal is true,
        // the sentinel wins and we skip the fork entirely.
        inputs.is_macos = true;
        inputs.recommended = Provider::VllmMlx;
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::Configured
        );
    }

    #[test]
    fn parish_provider_env_short_circuits() {
        let mut inputs = unconfigured();
        inputs.parish_provider_env_set = true;
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::Configured
        );
    }

    #[test]
    fn api_key_env_short_circuits() {
        let mut inputs = unconfigured();
        inputs.api_key_env_set = true;
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::Configured
        );
    }

    #[test]
    fn explicit_non_simulator_provider_short_circuits() {
        let mut inputs = unconfigured();
        inputs.explicit_provider_set = true;
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::Configured
        );
    }

    #[test]
    fn mac_with_vllm_recommendation_picks_local_recommended() {
        let inputs = OnboardingInputs {
            is_macos: true,
            recommended: Provider::VllmMlx,
            ..unconfigured()
        };
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::LocalRecommended
        );
    }

    #[test]
    fn mac_below_16gb_picks_local_low_mem() {
        // recommended_for_platform returns Simulator for Mac < 16 GB.
        // We still offer a local-fork option (small slot only); we
        // don't fall through to BYOK-only.
        let inputs = OnboardingInputs {
            is_macos: true,
            recommended: Provider::Simulator,
            ..unconfigured()
        };
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::LocalLowMem
        );
    }

    #[test]
    fn mac_with_non_mac_recommendation_falls_back_to_byok_only() {
        // Defensive: if the platform-recommender ever returns Ollama
        // or another non-local pick on Mac, we don't expose a local
        // fork — surface BYOK-only.
        let inputs = OnboardingInputs {
            is_macos: true,
            recommended: Provider::Ollama,
            ..unconfigured()
        };
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::LocalUnavailable
        );
    }

    #[test]
    fn non_mac_always_picks_byok_only() {
        let inputs = OnboardingInputs {
            is_macos: false,
            recommended: Provider::VllmMlx, // ignored on non-Mac
            ..unconfigured()
        };
        assert_eq!(
            resolve_onboarding_choice(inputs),
            OnboardingChoice::LocalUnavailable
        );
    }
}
