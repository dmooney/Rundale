//! Tauri command handlers for the Parish desktop frontend.
//!
//! Each public function here is registered with `tauri::generate_handler!` and
//! becomes callable from the Svelte frontend via `invoke("command_name", args)`.

use std::sync::Arc;

use parish_core::config::InferenceCategory;
use parish_core::debug_snapshot::{self, AuthDebug, DebugEvent, DebugSnapshot, InferenceDebug};
// AnyClient, InferenceQueue, spawn_inference_worker formerly imported here —
// now handled by parish_core::game_loop::rebuild_inference_worker (#696).
use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::{compute_name_hints, text_log, text_log_for_stream_turn, text_log_typed};
use parish_core::npc::reactions;
use parish_core::world::LocationId;
// DEFAULT_START_LOCATION — no longer used directly; handled by load_fresh_world_and_npcs (#696).
use tauri::Emitter;

use crate::events::{
    EVENT_STREAM_END, EVENT_STREAM_TOKEN, EVENT_STREAM_TURN_END, EVENT_TEXT_LOG,
    EVENT_TRAVEL_START, EVENT_WORLD_UPDATE, StreamEndPayload, StreamTokenPayload,
    StreamTurnEndPayload, TextLogPayload,
};
use crate::{AppState, MapData, MapLocation, NpcInfo, SaveState, ThemePalette, WorldSnapshot};

/// Returns a formatted game-time string (`HH:MM YYYY-MM-DD`) snapshotted
/// from the shared world clock. Used for debug event timestamps so the
/// Events tab no longer renders blank times.
async fn debug_event_timestamp(state: &Arc<AppState>) -> String {
    let world = state.world.lock().await;
    world.clock.now().format("%H:%M %Y-%m-%d").to_string()
}

// ── Helper: build a WorldSnapshot from locked world state ────────────────────

/// Builds a [`WorldSnapshot`] from a locked world state reference.
///
/// Used both by the `get_world_snapshot` command and by the background
/// idle-tick task in `lib.rs`. Includes name pronunciation hints when
/// NPC manager and pronunciation data are provided.
pub fn get_world_snapshot_inner(
    world: &parish_core::world::WorldState,
    npc_manager: Option<&parish_core::npc::manager::NpcManager>,
    pronunciations: &[parish_core::game_mod::PronunciationEntry],
) -> WorldSnapshot {
    let mut snapshot = snapshot_from_world(world);
    if let Some(npc_mgr) = npc_manager {
        snapshot.name_hints = compute_name_hints(world, npc_mgr, pronunciations);
    }
    snapshot
}

/// Converts a core [`parish_core::ipc::WorldSnapshot`] into the Tauri-specific
/// [`WorldSnapshot`] (which includes additional fields like `name_hints`).
fn snapshot_from_world(world: &parish_core::world::WorldState) -> WorldSnapshot {
    let core = parish_core::ipc::snapshot_from_world(world);
    WorldSnapshot {
        location_name: core.location_name,
        location_description: core.location_description,
        time_label: core.time_label,
        hour: core.hour,
        minute: core.minute,
        weather: core.weather,
        season: core.season,
        festival: core.festival,
        paused: core.paused,
        inference_paused: core.inference_paused,
        game_epoch_ms: core.game_epoch_ms,
        speed_factor: core.speed_factor,
        name_hints: vec![],
        day_of_week: core.day_of_week,
    }
}

// compute_name_hints is now shared via parish_core::ipc::compute_name_hints

// ── Commands ─────────────────────────────────────────────────────────────────

/// Returns a snapshot of the current world state (location, time, weather, season).
#[tauri::command]
pub async fn get_world_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<WorldSnapshot, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
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
    let config = state.config.lock().await;
    let transport = state.transport.default_mode();
    let core_map =
        parish_core::ipc::build_map_data(&world, transport, config.reveal_unexplored_locations);

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
        edge_traversals: core_map.edge_traversals,
        transport_label: core_map.transport_label,
        transport_id: core_map.transport_id,
    })
}

/// Returns the list of NPCs currently at the player's location.
#[tauri::command]
pub async fn get_npcs_here(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<NpcInfo>, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Ok(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

/// Returns the current time-of-day palette as CSS hex colours.
#[tauri::command]
pub async fn get_theme(state: tauri::State<'_, Arc<AppState>>) -> Result<ThemePalette, String> {
    use chrono::Timelike;
    use parish_palette::compute_palette;
    let world = state.world.lock().await;
    let now = world.clock.now();
    let raw = compute_palette(now.hour(), now.minute());
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
    let game_events = state.game_events.lock().await;
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
        reaction_req_id: parish_core::game_session::reaction_req_id_peek(),
        improv_enabled: config.improv_enabled,
        call_log,
        categories: parish_core::debug_snapshot::build_inference_categories(&config),
        configured_providers: parish_core::debug_snapshot::build_configured_providers(),
    };

    Ok(debug_snapshot::build_debug_snapshot(
        &world,
        &npc_manager,
        &events,
        &game_events,
        &inference,
        &AuthDebug::disabled(),
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

/// Returns the latest provider-bootstrap status for the startup overlay.
#[tauri::command]
pub async fn get_setup_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::SetupStatusSnapshot, String> {
    Ok(state
        .setup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone())
}

// ── BYOK onboarding commands ─────────────────────────────────────────────────

fn byok_ctx<'a>(state: &'a Arc<AppState>) -> parish_core::ipc::byok::ByokContext<'a> {
    parish_core::ipc::byok::ByokContext {
        config: &state.config,
        inference_config: &state.inference_config,
        inference_log: state.inference_log.clone(),
        slots: parish_core::game_loop::inference::InferenceSlots {
            client: &state.client,
            worker_handle: &state.worker_handle,
            inference_queue: &state.inference_queue,
        },
        secrets: std::sync::Arc::clone(&state.secret_store),
        user_config_dir: state.user_config_dir.as_path(),
    }
}

/// Validates an unsaved provider/key combination by issuing a tiny live
/// request. Used by the BYOK wizard before saving.
#[tauri::command]
pub async fn validate_provider_config(
    args: parish_core::ipc::byok::ValidateProviderConfigArgs,
) -> Result<parish_core::inference::validate::ValidationOutcome, String> {
    Ok(parish_core::ipc::byok::handle_validate_provider_config(args).await)
}

/// Persists a BYOK config (keychain + parish.toml), updates GameConfig, and
/// rebuilds the inference worker. Emits a fresh `setup-done` so the overlay
/// dismisses.
#[tauri::command]
pub async fn set_provider_config(
    args: parish_core::ipc::byok::SetProviderConfigArgs,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let state_arc = state.inner().clone();
    parish_core::ipc::byok::handle_set_provider_config(args, byok_ctx(&state_arc))
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut s = state_arc
            .setup_status
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        s.clear_needs_onboarding();
    }
    crate::record_setup_done(&state_arc, true, String::new());
    let _ = app.emit(
        crate::events::EVENT_SETUP_DONE,
        crate::events::SetupDonePayload {
            success: true,
            error: String::new(),
        },
    );
    Ok(())
}

/// Returns the current effective provider config for the settings panel.
/// Never returns the API key itself — only `has_api_key`/`has_env_key` flags.
#[tauri::command]
pub async fn get_provider_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<parish_core::ipc::byok::GetProviderConfigResult, String> {
    Ok(parish_core::ipc::byok::handle_get_provider_config(&state.config).await)
}

/// Wipes the keychain entry for the active provider and clears parish.toml.
#[tauri::command]
pub async fn clear_provider_config(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let state_arc = state.inner().clone();
    parish_core::ipc::byok::handle_clear_provider_config(byok_ctx(&state_arc))
        .await
        .map_err(|e| e.to_string())
}

/// Returns `{provider_id: has_env_key}` for every known provider so the BYOK
/// wizard can show the env-detected hint on the picked provider, not just the
/// current one.
#[tauri::command]
pub async fn list_byok_env_keys() -> Result<std::collections::BTreeMap<String, bool>, String> {
    Ok(parish_core::ipc::byok::handle_list_env_keys())
}

/// Returns `{provider_id: {dialogue, simulation, intent, reaction}}` — the
/// wizard uses this so its prefill matches what fill_missing_models_from_presets
/// will actually install for the other tiers.
#[tauri::command]
pub async fn list_preset_models() -> Result<
    std::collections::BTreeMap<String, Vec<parish_core::ipc::byok::ProviderPresetOption>>,
    String,
> {
    Ok(parish_core::ipc::byok::handle_list_preset_models())
}

// ── #?: local-inference onboarding commands ────────────────────────────────

/// Onboarding-choice payload returned to the SetupOverlay so it can render
/// the right fork (local-recommended / local-low-mem / local-unavailable /
/// configured).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct OnboardingOptions {
    /// The variant computed from platform + RAM + provider env.
    pub choice: crate::setup::OnboardingChoice,
    /// Unified memory in GB (macOS) or 0 elsewhere — feeds the "your Mac
    /// has Xgb" copy on the local-recommended card.
    pub ram_gb: u64,
}

/// Computes the onboarding fork to show the user on first run.
///
/// Pure read: no side effects on `AppState`. The SetupOverlay calls this
/// once on mount to populate the fork UI; the same value is also persisted
/// on `SetupStatusSnapshot.onboarding_choice` so reconnects after the
/// EVENT_SETUP_NEEDS_ONBOARDING event still resolve.
#[tauri::command]
pub async fn get_onboarding_options(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<OnboardingOptions, String> {
    Ok(do_get_onboarding_options(state.inner()).await)
}

/// Internal helper shared with `mcp_bridge::onboarding_options`. Takes
/// `&Arc<AppState>` so the MCP route can call it without the Tauri
/// `State<'_, _>` wrapper.
pub(crate) async fn do_get_onboarding_options(state: &Arc<AppState>) -> OnboardingOptions {
    use parish_core::config::Provider;

    let cfg = state.config.lock().await;
    let provider_config = parish_core::config::ProviderConfig {
        provider: Provider::from_str_loose(&cfg.provider_name).unwrap_or_default(),
        api_key: cfg.api_key.clone(),
        base_url: cfg.base_url.clone(),
        model: Some(cfg.model_name.clone()),
    };
    drop(cfg);
    let choice = crate::setup::onboarding_choice_for_platform(state, &provider_config);
    let ram_gb = parish_core::config::unified_memory_bytes()
        .map(|b| b / (1024 * 1024 * 1024))
        .unwrap_or(0);
    OnboardingOptions { choice, ram_gb }
}

/// Selection submitted from `LocalInferenceFork`. `two-slot` runs the
/// 14B Dialogue + 1.5B small-slot loadout (recommended ≥16 GB);
/// `small-only` runs Qwen2.5-1.5B-Instruct-4bit for every category on
/// the same port (acceptable on <16 GB Macs at degraded quality).
#[derive(serde::Deserialize)]
pub struct LocalSetupArgs {
    pub variant: String,
}

/// Persists local-inference config (provider=vllm-mlx + per-category
/// overrides for the two-slot loadout), pre-downloads the required
/// HuggingFace repos with progress reporting through the existing
/// SetupOverlay surface, and rebuilds the inference worker.
///
/// On success, emits `setup-done` and clears the onboarding gate exactly
/// like `set_provider_config` does for BYOK.
#[tauri::command]
pub async fn start_local_inference_setup(
    args: LocalSetupArgs,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_start_local_inference_setup(state.inner(), &app, args).await
}

/// Internal worker for the local-inference setup flow, shared with the MCP
/// bridge route so an MCP client can drive the same wizard the desktop UI
/// uses. Takes plain `&Arc<AppState>` + `&AppHandle` to sidestep the Tauri
/// `State` wrapper that the `#[tauri::command]` shim provides.
pub(crate) async fn do_start_local_inference_setup(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    args: LocalSetupArgs,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    // Idempotency guard — a second POST while the first is still running
    // would race the bootstrap pipeline, double-spawn vllm-mlx serve, and
    // produce undefined behaviour for the inference queue. Drop the
    // duplicate with a busy error; the in-flight wizard keeps running.
    // RAII guard restores the flag on every exit path.
    struct WizardGuard<'a>(&'a std::sync::atomic::AtomicBool);
    impl<'a> Drop for WizardGuard<'a> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::Release);
        }
    }
    if state
        .wizard_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("local-inference setup already in progress".to_string());
    }
    let _guard = WizardGuard(&state.wizard_in_flight);

    // Error-path UX (#?) — every failing exit emits a `setup-done` with
    // success=false + the error message, so the SetupOverlay drops out of
    // the "Downloading…" spinner and the user sees what went wrong
    // (network blip, disk full, vllm-mlx spawn failure). Without this the
    // wizard hangs on the spinner forever and the user has to restart the
    // app to see anything. The inner `result` is also returned to the
    // caller so the MCP / Tauri shim can pass it back over the wire.
    let inner = do_start_local_inference_setup_inner(state, app, args).await;
    if let Err(ref e) = inner {
        crate::record_setup_done(state, false, e.clone());
        let _ = app.emit(
            crate::events::EVENT_SETUP_DONE,
            crate::events::SetupDonePayload {
                success: false,
                error: e.clone(),
            },
        );
    }
    inner
}

async fn do_start_local_inference_setup_inner(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    args: LocalSetupArgs,
) -> Result<(), String> {
    use parish_core::inference::hf_downloader::HfModelDownloader;
    use parish_core::inference::setup::SetupProgress;
    use std::sync::Arc as StdArc;

    let state_arc = state.clone();

    let two_slot = match args.variant.as_str() {
        "two-slot" => true,
        "small-only" => false,
        other => return Err(format!("unknown variant: {other}")),
    };

    // Repos to fetch. The small slot is always present; the big slot only
    // when the user picked two-slot (≥16 GB hosts).
    let mut repos: Vec<&str> = vec!["mlx-community/Qwen2.5-1.5B-Instruct-4bit"];
    if two_slot {
        repos.insert(0, "mlx-community/Qwen2.5-14B-Instruct-4bit");
    }

    // Co-locate the HF cache under the user's app-config dir so a
    // re-extracted bundle finds the existing models. `HF_HOME` env var
    // gets set to this same path when we spawn vllm-mlx serve.
    let hf_home = state_arc.user_config_dir.join("models");
    std::fs::create_dir_all(&hf_home).map_err(|e| format!("mkdir {hf_home:?}: {e}"))?;

    // `hf-hub` follows the Python convention: cache lives at
    // `$HF_HOME/hub/...`. We pass the `hub` subdir directly to
    // `with_cache_dir` (hf-hub treats it as the root for `models--…`).
    let hf_hub = hf_home.join("hub");

    let progress: StdArc<dyn SetupProgress> = StdArc::new(crate::TauriProgress::new(
        app.clone(),
        StdArc::clone(&state_arc),
    ));

    let downloader = HfModelDownloader::new(progress).with_cache_dir(hf_hub);
    downloader
        .download_models(&repos)
        .await
        .map_err(|e| format!("HF download failed: {e}"))?;

    // Hand HF cache root + offline marker to the spawned vllm-mlx serve so
    // it never re-checks the hub.
    //
    // SAFETY: set_var is unsafe on POSIX in multi-threaded contexts.
    // The Tauri app is already running multi-threaded at this point, but
    // we still need the env to be picked up by `VllmMlxProcess::ensure_running`
    // when bootstrap runs. The variables are set before any worker spawn
    // and the threads that read them do so within Command::env-style
    // snapshots; this is the same approach used by parish_tauri::run() for
    // VLLM_MLX_BIN.
    unsafe {
        std::env::set_var("PARISH_HF_HOME", &hf_home);
    }

    // Write provider config: two-slot puts Sim/Reaction/Intent on the
    // small-slot port, Dialogue on the big-slot port. Small-only puts
    // everything on the small-slot port.
    let (provider_name, model_name, base_url) = if two_slot {
        (
            "vllm-mlx".to_string(),
            "mlx-community/Qwen2.5-14B-Instruct-4bit".to_string(),
            "http://localhost:8000".to_string(),
        )
    } else {
        (
            "vllm-mlx".to_string(),
            "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string(),
            "http://localhost:8001".to_string(),
        )
    };

    let mut category_overrides: std::collections::BTreeMap<
        String,
        parish_core::config::user_config::CategoryOverride,
    > = std::collections::BTreeMap::new();
    if two_slot {
        // Intent stays on the small slot (1.5B) — fast classification
        // doesn't need 14B and parse_intent's `Unknown` fallback handles
        // any JSON parse failures (falls through to handle_npc_conversation
        // as dialogue).
        category_overrides.insert(
            "intent".to_string(),
            parish_core::config::user_config::CategoryOverride {
                provider: Some("vllm-mlx".to_string()),
                base_url: Some("http://localhost:8001".to_string()),
                model: Some("mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string()),
            },
        );
        // Sim + Reaction route to the simulator: the 1.5B can't reliably
        // hold strict JSON for Tier 2 / Tier 3, and the resulting parse
        // failures flooded the log every 5 game-seconds. The simulator
        // returns valid `Tier2Response` / `Tier3Update` shapes (all
        // `#[serde(default)]`), so ticks succeed as "uneventful" and the
        // 1.5B is reserved for intent + the deterministic schedule logic
        // handles NPC motion. The 14B big slot stays free for dialogue.
        for cat_name in ["simulation", "reaction"] {
            category_overrides.insert(
                cat_name.to_string(),
                parish_core::config::user_config::CategoryOverride {
                    provider: Some("simulator".to_string()),
                    base_url: None,
                    model: None,
                },
            );
        }
    } else {
        // small-only variant: 1.5B can't reliably hold the strict JSON
        // schema Tier 2 (Simulation) and Tier 3 (Reaction) expect, and
        // the resulting parse-failure storm both floods logs and starves
        // the model slot Tier 1 needs for the dialogue stream. Route
        // those categories to the in-process simulator so the
        // living-world ticks stay quiet and every cycle of the 1.5B is
        // spent on player-facing dialogue.
        //
        // Intent stays on vllm-mlx: parse_intent's failure path returns
        // `Unknown` which falls through to `handle_npc_conversation`,
        // i.e. a bad JSON parse still ends up routing player input as
        // dialogue. The simulator's intent_json_for matches verb
        // prefixes by `starts_with(mw)` without a word boundary, so
        // "Good morning" gets classified as `Move`-to-"od morning" and
        // the actual conversation never fires.
        for cat_name in ["simulation", "reaction"] {
            category_overrides.insert(
                cat_name.to_string(),
                parish_core::config::user_config::CategoryOverride {
                    provider: Some("simulator".to_string()),
                    base_url: None,
                    model: None,
                },
            );
        }
    }

    let set_args = parish_core::ipc::byok::SetProviderConfigArgs {
        provider: provider_name,
        base_url: Some(base_url),
        model: Some(model_name),
        api_key: None,
        category_overrides,
    };
    parish_core::ipc::byok::handle_set_provider_config(set_args, byok_ctx(&state_arc))
        .await
        .map_err(|e| e.to_string())?;

    // Clear the onboarding gate so the bootstrap path below doesn't
    // bail out on the `onboarding_choice_for_platform` check.
    {
        let mut s = state_arc
            .setup_status
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        s.clear_needs_onboarding();
    }

    // Run the same post-gate startup pipeline run() executes when the
    // bootstrap completes cleanly on a returning user. Without this,
    // the wizard would write the config + emit setup-done but the
    // vllm-mlx serve process never spawns, no inference queue exists,
    // and the world / autosave ticks never start — leaving the game
    // visibly "ready" but functionally inert until a manual relaunch.
    let (provider_config, _, _, _) = crate::provider_config_from_env(&state_arc.user_config_dir);
    let inference_config_clone = state_arc.inference_config.clone();
    let bootstrapped = crate::setup::bootstrap_inference_provider(
        app,
        &state_arc,
        &provider_config,
        &inference_config_clone,
    )
    .await;
    if !bootstrapped {
        return Err("inference bootstrap failed after wizard".to_string());
    }
    crate::setup::init_inference_queue(&state_arc).await;
    crate::setup::init_persistence(app, &state_arc).await;
    crate::setup::spawn_event_bus_fanin(&state_arc).await;
    crate::setup::spawn_world_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_inactivity_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_debug_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_autosave_tick(Arc::clone(&state_arc));
    Ok(())
}

/// Processes player text input: classification → movement, look, or NPC conversation.
///
/// Movement and look results are resolved synchronously. NPC conversations
/// submit an inference request and stream tokens back via `stream-token` events.
#[tauri::command]
pub async fn submit_input(
    text: String,
    addressed_to: Option<Vec<String>>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_submit_input(state.inner(), &app, text, addressed_to.unwrap_or_default()).await
}

/// Internal submit-input implementation shared with the MCP bridge.
///
/// Mirrors the Tauri command body but takes plain `&Arc<AppState>` /
/// `&tauri::AppHandle` so the `mcp_bridge` Axum handler can drive the same
/// dispatcher against the same live AppState the desktop window observes.
pub(crate) async fn do_submit_input(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: String,
    addressed_to: Vec<String>,
) -> Result<(), String> {
    let text = validate_input_text(&text)?;
    if text.is_empty() {
        return Ok(());
    }
    // #752 — cap addressed_to to prevent unbounded memory/allocation via the
    // NPC-addressing chip list.  Max 10 entries; each name ≤ 100 chars.
    validate_addressed_to(&addressed_to)?;

    touch_player_activity(state).await;

    // #9 — preempt any in-flight Tier 2 / Tier 3 sim call so the player's
    // turn doesn't queue behind a 30 s constrained-decode batch. Cancel the
    // current sim token (any sim call that snapshotted it will drop
    // mid-stream and free the model slot) and install a fresh token so the
    // next sim cycle has a live one to snapshot.
    {
        let mut sc = state.sim_cancel.lock().await;
        sc.cancel();
        *sc = tokio_util::sync::CancellationToken::new();
    }

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            handle_system_command(cmd, state, app).await;
        }
        InputResult::GameInput(raw) => {
            tracing::info!(input = %raw, "chat [player]");
            // Emit the player's own text as a dialogue bubble only for actual dialogue
            let player_msg = text_log("player", format!("> {}", raw));
            let player_msg_id = player_msg.id.clone();
            let _ = app.emit(EVENT_TEXT_LOG, player_msg);
            let raw_for_reactions = raw.clone();
            // Capture location before handle_game_input (which may move the player).
            let reaction_location = state.world.lock().await.player_location;
            handle_game_input(raw, addressed_to, state, app.clone()).await;
            // Generate NPC reactions to the player's message in the background.
            emit_npc_reactions(
                &player_msg_id,
                &raw_for_reactions,
                reaction_location,
                state,
                app,
            );
        }
    }

    Ok(())
}

// ── #752 — addressed_to validation ───────────────────────────────────────────

/// Validates and trims player free-text input for `submit_input`.
///
/// - Trims leading/trailing whitespace.
/// - Returns `Ok(String)` (the trimmed text) for empty input — callers should
///   short-circuit before calling this if they want to silently drop empties.
/// - Returns `Err` when the trimmed length exceeds 2000 characters.
pub fn validate_input_text(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().to_string();
    if trimmed.len() > 2000 {
        return Err("Input too long (max 2000 characters).".to_string());
    }
    Ok(trimmed)
}

/// Maximum number of NPC chips a single `submit_input` may carry. Validated
/// in [`validate_addressed_to`] and reused by `handle_game_input` to bound
/// `Vec::with_capacity` calls (#933 — CodeQL `rust/uncontrolled-allocation-size`).
pub(crate) const MAX_ADDRESSED_TO: usize = 10;

/// Upper bound for the merged `addressed_to + mentions` target list. Sized
/// generously above the realistic combined total — `addressed_to` is capped
/// at [`MAX_ADDRESSED_TO`] and `mentions.names.len()` is bounded by NPCs in
/// the world — so the allocation is guaranteed-small regardless of input.
pub(crate) const MAX_TARGETS: usize = 64;

/// Validates the `addressed_to` list from the `submit_input` command.
///
/// Rules (mode-parity with the server path in `parish-server`):
/// - At most [`MAX_ADDRESSED_TO`] entries (prevents unbounded NPC-chip spam).
/// - Each name is at most **100** characters.
///
/// Returns `Err(String)` with a user-visible message on any violation.
pub fn validate_addressed_to(addressed_to: &[String]) -> Result<(), String> {
    if addressed_to.len() > MAX_ADDRESSED_TO {
        return Err(format!("Too many addressees (max {MAX_ADDRESSED_TO})."));
    }
    if addressed_to.iter().any(|name| name.len() > 100) {
        return Err("Addressee name too long (max 100 characters).".to_string());
    }
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Rebuilds the inference pipeline after a provider/key/client change.
///
/// Replaces the client and respawns the inference worker so subsequent
/// NPC conversations use the new configuration.
pub async fn rebuild_inference_inner(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let (provider_name, base_url, api_key) = {
        let config = state.config.lock().await;
        (
            config.provider_name.clone(),
            config.base_url.clone(),
            config.api_key.clone(),
        )
    };

    // Delegate to shared worker-lifecycle helper (#696).
    let (_any_client, url_warning) = parish_core::game_loop::rebuild_inference_worker(
        &provider_name,
        &base_url,
        api_key.as_deref(),
        &state.inference_config,
        state.inference_log.clone(),
        parish_core::game_loop::inference::InferenceSlots {
            client: &state.client,
            worker_handle: &state.worker_handle,
            inference_queue: &state.inference_queue,
        },
    )
    .await;

    // Surface URL warning via Tauri emit (Tauri-specific side effect).
    if let Some(warn) = url_warning {
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".into(),
                content: warn,
                subtype: None,
            },
        );
    }
    // Note: Tauri has no trait-erased inference_client slot (unlike the server),
    // so no additional slot update is needed here.
}

async fn touch_player_activity(state: &Arc<AppState>) {
    let mut conversation = state.conversation.lock().await;
    let now = std::time::Instant::now();
    conversation.last_player_activity = now;
    conversation.last_spoken_at = now;
}

async fn emit_world_update(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
    let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
}

/// Handles `/command` inputs.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`TauriCommandHost`] adapter (#696 slice 7).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use crate::command_host::TauriCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;

    let host = TauriCommandHost::new(Arc::clone(state), app.clone());
    shared_handle(&host, cmd).await;
}

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
///
/// Takes plain `&Arc<AppState>` (not `tauri::State<...>`) so the body can be
/// called from non-Tauri-extractor contexts — namely the `mcp_bridge` Axum
/// handlers, which share the same live AppState as the desktop window. The
/// Tauri callsite passes `state.inner()`.
pub(crate) async fn handle_game_input(
    raw: String,
    addressed_to: Vec<String>,
    state: &Arc<AppState>,
    app: tauri::AppHandle,
) {
    // Resolve the intent client and model (Intent category override, or base).
    let (client, model) = {
        let config = state.config.lock().await;
        let base_client = state.client.lock().await;
        config.resolve_category_client(InferenceCategory::Intent, base_client.as_ref())
    };

    // Parse intent: tries local keywords first, then LLM for ambiguous input.
    let intent = if let Some(client) = &client {
        // Capture generation before releasing the lock so we can detect TOCTOU
        // races on re-acquire (issue #283).
        let gen_before = {
            let mut world = state.world.lock().await;
            world.clock.inference_pause();
            world.tick_generation
        };
        let result = parse_intent(client, &raw, &model).await;
        {
            let mut world = state.world.lock().await;
            world.clock.inference_resume();
            let gen_after = world.tick_generation;
            if gen_after != gen_before {
                tracing::warn!(
                    gen_before,
                    gen_after,
                    "World advanced during intent parse (TOCTOU #283) — \
                     {} tick(s) elapsed; proceeding with parsed intent",
                    gen_after.wrapping_sub(gen_before),
                );
                let _ = app.emit(
                    crate::events::EVENT_TEXT_LOG,
                    text_log(
                        "system",
                        "The world shifted while your words were in the air.",
                    ),
                );
            }
        }
        result.ok()
    } else {
        // No client configured — use local keyword parsing only.
        parish_core::input::parse_intent_local(&raw)
    };

    let is_move = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Move))
        .unwrap_or(false);
    let is_look = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Look))
        .unwrap_or(false);
    let is_talk = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Talk))
        .unwrap_or(false);
    let move_target = intent
        .as_ref()
        .filter(|_i| is_move)
        .and_then(|i| i.target.clone());
    let talk_target = intent
        .as_ref()
        .filter(|_i| is_talk)
        .and_then(|i| i.target.clone());

    if is_move {
        if let Some(target) = move_target {
            handle_movement(&target, state, &app).await;
        } else {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".into(),
                    content: "And where would ye be off to?".to_string(),
                    subtype: None,
                },
            );
        }
        return;
    }

    if is_look {
        handle_look(state, &app).await;
        return;
    }

    // `talk to <name>` / `speak to <name>` — bypass @mention parsing and
    // route directly to the multi-target dispatch loop with this single
    // addressee. The chip-selection list still gets prepended below.
    //
    // Pass `raw` (the original input) rather than an empty string so that
    // dialogue like "Hello Brigid, good morning!" is not discarded when the
    // intent parser classifies it as Talk. An empty `raw` still produces the
    // "say something first" prompt, which is correct for bare "talk to X".
    if is_talk && let Some(target) = talk_target {
        // Pre-allocate at the validated upper bound. Using the constant
        // directly (rather than `addressed_to.len() + 1`) keeps the
        // allocation size independent of user-controlled values for
        // CodeQL's `rust/uncontrolled-allocation-size` query (#933).
        let mut targets: Vec<String> = Vec::with_capacity(MAX_ADDRESSED_TO + 1);
        for name in addressed_to {
            if !targets.iter().any(|t| t == &name) {
                targets.push(name);
            }
        }
        if !targets.iter().any(|t| t == &target) {
            targets.push(target);
        }
        handle_npc_conversation(raw, targets, state, app).await;
        return;
    }

    let mentions = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        parish_core::ipc::extract_npc_mentions(&raw, &world, &npc_manager)
    };

    // Chip selections (real names from the frontend) come first, then any
    // inline @mentions that aren't already in the chip set. Deduping happens
    // in `resolve_npc_targets` via `find_by_name`, which matches both real
    // and display names.
    // See note above. Pre-allocate at the fixed upper bound so the
    // allocation argument is a constant — independent of any user-controlled
    // input — and CodeQL's data-flow analyzer can see that.
    let mut targets: Vec<String> = Vec::with_capacity(MAX_TARGETS);
    for name in addressed_to {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }
    for name in mentions.names {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }

    handle_npc_conversation(mentions.remaining, targets, state, app).await;
}

/// Resolves movement to a named location using the shared movement pipeline.
///
/// Delegates all state mutation and message generation to
/// [`parish_core::game_session::apply_movement`], then emits the returned
/// effects to the frontend.
async fn handle_movement(target: &str, state: &Arc<AppState>, app: &tauri::AppHandle) {
    use parish_core::game_session::{
        apply_movement, enrich_travel_encounter, roll_travel_encounter,
    };

    let transport = state.transport.default_mode().clone();

    // Apply all movement state changes within a single lock scope to prevent
    // TOCTOU races.
    let (effects, rolled_encounter) = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let effects = apply_movement(
            &mut world,
            &mut npc_manager,
            &state.reaction_templates,
            target,
            &transport,
        );
        let rolled = if effects.world_changed {
            let config = state.config.lock().await;
            if !config.flags.is_disabled("travel-encounters") {
                roll_travel_encounter(&world, &effects)
            } else {
                None
            }
        } else {
            None
        };
        (effects, rolled)
    };

    // Resolve encounter text — LLM-enriched when a reaction client exists
    // and the `travel-encounters-llm` flag is not explicitly disabled.
    let encounter_line: Option<String> = if let Some(rolled) = rolled_encounter.as_ref() {
        let llm_enabled = {
            let cfg = state.config.lock().await;
            !cfg.flags.is_disabled("travel-encounters-llm")
        };
        let (reaction_client, reaction_model) = if llm_enabled {
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;
            config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref())
        } else {
            (None, String::new())
        };
        let text = if let Some(client) = reaction_client.as_ref() {
            enrich_travel_encounter(rolled, client, &reaction_model, 15).await
        } else {
            rolled.canned.text.clone()
        };
        let formatted = format!("  · {text}");
        {
            let mut world = state.world.lock().await;
            world.log(formatted.clone());
        }
        Some(formatted)
    } else {
        None
    };

    // Emit travel-start animation payload first
    if let Some(travel_payload) = &effects.travel_start {
        let _ = app.emit(EVENT_TRAVEL_START, travel_payload);
    }

    // Emit all player-visible messages in order
    for msg in &effects.messages {
        tracing::info!(source = %msg.source, text = %msg.text.trim(), "chat");
        let payload = match msg.subtype {
            Some(st) => text_log_typed(msg.source, &msg.text, st),
            None => text_log(msg.source, &msg.text),
        };
        let _ = app.emit(EVENT_TEXT_LOG, payload);
    }

    // Emit travel encounter line if one fired
    if let Some(line) = encounter_line {
        let _ = app.emit(EVENT_TEXT_LOG, text_log("system", &line));
    }

    // Emit NPC arrival reactions — stream gradually like normal NPC dialogue
    if !effects.arrival_reactions.is_empty() {
        use parish_core::game_session::stream_reaction_texts;

        let (
            all_npcs,
            current_location_id,
            loc_name,
            tod,
            weather,
            introduced,
            reaction_client,
            reaction_model,
        ) = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;
            let (rc, rm) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
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
                rc,
                rm,
            )
        };

        stream_reaction_texts(
            &effects.arrival_reactions,
            &all_npcs,
            current_location_id,
            &loc_name,
            tod,
            &weather,
            &introduced,
            reaction_client.as_ref(),
            &reaction_model,
            Some(&state.inference_log),
            &state.language_settings,
            |turn_id, npc_name| {
                // Use `text_log_for_stream_turn` so the UI's streaming-
                // placeholder guard recognises this entry and can finalise
                // (remove) it when the per-turn `stream-turn-end` fires with
                // no tokens — otherwise an empty bubble lingers in the chat
                // (#984 follow-up: "blank NPC reply" reported on the
                // `just demo 2 10` run).
                let _ = app.emit(
                    EVENT_TEXT_LOG,
                    text_log_for_stream_turn(npc_name.to_string(), String::new(), turn_id),
                );
            },
            |turn_id, source, batch| {
                let _ = app.emit(
                    EVENT_STREAM_TOKEN,
                    StreamTokenPayload {
                        token: batch.to_string(),
                        turn_id,
                        source: source.to_string(),
                    },
                );
            },
            |turn_id| {
                let _ = app.emit(EVENT_STREAM_TURN_END, StreamTurnEndPayload { turn_id });
            },
        )
        .await;

        // Finalise the streaming state so the frontend marks the last entry done.
        let _ = app.emit(EVENT_STREAM_END, StreamEndPayload { hints: vec![] });
    }

    // Record tier transitions in the debug event log
    if !effects.tier_transitions.is_empty() {
        let ts = debug_event_timestamp(state).await;
        let mut debug_events = state.debug_events.lock().await;
        for tt in &effects.tier_transitions {
            if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
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

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
        let current_location = {
            let world = state.world.lock().await;
            world.player_location
        };
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(current_location);
        conversation.last_spoken_at = std::time::Instant::now();
        drop(conversation);

        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
        let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    }
}

/// Renders the current location description and exits.
async fn handle_look(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let text = parish_core::ipc::render_look_text(
        &world,
        &npc_manager,
        transport.speed_m_per_s,
        &transport.label,
        false,
    );
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: text,
            subtype: None,
        },
    );
}

/// Helper: sets `conversation_in_progress` on the conversation mutex.
///
/// Only used in unit tests; production code uses the `GameLoopContext`-based
/// shared orchestration which manages this flag internally.
#[cfg(test)]
async fn set_conversation_running(state: &Arc<AppState>, running: bool) {
    let mut conversation = state.conversation.lock().await;
    conversation.conversation_in_progress = running;
}

/// Routes input to one or more NPCs at the player's location, or shows an idle message.
///
/// Delegates to [`parish_core::game_loop::handle_npc_conversation`] for all
/// shared logic (#696), then emits a world-update snapshot when inference
/// finishes.
async fn handle_npc_conversation(
    raw: String,
    target_names: Vec<String>,
    state: &Arc<AppState>,
    app: tauri::AppHandle,
) {
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
    };

    let app_for_loading = app.clone();
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        crate::events::spawn_loading_animation(app_for_loading.clone(), cancel.clone());
        Some(cancel)
    };

    emit_world_update(state, &app).await;
    parish_core::game_loop::handle_npc_conversation(&ctx, raw, target_names, spawn_loading).await;
    emit_world_update(state, &app).await;
}

/// Delegates to [`parish_core::game_loop::run_idle_banter`] for all shared
/// logic (#696), then emits a world-update snapshot when the sequence ends.
async fn run_idle_banter(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
    };

    emit_world_update(state, app).await;
    // Idle banter spawns no loading animation.
    parish_core::game_loop::run_idle_banter(&ctx, || None).await;
    emit_world_update(state, app).await;
}

pub(crate) async fn tick_inactivity(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let (last_player_activity, last_spoken_at, running, idle_after, auto_pause_after) = {
        let conversation = state.conversation.lock().await;
        let config = state.config.lock().await;
        (
            conversation.last_player_activity,
            conversation.last_spoken_at,
            conversation.conversation_in_progress,
            config.idle_banter_after_secs,
            config.auto_pause_after_secs,
        )
    };

    if running {
        return;
    }

    let world_state = {
        let world = state.world.lock().await;
        (
            world.clock.is_paused(),
            world.clock.is_inference_paused(),
            world.player_location,
        )
    };

    if world_state.0 || world_state.1 {
        return;
    }

    {
        let mut conversation = state.conversation.lock().await;
        conversation.sync_location(world_state.2);
    }

    let now = std::time::Instant::now();
    let player_idle = now.duration_since(last_player_activity).as_secs();
    let speech_idle = now.duration_since(last_spoken_at).as_secs();

    if player_idle >= auto_pause_after {
        {
            let mut world = state.world.lock().await;
            if world.clock.is_paused() || world.clock.is_inference_paused() {
                return;
            }
            world.clock.pause();
        }
        let _ = app.emit(
            EVENT_TEXT_LOG,
            TextLogPayload {
                id: String::new(),
                stream_turn_id: None,
                source: "system".into(),
                content:
                    "The parish falls quiet after a full minute of silence. Time is now paused."
                        .to_string(),
                subtype: None,
            },
        );
        emit_world_update(state, app).await;
        let mut conversation = state.conversation.lock().await;
        conversation.last_spoken_at = now;
        return;
    }

    if player_idle >= idle_after && speech_idle >= idle_after {
        run_idle_banter(state, app).await;
    }
}

// ── Persistence commands ────────────────────────────────────────────────────

use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

/// Returns the list of save files with branch metadata.
#[tauri::command]
pub async fn discover_save_files(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileInfo>, String> {
    let world = state.world.lock().await;
    let saves = discover_saves(&state.saves_dir, &world.graph);
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

/// Internal save implementation — delegates to the shared canonical impl (#696).
pub(crate) async fn do_save_game(state: &Arc<AppState>) -> Result<String, String> {
    parish_core::game_loop::do_save_game(
        &state.world,
        &state.npc_manager,
        &state.save_path,
        &state.current_branch_id,
        &state.current_branch_name,
        &state.saves_dir,
    )
    .await
}

/// Loads a branch from a save file, restoring world and NPC state.
#[tauri::command]
pub async fn load_branch(
    file_path: String,
    branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_load_branch(state.inner(), &app, file_path, branch_id).await
}

/// Internal load-branch implementation shared with the MCP bridge.
///
/// Takes plain `&Arc<AppState>` / `&tauri::AppHandle` so it can be called
/// from non-Tauri-extractor contexts (e.g. `mcp_bridge::load_branch_route`).
pub async fn do_load_branch(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    file_path: String,
    branch_id: i64,
) -> Result<(), String> {
    use parish_core::persistence::SaveFileLock;

    let path = std::path::PathBuf::from(&file_path);

    // If switching to a different save file, acquire a new lock first.
    let current_path = state.save_path.lock().await.clone();
    let switching_files = current_path.as_ref() != Some(&path);

    if switching_files {
        let lock = SaveFileLock::try_acquire(&path)
            .ok_or_else(|| "This save file is in use by another instance.".to_string())?;
        // Release old lock and store new one.
        *state.save_lock.lock().await = Some(lock);
    }

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
        .map_err(|e| e.to_string())??;

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
    let ws = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
    drop(npc_manager);
    let _ = app.emit(EVENT_WORLD_UPDATE, ws);
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: format!("Loaded {} (branch: {}).", filename, branch_name),
            subtype: None,
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
pub async fn do_create_branch(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };

    tracing::info!(
        "Creating branch '{}' with parent {} in {:?}",
        name,
        parent_branch_id,
        db_path
    );

    // Capture snapshot before spawn_blocking so the tokio locks are not held across it.
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let name_owned = name.to_string();
    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let new_id = db
            .create_branch(&name_owned, Some(parent_branch_id))
            .map_err(|e| {
                tracing::error!("create_branch failed: {}", e);
                e.to_string()
            })?;
        tracing::info!("Branch '{}' created with id {}", name_owned, new_id);
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        tracing::info!("Snapshot saved to branch '{}'", name_owned);
        Ok(new_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    // Switch to the new branch
    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.to_string());

    Ok(format!("Created new branch '{}'.", name))
}

/// Creates a new save file and saves the current state.
#[tauri::command]
pub async fn new_save_file(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    use parish_core::persistence::SaveFileLock;

    let path = new_save_path(&state.saves_dir);

    // Acquire lock on the new save file, releasing any previous lock.
    let lock = SaveFileLock::try_acquire(&path)
        .ok_or_else(|| "Could not lock the new save file.".to_string())?;
    *state.save_lock.lock().await = Some(lock);

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or("Failed to create main branch")?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())??;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Internal helper that reloads world/NPCs and creates a fresh save file.
///
/// Called both by the `new_game` Tauri command and the `CommandEffect::NewGame`
/// handler.  Delegates to the shared `parish_core::game_loop::do_new_game` (#696).
pub async fn do_new_game(state: &Arc<AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    use parish_core::game_loop::{NewGameParams, do_new_game as core_do_new_game};

    // Rediscover the active game mod (Tauri AppState does not cache it).
    let game_mod = parish_core::game_mod::find_default_mod()
        .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok());

    let emitter = crate::events::TauriEmitter::new(app.clone());
    core_do_new_game(NewGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        conversation: &state.conversation,
        save_path: &state.save_path,
        current_branch_id: &state.current_branch_id,
        current_branch_name: &state.current_branch_name,
        saves_dir: &state.saves_dir,
        game_mod: game_mod.as_ref(),
        data_dir: &state.data_dir,
        pronunciations: &state.pronunciations,
        default_transport: state.transport.default_mode(),
        emitter: &emitter,
    })
    .await
}

/// Starts a brand new game: reloads world and NPCs from data files,
/// creates a new save file, and saves the fresh initial state.
#[tauri::command]
pub async fn new_game(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_new_game(&state, &app).await?;
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: "A new chapter begins in the parish...".to_string(),
            subtype: None,
        },
    );
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

// ── Screenshot capture (player-triggered, MCP-readable) ──────────────────────

/// Metadata describing the most-recently-saved screenshot.
///
/// The image is written to `<saves_dir>/screenshots/parish-<ISO-timestamp>.png`
/// by the Svelte frontend (which calls `html-to-image` and posts the resulting
/// `data:image/png;base64,...` URL to the `save_screenshot` command). Once
/// written, the absolute path is cached on `AppState::latest_screenshot_path`
/// so the `parish_latest_screenshot` MCP tool can report it without scanning
/// the directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotInfo {
    /// Absolute filesystem path to the PNG.
    pub path: String,
    /// ISO-8601 UTC timestamp the file was written (`YYYY-MM-DDTHH:MM:SSZ`).
    pub taken_at: String,
    /// Size of the PNG payload in bytes.
    pub size_bytes: u64,
}

/// Decodes a `data:image/png;base64,...` URL into the raw PNG byte payload.
///
/// Returns `Err` if the URL is malformed, has the wrong MIME type, or the
/// base64 segment fails to decode.
pub fn decode_data_url_png(data_url: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    const PREFIX: &str = "data:image/png;base64,";
    let b64 = data_url
        .strip_prefix(PREFIX)
        .ok_or_else(|| format!("expected data URL to start with `{PREFIX}`"))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| format!("base64 decode failed: {e}"))
}

/// Writes the decoded PNG bytes under `<saves_dir>/screenshots/parish-<ISO>.png`
/// and returns the [`ScreenshotInfo`] metadata for the newly created file.
///
/// Pure on `(saves_dir, png_bytes, now)` — no AppState, no Tauri handle — so
/// it can be unit-tested in isolation. The `now` callback returns the UTC
/// timestamp used both in the filename and the response (it is parameterised
/// so tests can pin the value).
pub fn write_screenshot_to_disk(
    saves_dir: &std::path::Path,
    png_bytes: &[u8],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<ScreenshotInfo, String> {
    let dir = saves_dir.join("screenshots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;

    // Filenames must be filesystem-safe on every platform we support, so use
    // `-` instead of `:` in the time component. `format!("{:?}", ts)` would
    // include sub-second precision plus the trailing `Z`; we keep the stem
    // tidy by formatting with the second-precision strftime template.
    let stem = now.format("parish-%Y-%m-%dT%H-%M-%SZ").to_string();
    let path = dir.join(format!("{stem}.png"));

    std::fs::write(&path, png_bytes)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    let size_bytes = png_bytes.len() as u64;
    let path_string = path.to_string_lossy().to_string();
    let taken_at = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    Ok(ScreenshotInfo {
        path: path_string,
        taken_at,
        size_bytes,
    })
}

/// Internal save-screenshot implementation shared with the MCP bridge / web
/// route. Decodes the `data_url`, writes the PNG, and updates
/// [`AppState::latest_screenshot_path`].
pub(crate) async fn do_save_screenshot(
    state: &Arc<AppState>,
    data_url: String,
) -> Result<ScreenshotInfo, String> {
    let bytes = decode_data_url_png(&data_url)?;
    let info = write_screenshot_to_disk(&state.saves_dir, &bytes, chrono::Utc::now())?;
    *state.latest_screenshot_path.lock().await = Some(std::path::PathBuf::from(&info.path));
    Ok(info)
}

/// Internal latest-screenshot reader shared with the MCP bridge / web route.
///
/// Re-stat`s the file each call so a screenshot deleted out from under the
/// session is reported as missing rather than reused indefinitely.
pub(crate) async fn do_get_latest_screenshot(
    state: &Arc<AppState>,
) -> Result<Option<ScreenshotInfo>, String> {
    let Some(path) = state.latest_screenshot_path.lock().await.clone() else {
        return Ok(None);
    };
    // Use tokio::fs::metadata so the stat call doesn't block the async
    // executor under load (this handler may be invoked from the MCP bridge
    // while other Tokio tasks are waiting on the same worker thread).
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let modified = metadata
        .modified()
        .map_err(|e| format!("stat({}): {e}", path.display()))?;
    let taken_at = chrono::DateTime::<chrono::Utc>::from(modified)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    Ok(Some(ScreenshotInfo {
        path: path.to_string_lossy().to_string(),
        taken_at,
        size_bytes: metadata.len(),
    }))
}

/// Persists a screenshot captured by the frontend.
///
/// `data_url` is a `data:image/png;base64,...` string produced by
/// `html-to-image` in the Svelte UI. The PNG is decoded and written to
/// `<saves_dir>/screenshots/parish-<ISO-timestamp>.png`; the resulting path
/// is cached on [`AppState::latest_screenshot_path`] so the MCP
/// `parish_latest_screenshot` tool can read it back without rescanning.
#[tauri::command]
pub async fn save_screenshot(
    data_url: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ScreenshotInfo, String> {
    do_save_screenshot(&state, data_url).await
}

/// Reads metadata for the most recently captured screenshot, if any.
///
/// Returns `Ok(None)` when no screenshot has been captured this session, or
/// when the cached path no longer exists on disk.
#[tauri::command]
pub async fn get_latest_screenshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<ScreenshotInfo>, String> {
    do_get_latest_screenshot(&state).await
}

/// Agent-triggered screenshot capture. Registered as a Tauri command for
/// wiring-parity with the `/api/take-screenshot` bridge route.
///
/// In Tauri mode this is not invoked directly by the frontend — use the MCP
/// tool `parish_take_screenshot` instead. The bridge handler also calls
/// `do_take_screenshot` directly for a shared implementation.
#[tauri::command]
pub async fn take_screenshot(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    do_take_screenshot(&state, &app).await
}

/// Shared implementation for agent-triggered screenshot capture.
///
/// Generates a UUID request ID, stashes a `oneshot::Sender` in
/// `AppState::pending_screenshots`, emits `request-screenshot` to the live
/// frontend window, and awaits the result for up to 15 seconds.
///
/// On emit failure the pending entry is cleaned up immediately so the map
/// does not grow unbounded. On timeout the entry is also removed.
/// The frontend delivers the result via `notify_screenshot_captured` (success)
/// or `notify_screenshot_error` (failure).
pub(crate) async fn do_take_screenshot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    use tokio::sync::oneshot;
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = state.pending_screenshots.lock().await;
        pending.insert(request_id.clone(), tx);
    }
    if let Err(e) = app.emit(
        crate::events::EVENT_REQUEST_SCREENSHOT,
        serde_json::json!({"request_id": request_id}),
    ) {
        let mut pending = state.pending_screenshots.lock().await;
        pending.remove(&request_id);
        return Err(e.to_string());
    }
    match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("screenshot request cancelled".into()),
        Err(_) => {
            let mut pending = state.pending_screenshots.lock().await;
            pending.remove(&request_id);
            Err("screenshot capture timed out after 15 s".into())
        }
    }
}

/// Called by the frontend after successfully capturing a screenshot in
/// response to a `request-screenshot` Tauri event. Delivers the
/// `ScreenshotInfo` through the pending oneshot channel so the waiting
/// bridge handler can return it to the MCP client.
///
/// Internal round-trip callback — no HTTP equivalent on `parish-server`.
#[tauri::command]
pub async fn notify_screenshot_captured(
    request_id: String,
    info: ScreenshotInfo,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut pending = state.pending_screenshots.lock().await;
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(Ok(info));
    }
    Ok(())
}

/// Called by the frontend when screenshot capture fails (e.g. `html-to-image`
/// error or `save_screenshot` IPC error). Immediately unblocks the waiting
/// bridge handler with the error message instead of letting it time out.
///
/// Internal round-trip callback — no HTTP equivalent on `parish-server`.
#[tauri::command]
pub async fn notify_screenshot_error(
    request_id: String,
    error: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut pending = state.pending_screenshots.lock().await;
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(Err(error));
    }
    Ok(())
}

/// Formats branch list as text for the /branches command.
pub async fn do_list_branches_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let current_id = *state.current_branch_id.lock().await;

    let branches = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.list_branches().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

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
pub async fn do_branch_log_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let bid = state
        .current_branch_id
        .lock()
        .await
        .ok_or("No active branch.")?;

    let log = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.branch_log(bid).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

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

// Snippet injection validation is shared via parish_core::game_loop::is_snippet_injection_char
// (#687 security parity). Delegating here guarantees server and Tauri use identical logic.
pub use parish_core::game_loop::is_snippet_injection_char;

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

    // Reject snippets that could inject content into NPC system prompts (#687).
    if message_snippet.chars().any(is_snippet_injection_char) {
        return Err("Message snippet contains disallowed characters.".to_string());
    }

    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log.add(&emoji, &message_snippet, now);
    }

    Ok(())
}

/// Delegates to [`parish_core::game_loop::emit_npc_reactions`] (#696 slice 5).
///
/// `location` must be the player's location **at the time the message was
/// sent**, captured before any `handle_game_input` call that might move the
/// player. This prevents a race where the player moves between spawn and
/// execution, causing reactions to be attributed to NPCs at the wrong location.
///
/// Pre-captures the NPC list, resolves the reaction client and feature flags,
/// constructs a `TauriEmitter`, and delegates to the shared implementation.
fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    location: LocationId,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    let state_clone = Arc::clone(state);
    let player_msg_id = player_msg_id.to_string();
    let player_input = player_input.to_string();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));

    // Persist callback: closes over Arc<AppState> and locks npc_manager to
    // record each reaction in the NPC's reaction_log (#403).
    let state_for_persist = Arc::clone(state);
    let persist: parish_core::game_loop::PersistReactionFn = std::sync::Arc::new(
        move |npc_name: String, emoji: String, player_input: String| {
            let state = Arc::clone(&state_for_persist);
            tokio::spawn(async move {
                let mut npc_manager = state.npc_manager.lock().await;
                if let Some(npc_mut) = npc_manager.find_by_name_mut(&npc_name) {
                    npc_mut.reaction_log.add_player_message_reaction(
                        &emoji,
                        &player_input,
                        chrono::Utc::now(),
                    );
                }
            });
        },
    );

    tokio::spawn(async move {
        // Pre-capture the NPC list at the given location (the player may have
        // moved by the time the background task runs).
        let (npcs_here, reaction_client, reaction_model, llm_enabled) = {
            let npc_manager = state_clone.npc_manager.lock().await;
            let config = state_clone.config.lock().await;
            let base_client = state_clone.client.lock().await;
            let npcs = npc_manager
                .npcs_at(location)
                .iter()
                .map(|npc| (*npc).clone())
                .collect::<Vec<_>>();
            let (client, model) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            let enabled = !config.flags.is_disabled("npc-llm-reactions");
            (npcs, client, model, enabled)
        };

        parish_core::game_loop::emit_npc_reactions(
            player_msg_id,
            player_input,
            npcs_here,
            reaction_client,
            reaction_model,
            llm_enabled,
            emitter,
            persist,
        );
    });
}

// ── Demo / auto-player commands ──────────────────────────────────────────────

/// A single NPC visible to the demo player at the current location.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoNpcInfo {
    pub name: String,
    pub description: String,
}

/// An adjacent location visible to the demo player.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoAdjacentLocation {
    pub name: String,
    pub travel_minutes: Option<u16>,
    pub visited: bool,
}

/// A snapshot of the game context passed to the LLM player each turn.
///
/// Backend fills all fields except `recent_log`; the frontend appends the
/// last 40 entries from the `textLog` store before calling `get_llm_player_action`.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct DemoContextSnapshot {
    pub location_name: String,
    pub location_description: String,
    pub game_time: String,
    pub season: String,
    pub weather: String,
    pub npcs_here: Vec<DemoNpcInfo>,
    pub adjacent: Vec<DemoAdjacentLocation>,
    pub recent_log: Vec<String>,
    pub extra_prompt: Option<String>,
}

/// Demo configuration returned by `get_demo_config`.
#[derive(serde::Serialize, Clone)]
pub struct DemoConfigPayload {
    pub auto_start: bool,
    pub extra_prompt: Option<String>,
    pub turn_pause_secs: f32,
    pub max_turns: Option<u32>,
}

/// Returns the demo configuration (CLI flags parsed at startup).
#[tauri::command]
pub async fn get_demo_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoConfigPayload, String> {
    let dc = &state.demo_config;
    Ok(DemoConfigPayload {
        auto_start: dc.auto_start,
        extra_prompt: dc.extra_prompt.clone(),
        turn_pause_secs: dc.turn_pause_secs,
        max_turns: dc.max_turns,
    })
}

/// Builds a context snapshot for the LLM demo player.
///
/// Returns location, time, weather, NPCs present, and adjacent locations.
/// The `recent_log` field is empty; the frontend fills it from the text log
/// store before passing the snapshot to `get_llm_player_action`.
#[tauri::command]
pub async fn get_demo_context(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<DemoContextSnapshot, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Lock order: world → npc_manager (matches AppState contract).
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;

    let player_loc = world.player_location;

    let location_name = world
        .graph
        .get(player_loc)
        .map(|d| d.name.clone())
        .unwrap_or_default();

    let location_description = if let Some(loc_data) = world.current_location_data() {
        parish_core::world::description::render_description(
            loc_data,
            world.clock.time_of_day(),
            &world.weather.to_string(),
            &[],
        )
    } else {
        String::new()
    };

    use chrono::Datelike;
    let now = world.clock.now();
    let time_of_day = world.clock.time_of_day();
    let game_time = format!(
        "{}, {} {} {}, {}",
        now.format("%A"),
        now.day(),
        now.format("%B"),
        now.year(),
        time_of_day,
    );
    let season = format!("{}", world.clock.season());
    let weather = world.weather.to_string();

    let npcs_here = npc_manager
        .npcs_at(player_loc)
        .iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            DemoNpcInfo {
                name: npc.display_name(introduced).to_string(),
                description: npc.occupation.clone(),
            }
        })
        .collect();

    let speed = state.transport.default_mode().speed_m_per_s;
    let adjacent = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(neighbor_id, _conn)| {
            let name = world
                .graph
                .get(neighbor_id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("Location {}", neighbor_id.0));
            let travel_minutes = Some(world.graph.edge_travel_minutes(
                player_loc,
                neighbor_id,
                speed,
            ));
            let visited = world.visited_locations.contains(&neighbor_id);
            DemoAdjacentLocation {
                name,
                travel_minutes,
                visited,
            }
        })
        .collect();

    let extra_prompt = state.demo_config.extra_prompt.clone();

    Ok(DemoContextSnapshot {
        location_name,
        location_description,
        game_time,
        season,
        weather,
        npcs_here,
        adjacent,
        recent_log: Vec::new(),
        extra_prompt,
    })
}

/// Extracts the player action from an LLM response.
///
/// Handles four patterns:
/// 1. Completion: model received `{"action": "` and completed it — response is
///    something like `go to the mill"}`. Extract up to the closing quote.
/// 2. Full JSON: model output `{"action": "go to the mill"}` — scan for `{`
///    and JSON-parse from there.
/// 3. Envelope-leak: model emitted only the JSON suffix — e.g.
///    `action": "hello"}` or `hello"}` — strip the wrapper bits.
/// 4. Fallback: no JSON at all — strip thinking preamble, take last line.
fn extract_action_from_response(text: &str) -> String {
    // Strip thinking blocks first so all patterns operate on clean text.
    let stripped = strip_thinking_block(text);
    let trimmed = stripped.trim();

    // Pattern 1: fill-in-the-blank completion — response starts with the
    // action text and ends with `"}` or just `"`. The model completed
    // `{"action": "` → `go to the mill"}`.
    // We also handle `go to the mill"` (no closing brace).
    let completion = trimmed
        .trim_end_matches('}')
        .trim_end()
        .trim_end_matches('"')
        .trim();
    // Valid completion: no opening brace in the extracted text (it's pure action).
    if !completion.is_empty() && !completion.starts_with('{') && !completion.contains("action") {
        // Check that the raw response looked like a completion (no full JSON object).
        if !trimmed.contains("{\"action\"") && !trimmed.contains("{ \"action\"") {
            return completion.to_string();
        }
    }

    // Pattern 2: full JSON object anywhere in the response.
    let mut search = trimmed;
    while let Some(start) = search.find('{') {
        let candidate = &search[start..];
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(candidate)
            && let Some(action) = val.get("action").and_then(|v| v.as_str())
        {
            let action = action.trim();
            if !action.is_empty() {
                return action.to_string();
            }
        }
        search = &search[start + 1..];
    }

    // Pattern 3: envelope-leak — model emitted only the JSON suffix (no
    // matching `{...}` object) such as:
    //   action": "Good morning..."}
    //   "action": "Good morning..."}
    //   Good morning..."}
    // Strip a leading `[{][\s]*["]?action["]\s*:\s*"` prefix and a trailing
    // `"\s*}` (or bare `"}`) suffix, then return the inner text.
    if let Some(cleaned) = strip_envelope_leak(trimmed)
        && !cleaned.is_empty()
    {
        return cleaned;
    }

    // Pattern 4: fallback — take last meaningful line from already-stripped text.
    trimmed.trim_matches('"').trim_matches('\'').to_string()
}

/// Strips a leaked JSON envelope from `text`. Returns `Some(inner)` when at
/// least one envelope marker (a leading `action":` prefix or a trailing `"}`
/// suffix) was found and removed. Returns `None` if neither side looks like
/// a leak, so the caller can apply its own fallback.
fn strip_envelope_leak(text: &str) -> Option<String> {
    let mut s = text.trim();
    let mut changed = false;

    // Leading wrapper: optional `{`, optional whitespace, optional `"`,
    // literal `action`, optional `"`, whitespace, `:`, whitespace, `"`.
    // We hand-roll this instead of pulling a regex dep.
    let original = s;
    let mut rest = s;
    rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix('{') {
        rest = stripped.trim_start();
    }
    if let Some(stripped) = rest.strip_prefix('"') {
        rest = stripped;
    }
    if let Some(stripped) = rest.strip_prefix("action") {
        let mut after = stripped;
        if let Some(stripped) = after.strip_prefix('"') {
            after = stripped;
        }
        after = after.trim_start();
        if let Some(stripped) = after.strip_prefix(':') {
            let mut after = stripped.trim_start();
            if let Some(stripped) = after.strip_prefix('"') {
                after = stripped;
                s = after;
                changed = true;
            }
        }
    }
    if !changed {
        s = original;
    }

    // Trailing wrapper: optional `}`, whitespace, `"` (in reverse order
    // since we work from the end).
    let original_tail = s;
    let mut tail = s.trim_end();
    if let Some(stripped) = tail.strip_suffix('}') {
        tail = stripped.trim_end();
    }
    if let Some(stripped) = tail.strip_suffix('"') {
        tail = stripped;
        s = tail;
        changed = true;
    } else {
        s = original_tail;
    }

    if changed {
        Some(s.trim().to_string())
    } else {
        None
    }
}

/// Strips reasoning preamble from LLM responses so only the action remains.
///
/// Handles two patterns:
/// 1. Tagged blocks: `<thinking>...</thinking>` / `<think>...</think>` from
///    reasoning models (deepseek-r1, qwq). Takes everything after the last
///    closing tag.
/// 2. Plain-text multi-paragraph reasoning: if the response has blank-line-
///    separated paragraphs, takes the last paragraph. This covers models that
///    output reasoning prose before the final action without tags.
///
/// Falls back to the full trimmed text if neither pattern applies.
fn strip_thinking_block(text: &str) -> &str {
    let trimmed = text.trim();

    // Strip tagged thinking blocks first.
    for close_tag in &["</thinking>", "</think>"] {
        if let Some(pos) = trimmed.rfind(close_tag) {
            let after = trimmed[pos + close_tag.len()..].trim();
            if !after.is_empty() {
                return after;
            }
        }
    }

    // If the response has multiple blank-line-separated paragraphs, take the
    // last one — reasoning models often output rationale before the action.
    if let Some(last_para) = trimmed.rsplit("\n\n").find(|p| !p.trim().is_empty()) {
        let candidate = last_para.trim();
        // Only use the last paragraph if it looks like a short action (≤ 3
        // lines), not if the whole thing is one paragraph of prose.
        let line_count = candidate.lines().count();
        if line_count <= 3 && candidate.len() < trimmed.len() {
            return candidate;
        }
    }

    // Last-line fallback: models sometimes separate reasoning from action with
    // a single newline. If the last non-empty line is much shorter than the
    // full response, treat it as the action.
    let lines: Vec<&str> = trimmed.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() > 1
        && let Some(&last) = lines.last()
    {
        let last = last.trim();
        if last.len() <= 200 && last.len() < trimmed.len() {
            return last;
        }
    }

    trimmed
}

/// Asks the LLM to choose the next player action given the current game context.
///
/// The frontend fills `ctx.recent_log` from the text log store before calling
/// this command. Returns the trimmed action string.
#[tauri::command]
pub async fn get_llm_player_action(
    ctx: DemoContextSnapshot,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    {
        let config = state.config.lock().await;
        if !config.flags.is_enabled("demo-mode") {
            return Err("Demo mode is not active.".to_string());
        }
    }

    // Resolve client and model (base client; no per-category override for demo).
    let (client_opt, model) = {
        let config = state.config.lock().await;
        let client_guard = state.client.lock().await;
        let model = config.model_name.clone();
        let client = client_guard.as_ref().cloned();
        (client, model)
    };

    let Some(client) = client_opt else {
        return Err("No LLM client configured.".to_string());
    };

    let extra_section = ctx
        .extra_prompt
        .as_deref()
        .map(|p| format!("\n\n{}", p))
        .unwrap_or_default();

    let system_prompt = format!(
        "You are playing Rundale, an Irish living-world simulation set in 1820. You are a \
wandering stranger exploring the townlands of east Roscommon. The world is populated by \
historical Irish villagers — farmers, priests, weavers, matchmakers — each living their \
own life.\n\
\n\
Date: 1820. Catholic Emancipation: 1829 (not yet). Famine: 1845 (not yet).\n\
\n\
Speak as a 1820 traveller would: plain, short, period-appropriate. Avoid modern words \
like: fascinating, amazing, definitely, totally, decided to visit, taking in the sights, \
healing properties.\n\
\n\
Explore naturally: talk to people, learn their stories, travel between locations, and \
respond to whatever you encounter. Act as a curious outsider would.{extra}\n\
\n\
Respond with a JSON object containing a single field \"action\" — the text the player \
would type into the game. Do NOT use meta-commands like \"talk to X\"; write the actual \
words or command directly.\n\
\n\
Examples:\n\
  {{\"action\": \"Good mornin'. Might I look about the village a while?\"}}\n\
  {{\"action\": \"I've come from up the road. What news do ye have hereabouts?\"}}\n\
  {{\"action\": \"go to the mill\"}}\n\
  {{\"action\": \"look\"}}\n\
  {{\"action\": \"ask about the harvest\"}}\n\
\n\
Your entire response must be a single JSON object — nothing before or after it.",
        extra = extra_section,
    );

    let mut user_parts: Vec<String> = Vec::new();
    user_parts.push(format!("Location: {}", ctx.location_name));
    if !ctx.location_description.is_empty() {
        user_parts.push(ctx.location_description.clone());
    }
    user_parts.push(format!("Date and time: {} | {}", ctx.game_time, ctx.season));
    user_parts.push(format!("Weather: {}", ctx.weather));

    if ctx.npcs_here.is_empty() {
        user_parts.push("NPCs here: none".to_string());
    } else {
        let npc_lines: Vec<String> = ctx
            .npcs_here
            .iter()
            .map(|n| format!("  - {} ({})", n.name, n.description))
            .collect();
        user_parts.push(format!("NPCs here:\n{}", npc_lines.join("\n")));
    }

    if !ctx.adjacent.is_empty() {
        let adj_lines: Vec<String> = ctx
            .adjacent
            .iter()
            .map(|a| {
                let mins = a
                    .travel_minutes
                    .map(|m| format!("{} min", m))
                    .unwrap_or_else(|| "? min".to_string());
                let vis = if a.visited { "visited" } else { "unvisited" };
                format!("  - {} ({}, {})", a.name, mins, vis)
            })
            .collect();
        user_parts.push(format!("Adjacent locations:\n{}", adj_lines.join("\n")));
    }

    if !ctx.recent_log.is_empty() {
        let log_lines: Vec<String> = ctx.recent_log.iter().map(|l| format!("> {}", l)).collect();
        user_parts.push(format!("Recent events:\n{}", log_lines.join("\n")));
    }

    // Fill-in-the-blank technique: end the prompt with an incomplete JSON
    // object so the model completes the string rather than reasoning about it.
    user_parts.push("Action (complete the JSON):\n{\"action\": \"".to_string());

    let user_prompt = user_parts.join("\n\n");

    let raw = client
        .generate(
            &model,
            &user_prompt,
            Some(&system_prompt),
            Some(200),
            Some(0.9),
        )
        .await
        .map_err(|e| e.to_string())?;

    // Primary: extract the "action" field from JSON output.
    // The system prompt asks for {"action": "..."}, which is robust against
    // any amount of preamble or reasoning text the model emits before it.
    let action_text = extract_action_from_response(&raw);
    tracing::info!(
        location = %ctx.location_name,
        raw_len = raw.len(),
        action = %action_text,
        "demo turn: LLM chose action"
    );

    // Quality sensors — emit WARN on any structural issue in the parsed
    // player action. These don't gate execution; they surface bugs in the
    // demo log so the judging pass can pick them up.
    for issue in parish_core::npc::quality::detect_all_text_issues(&action_text) {
        tracing::warn!(
            site = "demo-player-action",
            kind = issue.kind.as_str(),
            detail = %issue.detail,
            "quality issue in LLM player action"
        );
    }

    Ok(action_text)
}

#[cfg(test)]
mod demo_tests {
    use super::{extract_action_from_response, strip_thinking_block};

    #[test]
    fn extracts_action_from_json() {
        let input = r#"{"action": "Good morning! How are you today?"}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Good morning! How are you today?"
        );
    }

    #[test]
    fn extracts_action_from_json_after_preamble() {
        let input = "Let me think... However, we are a wandering stranger.\n{\"action\": \"go to the crossroads\"}";
        assert_eq!(extract_action_from_response(input), "go to the crossroads");
    }

    #[test]
    fn extracts_action_from_json_after_thinking_tags() {
        let input = "<think>reasoning here</think>\n{\"action\": \"look\"}";
        assert_eq!(extract_action_from_response(input), "look");
    }

    #[test]
    fn falls_back_to_stripping_when_no_json() {
        let input = "Some reasoning.\nask about the harvest";
        assert_eq!(extract_action_from_response(input), "ask about the harvest");
    }

    #[test]
    fn strips_envelope_leak_with_action_prefix() {
        // Live demo bug: model emits only the JSON suffix, no opening `{`.
        let input = r#"action": "Good morning, Peig Hannigan. My name is [Your Name]. I'm just wandering."}"#;
        assert_eq!(
            extract_action_from_response(input),
            "Good morning, Peig Hannigan. My name is [Your Name]. I'm just wandering."
        );
    }

    #[test]
    fn strips_trailing_envelope_suffix() {
        // Live demo bug: model leaves only the trailing `"}` after a clean reply.
        let input = r#"I'm just curious about the community."}"#;
        assert_eq!(
            extract_action_from_response(input),
            "I'm just curious about the community."
        );
    }

    #[test]
    fn strips_thinking_block_before_action() {
        let input =
            "<thinking>\nI should greet the farmer.\n</thinking>\nHello there, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello there, good morning!");
    }

    #[test]
    fn strips_think_tag_variant() {
        let input = "<think>reasoning</think>\ngo to the mill";
        assert_eq!(strip_thinking_block(input), "go to the mill");
    }

    #[test]
    fn no_thinking_block_returns_trimmed() {
        let input = "  ask Brigid about the harvest  ";
        assert_eq!(strip_thinking_block(input), "ask Brigid about the harvest");
    }

    #[test]
    fn only_thinking_block_falls_back_to_full() {
        let input = "<thinking>just thinking, nothing after</thinking>";
        assert_eq!(
            strip_thinking_block(input),
            "<thinking>just thinking, nothing after</thinking>"
        );
    }

    #[test]
    fn uses_last_closing_tag_for_nested() {
        let input = "<thinking>outer <think>inner</think> more</thinking>\nlook around";
        assert_eq!(strip_thinking_block(input), "look around");
    }

    #[test]
    fn strips_plain_text_reasoning_before_action() {
        let input = "Looking at the context, I see Peig is here. I should greet her warmly.\n\nHello Peig, good morning!";
        assert_eq!(strip_thinking_block(input), "Hello Peig, good morning!");
    }

    #[test]
    fn single_paragraph_returned_as_is() {
        let input = "Hello Seamus, how goes the harvest?";
        assert_eq!(strip_thinking_block(input), input);
    }

    #[test]
    fn strips_single_newline_reasoning_before_action() {
        let input = "I need to explore. The crossroads is nearby.\ngo to the crossroads";
        assert_eq!(strip_thinking_block(input), "go to the crossroads");
    }

    #[test]
    fn strips_multi_sentence_reasoning_single_newline() {
        let input = "Based on my previous interaction with Peig, I should explore. The mill is unvisited.\nask about the mill";
        assert_eq!(strip_thinking_block(input), "ask about the mill");
    }
}

#[cfg(test)]
mod cmd_tests {
    use super::*;
    use std::sync::Arc;

    /// Builds a minimal [`AppState`] for unit tests — matches the structure
    /// used in `parish-server` tests (`routes::tests::test_app_state`).
    fn test_app_state() -> Arc<AppState> {
        use crate::{
            AppState, ConversationRuntimeState, DEBUG_EVENT_CAPACITY, DemoConfig, GameConfig,
            UiConfigSnapshot,
        };
        use parish_core::inference::new_inference_log;
        use parish_core::npc::manager::NpcManager;
        use parish_core::world::transport::TransportConfig;
        use parish_core::world::{DEFAULT_START_LOCATION, WorldState};
        use tokio::sync::Mutex;
        use tokio_util::sync::CancellationToken;

        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let world =
            WorldState::from_parish_file(&data_dir.join("world.json"), DEFAULT_START_LOCATION)
                .unwrap();
        let npc_manager = NpcManager::new();
        let transport = TransportConfig::default();
        let ui_config = UiConfigSnapshot {
            hints_label: "test".to_string(),
            default_accent: "#000".to_string(),
            splash_text: String::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            auto_pause_timeout_seconds: 300,
        };
        let theme_palette = parish_core::game_mod::default_theme_palette();
        let pronunciations = Vec::new();
        let reaction_templates = parish_core::npc::reactions::ReactionTemplates::default();
        let game_config = GameConfig {
            provider_name: String::new(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            flags: parish_core::config::FeatureFlags::default(),
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
        };
        let saves_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../saves");
        let shutdown_token = CancellationToken::new();
        let session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore> =
            std::sync::Arc::new(parish_core::session_store::DbSessionStore::new(
                saves_dir.clone(),
            ));

        Arc::new(AppState {
            world: Mutex::new(world),
            npc_manager: Mutex::new(npc_manager),
            inference_queue: Mutex::new(None),
            client: Mutex::new(None),
            cloud_client: Mutex::new(None),
            conversation: Mutex::new(ConversationRuntimeState::new()),
            debug_events: Mutex::new(std::collections::VecDeque::with_capacity(
                DEBUG_EVENT_CAPACITY,
            )),
            game_events: Mutex::new(std::collections::VecDeque::with_capacity(
                DEBUG_EVENT_CAPACITY,
            )),
            inference_log: new_inference_log(),
            ui_config,
            theme_palette,
            pronunciations,
            reaction_templates,
            save_path: Mutex::new(None),
            current_branch_id: Mutex::new(None),
            current_branch_name: Mutex::new(None),
            transport,
            data_dir: data_dir.clone(),
            saves_dir,
            latest_screenshot_path: Mutex::new(None),
            pending_screenshots: Mutex::new(std::collections::HashMap::new()),
            worker_handle: Mutex::new(None),
            editor: std::sync::Mutex::new(parish_core::ipc::editor::EditorSession::default()),
            save_lock: Mutex::new(None),
            runtime_processes: Mutex::new(parish_core::inference::client::RuntimeProcesses::none()),
            inference_config: parish_core::config::InferenceConfig::default(),
            setup_status: std::sync::Mutex::new(crate::SetupStatusSnapshot::default()),
            wizard_in_flight: std::sync::atomic::AtomicBool::new(false),
            language_settings: parish_core::npc::LanguageSettings::english_only(),
            config: Mutex::new(game_config),
            demo_config: DemoConfig::default(),
            shutdown_token,
            sim_cancel: Mutex::new(CancellationToken::new()),
            session_store,
            user_config_dir: std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            secret_store: std::sync::Arc::new(parish_core::secret_store::InMemorySecretStore::new()),
        })
    }

    // ── validate_input_text ─────────────────────────────────────────────────

    #[test]
    fn validate_input_accepts_normal_text() {
        assert!(validate_input_text("ask Brigid about the harvest").is_ok());
    }

    #[test]
    fn validate_input_trims_whitespace() {
        let result = validate_input_text("  hello  ").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn validate_input_allows_empty_after_trim() {
        assert!(validate_input_text("   ").is_ok());
    }

    #[test]
    fn validate_input_rejects_over_2000_chars() {
        let long: String = "a".repeat(2001);
        assert!(validate_input_text(&long).is_err());
    }

    #[test]
    fn validate_input_accepts_exactly_2000_chars() {
        let exactly: String = "a".repeat(2000);
        assert!(validate_input_text(&exactly).is_ok());
    }

    // ── validate_addressed_to ───────────────────────────────────────────────

    #[test]
    fn validate_addressed_to_accepts_empty_list() {
        assert!(validate_addressed_to(&[]).is_ok());
    }

    #[test]
    fn validate_addressed_to_accepts_up_to_10() {
        let names: Vec<String> = (0..10).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_ok());
    }

    #[test]
    fn validate_addressed_to_rejects_11_names() {
        let names: Vec<String> = (0..11).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_err());
    }

    #[test]
    fn validate_addressed_to_rejects_name_over_100_chars() {
        let long_name = "a".repeat(101);
        assert!(validate_addressed_to(&[long_name]).is_err());
    }

    #[test]
    fn validate_addressed_to_accepts_100_char_name() {
        let name = "a".repeat(100);
        assert!(validate_addressed_to(&[name]).is_ok());
    }

    // ── is_snippet_injection_char ───────────────────────────────────────────

    #[test]
    fn snippet_injection_rejects_double_quote() {
        assert!(is_snippet_injection_char('"'));
    }

    #[test]
    fn snippet_injection_rejects_backslash() {
        assert!(is_snippet_injection_char('\\'));
    }

    #[test]
    fn snippet_injection_rejects_line_separator() {
        assert!(is_snippet_injection_char('\u{2028}'));
    }

    #[test]
    fn snippet_injection_rejects_paragraph_separator() {
        assert!(is_snippet_injection_char('\u{2029}'));
    }

    #[test]
    fn snippet_injection_rejects_null_byte() {
        assert!(is_snippet_injection_char('\0'));
    }

    #[test]
    fn snippet_injection_accepts_normal_chars() {
        for c in "abcdefghijklmnopqrstuvwxyz ÁÉÍÓÚ,.!?'".chars() {
            assert!(
                !is_snippet_injection_char(c),
                "char {:?} should be allowed",
                c
            );
        }
    }

    // ── AppState save state on fresh state ─────────────────────────────────

    #[tokio::test]
    async fn save_state_initial_is_empty() {
        let state = test_app_state();
        let save_path = state.save_path.lock().await;
        let branch_id = state.current_branch_id.lock().await;
        let branch_name = state.current_branch_name.lock().await;

        let filename = save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        assert!(filename.is_none(), "fresh state should have no save file");
        assert!(branch_id.is_none(), "fresh state should have no branch id");
        assert!(
            branch_name.is_none(),
            "fresh state should have no branch name"
        );
    }

    // ── conversation state ──────────────────────────────────────────────────

    #[tokio::test]
    async fn set_conversation_running_toggles_flag() {
        let state = test_app_state();

        // Initially not running
        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }

        set_conversation_running(&state, true).await;

        {
            let conv = state.conversation.lock().await;
            assert!(conv.conversation_in_progress);
        }

        set_conversation_running(&state, false).await;

        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }
    }

    // ── tick_inactivity does nothing when paused ────────────────────────────

    #[tokio::test]
    async fn world_clock_paused_state_has_expected_invariants() {
        let state = test_app_state();

        // Pause the world clock
        {
            let mut world = state.world.lock().await;
            world.clock.pause();
        }

        // tick_inactivity needs an AppHandle which we can't construct in unit
        // tests.  The early-return path (world is paused) never touches app,
        // so we exercise the banter-after-silence guard indirectly via
        // conversation state: conversation_in_progress=false, clock paused →
        // the guard returns immediately without calling run_idle_banter.
        // We verify world state is unchanged.
        let (paused_before, loc_before) = {
            let world = state.world.lock().await;
            (world.clock.is_paused(), world.player_location)
        };
        assert!(paused_before, "clock should be paused before tick");

        // We can't call tick_inactivity here because it needs tauri::AppHandle.
        // Instead, confirm the state invariants hold so future tests that mock
        // AppHandle can call tick_inactivity against this base.
        let paused_after = {
            let world = state.world.lock().await;
            world.clock.is_paused()
        };
        assert!(paused_after, "paused flag should still be set");
        assert_eq!(
            state.world.lock().await.player_location,
            loc_before,
            "location should be unchanged"
        );
    }

    // ── world snapshot consistency ─────────────────────────────────────────

    #[tokio::test]
    async fn world_state_loads_kilteevan_as_start_location() {
        let state = test_app_state();
        let world = state.world.lock().await;
        let loc_name = world
            .current_location_data()
            .map(|d| d.name.as_str())
            .unwrap_or("unknown");
        // Default start location for Rundale is Kilteevan Village
        assert_eq!(loc_name, "Kilteevan Village");
    }

    #[tokio::test]
    async fn discover_save_files_returns_ok_for_missing_saves_dir() {
        let state = test_app_state();
        let world = state.world.lock().await;
        let nonexistent_dir = std::path::Path::new("/tmp/rundale_test_nonexistent_saves_dir");
        let saves = parish_core::persistence::picker::discover_saves(nonexistent_dir, &world.graph);
        // Missing dir should return empty vec, not panic
        assert!(
            saves.is_empty(),
            "discover_saves should return empty vec for missing directory"
        );
    }

    #[tokio::test]
    async fn get_world_snapshot_inner_returns_start_location() {
        let state = test_app_state();
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
        assert!(
            !snapshot.location_name.is_empty(),
            "location name should be populated"
        );
        assert_eq!(snapshot.location_name, "Kilteevan Village");
    }

    // ── Screenshot helpers ────────────────────────────────────────────────

    /// A 1×1 transparent PNG, as the smallest valid payload to round-trip.
    const ONE_PIXEL_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkAAIAAAoAAv/lxKUAAAAASUVORK5CYII=";

    fn one_pixel_data_url() -> String {
        format!("data:image/png;base64,{ONE_PIXEL_PNG_B64}")
    }

    #[test]
    fn decode_data_url_png_accepts_well_formed_url() {
        let bytes = decode_data_url_png(&one_pixel_data_url()).unwrap();
        // PNG magic header is 8 bytes starting with 0x89 'P' 'N' 'G'.
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn decode_data_url_png_rejects_wrong_prefix() {
        let err = decode_data_url_png("data:image/jpeg;base64,xxxx").unwrap_err();
        assert!(err.contains("data:image/png;base64,"), "got: {err}");
    }

    #[test]
    fn decode_data_url_png_rejects_invalid_base64() {
        let err = decode_data_url_png("data:image/png;base64,***not-base64***").unwrap_err();
        assert!(err.contains("base64 decode failed"), "got: {err}");
    }

    #[test]
    fn write_screenshot_to_disk_creates_file_and_reports_size() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = decode_data_url_png(&one_pixel_data_url()).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-09T12:34:56Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let info = write_screenshot_to_disk(tmp.path(), &bytes, now).unwrap();

        let path = std::path::PathBuf::from(&info.path);
        assert!(path.exists(), "PNG should be written to {}", info.path);
        assert!(
            info.path.ends_with("parish-2026-05-09T12-34-56Z.png"),
            "filename should embed the timestamp; got {}",
            info.path
        );
        assert_eq!(info.size_bytes, bytes.len() as u64);
        assert_eq!(info.taken_at, "2026-05-09T12:34:56Z");

        // The directory was auto-created.
        assert!(tmp.path().join("screenshots").is_dir());
    }

    #[tokio::test]
    async fn do_save_screenshot_round_trips_through_app_state() {
        // Override saves_dir to a fresh tempdir so the test is hermetic
        // (otherwise the screenshot would land under the workspace `saves/`
        // dir and pollute later runs).
        let tmp = tempfile::tempdir().unwrap();
        let mut state = test_app_state();
        let s = std::sync::Arc::get_mut(&mut state)
            .expect("test_app_state must hand back a unique Arc");
        s.saves_dir = tmp.path().to_path_buf();

        let info = do_save_screenshot(&state, one_pixel_data_url())
            .await
            .expect("screenshot should save");

        assert!(std::path::PathBuf::from(&info.path).exists());

        // The latest_screenshot_path is now populated.
        let latest = state.latest_screenshot_path.lock().await.clone();
        assert_eq!(
            latest.map(|p| p.to_string_lossy().to_string()),
            Some(info.path.clone()),
        );

        // get_latest_screenshot reads the same file back.
        let read_back = do_get_latest_screenshot(&state)
            .await
            .expect("read back must succeed")
            .expect("a screenshot should be cached");
        assert_eq!(read_back.path, info.path);
        assert_eq!(read_back.size_bytes, info.size_bytes);
    }

    #[tokio::test]
    async fn do_get_latest_screenshot_returns_none_when_unset() {
        let state = test_app_state();
        let info = do_get_latest_screenshot(&state).await.unwrap();
        assert!(info.is_none(), "no screenshot taken yet → None");
    }

    #[tokio::test]
    async fn do_get_latest_screenshot_returns_none_when_file_missing() {
        let mut state = test_app_state();
        let s = std::sync::Arc::get_mut(&mut state).unwrap();
        // Point at a path that doesn't exist on disk.
        *s.latest_screenshot_path.get_mut() =
            Some(std::path::PathBuf::from("/no/such/parish-screenshot.png"));
        let info = do_get_latest_screenshot(&state).await.unwrap();
        assert!(info.is_none(), "missing file on disk → None");
    }

    // ── #9 sim-cancel preemption ─────────────────────────────────────────────

    /// Snapshotting `sim_cancel`, then calling the fire-and-replace block
    /// from `do_submit_input`, must cancel the snapshot AND leave a fresh
    /// (uncancelled) token in place for the next sim cycle.
    #[tokio::test]
    async fn sim_cancel_fires_snapshot_and_replaces_token() {
        let state = test_app_state();

        // Snapshot the current sim_cancel — this is what Tier 2/3 spawn
        // captures before player input arrives.
        let snapshot = state.sim_cancel.lock().await.clone();
        assert!(!snapshot.is_cancelled(), "fresh token starts uncancelled");

        // Inline the fire-and-replace block from do_submit_input. We don't
        // call do_submit_input directly because it needs a tauri::AppHandle.
        {
            let mut sc = state.sim_cancel.lock().await;
            sc.cancel();
            *sc = tokio_util::sync::CancellationToken::new();
        }

        // The snapshot is cancelled — any in-flight Tier 2/3 call that
        // captured it will see is_cancelled() == true and bail.
        assert!(
            snapshot.is_cancelled(),
            "captured snapshot must observe cancellation"
        );

        // The new token replacing it is *not* cancelled — the next sim
        // cycle captures this one cleanly.
        let next = state.sim_cancel.lock().await.clone();
        assert!(
            !next.is_cancelled(),
            "replacement token must be fresh / uncancelled"
        );
    }
}
