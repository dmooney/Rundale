use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use parish_engine::config::{
    CliCategoryOverrides, CliCloudOverrides, CliOverrides, InferenceCategory, ProviderConfig,
    resolve_category_configs, resolve_cloud_config, resolve_config,
};
use parish_engine::headless;
use parish_engine::inference::InferenceClients;
use parish_engine::inference::setup::{self, StdoutProgress};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Parish — An Irish Living World Text Adventure
#[derive(Parser, Debug)]
#[command(name = "parish", version, about)]
struct Cli {
    /// Run in headless mode (plain stdin/stdout REPL) — this is the default
    #[arg(long)]
    headless: bool,

    /// Run commands from a script file (one per line, JSON output, no LLM needed)
    #[arg(long, value_name = "FILE")]
    script: Option<String>,

    /// LLM provider: ollama (default), lmstudio, openrouter, vllm-mlx, openai, google,
    /// groq, xai, mistral, deepseek, together, nvidia-nim, anthropic, custom, simulator
    #[arg(long, env = "PARISH_PROVIDER")]
    provider: Option<String>,

    /// Override the model name (required for non-Ollama providers)
    #[arg(long, env = "PARISH_MODEL")]
    model: Option<String>,

    /// Override the API base URL
    #[arg(long, env = "PARISH_BASE_URL")]
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

    /// Cloud LLM provider for player dialogue: google (default), openai,
    /// google, groq, xai, mistral, deepseek, together, nvidia-nim, anthropic, custom
    #[arg(long, env = "PARISH_CLOUD_PROVIDER")]
    cloud_provider: Option<String>,

    /// Cloud LLM model name (required when cloud provider is set)
    #[arg(long, env = "PARISH_CLOUD_MODEL")]
    cloud_model: Option<String>,

    /// Cloud LLM API base URL override
    #[arg(long, env = "PARISH_CLOUD_BASE_URL")]
    cloud_base_url: Option<String>,

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

    /// Simulation LLM provider override
    #[arg(long, env = "PARISH_SIMULATION_PROVIDER")]
    simulation_provider: Option<String>,
    /// Simulation LLM model override
    #[arg(long, env = "PARISH_SIMULATION_MODEL")]
    simulation_model: Option<String>,
    /// Simulation LLM base URL override
    #[arg(long, env = "PARISH_SIMULATION_BASE_URL")]
    simulation_base_url: Option<String>,

    /// Intent parsing LLM provider override
    #[arg(long, env = "PARISH_INTENT_PROVIDER")]
    intent_provider: Option<String>,
    /// Intent parsing LLM model override
    #[arg(long, env = "PARISH_INTENT_MODEL")]
    intent_model: Option<String>,
    /// Intent parsing LLM base URL override
    #[arg(long, env = "PARISH_INTENT_BASE_URL")]
    intent_base_url: Option<String>,

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

/// Resolves provider config, cloud config, and per-category configs from CLI.
struct ResolvedConfigs {
    provider_config: ProviderConfig,
    cloud_config: Option<parish_engine::config::CloudConfig>,
    category_configs:
        std::collections::HashMap<InferenceCategory, parish_engine::config::CategoryConfig>,
    clients: InferenceClients,
    engine_inference: parish_engine::config::InferenceConfig,
}

async fn resolve_configs(
    cli: &Cli,
) -> Result<(
    ResolvedConfigs,
    parish_engine::inference::client::RuntimeProcesses,
)> {
    let config_path = cli.config.as_ref().map(|p| Path::new(p.as_str()));
    let overrides = CliOverrides {
        provider: cli.provider.clone(),
        base_url: cli.base_url.clone(),
        model: cli.model.clone(),
    };
    let provider_config = resolve_config(config_path, &overrides)?;

    let cloud_overrides = CliCloudOverrides {
        provider: cli.cloud_provider.clone(),
        base_url: cli.cloud_base_url.clone(),
        model: cli.cloud_model.clone(),
    };
    let cloud_config_opt = resolve_cloud_config(config_path, &cloud_overrides)?;

    let cli_category_overrides = build_cli_category_overrides(cli);
    let category_configs = resolve_category_configs(
        config_path,
        &provider_config,
        &cli_category_overrides,
        &cloud_overrides,
    )?;

    let (client, model, runtime_processes) = setup_provider(cli, &provider_config).await?;

    let clients = build_inference_clients(&provider_config, &client, &model, &category_configs);

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

    let engine_config_path = if let Some(p) = cli.engine_config.as_deref() {
        PathBuf::from(p)
    } else {
        let user_data = parish_core::persistence::paths::resolve_user_data_dir(
            parish_core::persistence::paths::DEFAULT_APP_NAME,
        );
        parish_core::config::resolve_config_path(&user_data)
    };
    let engine_config = parish_core::config::load_engine_config(&engine_config_path);

    Ok((
        ResolvedConfigs {
            provider_config,
            cloud_config: cloud_config_opt,
            category_configs,
            clients,
            engine_inference: engine_config.inference,
        },
        runtime_processes,
    ))
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
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let cli = Cli::parse();

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
        cfg.cloud_config.as_ref(),
        &cfg.category_configs,
        cli.improv,
        game_mod,
        Some(headless_data_dir),
        cfg.engine_inference,
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

/// Builds the per-category inference routing struct from base and category configs.
///
/// For categories without an explicit override, falls back to the base
/// provider's preset model for that role when the preset differs from the
/// base model. This way, setting only `PARISH_PROVIDER=anthropic` (no
/// per-category env vars) routes Dialogue → Opus, Simulation/Reaction →
/// Sonnet, Intent → Haiku — even though `category_configs` is empty.
fn build_inference_clients(
    base_provider_config: &parish_engine::config::ProviderConfig,
    base_client: &parish_engine::inference::AnyClient,
    base_model: &str,
    category_configs: &std::collections::HashMap<
        InferenceCategory,
        parish_engine::config::CategoryConfig,
    >,
) -> InferenceClients {
    let mut overrides = std::collections::HashMap::new();
    let inference_cfg = parish_engine::config::InferenceConfig::default();
    for (category, cfg) in category_configs {
        let client = parish_engine::inference::build_client(
            &cfg.provider,
            &cfg.base_url,
            cfg.api_key.as_deref(),
            &inference_cfg,
        );
        let model = cfg.model.clone().unwrap_or_else(|| base_model.to_string());
        overrides.insert(*category, (client, model));
    }

    // Fill in per-role presets for categories without explicit overrides.
    // The override reuses the base client (same provider/url/key) but
    // points the category at the per-role preset model.
    //
    // Skipped for Ollama: auto-setup pulls a single hardware-matched model
    // and the static qwen3 preset would route every role away from it.
    // Letting these categories fall through to `base_model` keeps every
    // request on the model that is actually on disk.
    if base_provider_config.provider.id() != "ollama" {
        for category in InferenceCategory::ALL {
            if overrides.contains_key(&category) {
                continue;
            }
            if let Some(preset) = base_provider_config.provider.preset_model(category)
                && preset != base_model
            {
                overrides.insert(category, (base_client.clone(), preset.to_string()));
            }
        }
    }

    InferenceClients::new(base_client.clone(), base_model.to_string(), overrides)
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

/// Builds per-category CLI overrides from the parsed CLI arguments.
fn build_cli_category_overrides(cli: &Cli) -> CliCategoryOverrides {
    let mut categories = std::collections::HashMap::new();

    for (name, provider, base_url, model) in [
        (
            "dialogue",
            &cli.dialogue_provider,
            &cli.dialogue_base_url,
            &cli.dialogue_model,
        ),
        (
            "simulation",
            &cli.simulation_provider,
            &cli.simulation_base_url,
            &cli.simulation_model,
        ),
        (
            "intent",
            &cli.intent_provider,
            &cli.intent_base_url,
            &cli.intent_model,
        ),
    ] {
        if provider.is_some() || base_url.is_some() || model.is_some() {
            categories.insert(
                name.to_string(),
                CliOverrides {
                    provider: provider.clone(),
                    base_url: base_url.clone(),
                    model: model.clone(),
                },
            );
        }
    }

    CliCategoryOverrides { categories }
}
