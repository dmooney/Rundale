//! Provider/cloud config resolution from file, env vars, and CLI flags.
//!
//! The 4-layer precedence (TOML → env → CLI, with provider defaults beneath)
//! and the resolved [`ProviderConfig`] / [`CloudConfig`] outputs. Split out of
//! the monolithic `provider` module (#1200).

use std::path::Path;

use parish_types::ParishError;
use serde::Deserialize;

use super::category::InferenceCategory;
use super::schema::Provider;

/// Resolved provider configuration ready for use.
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
#[derive(Debug, Clone)]
pub struct CloudConfig {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
}

/// CLI-provided overrides for cloud provider configuration.
#[derive(Debug, Default)]
pub struct CliCloudOverrides {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

/// Raw TOML file structure for `parish.toml`.
///
/// `pub(super)` so the `provider` test module (a sibling of this module) can
/// inspect parsed sections via [`read_toml_config`].
#[derive(Debug, Deserialize, Default)]
pub(super) struct TomlConfig {
    #[serde(default)]
    pub(super) provider: TomlProvider,
    #[serde(default)]
    pub(super) cloud: TomlCloud,
}

/// The `[provider]` section of the TOML config.
#[derive(Debug, Deserialize, Default)]
pub(super) struct TomlProvider {
    pub(super) name: Option<String>,
    pub(super) base_url: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
}

/// The `[cloud]` section of the TOML config for cloud dialogue provider.
#[derive(Debug, Deserialize, Default)]
pub(super) struct TomlCloud {
    pub(super) name: Option<String>,
    pub(super) base_url: Option<String>,
    pub(super) api_key: Option<String>,
    pub(super) model: Option<String>,
}

/// CLI-provided overrides for provider configuration.
#[derive(Debug, Default)]
pub struct CliOverrides {
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

impl ProviderConfig {
    /// Returns a display-friendly provider name.
    pub fn provider_display(&self) -> String {
        self.provider.id().to_string()
    }
}

struct RawLayers {
    provider_str: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

fn apply_env_and_cli_layers(
    mut raw: RawLayers,
    env_prefix: &str,
    cli_provider: Option<&str>,
    cli_base_url: Option<&str>,
    cli_model: Option<&str>,
) -> RawLayers {
    if let Some(val) = env_non_empty(&format!("{env_prefix}_PROVIDER")) {
        raw.provider_str = Some(val);
    }
    if let Some(val) = env_non_empty(&format!("{env_prefix}_BASE_URL")) {
        raw.base_url = Some(val);
    }
    if let Some(val) = env_non_empty(&format!("{env_prefix}_MODEL")) {
        raw.model = Some(val);
    }

    if let Some(val) = cli_provider {
        raw.provider_str = Some(val.to_string());
    }
    if let Some(val) = cli_base_url {
        raw.base_url = Some(val.to_string());
    }
    if let Some(val) = cli_model {
        raw.model = Some(val.to_string());
    }

    raw
}

fn load_toml(config_path: Option<&Path>) -> Result<TomlConfig, ParishError> {
    match config_path {
        Some(path) => read_toml_config(path),
        None => Ok(TomlConfig::default()),
    }
}

/// Resolves provider configuration from file, env vars, and CLI flags.
pub fn resolve_config(
    config_path: Option<&Path>,
    cli: &CliOverrides,
) -> Result<ProviderConfig, ParishError> {
    let toml_cfg = load_toml(config_path)?;

    let toml_raw = RawLayers {
        provider_str: toml_cfg.provider.name,
        base_url: toml_cfg.provider.base_url,
        api_key: toml_cfg.provider.api_key,
        model: toml_cfg.provider.model,
    };
    let mut raw = apply_env_and_cli_layers(
        toml_raw,
        "PARISH",
        cli.provider.as_deref(),
        cli.base_url.as_deref(),
        cli.model.as_deref(),
    );

    // Deprecated PARISH_OLLAMA_URL fallback
    if raw.base_url.is_none()
        && let Some(val) = env_non_empty("PARISH_OLLAMA_URL")
    {
        tracing::warn!("PARISH_OLLAMA_URL is deprecated, use PARISH_BASE_URL instead");
        raw.base_url = Some(val);
    }

    let provider = match &raw.provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::default(),
    };

    let mut api_key = raw.api_key;
    if let Some(val) = provider.api_key_env_var().and_then(env_non_empty) {
        api_key = Some(val);
    }

    let base_url = raw
        .base_url
        .unwrap_or_else(|| provider.default_base_url().to_string());
    let api_key = api_key.filter(|s| !s.is_empty());
    let model = raw.model.filter(|s| !s.is_empty());

    // Fall back to the provider's Dialogue preset if no model is configured.
    // Skipped for Ollama: leaving model None lets setup_ollama_with_config pick
    // a hardware-matched tier instead of the static preset tag.
    let model = if provider.id() == "ollama" {
        model
    } else {
        model.or_else(|| {
            provider
                .preset_model(InferenceCategory::Dialogue)
                .map(String::from)
        })
    };

    if provider.requires_api_key() && api_key.is_none() {
        let hint = provider
            .api_key_env_var()
            .unwrap_or("the provider API key env var");
        return Err(ParishError::Config(format!(
            "{} provider requires an API key. Set {}.",
            provider.id(),
            hint
        )));
    }
    if provider.needs_base_url_from_user() && base_url.is_empty() {
        return Err(ParishError::Config(format!(
            "{} provider requires a base_url. Set PARISH_BASE_URL or --base-url.",
            provider.id()
        )));
    }

    Ok(ProviderConfig {
        provider,
        base_url,
        api_key,
        model,
    })
}

/// Resolves cloud provider configuration from file, env vars, and CLI flags.
pub fn resolve_cloud_config(
    config_path: Option<&Path>,
    cli: &CliCloudOverrides,
) -> Result<Option<CloudConfig>, ParishError> {
    let toml_cfg = load_toml(config_path)?;

    let toml_raw = RawLayers {
        provider_str: toml_cfg.cloud.name,
        base_url: toml_cfg.cloud.base_url,
        api_key: toml_cfg.cloud.api_key,
        model: toml_cfg.cloud.model,
    };
    let raw = apply_env_and_cli_layers(
        toml_raw,
        "PARISH_CLOUD",
        cli.provider.as_deref(),
        cli.base_url.as_deref(),
        cli.model.as_deref(),
    );

    if raw.provider_str.is_none()
        && raw.base_url.is_none()
        && raw.api_key.is_none()
        && raw.model.is_none()
    {
        return Ok(None);
    }

    // Default to OpenRouter for cloud. If the openrouter mod is absent
    // from this deployment, surface that as a config error instead of
    // panicking — operators who never set `PARISH_CLOUD_PROVIDER` should
    // get an actionable message, not a crashed binary (codex P1).
    let provider = match &raw.provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::from_id("openrouter").ok_or_else(|| {
            ParishError::Config(
                "Cloud provider default 'openrouter' is not registered. \
                 Set PARISH_CLOUD_PROVIDER (or [llm.cloud].provider) to a \
                 registered provider id, or install the openrouter \
                 provider mod under mods/openrouter-provider/."
                    .into(),
            )
        })?,
    };

    let mut api_key = raw.api_key.filter(|s| !s.is_empty());
    if let Some(val) = provider.api_key_env_var().and_then(env_non_empty) {
        api_key = Some(val);
    }

    let base_url = raw
        .base_url
        .unwrap_or_else(|| provider.default_base_url().to_string());
    let model = raw.model.filter(|s| !s.is_empty());

    let model = model.ok_or_else(|| {
        ParishError::Config(
            "Cloud provider requires a model name. Set PARISH_CLOUD_MODEL or --cloud-model."
                .to_string(),
        )
    })?;

    if provider.requires_api_key() && api_key.is_none() {
        let hint = provider
            .api_key_env_var()
            .unwrap_or("the provider API key env var");
        return Err(ParishError::Config(format!(
            "Cloud {} provider requires an API key. Set {}.",
            provider.id(),
            hint
        )));
    }
    if provider.needs_base_url_from_user() && base_url.is_empty() {
        return Err(ParishError::Config(format!(
            "Cloud {} provider requires a base_url. Set PARISH_CLOUD_BASE_URL or --cloud-base-url.",
            provider.id()
        )));
    }

    Ok(Some(CloudConfig {
        provider,
        base_url,
        api_key,
        model,
    }))
}

/// Reads an env var, trimming surrounding whitespace and mapping empty (or
/// whitespace-only) values to `None`. Trimming matters for API keys: secret
/// stores and `echo key >> .env` leave a trailing newline, which poisons the
/// `Authorization` header downstream and fails every cloud-inference call.
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

pub(super) fn read_toml_config(path: &Path) -> Result<TomlConfig, ParishError> {
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
