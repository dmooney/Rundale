//! Provider configuration for LLM inference backends.
//!
//! Providers are loaded at compile time from TOML files in
//! `parish/crates/parish-config/providers/*.toml` via `build.rs`.
//! The `ProviderRegistry` is initialised once via `OnceLock` and
//! accessible through the `registry()` function.

use parish_types::ParishError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

// ── ProviderKind ─────────────────────────────────────────────────────────────

/// Client-routing category. Controls which HTTP client (Anthropic
/// Messages vs OpenAI-compat vs Simulator) is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderKind {
    Anthropic,
    #[serde(rename = "openai-compat")]
    OpenAiCompat,
    /// Ollama, LM Studio, vLLM — OpenAI-compat on the wire but managed locally.
    Local,
    Simulator,
}

// ── ProviderPreset ────────────────────────────────────────────────────────────

/// Per-category base-URL overrides shipped with a provider preset.
///
/// Two-slot loadouts (e.g. vllm-mlx on Apple Silicon: 14B on :8000 + 1.5B
/// on :8001) need each category routed to the slot where its preset model
/// is actually loaded. Without this, `fill_missing_models_from_presets`
/// picks the preset model but inherits the base URL — guaranteeing a
/// model/URL mismatch and a 404 on every reaction/simulation call.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PresetBaseUrls {
    pub dialogue: Option<String>,
    pub simulation: Option<String>,
    pub intent: Option<String>,
    pub reaction: Option<String>,
}

impl PresetBaseUrls {
    pub fn url(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.dialogue.as_deref(),
            InferenceCategory::Simulation => self.simulation.as_deref(),
            InferenceCategory::Intent => self.intent.as_deref(),
            InferenceCategory::Reaction => self.reaction.as_deref(),
        }
    }
}

/// One named model configuration shipped with a provider.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderPreset {
    pub key: String,
    pub label: String,
    pub dialogue: Option<String>,
    pub simulation: Option<String>,
    pub intent: Option<String>,
    pub reaction: Option<String>,
    /// Per-category base URL hints. When unset for a category the user's
    /// base URL is inherited (single-slot providers). See [`PresetBaseUrls`].
    #[serde(default)]
    pub base_urls: PresetBaseUrls,
}

impl ProviderPreset {
    pub fn model(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.dialogue.as_deref(),
            InferenceCategory::Simulation => self.simulation.as_deref(),
            InferenceCategory::Intent => self.intent.as_deref(),
            InferenceCategory::Reaction => self.reaction.as_deref(),
        }
    }

    pub fn base_url(&self, cat: InferenceCategory) -> Option<&str> {
        self.base_urls.url(cat)
    }
}

// ── ProviderMod ───────────────────────────────────────────────────────────────

/// All data about one provider, deserialized from TOML.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderMod {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub kind: ProviderKind,
    pub default_base_url: String,
    #[serde(default)]
    pub requires_api_key: bool,
    #[serde(default)]
    pub needs_base_url_from_user: bool,
    #[serde(default = "default_true")]
    pub requires_model: bool,
    pub api_key_env_var: Option<String>,
    pub blurb: Option<String>,
    pub signup_url: Option<String>,
    #[serde(default)]
    pub featured: bool,
    #[serde(default)]
    pub presets: Vec<ProviderPreset>,
}

fn default_true() -> bool {
    true
}

impl ProviderMod {
    pub fn has_preset(&self) -> bool {
        !self.presets.is_empty()
    }

    pub fn preset_model(&self, cat: InferenceCategory) -> Option<&str> {
        self.presets.first()?.model(cat)
    }

    /// Per-category base URL from the recommended preset, when supplied.
    /// Returns `None` if the preset omits the field (single-slot providers).
    pub fn preset_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        self.presets.first()?.base_url(cat)
    }

    pub fn preset_models_array(&self) -> [Option<&str>; 4] {
        let first = self.presets.first();
        [
            first.and_then(|p| p.dialogue.as_deref()),
            first.and_then(|p| p.simulation.as_deref()),
            first.and_then(|p| p.intent.as_deref()),
            first.and_then(|p| p.reaction.as_deref()),
        ]
    }
}

// ── Provider ──────────────────────────────────────────────────────────────────

/// Returns the host's total physical memory in bytes, or `None` on
/// platforms where we can't read it cheaply.
pub fn unified_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let s = std::str::from_utf8(&output.stdout).ok()?;
        s.trim().parse::<u64>().ok()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// A loaded provider configuration. Wraps `Arc<ProviderMod>` for
/// cheap cloning and shared ownership.
#[derive(Debug, Clone)]
pub struct Provider(pub Arc<ProviderMod>);

impl Provider {
    pub fn id(&self) -> &str {
        &self.0.id
    }
    pub fn kind(&self) -> ProviderKind {
        self.0.kind
    }
    pub fn default_base_url(&self) -> &str {
        &self.0.default_base_url
    }
    pub fn requires_api_key(&self) -> bool {
        self.0.requires_api_key
    }
    pub fn needs_base_url_from_user(&self) -> bool {
        self.0.needs_base_url_from_user
    }
    pub fn requires_model(&self) -> bool {
        self.0.requires_model
    }
    pub fn api_key_env_var(&self) -> Option<&str> {
        self.0.api_key_env_var.as_deref()
    }
    pub fn presets(&self) -> &[ProviderPreset] {
        &self.0.presets
    }
    pub fn has_preset(&self) -> bool {
        self.0.has_preset()
    }
    pub fn preset_model(&self, cat: InferenceCategory) -> Option<&str> {
        self.0.preset_model(cat)
    }
    pub fn preset_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        self.0.preset_base_url(cat)
    }
    /// Returns `[dialogue, simulation, intent, reaction]` from the first preset.
    pub fn preset_models(&self) -> [Option<&str>; 4] {
        self.0.preset_models_array()
    }
    pub fn display_name(&self) -> &str {
        &self.0.display_name
    }

    pub fn is_configured_in_env(&self) -> bool {
        if !self.requires_api_key() {
            return true;
        }
        self.api_key_env_var()
            .and_then(|v| std::env::var(v).ok())
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    }

    pub fn recommended_for_platform() -> Self {
        if cfg!(target_os = "macos") {
            if unified_memory_bytes().unwrap_or(0) >= 16 * 1_073_741_824 {
                registry()
                    .get("vllmmlx")
                    .expect("vllmmlx must be registered")
            } else {
                registry()
                    .get("simulator")
                    .expect("simulator must be registered")
            }
        } else {
            registry().get("vllm").expect("vllm must be registered")
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        registry().get(id)
    }

    pub fn from_str_loose(s: &str) -> Result<Self, ParishError> {
        registry().lookup(s)
    }

    // Named constructors used throughout the codebase.
    pub fn ollama() -> Self {
        registry().get("ollama").expect("ollama must be registered")
    }
    pub fn simulator() -> Self {
        registry()
            .get("simulator")
            .expect("simulator must be registered")
    }
    pub fn openrouter() -> Self {
        registry()
            .get("openrouter")
            .expect("openrouter must be registered")
    }
    pub fn custom() -> Self {
        registry().get("custom").expect("custom must be registered")
    }
    pub fn anthropic() -> Self {
        registry()
            .get("anthropic")
            .expect("anthropic must be registered")
    }
    pub fn openai() -> Self {
        registry().get("openai").expect("openai must be registered")
    }
    pub fn google() -> Self {
        registry().get("google").expect("google must be registered")
    }
    pub fn groq() -> Self {
        registry().get("groq").expect("groq must be registered")
    }
    pub fn xai() -> Self {
        registry().get("xai").expect("xai must be registered")
    }
    pub fn mistral() -> Self {
        registry()
            .get("mistral")
            .expect("mistral must be registered")
    }
    pub fn deepseek() -> Self {
        registry()
            .get("deepseek")
            .expect("deepseek must be registered")
    }
    pub fn together() -> Self {
        registry()
            .get("together")
            .expect("together must be registered")
    }
    pub fn vllmmlx() -> Self {
        registry()
            .get("vllmmlx")
            .expect("vllmmlx must be registered")
    }
    pub fn lmstudio() -> Self {
        registry()
            .get("lmstudio")
            .expect("lmstudio must be registered")
    }
    pub fn github_models() -> Self {
        registry()
            .get("github_models")
            .expect("github_models must be registered")
    }
}

impl PartialEq for Provider {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for Provider {}
impl std::hash::Hash for Provider {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.id.hash(state);
    }
}
impl Default for Provider {
    fn default() -> Self {
        Self::simulator()
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.id)
    }
}

// ── ProviderRegistry ─────────────────────────────────────────────────────────

pub struct ProviderRegistry {
    by_id: HashMap<String, Provider>,
}

impl ProviderRegistry {
    fn load() -> Self {
        let mut by_id = HashMap::new();
        for raw in RAW_PROVIDER_MODS {
            let m: ProviderMod =
                toml::from_str(raw).expect("provider TOML parse failed — check providers/*.toml");
            let p = Provider(Arc::new(m));
            by_id.insert(p.id().to_string(), p.clone());
        }
        ProviderRegistry { by_id }
    }

    pub fn get(&self, id: &str) -> Option<Provider> {
        self.by_id.get(id).cloned()
    }

    pub fn lookup(&self, s: &str) -> Result<Provider, ParishError> {
        let lower = s.to_lowercase();
        // Try id directly
        if let Some(p) = self.by_id.get(&lower) {
            return Ok(p.clone());
        }
        // Try aliases
        for p in self.by_id.values() {
            if p.0.aliases.iter().any(|a| a == &lower) {
                return Ok(p.clone());
            }
        }
        let mut known: Vec<&str> = self.by_id.keys().map(String::as_str).collect();
        known.sort();
        Err(ParishError::Config(format!(
            "unknown provider '{}'. Known: {}",
            s,
            known.join(", ")
        )))
    }

    pub fn all(&self) -> Vec<Provider> {
        let mut v: Vec<_> = self.by_id.values().cloned().collect();
        v.sort_by(|a, b| a.id().cmp(b.id()));
        v
    }

    pub fn featured(&self) -> Vec<Provider> {
        let mut v: Vec<_> = self
            .by_id
            .values()
            .filter(|p| p.0.featured)
            .cloned()
            .collect();
        v.sort_by(|a, b| a.id().cmp(b.id()));
        v
    }
}

// Include the generated array of raw TOML strings at module level.
include!(concat!(env!("OUT_DIR"), "/providers_gen.rs"));

static REGISTRY: OnceLock<ProviderRegistry> = OnceLock::new();

pub fn registry() -> &'static ProviderRegistry {
    REGISTRY.get_or_init(ProviderRegistry::load)
}

// ── InferenceCategory ────────────────────────────────────────────────────────

/// Inference categories that can each have independent provider/model/key settings.
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

    /// Array index matching [`InferenceCategory::ALL`] order.
    pub fn idx(self) -> usize {
        match self {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
            InferenceCategory::Reaction => 3,
        }
    }

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

// ── ProviderConfig ────────────────────────────────────────────────────────────

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
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
}

/// The `[cloud]` section of the TOML config for cloud dialogue provider.
#[derive(Debug, Deserialize, Default)]
struct TomlCloud {
    name: Option<String>,
    base_url: Option<String>,
    api_key: Option<String>,
    model: Option<String>,
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

// ── Shared 4-layer resolution helper ─────────────────────────────────────────

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

    // Default to OpenRouter for cloud
    let provider = match &raw.provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => Provider::openrouter(),
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

fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

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

    fn clear_parish_env() {
        // SAFETY: All callers are annotated with `#[serial(parish_env)]`
        unsafe {
            std::env::remove_var("PARISH_PROVIDER");
            std::env::remove_var("PARISH_BASE_URL");
            std::env::remove_var("PARISH_OLLAMA_URL");
            std::env::remove_var("PARISH_MODEL");
            std::env::remove_var("PARISH_CLOUD_PROVIDER");
            std::env::remove_var("PARISH_CLOUD_BASE_URL");
            std::env::remove_var("PARISH_CLOUD_MODEL");
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

    struct CwdGuard {
        original: std::path::PathBuf,
    }

    impl CwdGuard {
        fn enter(path: &Path) -> Self {
            let original = std::env::current_dir().unwrap();
            std::env::set_current_dir(path).unwrap();
            Self { original }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            std::env::set_current_dir(&self.original).unwrap();
        }
    }

    #[test]
    fn test_provider_from_str_loose() {
        assert_eq!(Provider::from_str_loose("ollama").unwrap().id(), "ollama");
        assert_eq!(Provider::from_str_loose("OLLAMA").unwrap().id(), "ollama");
        assert_eq!(
            Provider::from_str_loose("lmstudio").unwrap().id(),
            "lmstudio"
        );
        assert_eq!(
            Provider::from_str_loose("lm-studio").unwrap().id(),
            "lmstudio"
        );
        assert_eq!(
            Provider::from_str_loose("lm_studio").unwrap().id(),
            "lmstudio"
        );
        assert_eq!(
            Provider::from_str_loose("openrouter").unwrap().id(),
            "openrouter"
        );
        assert_eq!(
            Provider::from_str_loose("open-router").unwrap().id(),
            "openrouter"
        );
        assert_eq!(Provider::from_str_loose("custom").unwrap().id(), "custom");

        // Cloud providers
        assert_eq!(Provider::from_str_loose("openai").unwrap().id(), "openai");
        assert_eq!(Provider::from_str_loose("open-ai").unwrap().id(), "openai");
        assert_eq!(Provider::from_str_loose("open_ai").unwrap().id(), "openai");
        assert_eq!(Provider::from_str_loose("OpenAI").unwrap().id(), "openai");
        assert_eq!(Provider::from_str_loose("google").unwrap().id(), "google");
        assert_eq!(Provider::from_str_loose("gemini").unwrap().id(), "google");
        assert_eq!(Provider::from_str_loose("groq").unwrap().id(), "groq");
        assert_eq!(Provider::from_str_loose("xai").unwrap().id(), "xai");
        assert_eq!(Provider::from_str_loose("x-ai").unwrap().id(), "xai");
        assert_eq!(Provider::from_str_loose("grok").unwrap().id(), "xai");
        assert_eq!(Provider::from_str_loose("mistral").unwrap().id(), "mistral");
        assert_eq!(
            Provider::from_str_loose("deepseek").unwrap().id(),
            "deepseek"
        );
        assert_eq!(
            Provider::from_str_loose("deep-seek").unwrap().id(),
            "deepseek"
        );
        assert_eq!(
            Provider::from_str_loose("deep_seek").unwrap().id(),
            "deepseek"
        );
        assert_eq!(
            Provider::from_str_loose("together").unwrap().id(),
            "together"
        );
        assert_eq!(
            Provider::from_str_loose("togetherai").unwrap().id(),
            "together"
        );
        assert_eq!(
            Provider::from_str_loose("together-ai").unwrap().id(),
            "together"
        );
        assert_eq!(
            Provider::from_str_loose("together_ai").unwrap().id(),
            "together"
        );
        assert_eq!(
            Provider::from_str_loose("nvidia-nim").unwrap().id(),
            "nvidia-nim"
        );
        assert_eq!(
            Provider::from_str_loose("nvidia_nim").unwrap().id(),
            "nvidia-nim"
        );
        assert_eq!(
            Provider::from_str_loose("nvidianim").unwrap().id(),
            "nvidia-nim"
        );
        assert_eq!(Provider::from_str_loose("nim").unwrap().id(), "nvidia-nim");
        assert_eq!(
            Provider::from_str_loose("NVIDIA").unwrap().id(),
            "nvidia-nim"
        );
        assert_eq!(
            Provider::from_str_loose("anthropic").unwrap().id(),
            "anthropic"
        );
        assert_eq!(
            Provider::from_str_loose("claude").unwrap().id(),
            "anthropic"
        );
        assert_eq!(
            Provider::from_str_loose("Anthropic").unwrap().id(),
            "anthropic"
        );

        assert!(Provider::from_str_loose("unknown").is_err());
    }

    #[test]
    fn test_provider_default_base_url() {
        assert_eq!(
            Provider::ollama().default_base_url(),
            "http://localhost:11434"
        );
        assert_eq!(
            Provider::from_str_loose("lmstudio")
                .unwrap()
                .default_base_url(),
            "http://localhost:1234"
        );
        assert_eq!(
            Provider::openrouter().default_base_url(),
            "https://openrouter.ai/api"
        );
        assert_eq!(
            Provider::from_str_loose("openai")
                .unwrap()
                .default_base_url(),
            "https://api.openai.com"
        );
        assert_eq!(
            Provider::from_str_loose("google")
                .unwrap()
                .default_base_url(),
            "https://generativelanguage.googleapis.com/v1beta/openai"
        );
        assert_eq!(
            Provider::from_str_loose("groq").unwrap().default_base_url(),
            "https://api.groq.com/openai"
        );
        assert_eq!(
            Provider::from_str_loose("xai").unwrap().default_base_url(),
            "https://api.x.ai"
        );
        assert_eq!(
            Provider::from_str_loose("mistral")
                .unwrap()
                .default_base_url(),
            "https://api.mistral.ai"
        );
        assert_eq!(
            Provider::from_str_loose("deepseek")
                .unwrap()
                .default_base_url(),
            "https://api.deepseek.com"
        );
        assert_eq!(
            Provider::from_str_loose("together")
                .unwrap()
                .default_base_url(),
            "https://api.together.xyz"
        );
        assert_eq!(
            Provider::from_str_loose("nvidia-nim")
                .unwrap()
                .default_base_url(),
            "https://integrate.api.nvidia.com"
        );
        assert_eq!(
            Provider::anthropic().default_base_url(),
            "https://api.anthropic.com"
        );
        assert_eq!(Provider::custom().default_base_url(), "");
    }

    #[test]
    fn test_provider_requirements() {
        // Local providers don't require API keys
        assert!(!Provider::ollama().requires_api_key());
        assert!(
            !Provider::from_str_loose("lmstudio")
                .unwrap()
                .requires_api_key()
        );
        assert!(
            !Provider::from_str_loose("vllmmlx")
                .unwrap()
                .requires_api_key()
        );
        assert!(!Provider::custom().requires_api_key());

        // All cloud providers require API keys
        assert!(Provider::openrouter().requires_api_key());
        assert!(
            Provider::from_str_loose("openai")
                .unwrap()
                .requires_api_key()
        );
        assert!(
            Provider::from_str_loose("google")
                .unwrap()
                .requires_api_key()
        );
        assert!(Provider::from_str_loose("groq").unwrap().requires_api_key());
        assert!(Provider::from_str_loose("xai").unwrap().requires_api_key());
        assert!(
            Provider::from_str_loose("mistral")
                .unwrap()
                .requires_api_key()
        );
        assert!(
            Provider::from_str_loose("deepseek")
                .unwrap()
                .requires_api_key()
        );
        assert!(
            Provider::from_str_loose("together")
                .unwrap()
                .requires_api_key()
        );
        assert!(
            Provider::from_str_loose("nvidia-nim")
                .unwrap()
                .requires_api_key()
        );
        assert!(Provider::anthropic().requires_api_key());

        // Only Ollama and Simulator auto-detect model
        assert!(!Provider::ollama().requires_model());
        assert!(!Provider::simulator().requires_model());
        assert!(
            Provider::from_str_loose("lmstudio")
                .unwrap()
                .requires_model()
        );
        assert!(Provider::openrouter().requires_model());
        assert!(
            Provider::from_str_loose("vllmmlx")
                .unwrap()
                .requires_model()
        );
        assert!(Provider::from_str_loose("openai").unwrap().requires_model());
        assert!(Provider::from_str_loose("google").unwrap().requires_model());
        assert!(Provider::from_str_loose("groq").unwrap().requires_model());
        assert!(Provider::from_str_loose("xai").unwrap().requires_model());
        assert!(
            Provider::from_str_loose("mistral")
                .unwrap()
                .requires_model()
        );
        assert!(
            Provider::from_str_loose("deepseek")
                .unwrap()
                .requires_model()
        );
        assert!(
            Provider::from_str_loose("together")
                .unwrap()
                .requires_model()
        );
        assert!(
            Provider::from_str_loose("nvidia-nim")
                .unwrap()
                .requires_model()
        );
        assert!(Provider::anthropic().requires_model());
        assert!(Provider::custom().requires_model());
    }

    #[test]
    fn test_vllm_provider_from_str() {
        // "vllm" string resolves to the Linux/Windows vllm provider, not vllm-mlx.
        assert_eq!(Provider::from_str_loose("vllm").unwrap().id(), "vllm");
        assert_eq!(Provider::from_str_loose("VLLM").unwrap().id(), "vllm");
    }

    #[test]
    fn test_vllm_provider_defaults() {
        let p = Provider::from_str_loose("vllmmlx").unwrap();
        assert_eq!(p.default_base_url(), "http://localhost:8000");
        assert!(!p.requires_api_key());
        assert!(p.requires_model());

        let v = Provider::from_str_loose("vllm").unwrap();
        assert_eq!(v.default_base_url(), "http://localhost:8000");
        assert!(!v.requires_api_key());
        assert!(v.requires_model());
    }

    #[test]
    fn recommended_for_platform_picks_vllm_mlx_on_macos_else_vllm() {
        let rec = Provider::recommended_for_platform();
        if cfg!(target_os = "macos") {
            assert!(rec.id() == "vllmmlx" || rec.id() == "simulator");
        } else {
            assert_eq!(rec.id(), "vllm");
        }
    }

    #[test]
    fn vllm_mlx_aliases_resolve() {
        // "vllm" alone now resolves to the Linux/Windows variant.
        for alias in ["vllm-mlx", "vllm_mlx", "vllmmlx", "VLLM-MLX"] {
            assert_eq!(
                Provider::from_str_loose(alias).unwrap().id(),
                "vllmmlx",
                "alias {alias} must resolve to vllmmlx"
            );
        }
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_vllm() {
        clear_parish_env();

        let cli = CliOverrides {
            provider: Some("vllm".to_string()),
            base_url: None,
            model: Some("Qwen/Qwen2.5-14B-Instruct".to_string()),
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider.id(), "vllm");
        assert_eq!(config.base_url, "http://localhost:8000");
        assert!(config.api_key.is_none());
        assert_eq!(config.model.as_deref(), Some("Qwen/Qwen2.5-14B-Instruct"));
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
        assert_eq!(config.provider.id(), "vllm");
        assert_eq!(config.base_url, "http://gpu-server:8000");
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_defaults() {
        clear_parish_env();

        let cli = CliOverrides::default();
        let config = resolve_config(Some(Path::new("/nonexistent/parish.toml")), &cli).unwrap();
        assert_eq!(config.provider.id(), "simulator");
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
        assert_eq!(config.provider.id(), "lmstudio");
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
        assert_eq!(config.provider.id(), "lmstudio");
        assert_eq!(config.model.as_deref(), Some("cli-model"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_env_overrides_toml_and_cli_overrides_env() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "lmstudio"
base_url = "http://toml-host:5555"
model = "toml-model"
"#
        )
        .unwrap();

        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe {
            std::env::set_var("PARISH_PROVIDER", "ollama");
            std::env::set_var("PARISH_BASE_URL", "http://env-host:11434");
            std::env::set_var("PARISH_MODEL", "env-model");
        }

        let cli = CliOverrides {
            provider: Some("vllm".to_string()),
            base_url: None,
            model: Some("cli-model".to_string()),
        };
        let config = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config.provider.id(), "vllm");
        assert_eq!(config.base_url, "http://env-host:11434");
        assert_eq!(config.model.as_deref(), Some("cli-model"));

        clear_parish_env();
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_provider_key_env_overrides_toml_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "anthropic"
api_key = "sk-toml"
model = "claude-test"
"#
        )
        .unwrap();

        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-env") };

        let cli = CliOverrides::default();
        let config = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config.provider.id(), "anthropic");
        assert_eq!(config.api_key.as_deref(), Some("sk-env"));
        assert_eq!(config.model.as_deref(), Some("claude-test"));

        clear_parish_env();
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_none_does_not_read_cwd_parish_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        std::fs::write(
            &path,
            r#"
[provider]
name = "lmstudio"
base_url = "http://cwd-host:1234"
model = "cwd-model"
"#,
        )
        .unwrap();

        clear_parish_env();
        let _cwd = CwdGuard::enter(dir.path());

        let cli = CliOverrides::default();
        let config = resolve_config(None, &cli).unwrap();
        assert_eq!(config.provider.id(), "simulator");
        assert_eq!(config.base_url, "");
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_openrouter_requires_api_key() {
        clear_parish_env();

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
        assert_eq!(config.provider.id(), "openrouter");
        assert_eq!(config.base_url, "https://openrouter.ai/api");
        assert_eq!(config.api_key.as_deref(), Some("sk-test-key"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_nvidia_nim_requires_api_key() {
        clear_parish_env();

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

        let cli = CliOverrides {
            provider: Some("nvidia-nim".to_string()),
            base_url: None,
            model: None,
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider.id(), "nvidia-nim");
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

        let providers = [
            ("openai", "https://api.openai.com", "openai"),
            (
                "google",
                "https://generativelanguage.googleapis.com/v1beta/openai",
                "google",
            ),
            ("groq", "https://api.groq.com/openai", "groq"),
            ("xai", "https://api.x.ai", "xai"),
            ("mistral", "https://api.mistral.ai", "mistral"),
            ("deepseek", "https://api.deepseek.com", "deepseek"),
            ("together", "https://api.together.xyz", "together"),
            (
                "nvidia-nim",
                "https://integrate.api.nvidia.com",
                "nvidia-nim",
            ),
            ("anthropic", "https://api.anthropic.com", "anthropic"),
        ];

        for (name, expected_url, expected_id) in providers {
            let provider = Provider::from_str_loose(name).unwrap();
            // SAFETY: serialised by #[serial(parish_env)]
            let key_var = provider.api_key_env_var().unwrap();
            unsafe { std::env::set_var(key_var, "sk-test") };

            let cli = CliOverrides {
                provider: Some(name.to_string()),
                base_url: None,
                model: Some("test-model".to_string()),
            };
            let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
            assert_eq!(
                config.provider.id(),
                expected_id,
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
        assert_eq!(config.provider.id(), "anthropic");
        assert_eq!(config.model.as_deref(), Some("claude-opus-4-7"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_ollama_leaves_model_none_for_setup_to_pick() {
        clear_parish_env();
        let cli = CliOverrides {
            provider: Some("ollama".to_string()),
            base_url: None,
            model: None,
        };
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider.id(), "ollama");
        assert!(
            config.model.is_none(),
            "Ollama must leave model as None so setup_ollama_with_config \
             can pick a hardware-matched gemma4 tier; got {:?}",
            config.model
        );
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
        assert_eq!(config.provider.id(), "openrouter");
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
        assert_eq!(config.provider.id(), "openrouter");
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
        assert_eq!(config.provider.id(), "openrouter");
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
        assert_eq!(config.provider.id(), "ollama");
        assert_eq!(config.base_url, "http://remote-ollama:11434");
        assert_eq!(config.model, "llama3");
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_cloud_config_vercel_ai_requires_base_url() {
        clear_parish_env();
        // Provide an API key so we get past the requires_api_key guard and
        // reach the needs_base_url_from_user check.
        unsafe {
            std::env::set_var("VERCEL_API_KEY", "tok-test");
        }
        let cli = CliCloudOverrides {
            provider: Some("vercel-ai".to_string()),
            base_url: None,
            model: Some("anthropic/claude-sonnet-4-5".to_string()),
        };
        let err = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("base_url") || msg.contains("base-url"),
            "error should mention base_url: {msg}"
        );
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_env_vars_override_provider_base_url_model() {
        clear_parish_env();
        unsafe {
            std::env::set_var("PARISH_PROVIDER", "ollama");
            std::env::set_var("PARISH_BASE_URL", "http://env-host:11434");
            std::env::set_var("PARISH_MODEL", "gemma3:4b");
        }
        let cli = CliOverrides::default();
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        assert_eq!(config.provider.id(), "ollama");
        assert_eq!(config.base_url, "http://env-host:11434");
        assert_eq!(config.model, Some("gemma3:4b".to_string()));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_config_deprecated_parish_ollama_url_fallback() {
        clear_parish_env();
        unsafe {
            std::env::set_var("PARISH_OLLAMA_URL", "http://legacy-host:11434");
        }
        let cli = CliOverrides::default();
        let config = resolve_config(Some(Path::new("/nonexistent")), &cli).unwrap();
        // Deprecated env var should still set base_url
        assert_eq!(config.base_url, "http://legacy-host:11434");
    }

    #[test]
    fn test_provider_api_key_env_var() {
        assert_eq!(
            Provider::anthropic().api_key_env_var(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("openai")
                .unwrap()
                .api_key_env_var(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            Provider::openrouter().api_key_env_var(),
            Some("OPENROUTER_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("google")
                .unwrap()
                .api_key_env_var(),
            Some("GOOGLE_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("groq").unwrap().api_key_env_var(),
            Some("GROQ_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("xai").unwrap().api_key_env_var(),
            Some("XAI_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("mistral")
                .unwrap()
                .api_key_env_var(),
            Some("MISTRAL_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("deepseek")
                .unwrap()
                .api_key_env_var(),
            Some("DEEPSEEK_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("together")
                .unwrap()
                .api_key_env_var(),
            Some("TOGETHER_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("nvidia-nim")
                .unwrap()
                .api_key_env_var(),
            Some("NVIDIA_API_KEY")
        );

        // Local providers and Custom have no env var
        assert_eq!(Provider::ollama().api_key_env_var(), None);
        assert_eq!(
            Provider::from_str_loose("lmstudio")
                .unwrap()
                .api_key_env_var(),
            None
        );
        assert_eq!(
            Provider::from_str_loose("vllmmlx")
                .unwrap()
                .api_key_env_var(),
            None
        );
        assert_eq!(Provider::custom().api_key_env_var(), None);
        assert_eq!(Provider::simulator().api_key_env_var(), None);
    }

    #[test]
    #[serial(parish_env)]
    fn test_provider_is_configured_in_env() {
        clear_parish_env();

        // Local providers are always "configured"
        assert!(Provider::ollama().is_configured_in_env());
        assert!(
            Provider::from_str_loose("lmstudio")
                .unwrap()
                .is_configured_in_env()
        );
        assert!(
            Provider::from_str_loose("vllmmlx")
                .unwrap()
                .is_configured_in_env()
        );
        assert!(Provider::simulator().is_configured_in_env());
        assert!(Provider::custom().is_configured_in_env());

        // Cloud providers without keys are not configured
        assert!(
            !Provider::from_str_loose("openai")
                .unwrap()
                .is_configured_in_env()
        );
        assert!(!Provider::anthropic().is_configured_in_env());

        // Set a key and verify
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-test") };
        assert!(Provider::anthropic().is_configured_in_env());

        // Empty string counts as not configured
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "") };
        assert!(!Provider::anthropic().is_configured_in_env());
    }

    #[test]
    fn test_provider_config_provider_display() {
        let cfg = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        assert_eq!(cfg.provider_display(), "ollama");

        let cfg = ProviderConfig {
            provider: Provider::from_str_loose("nvidia-nim").unwrap(),
            base_url: "https://integrate.api.nvidia.com".to_string(),
            api_key: None,
            model: None,
        };
        assert_eq!(cfg.provider_display(), "nvidia-nim");

        let cfg = ProviderConfig {
            provider: Provider::from_str_loose("openai").unwrap(),
            base_url: "https://api.openai.com".to_string(),
            api_key: None,
            model: None,
        };
        assert_eq!(cfg.provider_display(), "openai");
    }

    #[test]
    fn test_inference_category_name() {
        assert_eq!(InferenceCategory::Dialogue.name(), "dialogue");
        assert_eq!(InferenceCategory::Simulation.name(), "simulation");
        assert_eq!(InferenceCategory::Intent.name(), "intent");
        assert_eq!(InferenceCategory::Reaction.name(), "reaction");
    }

    #[test]
    fn test_inference_category_from_name() {
        assert_eq!(
            InferenceCategory::from_name("dialogue"),
            Some(InferenceCategory::Dialogue)
        );
        assert_eq!(
            InferenceCategory::from_name("simulation"),
            Some(InferenceCategory::Simulation)
        );
        assert_eq!(
            InferenceCategory::from_name("intent"),
            Some(InferenceCategory::Intent)
        );
        assert_eq!(
            InferenceCategory::from_name("reaction"),
            Some(InferenceCategory::Reaction)
        );
        assert_eq!(InferenceCategory::from_name("unknown"), None);
        assert_eq!(
            InferenceCategory::from_name("Dialogue"),
            Some(InferenceCategory::Dialogue)
        );
        assert_eq!(
            InferenceCategory::from_name("DIALOGUE"),
            Some(InferenceCategory::Dialogue)
        );
    }

    #[test]
    fn test_inference_category_env_prefix() {
        assert_eq!(InferenceCategory::Dialogue.env_prefix(), "PARISH_DIALOGUE");
        assert_eq!(
            InferenceCategory::Simulation.env_prefix(),
            "PARISH_SIMULATION"
        );
        assert_eq!(InferenceCategory::Intent.env_prefix(), "PARISH_INTENT");
        assert_eq!(InferenceCategory::Reaction.env_prefix(), "PARISH_REACTION");
    }

    #[test]
    fn test_registry_has_all_providers() {
        let reg = registry();
        // Must have all original 15 providers
        for id in [
            "ollama",
            "lmstudio",
            "openrouter",
            "vllmmlx",
            "openai",
            "google",
            "groq",
            "xai",
            "mistral",
            "deepseek",
            "together",
            "nvidia-nim",
            "anthropic",
            "custom",
            "simulator",
        ] {
            assert!(reg.get(id).is_some(), "registry missing provider: {id}");
        }
        // Must also have new providers
        for id in [
            "vercel-ai",
            "qwen",
            "zhipu",
            "moonshot",
            "siliconflow",
            "cohere",
            "scaleway",
        ] {
            assert!(reg.get(id).is_some(), "registry missing new provider: {id}");
        }
    }

    #[test]
    fn inference_category_idx_matches_all_order() {
        for (i, cat) in InferenceCategory::ALL.iter().enumerate() {
            assert_eq!(cat.idx(), i, "idx() must match position in ALL");
        }
    }

    #[test]
    fn provider_preset_model_returns_correct_field() {
        let p = registry().get("anthropic").unwrap();
        assert!(p.preset_model(InferenceCategory::Dialogue).is_some());
        assert!(p.preset_model(InferenceCategory::Simulation).is_some());
        assert!(p.preset_model(InferenceCategory::Intent).is_some());
        assert!(p.preset_model(InferenceCategory::Reaction).is_some());
    }

    #[test]
    fn provider_has_preset_and_models_array() {
        let cloud = registry().get("openrouter").unwrap();
        assert!(cloud.has_preset());
        let arr = cloud.preset_models();
        assert!(arr.iter().any(|m| m.is_some()));
        let sim = registry().get("simulator").unwrap();
        assert!(!sim.has_preset());
        assert_eq!(sim.preset_models(), [None, None, None, None]);
    }

    #[test]
    fn provider_preset_all_categories_via_model_method() {
        let p = registry().get("groq").unwrap();
        let first_preset = p.presets().first().expect("groq has presets");
        for cat in InferenceCategory::ALL {
            assert_eq!(first_preset.model(cat), p.preset_model(cat));
        }
    }

    #[test]
    fn registry_all_returns_sorted_list_of_all_providers() {
        let all = registry().all();
        assert!(all.len() >= 22, "must have at least 22 providers");
        let ids: Vec<&str> = all.iter().map(|p| p.id()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "all() must return providers sorted by id");
    }

    #[test]
    fn registry_featured_returns_subset_of_all() {
        let featured = registry().featured();
        let all = registry().all();
        assert!(
            !featured.is_empty(),
            "at least one provider should be featured"
        );
        assert!(featured.len() <= all.len());
        for p in &featured {
            assert!(
                all.iter().any(|a| a.id() == p.id()),
                "featured provider {} must also be in all()",
                p.id()
            );
        }
    }

    #[test]
    fn registry_lookup_finds_by_id_and_rejects_unknown() {
        assert!(registry().lookup("anthropic").is_ok());
        assert!(registry().lookup("ANTHROPIC").is_ok());
        let err = registry().lookup("not-a-real-provider-xyz");
        assert!(err.is_err());
    }

    #[test]
    fn provider_from_id_roundtrip() {
        let p = Provider::from_id("openai").expect("openai must exist");
        assert_eq!(p.id(), "openai");
        assert!(Provider::from_id("does-not-exist").is_none());
    }

    #[test]
    fn provider_display_name_and_kind_accessors() {
        let p = Provider::anthropic();
        assert!(!p.display_name().is_empty());
        assert_eq!(p.kind(), ProviderKind::Anthropic);
        let sim = Provider::simulator();
        assert_eq!(sim.kind(), ProviderKind::Simulator);
    }

    #[test]
    fn provider_equality_and_hash_by_id() {
        let a = Provider::openai();
        let b = Provider::from_id("openai").unwrap();
        assert_eq!(a, b);
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(a.clone());
        set.insert(b.clone());
        assert_eq!(set.len(), 1, "same provider id must hash identically");
    }

    #[test]
    fn provider_display_fmt_is_id() {
        let p = Provider::ollama();
        assert_eq!(format!("{p}"), "ollama");
    }

    #[test]
    fn provider_recommended_for_platform_returns_valid_provider() {
        let p = Provider::recommended_for_platform();
        assert!(!p.id().is_empty());
    }

    #[test]
    fn provider_default_is_simulator() {
        let p = Provider::default();
        assert_eq!(p.id(), "simulator");
    }

    #[test]
    fn provider_mod_default_true_fires_when_requires_model_omitted() {
        // Deserializing a ProviderMod without `requires_model` should invoke
        // the `default_true` serde default, yielding `requires_model = true`.
        let raw = r#"
            id = "test-default"
            display_name = "Test"
            kind = "openai-compat"
            default_base_url = "https://example.com"
            requires_api_key = false
        "#;
        let m: ProviderMod = toml::from_str(raw).expect("valid minimal ProviderMod");
        assert!(m.requires_model, "default_true() should produce true");
    }

    #[test]
    fn load_toml_returns_error_when_path_is_directory() {
        // `read_to_string` on a directory triggers the IO-error closure
        // (lines 714-717 in read_toml_config).
        let tmp = std::env::temp_dir();
        let result = resolve_config(Some(tmp.as_path()), &CliOverrides::default());
        assert!(
            result.is_err(),
            "reading a directory as config should error"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("failed to read config file"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn all_named_constructors_return_correct_id() {
        assert_eq!(Provider::openai().id(), "openai");
        assert_eq!(Provider::google().id(), "google");
        assert_eq!(Provider::groq().id(), "groq");
        assert_eq!(Provider::xai().id(), "xai");
        assert_eq!(Provider::mistral().id(), "mistral");
        assert_eq!(Provider::deepseek().id(), "deepseek");
        assert_eq!(Provider::together().id(), "together");
        assert_eq!(Provider::vllmmlx().id(), "vllmmlx");
        assert_eq!(Provider::lmstudio().id(), "lmstudio");
        assert_eq!(Provider::openrouter().id(), "openrouter");
        assert_eq!(Provider::custom().id(), "custom");
        assert_eq!(Provider::ollama().id(), "ollama");
    }
}
