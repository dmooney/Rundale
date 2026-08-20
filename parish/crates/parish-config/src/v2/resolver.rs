use std::collections::BTreeMap;

use thiserror::Error;

use super::*;

const CATEGORIES: [&str; 4] = ["dialogue", "simulation", "intent", "reaction"];
const SUBROLES: [(&str, &str); 8] = [
    ("dialogue", "dialogue"),
    ("intent", "intent"),
    ("arrival-reaction", "reaction"),
    ("message-reaction", "reaction"),
    ("travel-encounter", "reaction"),
    ("tier2-simulation", "simulation"),
    ("tier3-simulation", "simulation"),
    ("demo-player", "intent"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayerSource {
    Compiled,
    Project,
    User,
    Environment,
    Cli,
}

#[derive(Debug, Clone, Default)]
pub struct RoutingOverrideSet {
    pub active_loadout: Option<String>,
    pub global_env: RoutePatch,
    pub global_cli: RoutePatch,
    pub category_env: BTreeMap<String, RoutePatch>,
    pub category_cli: BTreeMap<String, RoutePatch>,
}

pub fn routing_overrides_from_env() -> Result<RoutingOverrideSet, ResolverError> {
    for legacy in [
        "PARISH_CLOUD_PROVIDER",
        "PARISH_CLOUD_MODEL",
        "PARISH_CLOUD_BASE_URL",
    ] {
        if std::env::var_os(legacy).is_some() {
            return Err(ResolverError::InvalidRoute {
                category: "global".into(),
                message: format!("{legacy} was removed by config v2; use PARISH_DIALOGUE_*"),
            });
        }
    }
    let value = |name: &str| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    let patch = |prefix: &str| RoutePatch {
        provider: value(&format!("{prefix}_PROVIDER")),
        model: value(&format!("{prefix}_MODEL")),
        inference_base_url: value(&format!("{prefix}_BASE_URL")),
        ..RoutePatch::default()
    };
    Ok(RoutingOverrideSet {
        active_loadout: value("PARISH_LOADOUT"),
        global_env: patch("PARISH"),
        category_env: BTreeMap::from([
            ("dialogue".into(), patch("PARISH_DIALOGUE")),
            ("simulation".into(), patch("PARISH_SIMULATION")),
            ("intent".into(), patch("PARISH_INTENT")),
            ("reaction".into(), patch("PARISH_REACTION")),
        ]),
        ..RoutingOverrideSet::default()
    })
}

#[derive(Debug, Clone)]
pub struct MergedInferenceLayer {
    pub active_loadout: Option<String>,
    pub loadouts: BTreeMap<String, (ConfigLayerSource, LoadoutDefinition)>,
    pub providers: BTreeMap<String, (ConfigLayerSource, CustomProviderDefinition)>,
}

pub fn resolve_credential_slots(
    registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
    bindings: &BTreeMap<String, CredentialBinding>,
    mut keychain_get: impl FnMut(&str) -> Option<String>,
) -> BTreeMap<String, SecretString> {
    let mut slots = registry
        .values()
        .flat_map(|provider| provider.endpoints.values())
        .filter_map(|endpoint| endpoint.credential_slot.clone())
        .collect::<std::collections::BTreeSet<_>>();
    slots.extend(merged.providers.keys().map(|slug| format!("custom:{slug}")));
    slots
        .into_iter()
        .filter_map(|slot| {
            let binding = bindings.get(&slot);
            let env_name = binding
                .and_then(|binding| binding.env.as_deref())
                .map(str::to_string)
                .or_else(|| {
                    (!slot.starts_with("custom:"))
                        .then(|| crate::Provider::from_str_loose(&slot))
                        .and_then(Result::ok)
                        .and_then(|provider| provider.api_key_env_var().map(str::to_string))
                });
            let from_env = env_name
                .as_deref()
                .and_then(|name| std::env::var(name).ok())
                .filter(|value| !value.trim().is_empty());
            from_env
                .or_else(|| keychain_get(&slot))
                .map(|secret| (slot, SecretString::new(secret)))
        })
        .collect()
}

#[derive(Clone)]
pub struct ResolvedInferenceSnapshot {
    pub configuration_epoch: u64,
    pub active_loadout: String,
    pub category_routes: BTreeMap<String, ResolvedRoute>,
    pub subrole_routes: BTreeMap<String, ResolvedRoute>,
}

impl std::fmt::Debug for ResolvedInferenceSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedInferenceSnapshot")
            .field("configuration_epoch", &self.configuration_epoch)
            .field("active_loadout", &self.active_loadout)
            .field(
                "category_routes",
                &self
                    .category_routes
                    .iter()
                    .map(|(name, route)| (name, route.view(self.configuration_epoch)))
                    .collect::<BTreeMap<_, _>>(),
            )
            .field(
                "subrole_routes",
                &self
                    .subrole_routes
                    .iter()
                    .map(|(name, route)| (name, route.view(self.configuration_epoch)))
                    .collect::<BTreeMap<_, _>>(),
            )
            .finish()
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum ResolverError {
    #[error("unknown loadout {0:?}")]
    UnknownLoadout(String),
    #[error("unknown provider {0:?}")]
    UnknownProvider(String),
    #[error("provider {provider:?} has no endpoint {endpoint:?}")]
    UnknownEndpoint { provider: String, endpoint: String },
    #[error("{category} route has no model after resolving provider {provider:?}")]
    MissingModel { category: String, provider: String },
    #[error("{category} route endpoint {endpoint:?} requires credential slot {slot:?}")]
    MissingCredential {
        category: String,
        endpoint: String,
        slot: String,
    },
    #[error("invalid {category} route: {message}")]
    InvalidRoute { category: String, message: String },
}

pub fn merge_inference_layers(
    compiled: &InferenceLayer,
    project: &InferenceLayer,
    user: &InferenceLayer,
) -> MergedInferenceLayer {
    let mut loadouts = BTreeMap::new();
    let mut providers = BTreeMap::new();
    for (source, layer) in [
        (ConfigLayerSource::Compiled, compiled),
        (ConfigLayerSource::Project, project),
        (ConfigLayerSource::User, user),
    ] {
        for (name, value) in &layer.loadouts {
            loadouts.insert(name.clone(), (source, value.clone()));
        }
        for (name, value) in &layer.providers {
            providers.insert(name.clone(), (source, value.clone()));
        }
    }
    MergedInferenceLayer {
        active_loadout: user
            .active_loadout
            .clone()
            .or_else(|| project.active_loadout.clone())
            .or_else(|| compiled.active_loadout.clone()),
        loadouts,
        providers,
    }
}

pub fn resolve_inference_snapshot(
    epoch: u64,
    registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
    overrides: &RoutingOverrideSet,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    credentials: &BTreeMap<String, SecretString>,
) -> Result<ResolvedInferenceSnapshot, ResolverError> {
    let effective_registry = effective_provider_registry(registry, merged);
    let registry = &effective_registry;
    resolve_inference_snapshot_with_registry(
        epoch,
        registry,
        merged,
        overrides,
        availability,
        credentials,
        true,
    )
}

/// Resolves only the exact endpoint/origin/adapter/account topology needed to
/// locate account-scoped catalog evidence. The returned value is never
/// publishable: eligibility is deliberately deferred to the subsequent full
/// `resolve_inference_snapshot` call.
pub fn resolve_inference_topology_snapshot(
    epoch: u64,
    registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
    overrides: &RoutingOverrideSet,
    credentials: &BTreeMap<String, SecretString>,
) -> Result<ResolvedInferenceSnapshot, ResolverError> {
    let effective_registry = effective_provider_registry(registry, merged);
    resolve_inference_snapshot_with_registry(
        epoch,
        &effective_registry,
        merged,
        overrides,
        &BTreeMap::new(),
        credentials,
        false,
    )
}

/// Full, publishable resolution against an already merged and remotely
/// constrained registry. Callers must obtain this registry from the exact
/// active account/origin catalog identities; remote data may only narrow the
/// authored descriptors.
pub fn resolve_inference_snapshot_from_effective_registry(
    epoch: u64,
    effective_registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
    overrides: &RoutingOverrideSet,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    credentials: &BTreeMap<String, SecretString>,
) -> Result<ResolvedInferenceSnapshot, ResolverError> {
    resolve_inference_snapshot_with_registry(
        epoch,
        effective_registry,
        merged,
        overrides,
        availability,
        credentials,
        true,
    )
}

pub fn effective_provider_registry(
    registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
) -> BTreeMap<String, ProviderDefinition> {
    let mut effective_registry = registry.clone();
    for (provider_slug, (_, provider)) in &merged.providers {
        let provider_id = format!("custom:{provider_slug}");
        effective_registry.insert(
            provider_id.clone(),
            custom_provider_definition(&provider_id, provider),
        );
    }
    effective_registry
}

fn resolve_inference_snapshot_with_registry(
    epoch: u64,
    registry: &BTreeMap<String, ProviderDefinition>,
    merged: &MergedInferenceLayer,
    overrides: &RoutingOverrideSet,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    credentials: &BTreeMap<String, SecretString>,
    enforce_eligibility: bool,
) -> Result<ResolvedInferenceSnapshot, ResolverError> {
    let active = overrides
        .active_loadout
        .as_deref()
        .or(merged.active_loadout.as_deref())
        .unwrap_or("default");
    let (_, loadout) = merged
        .loadouts
        .get(active)
        .ok_or_else(|| ResolverError::UnknownLoadout(active.to_string()))?;

    let mut category_routes = BTreeMap::new();
    for category in CATEGORIES {
        let mut state = initial_route_for_category(registry, category)?;
        apply_route_patch(&mut state, &loadout.default, category, registry)?;
        apply_route_patch(&mut state, &overrides.global_env, category, registry)?;
        apply_route_patch(&mut state, &overrides.global_cli, category, registry)?;
        if let Some(patch) = category_patch(loadout, category) {
            apply_route_patch(&mut state, patch, category, registry)?;
        }
        if let Some(patch) = overrides.category_env.get(category) {
            apply_route_patch(&mut state, patch, category, registry)?;
        }
        if let Some(patch) = overrides.category_cli.get(category) {
            apply_route_patch(&mut state, patch, category, registry)?;
        }
        let route = finish_route(
            state,
            category,
            category,
            &loadout.default,
            category_patch(loadout, category),
            None,
            registry,
            availability,
            credentials,
            enforce_eligibility,
        )?;
        category_routes.insert(category.to_string(), route);
    }

    // Catalog/probe authority is keyed by ModelRouteKey. Allowing that same
    // key to denote two base paths or adapter identities in one immutable
    // snapshot would make eligibility evidence ambiguous. Users who need two
    // gateways must give them distinct custom provider/endpoint IDs.
    let mut identities = BTreeMap::<ModelRouteKey, (&str, &ResolvedRoute)>::new();
    for (category, route) in &category_routes {
        if let Some((other_category, other)) =
            identities.insert(route.key.clone(), (category.as_str(), route))
            && (other.inference_base_url != route.inference_base_url
                || other.discovery_base_url != route.discovery_base_url
                || other.inference_adapter != route.inference_adapter
                || other.discovery_adapter != route.discovery_adapter)
        {
            return Err(ResolverError::InvalidRoute {
                category: category.clone(),
                message: format!(
                    "model route {}:{}:{} has a different endpoint identity from {other_category}; use distinct provider/endpoint IDs",
                    route.key.provider_id, route.key.endpoint_id, route.key.model_id
                ),
            });
        }
    }

    let mut subrole_routes = BTreeMap::new();
    for (subrole, category) in SUBROLES {
        let base = category_routes
            .get(category)
            .expect("all category routes were resolved")
            .clone();
        let route = finish_profile_for_existing_route(
            base,
            category,
            subrole,
            &loadout.default,
            category_patch(loadout, category),
            loadout.subroles.get(subrole),
            registry,
        )?;
        subrole_routes.insert(subrole.to_string(), route);
    }

    Ok(ResolvedInferenceSnapshot {
        configuration_epoch: epoch,
        active_loadout: active.to_string(),
        category_routes,
        subrole_routes,
    })
}

fn custom_provider_definition(
    provider_id: &str,
    custom: &CustomProviderDefinition,
) -> ProviderDefinition {
    let endpoints = custom
        .endpoints
        .iter()
        .map(|(endpoint_id, endpoint)| {
            (
                endpoint_id.clone(),
                EndpointDefinition {
                    inference_base_url: endpoint.inference_base_url.clone(),
                    discovery_base_url: endpoint.discovery_base_url.clone(),
                    inference_adapter: endpoint.inference_adapter,
                    discovery_adapter: endpoint.discovery_adapter,
                    backend_kind: BackendKind::Remote,
                    management_adapter: ManagementAdapter::None,
                    auth_adapter: endpoint.auth_adapter,
                    credential_slot: (endpoint.auth_adapter != AuthAdapter::None)
                        .then(|| provider_id.to_string()),
                    default_reasoning_dialect: endpoint.default_reasoning_dialect,
                    default_openai_generation_wire: endpoint.default_openai_generation_wire.clone(),
                },
            )
        })
        .collect();
    let curated_models = custom
        .models
        .iter()
        .map(|(endpoint_id, models)| {
            let models = models
                .iter()
                .filter_map(|(model_id, capability)| {
                    Some((
                        model_id.clone(),
                        CuratedModelRoute {
                            qualified: false,
                            recommended: false,
                            generation_defaults: BTreeMap::new(),
                            reasoning: capability.reasoning.clone()?,
                            generation: capability.generation.clone()?,
                            output_contracts: capability.output_contracts.clone()?,
                            openai_generation_wire: capability.openai_generation_wire.clone(),
                            qualification_receipt: None,
                        },
                    ))
                })
                .collect();
            (endpoint_id.clone(), models)
        })
        .collect();
    ProviderDefinition {
        id: provider_id.to_string(),
        display_name: custom.display_name.clone(),
        aliases: Vec::new(),
        default_endpoint: custom
            .default_endpoint
            .clone()
            .expect("validated custom providers declare a default endpoint"),
        recommended_preset: None,
        endpoints,
        presets: BTreeMap::new(),
        curated_models,
    }
}

#[derive(Clone)]
struct RouteState {
    provider: String,
    endpoint: Option<String>,
    model: Option<String>,
    inference_base_url: Option<String>,
    discovery_base_url: Option<String>,
    allow_unverified_model: bool,
}

fn initial_route_for_category(
    registry: &BTreeMap<String, ProviderDefinition>,
    category: &str,
) -> Result<RouteState, ResolverError> {
    let provider = registry
        .values()
        .find(|provider| provider.id == "ollama")
        .or_else(|| registry.values().next())
        .ok_or_else(|| ResolverError::UnknownProvider("<compiled default>".into()))?;
    let mut state = RouteState {
        provider: provider.id.clone(),
        endpoint: None,
        model: None,
        inference_base_url: None,
        discovery_base_url: None,
        allow_unverified_model: false,
    };
    refill_provider_defaults(&mut state, provider, category);
    Ok(state)
}

fn apply_route_patch(
    state: &mut RouteState,
    patch: &RoutePatch,
    category: &str,
    registry: &BTreeMap<String, ProviderDefinition>,
) -> Result<(), ResolverError> {
    if let Some(provider_id) = &patch.provider {
        let provider = registry
            .get(provider_id)
            .ok_or_else(|| ResolverError::UnknownProvider(provider_id.clone()))?;
        state.provider = provider_id.clone();
        state.endpoint = None;
        state.model = None;
        state.inference_base_url = None;
        state.discovery_base_url = None;
        refill_provider_defaults(state, provider, category);
    }
    if let Some(endpoint_id) = &patch.endpoint {
        let provider = registry
            .get(&state.provider)
            .ok_or_else(|| ResolverError::UnknownProvider(state.provider.clone()))?;
        let endpoint =
            provider
                .endpoints
                .get(endpoint_id)
                .ok_or_else(|| ResolverError::UnknownEndpoint {
                    provider: state.provider.clone(),
                    endpoint: endpoint_id.clone(),
                })?;
        state.endpoint = Some(endpoint_id.clone());
        state.model = recommended_route(provider, category)
            .filter(|preset| preset.endpoint == *endpoint_id)
            .map(|preset| preset.model.clone());
        state.inference_base_url = Some(endpoint.inference_base_url.clone());
        state.discovery_base_url = endpoint.discovery_base_url.clone();
    }
    if let Some(model) = &patch.model {
        state.model = Some(model.clone());
    }
    if let Some(url) = &patch.inference_base_url {
        state.inference_base_url = Some(url.clone());
    }
    if let Some(url) = &patch.discovery_base_url {
        state.discovery_base_url = Some(url.clone());
    }
    if let Some(value) = patch.allow_unverified_model {
        state.allow_unverified_model = value;
    }
    Ok(())
}

fn refill_provider_defaults(state: &mut RouteState, provider: &ProviderDefinition, category: &str) {
    if let Some(preset) = recommended_route(provider, category) {
        state.endpoint = Some(preset.endpoint.clone());
        state.model = Some(preset.model.clone());
    } else {
        state.endpoint = Some(provider.default_endpoint.clone());
    }
    if let Some(endpoint) = state
        .endpoint
        .as_ref()
        .and_then(|id| provider.endpoints.get(id))
    {
        state.inference_base_url = Some(endpoint.inference_base_url.clone());
        state.discovery_base_url = endpoint.discovery_base_url.clone();
    }
}

fn recommended_route<'a>(
    provider: &'a ProviderDefinition,
    category: &str,
) -> Option<&'a PresetRoute> {
    let preset = provider
        .recommended_preset
        .as_ref()
        .and_then(|id| provider.presets.get(id))?;
    match category {
        "dialogue" => preset.dialogue.as_ref(),
        "simulation" => preset.simulation.as_ref(),
        "intent" => preset.intent.as_ref(),
        "reaction" => preset.reaction.as_ref(),
        _ => None,
    }
}

fn category_patch<'a>(loadout: &'a LoadoutDefinition, category: &str) -> Option<&'a RoutePatch> {
    match category {
        "dialogue" => loadout.routes.dialogue.as_ref(),
        "simulation" => loadout.routes.simulation.as_ref(),
        "intent" => loadout.routes.intent.as_ref(),
        "reaction" => loadout.routes.reaction.as_ref(),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_route(
    state: RouteState,
    category: &str,
    subrole: &str,
    default_patch: &RoutePatch,
    category_patch: Option<&RoutePatch>,
    subrole_patch: Option<&GenerationPatch>,
    registry: &BTreeMap<String, ProviderDefinition>,
    availability: &BTreeMap<ModelRouteKey, Availability>,
    credentials: &BTreeMap<String, SecretString>,
    enforce_eligibility: bool,
) -> Result<ResolvedRoute, ResolverError> {
    let provider = registry
        .get(&state.provider)
        .ok_or_else(|| ResolverError::UnknownProvider(state.provider.clone()))?;
    let endpoint_id = state
        .endpoint
        .clone()
        .ok_or_else(|| ResolverError::InvalidRoute {
            category: category.into(),
            message: "no endpoint".into(),
        })?;
    let endpoint =
        provider
            .endpoints
            .get(&endpoint_id)
            .ok_or_else(|| ResolverError::UnknownEndpoint {
                provider: provider.id.clone(),
                endpoint: endpoint_id.clone(),
            })?;
    let model = if endpoint.inference_adapter == InferenceAdapter::Simulator {
        state.model.unwrap_or_else(|| "simulator".into())
    } else {
        state.model.ok_or_else(|| ResolverError::MissingModel {
            category: category.into(),
            provider: provider.id.clone(),
        })?
    };
    let key = ModelRouteKey {
        provider_id: provider.id.clone(),
        endpoint_id: endpoint_id.clone(),
        model_id: model,
    };
    let curated = provider
        .curated_models
        .get(&endpoint_id)
        .and_then(|models| models.get(&key.model_id));
    let observed = availability
        .get(&key)
        .cloned()
        .unwrap_or(Availability::Unknown);
    let mut diagnostics = Vec::new();
    if enforce_eligibility && endpoint.inference_adapter != InferenceAdapter::Simulator {
        validate_eligibility(
            curated,
            &observed,
            state.allow_unverified_model,
            &mut diagnostics,
            category,
        )?;
    }
    let profile = resolve_generation_profile(
        subrole,
        curated,
        default_patch,
        category_patch,
        subrole_patch,
        &mut diagnostics,
        category,
    )?;
    let structured_output = select_structured_output(subrole, curated, endpoint)?;
    let credential = endpoint
        .credential_slot
        .as_ref()
        .and_then(|slot| credentials.get(slot))
        .cloned();
    if endpoint.auth_adapter != AuthAdapter::None && credential.is_none() {
        return Err(ResolverError::MissingCredential {
            category: category.into(),
            endpoint: endpoint_id.clone(),
            slot: endpoint
                .credential_slot
                .clone()
                .unwrap_or_else(|| provider.id.clone()),
        });
    }
    let inference_base_url = state
        .inference_base_url
        .unwrap_or_else(|| endpoint.inference_base_url.clone());
    let discovery_base_url = state
        .discovery_base_url
        .or_else(|| endpoint.discovery_base_url.clone());
    if endpoint.auth_adapter != AuthAdapter::None {
        require_same_origin(
            category,
            "inference",
            &endpoint.inference_base_url,
            &inference_base_url,
        )?;
        if let (Some(expected), Some(actual)) = (
            endpoint.discovery_base_url.as_deref(),
            discovery_base_url.as_deref(),
        ) {
            require_same_origin(category, "discovery", expected, actual)?;
        }
    }
    Ok(ResolvedRoute {
        key,
        inference_base_url,
        discovery_base_url,
        credential,
        inference_adapter: endpoint.inference_adapter,
        discovery_adapter: endpoint.discovery_adapter,
        backend_kind: endpoint.backend_kind,
        management_adapter: endpoint.management_adapter,
        auth_adapter: endpoint.auth_adapter,
        reasoning_dialect: resolved_reasoning_dialect(
            &profile.1.reasoning,
            curated,
            endpoint.default_reasoning_dialect,
        ),
        openai_output_limit_field: curated
            .and_then(|model| model.openai_generation_wire.as_ref())
            .or(endpoint.default_openai_generation_wire.as_ref())
            .map(|wire| wire.output_limit_field),
        requested_profile: profile.0,
        effective_profile: profile.1,
        structured_output,
        availability: observed,
        diagnostics,
    })
}

fn require_same_origin(
    category: &str,
    purpose: &str,
    expected: &str,
    actual: &str,
) -> Result<(), ResolverError> {
    let parse = |value: &str| {
        url::Url::parse(value).map_err(|error| ResolverError::InvalidRoute {
            category: category.into(),
            message: format!("invalid {purpose} URL: {error}"),
        })
    };
    let expected = parse(expected)?;
    let actual = parse(actual)?;
    let same = expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected.port_or_known_default() == actual.port_or_known_default();
    if !same {
        return invalid_route(
            category,
            format!("authenticated {purpose} URL override must keep the endpoint origin"),
        );
    }
    Ok(())
}

fn resolved_reasoning_dialect(
    reasoning: &ReasoningIntent,
    curated: Option<&CuratedModelRoute>,
    endpoint_default: ReasoningDialect,
) -> ReasoningDialect {
    let Some(capabilities) = curated.map(|model| &model.reasoning) else {
        return endpoint_default;
    };
    match reasoning {
        ReasoningIntent::Off => capabilities.off_dialect.unwrap_or(endpoint_default),
        ReasoningIntent::Effort { .. } => capabilities
            .effort
            .as_ref()
            .map(|effort| effort.dialect)
            .unwrap_or(endpoint_default),
        ReasoningIntent::Budget { .. } => capabilities
            .budget
            .as_ref()
            .map(|budget| budget.dialect)
            .unwrap_or(endpoint_default),
        ReasoningIntent::Auto => endpoint_default,
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_profile_for_existing_route(
    mut route: ResolvedRoute,
    category: &str,
    subrole: &str,
    default_patch: &RoutePatch,
    category_patch: Option<&RoutePatch>,
    subrole_patch: Option<&GenerationPatch>,
    registry: &BTreeMap<String, ProviderDefinition>,
) -> Result<ResolvedRoute, ResolverError> {
    let provider = registry
        .get(&route.key.provider_id)
        .expect("route provider was already validated");
    let endpoint = provider
        .endpoints
        .get(&route.key.endpoint_id)
        .expect("route endpoint was already validated");
    let curated = provider
        .curated_models
        .get(&route.key.endpoint_id)
        .and_then(|models| models.get(&route.key.model_id));
    let profile = resolve_generation_profile(
        subrole,
        curated,
        default_patch,
        category_patch,
        subrole_patch,
        &mut route.diagnostics,
        category,
    )?;
    route.requested_profile = profile.0;
    route.effective_profile = profile.1;
    route.reasoning_dialect = resolved_reasoning_dialect(
        &route.effective_profile.reasoning,
        curated,
        endpoint.default_reasoning_dialect,
    );
    route.structured_output = select_structured_output(subrole, curated, endpoint)?;
    Ok(route)
}

fn resolve_generation_profile(
    subrole: &str,
    curated: Option<&CuratedModelRoute>,
    default_patch: &RoutePatch,
    category_patch: Option<&RoutePatch>,
    subrole_patch: Option<&GenerationPatch>,
    diagnostics: &mut Vec<ConfigDiagnostic>,
    category: &str,
) -> Result<(GenerationProfile, GenerationProfile), ResolverError> {
    let mut requested = curated
        .and_then(|model| model.generation_defaults.get(subrole))
        .cloned()
        .unwrap_or_else(|| adapter_safe_profile(subrole));
    apply_generation_from_route(&mut requested, default_patch);
    if let Some(patch) = category_patch {
        apply_generation_from_route(&mut requested, patch);
    }
    if let Some(patch) = subrole_patch {
        apply_generation_patch(&mut requested, patch);
    }
    let mut effective = requested.clone();
    if let Some(model) = curated {
        effective.reasoning = resolve_reasoning(
            &requested.reasoning,
            &model.reasoning,
            subrole,
            requested.max_output_tokens,
            diagnostics,
            category,
        )?;
        validate_generation_capabilities(&effective, &model.generation, category)?;
    } else if !matches!(requested.reasoning, ReasoningIntent::Auto)
        || requested.temperature.is_some()
        || requested.frequency_penalty.is_some()
        || requested.service_tier != ServiceTierIntent::Auto
    {
        return Err(ResolverError::InvalidRoute {
            category: category.into(),
            message: "uncurated model requires declared capabilities for reasoning, sampling, or service tier".into(),
        });
    }
    Ok((requested, effective))
}

fn resolve_reasoning(
    requested: &ReasoningIntent,
    capabilities: &ReasoningCapabilities,
    subrole: &str,
    output_cap: u32,
    diagnostics: &mut Vec<ConfigDiagnostic>,
    category: &str,
) -> Result<ReasoningIntent, ResolverError> {
    let concrete = match requested {
        ReasoningIntent::Auto => capabilities
            .default_by_subrole
            .get(subrole)
            .cloned()
            .unwrap_or(ReasoningIntent::Auto),
        other => *other,
    };
    let translated = capabilities
        .translations
        .iter()
        .find(|rule| rule.from == concrete)
        .map(|rule| {
            diagnostics.push(ConfigDiagnostic {
                code: "reasoning-translated".into(),
                severity: DiagnosticSeverity::Warning,
                message: format!("reasoning request translated by {}", rule.rule_id),
                rule_id: Some(rule.rule_id.clone()),
            });
            rule.to
        })
        .unwrap_or(concrete);
    match &translated {
        ReasoningIntent::Auto => {
            if capabilities.mandatory {
                return invalid_route(
                    category,
                    "mandatory reasoning has no concrete subrole default",
                );
            }
        }
        ReasoningIntent::Off => {
            if capabilities.mandatory || capabilities.off_dialect.is_none() {
                return invalid_route(
                    category,
                    "reasoning cannot be disabled for this model route",
                );
            }
        }
        ReasoningIntent::Effort { level } => {
            let effort = capabilities
                .effort
                .as_ref()
                .ok_or_else(|| invalid_route_error(category, "effort reasoning is unsupported"))?;
            if !effort.supported_levels.contains(level) {
                return invalid_route(category, "requested reasoning effort is unsupported");
            }
            let minimum = effort
                .minimum_total_output_tokens
                .get(subrole)
                .and_then(|by_level| by_level.get(level));
            if capabilities.mandatory && minimum.is_none() {
                return invalid_route(
                    category,
                    "mandatory reasoning effort lacks measured headroom",
                );
            }
            if minimum.is_some_and(|minimum| output_cap < *minimum) {
                return invalid_route(category, "output cap is below reasoning headroom minimum");
            }
        }
        ReasoningIntent::Budget { tokens } => {
            let budget = capabilities
                .budget
                .as_ref()
                .ok_or_else(|| invalid_route_error(category, "reasoning budget is unsupported"))?;
            if !(budget.min_tokens..=budget.max_tokens).contains(tokens)
                || tokens.saturating_add(budget.min_visible_output_headroom) > output_cap
            {
                return invalid_route(
                    category,
                    "reasoning budget violates range or output headroom",
                );
            }
        }
    }
    Ok(translated)
}

fn validate_generation_capabilities(
    profile: &GenerationProfile,
    capabilities: &GenerationCapabilities,
    category: &str,
) -> Result<(), ResolverError> {
    if !(capabilities.min_output_tokens..=capabilities.max_output_tokens)
        .contains(&profile.max_output_tokens)
    {
        return invalid_route(category, "output cap is outside model capability range");
    }
    for (value, range, name) in [
        (profile.temperature, capabilities.temperature, "temperature"),
        (
            profile.frequency_penalty,
            capabilities.frequency_penalty,
            "frequency penalty",
        ),
    ] {
        if let Some(value) = value {
            let Some(range) = range else {
                return invalid_route(category, format!("explicit {name} is unsupported"));
            };
            if !(range.min..=range.max).contains(&value) {
                return invalid_route(category, format!("{name} is outside supported range"));
            }
        }
    }
    if profile.service_tier != ServiceTierIntent::Auto
        && !capabilities
            .service_tiers
            .contains_key(&profile.service_tier)
    {
        return invalid_route(category, "explicit service tier is unsupported");
    }
    Ok(())
}

fn validate_eligibility(
    curated: Option<&CuratedModelRoute>,
    availability: &Availability,
    allow_unverified: bool,
    diagnostics: &mut Vec<ConfigDiagnostic>,
    category: &str,
) -> Result<(), ResolverError> {
    if availability == &Availability::Incompatible {
        return invalid_route(
            category,
            "model route is explicitly incompatible with required text streaming transport",
        );
    }
    if curated.is_some_and(|model| model.qualified || model.recommended) {
        if availability != &Availability::Listed {
            diagnostics.push(ConfigDiagnostic {
                code: "curated-availability-unconfirmed".into(),
                severity: if availability == &Availability::NotListed {
                    DiagnosticSeverity::High
                } else {
                    DiagnosticSeverity::Warning
                },
                message: "qualified curated route is usable, but live availability is unconfirmed"
                    .into(),
                rule_id: None,
            });
        }
        return Ok(());
    }
    if matches!(availability, Availability::Listed) || allow_unverified {
        return Ok(());
    }
    invalid_route(
        category,
        "model is not freshly listed; set allow_unverified_model=true to opt in",
    )
}

fn select_structured_output(
    subrole: &str,
    curated: Option<&CuratedModelRoute>,
    endpoint: &EndpointDefinition,
) -> Result<Option<StructuredOutputMode>, ResolverError> {
    if !requires_validated_json(subrole) {
        return Ok(None);
    }
    let capabilities = curated
        .map(|model| model.output_contracts.clone())
        .unwrap_or(OutputContractCapabilities {
            prompt_validated_json: true,
            native_json_object: false,
            native_json_schema: false,
        });
    let selected = if capabilities.native_json_schema {
        Some(StructuredOutputMode::JsonSchema)
    } else if capabilities.native_json_object {
        Some(StructuredOutputMode::JsonObject)
    } else if capabilities.prompt_validated_json {
        Some(StructuredOutputMode::PromptValidatedJson)
    } else {
        None
    };
    if let Some(mode) = selected
        && endpoint.inference_adapter == InferenceAdapter::OpenaiChatV1
        && !curated
            .and_then(|model| model.openai_generation_wire.as_ref())
            .or(endpoint.default_openai_generation_wire.as_ref())
            .as_ref()
            .is_some_and(|wire| wire.structured_output.contains(&mode))
    {
        return Err(ResolverError::InvalidRoute {
            category: subrole.into(),
            message: format!(
                "resolved output contract {mode:?} is unsupported by the endpoint wire"
            ),
        });
    }
    selected
        .map(Some)
        .ok_or_else(|| ResolverError::InvalidRoute {
            category: subrole.into(),
            message: "no validated JSON output contract remains".into(),
        })
}

fn requires_validated_json(subrole: &str) -> bool {
    matches!(
        subrole,
        "dialogue" | "intent" | "message-reaction" | "tier2-simulation" | "tier3-simulation"
    )
}

fn adapter_safe_profile(subrole: &str) -> GenerationProfile {
    GenerationProfile {
        max_output_tokens: match subrole {
            "intent" => 256,
            "arrival-reaction" | "message-reaction" | "travel-encounter" => 1_024,
            "tier2-simulation" => 2_048,
            "tier3-simulation" => 4_096,
            "demo-player" => 200,
            _ => 768,
        },
        temperature: None,
        frequency_penalty: None,
        reasoning: ReasoningIntent::Auto,
        service_tier: ServiceTierIntent::Auto,
    }
}

fn apply_generation_from_route(profile: &mut GenerationProfile, patch: &RoutePatch) {
    if let Some(value) = patch.max_output_tokens {
        profile.max_output_tokens = value;
    }
    if patch.temperature.is_some() {
        profile.temperature = patch.temperature;
    }
    if patch.frequency_penalty.is_some() {
        profile.frequency_penalty = patch.frequency_penalty;
    }
    if let Some(value) = &patch.reasoning {
        profile.reasoning = *value;
    }
    if let Some(value) = patch.service_tier {
        profile.service_tier = value;
    }
}

fn apply_generation_patch(profile: &mut GenerationProfile, patch: &GenerationPatch) {
    if let Some(value) = patch.max_output_tokens {
        profile.max_output_tokens = value;
    }
    if patch.temperature.is_some() {
        profile.temperature = patch.temperature;
    }
    if patch.frequency_penalty.is_some() {
        profile.frequency_penalty = patch.frequency_penalty;
    }
    if let Some(value) = &patch.reasoning {
        profile.reasoning = *value;
    }
    if let Some(value) = patch.service_tier {
        profile.service_tier = value;
    }
}

fn invalid_route<T>(category: &str, message: impl Into<String>) -> Result<T, ResolverError> {
    Err(invalid_route_error(category, message))
}

fn invalid_route_error(category: &str, message: impl Into<String>) -> ResolverError {
    ResolverError::InvalidRoute {
        category: category.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn registry() -> BTreeMap<String, ProviderDefinition> {
        let endpoint = EndpointDefinition {
            inference_base_url: "https://example.test/v1".into(),
            discovery_base_url: Some("https://example.test/v1".into()),
            inference_adapter: InferenceAdapter::OpenaiChatV1,
            discovery_adapter: DiscoveryAdapter::OpenaiModelsV1,
            backend_kind: BackendKind::Remote,
            management_adapter: ManagementAdapter::None,
            auth_adapter: AuthAdapter::None,
            credential_slot: None,
            default_reasoning_dialect: ReasoningDialect::None,
            default_openai_generation_wire: Some(OpenAiChatGenerationWire {
                output_limit_field: OutputLimitField::MaxTokens,
                structured_output: BTreeSet::from([StructuredOutputMode::PromptValidatedJson]),
            }),
        };
        let preset = ProviderPresetV2 {
            dialogue: Some(PresetRoute {
                endpoint: "chat".into(),
                model: "model-d".into(),
            }),
            simulation: Some(PresetRoute {
                endpoint: "chat".into(),
                model: "model-s".into(),
            }),
            intent: Some(PresetRoute {
                endpoint: "chat".into(),
                model: "model-i".into(),
            }),
            reaction: Some(PresetRoute {
                endpoint: "chat".into(),
                model: "model-r".into(),
            }),
        };
        BTreeMap::from([(
            "ollama".into(),
            ProviderDefinition {
                id: "ollama".into(),
                display_name: "Test".into(),
                aliases: vec![],
                default_endpoint: "chat".into(),
                recommended_preset: Some("recommended".into()),
                endpoints: BTreeMap::from([("chat".into(), endpoint)]),
                presets: BTreeMap::from([("recommended".into(), preset)]),
                curated_models: BTreeMap::new(),
            },
        )])
    }

    fn merged() -> MergedInferenceLayer {
        MergedInferenceLayer {
            active_loadout: Some("recommended".into()),
            loadouts: BTreeMap::from([(
                "recommended".into(),
                (ConfigLayerSource::Compiled, LoadoutDefinition::default()),
            )]),
            providers: BTreeMap::new(),
        }
    }

    #[test]
    fn category_presets_are_resolved_independently() {
        let snapshot = resolve_inference_snapshot(
            4,
            &registry(),
            &merged(),
            &RoutingOverrideSet::default(),
            &BTreeMap::from([
                (
                    ModelRouteKey {
                        provider_id: "ollama".into(),
                        endpoint_id: "chat".into(),
                        model_id: "model-d".into(),
                    },
                    Availability::Listed,
                ),
                (
                    ModelRouteKey {
                        provider_id: "ollama".into(),
                        endpoint_id: "chat".into(),
                        model_id: "model-s".into(),
                    },
                    Availability::Listed,
                ),
                (
                    ModelRouteKey {
                        provider_id: "ollama".into(),
                        endpoint_id: "chat".into(),
                        model_id: "model-i".into(),
                    },
                    Availability::Listed,
                ),
                (
                    ModelRouteKey {
                        provider_id: "ollama".into(),
                        endpoint_id: "chat".into(),
                        model_id: "model-r".into(),
                    },
                    Availability::Listed,
                ),
            ]),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(snapshot.category_routes["dialogue"].key.model_id, "model-d");
        assert_eq!(
            snapshot.category_routes["simulation"].key.model_id,
            "model-s"
        );
        assert_eq!(snapshot.configuration_epoch, 4);
    }

    #[test]
    fn loadout_maps_replace_whole_entries() {
        let compiled = InferenceLayer {
            loadouts: BTreeMap::from([(
                "same".into(),
                LoadoutDefinition {
                    default: RoutePatch {
                        model: Some("compiled".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let user = InferenceLayer {
            loadouts: BTreeMap::from([(
                "same".into(),
                LoadoutDefinition {
                    default: RoutePatch {
                        provider: Some("ollama".into()),
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        let merged = merge_inference_layers(&compiled, &InferenceLayer::default(), &user);
        assert_eq!(merged.loadouts["same"].0, ConfigLayerSource::User);
        assert!(merged.loadouts["same"].1.default.model.is_none());
    }

    #[test]
    fn one_model_route_key_cannot_name_two_endpoint_identities() {
        let mut merged = merged();
        let loadout = &mut merged.loadouts.get_mut("recommended").unwrap().1;
        loadout.default.model = Some("model-d".into());
        loadout.default.allow_unverified_model = Some(true);
        loadout.routes.simulation = Some(RoutePatch {
            inference_base_url: Some("https://example.test/other-prefix".into()),
            ..Default::default()
        });
        let error = resolve_inference_snapshot(
            1,
            &registry(),
            &merged,
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("use distinct provider/endpoint IDs"),
            "{error}"
        );
    }

    #[test]
    fn allow_unverified_cannot_bypass_explicit_transport_incompatibility() {
        let mut merged = merged();
        merged
            .loadouts
            .get_mut("recommended")
            .unwrap()
            .1
            .default
            .allow_unverified_model = Some(true);
        let registry = registry();
        let availability = CATEGORIES
            .iter()
            .map(|category| {
                let preset = recommended_route(&registry["ollama"], category).unwrap();
                (
                    ModelRouteKey {
                        provider_id: "ollama".into(),
                        endpoint_id: preset.endpoint.clone(),
                        model_id: preset.model.clone(),
                    },
                    Availability::Incompatible,
                )
            })
            .collect();
        let error = resolve_inference_snapshot(
            1,
            &registry,
            &merged,
            &RoutingOverrideSet::default(),
            &availability,
            &BTreeMap::new(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("explicitly incompatible"),
            "{error}"
        );
    }
}
