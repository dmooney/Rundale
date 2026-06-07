//! Provider schema types and the [`Provider`] handle.
//!
//! `ProviderKind`, `PresetBaseUrls`, `ProviderPreset`, and `ProviderMod` are
//! the TOML-deserialised data describing a provider; [`Provider`] is the
//! cheap `Arc`-wrapped handle the rest of the engine carries. Lookups go
//! through the [`super::registry`] module. Split out of the monolithic
//! `provider` module (#1200).

use std::sync::Arc;

use parish_types::ParishError;

use super::category::InferenceCategory;
use super::registry::registry;

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

/// Per-category base-URL overrides shipped with a provider preset.
///
/// Two-slot loadouts (e.g. vllm-mlx on Apple Silicon: 14B on :8000 + 1.5B
/// on :8001) need each category routed to the slot where its preset model
/// is actually loaded. Without this, `fill_missing_models_from_presets`
/// picks the preset model but inherits the base URL — guaranteeing a
/// model/URL mismatch and a 404 on every reaction/simulation call.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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

/// All data about one provider, deserialized from TOML.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
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
    /// True when the provider is local inference where API keys are
    /// irrelevant (ollama, lmstudio, vllm, vllm_mlx, simulator). The
    /// onboarding wizard uses this to relax model-name/key guards.
    /// Distinct from `requires_api_key`: `custom` does not require an
    /// API key but still needs a model name and base URL, so its
    /// `keyless` is `false` (codex P2 regression fix).
    #[serde(default)]
    pub keyless: bool,
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

    // Builtin convenience constructors. Builtins are always present.
    pub fn ollama() -> Self {
        Self::from_id("ollama").expect("ollama builtin must be registered")
    }
    pub fn simulator() -> Self {
        Self::from_id("simulator").expect("simulator builtin must be registered")
    }
    pub fn custom() -> Self {
        Self::from_id("custom").expect("custom builtin must be registered")
    }
    pub fn vllm() -> Self {
        Self::from_id("vllm").expect("vllm builtin must be registered")
    }
    pub fn vllmmlx() -> Self {
        Self::from_id("vllmmlx").expect("vllmmlx builtin must be registered")
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
