//! BYOK onboarding and local inference setup commands.

use std::sync::Arc;

use crate::AppState;

// ── BYOK onboarding commands ─────────────────────────────────────────────────

pub(super) fn byok_ctx<'a>(state: &'a Arc<AppState>) -> parish_core::ipc::byok::ByokContext<'a> {
    parish_core::ipc::byok::ByokContext {
        config: &state.config,
        inference_config: &state.inference_config,
        inference_log: state.inference_log.clone(),
        inference_file_log: state.inference_file_log.clone(),
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
    use tauri::Emitter;
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

/// Returns the registry split into `featured` + `other` lists. The BYOK
/// wizard renders directly from this — the source of truth is now the
/// provider registry (builtins + mod-loaded), not a hand-curated TS
/// constant.
#[tauri::command]
pub async fn list_available_providers() -> Result<
    std::collections::HashMap<&'static str, Vec<parish_core::ipc::byok::ProviderInfo>>,
    String,
> {
    Ok(parish_core::ipc::byok::handle_list_available_providers())
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
        use tauri::Emitter;
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
    let persistence_ready = state_arc.save_path.lock().await.is_some()
        || crate::setup::init_persistence(&state_arc).await;
    if !persistence_ready {
        return Err("persistence initialization failed after wizard".to_string());
    }

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
    crate::setup::spawn_event_bus_fanin(&state_arc).await;
    crate::setup::spawn_world_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_inactivity_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_debug_tick(app.clone(), Arc::clone(&state_arc));
    crate::setup::spawn_autosave_tick(Arc::clone(&state_arc));
    Ok(())
}
