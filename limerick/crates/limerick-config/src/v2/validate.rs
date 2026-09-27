use thiserror::Error;
use url::Url;

use super::*;

const VALID_SUBROLES: &[&str] = &[
    "dialogue",
    "intent",
    "arrival-reaction",
    "message-reaction",
    "travel-encounter",
    "tier2-simulation",
    "tier3-simulation",
    "demo-player",
];

#[derive(Debug, Error, PartialEq)]
pub enum ConfigV2Error {
    #[error("unsupported configuration schema version {found}; expected 2")]
    Version { found: u8 },
    #[error("invalid {field}: {message}")]
    Invalid { field: String, message: String },
}

pub fn validate_project_config(config: &ProjectConfigV2) -> Result<(), ConfigV2Error> {
    validate_version(config.schema_version)?;
    validate_inference_layer(&config.inference, false, &Default::default())
}

pub fn validate_user_config(config: &UserConfigV2) -> Result<(), ConfigV2Error> {
    validate_version(config.schema_version)?;
    validate_inference_layer(&config.inference, true, &config.credential_bindings)?;
    for (slug, binding) in &config.credential_bindings {
        let Some(custom_slug) = slug.strip_prefix("custom:") else {
            return invalid(
                format!("credential_bindings.{slug}"),
                "credential bindings are custom-only and must use custom:<slug>",
            );
        };
        validate_slug(custom_slug, "credential_bindings")?;
        if let Some(env) = &binding.env
            && !valid_env_name(env)
        {
            return invalid(
                format!("credential_bindings.{slug}.env"),
                "must match [A-Z][A-Z0-9_]*",
            );
        }
        {
            let Some(provider) = config.inference.providers.get(custom_slug) else {
                return invalid(
                    format!("credential_bindings.{slug}"),
                    "must reference a custom provider defined in the same user file",
                );
            };
            if provider
                .endpoints
                .values()
                .all(|endpoint| endpoint.auth_adapter == AuthAdapter::None)
            {
                return invalid(
                    format!("credential_bindings.{slug}"),
                    "is forbidden because every endpoint is keyless",
                );
            }
        }
    }
    Ok(())
}

pub fn validate_provider_registry(
    registry: &std::collections::BTreeMap<String, ProviderDefinition>,
) -> Result<(), ConfigV2Error> {
    for (provider_id, provider) in registry {
        validate_slug(provider_id, "provider_registry")?;
        if provider.id != *provider_id
            || !provider.endpoints.contains_key(&provider.default_endpoint)
        {
            return invalid(
                format!("provider_registry.{provider_id}"),
                "provider id and default endpoint must be internally consistent",
            );
        }
        for (endpoint_id, endpoint) in &provider.endpoints {
            if endpoint.inference_adapter != InferenceAdapter::Simulator
                && !endpoint.inference_base_url.is_empty()
            {
                validate_url(
                    &endpoint.inference_base_url,
                    matches!(endpoint.backend_kind, BackendKind::Local { .. }),
                    &format!(
                        "provider_registry.{provider_id}.endpoints.{endpoint_id}.inference_base_url"
                    ),
                )?;
            }
            if endpoint.auth_adapter != AuthAdapter::None && endpoint.credential_slot.is_none() {
                return invalid(
                    format!(
                        "provider_registry.{provider_id}.endpoints.{endpoint_id}.credential_slot"
                    ),
                    "authenticated endpoints require a credential slot",
                );
            }
            let custom_view = CustomEndpointDefinition {
                inference_base_url: endpoint.inference_base_url.clone(),
                discovery_base_url: endpoint.discovery_base_url.clone(),
                inference_adapter: endpoint.inference_adapter,
                discovery_adapter: endpoint.discovery_adapter,
                auth_adapter: endpoint.auth_adapter,
                default_reasoning_dialect: endpoint.default_reasoning_dialect,
                allow_insecure_http: matches!(endpoint.backend_kind, BackendKind::Local { .. }),
                default_openai_generation_wire: endpoint.default_openai_generation_wire.clone(),
            };
            if endpoint.inference_adapter != InferenceAdapter::Simulator {
                validate_adapter_matrix(
                    &custom_view,
                    &format!("provider_registry.{provider_id}.endpoints.{endpoint_id}"),
                )?;
            }
            if let Some(models) = provider.curated_models.get(endpoint_id) {
                for (model_id, model) in models {
                    if model.recommended
                        && (!model.qualified || model.qualification_receipt.is_none())
                    {
                        return invalid(
                            format!("provider_registry.{provider_id}.{endpoint_id}.{model_id}"),
                            "recommended routes require a qualification receipt",
                        );
                    }
                    if model.qualified != model.qualification_receipt.is_some() {
                        return invalid(
                            format!(
                                "provider_registry.{provider_id}.{endpoint_id}.{model_id}.qualification_receipt"
                            ),
                            "qualified state and receipt must agree",
                        );
                    }
                    validate_declared_capabilities(
                        &UserModelCapabilityOverride {
                            reasoning: Some(model.reasoning.clone()),
                            generation: Some(model.generation.clone()),
                            output_contracts: Some(model.output_contracts.clone()),
                            openai_generation_wire: model.openai_generation_wire.clone(),
                        },
                        &custom_view,
                        &format!("provider_registry.{provider_id}.{endpoint_id}.{model_id}"),
                    )?;
                }
            }
        }
        for (preset_id, preset) in &provider.presets {
            for route in [
                &preset.dialogue,
                &preset.simulation,
                &preset.intent,
                &preset.reaction,
            ]
            .into_iter()
            .flatten()
            {
                if !provider.endpoints.contains_key(&route.endpoint) {
                    return invalid(
                        format!("provider_registry.{provider_id}.presets.{preset_id}"),
                        "preset references an unknown endpoint",
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_version(version: u8) -> Result<(), ConfigV2Error> {
    if version == INFERENCE_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(ConfigV2Error::Version { found: version })
    }
}

fn validate_inference_layer(
    layer: &InferenceLayer,
    user_layer: bool,
    bindings: &std::collections::BTreeMap<String, CredentialBinding>,
) -> Result<(), ConfigV2Error> {
    for (name, loadout) in &layer.loadouts {
        if name == MANAGED_OLLAMA_LOADOUT {
            if !user_layer || loadout.managed_by != Some(ManagedLoadoutOwner::OllamaSetupV1) {
                return invalid(
                    format!("inference.loadouts.{name}"),
                    "reserved setup loadout requires managed_by = ollama-setup-v1 in user config",
                );
            }
        } else {
            validate_name(name, "inference.loadouts")?;
        }
        validate_route_patch(
            &loadout.default,
            &format!("inference.loadouts.{name}.default"),
        )?;
        for (subrole, patch) in &loadout.subroles {
            if !VALID_SUBROLES.contains(&subrole.as_str()) {
                return invalid(
                    format!("inference.loadouts.{name}.subroles.{subrole}"),
                    "unknown inference subrole",
                );
            }
            validate_generation_patch(
                patch,
                &format!("inference.loadouts.{name}.subroles.{subrole}"),
            )?;
        }
        for (category, route) in [
            ("dialogue", loadout.routes.dialogue.as_ref()),
            ("simulation", loadout.routes.simulation.as_ref()),
            ("intent", loadout.routes.intent.as_ref()),
            ("reaction", loadout.routes.reaction.as_ref()),
        ] {
            if let Some(route) = route {
                validate_route_patch(
                    route,
                    &format!("inference.loadouts.{name}.routes.{category}"),
                )?;
            }
        }
    }
    for (slug, provider) in &layer.providers {
        validate_slug(slug, "inference.providers")?;
        if provider.endpoints.is_empty() {
            return invalid(
                format!("inference.providers.{slug}.endpoints"),
                "must contain at least one endpoint",
            );
        }
        let Some(default) = &provider.default_endpoint else {
            return invalid(
                format!("inference.providers.{slug}.default_endpoint"),
                "is required; endpoint selection must not depend on map order",
            );
        };
        if !provider.endpoints.contains_key(default) {
            return invalid(
                format!("inference.providers.{slug}.default_endpoint"),
                "must name an endpoint in the same provider",
            );
        }
        for (id, endpoint) in &provider.endpoints {
            validate_name(id, &format!("inference.providers.{slug}.endpoints"))?;
            validate_custom_endpoint(endpoint, slug, id)?;
            if !user_layer && endpoint.auth_adapter != AuthAdapter::None {
                return invalid(
                    format!("inference.providers.{slug}.endpoints.{id}.auth_adapter"),
                    "authenticated custom providers must be pinned in user config",
                );
            }
        }
        for (endpoint_id, models) in &provider.models {
            if !provider.endpoints.contains_key(endpoint_id) {
                return invalid(
                    format!("inference.providers.{slug}.models.{endpoint_id}"),
                    "must name an endpoint in the same provider",
                );
            }
            for (model_id, capability) in models {
                let field = format!("inference.providers.{slug}.models.{endpoint_id}.{model_id}");
                if capability.reasoning.is_none()
                    || capability.generation.is_none()
                    || capability.output_contracts.is_none()
                {
                    return invalid(
                        field,
                        "custom model declarations must include reasoning, generation, and output_contracts",
                    );
                }
                validate_declared_capabilities(
                    capability,
                    &provider.endpoints[endpoint_id],
                    &field,
                )?;
            }
        }
        if user_layer
            && provider
                .endpoints
                .values()
                .any(|endpoint| endpoint.auth_adapter != AuthAdapter::None)
            && !bindings.contains_key(&format!("custom:{slug}"))
        {
            return invalid(
                format!("inference.providers.{slug}"),
                "authenticated custom provider requires a user credential binding",
            );
        }
    }
    Ok(())
}

fn validate_declared_capabilities(
    capability: &UserModelCapabilityOverride,
    endpoint: &CustomEndpointDefinition,
    field: &str,
) -> Result<(), ConfigV2Error> {
    let generation = capability
        .generation
        .as_ref()
        .expect("caller checked complete declaration");
    if generation.min_output_tokens == 0
        || generation.min_output_tokens > generation.max_output_tokens
        || generation.max_output_tokens > 65_536
    {
        return invalid(
            format!("{field}.generation"),
            "output-token range must be ordered within 1..=65536",
        );
    }
    for (name, range) in [
        ("temperature", generation.temperature),
        ("frequency_penalty", generation.frequency_penalty),
    ] {
        if let Some(range) = range
            && (!range.min.is_finite() || !range.max.is_finite() || range.min > range.max)
        {
            return invalid(
                format!("{field}.generation.{name}"),
                "range bounds must be finite and ordered",
            );
        }
    }
    let reasoning = capability.reasoning.as_ref().expect("complete declaration");
    if reasoning.mandatory && reasoning.effort.is_none() && reasoning.budget.is_none() {
        return invalid(
            format!("{field}.reasoning"),
            "mandatory reasoning needs an effort or budget capability",
        );
    }
    if let Some(effort) = &reasoning.effort {
        if effort.supported_levels.is_empty()
            || !dialect_matches_adapter(effort.dialect, endpoint.inference_adapter)
        {
            return invalid(
                format!("{field}.reasoning.effort"),
                "effort levels and dialect must match the inference adapter",
            );
        }
        if endpoint.inference_adapter == InferenceAdapter::OpenaiResponsesV1
            && effort.supported_levels.contains(&ReasoningEffortV2::Max)
        {
            return invalid(
                format!("{field}.reasoning.effort.supported_levels"),
                "OpenAI Responses supports none/minimal/low/medium/high/xhigh, not max",
            );
        }
        for (subrole, levels) in &effort.minimum_total_output_tokens {
            if !VALID_SUBROLES.contains(&subrole.as_str()) {
                return invalid(
                    format!("{field}.reasoning.effort.minimum_total_output_tokens.{subrole}"),
                    "unknown subrole",
                );
            }
            for (level, minimum) in levels {
                if !effort.supported_levels.contains(level)
                    || *minimum == 0
                    || *minimum > generation.max_output_tokens
                {
                    return invalid(
                        format!("{field}.reasoning.effort.minimum_total_output_tokens.{subrole}"),
                        "headroom must reference a supported level within the generation ceiling",
                    );
                }
            }
        }
    }
    if let Some(budget) = &reasoning.budget
        && (budget.min_tokens == 0
            || budget.min_tokens > budget.max_tokens
            || budget
                .max_tokens
                .saturating_add(budget.min_visible_output_headroom)
                > generation.max_output_tokens
            || !dialect_matches_adapter(budget.dialect, endpoint.inference_adapter))
    {
        return invalid(
            format!("{field}.reasoning.budget"),
            "budget range, headroom, and dialect must fit the model generation contract",
        );
    }
    if reasoning
        .off_dialect
        .is_some_and(|dialect| !dialect_matches_adapter(dialect, endpoint.inference_adapter))
    {
        return invalid(
            format!("{field}.reasoning.off_dialect"),
            "dialect does not match the inference adapter",
        );
    }
    for (subrole, intent) in &reasoning.default_by_subrole {
        if !VALID_SUBROLES.contains(&subrole.as_str()) {
            return invalid(
                format!("{field}.reasoning.default_by_subrole.{subrole}"),
                "unknown subrole",
            );
        }
        validate_reasoning_intent(intent, reasoning, generation, field)?;
        if reasoning.mandatory && matches!(intent, ReasoningIntent::Off) {
            return invalid(
                format!("{field}.reasoning.default_by_subrole.{subrole}"),
                "mandatory reasoning cannot default off",
            );
        }
    }
    let mut rule_ids = std::collections::BTreeSet::new();
    for translation in &reasoning.translations {
        if translation.rule_id.is_empty() || !rule_ids.insert(&translation.rule_id) {
            return invalid(
                format!("{field}.reasoning.translations"),
                "translation rule IDs must be non-empty and unique",
            );
        }
        validate_reasoning_intent(&translation.to, reasoning, generation, field)?;
    }
    for (intent, wire) in &generation.service_tiers {
        let valid = endpoint.inference_adapter == InferenceAdapter::GoogleInteractionsV1
            && matches!(
                (intent, wire),
                (ServiceTierIntent::Standard, WireServiceTier::GoogleStandard)
                    | (ServiceTierIntent::Priority, WireServiceTier::GooglePriority)
            );
        if !valid {
            return invalid(
                format!("{field}.generation.service_tiers"),
                "service tiers are a Google-only closed wire mapping",
            );
        }
    }
    let outputs = capability
        .output_contracts
        .as_ref()
        .expect("complete declaration");
    if !outputs.prompt_validated_json {
        return invalid(
            format!("{field}.output_contracts.prompt_validated_json"),
            "must be true; every text adapter retains the local prompt-and-parser fallback",
        );
    }
    if endpoint.inference_adapter == InferenceAdapter::OpenaiChatV1 {
        let wire = capability
            .openai_generation_wire
            .as_ref()
            .or(endpoint.default_openai_generation_wire.as_ref())
            .ok_or_else(|| ConfigV2Error::Invalid {
                field: format!("{field}.openai_generation_wire"),
                message: "OpenAI-compatible models require an explicit wire contract".into(),
            })?;
        for (enabled, mode) in [
            (
                outputs.prompt_validated_json,
                StructuredOutputMode::PromptValidatedJson,
            ),
            (outputs.native_json_object, StructuredOutputMode::JsonObject),
            (outputs.native_json_schema, StructuredOutputMode::JsonSchema),
        ] {
            if enabled && !wire.structured_output.contains(&mode) {
                return invalid(
                    format!("{field}.output_contracts"),
                    "declared output support exceeds the endpoint wire contract",
                );
            }
        }
    } else if capability.openai_generation_wire.is_some() {
        return invalid(
            format!("{field}.openai_generation_wire"),
            "is only valid for the OpenAI-compatible adapter",
        );
    }
    Ok(())
}

fn validate_reasoning_intent(
    intent: &ReasoningIntent,
    reasoning: &ReasoningCapabilities,
    generation: &GenerationCapabilities,
    field: &str,
) -> Result<(), ConfigV2Error> {
    match intent {
        ReasoningIntent::Auto => Ok(()),
        ReasoningIntent::Off if !reasoning.mandatory => Ok(()),
        ReasoningIntent::Off => invalid(
            format!("{field}.reasoning"),
            "mandatory reasoning cannot be disabled",
        ),
        ReasoningIntent::Effort { level }
            if reasoning
                .effort
                .as_ref()
                .is_some_and(|value| value.supported_levels.contains(level)) =>
        {
            Ok(())
        }
        ReasoningIntent::Budget { tokens }
            if reasoning.budget.as_ref().is_some_and(|value| {
                (value.min_tokens..=value.max_tokens).contains(tokens)
                    && tokens.saturating_add(value.min_visible_output_headroom)
                        <= generation.max_output_tokens
            }) =>
        {
            Ok(())
        }
        _ => invalid(
            format!("{field}.reasoning"),
            "intent is not supported by the declared capability",
        ),
    }
}

fn dialect_matches_adapter(dialect: ReasoningDialect, adapter: InferenceAdapter) -> bool {
    match adapter {
        InferenceAdapter::OpenaiResponsesV1 => {
            matches!(
                dialect,
                ReasoningDialect::None | ReasoningDialect::OpenaiResponsesEffort
            )
        }
        InferenceAdapter::OpenaiChatV1 => matches!(
            dialect,
            ReasoningDialect::None
                | ReasoningDialect::OpenaiChatEffort
                | ReasoningDialect::OpenrouterReasoning
                | ReasoningDialect::DeepseekThinking
                | ReasoningDialect::LocalTemplateToggle
        ),
        InferenceAdapter::AnthropicMessages2023_06_01 => matches!(
            dialect,
            ReasoningDialect::None
                | ReasoningDialect::AnthropicAdaptive
                | ReasoningDialect::AnthropicManualBudget
        ),
        InferenceAdapter::GoogleInteractionsV1 => {
            matches!(dialect, ReasoningDialect::GoogleThinkingLevel)
        }
        InferenceAdapter::Simulator => dialect == ReasoningDialect::None,
    }
}

fn validate_route_patch(route: &RoutePatch, field: &str) -> Result<(), ConfigV2Error> {
    if let Some(url) = &route.inference_base_url {
        validate_url(url, false, &format!("{field}.inference_base_url"))?;
    }
    if let Some(url) = &route.discovery_base_url {
        validate_url(url, false, &format!("{field}.discovery_base_url"))?;
    }
    validate_generation_values(
        route.max_output_tokens,
        route.temperature,
        route.frequency_penalty,
        field,
    )
}

fn validate_generation_patch(patch: &GenerationPatch, field: &str) -> Result<(), ConfigV2Error> {
    validate_generation_values(
        patch.max_output_tokens,
        patch.temperature,
        patch.frequency_penalty,
        field,
    )
}

fn validate_generation_values(
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    frequency_penalty: Option<f32>,
    field: &str,
) -> Result<(), ConfigV2Error> {
    if let Some(value) = max_tokens
        && !(1..=65_536).contains(&value)
    {
        return invalid(
            format!("{field}.max_output_tokens"),
            "must be between 1 and 65536",
        );
    }
    if let Some(value) = temperature
        && (!value.is_finite() || !(0.0..=2.0).contains(&value))
    {
        return invalid(
            format!("{field}.temperature"),
            "must be finite and between 0 and 2",
        );
    }
    if let Some(value) = frequency_penalty
        && (!value.is_finite() || !(-2.0..=2.0).contains(&value))
    {
        return invalid(
            format!("{field}.frequency_penalty"),
            "must be finite and between -2 and 2",
        );
    }
    Ok(())
}

fn validate_custom_endpoint(
    endpoint: &CustomEndpointDefinition,
    slug: &str,
    id: &str,
) -> Result<(), ConfigV2Error> {
    let prefix = format!("inference.providers.{slug}.endpoints.{id}");
    let inference_url = validate_url(
        &endpoint.inference_base_url,
        endpoint.allow_insecure_http,
        &format!("{prefix}.inference_base_url"),
    )?;
    if let Some(discovery) = &endpoint.discovery_base_url {
        validate_url(
            discovery,
            endpoint.allow_insecure_http,
            &format!("{prefix}.discovery_base_url"),
        )?;
    }
    if inference_url.scheme() == "http"
        && !is_loopback_host(inference_url.host_str())
        && !endpoint.allow_insecure_http
    {
        return invalid(
            format!("{prefix}.allow_insecure_http"),
            "must be true for non-loopback HTTP",
        );
    }
    validate_adapter_matrix(endpoint, &prefix)
}

fn validate_adapter_matrix(
    endpoint: &CustomEndpointDefinition,
    prefix: &str,
) -> Result<(), ConfigV2Error> {
    let valid = match endpoint.inference_adapter {
        InferenceAdapter::OpenaiResponsesV1 => {
            matches!(
                endpoint.auth_adapter,
                AuthAdapter::None | AuthAdapter::Bearer
            ) && matches!(
                endpoint.discovery_adapter,
                DiscoveryAdapter::None | DiscoveryAdapter::OpenaiModelsV1
            ) && matches!(
                endpoint.default_reasoning_dialect,
                ReasoningDialect::None | ReasoningDialect::OpenaiResponsesEffort
            ) && endpoint.default_openai_generation_wire.is_none()
        }
        InferenceAdapter::OpenaiChatV1 => {
            matches!(
                endpoint.auth_adapter,
                AuthAdapter::None | AuthAdapter::Bearer
            ) && matches!(
                endpoint.discovery_adapter,
                DiscoveryAdapter::None
                    | DiscoveryAdapter::OpenaiModelsV1
                    | DiscoveryAdapter::OpenrouterModelsV1
                    | DiscoveryAdapter::OllamaTagsV1
            ) && matches!(
                endpoint.default_reasoning_dialect,
                ReasoningDialect::None
                    | ReasoningDialect::OpenaiChatEffort
                    | ReasoningDialect::OpenrouterReasoning
                    | ReasoningDialect::DeepseekThinking
                    | ReasoningDialect::LocalTemplateToggle
            ) && endpoint.default_openai_generation_wire.is_some()
        }
        InferenceAdapter::AnthropicMessages2023_06_01 => {
            endpoint.auth_adapter == AuthAdapter::AnthropicKey
                && matches!(
                    endpoint.discovery_adapter,
                    DiscoveryAdapter::None | DiscoveryAdapter::AnthropicModelsV1
                )
                && matches!(
                    endpoint.default_reasoning_dialect,
                    ReasoningDialect::None
                        | ReasoningDialect::AnthropicAdaptive
                        | ReasoningDialect::AnthropicManualBudget
                )
                && endpoint.default_openai_generation_wire.is_none()
        }
        InferenceAdapter::GoogleInteractionsV1 => {
            endpoint.auth_adapter == AuthAdapter::GoogleKey
                && matches!(
                    endpoint.discovery_adapter,
                    DiscoveryAdapter::None | DiscoveryAdapter::GoogleModelsV1Beta
                )
                && endpoint.default_reasoning_dialect == ReasoningDialect::GoogleThinkingLevel
                && endpoint.default_openai_generation_wire.is_none()
        }
        InferenceAdapter::Simulator => false,
    };
    if valid {
        Ok(())
    } else {
        invalid(prefix.to_string(), "unsupported adapter combination")
    }
}

fn validate_url(value: &str, allow_insecure: bool, field: &str) -> Result<Url, ConfigV2Error> {
    let parsed = Url::parse(value).map_err(|error| ConfigV2Error::Invalid {
        field: field.to_string(),
        message: error.to_string(),
    })?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return invalid(field.to_string(), "userinfo is forbidden");
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return invalid(field.to_string(), "query and fragment are forbidden");
    }
    match parsed.scheme() {
        "https" => {}
        "http" if is_loopback_host(parsed.host_str()) || allow_insecure => {}
        _ => return invalid(field.to_string(), "HTTPS is required outside loopback"),
    }
    Ok(parsed)
}

fn is_loopback_host(host: Option<&str>) -> bool {
    matches!(host, Some("localhost" | "127.0.0.1" | "::1"))
}

fn validate_slug(value: &str, field: &str) -> Result<(), ConfigV2Error> {
    if value.is_empty() || value.len() > 63 {
        return invalid(field.to_string(), "slug length must be 1..=63");
    }
    let mut chars = value.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_lowercase())
        || !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
    {
        return invalid(field.to_string(), "slug must match [a-z][a-z0-9_-]{0,62}");
    }
    Ok(())
}

fn validate_name(value: &str, field: &str) -> Result<(), ConfigV2Error> {
    validate_slug(value, field)
}

fn valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn invalid<T>(field: String, message: impl Into<String>) -> Result<T, ConfigV2Error> {
    Err(ConfigV2Error::Invalid {
        field,
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EngineConfig;
    use std::collections::{BTreeMap, BTreeSet};

    fn openai_endpoint() -> CustomEndpointDefinition {
        CustomEndpointDefinition {
            inference_base_url: "https://example.test/v1".into(),
            discovery_base_url: Some("https://example.test/v1".into()),
            inference_adapter: InferenceAdapter::OpenaiChatV1,
            discovery_adapter: DiscoveryAdapter::OpenaiModelsV1,
            auth_adapter: AuthAdapter::None,
            default_reasoning_dialect: ReasoningDialect::None,
            allow_insecure_http: false,
            default_openai_generation_wire: Some(OpenAiChatGenerationWire {
                output_limit_field: OutputLimitField::MaxTokens,
                structured_output: BTreeSet::from([StructuredOutputMode::PromptValidatedJson]),
            }),
        }
    }

    #[test]
    fn rejects_wrong_version() {
        let config = UserConfigV2 {
            schema_version: 1,
            ..Default::default()
        };
        assert_eq!(
            validate_user_config(&config),
            Err(ConfigV2Error::Version { found: 1 })
        );
    }

    #[test]
    fn rejects_authenticated_project_custom_provider() {
        let mut endpoint = openai_endpoint();
        endpoint.auth_adapter = AuthAdapter::Bearer;
        let config = ProjectConfigV2 {
            schema_version: 2,
            engine: EngineConfig::default(),
            inference: InferenceLayer {
                providers: BTreeMap::from([(
                    "acme".into(),
                    CustomProviderDefinition {
                        display_name: "Acme".into(),
                        default_endpoint: Some("chat".into()),
                        endpoints: BTreeMap::from([("chat".into(), endpoint)]),
                        models: BTreeMap::new(),
                    },
                )]),
                ..Default::default()
            },
        };
        assert!(validate_project_config(&config).is_err());
    }

    #[test]
    fn user_binding_and_provider_validate_together() {
        let mut endpoint = openai_endpoint();
        endpoint.auth_adapter = AuthAdapter::Bearer;
        let config = UserConfigV2 {
            schema_version: 2,
            inference: InferenceLayer {
                providers: BTreeMap::from([(
                    "acme".into(),
                    CustomProviderDefinition {
                        display_name: "Acme".into(),
                        default_endpoint: Some("chat".into()),
                        endpoints: BTreeMap::from([("chat".into(), endpoint)]),
                        models: BTreeMap::new(),
                    },
                )]),
                ..Default::default()
            },
            credential_bindings: BTreeMap::from([(
                "custom:acme".into(),
                CredentialBinding {
                    env: Some("ACME_API_KEY".into()),
                },
            )]),
        };
        validate_user_config(&config).unwrap();
    }

    #[test]
    fn route_generation_ranges_are_strict() {
        let route = RoutePatch {
            max_output_tokens: Some(0),
            ..Default::default()
        };
        assert!(validate_route_patch(&route, "route").is_err());
    }
}
