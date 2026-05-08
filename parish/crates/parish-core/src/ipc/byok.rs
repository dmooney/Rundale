//! BYOK (Bring Your Own Key) IPC handlers.
//!
//! Backend-agnostic per-CLAUDE.md rule 12: each runtime crate (parish-tauri,
//! parish-server, parish-cli) wires a thin shim and reuses these handlers.
//! v1 is desktop-only; web/CLI shims should return a "desktop-only" error.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::user_config::{
    clear_user_config, load_user_config, mark_onboarding_complete, save_user_config,
};
use crate::config::{InferenceConfig, Provider};
use crate::game_loop::inference::{InferenceSlots, rebuild_inference_worker};
use crate::inference::{AnyClient, InferenceLog, validate};
use crate::ipc::config::GameConfig;
use crate::secret_store::{SecretStore, SecretStoreError, provider_account};

/// Args for `set_provider_config` — the wizard's "Save & continue" button.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SetProviderConfigArgs {
    /// Lowercase provider name. Parsed via `Provider::from_str_loose`.
    pub provider: String,
    /// None → use `Provider::default_base_url()`.
    #[serde(default)]
    pub base_url: Option<String>,
    /// None → fall back to `Provider::preset_model(Dialogue)`.
    #[serde(default)]
    pub model: Option<String>,
    /// None → wipe the keychain entry for this provider. The wizard always
    /// sends Some for cloud providers; Ollama/LM Studio/Simulator send None.
    #[serde(default)]
    pub api_key: Option<String>,
    /// Optional per-category overrides keyed by lowercase category name.
    #[serde(default)]
    pub category_overrides:
        std::collections::BTreeMap<String, crate::config::user_config::CategoryOverride>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetProviderConfigResult {
    pub provider: String,
    pub model: String,
    pub base_url: String,
    /// True iff `GameConfig.api_key` is currently populated.
    pub has_api_key: bool,
    /// True iff the standard provider env var (`ANTHROPIC_API_KEY` etc.) is
    /// set in this process. Lets the UI pre-fill the wizard so a power user
    /// who exported the env var doesn't have to paste again.
    pub has_env_key: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ValidateProviderConfigArgs {
    pub provider: String,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ByokError {
    #[error("unknown provider: {0}")]
    UnknownProvider(String),
    #[error("provider {provider} requires base_url (use Custom only with explicit URL)")]
    MissingBaseUrl { provider: String },
    #[error("provider {provider} requires an API key")]
    MissingApiKey { provider: String },
    #[error("config persistence: {0}")]
    Config(String),
    #[error("secret store: {0}")]
    Secret(#[from] SecretStoreError),
}

/// Per-call inputs that shape behavior but aren't naturally on a single
/// shared object. Grouped to keep the function signature tidy.
pub struct ByokContext<'a> {
    pub config: &'a Mutex<GameConfig>,
    pub inference_config: &'a InferenceConfig,
    pub inference_log: InferenceLog,
    pub slots: InferenceSlots<'a>,
    pub secrets: Arc<dyn SecretStore>,
    pub user_config_dir: &'a Path,
}

/// Validates the config (live ping) without touching state.
pub async fn handle_validate_provider_config(
    args: ValidateProviderConfigArgs,
) -> validate::ValidationOutcome {
    let provider = match Provider::from_str_loose(&args.provider) {
        Ok(p) => p,
        Err(_) => {
            return validate::ValidationOutcome::Unexpected {
                status: 0,
                body_excerpt: format!("unknown provider: {}", args.provider),
            };
        }
    };
    let url = args
        .base_url
        .unwrap_or_else(|| provider.default_base_url().to_string());
    validate::validate(&provider, &url, args.api_key.as_deref()).await
}

/// Returns the current effective config (without exposing the API key) so the
/// UI's settings panel can render it.
pub async fn handle_get_provider_config(
    config: &Mutex<GameConfig>,
) -> GetProviderConfigResult {
    let cfg = config.lock().await;
    let env_key = Provider::from_str_loose(&cfg.provider_name)
        .ok()
        .and_then(|p| p.api_key_env_var())
        .and_then(|var| std::env::var(var).ok())
        .filter(|v| !v.trim().is_empty());
    GetProviderConfigResult {
        provider: cfg.provider_name.clone(),
        model: cfg.model_name.clone(),
        base_url: cfg.base_url.clone(),
        has_api_key: cfg
            .api_key
            .as_deref()
            .map(|k| !k.is_empty())
            .unwrap_or(false),
        has_env_key: env_key.is_some(),
    }
}

/// Applies a new provider config: persists non-secret fields to TOML, stores
/// the API key in the OS keychain, rebuilds the inference worker, and marks
/// onboarding complete.
pub async fn handle_set_provider_config(
    args: SetProviderConfigArgs,
    ctx: ByokContext<'_>,
) -> Result<AnyClient, ByokError> {
    let provider = Provider::from_str_loose(&args.provider)
        .map_err(|_| ByokError::UnknownProvider(args.provider.clone()))?;

    let provider_name = args.provider.to_lowercase();

    // Custom requires an explicit base URL — the curated providers all carry
    // their own default so leaving base_url unset is fine for them.
    let base_url = match (provider.clone(), args.base_url.clone()) {
        (Provider::Custom, None) => {
            return Err(ByokError::MissingBaseUrl {
                provider: provider_name,
            });
        }
        (_, Some(s)) if !s.trim().is_empty() => s,
        _ => provider.default_base_url().to_string(),
    };

    // Cloud providers need a key; local providers (Ollama / LM Studio / vLLM /
    // Simulator) accept None. Custom is "user knows what they're doing" — if
    // their endpoint needs a key they'll provide one.
    if provider.requires_api_key() && args.api_key.as_deref().unwrap_or("").is_empty() {
        return Err(ByokError::MissingApiKey {
            provider: provider_name,
        });
    }

    // Sanitise the key so leading/trailing whitespace from clipboards never
    // makes it into the keychain or HTTP headers.
    let api_key = args
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    // Persist secret to keychain (or wipe if None).
    let account = provider_account(&provider_name);
    match &api_key {
        Some(k) => ctx.secrets.set(&account, k)?,
        None => ctx.secrets.delete(&account)?,
    }

    // Persist non-secret choices to ~/parish.toml.
    let mut user = load_user_config(ctx.user_config_dir)
        .map_err(|e| ByokError::Config(e.to_string()))?;
    user.provider = Some(provider_name.clone());
    user.base_url = if args.base_url.is_some() {
        Some(base_url.clone())
    } else {
        None
    };
    user.model = args.model.clone();
    user.category_overrides = args.category_overrides.clone();
    save_user_config(ctx.user_config_dir, &user)
        .map_err(|e| ByokError::Config(e.to_string()))?;

    // Update GameConfig in memory.
    {
        let mut cfg = ctx.config.lock().await;
        cfg.provider_name = provider_name.clone();
        cfg.base_url = base_url.clone();
        cfg.api_key = api_key.clone();
        if let Some(m) = args.model.clone() {
            cfg.model_name = m;
        }
        // Reset per-category overrides; the wizard's optional advanced step
        // sends a complete map on save.
        cfg.category_provider.clear();
        cfg.category_model.clear();
        cfg.category_base_url.clear();
        cfg.category_api_key.clear();
        for (cat_name, ov) in &args.category_overrides {
            let Ok(cat) = parse_category(cat_name) else {
                continue;
            };
            if let Some(p) = ov.provider.clone() {
                cfg.category_provider.insert(cat, p);
            }
            if let Some(m) = ov.model.clone() {
                cfg.category_model.insert(cat, m);
            }
            if let Some(u) = ov.base_url.clone() {
                cfg.category_base_url.insert(cat, u);
            }
        }
        cfg.fill_missing_models_from_presets();
    }

    // Rebuild the inference worker against the new config.
    let (provider_name_for_rebuild, base_url_for_rebuild, key_for_rebuild) = {
        let cfg = ctx.config.lock().await;
        (
            cfg.provider_name.clone(),
            cfg.base_url.clone(),
            cfg.api_key.clone(),
        )
    };
    let (client, _url_warning) = rebuild_inference_worker(
        &provider_name_for_rebuild,
        &base_url_for_rebuild,
        key_for_rebuild.as_deref(),
        ctx.inference_config,
        ctx.inference_log,
        ctx.slots,
    )
    .await;

    // Sentinel — first-run gate now skips on next launch.
    mark_onboarding_complete(ctx.user_config_dir)
        .map_err(|e| ByokError::Config(e.to_string()))?;

    Ok(client)
}

/// Wipes the keychain entry for the current provider, clears the on-disk
/// config, and resets `GameConfig.api_key` to None. Does NOT abort the
/// running inference worker — call set_provider_config afterwards with a new
/// provider to rebuild.
pub async fn handle_clear_provider_config(
    ctx: ByokContext<'_>,
) -> Result<(), ByokError> {
    let provider_name = {
        let cfg = ctx.config.lock().await;
        cfg.provider_name.clone()
    };
    let account = provider_account(&provider_name.to_lowercase());
    let _ = ctx.secrets.delete(&account); // idempotent
    clear_user_config(ctx.user_config_dir)
        .map_err(|e| ByokError::Config(e.to_string()))?;
    {
        let mut cfg = ctx.config.lock().await;
        cfg.api_key = None;
        cfg.category_api_key.clear();
    }
    Ok(())
}

fn parse_category(name: &str) -> Result<crate::config::InferenceCategory, ()> {
    use crate::config::InferenceCategory;
    match name.to_lowercase().as_str() {
        "dialogue" => Ok(InferenceCategory::Dialogue),
        "simulation" => Ok(InferenceCategory::Simulation),
        "intent" => Ok(InferenceCategory::Intent),
        "reaction" => Ok(InferenceCategory::Reaction),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FeatureFlags;
    use crate::config::RateLimitConfig;
    use crate::config::user_config::UserConfig;
    use crate::secret_store::InMemorySecretStore;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::task::JoinHandle;

    fn empty_game_config() -> GameConfig {
        GameConfig {
            provider_name: "simulator".to_string(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 60,
            auto_pause_after_secs: 300,
            category_provider: HashMap::new(),
            category_model: HashMap::new(),
            category_api_key: HashMap::new(),
            category_base_url: HashMap::new(),
            category_rate_limit: HashMap::new(),
            flags: FeatureFlags::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
        }
    }

    fn fresh_inference_config() -> InferenceConfig {
        InferenceConfig {
            timeout_secs: 30,
            streaming_timeout_secs: 30,
            reachability_timeout_secs: 5,
            model_download_timeout_secs: 600,
            force_model_redownload: false,
            model_loading_timeout_secs: 60,
            log_capacity: 16,
            rate_limits: RateLimitConfig::default(),
        }
    }

    struct Slots {
        client: Mutex<Option<AnyClient>>,
        worker: Mutex<Option<JoinHandle<()>>>,
        queue: Mutex<Option<crate::inference::InferenceQueue>>,
    }

    impl Slots {
        fn new() -> Self {
            Self {
                client: Mutex::new(None),
                worker: Mutex::new(None),
                queue: Mutex::new(None),
            }
        }
        fn slots(&self) -> InferenceSlots<'_> {
            InferenceSlots {
                client: &self.client,
                worker_handle: &self.worker,
                inference_queue: &self.queue,
            }
        }
    }

    #[tokio::test]
    async fn set_provider_config_anthropic_persists_and_rebuilds() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let log = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        let result = handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "anthropic".to_string(),
                base_url: None,
                model: Some("claude-opus-4-7".to_string()),
                api_key: Some("sk-ant-test".to_string()),
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                slots: slots.slots(),
                secrets: Arc::clone(&secrets),
                user_config_dir: dir.path(),
            },
        )
        .await;
        if let Err(e) = result {
            panic!("set_provider_config failed: {e}");
        }

        // GameConfig populated.
        {
            let g = cfg.lock().await;
            assert_eq!(g.provider_name, "anthropic");
            assert_eq!(g.api_key.as_deref(), Some("sk-ant-test"));
            assert_eq!(g.model_name, "claude-opus-4-7");
        }
        // Key in keychain.
        assert_eq!(
            secrets.get("provider:anthropic").unwrap().as_deref(),
            Some("sk-ant-test")
        );
        // user_config TOML has provider but NOT api_key.
        let body =
            std::fs::read_to_string(dir.path().join("parish.toml")).unwrap();
        assert!(body.contains("provider = \"anthropic\""));
        assert!(!body.contains("api_key"));
        // Onboarding sentinel exists.
        assert!(dir.path().join(".onboarded").exists());
        // Worker installed.
        assert!(slots.queue.lock().await.is_some());
    }

    #[tokio::test]
    async fn set_provider_config_rejects_cloud_without_key() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let log = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        let res = handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "openai".to_string(),
                base_url: None,
                model: None,
                api_key: None,
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                slots: slots.slots(),
                secrets: Arc::clone(&secrets),
                user_config_dir: dir.path(),
            },
        )
        .await;
        let err = match res {
            Ok(_) => panic!("expected error, got Ok"),
            Err(e) => e,
        };
        assert!(matches!(err, ByokError::MissingApiKey { .. }));
        // Nothing persisted.
        assert!(!dir.path().join(".onboarded").exists());
        assert!(secrets.get("provider:openai").unwrap().is_none());
    }

    #[tokio::test]
    async fn set_provider_config_custom_requires_base_url() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let log = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        let res = handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "custom".to_string(),
                base_url: None,
                model: Some("foo".to_string()),
                api_key: Some("sk".to_string()),
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                slots: slots.slots(),
                secrets: Arc::clone(&secrets),
                user_config_dir: dir.path(),
            },
        )
        .await;
        let err = match res {
            Ok(_) => panic!("expected error, got Ok"),
            Err(e) => e,
        };
        assert!(matches!(err, ByokError::MissingBaseUrl { .. }));
    }

    #[tokio::test]
    async fn set_provider_config_trims_key_whitespace() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let log = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "anthropic".to_string(),
                base_url: None,
                model: None,
                api_key: Some("  sk-ant-with-spaces \n".to_string()),
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                slots: slots.slots(),
                secrets: Arc::clone(&secrets),
                user_config_dir: dir.path(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            secrets.get("provider:anthropic").unwrap().as_deref(),
            Some("sk-ant-with-spaces")
        );
    }

    #[tokio::test]
    async fn clear_provider_config_wipes_keychain_and_disk() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(GameConfig {
            provider_name: "anthropic".to_string(),
            api_key: Some("sk-ant".to_string()),
            ..empty_game_config()
        });
        secrets.set("provider:anthropic", "sk-ant").unwrap();
        save_user_config(dir.path(), &UserConfig::default()).unwrap();

        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let log = std::sync::Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        handle_clear_provider_config(ByokContext {
            config: &cfg,
            inference_config: &icfg,
            inference_log: log,
            slots: slots.slots(),
            secrets: Arc::clone(&secrets),
            user_config_dir: dir.path(),
        })
        .await
        .unwrap();

        assert!(secrets.get("provider:anthropic").unwrap().is_none());
        assert!(!dir.path().join("parish.toml").exists());
        assert!(cfg.lock().await.api_key.is_none());
    }

    #[tokio::test]
    async fn get_provider_config_does_not_return_key() {
        let cfg = Mutex::new(GameConfig {
            provider_name: "anthropic".to_string(),
            base_url: "https://api.anthropic.com".to_string(),
            api_key: Some("super-secret".to_string()),
            model_name: "claude-opus-4-7".to_string(),
            ..empty_game_config()
        });
        let res = handle_get_provider_config(&cfg).await;
        assert_eq!(res.provider, "anthropic");
        assert_eq!(res.model, "claude-opus-4-7");
        assert!(res.has_api_key);
        // The struct has no api_key field at all — serialize and double-check.
        let json = serde_json::to_string(&res).unwrap();
        assert!(!json.contains("super-secret"));
        assert!(!json.contains("\"api_key\""));
    }
}
