//! Shared mutable runtime configuration for provider, model, and cloud settings.
//!
//! [`GameConfig`] is the single source of truth for LLM provider configuration
//! at runtime. It is used by the Tauri desktop backend, the axum web server,
//! and the headless CLI — eliminating the duplicate `GameConfig` structs that
//! previously lived in each backend.

use std::collections::HashMap;

use crate::config::{FeatureFlags, InferenceCategory, RateLimitConfig};

const DEFAULT_AUTO_PAUSE_SECS: u64 = 300;
use crate::inference::InferenceRateLimiter;

/// Canonical identifiers for the bundled local two-slot Apple Silicon loadout
/// (Qwen-14B-4bit on `:8000` + Qwen-1.5B-4bit on `:8001`).
///
/// Single source of truth shared by the Tauri setup wizard and
/// `parish-server --headless-models` (#1364) so the two entry points can't
/// drift on model ids or ports (rule #12). The port/model strings mirror the
/// values `parish-tauri`'s wizard writes and the bundled `vllm-mlx` serves.
pub mod local_models {
    /// Base provider id for the local Apple Silicon runtime.
    pub const PROVIDER: &str = "vllm-mlx";
    /// In-process simulator provider id for categories the 1.5B can't serve.
    pub const SIMULATOR_PROVIDER: &str = "simulator";
    /// Big slot: 14B model, used for Dialogue.
    pub const DIALOGUE_MODEL: &str = "mlx-community/Qwen2.5-14B-Instruct-4bit";
    /// Big-slot base URL (`:8000`).
    pub const DIALOGUE_BASE_URL: &str = "http://localhost:8000";
    /// Small slot: 1.5B model, used for Intent.
    pub const INTENT_MODEL: &str = "mlx-community/Qwen2.5-1.5B-Instruct-4bit";
    /// Small-slot base URL (`:8001`).
    pub const INTENT_BASE_URL: &str = "http://localhost:8001";
}

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
    /// Per-category provider name overrides (absent key = inherits base).
    pub category_provider: HashMap<InferenceCategory, String>,
    /// Per-category model name overrides (absent key = inherits base).
    pub category_model: HashMap<InferenceCategory, String>,
    /// Per-category API key overrides (absent key = inherits base).
    pub category_api_key: HashMap<InferenceCategory, String>,
    /// Per-category base URL overrides (absent key = inherits base).
    pub category_base_url: HashMap<InferenceCategory, String>,
    /// Base inference tuning overrides inherited by every role.
    pub inference_profile_override: parish_config::InferenceProfileOverride,
    /// Per-category inference tuning overrides layered over the base values.
    pub category_inference_profile:
        HashMap<InferenceCategory, parish_config::InferenceProfileOverride>,
    /// Runtime feature flags for safe deployment of in-progress features.
    pub flags: FeatureFlags,
    /// Per-category outbound rate limiters, pre-built from the
    /// engine `[inference.rate_limits]` config.
    ///
    /// Attached automatically by [`Self::resolve_category_client`] when
    /// constructing per-category override clients. Categories that fall
    /// back to the base client inherit whatever limiter the base client
    /// was constructed with (see [`crate::inference::openai_client::OpenAiClient::with_rate_limit`]).
    pub category_rate_limit: HashMap<InferenceCategory, InferenceRateLimiter>,
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
    /// Model chosen and pulled by Ollama auto-setup.
    ///
    /// `None` for non-Ollama providers, or before auto-setup has run.
    /// Used by `Command::ApplyPreset` to keep `/preset ollama` aligned
    /// with the model that was actually downloaded — the static qwen3
    /// preset list assumes models the user has not pulled.
    pub auto_setup_model: Option<String>,
}

/// Supplies the per-category override view that `parish-diagnostics` needs to
/// build the inference-debug table, without that crate depending on
/// `parish-core` (which would be a dependency cycle). See
/// [`parish_diagnostics::debug_snapshot::InferenceCategoryConfig`].
impl parish_diagnostics::debug_snapshot::InferenceCategoryConfig for GameConfig {
    fn category_provider(&self, cat: InferenceCategory) -> Option<String> {
        self.category_provider.get(&cat).cloned()
    }
    fn category_model(&self, cat: InferenceCategory) -> Option<String> {
        self.category_model.get(&cat).cloned()
    }
    fn category_base_url(&self, cat: InferenceCategory) -> Option<String> {
        self.category_base_url.get(&cat).cloned()
    }
    fn subrole_profile(
        &self,
        subrole: parish_config::InferenceSubrole,
    ) -> parish_config::InferenceProfile {
        self.inference_profile(subrole)
    }
}

impl GameConfig {
    /// Resolves the effective inference profile for a concrete workload.
    pub fn inference_profile(
        &self,
        subrole: parish_config::InferenceSubrole,
    ) -> parish_config::InferenceProfile {
        let category = subrole.category();
        let checked = parish_config::InferenceProfile::for_subrole(subrole);
        let base = self.inference_profile_override.apply(checked, subrole);
        self.category_inference_profile
            .get(&category)
            .copied()
            .unwrap_or_default()
            .apply(base, subrole)
    }

    /// Resolves the client and model for a given inference category.
    ///
    /// If the category has any per-category override — provider, model, URL,
    /// or API key — builds a fresh [`AnyClient`] from those settings and
    /// attaches the per-category rate limiter (if configured). Otherwise
    /// falls back to the supplied `base_client`, which already carries its
    /// own rate limiter from setup. The model falls back to `self.model_name`
    /// if no per-category model is set.
    ///
    /// `category_model` participates in the override check because a
    /// model-only override routes a divergent model through the base URL —
    /// e.g. on the vllm-mlx two-slot loadout, the 1.5B reaction model is not
    /// loaded on the 14B `:8000` dialogue slot. Sending it there yields a
    /// `404 Not Found` from vLLM (#993).
    ///
    /// The per-category provider (from `category_provider[cat]`, falling back
    /// to `provider_name`) determines which transport is built: OpenAI-compat
    /// for most providers, the native [`AnthropicClient`] for `anthropic`.
    ///
    /// When `category_base_url[cat]` is empty, the URL fallback chain prefers
    /// the resolved provider's `preset_base_url(cat)` over the base URL.
    /// This makes routing robust against a transient state where
    /// `fill_missing_models_from_presets` has populated `category_model` but
    /// has not yet (or has been blocked from) populating `category_base_url`.
    /// For single-slot providers whose preset omits `[presets.base_urls]`,
    /// `preset_base_url` returns `None` and the chain falls through to the
    /// base-URL fallback already in place — no behaviour change.
    ///
    /// Returns `None` if no client is available (base is `None` and no
    /// category override is configured).
    pub fn resolve_category_client(
        &self,
        cat: InferenceCategory,
        base_client: Option<&crate::inference::AnyClient>,
    ) -> (Option<crate::inference::AnyClient>, String) {
        use parish_config::Provider;
        let model = self
            .category_model
            .get(&cat)
            .cloned()
            .unwrap_or_else(|| self.model_name.clone());

        // Any per-category divergence triggers a fresh client. `category_model`
        // is part of the check because routing a category model through the
        // base URL silently 404s on multi-slot loadouts (#993).
        let has_override = self.category_provider.contains_key(&cat)
            || self.category_model.contains_key(&cat)
            || self.category_base_url.contains_key(&cat)
            || self.category_api_key.contains_key(&cat);

        let client = if has_override {
            // Resolve the effective provider for this category.
            let provider_str = self
                .category_provider
                .get(&cat)
                .map(String::as_str)
                .unwrap_or(&self.provider_name);
            let provider = Provider::from_str_loose(provider_str).unwrap_or_default();

            // URL fallback chain:
            //   1. explicit per-category URL (`category_base_url[cat]`)
            //   2. provider's preset URL for this category — multi-slot
            //      loadouts (vllm-mlx 14B :8000 + 1.5B :8001) need this so
            //      the reaction/intent calls land on the slot where their
            //      model is loaded, even if `fill_missing_models_from_presets`
            //      hasn't populated `category_base_url` yet (#993).
            //   3. user's base URL (matches single-slot providers).
            //   4. provider default URL (catches an Anthropic-override-on-
            //      Ollama-base setup whose base URL is the Ollama localhost).
            let url = if let Some(u) = self.category_base_url.get(&cat) {
                u.clone()
            } else if let Some(u) = provider.preset_base_url(cat) {
                u.to_string()
            } else if !self.base_url.is_empty() {
                self.base_url.clone()
            } else {
                provider.default_base_url().to_string()
            };
            let key = self
                .category_api_key
                .get(&cat)
                .map(String::as_str)
                .or(self.api_key.as_deref());

            let inference_cfg = parish_config::InferenceConfig::default();
            let built = crate::inference::build_client(&provider, &url, key, &inference_cfg);
            // Attach the per-category rate limiter to the inner variant
            // (rate-limiting is per-transport, not at the AnyClient layer).
            let limiter = self.category_rate_limit.get(&cat).cloned();
            Some(attach_rate_limit(built, limiter))
        } else {
            base_client.cloned()
        };

        (client, model)
    }

    /// Collects extra vllm-mlx slots beyond the base provider's slot.
    ///
    /// Walks per-category overrides. For every category whose effective
    /// provider is `vllm-mlx` AND whose `(base_url, model)` differs from
    /// the base slot, emits a [`VllmMlxSlot`]. Used by
    /// `setup_provider_client` to auto-spawn one vllm-mlx process per
    /// unique slot for the two-slot Apple Silicon loadout.
    ///
    /// Returns an empty `Vec` when no per-category overrides resolve to
    /// vllm-mlx slots that differ from the base. Deduplication of the
    /// returned slots is handled downstream in
    /// [`crate::inference::client::VllmMlxProcess::ensure_slots`].
    pub fn vllm_mlx_extra_slots(&self) -> Vec<crate::inference::client::VllmMlxSlot> {
        use crate::config::Provider;
        let base_provider_is_vllm_mlx = Provider::from_str_loose(&self.provider_name)
            .map(|p| p.id() == "vllmmlx")
            .unwrap_or(false);
        let base_slot = (self.base_url.clone(), self.model_name.clone());

        let mut out = Vec::new();
        for cat in InferenceCategory::ALL {
            let effective_provider_str = self
                .category_provider
                .get(&cat)
                .map(String::as_str)
                .unwrap_or(&self.provider_name);
            let effective_provider =
                Provider::from_str_loose(effective_provider_str).unwrap_or_default();
            if effective_provider.id() != "vllmmlx" {
                continue;
            }
            let url = self
                .category_base_url
                .get(&cat)
                .cloned()
                .unwrap_or_else(|| self.base_url.clone());
            let model = self
                .category_model
                .get(&cat)
                .cloned()
                .unwrap_or_else(|| self.model_name.clone());

            // Skip the base slot — it's auto-spawned by setup_provider_client
            // for Provider::VllmMlx bases. For non-VllmMlx bases (e.g. an
            // Ollama base routing only Intent to vllm-mlx), the base slot
            // is irrelevant and we include all VllmMlx category slots.
            if base_provider_is_vllm_mlx && (url.clone(), model.clone()) == base_slot {
                continue;
            }
            out.push(crate::inference::client::VllmMlxSlot {
                base_url: url,
                model,
            });
        }
        out
    }

    /// Collects extra vllm slots beyond the base provider's slot.
    ///
    /// Parallel to [`Self::vllm_mlx_extra_slots`] for the Linux/Windows
    /// CUDA/ROCm vllm runtime. Used by `setup_provider_client` to auto-spawn
    /// one vllm process per unique slot for the two-slot Linux/Windows loadout.
    pub fn vllm_extra_slots(&self) -> Vec<crate::inference::client::VllmSlot> {
        use crate::config::Provider;
        let base_provider_is_vllm = Provider::from_str_loose(&self.provider_name)
            .map(|p| p.id() == "vllm")
            .unwrap_or(false);
        let base_slot = (self.base_url.clone(), self.model_name.clone());

        let mut out = Vec::new();
        for cat in InferenceCategory::ALL {
            let effective_provider_str = self
                .category_provider
                .get(&cat)
                .map(String::as_str)
                .unwrap_or(&self.provider_name);
            let effective_provider =
                Provider::from_str_loose(effective_provider_str).unwrap_or_default();
            if effective_provider.id() != "vllm" {
                continue;
            }
            let url = self
                .category_base_url
                .get(&cat)
                .cloned()
                .unwrap_or_else(|| self.base_url.clone());
            let model = self
                .category_model
                .get(&cat)
                .cloned()
                .unwrap_or_else(|| self.model_name.clone());

            if base_provider_is_vllm && (url.clone(), model.clone()) == base_slot {
                continue;
            }
            out.push(crate::inference::client::VllmSlot {
                base_url: url,
                model,
            });
        }
        out
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
        use crate::config::Provider;
        use std::collections::HashMap;

        type TransportKey = (String, String, String);
        type SharedQuota = (u32, u32, Vec<InferenceCategory>);

        // Google and other cloud quotas apply to the credential/endpoint, not
        // independently to gameplay roles. Build one bucket per effective
        // transport and clone it into every matching category. If category
        // limits disagree, the shared bucket deliberately uses the strictest
        // values so aggregate traffic can never exceed any declared quota.
        let mut groups: HashMap<TransportKey, SharedQuota> = HashMap::new();
        for cat in InferenceCategory::ALL {
            let Some(limit) = cfg.for_category(cat).or(cfg.default) else {
                continue;
            };
            if limit.per_minute == 0 {
                continue;
            }
            let provider_name = self
                .category_provider
                .get(&cat)
                .map(String::as_str)
                .unwrap_or(&self.provider_name);
            let provider = Provider::from_str_loose(provider_name).unwrap_or_default();
            let url = self
                .category_base_url
                .get(&cat)
                .cloned()
                .or_else(|| provider.preset_base_url(cat).map(str::to_string))
                .unwrap_or_else(|| {
                    if self.base_url.is_empty() {
                        provider.default_base_url().to_string()
                    } else {
                        self.base_url.clone()
                    }
                });
            let key = self
                .category_api_key
                .get(&cat)
                .cloned()
                .or_else(|| self.api_key.clone())
                .unwrap_or_default();
            groups
                .entry((provider.id().to_string(), url, key))
                .and_modify(|(per_minute, burst, categories)| {
                    *per_minute = (*per_minute).min(limit.per_minute);
                    *burst = (*burst).min(limit.burst.max(1));
                    categories.push(cat);
                })
                .or_insert((limit.per_minute, limit.burst.max(1), vec![cat]));
        }

        self.category_rate_limit.clear();
        for (_, (per_minute, burst, categories)) in groups {
            if let Some(limiter) = InferenceRateLimiter::new(per_minute, burst) {
                for category in categories {
                    self.category_rate_limit.insert(category, limiter.clone());
                }
            }
        }
    }

    /// Fills in any unset model fields with the appropriate provider preset.
    ///
    /// - The base [`Self::model_name`] is filled from
    ///   `provider.preset_model(InferenceCategory::Dialogue)` if the base
    ///   model name is empty.
    /// - Each [`Self::category_model`] entry that is absent is filled from
    ///   the *effective* provider's preset for that role — the effective
    ///   provider is the per-category override (`category_provider[cat]`)
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
    /// Pins a single model into the base slot and all four per-category slots.
    ///
    /// Called from two paths:
    /// - Tauri / web bootstrap, after Ollama auto-setup pulls a hardware-
    ///   matched model. The single downloaded model is the model every
    ///   category requests, regardless of the static `Provider::Ollama`
    ///   preset (which lists qwen3 models the user has not downloaded).
    /// - `Command::ApplyPreset(Ollama)`, when `auto_setup_model` is `Some`,
    ///   to re-pin the auto-setup model after a manual `/preset ollama`.
    ///
    /// Records the model in `auto_setup_model` so a later `/preset ollama`
    /// can re-pin the same value instead of writing the static preset.
    /// Overwrites any existing entries in `category_model`.
    pub fn pin_setup_model(&mut self, model: String) {
        self.model_name = model.clone();
        for cat in InferenceCategory::ALL {
            self.category_model.insert(cat, model.clone());
        }
        self.auto_setup_model = Some(model);
    }

    /// Applies the per-category overrides from `parish.toml`
    /// ([`parish_config::user_config::UserConfig::category_overrides`]) into
    /// `self.category_provider/model/base_url`.
    ///
    /// Unknown category names are skipped silently (forwards-compat with new
    /// roles). `Option` fields are only written when `Some` — `None` means
    /// "inherit base", which is encoded by absent map key.
    pub fn apply_user_category_overrides(
        &mut self,
        overrides: &std::collections::BTreeMap<
            String,
            parish_config::user_config::CategoryOverride,
        >,
    ) {
        for (cat_name, ov) in overrides {
            let Some(cat) = InferenceCategory::from_name(cat_name) else {
                continue;
            };
            if let Some(p) = ov.provider.clone() {
                self.category_provider.insert(cat, p);
            }
            if let Some(m) = ov.model.clone() {
                self.category_model.insert(cat, m);
            }
            if let Some(u) = ov.base_url.clone() {
                self.category_base_url.insert(cat, u);
            }
        }
    }

    /// Applies fully resolved per-category provider configurations.
    ///
    /// Runtimes resolve their supported input layers (environment, TOML, or
    /// CLI) in `parish-config`, then use this one backend-agnostic seam to
    /// populate the routing maps consumed by every inference call.
    pub fn apply_resolved_category_configs(
        &mut self,
        configs: &std::collections::HashMap<InferenceCategory, parish_config::CategoryConfig>,
    ) {
        for (category, resolved) in configs {
            self.category_provider
                .insert(*category, resolved.provider.id().to_string());
            self.category_base_url
                .insert(*category, resolved.base_url.clone());
            if let Some(model) = resolved.model.clone() {
                self.category_model.insert(*category, model);
            } else {
                self.category_model.remove(category);
            }
            if let Some(api_key) = resolved.api_key.clone() {
                self.category_api_key.insert(*category, api_key);
            } else {
                self.category_api_key.remove(category);
            }
        }
    }

    /// Applies persisted top-level and per-category inference tuning.
    pub fn apply_user_inference_profiles(&mut self, user: &parish_config::user_config::UserConfig) {
        self.inference_profile_override = parish_config::InferenceProfileOverride {
            thinking_level: user.thinking_level,
            max_output_tokens: user.max_output_tokens,
            service_tier: user.service_tier,
            ..Default::default()
        };
        self.category_inference_profile.clear();
        for (name, value) in &user.category_overrides {
            let Some(category) = InferenceCategory::from_name(name) else {
                tracing::warn!(category = %name, "ignoring unknown inference category override");
                continue;
            };
            self.category_inference_profile.insert(
                category,
                parish_config::InferenceProfileOverride {
                    thinking_level: value.thinking_level,
                    max_output_tokens: value.max_output_tokens,
                    service_tier: value.service_tier,
                    tier2_max_output_tokens: value.tier2_max_output_tokens,
                    tier3_max_output_tokens: value.tier3_max_output_tokens,
                },
            );
        }
    }

    /// Configures this `GameConfig` for the bundled local two-slot Apple
    /// Silicon loadout used by the Tauri wizard's `two-slot` path and by
    /// `parish-server --headless-models` (#1364):
    ///
    /// - base provider `vllm-mlx`, Qwen-14B-4bit on `:8000` (Dialogue),
    /// - Intent on the 1.5B slot `:8001`,
    /// - Simulation + Reaction on the in-process simulator (the 1.5B can't
    ///   hold the strict Tier-2/Tier-3 JSON; the simulator returns valid
    ///   shapes so the living world stays quiet and the slots stay free for
    ///   dialogue).
    ///
    /// This is the single backend-agnostic definition of the loadout (rule
    /// #12): the headless server and the desktop wizard both route through it
    /// instead of hand-rolling the same per-category override map. The actual
    /// process bring-up is handled downstream by
    /// [`crate::inference::setup::setup_provider_client`], which detect-reuses
    /// any vllm-mlx server already listening on `:8000` / `:8001` (a running
    /// Tauri app) rather than double-spawning.
    pub fn apply_local_qwen_two_slot(&mut self) {
        self.provider_name = local_models::PROVIDER.to_string();
        self.base_url = local_models::DIALOGUE_BASE_URL.to_string();
        self.api_key = None;
        self.model_name = local_models::DIALOGUE_MODEL.to_string();

        // Dialogue inherits the base slot (14B @ :8000); set it explicitly so
        // the debug snapshot shows the binding rather than "(inherits base)".
        self.category_provider
            .insert(InferenceCategory::Dialogue, local_models::PROVIDER.into());
        self.category_base_url.insert(
            InferenceCategory::Dialogue,
            local_models::DIALOGUE_BASE_URL.into(),
        );
        self.category_model.insert(
            InferenceCategory::Dialogue,
            local_models::DIALOGUE_MODEL.into(),
        );

        // Intent → 1.5B @ :8001.
        self.category_provider
            .insert(InferenceCategory::Intent, local_models::PROVIDER.into());
        self.category_base_url.insert(
            InferenceCategory::Intent,
            local_models::INTENT_BASE_URL.into(),
        );
        self.category_model
            .insert(InferenceCategory::Intent, local_models::INTENT_MODEL.into());

        // Simulation + Reaction → in-process simulator.
        for cat in [InferenceCategory::Simulation, InferenceCategory::Reaction] {
            self.category_provider
                .insert(cat, local_models::SIMULATOR_PROVIDER.into());
            self.category_base_url.remove(&cat);
            self.category_model.remove(&cat);
        }
    }

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

        // Per-category models + base URLs: fall back to each effective
        // provider's preset for that specific role.
        //
        // Correctness rules:
        //
        // 1. Fill model and URL independently — both are vacant-only writes.
        //    A user who set only one of `category_model[X]` /
        //    `category_base_url[X]` keeps that value; we only fill the
        //    *other* unset half. Filling both together (the previous
        //    `filled_model` gate) left a user-supplied model stranded on the
        //    base URL with no preset URL companion, which 404s on multi-slot
        //    loadouts (#993).
        //
        // 2. Don't override a user-set base URL with a preset URL. If the
        //    user has pointed `self.base_url` at a non-canonical host
        //    (e.g. `http://remote-gpu:8000` for a colocated box), skip the
        //    preset's hardcoded `http://localhost:PORT` so we don't silently
        //    reroute traffic to a host the user wasn't using. The user has
        //    stepped off the canonical path and owns category routing.
        //
        // base_url is critical for multi-slot loadouts (vllm-mlx 14B on
        // :8000 + 1.5B on :8001) — without it, the auto-filled model lands
        // on the wrong slot and every request 404s.
        for cat in InferenceCategory::ALL {
            let provider_str = self
                .category_provider
                .get(&cat)
                .map(String::as_str)
                .unwrap_or(&self.provider_name);
            let Ok(p) = Provider::from_str_loose(provider_str) else {
                continue;
            };
            // entry().or_insert_with-style flow keeps clippy's map_entry
            // lint happy.
            use std::collections::hash_map::Entry;
            if let Entry::Vacant(slot) = self.category_model.entry(cat)
                && let Some(m) = p.preset_model(cat)
            {
                slot.insert(m.to_string());
                changed = true;
            }
            if self.base_url == p.default_base_url()
                && let Entry::Vacant(slot) = self.category_base_url.entry(cat)
                && let Some(u) = p.preset_base_url(cat)
            {
                slot.insert(u.to_string());
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
        (AnyClient::Google(c), lim) => AnyClient::Google(c.maybe_with_rate_limit(lim)),
        // Simulator and mock have no network calls and ignore rate limiting.
        (c @ (AnyClient::Simulator(_) | AnyClient::Mock(_)), _) => c,
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
            idle_banter_after_secs: 120,
            auto_pause_after_secs: DEFAULT_AUTO_PAUSE_SECS,
            category_provider: HashMap::new(),
            category_model: HashMap::new(),
            category_api_key: HashMap::new(),
            category_base_url: HashMap::new(),
            inference_profile_override: parish_config::InferenceProfileOverride::default(),
            category_inference_profile: HashMap::new(),
            flags: FeatureFlags::default(),
            category_rate_limit: HashMap::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_inference_profiles_resolve_top_level_category_and_tier_caps() {
        let mut cfg = GameConfig::default();
        let user = parish_config::user_config::UserConfig {
            thinking_level: Some(parish_config::ThinkingLevel::Low),
            max_output_tokens: Some(3_000),
            service_tier: Some(parish_config::ServiceTier::Standard),
            category_overrides: std::collections::BTreeMap::from([(
                "simulation".to_string(),
                parish_config::user_config::CategoryOverride {
                    thinking_level: Some(parish_config::ThinkingLevel::High),
                    tier2_max_output_tokens: Some(1_500),
                    tier3_max_output_tokens: Some(5_000),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        cfg.apply_user_inference_profiles(&user);

        let dialogue = cfg.inference_profile(parish_config::InferenceSubrole::Dialogue);
        assert_eq!(dialogue.thinking_level, parish_config::ThinkingLevel::Low);
        assert_eq!(dialogue.max_output_tokens, 3_000);
        let tier2 = cfg.inference_profile(parish_config::InferenceSubrole::Tier2Simulation);
        assert_eq!(tier2.thinking_level, parish_config::ThinkingLevel::High);
        assert_eq!(tier2.max_output_tokens, 1_500);
        let tier3 = cfg.inference_profile(parish_config::InferenceSubrole::Tier3Simulation);
        assert_eq!(tier3.max_output_tokens, 5_000);
    }

    #[test]
    fn debug_profiles_cover_every_concrete_workload() {
        let cfg = GameConfig::default();
        let rows = parish_diagnostics::debug_snapshot::build_inference_categories(&cfg);
        assert_eq!(rows.len(), parish_config::InferenceSubrole::ALL.len());
        assert_eq!(rows[0].role, "dialogue");
        assert_eq!(rows[2].role, "arrival-reaction");
        assert_eq!(rows[5].role, "tier2-simulation");
        assert_eq!(rows[5].max_output_tokens, 2_048);
        assert_eq!(rows[6].role, "tier3-simulation");
        assert_eq!(rows[6].max_output_tokens, 4_096);
        assert_eq!(rows[7].role, "demo-player");
        assert_eq!(
            rows[7].thinking_level,
            parish_config::ThinkingLevel::Minimal
        );
    }

    #[test]
    fn apply_local_qwen_two_slot_routes_categories() {
        let mut c = GameConfig::default();
        c.apply_local_qwen_two_slot();

        assert_eq!(c.provider_name, local_models::PROVIDER);
        assert_eq!(c.base_url, local_models::DIALOGUE_BASE_URL);
        assert_eq!(c.model_name, local_models::DIALOGUE_MODEL);
        assert!(c.api_key.is_none(), "no key for a local provider");

        // Dialogue → 14B @ :8000.
        assert_eq!(
            c.category_base_url
                .get(&InferenceCategory::Dialogue)
                .map(String::as_str),
            Some(local_models::DIALOGUE_BASE_URL)
        );
        // Intent → 1.5B @ :8001.
        assert_eq!(
            c.category_model
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some(local_models::INTENT_MODEL)
        );
        assert_eq!(
            c.category_base_url
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some(local_models::INTENT_BASE_URL)
        );
        // Simulation + Reaction → simulator (no URL/model so they don't spawn
        // a vllm-mlx slot).
        for cat in [InferenceCategory::Simulation, InferenceCategory::Reaction] {
            assert_eq!(
                c.category_provider.get(&cat).map(String::as_str),
                Some(local_models::SIMULATOR_PROVIDER)
            );
            assert!(!c.category_base_url.contains_key(&cat));
            assert!(!c.category_model.contains_key(&cat));
        }

        // Exactly one extra vllm-mlx slot (the 1.5B :8001); the base 14B :8000
        // slot is auto-spawned by setup_provider_client, simulator spawns none.
        let extra = c.vllm_mlx_extra_slots();
        assert_eq!(extra.len(), 1);
        assert_eq!(extra[0].base_url, local_models::INTENT_BASE_URL);
        assert_eq!(extra[0].model, local_models::INTENT_MODEL);
    }

    #[test]
    fn default_config() {
        let c = GameConfig::default();
        assert_eq!(c.provider_name, "ollama");
        assert!(!c.improv_enabled);
        assert!(c.api_key.is_none());
        assert_eq!(c.max_follow_up_turns, 2);
        assert_eq!(c.idle_banter_after_secs, 120);
        assert_eq!(c.auto_pause_after_secs, DEFAULT_AUTO_PAUSE_SECS);
        assert!(c.active_tile_source.is_empty());
        assert!(c.tile_sources.is_empty());
        assert!(!c.reveal_unexplored_locations);
        assert!(c.auto_setup_model.is_none());
    }

    #[test]
    fn pin_setup_model_writes_all_four_category_slots() {
        let mut cfg = GameConfig::default();
        cfg.pin_setup_model("gemma4:e2b".to_string());
        assert_eq!(cfg.model_name, "gemma4:e2b");
        for cat in InferenceCategory::ALL {
            assert_eq!(
                cfg.category_model.get(&cat).map(String::as_str),
                Some("gemma4:e2b"),
                "category {:?} should be pinned",
                cat
            );
        }
        assert_eq!(cfg.auto_setup_model.as_deref(), Some("gemma4:e2b"));
    }

    #[test]
    fn pin_setup_model_overwrites_existing_slots() {
        let mut cfg = GameConfig::default();
        cfg.category_model
            .insert(InferenceCategory::Dialogue, "qwen3:32b".to_string());
        cfg.category_model
            .insert(InferenceCategory::Intent, "qwen3:4b".to_string());
        cfg.pin_setup_model("gemma4:e4b".to_string());
        for cat in InferenceCategory::ALL {
            assert_eq!(
                cfg.category_model.get(&cat).map(String::as_str),
                Some("gemma4:e4b")
            );
        }
    }

    #[test]
    fn apply_resolved_category_configs_populates_and_clears_optional_fields() {
        let mut config = GameConfig::default();
        config
            .category_model
            .insert(InferenceCategory::Reaction, "stale-model".into());
        config
            .category_api_key
            .insert(InferenceCategory::Reaction, "stale-key".into());
        let resolved = std::collections::HashMap::from([(
            InferenceCategory::Reaction,
            parish_config::CategoryConfig {
                provider: parish_config::Provider::simulator(),
                base_url: String::new(),
                api_key: None,
                model: None,
            },
        )]);

        config.apply_resolved_category_configs(&resolved);

        assert_eq!(
            config
                .category_provider
                .get(&InferenceCategory::Reaction)
                .map(String::as_str),
            Some("simulator")
        );
        assert_eq!(
            config
                .category_base_url
                .get(&InferenceCategory::Reaction)
                .map(String::as_str),
            Some("")
        );
        assert!(
            !config
                .category_model
                .contains_key(&InferenceCategory::Reaction)
        );
        assert!(
            !config
                .category_api_key
                .contains_key(&InferenceCategory::Reaction)
        );
    }

    #[test]
    fn fill_missing_models_from_presets_after_pin_is_noop() {
        let mut cfg = GameConfig {
            provider_name: "ollama".to_string(),
            ..GameConfig::default()
        };
        cfg.pin_setup_model("gemma4:e2b".to_string());
        let snapshot: Vec<_> = InferenceCategory::ALL
            .iter()
            .map(|c| cfg.category_model.get(c).cloned())
            .collect();
        let changed = cfg.fill_missing_models_from_presets();
        assert!(!changed, "fill should be a no-op after pin_setup_model");
        for (cat, before) in InferenceCategory::ALL.iter().zip(snapshot.iter()) {
            assert_eq!(cfg.category_model.get(cat).cloned(), before.clone());
        }
        assert_eq!(cfg.model_name, "gemma4:e2b");
    }

    #[test]
    fn resolve_category_client_returns_pinned_model() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let mut cfg = GameConfig {
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        cfg.pin_setup_model("gemma4:e2b".to_string());
        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        for cat in InferenceCategory::ALL {
            let (_, model) = cfg.resolve_category_client(cat, Some(&base));
            assert_eq!(model, "gemma4:e2b", "category {:?}", cat);
        }
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
        cfg.category_model
            .insert(InferenceCategory::Reaction, "reaction-model".to_string());
        cfg.category_base_url.insert(
            InferenceCategory::Reaction,
            "https://openrouter.ai/api".to_string(),
        );
        cfg.category_api_key
            .insert(InferenceCategory::Reaction, "sk-test".to_string());

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
        cfg.category_provider
            .insert(InferenceCategory::Reaction, "anthropic".to_string());
        cfg.category_api_key
            .insert(InferenceCategory::Reaction, "sk-ant-test".to_string());
        cfg.category_model
            .insert(InferenceCategory::Reaction, "claude-sonnet-4-5".to_string());

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

    // ── #993 regression tests ────────────────────────────────────────────────

    /// A model-only override (no provider/URL/key) must still trigger a
    /// fresh per-category client, not silently reuse the base client. The
    /// base client points at `:8000` (14B); reusing it for the 1.5B reaction
    /// model would 404 on vllm-mlx two-slot loadouts (#993).
    #[test]
    fn resolve_category_client_model_only_override_triggers_per_category_client() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let mut cfg = GameConfig {
            provider_name: "vllmmlx".to_string(),
            model_name: "mlx-community/Qwen2.5-14B-Instruct-4bit".to_string(),
            base_url: "http://localhost:8000".to_string(),
            ..GameConfig::default()
        };
        cfg.category_model.insert(
            InferenceCategory::Reaction,
            "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string(),
        );

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:8000", None));
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        let client = client.expect("model-only override builds a fresh client");
        let openai = client
            .as_open_ai()
            .expect("vllm-mlx maps to OpenAI-compat transport");
        // The preset URL fallback kicks in because category_base_url is
        // empty AND the resolved provider declares a per-category URL.
        assert_eq!(
            openai.base_url(),
            "http://localhost:8001",
            "reaction model must route to its preset slot, not the base URL"
        );
        assert_eq!(model, "mlx-community/Qwen2.5-1.5B-Instruct-4bit");
    }

    /// When `category_base_url` is empty but the provider declares a
    /// per-category preset URL, `resolve_category_client` must use the
    /// preset URL. This is the safety net that closes the race window
    /// where `fill_missing_models_from_presets` may not have populated
    /// the URL map yet (#993).
    #[test]
    fn resolve_category_client_falls_back_to_preset_base_url() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let mut cfg = GameConfig {
            provider_name: "vllmmlx".to_string(),
            model_name: "mlx-community/Qwen2.5-14B-Instruct-4bit".to_string(),
            base_url: "http://localhost:8000".to_string(),
            ..GameConfig::default()
        };
        // Only a category_provider entry — no category_base_url yet.
        cfg.category_provider
            .insert(InferenceCategory::Intent, "vllmmlx".to_string());

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:8000", None));
        let (client, _model) = cfg.resolve_category_client(InferenceCategory::Intent, Some(&base));
        let client = client.expect("override path builds a client");
        let openai = client
            .as_open_ai()
            .expect("vllm-mlx maps to OpenAI-compat transport");
        assert_eq!(
            openai.base_url(),
            "http://localhost:8001",
            "intent should route to the preset slot when category_base_url is empty"
        );
    }

    /// For single-slot providers whose preset omits `[presets.base_urls]`,
    /// the preset-URL fallback is a no-op and the resolver still uses the
    /// user's base URL. Guards against the new fallback silently rerouting
    /// providers like Ollama / Anthropic.
    #[test]
    fn resolve_category_client_preset_fallback_is_inert_for_single_slot_provider() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        let mut cfg = GameConfig {
            provider_name: "ollama".to_string(),
            model_name: "qwen3:32b".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        cfg.category_model
            .insert(InferenceCategory::Reaction, "qwen3:4b".to_string());

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:11434", None));
        let (client, _model) =
            cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        let openai = client.unwrap();
        let openai = openai.as_open_ai().unwrap();
        assert_eq!(
            openai.base_url(),
            "http://localhost:11434",
            "Ollama has no per-category preset URL → fall back to base URL"
        );
    }

    /// End-to-end hydration test mirroring the user's parish.toml from #993:
    /// vllm-mlx base + `[category_overrides.intent]` only. After
    /// `apply_user_category_overrides` + `fill_missing_models_from_presets`,
    /// the reaction client must build at `:8001` with the 1.5B model.
    #[test]
    fn issue_993_user_config_hydration_routes_reaction_to_slot_8001() {
        use crate::inference::{AnyClient, openai_client::OpenAiClient};
        use parish_config::user_config::CategoryOverride;
        use std::collections::BTreeMap;

        let mut cfg = GameConfig {
            provider_name: "vllmmlx".to_string(),
            base_url: "http://localhost:8000".to_string(),
            ..GameConfig::default()
        };

        let mut overrides = BTreeMap::new();
        overrides.insert(
            "intent".to_string(),
            CategoryOverride {
                provider: Some("vllm-mlx".to_string()),
                model: Some("mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string()),
                base_url: Some("http://localhost:8001".to_string()),
                ..Default::default()
            },
        );
        cfg.apply_user_category_overrides(&overrides);
        cfg.fill_missing_models_from_presets();

        let base = AnyClient::open_ai(OpenAiClient::new("http://localhost:8000", None));
        let (client, model) = cfg.resolve_category_client(InferenceCategory::Reaction, Some(&base));
        let openai = client.unwrap();
        let openai = openai.as_open_ai().unwrap();
        assert_eq!(openai.base_url(), "http://localhost:8001");
        assert_eq!(model, "mlx-community/Qwen2.5-1.5B-Instruct-4bit");
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
            cfg.category_model
                .get(&InferenceCategory::Dialogue)
                .map(String::as_str),
            Some("claude-opus-4-7"),
        );
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Simulation)
                .map(String::as_str),
            Some("claude-sonnet-4-6"),
        );
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some("claude-haiku-4-5"),
        );
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Reaction)
                .map(String::as_str),
            Some("claude-sonnet-4-6"),
        );
    }

    #[test]
    fn google_recommended_model_and_low_thinking_cover_every_category() {
        let mut cfg = GameConfig {
            provider_name: "google".to_string(),
            ..GameConfig::default()
        };
        assert!(cfg.fill_missing_models_from_presets());
        assert_eq!(cfg.model_name, "gemini-3.7-flash");

        for category in InferenceCategory::ALL {
            let model = cfg
                .category_model
                .get(&category)
                .unwrap_or_else(|| panic!("missing {category:?} model"));
            assert_eq!(model, "gemini-3.7-flash", "{category:?} model drifted");
            assert_eq!(
                parish_config::InferenceProfile::for_category(category)
                    .for_model(model)
                    .thinking_level,
                parish_config::ThinkingLevel::Low,
                "{category:?} must use Gemini 3.7's supported Low floor"
            );
        }
    }

    #[test]
    fn fill_missing_models_does_not_overwrite_existing_models() {
        let mut cfg = GameConfig {
            provider_name: "anthropic".to_string(),
            model_name: "user-chosen-model".to_string(),
            ..GameConfig::default()
        };
        cfg.category_model.insert(
            InferenceCategory::Dialogue,
            "user-chosen-dialogue".to_string(),
        );

        cfg.fill_missing_models_from_presets();
        assert_eq!(cfg.model_name, "user-chosen-model");
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Dialogue)
                .map(String::as_str),
            Some("user-chosen-dialogue"),
        );
        // The other three slots should still be filled from the preset.
        assert!(
            cfg.category_model
                .contains_key(&InferenceCategory::Simulation)
        );
        assert!(cfg.category_model.contains_key(&InferenceCategory::Intent));
        assert!(
            cfg.category_model
                .contains_key(&InferenceCategory::Reaction)
        );
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
        cfg.category_provider
            .insert(InferenceCategory::Intent, "anthropic".to_string());

        cfg.fill_missing_models_from_presets();
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Intent)
                .map(String::as_str),
            Some("claude-haiku-4-5"),
        );
        // The other categories should pick up the ollama presets.
        assert_eq!(
            cfg.category_model
                .get(&InferenceCategory::Dialogue)
                .map(String::as_str),
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
        assert!(cfg.category_model.is_empty());
    }

    #[test]
    fn fill_missing_models_returns_false_when_already_complete() {
        let mut cfg = GameConfig {
            provider_name: "anthropic".to_string(),
            model_name: "x".to_string(),
            ..GameConfig::default()
        };
        cfg.category_model
            .insert(InferenceCategory::Dialogue, "a".to_string());
        cfg.category_model
            .insert(InferenceCategory::Simulation, "b".to_string());
        cfg.category_model
            .insert(InferenceCategory::Intent, "c".to_string());
        cfg.category_model
            .insert(InferenceCategory::Reaction, "d".to_string());
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

        assert!(
            cfg.category_rate_limit
                .contains_key(&InferenceCategory::Dialogue)
        );
        assert!(
            cfg.category_rate_limit
                .contains_key(&InferenceCategory::Intent)
        );
        assert!(
            !cfg.category_rate_limit
                .contains_key(&InferenceCategory::Simulation)
        );
        assert!(
            !cfg.category_rate_limit
                .contains_key(&InferenceCategory::Reaction)
        );
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
        assert!(
            !cfg.category_rate_limit
                .contains_key(&InferenceCategory::Dialogue)
        );
    }

    #[test]
    fn identical_cloud_transport_shares_one_aggregate_rate_bucket() {
        use crate::config::{CategoryRateLimit, RateLimitConfig};

        let mut cfg = GameConfig {
            provider_name: "google".to_string(),
            base_url: "https://generativelanguage.googleapis.com/v1".to_string(),
            api_key: Some("test-key".to_string()),
            ..GameConfig::default()
        };
        let limits = RateLimitConfig {
            default: Some(CategoryRateLimit {
                per_minute: 60,
                burst: 2,
            }),
            ..RateLimitConfig::default()
        };
        cfg.install_rate_limits(&limits);

        let dialogue = cfg
            .category_rate_limit
            .get(&InferenceCategory::Dialogue)
            .expect("dialogue limiter");
        let intent = cfg
            .category_rate_limit
            .get(&InferenceCategory::Intent)
            .expect("intent limiter");
        assert!(dialogue.try_acquire());
        assert!(intent.try_acquire());
        assert!(
            !cfg.category_rate_limit[&InferenceCategory::Reaction].try_acquire(),
            "the third role must observe the two-token aggregate bucket as exhausted"
        );
    }

    #[test]
    fn resolve_category_client_attaches_per_category_rate_limit() {
        use crate::config::{CategoryRateLimit, RateLimitConfig};

        let mut cfg = GameConfig {
            model_name: "base-model".to_string(),
            base_url: "http://localhost:11434".to_string(),
            ..GameConfig::default()
        };
        cfg.category_base_url.insert(
            InferenceCategory::Reaction,
            "https://openrouter.ai/api".to_string(),
        );
        cfg.category_api_key
            .insert(InferenceCategory::Reaction, "sk-test".to_string());

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
        cfg.category_base_url.insert(
            InferenceCategory::Reaction,
            "https://openrouter.ai/api".to_string(),
        );
        cfg.category_api_key
            .insert(InferenceCategory::Reaction, "sk-test".to_string());

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

    #[test]
    fn vllm_mlx_extra_slots_empty_when_no_overrides() {
        let cfg = GameConfig {
            provider_name: "vllm-mlx".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string(),
            ..GameConfig::default()
        };
        let slots = cfg.vllm_mlx_extra_slots();
        assert!(slots.is_empty(), "no overrides → no extra slots");
    }

    #[test]
    fn vllm_mlx_extra_slots_emits_distinct_per_category_slot() {
        let mut cfg = GameConfig {
            provider_name: "vllm-mlx".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string(),
            ..GameConfig::default()
        };
        // Intent + Reaction + Simulation all route to a 1.5B slot on :8001.
        for cat in [
            InferenceCategory::Intent,
            InferenceCategory::Reaction,
            InferenceCategory::Simulation,
        ] {
            cfg.category_base_url
                .insert(cat, "http://localhost:8001".to_string());
            cfg.category_model
                .insert(cat, "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string());
        }
        let slots = cfg.vllm_mlx_extra_slots();
        // Three categories share one slot — emitter is per-category, dedup is downstream.
        assert_eq!(slots.len(), 3);
        for slot in &slots {
            assert_eq!(slot.base_url, "http://localhost:8001");
            assert_eq!(slot.model, "mlx-community/Qwen2.5-1.5B-Instruct-4bit");
        }
    }

    #[test]
    fn vllm_mlx_extra_slots_skips_base_slot_when_base_is_vllm_mlx() {
        let mut cfg = GameConfig {
            provider_name: "vllm-mlx".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string(),
            ..GameConfig::default()
        };
        // Dialogue stays on base (no override) — should NOT appear in extras
        // because setup_provider_client auto-spawns the base for VllmMlx.
        // Intent overrides to the same base slot — also should be skipped.
        cfg.category_base_url.insert(
            InferenceCategory::Intent,
            "http://localhost:8000".to_string(),
        );
        cfg.category_model.insert(
            InferenceCategory::Intent,
            "mlx-community/Qwen2.5-7B-Instruct-4bit".to_string(),
        );
        let slots = cfg.vllm_mlx_extra_slots();
        assert!(slots.is_empty(), "base-equal slots must be skipped");
    }

    #[test]
    fn vllm_extra_slots_empty_when_no_overrides() {
        let cfg = GameConfig {
            provider_name: "vllm".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "Qwen/Qwen2.5-14B-Instruct".to_string(),
            ..GameConfig::default()
        };
        let slots = cfg.vllm_extra_slots();
        assert!(slots.is_empty(), "no overrides → no extra slots");
    }

    #[test]
    fn vllm_extra_slots_emits_distinct_per_category_slot() {
        let mut cfg = GameConfig {
            provider_name: "vllm".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "Qwen/Qwen2.5-14B-Instruct".to_string(),
            ..GameConfig::default()
        };
        for cat in [
            InferenceCategory::Intent,
            InferenceCategory::Reaction,
            InferenceCategory::Simulation,
        ] {
            cfg.category_base_url
                .insert(cat, "http://localhost:8001".to_string());
            cfg.category_model
                .insert(cat, "Qwen/Qwen2.5-1.5B-Instruct".to_string());
        }
        let slots = cfg.vllm_extra_slots();
        assert_eq!(slots.len(), 3);
        for slot in &slots {
            assert_eq!(slot.base_url, "http://localhost:8001");
            assert_eq!(slot.model, "Qwen/Qwen2.5-1.5B-Instruct");
        }
    }

    #[test]
    fn vllm_extra_slots_skips_base_slot_when_base_is_vllm() {
        let mut cfg = GameConfig {
            provider_name: "vllm".to_string(),
            base_url: "http://localhost:8000".to_string(),
            model_name: "Qwen/Qwen2.5-14B-Instruct".to_string(),
            ..GameConfig::default()
        };
        cfg.category_base_url.insert(
            InferenceCategory::Intent,
            "http://localhost:8000".to_string(),
        );
        cfg.category_model.insert(
            InferenceCategory::Intent,
            "Qwen/Qwen2.5-14B-Instruct".to_string(),
        );
        let slots = cfg.vllm_extra_slots();
        assert!(slots.is_empty(), "base-equal slots must be skipped");
    }

    #[test]
    fn vllm_extra_slots_ignores_non_vllm_categories() {
        let mut cfg = GameConfig {
            provider_name: "ollama".to_string(),
            base_url: "http://localhost:11434".to_string(),
            model_name: "gemma3:4b".to_string(),
            ..GameConfig::default()
        };
        // Intent → vllm, but Reaction stays ollama. Only the vllm one emits.
        cfg.category_provider
            .insert(InferenceCategory::Intent, "vllm".to_string());
        cfg.category_base_url.insert(
            InferenceCategory::Intent,
            "http://localhost:8001".to_string(),
        );
        cfg.category_model.insert(
            InferenceCategory::Intent,
            "Qwen/Qwen2.5-1.5B-Instruct".to_string(),
        );
        let slots = cfg.vllm_extra_slots();
        assert_eq!(slots.len(), 1);
        assert_eq!(slots[0].base_url, "http://localhost:8001");
    }

    // Regression guard for #996. The Linux/Windows `vllm` provider preset
    // must declare a `[presets.base_urls]` block so the multi-slot loadout
    // round-trips through `fill_missing_models_from_presets` →
    // `vllm_extra_slots` → `VllmProcess::ensure_slots`. Without this, the
    // category model is auto-picked from the preset (e.g. Qwen3-8B) but the
    // category base URL inherits the user-level base (:8000, where only the
    // 14B is loaded) → 404 storm.
    #[test]
    fn vllm_preset_supplies_per_category_base_url() {
        use parish_config::Provider;

        // 1. Schema-level: the loaded provider exposes a base URL per category.
        let vllm = Provider::from_str_loose("vllm").expect("vllm provider loaded");
        assert_eq!(
            vllm.preset_base_url(InferenceCategory::Dialogue),
            Some("http://localhost:8000"),
        );
        assert_eq!(
            vllm.preset_base_url(InferenceCategory::Simulation),
            Some("http://localhost:8001"),
        );
        assert_eq!(
            vllm.preset_base_url(InferenceCategory::Intent),
            Some("http://localhost:8002"),
        );
        assert_eq!(
            vllm.preset_base_url(InferenceCategory::Reaction),
            Some("http://localhost:8001"),
        );

        // 2. Config-level: fill_missing_models_from_presets populates
        //    category_base_url alongside category_model for all four roles.
        let mut cfg = GameConfig {
            provider_name: "vllm".to_string(),
            base_url: "http://localhost:8000".to_string(),
            ..GameConfig::default()
        };
        let changed = cfg.fill_missing_models_from_presets();
        assert!(changed, "preset should fill all four categories");

        assert_eq!(
            cfg.category_base_url.get(&InferenceCategory::Dialogue),
            Some(&"http://localhost:8000".to_string()),
        );
        assert_eq!(
            cfg.category_base_url.get(&InferenceCategory::Simulation),
            Some(&"http://localhost:8001".to_string()),
        );
        assert_eq!(
            cfg.category_base_url.get(&InferenceCategory::Intent),
            Some(&"http://localhost:8002".to_string()),
        );
        assert_eq!(
            cfg.category_base_url.get(&InferenceCategory::Reaction),
            Some(&"http://localhost:8001".to_string()),
        );
        assert_eq!(
            cfg.category_model.get(&InferenceCategory::Dialogue),
            Some(&"Qwen/Qwen3-14B".to_string()),
        );
        assert_eq!(
            cfg.category_model.get(&InferenceCategory::Simulation),
            Some(&"Qwen/Qwen3-8B".to_string()),
        );
        assert_eq!(
            cfg.category_model.get(&InferenceCategory::Intent),
            Some(&"Qwen/Qwen3-4B".to_string()),
        );
        assert_eq!(
            cfg.category_model.get(&InferenceCategory::Reaction),
            Some(&"Qwen/Qwen3-8B".to_string()),
        );

        // 3. Spawn-list level: dialogue (= base 14B@:8000) is elided as the
        //    base slot; the remaining three categories emit one slot each.
        //    Downstream VllmProcess::ensure_slots dedups the duplicate 8B
        //    slot, but vllm_extra_slots itself does not.
        // Set base model to the dialogue preset so the base slot matches.
        cfg.model_name = "Qwen/Qwen3-14B".to_string();
        let slots = cfg.vllm_extra_slots();
        assert_eq!(slots.len(), 3, "sim + intent + reaction, dialogue elided");
        let urls_models: Vec<(String, String)> = slots
            .iter()
            .map(|s| (s.base_url.clone(), s.model.clone()))
            .collect();
        assert!(urls_models.contains(&(
            "http://localhost:8001".to_string(),
            "Qwen/Qwen3-8B".to_string(),
        )));
        assert!(urls_models.contains(&(
            "http://localhost:8002".to_string(),
            "Qwen/Qwen3-4B".to_string(),
        )));
        // Two of the three should be the shared 8B@:8001 slot (sim + reaction).
        let eight_b_count = urls_models
            .iter()
            .filter(|(u, m)| u == "http://localhost:8001" && m == "Qwen/Qwen3-8B")
            .count();
        assert_eq!(eight_b_count, 2, "sim + reaction share the 8B slot");
    }
}
