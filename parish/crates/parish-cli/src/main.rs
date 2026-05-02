use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use parish::config::{
    CliCategoryOverrides, CliCloudOverrides, CliOverrides, InferenceCategory, ProviderConfig,
    resolve_category_configs, resolve_cloud_config, resolve_config,
};
use parish::headless;
use parish::inference::InferenceClients;
use parish::inference::setup::{self, StdoutProgress};
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

    /// LLM provider: ollama (default), lmstudio, openrouter, vllm, openai, google,
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

    /// Enable improv craft mode for NPC dialogue
    #[arg(long, env = "PARISH_IMPROV")]
    improv: bool,

    /// Cloud LLM provider for player dialogue: openrouter (default), openai,
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

    /// Run as a web server (serves UI in browser for testing)
    ///
    /// Starts an axum HTTP server on the specified port (default: 3001)
    /// that serves the Svelte frontend and exposes REST + WebSocket
    /// endpoints. Use this for automated Chrome testing via Playwright.
    #[arg(long, value_name = "PORT", default_missing_value = "3001", num_args = 0..=1)]
    web: Option<u16>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load .env file if present (before anything reads env vars)
    dotenvy::dotenv().ok();

    // Set up logging: file appender (always)
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

    // Web server mode — serves UI in browser for testing
    if let Some(port) = cli.web {
        let data_dir = find_data_dir();
        let static_dir = find_ui_dist_dir();
        tracing::info!(
            "Starting web server on port {} (data={}, static={})",
            port,
            data_dir.display(),
            static_dir.display()
        );
        return parish_server::run_server(port, data_dir, static_dir).await;
    }

    // Resolve provider configuration from file + env + CLI
    let config_path = cli.config.as_ref().map(|p| Path::new(p.as_str()));
    let overrides = CliOverrides {
        provider: cli.provider.clone(),
        base_url: cli.base_url.clone(),
        model: cli.model.clone(),
    };
    let provider_config = resolve_config(config_path, &overrides)?;

    // Resolve cloud provider configuration (legacy, for backward compat)
    let cloud_overrides = CliCloudOverrides {
        provider: cli.cloud_provider.clone(),
        base_url: cli.cloud_base_url.clone(),
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

    // Load game mod (from --game-mod flag, env var, or auto-detect)
    let game_mod = {
        let mod_dir = if let Some(ref path) = cli.game_mod {
            Some(std::path::PathBuf::from(path))
        } else {
            parish_core::game_mod::find_default_mod()
        };
        match mod_dir {
            Some(dir) => match parish_core::game_mod::GameMod::load(&dir) {
                Ok(gm) => {
                    tracing::info!(
                        "Loaded game mod: {} ({})",
                        gm.manifest.meta.name,
                        dir.display()
                    );
                    Some(gm)
                }
                Err(e) => {
                    tracing::warn!("Failed to load mod from {}: {}", dir.display(), e);
                    None
                }
            },
            None => {
                tracing::info!("No game mod found; using built-in defaults");
                None
            }
        }
    };

    // Load engine config (parish.toml) for TOML-configured inference timeouts.
    // Missing file falls back to compiled-in defaults. (#417)
    let engine_config = parish_core::config::load_engine_config(None);

    // Headless REPL mode (default).
    // Detect non-interactive (piped / redirected) stdin so `run_headless` can
    // fail closed on a save-file lock conflict instead of silently proceeding
    // (#608).  `IsTerminal` is stable since Rust 1.70 — no extra dep needed.
    use std::io::IsTerminal as _;
    let script_mode = !std::io::stdin().is_terminal();
    let headless_data_dir = find_data_dir();
    let result = headless::run_headless(
        clients.clone(),
        &provider_config,
        cloud_config.as_ref(),
        &category_configs,
        cli.improv,
        game_mod,
        Some(headless_data_dir),
        engine_config.inference,
        script_mode,
    )
    .await;
    ollama_process.stop();
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
    parish::inference::AnyClient,
    String,
    parish::inference::client::OllamaProcess,
)> {
    let progress = StdoutProgress;
    let (client, model, process) = setup::setup_provider_client(
        config,
        &parish::config::InferenceConfig::default(),
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
    base_provider_config: &parish::config::ProviderConfig,
    base_client: &parish::inference::AnyClient,
    base_model: &str,
    category_configs: &std::collections::HashMap<InferenceCategory, parish::config::CategoryConfig>,
) -> InferenceClients {
    let mut overrides = std::collections::HashMap::new();
    let inference_cfg = parish::config::InferenceConfig::default();
    for (category, cfg) in category_configs {
        let client = parish::inference::build_client(
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

/// Finds the Svelte frontend build directory (`apps/ui/dist/`).
fn find_ui_dist_dir() -> PathBuf {
    let candidates = ["apps/ui/dist", "parish/apps/ui/dist", "ui/dist"];
    let mut p = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..4 {
        for c in &candidates {
            if p.join(c).join("index.html").exists() {
                return p.join(c);
            }
        }
        match p.parent() {
            Some(parent) => p = parent.to_path_buf(),
            None => break,
        }
    }
    PathBuf::from("apps/ui/dist")
}

/// Builds per-category CLI overrides from the parsed CLI arguments.
fn build_cli_category_overrides(cli: &Cli) -> CliCategoryOverrides {
    let mut categories = std::collections::HashMap::new();

    let dialogue = CliOverrides {
        provider: cli.dialogue_provider.clone(),
        base_url: cli.dialogue_base_url.clone(),
        model: cli.dialogue_model.clone(),
    };
    if dialogue.provider.is_some() || dialogue.base_url.is_some() || dialogue.model.is_some() {
        categories.insert("dialogue".to_string(), dialogue);
    }

    let simulation = CliOverrides {
        provider: cli.simulation_provider.clone(),
        base_url: cli.simulation_base_url.clone(),
        model: cli.simulation_model.clone(),
    };
    if simulation.provider.is_some() || simulation.base_url.is_some() || simulation.model.is_some()
    {
        categories.insert("simulation".to_string(), simulation);
    }

    let intent = CliOverrides {
        provider: cli.intent_provider.clone(),
        base_url: cli.intent_base_url.clone(),
        model: cli.intent_model.clone(),
    };
    if intent.provider.is_some() || intent.base_url.is_some() || intent.model.is_some() {
        categories.insert("intent".to_string(), intent);
    }

    CliCategoryOverrides { categories }
}
