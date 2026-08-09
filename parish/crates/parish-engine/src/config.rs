//! Provider configuration for LLM inference backends.
//!
//! Shared types (`Provider`, `ProviderConfig`, `CloudConfig`, `CliOverrides`,
//! `CliCloudOverrides`, `resolve_config`, `resolve_cloud_config`) live in
//! `parish-core` and are re-exported here. Only per-category configuration
//! logic is unique to this crate.
//!
//! Each inference category (dialogue, simulation, intent) can be independently
//! configured with its own provider, model, base URL, and API key via
//! per-category TOML sections, environment variables, or CLI flags. Unconfigured
//! categories inherit from the base `[provider]` config.

pub use parish_core::config::{
    CategoryConfig, CliCloudOverrides, CliOverrides, CloudConfig, CognitiveTierConfig,
    EncounterConfig, EngineConfig, FeatureFlags, InferenceCategory, InferenceConfig, NpcConfig,
    PaletteConfig, PersistenceConfig, Provider, ProviderConfig, RelationshipLabelConfig,
    SpeedConfig, WorldConfig, resolve_cloud_config, resolve_config,
};

use crate::error::ParishError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// CLI-provided overrides for per-category provider configuration.
#[derive(Debug, Default)]
pub struct CliCategoryOverrides {
    /// Per-category CLI overrides keyed by category name.
    pub categories: HashMap<String, CliOverrides>,
}

/// Raw TOML file structure for `parish.toml` (extended with per-category provider overrides).
#[derive(Debug, Deserialize, Default)]
struct TomlConfig {
    #[serde(default)]
    provider: TomlProvider,
    #[serde(default)]
    cloud: TomlCloud,
}

/// The `[provider]` section of the TOML config.
/// Only the per-category sub-tables are needed here; base provider fields
/// (name, base_url, api_key, model) are resolved via `resolve_config` from
/// parish-core and passed in as `ProviderConfig`. Serde ignores unknown keys.
#[derive(Debug, Deserialize, Default)]
struct TomlProvider {
    /// Per-category overrides: `[provider.dialogue]`, `[provider.simulation]`, `[provider.intent]`, `[provider.reaction]`.
    #[serde(default)]
    dialogue: Option<TomlCategoryOverride>,
    /// Per-category override for simulation.
    #[serde(default)]
    simulation: Option<TomlCategoryOverride>,
    /// Per-category override for intent parsing.
    #[serde(default)]
    intent: Option<TomlCategoryOverride>,
    /// Per-category override for NPC arrival reactions.
    #[serde(default)]
    reaction: Option<TomlCategoryOverride>,
}

/// Per-category provider override in TOML (e.g. `[provider.dialogue]`).
#[derive(Debug, Deserialize, Default, Clone)]
struct TomlCategoryOverride {
    /// Provider name override for this category.
    name: Option<String>,
    /// Base URL override for this category.
    base_url: Option<String>,
    /// API key override for this category.
    api_key: Option<String>,
    /// Model name override for this category.
    model: Option<String>,
}

/// The `[cloud]` section of the TOML config for legacy cloud dialogue provider.
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

/// Resolves per-category provider configurations.
///
/// For each [`InferenceCategory`], layers overrides on top of the base
/// [`ProviderConfig`]:
/// 1. Start with the base provider config as defaults
/// 2. Layer TOML `[provider.<category>]` overrides
/// 3. Layer `PARISH_<CATEGORY>_*` environment variables
/// 4. Layer per-category CLI flags
/// 5. For dialogue only: fall back to legacy `[cloud]` / `PARISH_CLOUD_*` / `--cloud-*`
///
/// Returns a map from category to resolved [`CategoryConfig`]. Categories with
/// no overrides are omitted (callers should fall back to the base config).
pub fn resolve_category_configs(
    config_path: Option<&Path>,
    base: &ProviderConfig,
    cli_categories: &CliCategoryOverrides,
    cli_cloud: &CliCloudOverrides,
) -> Result<HashMap<InferenceCategory, CategoryConfig>, ParishError> {
    let toml_cfg = match config_path {
        Some(path) => read_toml_config(path)?,
        None => TomlConfig::default(),
    };
    let mut result = HashMap::new();

    for category in InferenceCategory::ALL {
        if let Some(cfg) =
            resolve_single_category(&toml_cfg, category, base, cli_categories, cli_cloud)?
        {
            result.insert(category, cfg);
        }
    }

    Ok(result)
}

/// Returns the TOML category override for the given category.
fn category_toml_override(
    toml_cfg: &TomlConfig,
    cat: InferenceCategory,
) -> Option<&TomlCategoryOverride> {
    match cat {
        InferenceCategory::Dialogue => toml_cfg.provider.dialogue.as_ref(),
        InferenceCategory::Simulation => toml_cfg.provider.simulation.as_ref(),
        InferenceCategory::Intent => toml_cfg.provider.intent.as_ref(),
        InferenceCategory::Reaction => toml_cfg.provider.reaction.as_ref(),
    }
}

/// Checks whether the given category has any override from TOML, env, CLI, or
/// legacy cloud config.
fn category_has_overrides(
    toml_cfg: &TomlConfig,
    category: InferenceCategory,
    cli_override: Option<&CliOverrides>,
    cli_cloud: &CliCloudOverrides,
) -> bool {
    let toml_override = category_toml_override(toml_cfg, category);
    let has_toml = toml_override.is_some_and(|t| {
        t.name.is_some() || t.base_url.is_some() || t.api_key.is_some() || t.model.is_some()
    });
    let has_env = env_non_empty(&format!("{}_PROVIDER", category.env_prefix())).is_some()
        || env_non_empty(&format!("{}_BASE_URL", category.env_prefix())).is_some()
        || env_non_empty(&format!("{}_MODEL", category.env_prefix())).is_some();
    let has_cli = cli_override
        .is_some_and(|c| c.provider.is_some() || c.base_url.is_some() || c.model.is_some());

    let has_legacy_cloud = category == InferenceCategory::Dialogue
        && (toml_cfg.cloud.name.is_some()
            || toml_cfg.cloud.base_url.is_some()
            || toml_cfg.cloud.api_key.is_some()
            || toml_cfg.cloud.model.is_some()
            || env_non_empty("PARISH_CLOUD_PROVIDER").is_some()
            || env_non_empty("PARISH_CLOUD_BASE_URL").is_some()
            || env_non_empty("PARISH_CLOUD_MODEL").is_some()
            || cli_cloud.provider.is_some()
            || cli_cloud.base_url.is_some()
            || cli_cloud.model.is_some());

    has_toml || has_env || has_cli || has_legacy_cloud
}

/// Resolves configuration for a single inference category by applying the
/// 5 override layers. Returns `None` if there are no overrides for this category.
fn resolve_single_category(
    toml_cfg: &TomlConfig,
    category: InferenceCategory,
    base: &ProviderConfig,
    cli_categories: &CliCategoryOverrides,
    cli_cloud: &CliCloudOverrides,
) -> Result<Option<CategoryConfig>, ParishError> {
    let toml_override = category_toml_override(toml_cfg, category).cloned();
    let cli_override = cli_categories.categories.get(category.name());

    if !category_has_overrides(toml_cfg, category, cli_override, cli_cloud) {
        return Ok(None);
    }

    let mut provider_str: Option<String> = None;
    let mut cat_base_url: Option<String> = None;
    let mut cat_api_key: Option<String> = None;
    let mut cat_model: Option<String> = base.model.clone();

    // Layer 1: Legacy [cloud] for dialogue (lowest priority override)
    if category == InferenceCategory::Dialogue {
        if let Some(ref name) = toml_cfg.cloud.name {
            provider_str = Some(name.clone());
        }
        if let Some(ref url) = toml_cfg.cloud.base_url {
            cat_base_url = Some(url.clone());
        }
        if let Some(ref key) = toml_cfg.cloud.api_key {
            cat_api_key = Some(key.clone());
        }
        if let Some(ref m) = toml_cfg.cloud.model {
            cat_model = Some(m.clone());
        }
        if let Some(val) = env_non_empty("PARISH_CLOUD_PROVIDER") {
            provider_str = Some(val);
        }
        if let Some(val) = env_non_empty("PARISH_CLOUD_BASE_URL") {
            cat_base_url = Some(val);
        }
        if let Some(val) = env_non_empty("PARISH_CLOUD_MODEL") {
            cat_model = Some(val);
        }
        if let Some(ref val) = cli_cloud.provider {
            provider_str = Some(val.clone());
        }
        if let Some(ref val) = cli_cloud.base_url {
            cat_base_url = Some(val.clone());
        }
        if let Some(ref val) = cli_cloud.model {
            cat_model = Some(val.clone());
        }
    }

    // Layer 2: TOML [provider.<category>] overrides legacy cloud
    if let Some(ref toml_ov) = toml_override {
        if let Some(ref name) = toml_ov.name {
            provider_str = Some(name.clone());
        }
        if let Some(ref url) = toml_ov.base_url {
            cat_base_url = Some(url.clone());
        }
        if let Some(ref key) = toml_ov.api_key {
            cat_api_key = Some(key.clone());
        }
        if let Some(ref m) = toml_ov.model {
            cat_model = Some(m.clone());
        }
    }

    // Layer 3: Per-category env vars
    let prefix = category.env_prefix();
    if let Some(val) = env_non_empty(&format!("{prefix}_PROVIDER")) {
        provider_str = Some(val);
    }
    if let Some(val) = env_non_empty(&format!("{prefix}_BASE_URL")) {
        cat_base_url = Some(val);
    }
    if let Some(val) = env_non_empty(&format!("{prefix}_MODEL")) {
        cat_model = Some(val);
    }

    // Layer 4: Per-category CLI flags
    if let Some(cli_ov) = cli_override {
        if let Some(ref val) = cli_ov.provider {
            provider_str = Some(val.clone());
        }
        if let Some(ref val) = cli_ov.base_url {
            cat_base_url = Some(val.clone());
        }
        if let Some(ref val) = cli_ov.model {
            cat_model = Some(val.clone());
        }
    }

    // Resolve provider before the API key check
    let provider = match &provider_str {
        Some(s) => Provider::from_str_loose(s)?,
        None => base.provider.clone(),
    };

    // Layer 5: Standard provider API key env var
    if let Some(val) = provider.api_key_env_var().and_then(env_non_empty) {
        cat_api_key = Some(val);
    }

    let resolved_base_url = match cat_base_url {
        Some(url) if !url.is_empty() => url,
        _ => {
            if provider_str.is_some() {
                provider.default_base_url().to_string()
            } else {
                base.base_url.clone()
            }
        }
    };

    let cat_api_key = cat_api_key.filter(|s| !s.is_empty());
    let cat_model = cat_model.filter(|s| !s.is_empty());
    // Skipped for Ollama: auto-setup pulls one hardware-matched model.
    // Leaving `cat_model` as `None` lets `build_inference_clients` fall
    // through to the auto-setup-resolved `base_model`.
    let cat_model = if provider.id() == "ollama" {
        cat_model
    } else {
        cat_model.or_else(|| provider.preset_model(category).map(String::from))
    };

    if provider.requires_api_key() && cat_api_key.is_none() {
        let hint = provider
            .api_key_env_var()
            .unwrap_or("the provider API key env var");
        return Err(ParishError::Config(format!(
            "{} {:?} provider requires an API key. Set {}.",
            category.name(),
            provider,
            hint
        )));
    }
    if provider.needs_base_url_from_user() && resolved_base_url.is_empty() {
        return Err(ParishError::Config(format!(
            "{} custom provider requires a base_url. Set {}_BASE_URL or --{}-base-url.",
            category.name(),
            prefix,
            category.name()
        )));
    }

    Ok(Some(CategoryConfig {
        provider,
        base_url: resolved_base_url,
        api_key: cat_api_key,
        model: cat_model,
    }))
}

/// Returns the value of an environment variable if it exists and is non-empty.
fn env_non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// Reads and parses a TOML config file (with per-category provider fields).
/// Returns default config if file doesn't exist.
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

    /// Clears all PARISH_ env vars so tests don't interfere with each other.
    ///
    /// Callers **must** annotate their test with `#[serial(parish_env)]` so
    /// env-mutating tests never run concurrently — Rust 2024 marks
    /// `std::env::remove_var` and `set_var` unsafe precisely because
    /// concurrent access is UB.
    fn clear_parish_env() {
        // Provider mods now live under mods/<id>/. parish-engine tests reference
        // cloud providers (openrouter, anthropic, ...) that the registry only
        // knows about after this helper walks them in from the workspace.
        parish_core::config::ensure_mods_loaded();
        // SAFETY: All callers are annotated with `#[serial(parish_env)]`,
        // which serialises every test that touches env vars across this
        // module and the sibling `parish-config` tests.
        unsafe {
            std::env::remove_var("PARISH_PROVIDER");
            std::env::remove_var("PARISH_BASE_URL");
            std::env::remove_var("PARISH_OLLAMA_URL");
            std::env::remove_var("PARISH_MODEL");
            std::env::remove_var("PARISH_CLOUD_PROVIDER");
            std::env::remove_var("PARISH_CLOUD_BASE_URL");
            std::env::remove_var("PARISH_CLOUD_MODEL");
            for cat in &["DIALOGUE", "SIMULATION", "INTENT", "REACTION"] {
                std::env::remove_var(format!("PARISH_{cat}_PROVIDER"));
                std::env::remove_var(format!("PARISH_{cat}_BASE_URL"));
                std::env::remove_var(format!("PARISH_{cat}_MODEL"));
            }
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
    fn test_category_names() {
        assert_eq!(InferenceCategory::Dialogue.name(), "dialogue");
        assert_eq!(InferenceCategory::Simulation.name(), "simulation");
        assert_eq!(InferenceCategory::Intent.name(), "intent");
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_empty_when_no_overrides() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();
        assert!(configs.is_empty());
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_from_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"
model = "qwen3:14b"

[provider.dialogue]
name = "openrouter"
base_url = "https://openrouter.ai/api"
api_key = "sk-cat-test"
model = "anthropic/claude-sonnet-4-20250514"

[provider.intent]
model = "qwen3:1.5b"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        // Dialogue should be overridden to OpenRouter
        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(
            dialogue.provider,
            Provider::from_id("openrouter").expect("openrouter provider mod must be loaded")
        );
        assert_eq!(dialogue.base_url, "https://openrouter.ai/api");
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-cat-test"));
        assert_eq!(
            dialogue.model.as_deref(),
            Some("anthropic/claude-sonnet-4-20250514")
        );

        // Intent should inherit Ollama but override model
        let intent = configs.get(&InferenceCategory::Intent).unwrap();
        assert_eq!(intent.provider, Provider::ollama());
        assert_eq!(intent.base_url, "http://localhost:11434");
        assert_eq!(intent.model.as_deref(), Some("qwen3:1.5b"));

        // Simulation should not be present (no overrides)
        assert!(!configs.contains_key(&InferenceCategory::Simulation));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_legacy_cloud_maps_to_dialogue() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-legacy") };
        let cli_cloud = CliCloudOverrides {
            provider: Some("openrouter".to_string()),
            base_url: None,
            model: Some("gpt-4".to_string()),
        };
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();

        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(
            dialogue.provider,
            Provider::from_id("openrouter").expect("openrouter provider mod must be loaded")
        );
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-legacy"));
        assert_eq!(dialogue.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_toml_overrides_legacy_cloud() {
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
api_key = "sk-cloud-legacy"
model = "legacy-model"

[provider.dialogue]
name = "custom"
base_url = "https://my-api.example.com"
api_key = "sk-new"
model = "new-model"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        // [provider.dialogue] should override [cloud]
        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.provider, Provider::custom());
        assert_eq!(dialogue.base_url, "https://my-api.example.com");
        assert_eq!(dialogue.api_key.as_deref(), Some("sk-new"));
        assert_eq!(dialogue.model.as_deref(), Some("new-model"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_cli_overrides() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let mut categories = HashMap::new();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("OPENROUTER_API_KEY", "sk-sim") };
        categories.insert(
            "simulation".to_string(),
            CliOverrides {
                provider: Some("openrouter".to_string()),
                base_url: None,
                model: Some("sim-model".to_string()),
            },
        );
        let cli_cat = CliCategoryOverrides { categories };
        let cli_cloud = CliCloudOverrides::default();
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();

        let sim = configs.get(&InferenceCategory::Simulation).unwrap();
        assert_eq!(
            sim.provider,
            Provider::from_id("openrouter").expect("openrouter provider mod must be loaded")
        );
        assert_eq!(sim.base_url, "https://openrouter.ai/api");
        assert_eq!(sim.api_key.as_deref(), Some("sk-sim"));
        assert_eq!(sim.model.as_deref(), Some("sim-model"));
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_validates_api_key() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        let mut categories = HashMap::new();
        // OPENROUTER_API_KEY is cleared by clear_parish_env — should fail.
        categories.insert(
            "intent".to_string(),
            CliOverrides {
                provider: Some("openrouter".to_string()),
                base_url: None,
                model: Some("model".to_string()),
            },
        );
        let cli_cat = CliCategoryOverrides { categories };
        let cli_cloud = CliCloudOverrides::default();
        let result =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("intent"), "got: {}", err_msg);
        assert!(err_msg.contains("API key"), "got: {}", err_msg);
    }

    #[test]
    #[serial(parish_env)]
    fn test_resolve_category_configs_inherits_base_url() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"
base_url = "http://remote-ollama:11434"

[provider.simulation]
model = "qwen3:8b"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://remote-ollama:11434".to_string(),
            api_key: None,
            model: Some("qwen3:14b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        let sim = configs.get(&InferenceCategory::Simulation).unwrap();
        assert_eq!(sim.provider, Provider::ollama());
        // Should inherit the base URL since provider wasn't overridden
        assert_eq!(sim.base_url, "http://remote-ollama:11434");
        assert_eq!(sim.model.as_deref(), Some("qwen3:8b"));
    }

    #[test]
    #[serial(parish_env)]
    fn resolve_category_configs_ollama_override_without_model_skips_preset_fill() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "ollama"

[provider.dialogue]
base_url = "http://localhost:11434"
"#
        )
        .unwrap();

        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: None,
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.provider, Provider::ollama());
        assert!(
            dialogue.model.is_none(),
            "Ollama category without explicit model must not be filled \
             from the static qwen3 preset; it should fall through to the \
             base model resolved by setup. Got: {:?}",
            dialogue.model
        );
    }

    #[test]
    #[serial(parish_env)]
    fn resolve_category_configs_ollama_respects_env_model_override() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe {
            std::env::set_var("PARISH_DIALOGUE_MODEL", "gemma4:31b");
        }
        let base = ProviderConfig {
            provider: Provider::ollama(),
            base_url: "http://localhost:11434".to_string(),
            api_key: None,
            model: Some("gemma4:e2b".to_string()),
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs =
            resolve_category_configs(Some(Path::new("/nonexistent")), &base, &cli_cat, &cli_cloud)
                .unwrap();

        let dialogue = configs.get(&InferenceCategory::Dialogue).unwrap();
        assert_eq!(dialogue.model.as_deref(), Some("gemma4:31b"));
        // Other categories were not overridden, so they should not appear.
        assert!(!configs.contains_key(&InferenceCategory::Simulation));
        assert!(!configs.contains_key(&InferenceCategory::Intent));
    }

    #[test]
    #[serial(parish_env)]
    fn resolve_category_configs_anthropic_still_fills_presets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(
            f,
            r#"
[provider]
name = "anthropic"

[provider.intent]
name = "anthropic"
"#
        )
        .unwrap();

        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe {
            std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test");
        }
        let base = ProviderConfig {
            provider: Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded"),
            base_url: "https://api.anthropic.com".to_string(),
            api_key: Some("sk-ant-test".to_string()),
            model: None,
        };
        let cli_cat = CliCategoryOverrides::default();
        let cli_cloud = CliCloudOverrides::default();
        let configs = resolve_category_configs(Some(&path), &base, &cli_cat, &cli_cloud).unwrap();

        let intent = configs.get(&InferenceCategory::Intent).unwrap();
        assert_eq!(
            intent.provider,
            Provider::from_id("anthropic").expect("anthropic provider mod must be loaded")
        );
        assert_eq!(intent.model.as_deref(), Some("claude-haiku-4-5"));
    }
}
