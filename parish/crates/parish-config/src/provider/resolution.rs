//! Provider/cloud config resolution from file, env vars, and CLI flags.
//!
//! The 4-layer precedence (TOML → env → CLI, with provider defaults beneath)
//! and the resolved [`ProviderConfig`] / [`CloudConfig`] outputs. Split out of
//! the monolithic `provider` module (#1200).

use std::collections::HashMap;
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

/// Resolved provider configuration for one inference category.
///
/// This lives in `parish-config` so every runtime can apply the same
/// per-category routing contract. A category absent from the resolved map
/// inherits the base [`ProviderConfig`].
#[derive(Debug, Clone)]
pub struct CategoryConfig {
    pub provider: Provider,
    pub base_url: String,
    pub api_key: Option<String>,
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

/// Resolves `PARISH_<CATEGORY>_{PROVIDER,BASE_URL,MODEL}` overrides.
///
/// Only categories with at least one non-empty override are returned. Missing
/// fields inherit from `base`; when the provider itself is overridden, its
/// default URL and standard API-key environment variable are used. This is
/// the shared environment-only surface used by runtimes that do not expose
/// the `parish-engine` binary's TOML/CLI category flags (notably
/// `parish-server`).
pub fn resolve_category_env_configs(
    base: &ProviderConfig,
) -> Result<HashMap<InferenceCategory, CategoryConfig>, ParishError> {
    let mut result = HashMap::new();

    for category in InferenceCategory::ALL {
        let prefix = category.env_prefix();
        let provider_override = env_non_empty(&format!("{prefix}_PROVIDER"));
        let base_url_override = env_non_empty(&format!("{prefix}_BASE_URL"));
        let model_override = env_non_empty(&format!("{prefix}_MODEL"));

        if provider_override.is_none() && base_url_override.is_none() && model_override.is_none() {
            continue;
        }

        let provider = match provider_override.as_deref() {
            Some(value) => Provider::from_str_loose(value)?,
            None => base.provider.clone(),
        };
        let base_url = base_url_override.unwrap_or_else(|| {
            if provider_override.is_some() {
                provider.default_base_url().to_string()
            } else {
                base.base_url.clone()
            }
        });
        let api_key = provider
            .api_key_env_var()
            .and_then(env_non_empty)
            .or_else(|| {
                if provider_override.is_none() {
                    base.api_key.clone()
                } else {
                    None
                }
            });
        let model = model_override.or_else(|| {
            if provider_override.is_none() {
                base.model.clone()
            } else if provider.id() == "ollama" {
                None
            } else {
                provider.preset_model(category).map(String::from)
            }
        });

        if provider.requires_api_key() && api_key.is_none() {
            let hint = provider
                .api_key_env_var()
                .unwrap_or("the provider API key env var");
            return Err(ParishError::Config(format!(
                "{} {} provider requires an API key. Set {}.",
                category.name(),
                provider.id(),
                hint
            )));
        }
        if provider.needs_base_url_from_user() && base_url.is_empty() {
            return Err(ParishError::Config(format!(
                "{} custom provider requires a base_url. Set {}_BASE_URL.",
                category.name(),
                prefix
            )));
        }

        result.insert(
            category,
            CategoryConfig {
                provider,
                base_url,
                api_key,
                model,
            },
        );
    }

    Ok(result)
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

    // Default to native Google Gemini for cloud. If the provider mod is absent
    // from this deployment, surface that as a config error instead of
    // panicking — operators who never set `PARISH_CLOUD_PROVIDER` should
    // get an actionable message, not a crashed binary (codex P1).
    let provider = match &raw.provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::from_id("google").ok_or_else(|| {
            ParishError::Config(
                "Cloud provider default 'google' is not registered. \
                 Set PARISH_CLOUD_PROVIDER (or [llm.cloud].provider) to a \
                 registered provider id, or install the Google \
                 provider mod under mods/google-provider/."
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
    let model = raw
        .model
        .filter(|s| !s.is_empty())
        .or_else(|| (provider.id() == "google").then(|| "gemini-3.7-flash".to_string()));

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

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
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
