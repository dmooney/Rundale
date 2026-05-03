//! Provider configuration for LLM inference backends.
//!
//! Supports Simulator (offline, default), Ollama (local), LM Studio (local), vLLM (local),
//! and several cloud providers: OpenRouter, OpenAI, Google (Gemini), Groq,
//! xAI (Grok), Mistral, DeepSeek, Together AI, NVIDIA NIM, and Anthropic
//! (Claude) via the native Messages API. A custom OpenAI-compatible
//! endpoint is also available. Configuration is resolved from a TOML file,
//! environment variables, and CLI flags (in that priority order).

use parish_types::ParishError;
use serde::Deserialize;
use std::path::Path;

/// Default base URL for each provider.
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_LMSTUDIO_URL: &str = "http://localhost:1234";
const DEFAULT_OPENROUTER_URL: &str = "https://openrouter.ai/api";
const DEFAULT_VLLM_URL: &str = "http://localhost:8000";
const DEFAULT_OPENAI_URL: &str = "https://api.openai.com";
const DEFAULT_GOOGLE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/openai";
const DEFAULT_GROQ_URL: &str = "https://api.groq.com/openai";
const DEFAULT_XAI_URL: &str = "https://api.x.ai";
const DEFAULT_MISTRAL_URL: &str = "https://api.mistral.ai";
const DEFAULT_DEEPSEEK_URL: &str = "https://api.deepseek.com";
const DEFAULT_TOGETHER_URL: &str = "https://api.together.xyz";
const DEFAULT_NVIDIA_NIM_URL: &str = "https://integrate.api.nvidia.com";
const DEFAULT_ANTHROPIC_URL: &str = "https://api.anthropic.com";

/// Supported LLM provider backends.
///
/// All providers (except Simulator and Anthropic) use the OpenAI-compatible
/// chat completions API (`/v1/chat/completions`). Simulator is the default.
/// Ollama includes auto-start, GPU detection, and model pulling features.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Provider {
    /// Local Ollama server with auto-management.
    Ollama,
    /// Local LM Studio server.
    LmStudio,
    /// OpenRouter cloud gateway (requires API key).
    OpenRouter,
    /// Local vLLM inference server (OpenAI-compatible, requires model name).
    Vllm,
    /// OpenAI API (requires API key).
    OpenAi,
    /// Google Gemini via OpenAI-compatible endpoint (requires API key).
    Google,
    /// Groq cloud inference (requires API key).
    Groq,
    /// xAI Grok models (requires API key).
    Xai,
    /// Mistral AI (requires API key).
    Mistral,
    /// DeepSeek (requires API key).
    DeepSeek,
    /// Together AI (requires API key).
    Together,
    /// NVIDIA NIM cloud inference via the OpenAI-compatible
    /// `/v1/chat/completions` endpoint (requires API key).
    NvidiaNim,
    /// Anthropic Claude via the native Messages API (requires API key).
    ///
    /// Unlike every other cloud provider, Anthropic does not use the
    /// OpenAI `/v1/chat/completions` schema. Requests are routed through
    /// the dedicated `AnthropicClient` (native `/v1/messages` with
    /// `x-api-key` + `anthropic-version` headers).
    Anthropic,
    /// Any OpenAI-compatible endpoint (requires base_url).
    Custom,
    /// Browser-side WebGPU inference (web build only).
    ///
    /// When selected, the server forwards each prompt over the existing
    /// WebSocket to the calling browser, which runs the model locally via
    /// `transformers.js` on top of WebGPU and streams tokens back. Requires
    /// the `webgpu-provider` feature flag and a Chromium-based browser
    /// (Chrome 113+, Edge, or Safari Technology Preview). Selecting this
    /// provider in Tauri or headless CLI returns a clear error because there
    /// is no browser bridge available.
    WebGpu,
    /// Built-in offline simulator — generates funny nonsense locally,
    /// no network, no model download. Default when no provider is configured.
    #[default]
    Simulator,
}

impl Provider {
    /// All available providers.
    pub const ALL: [Provider; 15] = [
        Provider::Ollama,
        Provider::LmStudio,
        Provider::OpenRouter,
        Provider::Vllm,
        Provider::OpenAi,
        Provider::Google,
        Provider::Groq,
        Provider::Xai,
        Provider::Mistral,
        Provider::DeepSeek,
        Provider::Together,
        Provider::NvidiaNim,
        Provider::Anthropic,
        Provider::Custom,
        Provider::Simulator,
    ];

    /// Parses a provider name string (case-insensitive).
    pub fn from_str_loose(s: &str) -> Result<Self, ParishError> {
        match s.to_lowercase().as_str() {
            "ollama" => Ok(Provider::Ollama),
            "lmstudio" | "lm_studio" | "lm-studio" => Ok(Provider::LmStudio),
            "openrouter" | "open_router" | "open-router" => Ok(Provider::OpenRouter),
            "vllm" => Ok(Provider::Vllm),
            "openai" | "open_ai" | "open-ai" => Ok(Provider::OpenAi),
            "google" | "gemini" => Ok(Provider::Google),
            "groq" => Ok(Provider::Groq),
            "xai" | "x-ai" | "grok" => Ok(Provider::Xai),
            "mistral" => Ok(Provider::Mistral),
            "deepseek" | "deep-seek" | "deep_seek" => Ok(Provider::DeepSeek),
            "together" | "togetherai" | "together-ai" | "together_ai" => Ok(Provider::Together),
            "nvidia-nim" | "nvidia_nim" | "nvidianim" | "nim" | "nvidia" => Ok(Provider::NvidiaNim),
            "anthropic" | "claude" => Ok(Provider::Anthropic),
            "custom" => Ok(Provider::Custom),
            "webgpu" | "web-gpu" | "web_gpu" | "browser" => Ok(Provider::WebGpu),
            "simulator" | "sim" | "mock" => Ok(Provider::Simulator),
            other => Err(ParishError::Config(format!(
                "unknown provider '{}'. Expected: ollama, lmstudio, openrouter, vllm, openai, \
                 google, groq, xai, mistral, deepseek, together, nvidia-nim, anthropic, custom, webgpu, \
                 simulator",
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
            Provider::Vllm => DEFAULT_VLLM_URL,
            Provider::OpenAi => DEFAULT_OPENAI_URL,
            Provider::Google => DEFAULT_GOOGLE_URL,
            Provider::Groq => DEFAULT_GROQ_URL,
            Provider::Xai => DEFAULT_XAI_URL,
            Provider::Mistral => DEFAULT_MISTRAL_URL,
            Provider::DeepSeek => DEFAULT_DEEPSEEK_URL,
            Provider::Together => DEFAULT_TOGETHER_URL,
            Provider::NvidiaNim => DEFAULT_NVIDIA_NIM_URL,
            Provider::Anthropic => DEFAULT_ANTHROPIC_URL,
            Provider::Custom => "",
            // WebGPU has no HTTP endpoint — the "transport" is the per-session
            // WebSocket bridge to the player's browser.
            Provider::WebGpu => "",
            Provider::Simulator => "",
        }
    }

    /// Whether this provider requires an API key.
    pub fn requires_api_key(&self) -> bool {
        matches!(
            self,
            Provider::OpenRouter
                | Provider::OpenAi
                | Provider::Google
                | Provider::Groq
                | Provider::Xai
                | Provider::Mistral
                | Provider::DeepSeek
                | Provider::Together
                | Provider::NvidiaNim
                | Provider::Anthropic
        )
    }

    /// Whether this provider requires an explicit model name
    /// (no auto-detection available).
    ///
    /// WebGPU technically auto-selects a Hugging Face repo ID from the
    /// browser's GPU capabilities, but the user can still override via
    /// `/model <hf-repo-id>`, so we report `false` to mirror Ollama's
    /// "auto-detect" semantics.
    pub fn requires_model(&self) -> bool {
        !matches!(
            self,
            Provider::Ollama | Provider::Simulator | Provider::WebGpu
        )
    }

    /// The well-known environment variable that carries this provider's API key.
    ///
    /// Returns `None` for local providers (Ollama, LM Studio, vLLM, Simulator)
    /// and `Custom` — Custom provider keys must be set via TOML `api_key`.
    ///
    /// The returned name is the standard, provider-issued variable (e.g.
    /// `ANTHROPIC_API_KEY`) so users who already have it in their shell do not
    /// need to duplicate it under a `PARISH_` prefix.
    ///
    /// When adding a new provider, add a branch here AND update `.env.example`.
    pub fn api_key_env_var(&self) -> Option<&'static str> {
        match self {
            Provider::Anthropic => Some("ANTHROPIC_API_KEY"),
            Provider::OpenAi => Some("OPENAI_API_KEY"),
            Provider::OpenRouter => Some("OPENROUTER_API_KEY"),
            Provider::Google => Some("GOOGLE_API_KEY"),
            Provider::Groq => Some("GROQ_API_KEY"),
            Provider::Xai => Some("XAI_API_KEY"),
            Provider::Mistral => Some("MISTRAL_API_KEY"),
            Provider::DeepSeek => Some("DEEPSEEK_API_KEY"),
            Provider::Together => Some("TOGETHER_API_KEY"),
            Provider::NvidiaNim => Some("NVIDIA_API_KEY"),
            _ => None,
        }
    }

    /// Returns true if this provider is ready to be used (either local, or has its
    /// API key configured in the environment).
    pub fn is_configured_in_env(&self) -> bool {
        if !self.requires_api_key() {
            return true;
        }
        if let Some(var) = self.api_key_env_var() {
            std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false)
        } else {
            false
        }
    }
}

/// Inference categories that can each have independent provider/model/key settings.
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
    /// NPC arrival reactions/greetings (short timeout, fast model).
    Reaction,
}

impl InferenceCategory {
    /// All defined inference categories.
    pub const ALL: [InferenceCategory; 4] = [
        InferenceCategory::Dialogue,
        InferenceCategory::Simulation,
        InferenceCategory::Intent,
        InferenceCategory::Reaction,
    ];

    /// Returns the lowercase name used in TOML keys, env var prefixes, and CLI flags.
    pub fn name(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "dialogue",
            InferenceCategory::Simulation => "simulation",
            InferenceCategory::Intent => "intent",
            InferenceCategory::Reaction => "reaction",
        }
    }

    /// Parses a category name (case-insensitive).
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "dialogue" => Some(InferenceCategory::Dialogue),
            "simulation" => Some(InferenceCategory::Simulation),
            "intent" => Some(InferenceCategory::Intent),
            "reaction" => Some(InferenceCategory::Reaction),
            _ => None,
        }
    }

    /// Returns the SCREAMING_CASE prefix used in environment variables.
    pub fn env_prefix(&self) -> &'static str {
        match self {
            InferenceCategory::Dialogue => "PARISH_DIALOGUE",
            InferenceCategory::Simulation => "PARISH_SIMULATION",
            InferenceCategory::Intent => "PARISH_INTENT",
            InferenceCategory::Reaction => "PARISH_REACTION",
        }
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
    /// Provider name: "ollama", "lmstudio", "openrouter", "vllm", "custom".
    name: Option<String>,
    /// Base URL override.
    base_url: Option<String>,
    /// API key for cloud providers.
    api_key: Option<String>,
    /// Model name override.
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
    /// `--model` flag value.
    pub model: Option<String>,
}

impl ProviderConfig {
    /// Returns a display-friendly provider name.
    pub fn provider_display(&self) -> String {
        match self.provider {
            Provider::NvidiaNim => "nvidia-nim".to_string(),
            _ => format!("{:?}", self.provider).to_lowercase(),
        }
    }
}

/// Resolves provider configuration from file, env vars, and CLI flags.
///
/// Resolution order (later overrides earlier):
/// 1. TOML `[provider]` section
/// 2. `PARISH_PROVIDER`, `PARISH_BASE_URL`, `PARISH_MODEL` env vars
/// 3. CLI flags (provider, base_url, model — no api_key flag)
/// 4. Standard provider API key env var (e.g. `ANTHROPIC_API_KEY`), read
///    after provider is known so the right variable is used.
///    TOML `[provider] api_key` is an explicit escape hatch overridden by step 4.
///
/// Also checks for the deprecated `PARISH_OLLAMA_URL` env var and maps
/// it to `base_url` with a warning.
pub fn resolve_config(
    config_path: Option<&Path>,
    cli: &CliOverrides,
) -> Result<ProviderConfig, ParishError> {
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

    let mut provider_str = toml_cfg.provider.name;
    let mut base_url = toml_cfg.provider.base_url;
    let mut api_key = toml_cfg.provider.api_key;
    let mut model = toml_cfg.provider.model;

    // Env vars override TOML (non-key fields)
    if let Some(val) = env_non_empty("PARISH_PROVIDER") {
        provider_str = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_BASE_URL") {
        base_url = Some(val);
    }
    if base_url.is_none()
        && let Some(val) = env_non_empty("PARISH_OLLAMA_URL")
    {
        tracing::warn!("PARISH_OLLAMA_URL is deprecated, use PARISH_BASE_URL instead");
        base_url = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_MODEL") {
        model = Some(val);
    }

    // CLI flags override env (non-key fields)
    if let Some(ref val) = cli.provider {
        provider_str = Some(val.clone());
    }
    if let Some(ref val) = cli.base_url {
        base_url = Some(val.clone());
    }
    if let Some(ref val) = cli.model {
        model = Some(val.clone());
    }

    // Resolve provider early — needed to look up the right key env var.
    let provider = match &provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::default(),
    };

    // Standard provider key env var (e.g. ANTHROPIC_API_KEY) overrides TOML api_key.
    // The key is always bound to the provider that owns it.
    if let Some(val) = provider.api_key_env_var().and_then(env_non_empty) {
        api_key = Some(val);
    }

    let base_url = base_url.unwrap_or_else(|| provider.default_base_url().to_string());
    let api_key = api_key.filter(|s| !s.is_empty());
    let model = model.filter(|s| !s.is_empty());

    // Fall back to the provider's Dialogue preset if no model is configured.
    let model = model.or_else(|| {
        provider
            .preset_model(InferenceCategory::Dialogue)
            .map(String::from)
    });

    if provider.requires_api_key() && api_key.is_none() {
        let hint = provider
            .api_key_env_var()
            .unwrap_or("the provider API key env var");
        return Err(ParishError::Config(format!(
            "{:?} provider requires an API key. Set {}.",
            provider, hint
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
/// Returns `None` if no explicit cloud settings are present (backward compatible).
/// Returns `Some(CloudConfig)` when at least one setting is configured.
///
/// Resolution order (later overrides earlier):
/// 1. TOML `[cloud]` section (including `api_key` as an escape hatch)
/// 2. `PARISH_CLOUD_PROVIDER`, `PARISH_CLOUD_BASE_URL`, `PARISH_CLOUD_MODEL` env vars
/// 3. CLI flags (provider, base_url, model — no api_key flag)
/// 4. Standard provider API key env var (e.g. `OPENROUTER_API_KEY`), read
///    after provider is known. Overrides TOML `api_key`.
///
/// Having a provider key env var set globally (e.g. `OPENROUTER_API_KEY`)
/// does NOT auto-activate cloud mode — an explicit cloud setting must exist.
pub fn resolve_cloud_config(
    config_path: Option<&Path>,
    cli: &CliCloudOverrides,
) -> Result<Option<CloudConfig>, ParishError> {
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

    let mut provider_str = toml_cfg.cloud.name;
    let mut base_url = toml_cfg.cloud.base_url;
    let api_key = toml_cfg.cloud.api_key;
    let mut model = toml_cfg.cloud.model;

    // Env vars override TOML (non-key fields)
    if let Some(val) = env_non_empty("PARISH_CLOUD_PROVIDER") {
        provider_str = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_CLOUD_BASE_URL") {
        base_url = Some(val);
    }
    if let Some(val) = env_non_empty("PARISH_CLOUD_MODEL") {
        model = Some(val);
    }

    // CLI flags override env (non-key fields)
    if let Some(ref val) = cli.provider {
        provider_str = Some(val.clone());
    }
    if let Some(ref val) = cli.base_url {
        base_url = Some(val.clone());
    }
    if let Some(ref val) = cli.model {
        model = Some(val.clone());
    }

    // If no explicit cloud config was provided, return None (backward compatible).
    // Provider key env vars are intentionally excluded from this check — having
    // OPENROUTER_API_KEY set globally should not auto-activate cloud mode.
    if provider_str.is_none()
        && base_url.is_none()
        && api_key.is_none()
        && model.is_none()
        && cli.provider.is_none()
        && cli.base_url.is_none()
        && cli.model.is_none()
    {
        return Ok(None);
    }

    let mut api_key = api_key.filter(|s| !s.is_empty());
    let model = model.filter(|s| !s.is_empty());

    // Resolve provider early (default to OpenRouter for cloud).
    let provider = match &provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::OpenRouter,
    };

    // Standard provider key env var overrides TOML api_key.
    if let Some(val) = provider.api_key_env_var().and_then(env_non_empty) {
        api_key = Some(val);
    }

    let base_url = base_url.unwrap_or_else(|| provider.default_base_url().to_string());

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
            "Cloud {:?} provider requires an API key. Set {}.",
            provider, hint
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
    use serial_test::serial;
    use std::io::Write;

    /// Clears env vars that affect provider config so tests don't interfere.
    ///
    /// Callers **must** annotate their test with `#[serial(parish_env)]` so
    /// env-mutating tests never run concurrently — Rust 2024 marks
    /// `std::env::remove_var` and `set_var` unsafe precisely because
    /// concurrent access is UB.
    fn clear_parish_env() {
        // SAFETY: All callers are annotated with `#[serial(parish_env)]`,
        // which serialises every test that touches env vars across this
        // module and the sibling `parish-cli` tests.
        unsafe {
            std::env::remove_var("PARISH_PROVIDER");
            std::env::remove_var("PARISH_BASE_URL");
            std::env::remove_var("PARISH_OLLAMA_URL");
            std::env::remove_var("PARISH_MODEL");
            std::env::remove_var("PARISH_CLOUD_PROVIDER");
            std::env::remove_var("PARISH_CLOUD_BASE_URL");
            std::env::remove_var("PARISH_CLOUD_MODEL");
            // Standard provider key vars — cleared so tests don't pick up
            // real keys from the developer's shell environment.
            std::env::remove_var("ANTHROPIC_API_KEY");
            std::env::remove_var("OPENAI_API_KEY");
            std::env::remove_var("OPENROUTER_API_KEY");
            std::env::remove_var("GOOGLE_API_KEY");
            std::env::remove_var("GROQ_API_KEY");
            std::env::remove_var("XAI_API_KEY");
            std::env::remove_var("MISTRAL_API_KEY");
            std::env::remove_var("DEEPSEEK_API_KEY");
            std::env::remove_var("TOGETHER_API_KEY");
            std::env::remove_var("NVIDIA_API_KEY");
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

        // Cloud providers
        assert_eq!(
            Provider::from_str_loose("openai").unwrap(),
            Provider::OpenAi
        );
        assert_eq!(
            Provider::from_str_loose("open-ai").unwrap(),
            Provider::OpenAi
        );
        assert_eq!(
            Provider::from_str_loose("open_ai").unwrap(),
            Provider::OpenAi
        );
        assert_eq!(
            Provider::from_str_loose("OpenAI").unwrap(),
            Provider::OpenAi
        );
        assert_eq!(
            Provider::from_str_loose("google").unwrap(),
            Provider::Google
        );
        assert_eq!(
            Provider::from_str_loose("gemini").unwrap(),
            Provider::Google
        );
        assert_eq!(Provider::from_str_loose("groq").unwrap(), Provider::Groq);
        assert_eq!(Provider::from_str_loose("xai").unwrap(), Provider::Xai);
        assert_eq!(Provider::from_str_loose("x-ai").unwrap(), Provider::Xai);
        assert_eq!(Provider::from_str_loose("grok").unwrap(), Provider::Xai);
        assert_eq!(
            Provider::from_str_loose("mistral").unwrap(),
            Provider::Mistral
        );
        assert_eq!(
            Provider::from_str_loose("deepseek").unwrap(),
            Provider::DeepSeek
        );
        assert_eq!(
            Provider::from_str_loose("deep-seek").unwrap(),
            Provider::DeepSeek
        );
        assert_eq!(
            Provider::from_str_loose("deep_seek").unwrap(),
            Provider::DeepSeek
        );
        assert_eq!(
            Provider::from_str_loose("together").unwrap(),
            Provider::Together
        );
        assert_eq!(
            Provider::from_str_loose("togetherai").unwrap(),
            Provider::Together
        );
        assert_eq!(
            Provider::from_str_loose("together-ai").unwrap(),
            Provider::Together
        );
        assert_eq!(
            Provider::from_str_loose("together_ai").unwrap(),
            Provider::Together
        );
        assert_eq!(
            Provider::from_str_loose("nvidia-nim").unwrap(),
            Provider::NvidiaNim
        );
        assert_eq!(
            Provider::from_str_loose("nvidia_nim").unwrap(),
            Provider::NvidiaNim
        );
        assert_eq!(
            Provider::from_str_loose("nvidianim").unwrap(),
            Provider::NvidiaNim
        );
        assert_eq!(
            Provider::from_str_loose("nim").unwrap(),
            Provider::NvidiaNim
        );
        assert_eq!(
            Provider::from_str_loose("NVIDIA").unwrap(),
            Provider::NvidiaNim
        );
        assert_eq!(
            Provider::from_str_loose("anthropic").unwrap(),
            Provider::Anthropic
        );
        assert_eq!(
            Provider::from_str_loose("claude").unwrap(),
            Provider::Anthropic
        );
        assert_eq!(
            Provider::from_str_loose("Anthropic").unwrap(),
            Provider::Anthropic
        );

        // WebGPU
        assert_eq!(
            Provider::from_str_loose("webgpu").unwrap(),
            Provider::WebGpu
        );
        assert_eq!(
            Provider::from_str_loose("WEBGPU").unwrap(),
            Provider::WebGpu
        );
        assert_eq!(
            Provider::from_str_loose("web-gpu").unwrap(),
            Provider::WebGpu
        );
        assert_eq!(
            Provider::from_str_loose("web_gpu").unwrap(),
            Provider::WebGpu
        );
        assert_eq!(
            Provider::from_str_loose("browser").unwrap(),
            Provider::WebGpu
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
        assert_eq!(
            Provider::OpenAi.default_base_url(),
            "https://api.openai.com"
        );
        assert_eq!(
            Provider::Google.default_base_url(),
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
        assert_eq!(
            Provider::Groq.default_base_url(),
            "https://api.groq.com/openai"
        );
        assert_eq!(Provider::Xai.default_base_url(), "https://api.x.ai");
        assert_eq!(
            Provider::Mistral.default_base_url(),
            "https://api.mistral.ai"
        );
        assert_eq!(
            Provider::DeepSeek.default_base_url(),
            "https://api.deepseek.com"
        );
        assert_eq!(
            Provider::Together.default_base_url(),
            "https://api.together.xyz"
        );
        assert_eq!(
            Provider::NvidiaNim.default_base_url(),
            "https://integrate.api.nvidia.com"
        );
        assert_eq!(
            Provider::Anthropic.default_base_url(),
            "https://api.anthropic.com"
        );
        assert_eq!(Provider::Custom.default_base_url(), "");
        assert_eq!(Provider::WebGpu.default_base_url(), "");
    }

    #[test]
    fn test_provider_requirements() {
        // Local providers don't require API keys
        assert!(!Provider::Ollama.requires_api_key());
        assert!(!Provider::LmStudio.requires_api_key());
        assert!(!Provider::Vllm.requires_api_key());
        assert!(!Provider::Custom.requires_api_key());
        assert!(!Provider::WebGpu.requires_api_key());

        // All cloud providers require API keys
        assert!(Provider::OpenRouter.requires_api_key());
        assert!(Provider::OpenAi.requires_api_key());
        assert!(Provider::Google.requires_api_key());
        assert!(Provider::Groq.requires_api_key());
        assert!(Provider::Xai.requires_api_key());
        assert!(Provider::Mistral.requires_api_key());
        assert!(Provider::DeepSeek.requires_api_key());
        assert!(Provider::Together.requires_api_key());
        assert!(Provider::NvidiaNim.requires_api_key());
        assert!(Provider::Anthropic.requires_api_key());

        // Ollama, simulator, and WebGPU auto-detect a model
        assert!(!Provider::Ollama.requires_model());
        assert!(!Provider::Simulator.requires_model());
        assert!(!Provider::WebGpu.requires_model());
        assert!(Provider::LmStudio.requires_model());
        assert!(Provider::OpenRouter.requires_model());
        assert!(Provider::Vllm.requires_model());
        assert!(Provider::OpenAi.requires_model());
        assert!(Provider::Google.requires_model());
        assert!(Provider::Groq.requires_model());
        assert!(Provider::Xai.requires_model());
        assert!(Provider::Mistral.requires_model());
        assert!(Provider::DeepSeek.requires_model());
        assert!(Provider::Together.requires_model());
        assert!(Provider::NvidiaNim.requires_model());
        assert!(Provider::Anthropic.requires_model());
        assert!(Provider::Custom.requires_model());
    }

    #[test]
    fn test_vllm_provider_from_str() {
        assert_eq!(Provider::from_str_loose("vllm").unwrap(), Provider::Vllm);
        assert_eq!(Provider::from_str_loose("VLLM").unwrap(), Provider::Vllm);
    }

    #[test]
    fn test_vllm_provider_defaults() {
        assert_eq!(Provider::Vllm.default_base_url(), "http://localhost:8000");
        assert!(!Provider::Vllm.requires_api_key());
        assert!(Provider::Vllm.requires_model());
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_vllm() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("vllm".to_string()),
            base_url: None,
            model: Some("Qwen/Qwen3-8B".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::Vllm);
        assert_eq!(config.base_url, "http://localhost:8000");
        assert!(config.api_key.is_none());
        assert_eq!(config.model.as_deref(), Some("Qwen/Qwen3-8B"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_vllm_custom_base_url() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("vllm".to_string()),
            base_url: Some("http://gpu-server:8000".to_string()),
            model: Some("meta-llama/Llama-3-8B".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::Vllm);
        assert_eq!(config.base_url, "http://gpu-server:8000");
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_defaults() {
        clear_parish_env();

        let cli = CliOverrides::default();
        let config = resolve_config(Some(Path::new("/nonexistent/parish.toml")), &cli).unwrap();
        assert_eq!(config.provider, Provider::Simulator);
        assert_eq!(config.base_url, "");
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    #[serial(parish_env)]
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
    #[serial(parish_env)]
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
            model: Some("cli-model".to_string()),
        };
        let config = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config.provider, Provider::LmStudio);
        assert_eq!(config.model.as_deref(), Some("cli-model"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_openrouter_requires_api_key() {
        clear_parish_env();
        // OPENROUTER_API_KEY is cleared by clear_parish_env — no key available.

        let cli = CliOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            model: Some("anthropic/claude-sonnet-4-20250514".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
        assert!(err_msg.contains("OPENROUTER_API_KEY"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_openrouter_with_api_key() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-test-key") };

        let cli = CliOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            model: Some("anthropic/claude-sonnet-4-20250514".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.base_url, "https://openrouter.ai/api");
        assert_eq!(config.api_key.as_deref(), Some("sk-test-key"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_nvidia_nim_requires_api_key() {
        clear_parish_env();
        // NVIDIA_API_KEY cleared by clear_parish_env — should fail.

        let cli = CliOverrides {
            provider: Some("nvidia-nim".to_string()),
            base_url: None,
            model: Some("nvidia/nemotron-3-nano-30b-a3b".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
        assert!(err_msg.contains("NVIDIA_API_KEY"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_nvidia_nim_uses_dialogue_preset_when_model_unset() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("NVIDIA_API_KEY", "nvapi-test") };

        // Provider + key but no model — the resolver should fall through to
        // the NvidiaNim Dialogue preset declared in `presets.rs`.
        let cli = CliOverrides {
            provider: Some("nvidia-nim".to_string()),
            base_url: None,
            model: None,
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::NvidiaNim);
        assert_eq!(config.base_url, "https://integrate.api.nvidia.com");
        assert_eq!(
            config.model.as_deref(),
            Some("nvidia/nemotron-3-super-120b-a12b")
        );
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_builtin_cloud_providers() {
        clear_parish_env();

        // Each built-in cloud provider resolves with its default URL.
        // Key comes from the provider-specific env var, not a generic PARISH_API_KEY.
        let providers = [
            ("openai", "https://api.openai.com", Provider::OpenAi),
            (
                "google",
                "https://generativelanguage.googleapis.com/v1beta/openai",
                Provider::Google,
            ),
            ("groq", "https://api.groq.com/openai", Provider::Groq),
            ("xai", "https://api.x.ai", Provider::Xai),
            ("mistral", "https://api.mistral.ai", Provider::Mistral),
            ("deepseek", "https://api.deepseek.com", Provider::DeepSeek),
            ("together", "https://api.together.xyz", Provider::Together),
            (
                "nvidia-nim",
                "https://integrate.api.nvidia.com",
                Provider::NvidiaNim,
            ),
            (
                "anthropic",
                "https://api.anthropic.com",
                Provider::Anthropic,
            ),
        ];

        for (name, expected_url, expected_provider) in providers {
            // SAFETY: serialised by #[serial(parish_env)]
            let key_var = expected_provider.api_key_env_var().unwrap();
            unsafe { std::env::set_var(key_var, "sk-test") };

            let cli = CliOverrides {
                provider: Some(name.to_string()),
                base_url: None,
                model: Some("test-model".to_string()),
            };
            let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
            assert_eq!(
                config.provider, expected_provider,
                "provider mismatch for {name}"
            );
            assert_eq!(config.base_url, expected_url, "URL mismatch for {name}");
            assert_eq!(config.api_key.as_deref(), Some("sk-test"));

            unsafe { std::env::remove_var(key_var) };
        }
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_cloud_provider_requires_api_key() {
        clear_parish_env();
        // All provider key vars are cleared — every cloud provider should fail.

        for name in [
            "openai",
            "google",
            "groq",
            "xai",
            "mistral",
            "deepseek",
            "together",
            "anthropic",
        ] {
            let cli = CliOverrides {
                provider: Some(name.to_string()),
                base_url: None,
                model: Some("test-model".to_string()),
            };
            let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
            assert!(result.is_err(), "{name} should require an API key");
            let err_msg = result.unwrap_err().to_string();
            assert!(
                err_msg.contains("API key"),
                "{name} error should mention API key, got: {err_msg}"
            );
        }
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_switching_provider_does_not_carry_key() {
        clear_parish_env();
        // ANTHROPIC_API_KEY is set but OPENAI_API_KEY is not.
        // Switching to OpenAI must fail — the Anthropic key must not leak.
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-secret") };

        let cli = CliOverrides {
            provider: Some("openai".to_string()),
            base_url: None,
            model: Some("gpt-4".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err(), "OpenAI should fail without OPENAI_API_KEY");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("OPENAI_API_KEY"), "got: {err}");
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_custom_requires_base_url() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("custom".to_string()),
            base_url: None,
            model: Some("some-model".to_string()),
        };
        let result = resolve_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("base_url"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_falls_back_to_preset_model_when_unset() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-test") };

        let cli = CliOverrides {
            provider: Some("anthropic".to_string()),
            base_url: None,
            model: None,
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider, Provider::Anthropic);
        assert_eq!(config.model.as_deref(), Some("claude-opus-4-7"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_does_not_clobber_explicit_model() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-test") };

        let cli = CliOverrides {
            provider: Some("anthropic".to_string()),
            base_url: None,
            model: Some("claude-3-opus-20240229".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.model.as_deref(), Some("claude-3-opus-20240229"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_empty_strings_filtered() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: None,
            base_url: None,
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
    #[serial(parish_env)]
    fn test_resolve_cloud_config_none_when_not_configured() {
        clear_parish_env();
        let cli = CliCloudOverrides::default();
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert!(result.is_none());
    }

    #[test]
    #[serial(parish_env)]
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
    #[serial(parish_env)]
    fn test_resolve_cloud_config_from_cli() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-cli") };

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
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
    #[serial(parish_env)]
    fn test_resolve_cloud_config_requires_model() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-test") };

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            model: None,
        };
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("model"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_cloud_config_openrouter_requires_api_key() {
        clear_parish_env();
        // OPENROUTER_API_KEY cleared by clear_parish_env — no key available.

        let cli = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            model: Some("claude-3".to_string()),
        };
        let result = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
        assert!(err_msg.contains("OPENROUTER_API_KEY"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_cloud_config_defaults_to_openrouter() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-test") };

        let cli = CliCloudOverrides {
            provider: None,
            base_url: None,
            model: Some("my-model".to_string()),
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider, Provider::OpenRouter);
        assert_eq!(config.base_url, "https://openrouter.ai/api");
    }

    #[test]
    #[serial(parish_env)]
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
        // TOML api_key is explicit escape hatch; OPENROUTER_API_KEY cleared so TOML wins.

        let cli = CliCloudOverrides {
            provider: None,
            base_url: None,
            model: Some("cli-model".to_string()),
        };
        let config = resolve_cloud_config(Some(&path), &cli).unwrap().unwrap();
        assert_eq!(config.model, "cli-model");
        assert_eq!(config.api_key.as_deref(), Some("sk-toml"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_cloud_config_ollama_no_key_needed() {
        clear_parish_env();

        let cli = CliCloudOverrides {
            provider: Some("ollama".to_string()),
            base_url: Some("http://remote-ollama:11434".to_string()),
            model: Some("llama3".to_string()),
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider, Provider::Ollama);
        assert_eq!(config.base_url, "http://remote-ollama:11434");
        assert_eq!(config.model, "llama3");
    }
}
