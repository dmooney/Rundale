//! LLM-backed intent parsing.
//!
//! Routes ambiguous free-form input through the configured inference
//! client. Falls back to [`IntentKind::Unknown`] if the LLM call fails.

use parish_inference::AnyClient;
use parish_types::ParishError;
use serde::Deserialize;

use crate::intent_local::parse_intent_local;
use crate::intent_types::{IntentKind, PlayerIntent};

/// Raw JSON response from the LLM intent parser.
#[derive(Deserialize)]
pub(crate) struct IntentResponse {
    #[serde(default)]
    pub(crate) intent: Option<IntentKind>,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) dialogue: Option<String>,
}

/// The system prompt used for intent parsing.
const INTENT_SYSTEM_PROMPT: &str = "\
You are a text adventure input parser. Given the player's natural language input, \
determine their intent. Respond with valid JSON containing:\n\
- \"intent\": one of \"move\", \"talk\", \"look\", \"interact\", \"examine\", \"unknown\"\n\
- \"target\": what the action is directed at (string or null)\n\
- \"dialogue\": what the player is saying, if talking (string or null)\n\
\n\
IMPORTANT: \"move\" is ONLY for when the player expresses a present desire to \
navigate somewhere (imperative or future intent). Narrative, past-tense, or \
reflective statements that merely mention a place name are \"talk\", not \"move\".\n\
\n\
Examples:\n\
Input: \"go to the pub\" → {\"intent\": \"move\", \"target\": \"the pub\", \"dialogue\": null}\n\
Input: \"talk to Mary\" → {\"intent\": \"talk\", \"target\": \"Mary\", \"dialogue\": null}\n\
Input: \"tell Padraig I saw his cow\" → {\"intent\": \"talk\", \"target\": \"Padraig\", \"dialogue\": \"I saw his cow\"}\n\
Input: \"look around\" → {\"intent\": \"look\", \"target\": null, \"dialogue\": null}\n\
Input: \"pick up the stone\" → {\"intent\": \"interact\", \"target\": \"the stone\", \"dialogue\": null}\n\
Input: \"I came from the coast\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I came from the coast\"}\n\
Input: \"I was at the shore yesterday\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I was at the shore yesterday\"}\n\
\n\
Respond ONLY with valid JSON. No explanation.";

/// Parses natural language input into a structured `PlayerIntent`.
///
/// First tries local keyword matching for common commands (movement, look).
/// Falls back to LLM for ambiguous input. If the LLM call fails,
/// returns `IntentKind::Unknown`.
pub async fn parse_intent(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
) -> Result<PlayerIntent, ParishError> {
    // Try local parsing first — no LLM needed for obvious commands
    if let Some(intent) = parse_intent_local(raw_input) {
        return Ok(intent);
    }

    let result = client
        .generate_json::<IntentResponse>(model, raw_input, Some(INTENT_SYSTEM_PROMPT), None, None)
        .await;

    match result {
        Ok(resp) => Ok(PlayerIntent {
            intent: resp.intent.unwrap_or(IntentKind::Unknown),
            target: resp.target,
            dialogue: resp.dialogue,
            raw: raw_input.to_string(),
        }),
        Err(_) => Ok(PlayerIntent {
            intent: IntentKind::Unknown,
            target: None,
            dialogue: None,
            raw: raw_input.to_string(),
        }),
    }
}
