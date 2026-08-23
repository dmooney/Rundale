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
    ONBOARDING_MARKER_FILENAME, mark_onboarding_complete, onboarding_complete,
};
use crate::config::{InferenceConfig, Provider};
use crate::game_loop::inference::InferenceSlots;
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
    /// Explicit acknowledgement for plaintext non-loopback custom endpoints.
    #[serde(default)]
    pub allow_insecure_http: bool,
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
    #[serde(default)]
    pub allow_insecure_http: bool,
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
    /// Explicit project config authority used for the atomic v2 reload.
    pub project_config_path: &'a Path,
    /// Startup-resolved user-data root for post-publication catalog refresh.
    pub catalog_user_data: Option<&'a Path>,
    /// Production runtimes publish config and all clients through this one
    /// epoch seam. Tests that exercise persistence in isolation may omit it.
    pub runtime_manager: Option<Arc<crate::inference_runtime_v2::InferenceRuntimeManagerV2>>,
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
    if insecure_non_loopback(&url) && !args.allow_insecure_http {
        return validate::ValidationOutcome::Unexpected {
            status: 0,
            body_excerpt: "plaintext HTTP to a non-loopback host requires explicit insecure transport acknowledgement".into(),
        };
    }
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

fn insecure_non_loopback(url: &str) -> bool {
    reqwest::Url::parse(url).ok().is_some_and(|url| {
        url.scheme() == "http"
            && !url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            })
    })
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
    // `gemini`; preset lookup and credential slots must have one stable identity.
    let provider_name = provider.id().to_string();
    let requested_model = args
        .model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_string);
    let recommended_model = provider
        .preset_model(crate::config::InferenceCategory::Dialogue)
        .map(str::to_string);
    // The recommended preset is a provider default, not a user pin. Omitting it
    // from the v2 route lets a future authored promotion take effect.
    let selected_model = requested_model
        .as_ref()
        .filter(|model| Some(model.as_str()) != recommended_model.as_deref())
        .cloned();

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
    if provider_name == "custom" {
        let parsed = reqwest::Url::parse(&base_url)
            .map_err(|error| ByokError::Config(format!("invalid custom base URL: {error}")))?;
        if insecure_non_loopback(parsed.as_str()) && !args.allow_insecure_http {
            return Err(ByokError::Config(
                "custom non-loopback HTTP requires explicit allow_insecure_http acknowledgement"
                    .into(),
            ));
        }
    }

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

    // Production setup mutations share the runtime manager's lifecycle gate.
    // Keep it through durable commit, publication, and rollback so concurrent
    // set/set or set/clear calls cannot restore stale bytes over a winner.
    let _reconfiguration = match &ctx.runtime_manager {
        Some(manager) => Some(manager.begin_reconfiguration().await),
        None => None,
    };

    // Persist a complete v2 loadout. Custom transports are always namespaced,
    // so a user slug can never shadow a shipped provider definition.
    let user_path = ctx.user_config_dir.join("parish.toml");
    let mut user = crate::config::load_user_config_v2(&user_path)
        .map_err(|e| ByokError::Config(e.to_string()))?;
    let old_user = user.clone();
    let route_provider = if provider_name == "custom" {
        let slug = "wizard-custom";
        user.inference.providers.insert(
            slug.into(),
            parish_config::CustomProviderDefinition {
                display_name: "Custom OpenAI-compatible endpoint".into(),
                default_endpoint: Some("default".into()),
                endpoints: std::collections::BTreeMap::from([(
                    "default".into(),
                    parish_config::CustomEndpointDefinition {
                        inference_base_url: base_url.clone(),
                        discovery_base_url: Some(base_url.clone()),
                        inference_adapter: parish_config::InferenceAdapter::OpenaiChatV1,
                        discovery_adapter: parish_config::DiscoveryAdapter::OpenaiModelsV1,
                        auth_adapter: if api_key.is_some() {
                            parish_config::AuthAdapter::Bearer
                        } else {
                            parish_config::AuthAdapter::None
                        },
                        default_reasoning_dialect: parish_config::ReasoningDialect::None,
                        allow_insecure_http: args.allow_insecure_http,
                        default_openai_generation_wire: Some(
                            parish_config::OpenAiChatGenerationWire {
                                output_limit_field: parish_config::OutputLimitField::MaxTokens,
                                structured_output: std::collections::BTreeSet::from([
                                    parish_config::StructuredOutputMode::PromptValidatedJson,
                                ]),
                            },
                        ),
                    },
                )]),
                models: Default::default(),
            },
        );
        "custom:wizard-custom".to_string()
    } else {
        provider_name.clone()
    };
    let account = provider_account(&route_provider);
    let old_secret = ctx.secrets.get(&account)?;
    let mut loadout = parish_config::LoadoutDefinition {
        default: parish_config::RoutePatch {
            provider: Some(route_provider.clone()),
            model: selected_model,
            inference_base_url: args.base_url.as_ref().map(|_| base_url.clone()),
            allow_unverified_model: Some(true),
            ..Default::default()
        },
        ..Default::default()
    };
    if provider_name == "ollama" {
        if let Some(existing) = user
            .inference
            .loadouts
            .get(parish_config::MANAGED_OLLAMA_LOADOUT)
            && existing.managed_by != Some(parish_config::ManagedLoadoutOwner::OllamaSetupV1)
        {
            return Err(ByokError::Config(format!(
                "reserved loadout {} already exists and is not owned by the Ollama setup workflow",
                parish_config::MANAGED_OLLAMA_LOADOUT
            )));
        }
        loadout.managed_by = Some(parish_config::ManagedLoadoutOwner::OllamaSetupV1);
    }
    for (category, old) in &args.category_overrides {
        let patch = parish_config::RoutePatch {
            provider: old.provider.clone(),
            model: old.model.clone(),
            inference_base_url: old.base_url.clone(),
            reasoning: old.thinking_level.map(legacy_reasoning),
            service_tier: old.service_tier.map(legacy_service_tier),
            max_output_tokens: old.max_output_tokens,
            allow_unverified_model: Some(true),
            ..Default::default()
        };
        match category.as_str() {
            "dialogue" => loadout.routes.dialogue = Some(patch),
            "simulation" => loadout.routes.simulation = Some(patch),
            "intent" => loadout.routes.intent = Some(patch),
            "reaction" => loadout.routes.reaction = Some(patch),
            other => {
                return Err(ByokError::Config(format!(
                    "unknown inference category {other:?}"
                )));
            }
        }
    }
    let loadout_name = if provider_name == "ollama" {
        parish_config::MANAGED_OLLAMA_LOADOUT
    } else {
        "byok"
    };
    user.inference.active_loadout = Some(loadout_name.into());
    user.inference.loadouts.insert(loadout_name.into(), loadout);
    if route_provider.starts_with("custom:") && api_key.is_some() {
        user.credential_bindings.insert(
            route_provider.clone(),
            parish_config::CredentialBinding { env: None },
        );
    }
    let overrides = crate::config::routing_overrides_from_env()
        .map_err(|e| ByokError::Config(e.to_string()))?;
    let patch_is_ephemeral = |patch: &parish_config::RoutePatch| {
        patch.provider.is_some()
            || patch.model.is_some()
            || patch.inference_base_url.is_some()
            || patch.endpoint.is_some()
    };
    let has_ephemeral_routing = overrides.active_loadout.is_some()
        || patch_is_ephemeral(&overrides.global_env)
        || overrides.category_env.values().any(patch_is_ephemeral);
    let durable = provider_name != "ollama" || !has_ephemeral_routing;
    // An explicit setup action may run under temporary benchmark/environment
    // routing. Publish the managed Ollama selection for this process, but do
    // not derive persistent user state from those ephemeral authorities.
    let runtime_overrides = if durable {
        overrides.clone()
    } else {
        parish_config::RoutingOverrideSet {
            active_loadout: Some(loadout_name.into()),
            ..Default::default()
        }
    };
    let project = crate::config::load_project_config_v2(ctx.project_config_path)
        .map_err(|e| ByokError::Config(e.to_string()))?;
    let epoch = if let Some(manager) = &ctx.runtime_manager {
        manager.next_epoch()
    } else {
        ctx.config
            .lock()
            .await
            .inference_configuration_epoch
            .saturating_add(1)
    };
    let availability = ctx
        .runtime_manager
        .as_ref()
        .map(|manager| manager.snapshot().availability.clone())
        .unwrap_or_default();
    // Build the complete transport set before mutating either durable store.
    let candidate = crate::inference_runtime_v2::build_inference_runtime_v2(
        epoch,
        &project,
        &user,
        &runtime_overrides,
        &availability,
        |slot| {
            if slot == route_provider {
                api_key.clone()
            } else {
                ctx.secrets.get(&provider_account(slot)).ok().flatten()
            }
        },
    )
    .map_err(|e| ByokError::Config(e.to_string()))?;

    let restore_secret = || match &old_secret {
        Some(secret) => ctx.secrets.set(&account, secret),
        None => ctx.secrets.delete(&account),
    };
    let onboarding_preexisted = onboarding_complete(ctx.user_config_dir);
    if durable {
        match (&typed_key, used_env_key) {
            (Some(key), _) => ctx.secrets.set(&account, key)?,
            (None, _) => ctx.secrets.delete(&account)?,
        }
        if let Err(error) = crate::config::save_user_config_v2(&user_path, &user) {
            return match restore_secret() {
                Ok(()) => Err(ByokError::Config(error.to_string())),
                Err(rollback) => Err(ByokError::Config(format!(
                    "{error}; secret rollback also failed: {rollback}"
                ))),
            };
        }
        if let Err(error) = mark_onboarding_complete(ctx.user_config_dir) {
            let config_rollback = crate::config::save_user_config_v2(&user_path, &old_user).err();
            let secret_rollback = restore_secret().err();
            return Err(ByokError::Config(format!(
                "{error}; config rollback: {}; secret rollback: {}",
                config_rollback.map_or_else(|| "ok".into(), |error| error.to_string()),
                secret_rollback.map_or_else(|| "ok".into(), |error| error.to_string()),
            )));
        }
    }
    let runtime = if let Some(manager) = &ctx.runtime_manager {
        match manager.publish_candidate(candidate) {
            Ok(runtime) => runtime,
            Err(error) => {
                let config_rollback = durable
                    .then(|| crate::config::save_user_config_v2(&user_path, &old_user).err())
                    .flatten();
                let secret_rollback = durable.then(restore_secret).and_then(Result::err);
                let marker_rollback = if !durable || onboarding_preexisted {
                    None
                } else {
                    std::fs::remove_file(ctx.user_config_dir.join(ONBOARDING_MARKER_FILENAME)).err()
                };
                return Err(ByokError::Config(format!(
                    "{error}; config rollback: {}; secret rollback: {}; onboarding rollback: {}",
                    config_rollback.map_or_else(|| "ok".into(), |error| error.to_string()),
                    secret_rollback.map_or_else(|| "ok".into(), |error| error.to_string()),
                    marker_rollback.map_or_else(|| "ok".into(), |error| error.to_string()),
                )));
            }
        }
    } else {
        Arc::new(candidate)
    };
    let client = runtime.clients.dialogue_client().0.clone();
    // Admission changes first: the replacement queue carries its immutable
    // resolved profiles, so every newly admitted request is wholly old or
    // wholly new. The legacy read-only GameConfig projection follows only
    // after the infallible worker publication.
    crate::game_loop::inference::rebuild_inference_worker_with_clients(
        runtime.clients.clone(),
        client.clone(),
        provider,
        ctx.inference_config,
        ctx.inference_log,
        ctx.inference_file_log,
        ctx.slots,
    )
    .await;
    {
        let mut cfg = ctx.config.lock().await;
        cfg.apply_resolved_inference_v2(&runtime.config);
    }
    if let Some(user_data) = ctx.catalog_user_data {
        crate::inference_runtime_v2::spawn_catalog_refresh_v2(
            Arc::clone(&runtime.config),
            parish_config::CatalogStore::for_user_data_dir(user_data),
            user_data.to_path_buf(),
        );
    }

    Ok(client)
}

/// Clears the setup-managed selection without destroying unrelated user
/// configuration. A complete keyless simulator generation is built first;
/// durable config, secret removal, and live publication are then committed as
/// one recoverable transition. Existing in-flight calls retain their old Arc,
/// but no new admission can use the cleared credential after publication.
pub async fn handle_clear_provider_config(ctx: ByokContext<'_>) -> Result<(), ByokError> {
    let _reconfiguration = match &ctx.runtime_manager {
        Some(manager) => Some(manager.begin_reconfiguration().await),
        None => None,
    };
    let user_path = ctx.user_config_dir.join("parish.toml");
    let mut user = crate::config::load_user_config_v2(&user_path)
        .map_err(|error| ByokError::Config(error.to_string()))?;
    let old_user = user.clone();
    let active_slot = ctx
        .runtime_manager
        .as_ref()
        .and_then(|manager| {
            manager
                .snapshot()
                .config
                .category_routes
                .get("dialogue")
                .map(|route| route.key.provider_id.clone())
        })
        .unwrap_or_else(|| {
            ctx.config
                .try_lock()
                .map(|config| config.provider_name.clone())
                .unwrap_or_else(|_| "simulator".into())
        });
    let account = provider_account(&active_slot);
    let old_secret = ctx.secrets.get(&account)?;

    if let Some(active) = user.inference.active_loadout.clone()
        && (active == "byok" || active == parish_config::MANAGED_OLLAMA_LOADOUT)
    {
        user.inference.loadouts.remove(&active);
    }
    if active_slot == "custom:wizard-custom" {
        user.inference.providers.remove("wizard-custom");
        user.credential_bindings.remove(&active_slot);
    }
    const RESET_LOADOUT: &str = "rundale-reset-simulator";
    user.inference.active_loadout = Some(RESET_LOADOUT.into());
    user.inference.loadouts.insert(
        RESET_LOADOUT.into(),
        parish_config::LoadoutDefinition {
            default: parish_config::RoutePatch {
                provider: Some("simulator".into()),
                model: Some("simulator".into()),
                allow_unverified_model: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let project = crate::config::load_project_config_v2(ctx.project_config_path)
        .map_err(|error| ByokError::Config(error.to_string()))?;
    let overrides = parish_config::RoutingOverrideSet {
        active_loadout: Some(RESET_LOADOUT.into()),
        ..Default::default()
    };
    let epoch = ctx.runtime_manager.as_ref().map_or_else(
        || {
            ctx.config
                .try_lock()
                .map(|config| config.inference_configuration_epoch.saturating_add(1))
                .unwrap_or(1)
        },
        |manager| manager.next_epoch(),
    );
    let availability = ctx
        .runtime_manager
        .as_ref()
        .map(|manager| manager.snapshot().availability.clone())
        .unwrap_or_default();
    let candidate = crate::inference_runtime_v2::build_inference_runtime_v2(
        epoch,
        &project,
        &user,
        &overrides,
        &availability,
        |slot| ctx.secrets.get(&provider_account(slot)).ok().flatten(),
    )
    .map_err(|error| ByokError::Config(error.to_string()))?;

    ctx.secrets.delete(&account)?;
    if let Err(error) = crate::config::save_user_config_v2(&user_path, &user) {
        let rollback = old_secret.as_deref().map_or_else(
            || ctx.secrets.delete(&account),
            |secret| ctx.secrets.set(&account, secret),
        );
        return Err(ByokError::Config(format!(
            "{error}; secret rollback: {}",
            rollback.map_or_else(|rollback| rollback.to_string(), |()| "ok".into())
        )));
    }
    let runtime = if let Some(manager) = &ctx.runtime_manager {
        match manager.publish_candidate(candidate) {
            Ok(runtime) => runtime,
            Err(error) => {
                let config_rollback = crate::config::save_user_config_v2(&user_path, &old_user);
                let secret_rollback = old_secret.as_deref().map_or_else(
                    || ctx.secrets.delete(&account),
                    |secret| ctx.secrets.set(&account, secret),
                );
                return Err(ByokError::Config(format!(
                    "{error}; config rollback: {}; secret rollback: {}",
                    config_rollback.map_or_else(|rollback| rollback.to_string(), |()| "ok".into()),
                    secret_rollback.map_or_else(|rollback| rollback.to_string(), |()| "ok".into()),
                )));
            }
        }
    } else {
        Arc::new(candidate)
    };
    let client = runtime.clients.dialogue_client().0.clone();
    crate::game_loop::inference::rebuild_inference_worker_with_clients(
        runtime.clients.clone(),
        client,
        Provider::from_str_loose("simulator").expect("compiled simulator provider exists"),
        ctx.inference_config,
        ctx.inference_log,
        ctx.inference_file_log,
        ctx.slots,
    )
    .await;
    {
        let mut cfg = ctx.config.lock().await;
        cfg.apply_resolved_inference_v2(&runtime.config);
    }
    if let Some(user_data) = ctx.catalog_user_data {
        crate::inference_runtime_v2::spawn_catalog_refresh_v2(
            Arc::clone(&runtime.config),
            parish_config::CatalogStore::for_user_data_dir(user_data),
            user_data.to_path_buf(),
        );
    }
    Ok(())
}

fn legacy_reasoning(level: crate::config::ThinkingLevel) -> parish_config::ReasoningIntent {
    let level = match level {
        crate::config::ThinkingLevel::Minimal => parish_config::ReasoningEffortV2::Minimal,
        crate::config::ThinkingLevel::Low => parish_config::ReasoningEffortV2::Low,
        crate::config::ThinkingLevel::Medium => parish_config::ReasoningEffortV2::Medium,
        crate::config::ThinkingLevel::High => parish_config::ReasoningEffortV2::High,
    };
    parish_config::ReasoningIntent::Effort { level }
}

fn legacy_service_tier(tier: crate::config::ServiceTier) -> parish_config::ServiceTierIntent {
    match tier {
        crate::config::ServiceTier::Standard => parish_config::ServiceTierIntent::Standard,
        crate::config::ServiceTier::Priority => parish_config::ServiceTierIntent::Priority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FeatureFlags;
    use crate::config::RateLimitConfig;
    use crate::secret_store::InMemorySecretStore;
    use serial_test::serial;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::task::JoinHandle;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn empty_game_config() -> GameConfig {
        GameConfig {
            inference_routes_v2: HashMap::new(),
            inference_subrole_routes_v2: HashMap::new(),
            inference_configuration_epoch: 0,
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

    fn fresh_runtime_manager() -> Arc<crate::inference_runtime_v2::InferenceRuntimeManagerV2> {
        let runtime = crate::inference_runtime_v2::build_inference_runtime_v2(
            1,
            &parish_config::ProjectConfigV2::default(),
            &parish_config::UserConfigV2::default(),
            &parish_config::RoutingOverrideSet::default(),
            &Default::default(),
            |_| None,
        )
        .unwrap();
        Arc::new(crate::inference_runtime_v2::InferenceRuntimeManagerV2::new(
            runtime,
        ))
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
    #[serial(byok_env)]
    async fn concurrent_set_set_is_one_serial_durable_epoch_transaction() {
        let dir = TempDir::new().unwrap();
        let project_path = dir.path().join("project.toml");
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let manager = fresh_runtime_manager();
        let log = Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));
        let invoke = |provider: &str, model: &str| {
            handle_set_provider_config(
                SetProviderConfigArgs {
                    provider: provider.into(),
                    base_url: None,
                    model: Some(model.into()),
                    api_key: None,
                    allow_insecure_http: false,
                    category_overrides: Default::default(),
                },
                ByokContext {
                    config: &cfg,
                    inference_config: &icfg,
                    inference_log: Arc::clone(&log),
                    inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
                    slots: slots.slots(),
                    secrets: Arc::clone(&secrets),
                    user_config_dir: dir.path(),
                    project_config_path: &project_path,
                    catalog_user_data: None,
                    runtime_manager: Some(Arc::clone(&manager)),
                },
            )
        };
        let (left, right) = tokio::join!(
            invoke("simulator", "simulator"),
            invoke("ollama", "llama3.2")
        );
        if let Err(error) = left {
            panic!("first concurrent setup failed: {error}");
        }
        if let Err(error) = right {
            panic!("second concurrent setup failed: {error}");
        }
        let user = crate::config::load_user_config_v2(&dir.path().join("parish.toml")).unwrap();
        let active = user.inference.active_loadout.as_ref().unwrap();
        let durable_provider = user.inference.loadouts[active]
            .default
            .provider
            .as_deref()
            .unwrap();
        let live = manager.snapshot();
        assert_eq!(live.config.configuration_epoch, 3);
        assert_eq!(
            live.config.category_routes["dialogue"].key.provider_id,
            durable_provider
        );
    }

    #[tokio::test]
    #[serial(byok_env)]
    async fn concurrent_set_clear_cannot_restore_stale_config_over_winner() {
        let dir = TempDir::new().unwrap();
        let project_path = dir.path().join("project.toml");
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let manager = fresh_runtime_manager();
        let log = Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));
        let context = || ByokContext {
            config: &cfg,
            inference_config: &icfg,
            inference_log: Arc::clone(&log),
            inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
            slots: slots.slots(),
            secrets: Arc::clone(&secrets),
            user_config_dir: dir.path(),
            project_config_path: &project_path,
            catalog_user_data: None,
            runtime_manager: Some(Arc::clone(&manager)),
        };
        let set = handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "ollama".into(),
                base_url: None,
                model: Some("llama3.2".into()),
                api_key: None,
                allow_insecure_http: false,
                category_overrides: Default::default(),
            },
            context(),
        );
        let (set, clear) = tokio::join!(set, handle_clear_provider_config(context()));
        if let Err(error) = set {
            panic!("concurrent setup failed: {error}");
        }
        if let Err(error) = clear {
            panic!("concurrent clear failed: {error}");
        }
        let user = crate::config::load_user_config_v2(&dir.path().join("parish.toml")).unwrap();
        let active = user.inference.active_loadout.as_ref().unwrap();
        let durable_provider = user.inference.loadouts[active]
            .default
            .provider
            .as_deref()
            .unwrap();
        let live = manager.snapshot();
        assert_eq!(live.config.configuration_epoch, 3);
        assert_eq!(
            live.config.category_routes["dialogue"].key.provider_id,
            durable_provider
        );
    }

    #[tokio::test]
    #[serial(byok_env)]
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
                allow_insecure_http: false,
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
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
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
            !body
                .lines()
                .any(|line| line.trim_start().starts_with("model =")),
            "the recommended model is a moving default, not a persisted pin: {body}"
        );
        assert!(!body.contains("api_key"));
        // Onboarding sentinel exists.
        assert!(dir.path().join(".onboarded").exists());
        // Worker installed.
        assert!(slots.queue.lock().await.is_some());
    }

    #[tokio::test]
    #[serial(byok_env)]
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
    #[serial(byok_env)]
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
                allow_insecure_http: false,
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
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
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
    #[serial(byok_env)]
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
                allow_insecure_http: false,
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
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
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
    #[serial(byok_env)]
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
                allow_insecure_http: false,
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
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
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
    #[serial(byok_env)]
    async fn published_custom_byok_route_refreshes_exact_new_epoch_catalog() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{"id": "fresh-byok-model"}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let manager = fresh_runtime_manager();
        let log = Arc::new(tokio::sync::Mutex::new(
            crate::inference::BoundedInferenceLog::new(16),
        ));

        handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "custom".into(),
                base_url: Some(server.uri()),
                model: Some("fresh-byok-model".into()),
                api_key: None,
                allow_insecure_http: true,
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: log,
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
                slots: slots.slots(),
                secrets,
                user_config_dir: dir.path(),
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: Some(dir.path()),
                runtime_manager: Some(Arc::clone(&manager)),
            },
        )
        .await
        .unwrap();

        assert_eq!(manager.snapshot().config.configuration_epoch, 2);
        let store = parish_config::CatalogStore::for_user_data_dir(dir.path());
        let mut documents = Vec::new();
        for _ in 0..50 {
            documents = store.cached_documents().unwrap();
            if !documents.is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert_eq!(documents.len(), 1);
        assert!(documents[0].routes.contains_key("fresh-byok-model"));
        assert_eq!(manager.snapshot().config.configuration_epoch, 2);
    }

    #[tokio::test]
    #[serial(byok_env)]
    async fn managed_ollama_under_env_routing_is_process_only() {
        unsafe { std::env::set_var("PARISH_LOADOUT", "temporary-benchmark") };
        let dir = TempDir::new().unwrap();
        crate::config::save_user_config_v2(
            &dir.path().join("parish.toml"),
            &parish_config::UserConfigV2::default(),
        )
        .unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(empty_game_config());
        let icfg = fresh_inference_config();
        let slots = Slots::new();
        let result = handle_set_provider_config(
            SetProviderConfigArgs {
                provider: "ollama".into(),
                base_url: None,
                model: Some("qwen3:32b".into()),
                api_key: None,
                allow_insecure_http: false,
                category_overrides: Default::default(),
            },
            ByokContext {
                config: &cfg,
                inference_config: &icfg,
                inference_log: Arc::new(tokio::sync::Mutex::new(
                    crate::inference::BoundedInferenceLog::new(16),
                )),
                inference_file_log: crate::inference::file_log::InferenceFileLog::disabled(),
                slots: slots.slots(),
                secrets,
                user_config_dir: dir.path(),
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
            },
        )
        .await;
        unsafe { std::env::remove_var("PARISH_LOADOUT") };
        result.expect("explicit setup should publish a process-local Ollama runtime");
        let persisted = crate::config::load_user_config_v2(&dir.path().join("parish.toml"))
            .expect("original user config remains valid");
        assert!(persisted.inference.active_loadout.is_none());
        assert!(persisted.inference.loadouts.is_empty());
        assert!(!dir.path().join(ONBOARDING_MARKER_FILENAME).exists());
        assert_eq!(cfg.lock().await.provider_name, "ollama");
    }

    #[tokio::test]
    #[serial(byok_env)]
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
                allow_insecure_http: false,
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
                project_config_path: &dir.path().join("project.toml"),
                catalog_user_data: None,
                runtime_manager: None,
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
    #[serial(byok_env)]
    async fn clear_provider_config_preserves_unrelated_config_and_publishes_simulator() {
        let dir = TempDir::new().unwrap();
        let secrets: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::new());
        let cfg = Mutex::new(GameConfig {
            provider_name: "anthropic".to_string(),
            api_key: Some("sk-ant".to_string()),
            ..empty_game_config()
        });
        secrets.set("provider:anthropic", "sk-ant").unwrap();
        let mut user = parish_config::UserConfigV2::default();
        user.inference.active_loadout = Some("byok".into());
        user.inference
            .loadouts
            .insert("byok".into(), parish_config::LoadoutDefinition::default());
        user.inference.loadouts.insert(
            "keep-me".into(),
            parish_config::LoadoutDefinition {
                default: parish_config::RoutePatch {
                    provider: Some("simulator".into()),
                    model: Some("simulator".into()),
                    allow_unverified_model: Some(true),
                    ..Default::default()
                },
                ..Default::default()
            },
        );
        crate::config::save_user_config_v2(&dir.path().join("parish.toml"), &user).unwrap();

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
            project_config_path: &dir.path().join("project.toml"),
            catalog_user_data: None,
            runtime_manager: None,
        })
        .await
        .unwrap();

        assert!(secrets.get("provider:anthropic").unwrap().is_none());
        let persisted = crate::config::load_user_config_v2(&dir.path().join("parish.toml"))
            .expect("clear leaves a valid schema-v2 document");
        assert_eq!(
            persisted.inference.active_loadout.as_deref(),
            Some("rundale-reset-simulator")
        );
        assert!(!persisted.inference.loadouts.contains_key("byok"));
        assert!(persisted.inference.loadouts.contains_key("keep-me"));
        assert!(cfg.lock().await.api_key.is_none());
        assert_eq!(cfg.lock().await.provider_name, "simulator");
    }

    #[tokio::test]
    #[serial(byok_env)]
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
