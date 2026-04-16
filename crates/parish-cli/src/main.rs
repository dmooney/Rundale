use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Parser;
use parish::config::{
    CliCategoryOverrides, CliCloudOverrides, CliOverrides, InferenceCategory, Provider,
    ProviderConfig, resolve_category_configs, resolve_cloud_config, resolve_config,
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
    /// groq, xai, mistral, deepseek, together, anthropic, custom, simulator
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

    /// Cloud LLM provider for player dialogue: openrouter (default), openai,
    /// google, groq, xai, mistral, deepseek, together, anthropic, custom
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

    // Headless REPL mode (default)
    let headless_data_dir = find_data_dir();
    let result = headless::run_headless(
        clients.clone(),
        &provider_config,
        cloud_config.as_ref(),
        &category_configs,
        cli.improv,
        game_mod,
        Some(headless_data_dir),
    )
    .await;
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
    parish::inference::AnyClient,
    String,
    parish::inference::client::OllamaProcess,
)> {
    use parish::inference::AnyClient;
    match config.provider {
        Provider::Simulator => {
            // Built-in simulator: no network, no model download required.
            tracing::info!("Using built-in inference simulator (GPT-0 mode)");
            let dummy_process = parish::inference::client::OllamaProcess::none();
            Ok((
                AnyClient::simulator(),
                "simulator".to_string(),
                dummy_process,
            ))
        }
        Provider::Ollama => {
            let progress = StdoutProgress;
            let setup =
                setup::setup_ollama(&config.base_url, config.model.as_deref(), &progress).await?;
            let model = setup.model_name.clone();
            let client = AnyClient::open_ai(setup.client.clone());
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
            let client = parish::inference::build_client(
                &config.provider,
                &config.base_url,
                config.api_key.as_deref(),
                &parish::config::InferenceConfig::default(),
            );
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
    InferenceClients::new(base_client.clone(), base_model.to_string(), overrides)
}

/// Finds the active mod data directory (containing `world.json` + `npcs.json`).
fn find_data_dir() -> PathBuf {
    const MOD_REL: &str = "mods/rundale";
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
    let candidates = ["apps/ui/dist", "ui/dist"];
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
