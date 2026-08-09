//! LLM inference timeouts and per-category rate limits
//! (`[engine.inference]`, `[engine.inference.rate_limits]`).

use crate::provider::InferenceCategory;
use serde::Deserialize;

/// LLM inference timeouts.
#[derive(Debug, Deserialize, Clone)]
pub struct InferenceConfig {
    /// Non-streaming request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Streaming request timeout in seconds.
    #[serde(default = "default_streaming_timeout_secs")]
    pub streaming_timeout_secs: u64,
    /// Ollama reachability check timeout in seconds.
    #[serde(default = "default_reachability_timeout_secs")]
    pub reachability_timeout_secs: u64,
    /// Model download timeout in seconds.
    #[serde(default = "default_model_download_timeout_secs")]
    pub model_download_timeout_secs: u64,
    /// Force Ollama setup to delete the selected local model before pulling.
    #[serde(default)]
    pub force_model_redownload: bool,
    /// Model loading/warmup timeout in seconds.
    #[serde(default = "default_model_loading_timeout_secs")]
    pub model_loading_timeout_secs: u64,
    /// Maximum entries in the debug inference log ring buffer.
    #[serde(default = "default_log_capacity")]
    pub log_capacity: usize,
    /// Whether to also write every inference call to disk as JSONL.
    ///
    /// When `true` (default), each backend process writes
    /// `{saves_dir}/inference_logs/{session}.jsonl` so users can zip the
    /// folder and send it with a bug report. The `PARISH_INFERENCE_LOG`
    /// env var or the `--no-inference-log` CLI flag overrides this; the
    /// `/inference-log on|off` slash command toggles it at runtime.
    /// API-key shapes are scrubbed before writing.
    #[serde(default = "default_log_to_disk")]
    pub log_to_disk: bool,
    /// Per-category outbound request rate limits.
    ///
    /// Defaults to no limit. Useful when targeting paid providers
    /// (OpenRouter, Anthropic, etc.) to avoid burning through quota
    /// or hitting `429 Too Many Requests`.
    #[serde(default)]
    pub rate_limits: RateLimitConfig,
    /// Tier-1 dialogue generation parameters.
    ///
    /// These are explicit configuration so benchmarked model/backend profiles
    /// can carry their measured sampling settings into live gameplay.
    #[serde(default)]
    pub dialogue_generation: DialogueGenerationConfig,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            timeout_secs: default_timeout_secs(),
            streaming_timeout_secs: default_streaming_timeout_secs(),
            reachability_timeout_secs: default_reachability_timeout_secs(),
            model_download_timeout_secs: default_model_download_timeout_secs(),
            force_model_redownload: false,
            model_loading_timeout_secs: default_model_loading_timeout_secs(),
            log_capacity: default_log_capacity(),
            log_to_disk: default_log_to_disk(),
            rate_limits: RateLimitConfig::default(),
            dialogue_generation: DialogueGenerationConfig::default(),
        }
    }
}

/// Generation settings for player-facing Tier-1 dialogue.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct DialogueGenerationConfig {
    /// Maximum completion-token budget.
    #[serde(default = "default_dialogue_max_tokens")]
    pub max_tokens: u32,
    /// Sampling temperature.
    #[serde(default = "default_dialogue_temperature")]
    pub temperature: f32,
    /// OpenAI-compatible repetition penalty. `None` omits the field.
    #[serde(default = "default_dialogue_frequency_penalty")]
    pub frequency_penalty: Option<f32>,
    /// Request an OpenAI-compatible JSON object response.
    #[serde(default = "default_dialogue_json_mode")]
    pub json_mode: bool,
    /// Optional OpenAI-compatible reasoning switch. Keep omitted unless a
    /// measured provider/model profile requires it.
    #[serde(default)]
    pub enable_thinking: Option<bool>,
    /// Optional provider reasoning effort. Currently translated by the
    /// OpenRouter client; omitted profiles retain provider defaults.
    #[serde(default)]
    pub reasoning_effort: Option<ReasoningEffort>,
}

impl DialogueGenerationConfig {
    /// Apply a promoted model's measured reasoning profile when the operator
    /// has not explicitly chosen reasoning controls.
    ///
    /// Model identifiers intentionally include the provider namespace. This
    /// prevents evidence gathered through OpenRouter from silently promoting
    /// an unmeasured first-party route with different latency characteristics.
    pub fn for_model(mut self, model: &str) -> Self {
        if model == "google/gemini-3.6-flash"
            && self.enable_thinking.is_none()
            && self.reasoning_effort.is_none()
        {
            self.enable_thinking = Some(true);
            self.reasoning_effort = Some(ReasoningEffort::Low);
        }
        self
    }
}

/// Provider-neutral reasoning effort carried by measured dialogue profiles.
#[derive(Debug, Deserialize, serde::Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl ReasoningEffort {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
            Self::Max => "max",
        }
    }
}

impl Default for DialogueGenerationConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_dialogue_max_tokens(),
            temperature: default_dialogue_temperature(),
            frequency_penalty: default_dialogue_frequency_penalty(),
            json_mode: default_dialogue_json_mode(),
            enable_thinking: None,
            reasoning_effort: None,
        }
    }
}

fn default_dialogue_max_tokens() -> u32 {
    768
}

fn default_dialogue_temperature() -> f32 {
    0.7
}

fn default_dialogue_frequency_penalty() -> Option<f32> {
    Some(0.5)
}

fn default_dialogue_json_mode() -> bool {
    true
}

fn default_timeout_secs() -> u64 {
    // Matches `default_streaming_timeout_secs` so non-streaming
    // inference can absorb cold-loads of large local models. A 30s
    // budget triggered timeouts on Ollama after the model unloaded
    // post-idle (#?). Cloud APIs that respond promptly are unaffected.
    300
}
fn default_streaming_timeout_secs() -> u64 {
    300
}
fn default_reachability_timeout_secs() -> u64 {
    10
}
fn default_model_download_timeout_secs() -> u64 {
    3600
}
fn default_model_loading_timeout_secs() -> u64 {
    300
}
fn default_log_capacity() -> usize {
    50
}
fn default_log_to_disk() -> bool {
    true
}

/// Per-category rate limit configuration for outbound LLM requests.
///
/// All fields are optional. A `None` value disables rate limiting for
/// that category. Categories without an explicit override fall back to
/// the [`RateLimitConfig::default`] field, which applies to the base
/// provider client. Configuration example (`parish.toml`):
///
/// ```toml
/// [engine.inference.rate_limits.default]
/// per_minute = 60
/// burst = 10
///
/// [engine.inference.rate_limits.dialogue]
/// per_minute = 20
/// burst = 4
/// ```
#[derive(Debug, Default, Deserialize, Clone, Copy)]
pub struct RateLimitConfig {
    /// Default rate limit applied to the base provider client.
    /// Categories without an explicit override share this limiter.
    #[serde(default)]
    pub default: Option<CategoryRateLimit>,
    /// Override for the player-facing NPC dialogue category.
    #[serde(default)]
    pub dialogue: Option<CategoryRateLimit>,
    /// Override for the background NPC simulation category.
    #[serde(default)]
    pub simulation: Option<CategoryRateLimit>,
    /// Override for the player intent parsing category.
    #[serde(default)]
    pub intent: Option<CategoryRateLimit>,
    /// Override for the NPC arrival reaction category.
    #[serde(default)]
    pub reaction: Option<CategoryRateLimit>,
}

impl RateLimitConfig {
    /// Returns the configured rate limit for a category override, if any.
    ///
    /// This does NOT fall back to [`Self::default`] — the base limit is
    /// only applied to the base client itself, not to per-category
    /// override clients. (Override clients target a different provider
    /// endpoint and should have their own quota.)
    pub fn for_category(&self, cat: InferenceCategory) -> Option<CategoryRateLimit> {
        match cat {
            InferenceCategory::Dialogue => self.dialogue,
            InferenceCategory::Simulation => self.simulation,
            InferenceCategory::Intent => self.intent,
            InferenceCategory::Reaction => self.reaction,
        }
    }
}

/// A single rate-limit quota: sustained rate plus burst capacity.
///
/// Implements a token-bucket / GCRA model: up to `burst` requests may
/// be issued back-to-back, after which new requests are admitted at
/// `per_minute / 60` per second until the bucket refills.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct CategoryRateLimit {
    /// Sustained rate: maximum number of requests admitted per minute.
    /// Must be greater than zero — a value of zero disables the limiter.
    pub per_minute: u32,
    /// Maximum burst size (token-bucket capacity). Defaults to 1.
    #[serde(default = "default_burst")]
    pub burst: u32,
}

fn default_burst() -> u32 {
    1
}
