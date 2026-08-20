//! Provider configuration for LLM inference backends.
//!
//! Five providers are built in (see `builtin_providers/`): `simulator`,
//! `ollama`, `vllm`, `vllmmlx`, `custom`. The engine ships them as
//! `include_str!` TOMLs because it manages their local processes / model
//! downloads, or because they serve as universal fallbacks (`custom`).
//!
//! All other providers (anthropic, openai, openrouter, ...) are loaded at
//! runtime from `mods/<id>/providers/<id>.toml` via `discover_mods` +
//! `ProviderRegistry::register_mod_providers`. Adding a new cloud provider
//! requires no recompile — drop a mod under `mods/`.
//!
//! Bootstrap order: `registry()` returns a `RwLock<ProviderRegistry>`
//! pre-populated with builtins. The bootstrap path acquires a write lock
//! and calls `register_mod_providers` after `discover_mods`. After that
//! point the registry is read-only in practice; existing
//! `Arc<ProviderMod>` handles keep pointing at their snapshot regardless
//! of subsequent merges.
//!
//! Structure (#1200 decomposition): the former single module is split into
//! - [`category`] — the [`InferenceCategory`] enum;
//! - [`schema`] — provider schema types (`ProviderKind`, `ProviderPreset`,
//!   `ProviderMod`, …) and the [`Provider`] handle;
//! - [`registry`] — the builtin/mod-loaded [`ProviderRegistry`] + accessors;
//! - [`resolution`] — file/env/CLI config resolution.
//!
//! Every public item is re-exported flat here so the public paths
//! (`provider::Provider`, `provider::resolve_config`, …, and the crate-root
//! `pub use provider::*`) are unchanged.

mod category;
mod registry;
mod resolution;
mod schema;

pub use category::InferenceCategory;
pub use registry::{ProviderRegistry, ensure_mods_loaded, registry};
pub use resolution::{CategoryConfig, ProviderConfig};
pub use schema::{
    InferenceProfile, InferenceProfileOverride, InferenceSubrole, PresetBaseUrls, Provider,
    ProviderKind, ProviderMod, ProviderPreset, ServiceTier, ThinkingLevel, unified_memory_bytes,
};

// Private re-imports so the `tests` submodule (which uses `super::*`) keeps
// reaching the crate-internal helpers it pins.
#[cfg(test)]
use resolution::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtin_providers;
    use serial_test::serial;
    use std::io::Write;
    use std::path::Path;

    #[test]
    #[serial(provider_registry)]
    fn builtin_providers_parse_and_register() {
        // The five engine builtins must always be present in the registry,
        // even before any mod registration runs.
        for id in builtin_providers::BUILTIN_IDS {
            assert!(
                Provider::from_id(id).is_some(),
                "builtin '{}' must be registered on first access",
                id
            );
        }
    }

    #[test]
    #[serial(provider_registry)]
    fn register_mod_providers_merges_new_ids() {
        let raw = r#"
id = "test-merge-provider"
display_name = "Test Merge"
kind = "openai-compat"
default_base_url = "http://127.0.0.1:9001/v1"
requires_api_key = false
requires_model = true
featured = false
"#;
        let m: ProviderMod = toml::from_str(raw).unwrap();
        registry().register_mod_providers(vec![m]).unwrap();
        let p = Provider::from_id("test-merge-provider")
            .expect("registered provider must be retrievable");
        assert_eq!(p.id(), "test-merge-provider");
        assert_eq!(p.default_base_url(), "http://127.0.0.1:9001/v1");
    }

    #[test]
    #[serial(provider_registry)]
    fn register_mod_providers_rejects_collision() {
        let overridden = r#"
id = "simulator"
display_name = "Simulator OVERRIDDEN"
kind = "simulator"
default_base_url = "http://overridden.example/v1"
requires_api_key = false
requires_model = false
featured = false
"#;
        let m: ProviderMod = toml::from_str(overridden).unwrap();
        let error = registry().register_mod_providers(vec![m]).unwrap_err();
        assert!(error.to_string().contains("collision"));
        let p = Provider::from_id("simulator").unwrap();
        assert_ne!(p.default_base_url(), "http://overridden.example/v1");
    }

    fn clear_parish_env() {
        // Make sure provider mods from mods/ are loaded before any env
        // manipulation triggers a registry lookup.
        ensure_mods_loaded();
        // SAFETY: All callers are annotated with `#[serial(parish_env)]`
        unsafe {
            std::env::remove_var("PARISH_PROVIDER");
            std::env::remove_var("PARISH_BASE_URL");
            std::env::remove_var("PARISH_OLLAMA_URL");
            std::env::remove_var("PARISH_MODEL");
            std::env::remove_var("PARISH_CLOUD_PROVIDER");
            std::env::remove_var("PARISH_CLOUD_BASE_URL");
            std::env::remove_var("PARISH_CLOUD_MODEL");
            for category in ["DIALOGUE", "SIMULATION", "INTENT", "REACTION"] {
                std::env::remove_var(format!("PARISH_{category}_PROVIDER"));
                std::env::remove_var(format!("PARISH_{category}_BASE_URL"));
                std::env::remove_var(format!("PARISH_{category}_MODEL"));
            }
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
            Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded")
                .default_base_url(),
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
            "https://generativelanguage.googleapis.com/v1"
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
            Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .default_base_url(),
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
        assert!(
            Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded")
                .requires_api_key()
        );
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
        assert!(
            Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .requires_api_key()
        );

        // Only Ollama and Simulator auto-detect model
        assert!(!Provider::ollama().requires_model());
        assert!(!Provider::simulator().requires_model());
        assert!(
            Provider::from_str_loose("lmstudio")
                .unwrap()
                .requires_model()
        );
        assert!(
            Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded")
                .requires_model()
        );
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
        assert!(
            Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .requires_model()
        );
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

    // Verify that resolve_config(None, …) ignores any parish.toml that may
    // exist on disk — it must return defaults without reading any file path.
    // Previously this test mutated the process-global cwd (via CwdGuard) to
    // place a parish.toml in scope, which was brittle under parallel execution.
    // The refactored version passes the temp file's path explicitly to a
    // companion call to confirm the file *would* be read if a path were
    // supplied, while the None call proves the cwd is never consulted.
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

        // None → defaults, regardless of what sits on disk nearby.
        let cli = CliOverrides::default();
        let config = resolve_config(None, &cli).unwrap();
        assert_eq!(config.provider.id(), "simulator");
        assert_eq!(config.base_url, "");
        assert!(config.api_key.is_none());
        assert!(config.model.is_none());

        // Sanity-check: the file IS parsed when an explicit path is given,
        // confirming the test file is well-formed and would affect results.
        let config_from_path = resolve_config(Some(&path), &cli).unwrap();
        assert_eq!(config_from_path.provider.id(), "lmstudio");
        assert_eq!(config_from_path.base_url, "http://cwd-host:1234");
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
                "https://generativelanguage.googleapis.com/v1",
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
    fn test_resolve_cloud_config_defaults_to_google() {
        clear_parish_env();
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("GOOGLE_API_KEY", "sk-test") };

        let cli = CliCloudOverrides {
            provider: Some("google".to_string()),
            base_url: None,
            model: None,
        };
        let config = resolve_cloud_config(Some(Path::new("/nonexistent")), &cli)
            .unwrap()
            .unwrap();
        assert_eq!(config.provider.id(), "google");
        assert_eq!(
            config.base_url,
            "https://generativelanguage.googleapis.com/v1"
        );
        assert_eq!(config.model, "gemini-3.7-flash");
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
            Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .api_key_env_var(),
            Some("ANTHROPIC_API_KEY")
        );
        assert_eq!(
            Provider::from_str_loose("openai")
                .unwrap()
                .api_key_env_var(),
            Some("OPENAI_API_KEY")
        );
        assert_eq!(
            Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded")
                .api_key_env_var(),
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
        assert!(
            !Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .is_configured_in_env()
        );

        // Set a key and verify
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-test") };
        assert!(
            Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .is_configured_in_env()
        );

        // Empty string counts as not configured
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "") };
        assert!(
            !Provider::from_id("anthropic")
                .expect("anthropic provider mod must be loaded")
                .is_configured_in_env()
        );
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
        let google = registry().get("google").unwrap();
        assert!(google.has_preset());
        assert_eq!(
            google.preset_model(InferenceCategory::Dialogue),
            Some("gemini-3.7-flash"),
        );
        let arr = google.preset_models();
        assert!(arr.iter().any(|m| m.is_some()));

        let openrouter = registry().get("openrouter").unwrap();
        assert_eq!(
            openrouter.preset_model(InferenceCategory::Dialogue),
            Some("google/gemini-3.6-flash"),
            "the default Gemini 3.7 route is native Google, not OpenRouter"
        );
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
        let p = Provider::from_id("anthropic").expect("anthropic provider mod must be loaded");
        assert!(!p.display_name().is_empty());
        assert_eq!(p.kind(), ProviderKind::Anthropic);
        let sim = Provider::simulator();
        assert_eq!(sim.kind(), ProviderKind::Simulator);
    }

    #[test]
    fn provider_equality_and_hash_by_id() {
        let a = Provider::from_id("openai").expect("openai provider mod must be loaded");
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
    #[serial(parish_env)]
    fn category_env_configs_are_sparse_and_inherit_base_fields() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::custom(),
            base_url: "http://127.0.0.1:8010/v1".into(),
            api_key: Some("base-secret".into()),
            model: Some("dialogue-9b".into()),
        };
        // SAFETY: this test is serialised with every `parish_env` test.
        unsafe {
            std::env::set_var("PARISH_INTENT_MODEL", "intent-1.5b");
            std::env::set_var("PARISH_INTENT_BASE_URL", "http://127.0.0.1:8001/v1");
        }

        let resolved = resolve_category_env_configs(&base).expect("category env resolves");
        assert_eq!(resolved.len(), 1);
        let intent = resolved
            .get(&InferenceCategory::Intent)
            .expect("intent override");
        assert_eq!(intent.provider.id(), "custom");
        assert_eq!(intent.base_url, "http://127.0.0.1:8001/v1");
        assert_eq!(intent.model.as_deref(), Some("intent-1.5b"));
        assert_eq!(intent.api_key.as_deref(), Some("base-secret"));
        clear_parish_env();
    }

    #[test]
    #[serial(parish_env)]
    fn category_env_provider_override_uses_provider_defaults() {
        clear_parish_env();
        let base = ProviderConfig {
            provider: Provider::custom(),
            base_url: "http://127.0.0.1:8010/v1".into(),
            api_key: None,
            model: Some("dialogue-9b".into()),
        };
        // SAFETY: this test is serialised with every `parish_env` test.
        unsafe {
            std::env::set_var("PARISH_REACTION_PROVIDER", "simulator");
        }

        let resolved = resolve_category_env_configs(&base).expect("category env resolves");
        let reaction = resolved
            .get(&InferenceCategory::Reaction)
            .expect("reaction override");
        assert_eq!(reaction.provider.id(), "simulator");
        assert_eq!(reaction.base_url, Provider::simulator().default_base_url());
        assert!(reaction.model.is_none());
        assert!(reaction.api_key.is_none());
        clear_parish_env();
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
        assert_eq!(
            Provider::from_id("openai")
                .expect("openai provider mod must be loaded")
                .id(),
            "openai"
        );
        assert_eq!(
            Provider::from_id("google")
                .expect("google provider mod must be loaded")
                .id(),
            "google"
        );
        assert_eq!(
            Provider::from_id("groq")
                .expect("groq provider mod must be loaded")
                .id(),
            "groq"
        );
        assert_eq!(
            Provider::from_id("xai")
                .expect("xai provider mod must be loaded")
                .id(),
            "xai"
        );
        assert_eq!(
            Provider::from_id("mistral")
                .expect("mistral provider mod must be loaded")
                .id(),
            "mistral"
        );
        assert_eq!(
            Provider::from_id("deepseek")
                .expect("deepseek provider mod must be loaded")
                .id(),
            "deepseek"
        );
        assert_eq!(
            Provider::from_id("together")
                .expect("together provider mod must be loaded")
                .id(),
            "together"
        );
        assert_eq!(Provider::vllmmlx().id(), "vllmmlx");
        assert_eq!(
            Provider::from_id("lmstudio")
                .expect("lmstudio provider mod must be loaded")
                .id(),
            "lmstudio"
        );
        assert_eq!(
            Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded")
                .id(),
            "openrouter"
        );
        assert_eq!(Provider::custom().id(), "custom");
        assert_eq!(Provider::ollama().id(), "ollama");
    }
}
