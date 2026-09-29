//! Endpoint definitions authored as mod files (ADR-025 §5).
//!
//! A mod declares one Limerick Endpoint definition per inference role in its
//! manifest's `[endpoints]` table. Each file is named `<slug>.v<version>.json`,
//! so the file name is the single source of the Endpoint's identity, and its
//! body is the `EndpointDefinition` the Endpoints service publishes:
//! `inputSchema`, `outputSchema`, `instructions`, `providerConfig`, and
//! `inferenceConfig`. The engine reads each role's reference from here and
//! attaches it to the calls it makes; an Endpoint host executes the published
//! copy of the same file.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A model workload that is executed through a published Endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointRole {
    /// Tier-1 NPC dialogue.
    Dialogue,
    /// Player-input intent classification.
    Intent,
}

impl EndpointRole {
    /// The manifest key that declares this role.
    pub fn manifest_key(self) -> &'static str {
        match self {
            Self::Dialogue => "dialogue",
            Self::Intent => "intent",
        }
    }
}

/// Which published Endpoint version a call executes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointRef {
    /// The inference role the Endpoint serves.
    pub role: EndpointRole,
    /// The Endpoint's slug within its organization (e.g. `rundale-dialogue`).
    pub slug: String,
    /// The immutable published version.
    pub version: u32,
}

/// Provider targeted by a definition. Mirrors the Endpoints service's
/// `ProviderId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EndpointProvider {
    /// The service's deterministic test provider.
    Fake,
    /// OpenAI.
    Openai,
    /// Google.
    Google,
}

/// `providerConfig` of a definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointProviderConfig {
    /// Provider id.
    pub provider: EndpointProvider,
    /// Provider model id.
    pub model: String,
}

/// `inferenceConfig.streaming` of a definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointStreaming {
    /// Streaming projection version; only 1 exists.
    pub version: u32,
    /// Top-level output field streamed as text.
    pub text_field: String,
}

/// `inferenceConfig` of a definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointInferenceConfig {
    /// Sampling temperature, when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    /// Output token cap.
    pub max_output_tokens: u32,
    /// Provider-level retries (0 or 1).
    pub retry_count: u8,
    /// Streaming projection, when the role streams.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub streaming: Option<EndpointStreaming>,
}

/// The body of an Endpoint definition file, as the Endpoints service
/// publishes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EndpointDefinition {
    /// JSON Schema (draft-07) for the invocation input.
    pub input_schema: Map<String, Value>,
    /// JSON Schema (draft-07) for the structured output.
    pub output_schema: Map<String, Value>,
    /// The Endpoint's instructions (its prompt).
    pub instructions: String,
    /// Provider and model.
    pub provider_config: EndpointProviderConfig,
    /// Generation settings.
    pub inference_config: EndpointInferenceConfig,
}

/// One loaded definition file.
#[derive(Debug, Clone, PartialEq)]
pub struct EndpointFile {
    /// Identity derived from the manifest key and file name.
    pub reference: EndpointRef,
    /// Path relative to the mod directory, as declared in the manifest.
    pub path: String,
    /// The parsed definition.
    pub definition: EndpointDefinition,
}

/// Every Endpoint definition a mod declares, by role.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EndpointCatalog {
    /// The dialogue role's definition, when declared.
    pub dialogue: Option<EndpointFile>,
    /// The intent role's definition, when declared.
    pub intent: Option<EndpointFile>,
}

/// The catalog of a runtime with no mod loaded, or a mod that declares no
/// Endpoints.
pub static NO_ENDPOINTS: EndpointCatalog = EndpointCatalog {
    dialogue: None,
    intent: None,
};

impl EndpointCatalog {
    /// The reference the engine attaches to `role`'s calls, when declared.
    pub fn reference(&self, role: EndpointRole) -> Option<&EndpointRef> {
        let file = match role {
            EndpointRole::Dialogue => self.dialogue.as_ref(),
            EndpointRole::Intent => self.intent.as_ref(),
        };
        file.map(|file| &file.reference)
    }
}

/// The Endpoint catalog of an optional loaded mod.
pub fn endpoints_of(game_mod: &Option<crate::GameMod>) -> &EndpointCatalog {
    game_mod
        .as_ref()
        .map_or(&NO_ENDPOINTS, |game_mod| &game_mod.endpoints)
}

/// Parses `<slug>.v<version>.json` into its slug and version.
///
/// Slugs are lowercase ASCII letters, digits, and single hyphens, starting
/// with a letter; versions are positive integers without leading zeros.
pub fn parse_definition_file_name(file_name: &str) -> Result<(String, u32), String> {
    let invalid = || {
        format!(
            "Endpoint definition file name {file_name:?} must be <slug>.v<version>.json, \
             e.g. rundale-dialogue.v1.json"
        )
    };
    let stem = file_name.strip_suffix(".json").ok_or_else(invalid)?;
    let (slug, version) = stem.rsplit_once(".v").ok_or_else(invalid)?;
    let slug_ok = slug.starts_with(|c: char| c.is_ascii_lowercase())
        && !slug.ends_with('-')
        && !slug.contains("--")
        && slug
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    let version_ok = !version.is_empty()
        && !version.starts_with('0')
        && version.chars().all(|c| c.is_ascii_digit());
    if !slug_ok || !version_ok {
        return Err(invalid());
    }
    let version = version.parse::<u32>().map_err(|_| invalid())?;
    Ok((slug.to_string(), version))
}

/// Parses and checks one definition body.
pub fn parse_definition(text: &str) -> Result<EndpointDefinition, String> {
    let definition: EndpointDefinition =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    if definition.instructions.trim().is_empty() {
        return Err("instructions must not be empty".to_string());
    }
    if definition.provider_config.model.trim().is_empty() {
        return Err("providerConfig.model must not be empty".to_string());
    }
    let inference = &definition.inference_config;
    if inference.retry_count > 1 {
        return Err("inferenceConfig.retryCount must be 0 or 1".to_string());
    }
    if inference.max_output_tokens == 0 {
        return Err("inferenceConfig.maxOutputTokens must be positive".to_string());
    }
    if let Some(streaming) = &inference.streaming {
        let field = streaming.text_field.trim();
        if streaming.version != 1 || field.is_empty() || streaming.text_field.len() > 128 {
            return Err(
                "inferenceConfig.streaming must specify version 1 and a textField".to_string(),
            );
        }
    }
    Ok(definition)
}

/// Loads one declared definition. `read_text` resolves `rel` inside the mod
/// directory with the loader's traversal guard.
pub(crate) fn load_definition(
    role: EndpointRole,
    rel: &str,
    read_text: impl Fn(&str) -> Result<String, limerick_types::error::LimerickError>,
) -> Result<EndpointFile, limerick_types::error::LimerickError> {
    use limerick_types::error::LimerickError;
    let file_name = Path::new(rel)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(rel);
    let (slug, version) = parse_definition_file_name(file_name).map_err(|error| {
        LimerickError::Config(format!("endpoints.{}: {error}", role.manifest_key()))
    })?;
    let definition = parse_definition(&read_text(rel)?)
        .map_err(|error| LimerickError::Config(format!("failed to parse {rel}: {error}")))?;
    Ok(EndpointFile {
        reference: EndpointRef {
            role,
            slug,
            version,
        },
        path: rel.to_string(),
        definition,
    })
}
