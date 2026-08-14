//! BYOK (Bring Your Own Key) IPC handlers.
//!
//! Backend-agnostic per-CLAUDE.md rule 12: each runtime crate (parish-tauri,
//! parish-server, parish-engine) wires a thin shim and reuses these handlers.
//! v1 is desktop-only; web/CLI shims should return a "desktop-only" error.

use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::config::user_config::{
    clear_user_config, load_user_config, mark_onboarding_complete, save_user_config,
};
use crate::config::{InferenceCategory, InferenceConfig, Provider};
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
    pub inference_file_log: crate::inference::file_log::InferenceFileLog,
    pub slots: InferenceSlots<'a>,
    pub secrets: Arc<dyn SecretStore>,
    pub user_config_dir: &'a Path,
}

/// Validates the config (live ping) without touching state. When `api_key` is
/// blank but the provider's standard env var (e.g. `ANTHROPIC_API_KEY`) is set
/// in the host process, validates against the env value — that mirrors the
/// post-onboarding runtime, where the layered resolver picks env over keychain.
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
    let typed = args
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let env_key = provider
        .api_key_env_var()
        .and_then(|var| std::env::var(var).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let key_to_use: Option<&str> = typed.or(env_key.as_deref());
    validate::validate(&provider, &url, key_to_use).await
}

/// Provider metadata exposed to the UI / external clients via
/// `list_available_providers`. Fields match the shape the BYOK wizard
/// rendered from its old hand-curated `byokProviders.ts` arrays.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    /// Lowercase provider id. Matches `Provider::from_str_loose` on the
    /// Rust side.
    pub id: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Short tagline shown under the label in the picker.
    pub blurb: Option<String>,
    /// Where to get an API key.
    pub signup_url: Option<String>,
    /// True when the provider needs an explicit base URL (e.g. `custom`).
    pub needs_base_url: bool,
    /// True when the provider does not require an API key (local /
    /// simulator providers).
    pub keyless: bool,
    /// True for providers the engine recommends as primary picks.
    pub featured: bool,
}

/// Returns every provider in the registry, split into the featured /
/// other lists the BYOK wizard renders. Featured providers come first;
/// both lists are sorted by id.
pub fn handle_list_available_providers()
-> std::collections::HashMap<&'static str, Vec<ProviderInfo>> {
    use parish_config::registry;
    let mut featured: Vec<ProviderInfo> = Vec::new();
    let mut other: Vec<ProviderInfo> = Vec::new();
    for p in registry().all() {
        let info = ProviderInfo {
            id: p.id().to_string(),
            display_name: p.display_name().to_string(),
            blurb: p.0.blurb.clone(),
            signup_url: p.0.signup_url.clone(),
            needs_base_url: p.needs_base_url_from_user(),
            // Use the explicit `keyless` TOML flag (local-inference
            // providers only) rather than `!requires_api_key` — the
            // latter mislabels `custom`, which has no key requirement
            // but still needs a model name + base URL (codex P2
            // regression fix).
            keyless: p.0.keyless,
            featured: p.0.featured,
        };
        if p.0.featured {
            featured.push(info);
        } else {
            other.push(info);
        }
    }
    featured.sort_by_key(|provider| (provider.id != "google", provider.id.clone()));
    other.sort_by(|a, b| a.id.cmp(&b.id));
    let mut out = std::collections::HashMap::new();
    out.insert("featured", featured);
    out.insert("other", other);
    out
}

/// Returns `{provider_id: [preset_options]}` for every provider that has presets.
/// Single source of truth for the wizard's model prefill.
pub fn handle_list_preset_models() -> std::collections::BTreeMap<String, Vec<ProviderPresetOption>>
{
    use parish_config::registry;
    let mut out = std::collections::BTreeMap::new();
    for p in registry().all() {
        if !p.has_preset() {
            continue;
        }
        let opts: Vec<ProviderPresetOption> = p
            .presets()
            .iter()
            .map(|preset| ProviderPresetOption {
                key: preset.key.clone(),
                label: preset.label.clone(),
                dialogue: preset.dialogue.clone(),
                simulation: preset.simulation.clone(),
                intent: preset.intent.clone(),
                reaction: preset.reaction.clone(),
            })
            .collect();
        out.insert(p.id().to_string(), opts);
    }
    out
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPresetOption {
    pub key: String,
    pub label: String,
    pub dialogue: Option<String>,
    pub simulation: Option<String>,
    pub intent: Option<String>,
    pub reaction: Option<String>,
}

/// Backward-compat alias — the first preset option's models as a flat struct.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderPresetModels {
    pub dialogue: Option<String>,
    pub simulation: Option<String>,
    pub intent: Option<String>,
    pub reaction: Option<String>,
}

/// Returns a map of `{provider_id: has_env_key}` for every known provider.
/// The wizard uses this BEFORE the user has saved a choice — so the placeholder
/// hint can say "env var detected" for the provider being picked, not just the
/// current GameConfig provider.
pub fn handle_list_env_keys() -> std::collections::BTreeMap<String, bool> {
    use parish_config::registry;
    let mut out = std::collections::BTreeMap::new();
    for p in registry().all() {
        let has = p
            .api_key_env_var()
            .and_then(|var| std::env::var(var).ok())
            .map(|v: String| !v.trim().is_empty())
            .unwrap_or(false);
        out.insert(p.id().to_string(), has);
    }
    out
}

/// Returns the current effective config (without exposing the API key) so the
/// UI's settings panel can render it.
pub async fn handle_get_provider_config(config: &Mutex<GameConfig>) -> GetProviderConfigResult {
    let cfg = config.lock().await;
    let env_key = Provider::from_str_loose(&cfg.provider_name)
        .ok()
        .and_then(|p| p.api_key_env_var().and_then(|v| std::env::var(v).ok()))
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

    // Persist the registry's canonical id, never a user-supplied alias such as
    // `gemini`; migrations and preset lookup must have one stable identity.
    let provider_name = provider.id().to_string();
    let requested_model = args
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    let recommended_model = provider
        .preset_model(InferenceCategory::Dialogue)
        .map(str::to_string);
    let persisted_model = requested_model
        .as_ref()
        .filter(|model| Some(model.as_str()) != recommended_model.as_deref())
        .cloned();
    let effective_model = requested_model.clone().or(recommended_model);

    // Providers with needs_base_url_from_user require an explicit, non-empty
    // base URL — check both None and "" since callers may send either.
    let base_url = match args.base_url.as_deref() {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ if provider.needs_base_url_from_user() => {
            return Err(ByokError::MissingBaseUrl {
                provider: provider_name,
            });
        }
        _ => provider.default_base_url().to_string(),
    };

    // Cloud providers need a key; local providers (Ollama / LM Studio / vLLM /
    // Simulator) accept None. Custom is "user knows what they're doing" — if
    // their endpoint needs a key they'll provide one. Env-var fallback:
    // when the user leaves the key field blank and the standard provider env
    // var (e.g. ANTHROPIC_API_KEY) is set, treat that as the source of truth —
    // the rebuild pipeline picks it up via the layered config resolver on next
    // launch, and for this session we read it directly so the live inference
    // worker sees a key.
    let env_key = provider
        .api_key_env_var()
        .and_then(|var| std::env::var(var).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if provider.requires_api_key()
        && args.api_key.as_deref().unwrap_or("").is_empty()
        && env_key.is_none()
    {
        return Err(ByokError::MissingApiKey {
            provider: provider_name,
        });
    }

    // Sanitise the user-supplied key. If left blank we fall back to env_key
    // (already trimmed and non-empty when present). The keychain only stores
    // user-supplied keys — never the env var, because the env var IS the
    // user's chosen storage in that case and copying it would create a stale
    // duplicate that loses precedence on next launch.
    let typed_key = args
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let used_env_key = typed_key.is_none() && env_key.is_some();
    let api_key = typed_key.clone().or_else(|| env_key.clone());

    let account = provider_account(&provider_name);
    match (&typed_key, used_env_key) {
        (Some(k), _) => ctx.secrets.set(&account, k)?,
        (None, true) | (None, false) => {
            // No user-supplied key — try to wipe any stale keychain entry so a
            // previously-typed key can't shadow the env var (or, for keyless
            // local providers, doesn't linger after the user switches away).
            // Keychain platform failures (e.g. no default keychain on a
            // sandboxed test profile) are tolerated: the only case where
            // they'd corrupt state is when there *was* a stored key, and a
            // failure to delete it would have surfaced on `set` first.
            if let Err(e) = ctx.secrets.delete(&account) {
                tracing::warn!(
                    %account,
                    error = %e,
                    "secret store delete failed during keyless config; ignoring",
                );
            }
        }
    }

    // Persist non-secret choices to ~/parish.toml.
    let mut user =
        load_user_config(ctx.user_config_dir).map_err(|e| ByokError::Config(e.to_string()))?;
    user.provider = Some(provider_name.clone());
    user.base_url = if args.base_url.is_some() {
        Some(base_url.clone())
    } else {
        None
    };
    // A model equal to the provider's recommended preset is a default, not a
    // user pin. Leaving it absent lets future preset promotions take effect.
    user.model = persisted_model;
    user.category_overrides = args.category_overrides.clone();
    save_user_config(ctx.user_config_dir, &user).map_err(|e| ByokError::Config(e.to_string()))?;

    // Update GameConfig in memory.
    {
        let mut cfg = ctx.config.lock().await;
        cfg.provider_name = provider_name.clone();
        cfg.base_url = base_url.clone();
        cfg.api_key = api_key.clone();
        cfg.model_name = effective_model.unwrap_or_default();
        // Reset per-category overrides; the wizard's optional advanced step
        // sends a complete map on save.
        cfg.category_provider.clear();
        cfg.category_model.clear();
        cfg.category_base_url.clear();
        cfg.category_api_key.clear();
        cfg.apply_user_category_overrides(&args.category_overrides);
        cfg.apply_user_inference_profiles(&user);
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
        ctx.inference_file_log,
        ctx.slots,
    )
    .await;

    // Sentinel — first-run gate now skips on next launch.
    mark_onboarding_complete(ctx.user_config_dir).map_err(|e| ByokError::Config(e.to_string()))?;

    Ok(client)
}

/// Wipes the keychain entry for the current provider, clears the on-disk
/// config, and resets `GameConfig.api_key` to None. Does NOT abort the
/// running inference worker — call set_provider_config afterwards with a new
/// provider to rebuild.
pub async fn handle_clear_provider_config(ctx: ByokContext<'_>) -> Result<(), ByokError> {
    let provider_name = {
        let cfg = ctx.config.lock().await;
        cfg.provider_name.clone()
    };
    let account = provider_account(&provider_name.to_lowercase());
    let _ = ctx.secrets.delete(&account); // idempotent
    clear_user_config(ctx.user_config_dir).map_err(|e| ByokError::Config(e.to_string()))?;
    {
        let mut cfg = ctx.config.lock().await;
        cfg.api_key = None;
        cfg.category_api_key.clear();
    }
    Ok(())
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
            inference_profile_override: Default::default(),
            category_inference_profile: HashMap::new(),
            flags: FeatureFlags::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
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
            log_to_disk: false,
            rate_limits: RateLimitConfig::default(),
            dialogue_generation: Default::default(),
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
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
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
        let body = std::fs::read_to_string(dir.path().join("parish.toml")).unwrap();
        assert!(body.contains("provider = \"anthropic\""));
        assert!(
            !body.contains("model ="),
            "the recommended model is a moving default, not a persisted pin: {body}"
        );
        assert!(!body.contains("api_key"));
        // Onboarding sentinel exists.
        assert!(dir.path().join(".onboarded").exists());
        // Worker installed.
        assert!(slots.queue.lock().await.is_some());
    }

    #[tokio::test]
    async fn list_env_keys_includes_every_provider() {
        // Pin the shape so any new Provider variant must update id() and
        // therefore show up in the wizard's env-detection map.
        let map = handle_list_env_keys();
        for p in parish_config::registry().all() {
            assert!(
                map.contains_key(p.id()),
                "handle_list_env_keys missing entry for {:?}",
                p
            );
        }
    }

    #[tokio::test]
    async fn set_provider_config_accepts_blank_key_when_env_var_set() {
        // Guard against the "leave blank to use env var" UX promise being
        // silently broken at the handler boundary. SAFETY: set_var/remove_var
        // are unsafe on stable Rust 1.74+; tests are single-threaded for this
        // env manipulation. Use a unique sentinel to avoid clobbering a real
        // dev key.
        unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-test-env-fallback") };

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
                provider: "anthropic".to_string(),
                base_url: None,
                model: None,
                api_key: None,
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
                slots: slots.slots(),
                secrets: Arc::clone(&secrets),
                user_config_dir: dir.path(),
            },
        )
        .await;

        unsafe { std::env::remove_var("ANTHROPIC_API_KEY") };

        assert!(res.is_ok(), "env-var fallback should be accepted");
        // Live inference worker uses the env value.
        {
            let g = cfg.lock().await;
            assert_eq!(g.api_key.as_deref(), Some("sk-ant-test-env-fallback"));
        }
        // Keychain is NOT populated — env var IS the storage, no duplicate.
        assert!(secrets.get("provider:anthropic").unwrap().is_none());
        // Onboarding marked complete.
        assert!(dir.path().join(".onboarded").exists());
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
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
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
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
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
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
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
            inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
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
