//! LLM-backed naming for unlabelled historic-map features.
//!
//! The vision pass returns feature kinds but only some carry labels. Those
//! without labels need plausible 1820s Irish names (e.g. "Smith's Forge",
//! "St. Brigid's Well"). This module issues a single batched text-only
//! call so the model can see the full set of context labels and avoid
//! duplicating them.

use parish_inference::openai_client::OpenAiClient;
use parish_types::ParishError;
use serde::{Deserialize, Serialize};

use super::vision_prompt::VisionFeatureKind;

const NAMING_SYSTEM_PROMPT: &str = "You are a historian naming features in a \
1820s rural Irish parish. Produce names that feel plausible for the period: \
saint names for holy wells and churches, surname + feature (e.g. 'Murphy's \
Forge'), townland-style names ('Cnocán an tSagairt'), or simple descriptive \
names ('the old mill at the ford'). Avoid modern English phrasings and \
avoid using names that are already taken in the supplied context.";

/// One unnamed feature to be named.
#[derive(Debug, Clone, Serialize)]
pub struct NamingRequestFeature {
    /// Caller-assigned index; echoed back in the response so results line up.
    pub idx: usize,
    /// Feature kind classification (church, mill, forge, ...).
    pub feature_kind: VisionFeatureKind,
}

/// Request payload for [`generate_names`].
#[derive(Debug, Serialize)]
struct NamingRequest<'a> {
    /// Labels already present in the area (to avoid duplication).
    context_labels: &'a [String],
    /// Features needing names.
    unnamed: &'a [NamingRequestFeature],
}

/// One item in the LLM's response.
#[derive(Debug, Clone, Deserialize)]
pub struct NamedFeature {
    /// Echo of the request `idx`, so the caller can re-associate.
    pub idx: usize,
    /// Suggested 1820s-style name.
    pub name: String,
}

/// Top-level response schema.
#[derive(Debug, Deserialize)]
struct NamingResponse {
    #[serde(default)]
    named: Vec<NamedFeature>,
}

/// Generates names for each unnamed feature in a single batched call.
///
/// The client must target a text-only chat-completions endpoint. On
/// success, returns one [`NamedFeature`] per input (in the order returned
/// by the model — callers should reassociate via `idx`).
pub async fn generate_names(
    client: &OpenAiClient,
    model: &str,
    context_labels: &[String],
    unnamed: &[NamingRequestFeature],
) -> Result<Vec<NamedFeature>, ParishError> {
    if unnamed.is_empty() {
        return Ok(Vec::new());
    }

    let payload = NamingRequest {
        context_labels,
        unnamed,
    };
    let prompt = format!(
        "Assign a plausible 1820s Irish name to each unnamed feature below. \
Respond with JSON shaped as {{\"named\": [{{\"idx\": <n>, \"name\": \"<text>\"}}, ...]}}. \
Every `idx` in the input must appear exactly once in your response.\n\n\
Input: {input}",
        input = serde_json::to_string(&payload)
            .map_err(|e| ParishError::Inference(format!("naming input serialize: {e}")))?,
    );

    let resp: NamingResponse = client
        .generate_json(model, &prompt, Some(NAMING_SYSTEM_PROMPT), Some(2048), None)
        .await?;
    Ok(resp.named)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_naming_response_parses() {
        let json = r#"{"named": [{"idx": 0, "name": "Murphy's Forge"}, {"idx": 2, "name": "St. Brigid's Well"}]}"#;
        let parsed: NamingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.named.len(), 2);
        assert_eq!(parsed.named[0].idx, 0);
        assert_eq!(parsed.named[0].name, "Murphy's Forge");
        assert_eq!(parsed.named[1].idx, 2);
    }

    #[test]
    fn test_naming_response_empty_default() {
        let parsed: NamingResponse = serde_json::from_str("{}").unwrap();
        assert!(parsed.named.is_empty());
    }

    #[tokio::test]
    async fn test_generate_names_empty_input_short_circuits() {
        // No LLM call is made when there are no unnamed features.
        // Use an obviously bogus client URL; if a call were attempted the
        // test would fail.
        let client = OpenAiClient::new("http://127.0.0.1:0", None);
        let out = generate_names(&client, "m", &[], &[]).await.unwrap();
        assert!(out.is_empty());
    }
}
