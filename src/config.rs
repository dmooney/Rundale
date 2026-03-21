//! Provider configuration for LLM inference backends.
//!
//! Supports Ollama (local, default), LM Studio (local), OpenRouter (cloud),
//! and custom OpenAI-compatible endpoints. Configuration is resolved from
//! a TOML file, environment variables, and CLI flags (in that priority order).

use crate::error::ParishError;
use serde::Deserialize;
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

/// Raw TOML file structure for `parish.toml`.
#[derive(Debug, Deserialize, Default)]
struct TomlConfig {
    #[serde(default)]
    provider: TomlProvider,
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

/// Returns the value of an environment variable if it exists and is non-empty.
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
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
}
