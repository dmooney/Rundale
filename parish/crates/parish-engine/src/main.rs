use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use parish_engine::config::{InferenceCategory, ProviderConfig};
use parish_engine::headless;
use parish_engine::inference::InferenceClients;
use parish_engine::inference::setup::{self, StdoutProgress};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

fn keychain_secret(slot: &str) -> Option<String> {
    keyring::Entry::new(
        "com.parish.rundale",
        &parish_core::secret_store::provider_account(slot),
    )
    .ok()?
    .get_password()
    .ok()
}

/// Parish — An Irish Living World Text Adventure
#[derive(Parser, Debug)]
#[command(name = "parish", version, about)]
struct Cli {
    /// Select a named schema-v2 inference loadout.
    #[arg(long)]
    loadout: Option<String>,
    /// Run in headless mode (plain stdin/stdout REPL) — this is the default
    #[arg(long)]
    headless: bool,

    /// Run commands from a script file (one per line, JSON output, no LLM needed)
    #[arg(long, value_name = "FILE")]
    script: Option<String>,

    /// LLM provider: ollama (default), lmstudio, openrouter, vllm-mlx, openai, google,
    /// groq, xai, mistral, deepseek, together, nvidia-nim, anthropic, custom, simulator
    #[arg(long)]
    provider: Option<String>,

    /// Override the model name (required for non-Ollama providers)
    #[arg(long)]
    model: Option<String>,

    /// Override the API base URL
    #[arg(long)]
    base_url: Option<String>,

    /// Path to config file (default: parish.toml)
    #[arg(long)]
    config: Option<String>,

    /// Path to engine config (parish-flags.json). Default: platform user-data dir.
    #[arg(long, env = "PARISH_ENGINE_CONFIG")]
    engine_config: Option<String>,

    /// Enable improv craft mode for NPC dialogue
    #[arg(long, env = "PARISH_IMPROV")]
    improv: bool,

    // --- Per-category provider overrides ---
    /// Dialogue LLM provider override
    #[arg(long)]
    dialogue_provider: Option<String>,
    /// Dialogue LLM model override
    #[arg(long)]
    dialogue_model: Option<String>,
    /// Dialogue LLM base URL override
    #[arg(long)]
    dialogue_base_url: Option<String>,

    /// Simulation LLM provider override
    #[arg(long)]
    simulation_provider: Option<String>,
    /// Simulation LLM model override
    #[arg(long)]
    simulation_model: Option<String>,
    /// Simulation LLM base URL override
    #[arg(long)]
    simulation_base_url: Option<String>,

    /// Intent parsing LLM provider override
    #[arg(long)]
    intent_provider: Option<String>,
    /// Intent parsing LLM model override
    #[arg(long)]
    intent_model: Option<String>,
    /// Intent parsing LLM base URL override
    #[arg(long)]
    intent_base_url: Option<String>,

    /// Reaction LLM provider override
    #[arg(long)]
    reaction_provider: Option<String>,
    /// Reaction LLM model override
    #[arg(long)]
    reaction_model: Option<String>,
    /// Reaction LLM base URL override
    #[arg(long)]
    reaction_base_url: Option<String>,

    #[arg(long, hide = true)]
    cloud_provider: Option<String>,
    #[arg(long, hide = true)]
    cloud_model: Option<String>,
    #[arg(long, hide = true)]
    cloud_base_url: Option<String>,

    /// Path to a game mod directory (default: auto-detect mods/rundale/)
    #[arg(long, value_name = "DIR", env = "PARISH_MOD")]
    game_mod: Option<String>,

    /// Disable the on-disk inference call log.
    ///
    /// By default Parish writes every inference call (and the user-visible
    /// chat transcript) as JSONL to `{saves_dir}/inference_logs/` so users
    /// can zip the folder for bug reports. Pass this flag to opt out for
    /// the duration of the run; the `/inference-log on|off` slash command
    /// toggles the same setting at runtime. Overrides
    /// `PARISH_INFERENCE_LOG`.
    #[arg(long)]
    no_inference_log: bool,
}

/// Sets up tracing (file appender + env filter).
///
/// The engine binary is process-local; OpenTelemetry export lives in the
/// `parish-server` binary alongside the request-scoped span machinery that
/// actually populates the spans.
///
/// Returns a [`tracing_appender::non_blocking::WorkerGuard`] that must be
/// held for the lifetime of the process — dropping it drops pending logs.
fn setup_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    let log_dir = parish_core::persistence::paths::resolve_user_data_dir(
        parish_core::persistence::paths::DEFAULT_APP_NAME,
    )
    .join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    let file_appender = tracing_appender::rolling::daily(&log_dir, "parish-engine.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("parish=info")))
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .init();

    guard
}

/// Resolves the v2 provider routes and per-category clients from CLI.
struct ResolvedConfigs {
    provider_config: ProviderConfig,
    category_configs:
        std::collections::HashMap<InferenceCategory, parish_engine::config::CategoryConfig>,
    clients: InferenceClients,
    engine_inference: parish_engine::config::InferenceConfig,
    snapshot: std::sync::Arc<parish_core::config::ResolvedInferenceSnapshot>,
    catalog_store: parish_core::config::CatalogStore,
    catalog_user_data: PathBuf,
}

async fn resolve_configs(
    cli: &Cli,
) -> Result<(
    ResolvedConfigs,
    parish_engine::inference::client::RuntimeProcesses,
)> {
    let project_path = cli
        .config
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("parish.toml"));
    let user_data = parish_core::persistence::paths::resolve_user_data_dir(
        parish_core::persistence::paths::DEFAULT_APP_NAME,
    );
    let user_path = parish_core::config::user_config::resolve_user_config_dir().join("parish.toml");
    let overrides = build_v2_overrides(cli);
    let catalog = parish_core::config::CatalogStore::for_user_data_dir(&user_data);
    let (project, _user, runtime) =
        parish_core::inference_runtime_v2::load_inference_runtime_v2_with_catalog(
            1,
            &project_path,
            &user_path,
            &overrides,
            &catalog,
            &user_data,
            keychain_secret,
        )?;
    let snapshot = runtime.config;
    let clients = runtime.clients;

    let dialogue = &snapshot.category_routes["dialogue"];
    let provider = parish_engine::config::Provider::from_str_loose(&dialogue.key.provider_id)
        .unwrap_or_else(|_| parish_engine::config::Provider::custom());
    let provider_config = ProviderConfig {
        provider,
        base_url: dialogue.inference_base_url.clone(),
        api_key: dialogue
            .credential
            .as_ref()
            .map(|value| value.expose().to_string()),
        model: Some(dialogue.key.model_id.clone()),
    };
    let category_configs = snapshot
        .category_routes
        .iter()
        .filter_map(|(name, route)| {
            let category = match name.as_str() {
                "dialogue" => InferenceCategory::Dialogue,
                "simulation" => InferenceCategory::Simulation,
                "intent" => InferenceCategory::Intent,
                "reaction" => InferenceCategory::Reaction,
                _ => return None,
            };
            let provider = parish_engine::config::Provider::from_str_loose(&route.key.provider_id)
                .unwrap_or_else(|_| parish_engine::config::Provider::custom());
            Some((
                category,
                parish_engine::config::CategoryConfig {
                    provider,
                    base_url: route.inference_base_url.clone(),
                    api_key: route
                        .credential
                        .as_ref()
                        .map(|value| value.expose().to_string()),
                    model: Some(route.key.model_id.clone()),
                },
            ))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let runtime_processes =
        if dialogue.management_adapter == parish_core::config::ManagementAdapter::Ollama {
            let (_, _, processes) = setup_provider(cli, &provider_config).await?;
            processes
        } else {
            parish_engine::inference::client::RuntimeProcesses::default()
        };

    for (cat, cfg) in &category_configs {
        let key_status = if cfg.api_key.is_some() {
            "(set)"
        } else {
            "(not set)"
        };
        tracing::info!(
            "{:?} category: {:?} provider at {} with model {} (API key: {})",
            cat,
            cfg.provider,
            cfg.base_url,
            cfg.model.as_deref().unwrap_or("(auto)"),
            key_status
        );
    }

    Ok((
        ResolvedConfigs {
            provider_config,
            category_configs,
            clients,
            engine_inference: project.engine.inference,
            snapshot,
            catalog_store: catalog,
            catalog_user_data: user_data,
        },
        runtime_processes,
    ))
}

fn build_v2_overrides(cli: &Cli) -> parish_core::config::RoutingOverrideSet {
    use parish_core::config::RoutePatch;
    let route =
        |provider: &Option<String>, model: &Option<String>, base: &Option<String>| RoutePatch {
            provider: provider.clone(),
            model: model.clone(),
            inference_base_url: base.clone(),
            ..RoutePatch::default()
        };
    let mut overrides = parish_core::config::routing_overrides_from_env()
        .unwrap_or_else(|error| panic!("invalid v2 inference environment: {error}"));
    if cli.loadout.is_some() {
        overrides.active_loadout = cli.loadout.clone();
    }
    overrides.global_cli = route(&cli.provider, &cli.model, &cli.base_url);
    overrides.category_cli = std::collections::BTreeMap::from([
        (
            "dialogue".into(),
            route(
                &cli.dialogue_provider,
                &cli.dialogue_model,
                &cli.dialogue_base_url,
            ),
        ),
        (
            "simulation".into(),
            route(
                &cli.simulation_provider,
                &cli.simulation_model,
                &cli.simulation_base_url,
            ),
        ),
        (
            "intent".into(),
            route(
                &cli.intent_provider,
                &cli.intent_model,
                &cli.intent_base_url,
            ),
        ),
        (
            "reaction".into(),
            route(
                &cli.reaction_provider,
                &cli.reaction_model,
                &cli.reaction_base_url,
            ),
        ),
    ]);
    overrides
}

/// Loads the game mod from CLI path or auto-detect.
fn load_game_mod(cli: &Cli) -> Option<parish_core::game_mod::GameMod> {
    if let Some(ref path) = cli.game_mod {
        let dir = std::path::PathBuf::from(path);
        match parish_core::game_mod::GameMod::load(&dir) {
            Ok(gm) => {
                tracing::info!(
                    "Loaded game mod '{}' from explicit path ({})",
                    gm.manifest.meta.name,
                    dir.display()
                );
                Some(gm)
            }
            Err(e) => {
                tracing::warn!("Failed to load mod from {}: {}", dir.display(), e);
                None
            }
        }
    } else {
        parish_core::mod_source::load_base_mod_sync()
    }
}

#[tokio::main]
async fn main() {
    if let Err(error) = run_main().await {
        eprintln!("{error:#}");
        let config_error = error.chain().any(|cause| {
            cause
                .downcast_ref::<parish_core::error::ParishError>()
                .is_some_and(|error| matches!(error, parish_core::error::ParishError::Config(_)))
        });
        std::process::exit(if config_error { 78 } else { 1 });
    }
}

async fn run_main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();
    if cli.cloud_provider.is_some() || cli.cloud_model.is_some() || cli.cloud_base_url.is_some() {
        return Err(parish_core::error::ParishError::Config(
            "--cloud-provider/--cloud-model/--cloud-base-url were removed by configuration schema v2; use --loadout or category route overrides".into(),
        )
        .into());
    }

    let game_mod = load_game_mod(&cli);

    if let Some(script_path) = &cli.script {
        return parish_engine::testing::run_script_mode(Path::new(script_path), game_mod);
    }

    // tracing after script check — in script mode, no file logging needed
    let _tracing_guard = setup_tracing();
    tracing::info!("Starting Parish...");

    let (cfg, mut runtime_processes) = resolve_configs(&cli).await?;

    use std::io::IsTerminal as _;
    let script_mode = !std::io::stdin().is_terminal();
    #[allow(deprecated)]
    let headless_data_dir = find_data_dir();
    let result = headless::run_headless(
        cfg.clients.clone(),
        &cfg.provider_config,
        &cfg.category_configs,
        cli.improv,
        game_mod,
        Some(headless_data_dir),
        cfg.engine_inference,
        cfg.snapshot,
        cfg.catalog_store,
        cfg.catalog_user_data,
        script_mode,
        cli.no_inference_log,
    )
    .await;
    runtime_processes.stop();
    result
}

/// Sets up the inference client based on the resolved provider configuration.
///
/// Thin wrapper over [`setup::setup_provider_client`] — the shared helper
/// used by Tauri and the web server so all modes start with the same
/// Ollama bootstrap behaviour (CLAUDE.md rule #2 — mode parity).
async fn setup_provider(
    _cli: &Cli,
    config: &ProviderConfig,
) -> Result<(
    parish_engine::inference::AnyClient,
    String,
    parish_engine::inference::client::RuntimeProcesses,
)> {
    let progress = StdoutProgress;
    let (client, model, process) = setup::setup_provider_client(
        config,
        &[],
        &[],
        &parish_engine::config::InferenceConfig::default(),
        &progress,
    )
    .await?;
    tracing::info!(
        "Using {:?} provider at {} with model {}",
        config.provider,
        config.base_url,
        model
    );
    Ok((client, model, process))
}

/// Resolves the active mod data directory (containing `world.json` + `npcs.json`)
/// once at startup.
///
/// Resolution order:
/// 1. `PARISH_DATA_DIR` environment variable — explicit operator override.
/// 2. Walks up to 4 ancestors of the cwd looking for `mods/rundale/world.json`.
/// 3. Falls back to `./mods/rundale` and lets the load functions fail with a
///    clear error.
///
/// # Deprecated
///
/// Uses `std::env::current_dir()` which breaks in daemonised or `/tmp`
/// working-directory deployments. Replace callers with path resolution from
/// explicit `AppState` config per AGENTS.md rule #9.
#[deprecated(
    note = "cwd-relative path resolution breaks in non-CWD deployments; use explicit config"
)]
fn find_data_dir() -> PathBuf {
    const MOD_REL: &str = "mods/rundale";
    if let Some(explicit) = std::env::var_os("PARISH_DATA_DIR") {
        return PathBuf::from(explicit);
    }
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..4 {
        if p.join(MOD_REL).join("world.json").exists() {
            return p.join(MOD_REL);
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    PathBuf::from(MOD_REL)
}
