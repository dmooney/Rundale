//! Shared mutable runtime configuration for provider, model, and cloud settings.
//!
//! [`GameConfig`] is the single source of truth for LLM provider configuration
//! at runtime. It is used by the Tauri desktop backend, the axum web server,
//! and the headless CLI — eliminating the duplicate `GameConfig` structs that
//! previously lived in each backend.

use crate::config::{FeatureFlags, InferenceCategory, RateLimitConfig};

const DEFAULT_AUTO_PAUSE_SECS: u64 = 300;
use crate::inference::InferenceRateLimiter;

/// Mutable runtime configuration for provider, model, and cloud settings.
///
/// Each backend wraps this in the appropriate synchronisation primitive
/// (`Mutex<GameConfig>` for Tauri/web, direct field for headless `App`).
#[derive(Clone)]
pub struct GameConfig {
    /// Display name of the current base provider (e.g. "ollama", "openrouter").
    pub provider_name: String,
    /// Base URL for the current provider API.
    pub base_url: String,
    /// API key for the current provider (None for keyless providers like Ollama).
    pub api_key: Option<String>,
    /// Model name for NPC dialogue inference.
    pub model_name: String,
    /// Cloud provider name for dialogue (None = local only).
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// Whether improv craft mode is enabled for NPC dialogue.
    pub improv_enabled: bool,
    /// Maximum number of autonomous NPC follow-up turns after the initial reply pass.
    pub max_follow_up_turns: usize,
    /// Real-time silence threshold before nearby NPCs may start banter.
    pub idle_banter_after_secs: u64,
    /// Real-time inactivity threshold before the game auto-pauses.
    pub auto_pause_after_secs: u64,
    /// Per-category provider name overrides (None = inherits base).
    /// Index: Dialogue=0, Simulation=1, Intent=2, Reaction=3.
    pub category_provider: [Option<String>; 4],
    /// Per-category model name overrides (None = inherits base).
    pub category_model: [Option<String>; 4],
    /// Per-category API key overrides (None = inherits base).
    pub category_api_key: [Option<String>; 4],
    /// Per-category base URL overrides (None = inherits base).
    pub category_base_url: [Option<String>; 4],
    /// Runtime feature flags for safe deployment of in-progress features.
    pub flags: FeatureFlags,
    /// Per-category outbound rate limiters, pre-built from the
    /// engine `[inference.rate_limits]` config.
    ///
    /// Attached automatically by [`Self::resolve_category_client`] when
    /// constructing per-category override clients. Categories that fall
    /// back to the base client inherit whatever limiter the base client
    /// was constructed with (see [`crate::inference::openai_client::OpenAiClient::with_rate_limit`]).
    pub category_rate_limit: [Option<InferenceRateLimiter>; 4],
    /// Id of the map tile source currently applied (matches a key in
    /// `[engine.map.tile_sources]`). Empty string means "use engine default".
    pub active_tile_source: String,
    /// Registry of available tile sources as `(id, label)` pairs, alphabetical
    /// by id. Populated at backend boot from `EngineConfig::map.tile_sources`
    /// so the `/tiles` command handler can list and validate without taking
    /// a reference to the whole engine config.
    pub tile_sources: Vec<(String, String)>,
    /// Whether the map should reveal all unexplored locations.
    ///
    /// When `false` (default), fog-of-war shows only visited locations and the
    /// immediate frontier. When `true`, all graph nodes are shown with
    /// unvisited locations still marked `visited: false`.
    pub reveal_unexplored_locations: bool,
}

impl GameConfig {
    /// Returns the array index for a category.
    pub fn cat_idx(cat: InferenceCategory) -> usize {
        match cat {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
            InferenceCategory::Reaction => 3,
        }
    }

    /// Resolves the client and model for a given inference category.
    ///
    /// If the category has per-category overrides (provider/key/URL), builds a
    /// new [`AnyClient`] from those settings and attaches the per-category
    /// rate limiter (if configured). Otherwise falls back to the supplied
    /// `base_client`, which already carries its own rate limiter from setup.
    /// The model falls back to `self.model_name` if no per-category model is set.
    ///
    /// The per-category provider (from `category_provider[idx]`, falling back
    /// to `provider_name`) determines which transport is built: OpenAI-compat
    /// for most providers, the native [`AnthropicClient`] for `anthropic`.
    ///
    /// Returns `None` if no client is available (base is `None` and no
    /// category override is configured).
    pub fn resolve_category_client(
        &self,
        cat: InferenceCategory,
        base_client: Option<&crate::inference::AnyClient>,
    ) -> (Option<crate::inference::AnyClient>, String) {
        use parish_config::Provider;
        let idx = Self::cat_idx(cat);
        let model = self.category_model[idx]
            .clone()
            .unwrap_or_else(|| self.model_name.clone());

        // Build a per-category client if the provider, URL, or key is overridden.
        let has_override = self.category_provider[idx].is_some()
            || self.category_base_url[idx].is_some()
            || self.category_api_key[idx].is_some();

        let client = if has_override {
            // Resolve the effective provider for this category.
            let provider_str = self.category_provider[idx]
                .as_deref()
                .unwrap_or(&self.provider_name);
            let provider = Provider::from_str_loose(provider_str).unwrap_or_default();

            // URL falls back to the base URL, then to the provider default
            // (the latter matters when a category switches to Anthropic
            // while the base stays on Ollama — the Anthropic default URL
            // is not empty, so build_client can still reach a real host).
            let url: String = match self.category_base_url[idx].as_deref() {
                Some(u) => u.to_string(),
                None if !self.base_url.is_empty() => self.base_url.clone(),
                None => provider.default_base_url().to_string(),
            };
            let key = self.category_api_key[idx]
                .as_deref()
                .or(self.api_key.as_deref());

            let inference_cfg = parish_config::InferenceConfig::default();
            let built = crate::inference::build_client(&provider, &url, key, &inference_cfg);
            // Attach the per-category rate limiter to the inner variant
            // (rate-limiting is per-transport, not at the AnyClient layer).
            let limiter = self.category_rate_limit[idx].clone();
            Some(attach_rate_limit(built, limiter))
        } else {
            base_client.cloned()
        };

        (client, model)
    }

    /// Installs per-category rate limiters from a parsed config.
    ///
    /// Builds an [`InferenceRateLimiter`] for each category that has an
    /// entry in `cfg`, and stores them in `category_rate_limit`. Categories
    /// without an entry (or with a zero rate) are left unset, meaning the
    /// resolved client for that category will not be rate-limited beyond
    /// whatever limit the base client itself carries.
    ///
    /// The base client's rate limit (`cfg.default`) is NOT installed here —
    /// it must be applied at base-client construction time in `setup.rs`,
    /// because cloning a client preserves its limiter.
    pub fn install_rate_limits(&mut self, cfg: &RateLimitConfig) {
        for cat in InferenceCategory::ALL {
            let idx = Self::cat_idx(cat);
            self.category_rate_limit[idx] =
                InferenceRateLimiter::from_config(cfg.for_category(cat));
        }
    }

    /// Fills in any unset model fields with the appropriate provider preset.
    ///
    /// - The base [`Self::model_name`] is filled from
    ///   `provider.preset_model(InferenceCategory::Dialogue)` if the base
    ///   model name is empty.
    /// - Each [`Self::category_model`] slot that is `None` is filled from
    ///   the *effective* provider's preset for that role — the effective
    ///   provider is the per-category override (`category_provider[idx]`)
    ///   if set, otherwise the base [`Self::provider_name`].
    ///
    /// Already-configured models are never overwritten — this is the
    /// "fill defaults" complement to [`crate::input::Command::ApplyPreset`],
    /// which always overwrites. Returns true if any field changed.
    ///
    /// Called from [`crate::ipc::commands::handle_command`] after
    /// `Command::SetProvider`/`SetCategoryProvider`, and from each
    /// frontend's bootstrap so env-var / TOML / CLI configurations that
    /// only specify a provider still get sensible per-role models.
    pub fn fill_missing_models_from_presets(&mut self) -> bool {
        use parish_config::Provider;
        let mut changed = false;

        // Base model: fall back to the base provider's Dialogue preset.
        if self.model_name.is_empty()
            && let Ok(p) = Provider::from_str_loose(&self.provider_name)
            && let Some(m) = p.preset_model(InferenceCategory::Dialogue)
        {
            self.model_name = m.to_string();
            changed = true;
        }

        // Per-category models: fall back to each effective provider's
        // preset for that specific role.
        for cat in InferenceCategory::ALL {
            let idx = Self::cat_idx(cat);
            if self.category_model[idx].is_some() {
                continue;
            }
            let provider_str = self.category_provider[idx]
                .as_deref()
                .unwrap_or(&self.provider_name);
            if let Ok(p) = Provider::from_str_loose(provider_str)
                && let Some(m) = p.preset_model(cat)
            {
                self.category_model[idx] = Some(m.to_string());
                changed = true;
            }
        }

        changed
    }
}

/// Applies an optional rate limiter to whichever inner client variant
/// lives inside an [`crate::inference::AnyClient`].
///
/// Rate limiting is per-transport: each HTTP client struct carries its
/// own `InferenceRateLimiter`. This helper keeps the per-category
/// resolution site agnostic of which variant is being built.
fn attach_rate_limit(
    client: crate::inference::AnyClient,
    limiter: Option<InferenceRateLimiter>,
) -> crate::inference::AnyClient {
    use crate::inference::AnyClient;
    match (client, limiter) {
        (AnyClient::OpenAi(c), lim) => AnyClient::OpenAi(c.maybe_with_rate_limit(lim)),
        (AnyClient::Anthropic(c), lim) => AnyClient::Anthropic(c.maybe_with_rate_limit(lim)),
        // Simulator and WebGPU have no shared network bottleneck (WebGPU
        // runs in the user's own browser), so per-category rate limits
        // would be meaningless and are silently ignored.
        (c @ AnyClient::Simulator(_), _) => c,
        (c @ AnyClient::WebGpu(_), _) => c,
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            provider_name: "ollama".to_string(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: DEFAULT_AUTO_PAUSE_SECS,
            category_provider: [None, None, None, None],
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            flags: FeatureFlags::default(),
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let c = GameConfig::default();
        assert_eq!(c.provider_name, "ollama");
        assert!(!c.improv_enabled);
        assert!(c.api_key.is_none());
        assert_eq!(c.max_follow_up_turns, 2);
        assert_eq!(c.idle_banter_after_secs, 25);
        assert_eq!(c.auto_pause_after_secs, DEFAULT_AUTO_PAUSE_SECS);
        assert!(c.active_tile_source.is_empty());
        assert!(c.tile_sources.is_empty());
        assert!(!c.reveal_unexplored_locations);
    }

    #[test]
    fn cat_idx_mapping() {
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Dialogue), 0);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Simulation), 1);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Intent), 2);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Reaction), 3);
    }

    #[test]
    fn resolve_category_client_inherits_base() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        assert!(client.is_some());
        assert_eq!(model, "base-model");
    }

    #[test]
    fn resolve_category_client_uses_override() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let mut cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let idx = GameConfig::cat_idx(InferenceCategory::Reaction);
        cfg.category_model[idx] = Some("reaction-model".to_string());
        cfg.category_base_url[idx] = Some("https://openrouter.ai/api".to_string());
        cfg.category_api_key[idx] = Some("sk-test".to_string());

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        assert!(client.is_some());
        assert_eq!(model, "reaction-model");
    }

    #[test]
    fn resolve_category_client_anthropic_override_builds_native_client() {
        // Switching a single category to Anthropic should produce an
        // AnyClient::Anthropic variant, not a misrouted OpenAI-compat
        // client. Regression guard for dmooney/Rundale#172.
        let mut cfg = GameConfig {
            provider_name: "ollama".to_string(),
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let idx = GameConfig::cat_idx(InferenceCategory::Reaction);
        cfg.category_provider[idx] = Some("anthropic".to_string());
        cfg.category_api_key[idx] = Some("sk-ant-test".to_string());
        cfg.category_model[idx] = Some("claude-sonnet-4-5".to_string());

        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, None);
        let client = client.expect("override client built");
        assert!(
            client.as_anthropic().is_some(),
            "expected AnyClient::Anthropic"
        );
        assert_eq!(model, "claude-sonnet-4-5");
    }

    #[test]
    fn resolve_category_client_none_without_base() {
        let cfg = GameConfig::default();
        let (client, _model) = cfg.resolve_category_client(InferenceCategory::Intent, None);
        assert!(client.is_none());
    }

    // ── fill_missing_models_from_presets ─────────────────────────────────────

    #[test]
    fn fill_missing_models_populates_base_and_categories_from_anthropic_preset() {
        let mut cfg = GameConfig {
            provider_name: "anthropic".to_string(),
            ..GameConfig::default()
        };
        let changed = cfg.fill_missing_models_from_presets();
        assert!(changed);
        assert_eq!(cfg.model_name, "claude-opus-4-7");
        assert_eq!(
            cfg.category_model[InferenceCategory::Dialogue.idx()].as_deref(),
            Some("claude-opus-4-7"),
        );
        assert_eq!(
            cfg.category_model[InferenceCategory::Simulation.idx()].as_deref(),
            Some("claude-sonnet-4-6"),
        );
        assert_eq!(
            cfg.category_model[InferenceCategory::Intent.idx()].as_deref(),
            Some("claude-haiku-4-5"),
        );
        assert_eq!(
            cfg.category_model[InferenceCategory::Reaction.idx()].as_deref(),
            Some("claude-sonnet-4-6"),
        );
    }

    #[test]
    fn fill_missing_models_does_not_overwrite_existing_models() {
        let mut cfg = GameConfig {
            provider_name: "anthropic".to_string(),
            model_name: "user-chosen-model".to_string(),
            ..GameConfig::default()
        };
        let dialogue_idx = InferenceCategory::Dialogue.idx();
        cfg.category_model[dialogue_idx] = Some("user-chosen-dialogue".to_string());

        cfg.fill_missing_models_from_presets();
        assert_eq!(cfg.model_name, "user-chosen-model");
        assert_eq!(
            cfg.category_model[dialogue_idx].as_deref(),
            Some("user-chosen-dialogue"),
        );
        // The other three slots should still be filled from the preset.
        assert!(cfg.category_model[InferenceCategory::Simulation.idx()].is_some());
        assert!(cfg.category_model[InferenceCategory::Intent.idx()].is_some());
        assert!(cfg.category_model[InferenceCategory::Reaction.idx()].is_some());
    }

    #[test]
    fn fill_missing_models_uses_per_category_provider_when_overridden() {
        // Base provider ollama; one category overridden to anthropic → that
        // category should pick up the anthropic preset for its role, not the
        // ollama one.
        let mut cfg = GameConfig {
            provider_name: "ollama".to_string(),
            ..GameConfig::default()
        };
        let intent_idx = InferenceCategory::Intent.idx();
        cfg.category_provider[intent_idx] = Some("anthropic".to_string());

        cfg.fill_missing_models_from_presets();
        assert_eq!(
            cfg.category_model[intent_idx].as_deref(),
            Some("claude-haiku-4-5"),
        );
        // The other categories should pick up the ollama presets.
        assert_eq!(
            cfg.category_model[InferenceCategory::Dialogue.idx()].as_deref(),
            Some("qwen3:32b"),
        );
    }

    #[test]
    fn fill_missing_models_no_op_for_provider_without_preset() {
        let mut cfg = GameConfig {
            provider_name: "custom".to_string(),
            ..GameConfig::default()
        };
        let changed = cfg.fill_missing_models_from_presets();
        assert!(!changed);
        assert_eq!(cfg.model_name, "");
        assert!(cfg.category_model.iter().all(Option::is_none));
    }

    #[test]
    fn fill_missing_models_returns_false_when_already_complete() {
        let mut cfg = GameConfig {
            provider_name: "anthropic".to_string(),
            model_name: "x".to_string(),
            category_model: [
                Some("a".to_string()),
                Some("b".to_string()),
                Some("c".to_string()),
                Some("d".to_string()),
            ],
            ..GameConfig::default()
        };
        assert!(!cfg.fill_missing_models_from_presets());
    }

    #[test]
    fn install_rate_limits_populates_configured_categories() {
        use crate::config::{CategoryRateLimit, RateLimitConfig};

        let mut cfg = GameConfig::default();
        let rl = RateLimitConfig {
            dialogue: Some(CategoryRateLimit {
                per_minute: 30,
                burst: 5,
            }),
            intent: Some(CategoryRateLimit {
                per_minute: 120,
                burst: 10,
            }),
            ..RateLimitConfig::default()
        };
        cfg.install_rate_limits(&rl);

        let dial_idx = GameConfig::cat_idx(InferenceCategory::Dialogue);
        let intent_idx = GameConfig::cat_idx(InferenceCategory::Intent);
        let sim_idx = GameConfig::cat_idx(InferenceCategory::Simulation);
        let react_idx = GameConfig::cat_idx(InferenceCategory::Reaction);

        assert!(cfg.category_rate_limit[dial_idx].is_some());
        assert!(cfg.category_rate_limit[intent_idx].is_some());
        assert!(cfg.category_rate_limit[sim_idx].is_none());
        assert!(cfg.category_rate_limit[react_idx].is_none());
    }

    #[test]
    fn install_rate_limits_skips_zero_rate() {
        use crate::config::{CategoryRateLimit, RateLimitConfig};

        let mut cfg = GameConfig::default();
        let rl = RateLimitConfig {
            dialogue: Some(CategoryRateLimit {
                per_minute: 0,
                burst: 5,
            }),
            ..RateLimitConfig::default()
        };
        cfg.install_rate_limits(&rl);
        let idx = GameConfig::cat_idx(InferenceCategory::Dialogue);
        assert!(cfg.category_rate_limit[idx].is_none());
    }

    #[test]
    fn resolve_category_client_attaches_per_category_rate_limit() {
        use crate::config::{CategoryRateLimit, RateLimitConfig};

        let mut cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let idx = GameConfig::cat_idx(InferenceCategory::Reaction);
        cfg.category_base_url[idx] = Some("https://openrouter.ai/api".to_string());
        cfg.category_api_key[idx] = Some("sk-test".to_string());

        // Install a rate limit for the Reaction category.
        let rl_cfg = RateLimitConfig {
            reaction: Some(CategoryRateLimit {
                per_minute: 60,
                burst: 5,
            }),
            ..RateLimitConfig::default()
        };
        cfg.install_rate_limits(&rl_cfg);

        let (client, _model) = cfg.resolve_category_client(InferenceCategory::Reaction, None);
        let client = client.expect("override client built");
        assert!(client.has_rate_limiter());
    }

    #[test]
    fn resolve_category_client_override_without_rate_limit_is_unlimited() {
        let mut cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let idx = GameConfig::cat_idx(InferenceCategory::Reaction);
        cfg.category_base_url[idx] = Some("https://openrouter.ai/api".to_string());
        cfg.category_api_key[idx] = Some("sk-test".to_string());

        let (client, _model) = cfg.resolve_category_client(InferenceCategory::Reaction, None);
        let client = client.expect("override client built");
        assert!(!client.has_rate_limiter());
    }

    #[test]
    fn resolve_category_client_inherited_base_keeps_base_rate_limit() {
        use crate::inference::InferenceRateLimiter;
        use crate::inference::{AnyClient, openai_client::OpenAiClient};

        let cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let limiter = InferenceRateLimiter::new(60, 5).expect("limiter");
        let base = AnyClient::open_ai(
            OpenAiClient::new("http://localhost:11434", None).with_rate_limit(limiter),
        );

        let (client, _model) =
            cfg.resolve_category_client(InferenceCategory::Dialogue, Some(&base));
        let client = client.expect("inherits base");
        assert!(client.has_rate_limiter(), "base limiter is preserved");
    }
}
