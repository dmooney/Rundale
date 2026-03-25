//! Provider configuration for LLM inference backends.
//!
//! Supports Ollama (local, default), LM Studio (local), OpenRouter (cloud),
//! and custom OpenAI-compatible endpoints. Configuration is resolved from
//! a TOML file, environment variables, and CLI flags (in that priority order).
//!
//! Each inference category (dialogue, simulation, intent) can be independently
//! configured with its own provider, model, base URL, and API key via
//! per-category TOML sections, environment variables, or CLI flags. Unconfigured
//! categories inherit from the base `[provider]` config.

use crate::error::ParishError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Default base URL for each provider.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_LMSTUDIO_URL: &str = "http://localhost:1234";
const DEFAULT_OPENROUTER_URL: &str = "https://openrouter.ai/api";

/// Supported LLM provider backends.
///
/// All providers use the OpenAI-compatible chat completions API
/// (`/v1/chat/completions`). Ollama is the default and includes
/// auto-start, GPU detection, and model pulling features.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Provider {
    /// Local Ollama server with auto-management (default).
    #[default]
    Ollama,
    /// Local LM Studio server.
    LmStudio,
    /// OpenRouter cloud gateway (requires API key).
    OpenRouter,
    /// Any OpenAI-compatible endpoint (requires base_url).
    Custom,
}

impl Provider {
    /// Parses a provider name string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Result<Self, ParishError> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Provider::Ollama),
            "lmstudio" | "lm_studio" | "lm-studio" => Ok(Provider::LmStudio),
            "openrouter" | "open_router" | "open-router" => Ok(Provider::OpenRouter),
            "custom" => Ok(Provider::Custom),
            other => Err(ParishError::Config(format!(
                "unknown provider '{}'. Expected: ollama, lmstudio, openrouter, custom",
                other
            ))),
        }
    }

    /// Returns the default base URL for this provider.
    pub fn default_base_url(&self) -> &'static str {
        match self {
            Provider::Ollama => DEFAULT_OLLAMA_URL,
            Provider::LmStudio => DEFAULT_LMSTUDIO_URL,
            Provider::OpenRouter => DEFAULT_OPENROUTER_URL,
            Provider::Custom => "",
        }
    }

    /// Whether this provider requires an API key.
    pub fn requires_api_key(&self) -> bool {
        matches!(self, Provider::OpenRouter)
    }

    /// Whether this provider requires an explicit model name
    /// (no auto-detection available).
    pub fn requires_model(&self) -> bool {
        !matches!(self, Provider::Ollama)
    }
}

/// Inference categories that can each have independent provider configuration.
///
/// Each category can override the base `[provider]` config with its own
/// provider, model, base URL, and API key. Unconfigured categories fall
/// back to the base provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InferenceCategory {
    /// Player-facing NPC dialogue (Tier 1, streaming).
    Dialogue,
    /// Background NPC simulation (Tier 2, JSON).
    Simulation,
    /// Player input intent parsing (JSON, low-latency).
    Intent,
}

impl InferenceCategory {
    /// All defined inference categories.
    pub const ALL: [InferenceCategory; 3] = [
        InferenceCategory::Dialogue,
        InferenceCategory::Simulation,
        InferenceCategory::Intent,
    ];

    /// Returns the lowercase name used in TOML keys, env var prefixes, and CLI flags.
    pub fn name(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "dialogue",
            InferenceCategory::Simulation => "simulation",
            InferenceCategory::Intent => "intent",
        }
    }

    /// Returns the SCREAMING_CASE prefix used in environment variables.
    fn env_prefix(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "PARISH_DIALOGUE",
            InferenceCategory::Simulation => "PARISH_SIMULATION",
            InferenceCategory::Intent => "PARISH_INTENT",
        }
    }
}

/// Resolved provider configuration for a single inference category.
///
/// Built by layering the base `[provider]` config with per-category
/// overrides from TOML, environment variables, and CLI flags.
#[derive(Debug, Clone)]
pub struct CategoryConfig {
    /// The provider backend for this category.
    pub provider: Provider,
    /// Base URL for the provider API.
    pub base_url: String,
    /// API key for authenticated providers.
    pub api_key: Option<String>,
    /// Model name. `None` for Ollama (auto-selected).
    pub model: Option<String>,
}

/// CLI-provided overrides for per-category provider configuration.
#[derive(Debug, Default)]
pub struct CliCategoryOverrides {
    /// Per-category CLI overrides keyed by category name.
    pub categories: HashMap<String, CliOverrides>,
}

/// Resolved provider configuration ready for use.
///
/// Built from the TOML config file, environment variables, and CLI
/// flags via [`resolve_config`].
#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// The selected provider backend.
    pub provider: Provider,
    /// Base URL for the provider API.
    pub base_url: String,
    /// API key for authenticated providers (OpenRouter, etc.).
    pub api_key: Option<String>,
    /// Model name override. Required for non-Ollama providers.
    pub model: Option<String>,
}

/// Resolved cloud provider configuration for player-facing dialogue.
///
/// Present only when a cloud provider has been explicitly configured
/// via TOML `[cloud]` section, `PARISH_CLOUD_*` env vars, or `--cloud-*` CLI flags.
/// When absent, all inference (including dialogue) uses the local provider.
#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// The cloud provider backend (typically OpenRouter).
    pub provider: Provider,
    /// Base URL for the cloud API.
    pub base_url: String,
    /// API key for the cloud provider.
    pub api_key: Option<String>,
    /// Model name (required for cloud providers).
    pub model: String,
}

/// CLI-provided overrides for cloud provider configuration.
#[derive(Debug, Default)]
pub struct CliCloudOverrides {
    /// `--cloud-provider` flag value.
    pub provider: Option<String>,
    /// `--cloud-base-url` flag value.
    pub base_url: Option<String>,
    /// `--cloud-api-key` flag value.
    pub api_key: Option<String>,
    /// `--cloud-model` flag value.
    pub model: Option<String>,
}

/// Raw TOML file structure for `parish.toml`.
#[derive(Debug, Deserialize, Default)]
struct TomlConfig {
    #[serde(default)]
    provider: TomlProvider,
    #[serde(default)]
    cloud: TomlCloud,
}

/// The `[provider]` section of the TOML config.
#[derive(Debug, Deserialize, Default)]
struct TomlProvider {
    /// Provider name: "ollama", "lmstudio", "openrouter", "custom".
    name: Option<String>,
    /// Base URL override.
    base_url: Option<String>,
    /// API key for cloud providers.
    api_key: Option<String>,
    /// Model name override.
    model: Option<String>,
    /// Per-category overrides: `[provider.dialogue]`, `[provider.simulation]`, `[provider.intent]`.
    #[serde(default)]
    dialogue: Option<TomlCategoryOverride>,
    /// Per-category override for simulation.
    #[serde(default)]
    simulation: Option<TomlCategoryOverride>,
    /// Per-category override for intent parsing.
    #[serde(default)]
    intent: Option<TomlCategoryOverride>,
}

/// Per-category provider override in TOML (e.g. `[provider.dialogue]`).
#[derive(Debug, Deserialize, Default, Clone)]
struct TomlCategoryOverride {
    /// Provider name override for this category.
    name: Option<String>,
    /// Base URL override for this category.
    base_url: Option<String>,
    /// API key override for this category.
    api_key: Option<String>,
    /// Model name override for this category.
    model: Option<String>,
}

/// The `[cloud]` section of the TOML config for cloud dialogue provider.
#[derive(Debug, Deserialize, Default)]
struct TomlCloud {
    /// Provider name: "openrouter", "custom", etc.
    name: Option<String>,
    /// Base URL override.
    base_url: Option<String>,
    /// API key for cloud provider.
    api_key: Option<String>,
    /// Model name (required for cloud).
    model: Option<String>,
}

/// CLI-provided overrides for provider configuration.
#[derive(Debug, Default)]
pub struct CliOverrides {
    /// `--provider` flag value.
    pub provider: Option<String>,
    /// `--base-url` flag value.
    pub base_url: Option<String>,
    /// `--api-key` flag value.
    pub api_key: Option<String>,
    /// `--model` flag value.
    pub model: Option<String>,
}

impl ProviderConfig {
    /// Returns a display-friendly provider name.
    pub fn provider_display(&self) -> String {
        format!("{:?}", self.provider).to_lowercase()
    }
}

/// Resolves provider configuration from file, env vars, and CLI flags.
///
/// Resolution order (later overrides earlier):
/// 1. Hardcoded defaults per provider
/// 2. TOML config file (if it exists)
/// 3. Environment variables: `PARISH_PROVIDER`, `PARISH_BASE_URL`,
///    `PARISH_API_KEY`, `PARISH_MODEL`
/// 4. CLI flags via `CliOverrides`
///
/// Also checks for the deprecated `PARISH_OLLAMA_URL` env var and maps
/// it to `base_url` with a warning.
pub fn resolve_config(
    config_path: Option<&Path>,
    cli: &CliOverrides,
) -> Result<ProviderConfig, ParishError> {
    // Layer 1: Read TOML file (optional)
    let toml_cfg = if let Some(path) = config_path {
        read_toml_config(path)?
    } else {
        // Try default paths
        let cwd_path = Path::new("parish.toml");
        if cwd_path.exists() {
            read_toml_config(cwd_path)?
        } else {
            TomlConfig::default()
        }
    };

    // Layer 2: Start with TOML values
    let mut provider_str = toml_cfg.provider.name;
    let mut base_url = toml_cfg.provider.base_url;
    let mut api_key = toml_cfg.provider.api_key;
    let mut model = toml_cfg.provider.model;

    // Layer 3: Environment variables override TOML
    if let Some(val) = env_non_empty("PARISH_PROVIDER") {
        provider_str = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_BASE_URL") {
        base_url = Some(val);
    }
    // Deprecated: map PARISH_OLLAMA_URL to base_url if no explicit base_url set
    if base_url.is_none()
        && let Some(val) = env_non_empty("PARISH_OLLAMA_URL")
    {
        tracing::warn!("PARISH_OLLAMA_URL is deprecated, use PARISH_BASE_URL instead");
        base_url = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_API_KEY") {
        api_key = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_MODEL") {
        model = Some(val);
    }

    // Layer 4: CLI flags override everything
    if let Some(ref val) = cli.provider {
        provider_str = Some(val.clone());
    }
    if let Some(ref val) = cli.base_url {
        base_url = Some(val.clone());
    }
    if let Some(ref val) = cli.api_key {
        api_key = Some(val.clone());
    }
    if let Some(ref val) = cli.model {
        model = Some(val.clone());
    }

    // Resolve provider enum
    let provider = match &provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::default(),
    };

    // Apply default base URL if none specified
    let base_url = base_url.unwrap_or_else(|| provider.default_base_url().to_string());

    // Filter out empty api_key/model strings
    let api_key = api_key.filter(|s| !s.is_empty());
    let model = model.filter(|s| !s.is_empty());

    // Validate
    if provider.requires_api_key() && api_key.is_none() {
        return Err(ParishError::Config(format!(
            "{:?} provider requires an API key. Set PARISH_API_KEY or --api-key.",
            provider
        )));
    }
    if provider == Provider::Custom && base_url.is_empty() {
        return Err(ParishError::Config(
            "Custom provider requires a base_url. Set PARISH_BASE_URL or --base-url.".to_string(),
        ));
    }

    Ok(ProviderConfig {
        provider,
        base_url,
        api_key,
        model,
    })
}

/// Resolves cloud provider configuration from file, env vars, and CLI flags.
///
/// Returns `None` if no cloud settings are present anywhere (backward compatible).
/// Returns `Some(CloudConfig)` when at least one cloud setting is configured.
/// The model name is required for cloud providers.
///
/// Resolution order (later overrides earlier):
/// 1. TOML `[cloud]` section
/// 2. Environment variables: `PARISH_CLOUD_PROVIDER`, `PARISH_CLOUD_BASE_URL`,
///    `PARISH_CLOUD_API_KEY`, `PARISH_CLOUD_MODEL`
/// 3. CLI flags via `CliCloudOverrides`
pub fn resolve_cloud_config(
    config_path: Option<&Path>,
    cli: &CliCloudOverrides,
) -> Result<Option<CloudConfig>, ParishError> {
    // Layer 1: Read TOML file (reuse same file as local config)
    let toml_cfg = if let Some(path) = config_path {
        read_toml_config(path)?
    } else {
        let cwd_path = Path::new("parish.toml");
        if cwd_path.exists() {
            read_toml_config(cwd_path)?
        } else {
            TomlConfig::default()
        }
    };

    // Layer 2: Start with TOML cloud values
    let mut provider_str = toml_cfg.cloud.name;
    let mut base_url = toml_cfg.cloud.base_url;
    let mut api_key = toml_cfg.cloud.api_key;
    let mut model = toml_cfg.cloud.model;

    // Layer 3: Environment variables override TOML
    if let Some(val) = env_non_empty("PARISH_CLOUD_PROVIDER") {
        provider_str = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_CLOUD_BASE_URL") {
        base_url = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_CLOUD_API_KEY") {
        api_key = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_CLOUD_MODEL") {
        model = Some(val);
    }

    // Layer 4: CLI flags override everything
    if let Some(ref val) = cli.provider {
        provider_str = Some(val.clone());
    }
    if let Some(ref val) = cli.base_url {
        base_url = Some(val.clone());
    }
    if let Some(ref val) = cli.api_key {
        api_key = Some(val.clone());
    }
    if let Some(ref val) = cli.model {
        model = Some(val.clone());
    }

    // Filter out empty strings
    let api_key = api_key.filter(|s| !s.is_empty());
    let model = model.filter(|s| !s.is_empty());

    // If nothing is configured, return None (backward compatible)
    if provider_str.is_none() && base_url.is_none() && api_key.is_none() && model.is_none() {
        return Ok(None);
    }

    // Resolve provider (default to OpenRouter for cloud)
    let provider = match &provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::OpenRouter,
    };

    // Apply default base URL if none specified
    let base_url = base_url.unwrap_or_else(|| provider.default_base_url().to_string());

    // Cloud providers require a model name
    let model = model.ok_or_else(|| {
        ParishError::Config(
            "Cloud provider requires a model name. Set PARISH_CLOUD_MODEL or --cloud-model."
                .to_string(),
        )
    })?;

    // Validate
    if provider.requires_api_key() && api_key.is_none() {
        return Err(ParishError::Config(format!(
            "Cloud {:?} provider requires an API key. Set PARISH_CLOUD_API_KEY or --cloud-api-key.",
            provider
        )));
    }
    if provider == Provider::Custom && base_url.is_empty() {
        return Err(ParishError::Config(
            "Cloud custom provider requires a base_url. Set PARISH_CLOUD_BASE_URL or --cloud-base-url.".to_string(),
        ));
    }

    Ok(Some(CloudConfig {
        provider,
        base_url,
        api_key,
        model,
    }))
}

/// Resolves per-category provider configurations.
///
/// For each [`InferenceCategory`], layers overrides on top of the base
/// [`ProviderConfig`]:
/// 1. Start with the base provider config as defaults
/// 2. Layer TOML `[provider.<category>]` overrides
/// 3. Layer `PARISH_<CATEGORY>_*` environment variables
/// 4. Layer per-category CLI flags
/// 5. For dialogue only: fall back to legacy `[cloud]` / `PARISH_CLOUD_*` / `--cloud-*`
///
/// Returns a map from category to resolved [`CategoryConfig`]. Categories with
/// no overrides are omitted (callers should fall back to the base config).
pub fn resolve_category_configs(
    config_path: Option<&Path>,
    base: &ProviderConfig,
    cli_categories: &CliCategoryOverrides,
    cli_cloud: &CliCloudOverrides,
) -> Result<HashMap<InferenceCategory, CategoryConfig>, ParishError> {
    let toml_cfg = load_toml(config_path)?;
    let mut result = HashMap::new();

    for category in InferenceCategory::ALL {
        // Get the TOML override for this category
        let toml_override = match category {
            InferenceCategory::Dialogue => toml_cfg.provider.dialogue.clone(),
            InferenceCategory::Simulation => toml_cfg.provider.simulation.clone(),
            InferenceCategory::Intent => toml_cfg.provider.intent.clone(),
        };

        // Get CLI overrides for this category
        let cli_override = cli_categories.categories.get(category.name());

        // Check if there are any overrides at all for this category
        let has_toml = toml_override.as_ref().is_some_and(|t| {
            t.name.is_some() || t.base_url.is_some() || t.api_key.is_some() || t.model.is_some()
        });
        let has_env = env_non_empty(&format!("{}_PROVIDER", category.env_prefix())).is_some()
            || env_non_empty(&format!("{}_BASE_URL", category.env_prefix())).is_some()
            || env_non_empty(&format!("{}_API_KEY", category.env_prefix())).is_some()
            || env_non_empty(&format!("{}_MODEL", category.env_prefix())).is_some();
        let has_cli = cli_override.is_some_and(|c| {
            c.provider.is_some() || c.base_url.is_some() || c.api_key.is_some() || c.model.is_some()
        });

        // For dialogue: also check legacy [cloud] config
        let has_legacy_cloud = category == InferenceCategory::Dialogue
            && (toml_cfg.cloud.name.is_some()
                || toml_cfg.cloud.base_url.is_some()
                || toml_cfg.cloud.api_key.is_some()
                || toml_cfg.cloud.model.is_some()
                || env_non_empty("PARISH_CLOUD_PROVIDER").is_some()
                || env_non_empty("PARISH_CLOUD_BASE_URL").is_some()
                || env_non_empty("PARISH_CLOUD_API_KEY").is_some()
                || env_non_empty("PARISH_CLOUD_MODEL").is_some()
                || cli_cloud.provider.is_some()
                || cli_cloud.base_url.is_some()
                || cli_cloud.api_key.is_some()
                || cli_cloud.model.is_some());

        if !has_toml && !has_env && !has_cli && !has_legacy_cloud {
            continue;
        }

        // Start from base config
        let mut provider_str: Option<String> = None;
        let mut cat_base_url: Option<String> = None;
        let mut cat_api_key: Option<String> = base.api_key.clone();
        let mut cat_model: Option<String> = base.model.clone();

        // Layer 1: Legacy [cloud] for dialogue (lowest priority override)
        if category == InferenceCategory::Dialogue {
            if let Some(ref name) = toml_cfg.cloud.name {
                provider_str = Some(name.clone());
            }
            if let Some(ref url) = toml_cfg.cloud.base_url {
                cat_base_url = Some(url.clone());
            }
            if let Some(ref key) = toml_cfg.cloud.api_key {
                cat_api_key = Some(key.clone());
            }
            if let Some(ref m) = toml_cfg.cloud.model {
                cat_model = Some(m.clone());
            }
            // Legacy cloud env vars
            if let Some(val) = env_non_empty("PARISH_CLOUD_PROVIDER") {
                provider_str = Some(val);
            }
            if let Some(val) = env_non_empty("PARISH_CLOUD_BASE_URL") {
                cat_base_url = Some(val);
            }
            if let Some(val) = env_non_empty("PARISH_CLOUD_API_KEY") {
                cat_api_key = Some(val);
            }
            if let Some(val) = env_non_empty("PARISH_CLOUD_MODEL") {
                cat_model = Some(val);
            }
            // Legacy cloud CLI flags
            if let Some(ref val) = cli_cloud.provider {
                provider_str = Some(val.clone());
            }
            if let Some(ref val) = cli_cloud.base_url {
                cat_base_url = Some(val.clone());
            }
            if let Some(ref val) = cli_cloud.api_key {
                cat_api_key = Some(val.clone());
            }
            if let Some(ref val) = cli_cloud.model {
                cat_model = Some(val.clone());
            }
        }

        // Layer 2: TOML [provider.<category>] overrides legacy cloud
        if let Some(ref toml_ov) = toml_override {
            if let Some(ref name) = toml_ov.name {
                provider_str = Some(name.clone());
            }
            if let Some(ref url) = toml_ov.base_url {
                cat_base_url = Some(url.clone());
            }
            if let Some(ref key) = toml_ov.api_key {
                cat_api_key = Some(key.clone());
            }
            if let Some(ref m) = toml_ov.model {
                cat_model = Some(m.clone());
            }
        }

        // Layer 3: Per-category env vars
        let prefix = category.env_prefix();
        if let Some(val) = env_non_empty(&format!("{prefix}_PROVIDER")) {
            provider_str = Some(val);
        }
        if let Some(val) = env_non_empty(&format!("{prefix}_BASE_URL")) {
            cat_base_url = Some(val);
        }
        if let Some(val) = env_non_empty(&format!("{prefix}_API_KEY")) {
            cat_api_key = Some(val);
        }
        if let Some(val) = env_non_empty(&format!("{prefix}_MODEL")) {
            cat_model = Some(val);
        }

        // Layer 4: Per-category CLI flags
        if let Some(cli_ov) = cli_override {
            if let Some(ref val) = cli_ov.provider {
                provider_str = Some(val.clone());
            }
            if let Some(ref val) = cli_ov.base_url {
                cat_base_url = Some(val.clone());
            }
            if let Some(ref val) = cli_ov.api_key {
                cat_api_key = Some(val.clone());
            }
            if let Some(ref val) = cli_ov.model {
                cat_model = Some(val.clone());
            }
        }

        // Resolve provider: if overridden use that, else inherit base
        let provider = match &provider_str {
            Some(s) => Provider::from_str_loose(s)?,
            None => base.provider.clone(),
        };

        // Resolve base URL: if overridden use that, else use provider default or base
        let resolved_base_url = match cat_base_url {
            Some(url) if !url.is_empty() => url,
            _ => {
                if provider_str.is_some() {
                    // Provider was overridden, use its default URL
                    provider.default_base_url().to_string()
                } else {
                    base.base_url.clone()
                }
            }
        };

        // Filter empty strings
        let cat_api_key = cat_api_key.filter(|s| !s.is_empty());
        let cat_model = cat_model.filter(|s| !s.is_empty());

        // Validate
        if provider.requires_api_key() && cat_api_key.is_none() {
            return Err(ParishError::Config(format!(
                "{} {:?} provider requires an API key. Set {}_API_KEY or --{}-api-key.",
                category.name(),
                provider,
                prefix,
                category.name()
            )));
        }
        if provider == Provider::Custom && resolved_base_url.is_empty() {
            return Err(ParishError::Config(format!(
                "{} custom provider requires a base_url. Set {}_BASE_URL or --{}-base-url.",
                category.name(),
                prefix,
                category.name()
            )));
        }

        result.insert(
            category,
            CategoryConfig {
                provider,
                base_url: resolved_base_url,
                api_key: cat_api_key,
                model: cat_model,
            },
        );
    }

    Ok(result)
}

/// Returns the value of an environment variable if it exists and is non-empty.
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Loads the TOML config from the given path or default location.
fn load_toml(config_path: Option<&Path>) -> Result<TomlConfig, ParishError> {
    if let Some(path) = config_path {
        read_toml_config(path)
    } else {
        let cwd_path = Path::new("parish.toml");
        if cwd_path.exists() {
            read_toml_config(cwd_path)
        } else {
            Ok(TomlConfig::default())
        }
    }
}

/// Reads and parses a TOML config file. Returns default config if file doesn't exist.
fn read_toml_config(path: &Path) -> Result<TomlConfig, ParishError> {
    if !path.exists() {
        return Ok(TomlConfig::default());
    }
    let content = std::fs::read_to_string(path).map_err(|e| {
        ParishError::Config(format!(
            "failed to read config file {}: {}",
            path.display(),
            e
        ))
    })?;
    toml::from_str(&content).map_err(|e| {
        ParishError::Config(format!(
            "failed to parse config file {}: {}",
            path.display(),
            e
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Clears all PARISH_ env vars so tests don't interfere with each other.
    fn clear_parish_env() {
        // SAFETY: Tests run single-threaded via `cargo test -- --test-threads=1`
        // or are independent enough that concurrent env mutation is acceptable.
        unsafe {
            std::env::remove_var("PARISH_PROVIDER");
            std::env::remove_var("PARISH_BASE_URL");
            std::env::remove_var("PARISH_OLLAMA_URL");
            std::env::remove_var("PARISH_API_KEY");
            std::env::remove_var("PARISH_MODEL");
            std::env::remove_var("PARISH_CLOUD_PROVIDER");
            std::env::remove_var("PARISH_CLOUD_BASE_URL");
            std::env::remove_var("PARISH_CLOUD_API_KEY");
            std::env::remove_var("PARISH_CLOUD_MODEL");
            // Per-category env vars
            for cat in &["DIALOGUE", "SIMULATION", "INTENT"] {
                std::env::remove_var(format!("PARISH_{cat}_PROVIDER"));
                std::env::remove_var(format!("PARISH_{cat}_BASE_URL"));
                std::env::remove_var(format!("PARISH_{cat}_API_KEY"));
                std::env::remove_var(format!("PARISH_{cat}_MODEL"));
            }
        }
    }

    #[test]
    fn test_provider_from_str_loose() {
        assert_eq!(
            Provider::from_str_loose("ollama").unwrap(),
            Provider::Ollama
        );
        assert_eq!(
            Provider::from_str_loose("OLLAMA").unwrap(),
            Provider::Ollama
        );
        assert_eq!(
            Provider::from_str_loose("lmstudio").unwrap(),
            Provider::LmStudio
        );
        assert_eq!(
            Provider::from_str_loose("lm-studio").unwrap(),
            Provider::LmStudio
        );
        assert_eq!(
            Provider::from_str_loose("lm_studio").unwrap(),
            Provider::LmStudio
        );
        assert_eq!(
            Provider::from_str_loose("openrouter").unwrap(),
            Provider::OpenRouter
        );
        assert_eq!(
            Provider::from_str_loose("open-router").unwrap(),
            Provider::OpenRouter
        );
        assert_eq!(
            Provider::from_str_loose("custom").unwrap(),
            Provider::Custom
        );
        assert!(Provider::from_str_loose("unknown").is_err());
    }

    #[test]
    fn test_provider_default_base_url() {
        assert_eq!(
            Provider::Ollama.default_base_url(),
            "http://localhost:11434"
        );
        assert_eq!(
            Provider::LmStudio.default_base_url(),
            "http://localhost:1234"
        );
        assert_eq!(
            Provider::OpenRouter.default_base_url(),
            "https://openrouter.ai/api"
        );
        assert_eq!(Provider::Custom.default_base_url(), "");
    }

    #[test]
    fn test_provider_requirements() {
        assert!(!Provider::Ollama.requires_api_key());
        assert!(!Provider::LmStudio.requires_api_key());
        assert!(Provider::OpenRouter.requires_api_key());
        assert!(!Provider::Custom.requires_api_key());

        assert!(!Provider::Ollama.requires_model());
        assert!(Provider::LmStudio.requires_model());
        assert!(Provider::OpenRouter.requires_model());
        assert!(Provider::Custom.requires_model());
    }

    #[test]
    fn test_resolve_config_defaults() {
        clear_parish_env();

        let cli = CliOverrides::default();
        let config = resolve_config(Some(Path::new("/nonexistent/parish.toml")), &cli).unwrap();
        assert_eq!(config.provider, Provider::Ollama);
        assert_eq!(config.base_url, "http://localhost:11434");
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    fn test_resolve_config_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "lmstudio"
base_url = "http://myhost:5555"
model = "my-model"
"#
        )
        .unwrap();

        clear_parish_env();

        let cli = CliOverrides::default();
        let config = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config.provider, Provider::LmStudio);
        assert_eq!(config.base_url, "http://myhost:5555");
        assert_eq!(config.model.as_deref(), Some("my-model"));
    }

    #[test]
    fn test_resolve_config_cli_overrides_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "lmstudio"
model = "toml-model"
"#
        )
        .unwrap();

        clear_parish_env();

        let cli = CliOverrides {
            provider: None,
            base_url: None,
            api_key: None,
            model: Some("cli-model".to_string()),
        };
        let config = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config.provider, Provider::LmStudio);
        assert_eq!(config.model.as_deref(), Some("cli-model"));
    }

    #[test]
    fn test_resolve_config_openrouter_requires_api_key() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: None,
            model: Some("anthropic/claude-sonnet-4-20250514".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
    }

    #[test]
    fn test_resolve_config_openrouter_with_api_key() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: Some("sk-test-key".to_string()),
            model: Some("anthropic/claude-sonnet-4-20250514".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.base_url, "https://openrouter.ai/api");
        assert_eq!(config.api_key.as_deref(), Some("sk-test-key"));
    }

    #[test]
    fn test_resolve_config_custom_requires_base_url() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("custom".to_string()),
            base_url: None,
            api_key: None,
            model: Some("some-model".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("base_url"), "got: {}", err_msg);
    }

    #[test]
    fn test_resolve_config_empty_strings_filtered() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: None,
            base_url: None,
            api_key: Some(String::new()),
            model: Some(String::new()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    fn test_read_toml_config_missing_file() {
        let config = read_toml_config(Path::new("/nonexistent/parish.toml")).unwrap();
        assert!(config.provider.name.is_none());
    }

    #[test]
    fn test_read_toml_config_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        std::fs::write(&path, "").unwrap();
        let config = read_toml_config(&path).unwrap();
        assert!(config.provider.name.is_none());
    }

    #[test]
    fn test_read_toml_config_invalid_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        std::fs::write(&path, "this is not valid toml {{{{").unwrap();
        let result = read_toml_config(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_toml_config_minimal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        std::fs::write(&path, "[provider]\nname = \"ollama\"\n").unwrap();
        let config = read_toml_config(&path).unwrap();
        assert_eq!(config.provider.name.as_deref(), Some("ollama"));
    }

    // --- Cloud config tests ---

    #[test]
    fn test_resolve_cloud_config_none_when_not_configured() {
        clear_parish_env();
        let cli = CliCloudOverrides::default();
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_cloud_config_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"

[cloud]
name = "openrouter"
api_key = "sk-test"
model = "anthropic/claude-sonnet-4-20250514"
"#
        )
        .unwrap();

        clear_parish_env();

        let cli = CliCloudOverrides::default();
        let config = resolve_cloud_config(Some(&path), &cli).unwrap().unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.base_url, "https://openrouter.ai/api");
        assert_eq!(config.api_key.as_deref(), Some("sk-test"));
        assert_eq!(config.model, "anthropic/claude-sonnet-4-20250514");
    }

    #[test]
    fn test_resolve_cloud_config_from_cli() {
        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: Some("sk-cli".to_string()),
            model: Some("gpt-4".to_string()),
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.api_key.as_deref(), Some("sk-cli"));
        assert_eq!(config.model, "gpt-4");
    }

    #[test]
    fn test_resolve_cloud_config_requires_model() {
        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: Some("sk-test".to_string()),
            model: None,
        };
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("model"), "got: {}", err_msg);
    }

    #[test]
    fn test_resolve_cloud_config_openrouter_requires_api_key() {
        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: None,
            model: Some("claude-3".to_string()),
        };
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
    }

    #[test]
    fn test_resolve_cloud_config_defaults_to_openrouter() {
        clear_parish_env();

        // Only model + key, no explicit provider name
        let cli = CliCloudOverrides {
            provider: None,
            base_url: None,
            api_key: Some("sk-test".to_string()),
            model: Some("my-model".to_string()),
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.base_url, "https://openrouter.ai/api");
    }

    #[test]
    fn test_resolve_cloud_config_cli_overrides_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[cloud]
name = "openrouter"
api_key = "sk-toml"
model = "toml-model"
"#
        )
        .unwrap();

        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: None,
            base_url: None,
            api_key: None,
            model: Some("cli-model".to_string()),
        };
        let config = resolve_cloud_config(Some(&path), &cli).unwrap().unwrap();
        assert_eq!(config.model, "cli-model");
        assert_eq!(config.api_key.as_deref(), Some("sk-toml"));
    }

    #[test]
    fn test_resolve_cloud_config_ollama_no_key_needed() {
        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: Some("ollama".to_string()),
            base_url: Some("http://remote-ollama:11434".to_string()),
            api_key: None,
            model: Some("llama3".to_string()),
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider, Provider::Ollama);
        assert_eq!(config.base_url, "http://remote-ollama:11434");
        assert_eq!(config.model, "llama3");
    }

    // --- Per-category config tests ---

    #[test]
    fn test_category_names() {
        assert_eq!(InferenceCategory::Dialogue.name(), "dialogue");
        assert_eq!(InferenceCategory::Simulation.name(), "simulation");
        assert_eq!(InferenceCategory::Intent.name(), "intent");
    }

    #[test]
    fn test_resolve_category_configs_empty_when_no_overrides() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();
        assert!(configs.is_empty());
    }

    #[test]
    fn test_resolve_category_configs_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"
model = "qwen3:14b"

[provider.dialogue]
name = "openrouter"
base_url = "https://openrouter.ai/api"
api_key = "sk-cat-test"
model = "anthropic/claude-sonnet-4-20250514"

[provider.intent]
model = "qwen3:1.5b"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        // Dialogue should be overridden to OpenRouter
        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.provider, Provider::OpenRouter);
        assert_eq!(dialogue.base_url, "https://openrouter.ai/api");
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-cat-test"));
        assert_eq!(
            dialogue.model.as_deref(),
            Some("anthropic/claude-sonnet-4-20250514")
        );

        // Intent should inherit Ollama but override model
        let intent = configs.get(&InferenceCategory::Intent).unwrap();
        assert_eq!(intent.provider, Provider::Ollama);
        assert_eq!(intent.base_url, "http://localhost:11434");
        assert_eq!(intent.model.as_deref(), Some("qwen3:1.5b"));

        // Simulation should not be present (no overrides)
        assert!(!configs.contains_key(&InferenceCategory::Simulation));
    }

    #[test]
    fn test_resolve_category_configs_legacy_cloud_maps_to_dialogue() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            api_key: Some("sk-legacy".to_string()),
            model: Some("gpt-4".to_string()),
        };
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();

        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.provider, Provider::OpenRouter);
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-legacy"));
        assert_eq!(dialogue.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn test_resolve_category_configs_toml_overrides_legacy_cloud() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"

[cloud]
name = "openrouter"
api_key = "sk-cloud-legacy"
model = "legacy-model"

[provider.dialogue]
name = "custom"
base_url = "https://my-api.example.com"
api_key = "sk-new"
model = "new-model"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        // [provider.dialogue] should override [cloud]
        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.provider, Provider::Custom);
        assert_eq!(dialogue.base_url, "https://my-api.example.com");
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-new"));
        assert_eq!(dialogue.model.as_deref(), Some("new-model"));
    }

    #[test]
    fn test_resolve_category_configs_cli_overrides() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let mut categories = HashMap::new();
        categories.insert(
            "simulation".to_string(),
            CliOverrides {
                provider: Some("openrouter".to_string()),
                base_url: None,
                api_key: Some("sk-sim".to_string()),
                model: Some("sim-model".to_string()),
            },
        );
        let cli_cat = CliCategoryOverrides { categories };
        let cli_cloud = CliCloudOverrides::default();
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();

        let sim = configs.get(&InferenceCategory::Simulation).unwrap();
        assert_eq!(sim.provider, Provider::OpenRouter);
        assert_eq!(sim.base_url, "https://openrouter.ai/api");
        assert_eq!(sim.api_key.as_deref(), Some("sk-sim"));
        assert_eq!(sim.model.as_deref(), Some("sim-model"));
    }

    #[test]
    fn test_resolve_category_configs_validates_api_key() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        let mut categories = HashMap::new();
        categories.insert(
            "intent".to_string(),
            CliOverrides {
                provider: Some("openrouter".to_string()),
                base_url: None,
                api_key: None,
                model: Some("model".to_string()),
            },
        );
        let cli_cat = CliCategoryOverrides { categories };
        let cli_cloud = CliCloudOverrides::default();
        let result =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("intent"), "got: {}", err_msg);
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
    }

    #[test]
    fn test_resolve_category_configs_inherits_base_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"
base_url = "http://remote-ollama:11434"

[provider.simulation]
model = "qwen3:8b"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::Ollama,
            base_url: "http://remote-ollama:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        let sim = configs.get(&InferenceCategory::Simulation).unwrap();
        assert_eq!(sim.provider, Provider::Ollama);
        // Should inherit the base URL since provider wasn't overridden
        assert_eq!(sim.base_url, "http://remote-ollama:11434");
        assert_eq!(sim.model.as_deref(), Some("qwen3:8b"));
    }
}
