//! Shared mutable runtime configuration for provider, model, and cloud settings.
//!
//! [`GameConfig`] is the single source of truth for LLM provider configuration
//! at runtime. It is used by the Tauri desktop backend, the axum web server,
//! and the headless CLI — eliminating the duplicate `GameConfig` structs that
//! previously lived in each backend.

use crate::config::{FeatureFlags, InferenceCategory};

/// Mutable runtime configuration for provider, model, and cloud settings.
///
/// Each backend wraps this in the appropriate synchronisation primitive
/// (`Mutex<GameConfig>` for Tauri/web, direct field for headless `App`).
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
    /// new [`OpenAiClient`] from those settings. Otherwise falls back to the
    /// supplied `base_client`. The model falls back to `self.model_name` if no
    /// per-category model is set.
    ///
    /// Returns `None` if no client is available (base is `None` and no
    /// category override is configured).
    pub fn resolve_category_client(
        &self,
        cat: InferenceCategory,
        base_client: Option<&crate::inference::openai_client::OpenAiClient>,
    ) -> (
        Option<crate::inference::openai_client::OpenAiClient>,
        String,
    ) {
        let idx = Self::cat_idx(cat);
        let model = self.category_model[idx]
            .clone()
            .unwrap_or_else(|| self.model_name.clone());

        // Build a per-category client if the provider or URL is overridden.
        let client =
            if self.category_base_url[idx].is_some() || self.category_api_key[idx].is_some() {
                let url = self.category_base_url[idx]
                    .as_deref()
                    .unwrap_or(&self.base_url);
                let key = self.category_api_key[idx]
                    .as_deref()
                    .or(self.api_key.as_deref());
                Some(crate::inference::openai_client::OpenAiClient::new(url, key))
            } else {
                base_client.cloned()
            };

        (client, model)
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
            auto_pause_after_secs: 60,
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            flags: FeatureFlags::default(),
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
        assert_eq!(c.auto_pause_after_secs, 60);
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
        use crate::inference::openai_client::OpenAiClient;
        let cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let base = OpenAiClient::new("http://localhost:11434", None);
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        assert!(client.is_some());
        assert_eq!(model, "base-model");
    }

    #[test]
    fn resolve_category_client_uses_override() {
        use crate::inference::openai_client::OpenAiClient;
        let mut cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        let idx = GameConfig::cat_idx(InferenceCategory::Reaction);
        cfg.category_model[idx] = Some("reaction-model".to_string());
        cfg.category_base_url[idx] = Some("https://openrouter.ai/api".to_string());
        cfg.category_api_key[idx] = Some("sk-test".to_string());

        let base = OpenAiClient::new("http://localhost:11434", None);
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        assert!(client.is_some());
        assert_eq!(model, "reaction-model");
    }

    #[test]
    fn resolve_category_client_none_without_base() {
        let cfg = GameConfig::default();
        let (client, _model) = cfg.resolve_category_client(InferenceCategory::Intent, None);
        assert!(client.is_none());
    }
}
