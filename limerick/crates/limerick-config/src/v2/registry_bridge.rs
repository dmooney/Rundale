use std::collections::{BTreeMap, BTreeSet};

use crate::{InferenceCategory, InferenceSubrole, Provider, ProviderKind, registry};

use super::*;

pub fn compiled_inference_layer_v2() -> InferenceLayer {
    InferenceLayer {
        active_loadout: Some("default".into()),
        loadouts: BTreeMap::from([
            (
                "default".into(),
                LoadoutDefinition {
                    default: RoutePatch {
                        // A fresh installation must reach onboarding without a
                        // network credential. Setup transactionally replaces
                        // this process-local, keyless route after the user
                        // chooses a provider.
                        provider: Some("simulator".into()),
                        model: Some("simulator".into()),
                        allow_unverified_model: Some(true),
                        ..RoutePatch::default()
                    },
                    ..LoadoutDefinition::default()
                },
            ),
            (
                MANAGED_OLLAMA_LOADOUT.into(),
                LoadoutDefinition {
                    default: RoutePatch {
                        provider: Some("ollama".into()),
                        allow_unverified_model: Some(true),
                        ..RoutePatch::default()
                    },
                    managed_by: Some(ManagedLoadoutOwner::OllamaSetupV1),
                    ..LoadoutDefinition::default()
                },
            ),
        ]),
        providers: BTreeMap::new(),
    }
}

/// Projects installed provider endpoint metadata into v2. A preset is only a
/// route declaration; it does not by itself fabricate qualification evidence.
pub fn compiled_provider_registry_v2() -> BTreeMap<String, ProviderDefinition> {
    registry()
        .all()
        .into_iter()
        .filter(|provider| provider.id() != "custom")
        .map(|provider| (provider.id().to_string(), convert_provider(&provider)))
        .collect()
}

fn convert_provider(provider: &Provider) -> ProviderDefinition {
    let default_endpoint = if provider.id() == "openai" {
        "responses".to_string()
    } else {
        "default".to_string()
    };
    let mut default_definition = endpoint_for(provider, false);
    if provider.id() == "openai" {
        default_definition.inference_adapter = InferenceAdapter::OpenaiResponsesV1;
        default_definition.default_reasoning_dialect = ReasoningDialect::OpenaiResponsesEffort;
        default_definition.default_openai_generation_wire = None;
    }
    let mut endpoints = BTreeMap::from([(default_endpoint.clone(), default_definition)]);
    if provider.id() == "openai" {
        endpoints.insert("chat".into(), endpoint_for(provider, false));
    }
    if provider.id() == "opencode" {
        endpoints.insert("messages".into(), endpoint_for(provider, true));
    }

    let presets = provider
        .presets()
        .iter()
        .map(|preset| {
            let route = |category| {
                preset.model(category).map(|model| PresetRoute {
                    endpoint: default_endpoint.clone(),
                    model: model.to_string(),
                })
            };
            (
                preset.key.clone(),
                ProviderPresetV2 {
                    dialogue: route(InferenceCategory::Dialogue),
                    simulation: route(InferenceCategory::Simulation),
                    intent: route(InferenceCategory::Intent),
                    reaction: route(InferenceCategory::Reaction),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let mut curated_models: BTreeMap<String, BTreeMap<String, CuratedModelRoute>> = BTreeMap::new();
    for preset in provider.presets() {
        for category in InferenceCategory::ALL {
            if let Some(model) = preset.model(category) {
                curated_models
                    .entry(default_endpoint.clone())
                    .or_default()
                    .entry(model.to_string())
                    .or_insert_with(|| curated_model(provider, model));
                if provider.id() == "openai" {
                    curated_models
                        .entry("chat".into())
                        .or_default()
                        .entry(model.to_string())
                        .or_insert_with(|| openai_chat_curated_model(provider, model));
                }
            }
        }
    }
    if provider.id() == "opencode" {
        curated_models.entry("messages".into()).or_default().insert(
            "qwen3.7-max".into(),
            curated_model_for_endpoint(provider, "qwen3.7-max", true),
        );
    }

    ProviderDefinition {
        id: provider.id().to_string(),
        display_name: provider.display_name().to_string(),
        aliases: provider.0.aliases.clone(),
        default_endpoint,
        recommended_preset: provider.0.recommended_preset.clone(),
        endpoints,
        presets,
        curated_models,
    }
}

fn endpoint_for(provider: &Provider, messages: bool) -> EndpointDefinition {
    let kind = provider.kind();
    let inference_adapter = if messages {
        InferenceAdapter::AnthropicMessages2023_06_01
    } else {
        match kind {
            ProviderKind::Anthropic => InferenceAdapter::AnthropicMessages2023_06_01,
            ProviderKind::Google => InferenceAdapter::GoogleInteractionsV1,
            ProviderKind::Simulator => InferenceAdapter::Simulator,
            ProviderKind::OpenAiCompat | ProviderKind::Local => InferenceAdapter::OpenaiChatV1,
        }
    };
    let discovery_adapter = if messages || kind == ProviderKind::Anthropic {
        DiscoveryAdapter::AnthropicModelsV1
    } else if kind == ProviderKind::Google {
        DiscoveryAdapter::GoogleModelsV1Beta
    } else if provider.id() == "openrouter" {
        DiscoveryAdapter::OpenrouterModelsV1
    } else if provider.id() == "ollama" {
        DiscoveryAdapter::OllamaTagsV1
    } else if matches!(kind, ProviderKind::OpenAiCompat | ProviderKind::Local) {
        DiscoveryAdapter::OpenaiModelsV1
    } else {
        DiscoveryAdapter::None
    };
    let auth_adapter = if messages || kind == ProviderKind::Anthropic {
        AuthAdapter::AnthropicKey
    } else if kind == ProviderKind::Google {
        AuthAdapter::GoogleKey
    } else if provider.requires_api_key() {
        AuthAdapter::Bearer
    } else {
        AuthAdapter::None
    };
    let backend_kind = match provider.id() {
        "ollama" => BackendKind::Local {
            runtime: LocalRuntime::Ollama,
        },
        "lmstudio" => BackendKind::Local {
            runtime: LocalRuntime::LmStudio,
        },
        "vllm" => BackendKind::Local {
            runtime: LocalRuntime::Vllm,
        },
        "vllmmlx" => BackendKind::Local {
            runtime: LocalRuntime::VllmMlx,
        },
        _ if kind == ProviderKind::Local => BackendKind::Local {
            runtime: LocalRuntime::Generic,
        },
        _ if kind == ProviderKind::Simulator => BackendKind::Simulator,
        _ => BackendKind::Remote,
    };
    let management_adapter = match provider.id() {
        "ollama" => ManagementAdapter::Ollama,
        _ if kind == ProviderKind::Local => ManagementAdapter::External,
        _ => ManagementAdapter::None,
    };
    let base = api_prefix(provider, messages);
    let discovery_base_url = match discovery_adapter {
        DiscoveryAdapter::None => None,
        DiscoveryAdapter::GoogleModelsV1Beta => {
            Some(base.trim_end_matches("/v1").to_string() + "/v1beta")
        }
        DiscoveryAdapter::OllamaTagsV1 => Some(base.trim_end_matches("/v1").to_string()),
        _ => Some(base.clone()),
    };
    EndpointDefinition {
        inference_base_url: base.clone(),
        discovery_base_url,
        inference_adapter,
        discovery_adapter,
        backend_kind,
        management_adapter,
        auth_adapter,
        credential_slot: provider
            .requires_api_key()
            .then(|| provider.id().to_string()),
        default_reasoning_dialect: reasoning_dialect(provider, messages),
        default_openai_generation_wire: (inference_adapter == InferenceAdapter::OpenaiChatV1).then(
            || OpenAiChatGenerationWire {
                output_limit_field: if provider.id() == "openai" {
                    OutputLimitField::MaxCompletionTokens
                } else {
                    OutputLimitField::MaxTokens
                },
                structured_output: if matches!(
                    provider.id(),
                    "openai" | "vllm" | "vllmmlx" | "lmstudio"
                ) {
                    BTreeSet::from([
                        StructuredOutputMode::PromptValidatedJson,
                        StructuredOutputMode::JsonObject,
                        StructuredOutputMode::JsonSchema,
                    ])
                } else {
                    BTreeSet::from([
                        StructuredOutputMode::PromptValidatedJson,
                        StructuredOutputMode::JsonObject,
                    ])
                },
            },
        ),
    }
}

fn api_prefix(provider: &Provider, messages: bool) -> String {
    let base = provider.default_base_url().trim_end_matches('/');
    if base.is_empty() {
        return String::new();
    }
    if messages && provider.id() == "opencode" {
        return base.trim_end_matches("/v1").to_string() + "/v1";
    }
    match provider.kind() {
        ProviderKind::Anthropic | ProviderKind::Google if !base.ends_with("/v1") => {
            format!("{base}/v1")
        }
        ProviderKind::OpenAiCompat | ProviderKind::Local
            if provider.id() != "github_models" && !base.ends_with("/v1") =>
        {
            format!("{base}/v1")
        }
        _ => base.to_string(),
    }
}

fn reasoning_dialect(provider: &Provider, messages: bool) -> ReasoningDialect {
    if messages {
        return ReasoningDialect::AnthropicAdaptive;
    }
    match provider.id() {
        "openai" => ReasoningDialect::OpenaiChatEffort,
        "openrouter" => ReasoningDialect::OpenrouterReasoning,
        "deepseek" => ReasoningDialect::DeepseekThinking,
        _ if provider.kind() == ProviderKind::Google => ReasoningDialect::GoogleThinkingLevel,
        _ if provider.kind() == ProviderKind::Anthropic => ReasoningDialect::AnthropicAdaptive,
        _ if provider.kind() == ProviderKind::Local => ReasoningDialect::LocalTemplateToggle,
        _ => ReasoningDialect::None,
    }
}

fn curated_model(provider: &Provider, model: &str) -> CuratedModelRoute {
    curated_model_for_endpoint(provider, model, false)
}

fn openai_chat_curated_model(provider: &Provider, model: &str) -> CuratedModelRoute {
    let mut route = curated_model_for_endpoint(provider, model, false);
    let dialect = ReasoningDialect::OpenaiChatEffort;
    if let Some(effort) = &mut route.reasoning.effort {
        effort.dialect = dialect;
    }
    route.reasoning.off_dialect = None;
    let wire = endpoint_for(provider, false)
        .default_openai_generation_wire
        .expect("OpenAI chat endpoint has an authored wire contract");
    route.output_contracts.native_json_object = wire
        .structured_output
        .contains(&StructuredOutputMode::JsonObject);
    route.output_contracts.native_json_schema = wire
        .structured_output
        .contains(&StructuredOutputMode::JsonSchema);
    route.openai_generation_wire = Some(wire);
    route
}

fn curated_model_for_endpoint(
    provider: &Provider,
    model: &str,
    messages: bool,
) -> CuratedModelRoute {
    let model_dialect = if provider.id() == "openai" && !messages {
        ReasoningDialect::OpenaiResponsesEffort
    } else {
        explicit_model_reasoning_dialect(provider.id(), model, messages)
            .unwrap_or(ReasoningDialect::None)
    };
    // This is authored per exact shipped ModelRouteKey. A provider-wide
    // dialect is only a serializer choice and is never capability evidence.
    let supported_levels = match (provider.id(), model, messages, model_dialect) {
        (
            "openai",
            "gpt-5.5" | "gpt-5.4-mini" | "gpt-5.4-nano",
            false,
            ReasoningDialect::OpenaiResponsesEffort | ReasoningDialect::OpenaiChatEffort,
        ) => BTreeSet::from([
            ReasoningEffortV2::Minimal,
            ReasoningEffortV2::Low,
            ReasoningEffortV2::Medium,
            ReasoningEffortV2::High,
            ReasoningEffortV2::Xhigh,
        ]),
        // Gateway dialect support is not model-route evidence. Keep Auto as
        // omission until a specific OpenRouter route is authored or probed.
        (_, _, _, ReasoningDialect::OpenrouterReasoning) => BTreeSet::new(),
        (
            "google",
            "gemini-3.6-flash" | "gemini-3.7-flash",
            false,
            ReasoningDialect::GoogleThinkingLevel,
        ) => BTreeSet::from([
            ReasoningEffortV2::Minimal,
            ReasoningEffortV2::Low,
            ReasoningEffortV2::Medium,
            ReasoningEffortV2::High,
        ]),
        ("anthropic", "claude-opus-4-7", false, ReasoningDialect::AnthropicAdaptive) => {
            BTreeSet::from([
                ReasoningEffortV2::Low,
                ReasoningEffortV2::Medium,
                ReasoningEffortV2::High,
                ReasoningEffortV2::Xhigh,
                ReasoningEffortV2::Max,
            ])
        }
        ("anthropic", "claude-sonnet-4-6", false, ReasoningDialect::AnthropicAdaptive) => {
            BTreeSet::from([
                ReasoningEffortV2::Low,
                ReasoningEffortV2::Medium,
                ReasoningEffortV2::High,
                ReasoningEffortV2::Max,
            ])
        }
        ("anthropic", "claude-haiku-4-5", false, ReasoningDialect::AnthropicAdaptive) => {
            BTreeSet::from([
                ReasoningEffortV2::Low,
                ReasoningEffortV2::Medium,
                ReasoningEffortV2::High,
            ])
        }
        (_, _, _, ReasoningDialect::AnthropicAdaptive) => BTreeSet::new(),
        _ => BTreeSet::new(),
    };
    let translations = if provider.kind() == ProviderKind::Google && model == "gemini-3.7-flash" {
        vec![ReasoningTranslation {
            rule_id: "gemini-3.7-flash-minimal-to-low".into(),
            from: ReasoningIntent::Effort {
                level: ReasoningEffortV2::Minimal,
            },
            to: ReasoningIntent::Effort {
                level: ReasoningEffortV2::Low,
            },
        }]
    } else {
        Vec::new()
    };
    let effort = (!supported_levels.is_empty()).then_some(EffortCapability {
        dialect: model_dialect,
        supported_levels,
        minimum_total_output_tokens: BTreeMap::new(),
    });
    let exact_google_route = provider.id() == "google"
        && matches!(model, "gemini-3.6-flash" | "gemini-3.7-flash")
        && !messages;
    let exact_openai_responses_route = provider.id() == "openai"
        && matches!(model, "gpt-5.5" | "gpt-5.4-mini" | "gpt-5.4-nano")
        && !messages;
    let defaults = InferenceSubrole::ALL
        .into_iter()
        .map(|subrole| {
            let legacy = crate::InferenceProfile::for_subrole(subrole);
            (
                subrole.name().to_string(),
                GenerationProfile {
                    max_output_tokens: legacy.max_output_tokens,
                    temperature: None,
                    frequency_penalty: None,
                    reasoning: if exact_google_route {
                        ReasoningIntent::Effort {
                            level: match legacy.thinking_level {
                                crate::ThinkingLevel::Minimal => ReasoningEffortV2::Minimal,
                                crate::ThinkingLevel::Low => ReasoningEffortV2::Low,
                                crate::ThinkingLevel::Medium => ReasoningEffortV2::Medium,
                                crate::ThinkingLevel::High => ReasoningEffortV2::High,
                            },
                        }
                    } else {
                        ReasoningIntent::Auto
                    },
                    service_tier: if exact_google_route {
                        ServiceTierIntent::Standard
                    } else {
                        ServiceTierIntent::Auto
                    },
                },
            )
        })
        .collect();
    let endpoint_wire = if provider.id() == "openai" && !messages {
        None
    } else {
        endpoint_for(provider, messages).default_openai_generation_wire
    };
    let output_contracts = OutputContractCapabilities {
        prompt_validated_json: true,
        native_json_object: exact_openai_responses_route
            || endpoint_wire.as_ref().is_some_and(|wire| {
                wire.structured_output
                    .contains(&StructuredOutputMode::JsonObject)
            }),
        native_json_schema: exact_openai_responses_route
            || endpoint_wire.as_ref().is_some_and(|wire| {
                wire.structured_output
                    .contains(&StructuredOutputMode::JsonSchema)
            }),
    };
    CuratedModelRoute {
        qualified: false,
        recommended: false,
        generation_defaults: defaults,
        reasoning: ReasoningCapabilities {
            mandatory: false,
            default_by_subrole: BTreeMap::new(),
            effort,
            budget: (model_dialect == ReasoningDialect::AnthropicManualBudget).then_some(
                BudgetCapability {
                    dialect: ReasoningDialect::AnthropicManualBudget,
                    min_tokens: 1_024,
                    max_tokens: 7_168,
                    min_visible_output_headroom: 1_024,
                },
            ),
            off_dialect: matches!(
                model_dialect,
                ReasoningDialect::OpenaiResponsesEffort
                    | ReasoningDialect::LocalTemplateToggle
                    | ReasoningDialect::OpenrouterReasoning
                    | ReasoningDialect::DeepseekThinking
                    | ReasoningDialect::AnthropicAdaptive
                    | ReasoningDialect::AnthropicManualBudget
            )
            .then_some(model_dialect),
            translations,
        },
        generation: GenerationCapabilities {
            min_output_tokens: 1,
            max_output_tokens: 8_192,
            temperature: None,
            frequency_penalty: None,
            service_tiers: if exact_google_route {
                BTreeMap::from([
                    (ServiceTierIntent::Standard, WireServiceTier::GoogleStandard),
                    (ServiceTierIntent::Priority, WireServiceTier::GooglePriority),
                ])
            } else {
                BTreeMap::new()
            },
        },
        output_contracts,
        openai_generation_wire: endpoint_wire,
        qualification_receipt: None,
    }
}

/// Exact curated model contracts. Unknown model IDs deliberately receive no
/// reasoning capability until discovery/probe evidence supplies one.
fn explicit_model_reasoning_dialect(
    provider_id: &str,
    model: &str,
    messages: bool,
) -> Option<ReasoningDialect> {
    match (provider_id, model, messages) {
        ("google", "gemini-3.7-flash" | "gemini-3.6-flash", false) => {
            Some(ReasoningDialect::GoogleThinkingLevel)
        }
        ("openai", "gpt-5.5" | "gpt-5.4-mini" | "gpt-5.4-nano", false) => {
            Some(ReasoningDialect::OpenaiChatEffort)
        }
        ("anthropic", "claude-haiku-4-5", false) => Some(ReasoningDialect::AnthropicManualBudget),
        ("anthropic", "claude-opus-4-7" | "claude-sonnet-4-6", false)
        | ("opencode", "qwen3.7-max", true) => Some(ReasoningDialect::AnthropicAdaptive),
        ("deepseek", "deepseek-v4-pro" | "deepseek-v4-flash", false) => {
            Some(ReasoningDialect::DeepseekThinking)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_four_network_api_flavors_and_simulator() {
        let registry = compiled_provider_registry_v2();
        assert_eq!(
            registry["openai"].endpoints["responses"].inference_adapter,
            InferenceAdapter::OpenaiResponsesV1
        );
        assert_eq!(
            registry["openai"].endpoints["chat"].inference_adapter,
            InferenceAdapter::OpenaiChatV1
        );
        assert_eq!(
            registry["anthropic"].endpoints["default"].inference_adapter,
            InferenceAdapter::AnthropicMessages2023_06_01
        );
        assert_eq!(
            registry["google"].endpoints["default"].inference_adapter,
            InferenceAdapter::GoogleInteractionsV1
        );
        assert_eq!(
            registry["simulator"].endpoints["default"].inference_adapter,
            InferenceAdapter::Simulator
        );
    }

    #[test]
    fn reasoning_capabilities_are_exact_per_shipped_model_route() {
        let registry = compiled_provider_registry_v2();
        let openai = registry["openai"].curated_models["responses"]["gpt-5.5"]
            .reasoning
            .effort
            .as_ref()
            .unwrap();
        assert!(openai.supported_levels.contains(&ReasoningEffortV2::Xhigh));
        assert!(!openai.supported_levels.contains(&ReasoningEffortV2::Max));

        let opus = registry["anthropic"].curated_models["default"]["claude-opus-4-7"]
            .reasoning
            .effort
            .as_ref()
            .unwrap();
        assert!(opus.supported_levels.contains(&ReasoningEffortV2::Xhigh));
        assert!(opus.supported_levels.contains(&ReasoningEffortV2::Max));
        let sonnet = registry["anthropic"].curated_models["default"]["claude-sonnet-4-6"]
            .reasoning
            .effort
            .as_ref()
            .unwrap();
        assert!(!sonnet.supported_levels.contains(&ReasoningEffortV2::Xhigh));
        assert!(sonnet.supported_levels.contains(&ReasoningEffortV2::Max));

        let gateway = &registry["openrouter"].curated_models["default"]["google/gemini-3.6-flash"];
        assert!(gateway.reasoning.effort.is_none());
    }

    #[test]
    fn opencode_qwen_max_uses_messages_endpoint() {
        let registry = compiled_provider_registry_v2();
        let opencode = &registry["opencode"];
        assert_eq!(
            opencode.endpoints["messages"].inference_adapter,
            InferenceAdapter::AnthropicMessages2023_06_01
        );
        assert!(opencode.curated_models["messages"].contains_key("qwen3.7-max"));
    }

    #[test]
    fn local_management_is_not_inferred_from_the_wire_protocol() {
        let registry = compiled_provider_registry_v2();
        assert_eq!(
            registry["ollama"].endpoints["default"].management_adapter,
            ManagementAdapter::Ollama
        );
        assert_eq!(
            registry["vllm"].endpoints["default"].management_adapter,
            ManagementAdapter::External
        );
    }

    #[test]
    fn compiled_loadouts_keep_managed_ollama_separate_from_keyless_setup_route() {
        let layer = compiled_inference_layer_v2();
        assert_eq!(layer.active_loadout.as_deref(), Some("default"));
        assert_eq!(
            layer.loadouts[MANAGED_OLLAMA_LOADOUT].managed_by,
            Some(ManagedLoadoutOwner::OllamaSetupV1)
        );
        assert_eq!(
            layer.loadouts["default"].default.provider.as_deref(),
            Some("simulator")
        );
        let merged = merge_inference_layers(
            &layer,
            &InferenceLayer::default(),
            &InferenceLayer::default(),
        );
        let snapshot = resolve_inference_snapshot(
            1,
            &compiled_provider_registry_v2(),
            &merged,
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        )
        .unwrap();
        assert_eq!(
            snapshot.category_routes["dialogue"].key.provider_id,
            "simulator"
        );
    }

    #[test]
    fn gemini_37_translation_is_explicit_and_preserves_requested_intent() {
        let compiled = compiled_inference_layer_v2();
        let project = InferenceLayer {
            active_loadout: Some("google".into()),
            loadouts: BTreeMap::from([(
                "google".into(),
                LoadoutDefinition {
                    default: RoutePatch {
                        provider: Some("google".into()),
                        model: Some("gemini-3.7-flash".into()),
                        allow_unverified_model: Some(true),
                        ..RoutePatch::default()
                    },
                    ..LoadoutDefinition::default()
                },
            )]),
            providers: BTreeMap::new(),
        };
        let merged = merge_inference_layers(&compiled, &project, &InferenceLayer::default());
        let snapshot = resolve_inference_snapshot(
            2,
            &compiled_provider_registry_v2(),
            &merged,
            &RoutingOverrideSet::default(),
            &BTreeMap::new(),
            &BTreeMap::from([("google".into(), SecretString::new("key".into()))]),
        )
        .unwrap();
        let dialogue = &snapshot.subrole_routes["dialogue"];
        assert_eq!(
            dialogue.requested_profile.reasoning,
            ReasoningIntent::Effort {
                level: ReasoningEffortV2::Minimal
            }
        );
        assert_eq!(
            dialogue.effective_profile.reasoning,
            ReasoningIntent::Effort {
                level: ReasoningEffortV2::Low
            }
        );
        assert!(
            dialogue
                .diagnostics
                .iter()
                .any(|item| { item.rule_id.as_deref() == Some("gemini-3.7-flash-minimal-to-low") })
        );
    }
}
