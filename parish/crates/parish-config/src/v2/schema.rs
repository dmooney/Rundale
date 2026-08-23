use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::engine::EngineConfig;

pub const INFERENCE_SCHEMA_VERSION: u8 = 2;
pub const PROJECT_SCHEMA_ID: &str = "https://rundale.dev/schemas/parish-project-config/v2.json";
pub const USER_SCHEMA_ID: &str = "https://rundale.dev/schemas/parish-user-config/v2.json";
pub const MANAGED_OLLAMA_LOADOUT: &str = "__rundale_managed_ollama";
pub const CATALOG_CACHE_SCHEMA_VERSION: u8 = 1;
pub const PROBE_RECEIPT_SCHEMA_VERSION: u8 = 1;

pub fn generated_project_schema_v2() -> serde_json::Value {
    generated_schema::<ProjectConfigV2>(PROJECT_SCHEMA_ID)
}

pub fn generated_user_schema_v2() -> serde_json::Value {
    generated_schema::<UserConfigV2>(USER_SCHEMA_ID)
}

fn generated_schema<T: JsonSchema>(schema_id: &str) -> serde_json::Value {
    let mut value = serde_json::to_value(schemars::schema_for!(T))
        .expect("JSON schema generation is infallible for serializable schema values");
    value
        .as_object_mut()
        .expect("schema root is an object")
        .insert("$id".into(), serde_json::Value::String(schema_id.into()));
    for (definition, property, pattern) in [
        (
            "InferenceLayer",
            "loadouts",
            r"^(?:[a-z][a-z0-9_-]{0,62}|__rundale_managed_ollama)$",
        ),
        ("InferenceLayer", "providers", r"^[a-z][a-z0-9_-]{0,62}$"),
        (
            "LoadoutDefinition",
            "subroles",
            r"^(dialogue|intent|arrival-reaction|message-reaction|travel-encounter|tier2-simulation|tier3-simulation|demo-player)$",
        ),
        (
            "CustomProviderDefinition",
            "endpoints",
            r"^[a-z][a-z0-9_-]{0,62}$",
        ),
        (
            "CustomProviderDefinition",
            "models",
            r"^[a-z][a-z0-9_-]{0,62}$",
        ),
        (
            "UserConfigV2",
            "credential_bindings",
            r"^custom:[a-z][a-z0-9_-]{0,62}$",
        ),
    ] {
        let definition_path = format!("/$defs/{definition}/properties/{property}");
        let root_path = format!("/properties/{property}");
        let selected_path = if value.pointer(&definition_path).is_some() {
            &definition_path
        } else {
            &root_path
        };
        if let Some(map) = value
            .pointer_mut(selected_path)
            .and_then(serde_json::Value::as_object_mut)
        {
            map.insert(
                "propertyNames".into(),
                serde_json::json!({"pattern": pattern}),
            );
        }
    }
    value
}

#[cfg(test)]
mod generated_schema_tests {
    use super::*;

    fn json_semantic_difference(
        left: &serde_json::Value,
        right: &serde_json::Value,
        path: &str,
    ) -> Option<String> {
        match (left, right) {
            (serde_json::Value::Number(left), serde_json::Value::Number(right)) => {
                let numerically_equal =
                    left.as_f64()
                        .zip(right.as_f64())
                        .is_some_and(|(left, right)| {
                            // Prettier parses JSON numbers through JavaScript doubles and can
                            // move generated f32 defaults by one f64 ULP while reformatting.
                            (left - right).abs()
                                <= f64::EPSILON * left.abs().max(right.abs()).max(1.0) * 2.0
                        });
                (left != right && !numerically_equal)
                    .then(|| format!("{path} (checked-in {left:?}, generated {right:?})"))
            }
            (serde_json::Value::Array(left), serde_json::Value::Array(right)) => {
                if left.len() != right.len() {
                    return Some(format!("{path} (array length)"));
                }
                left.iter()
                    .zip(right)
                    .enumerate()
                    .find_map(|(index, (left, right))| {
                        json_semantic_difference(left, right, &format!("{path}/{index}"))
                    })
            }
            (serde_json::Value::Object(left), serde_json::Value::Object(right)) => {
                if left.len() != right.len() {
                    return Some(format!("{path} (object length)"));
                }
                left.iter().find_map(|(key, left)| {
                    let key_path = format!("{path}/{key}");
                    right.get(key).map_or_else(
                        || Some(key_path.clone()),
                        |right| json_semantic_difference(left, right, &key_path),
                    )
                })
            }
            _ => (left != right).then(|| path.to_owned()),
        }
    }

    #[test]
    fn checked_in_v2_schemas_match_generator() {
        let docs = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../docs/schemas");
        for (name, generated) in [
            (
                "parish-project-config-v2.schema.json",
                generated_project_schema_v2(),
            ),
            (
                "parish-user-config-v2.schema.json",
                generated_user_schema_v2(),
            ),
        ] {
            let checked_in = std::fs::read_to_string(docs.join(name))
                .unwrap_or_else(|error| panic!("read {name}: {error}"));
            let checked_in: serde_json::Value = serde_json::from_str(&checked_in)
                .unwrap_or_else(|error| panic!("parse {name}: {error}"));
            if let Some(path) = json_semantic_difference(&checked_in, &generated, "") {
                panic!("{name} drifted at {path}; run the v2 schema generator");
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfigV2 {
    pub schema_version: u8,
    #[serde(default)]
    pub engine: EngineConfig,
    #[serde(default)]
    pub inference: InferenceLayer,
}

impl Default for ProjectConfigV2 {
    fn default() -> Self {
        Self {
            schema_version: INFERENCE_SCHEMA_VERSION,
            engine: EngineConfig::default(),
            inference: InferenceLayer::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserConfigV2 {
    pub schema_version: u8,
    #[serde(default)]
    pub inference: InferenceLayer,
    #[serde(default)]
    pub credential_bindings: BTreeMap<String, CredentialBinding>,
}

impl Default for UserConfigV2 {
    fn default() -> Self {
        Self {
            schema_version: INFERENCE_SCHEMA_VERSION,
            inference: InferenceLayer::default(),
            credential_bindings: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InferenceLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_loadout: Option<String>,
    #[serde(default)]
    pub loadouts: BTreeMap<String, LoadoutDefinition>,
    #[serde(default)]
    pub providers: BTreeMap<String, CustomProviderDefinition>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadoutDefinition {
    #[serde(default)]
    pub default: RoutePatch,
    #[serde(default)]
    pub routes: CategoryRoutes,
    #[serde(default)]
    pub subroles: BTreeMap<String, GenerationPatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_by: Option<ManagedLoadoutOwner>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ManagedLoadoutOwner {
    OllamaSetupV1,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CategoryRoutes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue: Option<RoutePatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation: Option<RoutePatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<RoutePatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<RoutePatch>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoutePatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTierIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_unverified_model: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTierIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "kebab-case", deny_unknown_fields)]
pub enum ReasoningIntent {
    #[default]
    Auto,
    Off,
    Effort {
        level: ReasoningEffortV2,
    },
    Budget {
        tokens: u32,
    },
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffortV2 {
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema, Default,
)]
#[serde(rename_all = "kebab-case")]
pub enum ServiceTierIntent {
    #[default]
    Auto,
    Standard,
    Priority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InferenceAdapter {
    OpenaiResponsesV1,
    OpenaiChatV1,
    AnthropicMessages2023_06_01,
    GoogleInteractionsV1,
    Simulator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryAdapter {
    None,
    OpenaiModelsV1,
    AnthropicModelsV1,
    GoogleModelsV1Beta,
    OpenrouterModelsV1,
    OllamaTagsV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum BackendKind {
    Remote,
    Local { runtime: LocalRuntime },
    Simulator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum LocalRuntime {
    Ollama,
    LmStudio,
    Vllm,
    VllmMlx,
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ManagementAdapter {
    None,
    External,
    Ollama,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AuthAdapter {
    None,
    Bearer,
    AnthropicKey,
    GoogleKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningDialect {
    None,
    OpenaiResponsesEffort,
    OpenaiChatEffort,
    OpenrouterReasoning,
    DeepseekThinking,
    LocalTemplateToggle,
    AnthropicAdaptive,
    AnthropicManualBudget,
    GoogleThinkingLevel,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomProviderDefinition {
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_endpoint: Option<String>,
    pub endpoints: BTreeMap<String, CustomEndpointDefinition>,
    #[serde(default)]
    pub models: BTreeMap<String, BTreeMap<String, UserModelCapabilityOverride>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CustomEndpointDefinition {
    pub inference_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_base_url: Option<String>,
    pub inference_adapter: InferenceAdapter,
    pub discovery_adapter: DiscoveryAdapter,
    pub auth_adapter: AuthAdapter,
    pub default_reasoning_dialect: ReasoningDialect,
    #[serde(default)]
    pub allow_insecure_http: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_openai_generation_wire: Option<OpenAiChatGenerationWire>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CredentialBinding {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderDefinition {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub default_endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_preset: Option<String>,
    pub endpoints: BTreeMap<String, EndpointDefinition>,
    #[serde(default)]
    pub presets: BTreeMap<String, ProviderPresetV2>,
    #[serde(default)]
    pub curated_models: BTreeMap<String, BTreeMap<String, CuratedModelRoute>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EndpointDefinition {
    pub inference_base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discovery_base_url: Option<String>,
    pub inference_adapter: InferenceAdapter,
    pub discovery_adapter: DiscoveryAdapter,
    pub backend_kind: BackendKind,
    pub management_adapter: ManagementAdapter,
    pub auth_adapter: AuthAdapter,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_slot: Option<String>,
    pub default_reasoning_dialect: ReasoningDialect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_openai_generation_wire: Option<OpenAiChatGenerationWire>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProviderPresetV2 {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue: Option<PresetRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulation: Option<PresetRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<PresetRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reaction: Option<PresetRoute>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRoute {
    pub endpoint: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CuratedModelRoute {
    #[serde(default)]
    pub qualified: bool,
    #[serde(default)]
    pub recommended: bool,
    #[serde(default)]
    pub generation_defaults: BTreeMap<String, GenerationProfile>,
    pub reasoning: ReasoningCapabilities,
    pub generation: GenerationCapabilities,
    pub output_contracts: OutputContractCapabilities,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_generation_wire: Option<OpenAiChatGenerationWire>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualification_receipt: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UserModelCapabilityOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<ReasoningCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generation: Option<GenerationCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contracts: Option<OutputContractCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openai_generation_wire: Option<OpenAiChatGenerationWire>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasoningCapabilities {
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub default_by_subrole: BTreeMap<String, ReasoningIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<EffortCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<BudgetCapability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub off_dialect: Option<ReasoningDialect>,
    #[serde(default)]
    pub translations: Vec<ReasoningTranslation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffortCapability {
    pub dialect: ReasoningDialect,
    pub supported_levels: BTreeSet<ReasoningEffortV2>,
    #[serde(default)]
    pub minimum_total_output_tokens: BTreeMap<String, BTreeMap<ReasoningEffortV2, u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BudgetCapability {
    pub dialect: ReasoningDialect,
    pub min_tokens: u32,
    pub max_tokens: u32,
    pub min_visible_output_headroom: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ReasoningTranslation {
    pub rule_id: String,
    pub from: ReasoningIntent,
    pub to: ReasoningIntent,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationCapabilities {
    pub min_output_tokens: u32,
    pub max_output_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<NumericRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<NumericRange>,
    #[serde(default)]
    pub service_tiers: BTreeMap<ServiceTierIntent, WireServiceTier>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NumericRange {
    pub min: f32,
    pub max: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum WireServiceTier {
    GoogleStandard,
    GooglePriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenAiChatGenerationWire {
    pub output_limit_field: OutputLimitField,
    pub structured_output: BTreeSet<StructuredOutputMode>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum OutputLimitField {
    MaxTokens,
    MaxCompletionTokens,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum StructuredOutputMode {
    PromptValidatedJson,
    JsonObject,
    JsonSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputContractCapabilities {
    pub prompt_validated_json: bool,
    pub native_json_object: bool,
    pub native_json_schema: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GenerationProfile {
    pub max_output_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    #[serde(default)]
    pub reasoning: ReasoningIntent,
    #[serde(default)]
    pub service_tier: ServiceTierIntent,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Availability {
    Listed,
    NotListed,
    /// Explicitly cannot satisfy Parish's required transport/modalities.
    /// This is a hard constraint, not an opt-in discovery state.
    Incompatible,
    Unverified,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProvenanceKind {
    Curated,
    Remote,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    pub kind: ProvenanceKind,
    pub source_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Provenanced<T> {
    pub value: T,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelRouteKey {
    pub provider_id: String,
    pub endpoint_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogModelRoute {
    pub key: ModelRouteKey,
    pub availability: Provenanced<Availability>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<Provenanced<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<Provenanced<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<Provenanced<u64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<Provenanced<BTreeSet<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<Provenanced<BTreeSet<String>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<Provenanced<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contracts: Option<Provenanced<OutputContractCapabilities>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort_levels: Option<Provenanced<BTreeSet<ReasoningEffortV2>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_budget: Option<Provenanced<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_off: Option<Provenanced<bool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tiers: Option<Provenanced<BTreeSet<ServiceTierIntent>>>,
    #[serde(default)]
    pub observations: Vec<CatalogObservation>,
    #[serde(default)]
    pub omitted_observation_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogObservation {
    pub kind: CatalogObservationKind,
    pub observed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogCacheDocument {
    pub schema_version: u8,
    pub identity: CatalogCacheIdentity,
    pub fetched_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_attempt_at: Option<DateTime<Utc>>,
    pub expires_at: DateTime<Utc>,
    pub status: DiscoveryStatus,
    /// True only when every page was fetched within all configured limits.
    #[serde(default)]
    pub complete_listing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub routes: BTreeMap<String, DiscoveredModel>,
    #[serde(default)]
    pub diagnostics: Vec<ConfigDiagnostic>,
    #[serde(default)]
    pub conflicting_observations: Vec<CatalogConflictObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogConflictObservation {
    pub model_id: String,
    pub field: String,
    pub change_kind: CatalogConflictKind,
    pub previous_value: String,
    pub observed_value: String,
    pub previous_payload_hash: String,
    pub payload_hash: String,
    pub previous_observed_at: DateTime<Utc>,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogConflictKind {
    Added,
    Removed,
    Changed,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CatalogCacheIdentity {
    pub provider_id: String,
    pub endpoint_id: String,
    pub discovery_base_url: String,
    pub inference_base_url: String,
    pub inference_adapter_version: String,
    pub discovery_adapter_version: String,
    /// HMAC or hash supplied by the credential store. Never the credential.
    pub credential_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiscoveryStatus {
    Success,
    Unsupported,
    AuthenticationFailed,
    RateLimited,
    Unavailable,
    Malformed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DiscoveredModel {
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_modalities: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_modalities: Option<BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_contracts: Option<OutputContractCapabilities>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort_levels: Option<BTreeSet<ReasoningEffortV2>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_budget: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning_off: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tiers: Option<BTreeSet<ServiceTierIntent>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_temperature: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_frequency_penalty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProbeReceipt {
    pub schema_version: u8,
    pub attempt_id: String,
    pub route: ModelRouteKey,
    pub catalog_identity: CatalogCacheIdentity,
    pub configuration_epoch: u64,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub request_hash: String,
    pub request_path: String,
    pub inference_adapter_version: String,
    pub discovery_adapter_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_request_id: Option<String>,
    pub raw_response_path: String,
    pub raw_response_hash: String,
    pub outcome: ProbeOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_http_status: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProbeOutcome {
    Passed,
    NotListed,
    Rejected,
    TransportFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogObservationKind {
    Listed,
    NotListed,
    ProbePassed,
    ProbeFailed,
    ProbeNotListed,
}

#[derive(Clone)]
pub struct SecretString(String);

impl SecretString {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("SecretString([REDACTED])")
    }
}

#[derive(Clone)]
pub struct ResolvedRoute {
    pub key: ModelRouteKey,
    pub inference_base_url: String,
    pub discovery_base_url: Option<String>,
    pub credential: Option<SecretString>,
    pub inference_adapter: InferenceAdapter,
    pub discovery_adapter: DiscoveryAdapter,
    pub backend_kind: BackendKind,
    pub management_adapter: ManagementAdapter,
    pub auth_adapter: AuthAdapter,
    pub reasoning_dialect: ReasoningDialect,
    pub openai_output_limit_field: Option<OutputLimitField>,
    pub requested_profile: GenerationProfile,
    pub effective_profile: GenerationProfile,
    pub structured_output: Option<StructuredOutputMode>,
    pub availability: Availability,
    pub diagnostics: Vec<ConfigDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveRouteView {
    pub key: ModelRouteKey,
    pub inference_base_url: String,
    pub discovery_base_url: Option<String>,
    pub has_credential: bool,
    pub inference_adapter: InferenceAdapter,
    pub discovery_adapter: DiscoveryAdapter,
    pub backend_kind: BackendKind,
    pub management_adapter: ManagementAdapter,
    pub auth_adapter: AuthAdapter,
    pub reasoning_dialect: ReasoningDialect,
    pub openai_output_limit_field: Option<OutputLimitField>,
    pub requested_profile: GenerationProfile,
    pub effective_profile: GenerationProfile,
    pub structured_output: Option<StructuredOutputMode>,
    pub availability: Availability,
    pub diagnostics: Vec<ConfigDiagnostic>,
    pub configuration_epoch: u64,
}

impl ResolvedRoute {
    pub fn view(&self, configuration_epoch: u64) -> EffectiveRouteView {
        EffectiveRouteView {
            key: self.key.clone(),
            inference_base_url: self.inference_base_url.clone(),
            discovery_base_url: self.discovery_base_url.clone(),
            has_credential: self.credential.is_some(),
            inference_adapter: self.inference_adapter,
            discovery_adapter: self.discovery_adapter,
            backend_kind: self.backend_kind,
            management_adapter: self.management_adapter,
            auth_adapter: self.auth_adapter,
            reasoning_dialect: self.reasoning_dialect,
            openai_output_limit_field: self.openai_output_limit_field,
            requested_profile: self.requested_profile.clone(),
            effective_profile: self.effective_profile.clone(),
            structured_output: self.structured_output,
            availability: self.availability.clone(),
            diagnostics: self.diagnostics.clone(),
            configuration_epoch,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigDiagnostic {
    pub code: String,
    pub severity: DiagnosticSeverity,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    High,
}
