//! Shared mutable runtime configuration for provider, model, and cloud settings.
//!
//! [`GameConfig`] is the single source of truth for LLM provider configuration
//! at runtime. It is used by the Tauri desktop backend, the axum web server,
//! and the headless CLI — eliminating the duplicate `GameConfig` structs that
//! previously lived in each backend.

use crate::config::InferenceCategory;

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
    /// Per-category provider name overrides (None = inherits base).
    /// Index: Dialogue=0, Simulation=1, Intent=2, Reaction=3.
    pub category_provider: [Option<String>; 4],
    /// Per-category model name overrides (None = inherits base).
    pub category_model: [Option<String>; 4],
    /// Per-category API key overrides (None = inherits base).
    pub category_api_key: [Option<String>; 4],
    /// Per-category base URL overrides (None = inherits base).
    pub category_base_url: [Option<String>; 4],
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
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
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
    }

    #[test]
    fn cat_idx_mapping() {
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Dialogue), 0);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Simulation), 1);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Intent), 2);
        assert_eq!(GameConfig::cat_idx(InferenceCategory::Reaction), 3);
    }
}
