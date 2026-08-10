//! LLM-backed intent parsing.
//!
//! Routes ambiguous free-form input through the configured inference
//! client. Falls back to [`IntentKind::Unknown`] if the LLM call fails.

use parish_inference::{AnyClient, GenerateParams};
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
IMPORTANT: \"look\" is ONLY for a bare imperative observation command — \
\"look\", \"look around\", \"examine the room\", \"where am I\". A sentence \
that merely contains the word \"look\" inside conversational speech (e.g. \
\"Might I look about the village a while?\", \"I'll have a look at the \
cattle later\", \"It looks fine to me\") is \"talk\", not \"look\".\n\
\n\
Examples:\n\
Input: \"go to the pub\" → {\"intent\": \"move\", \"target\": \"the pub\", \"dialogue\": null}\n\
Input: \"talk to Mary\" → {\"intent\": \"talk\", \"target\": \"Mary\", \"dialogue\": null}\n\
Input: \"tell Padraig I saw his cow\" → {\"intent\": \"talk\", \"target\": \"Padraig\", \"dialogue\": \"I saw his cow\"}\n\
Input: \"look around\" → {\"intent\": \"look\", \"target\": null, \"dialogue\": null}\n\
Input: \"Might I look about the village a while?\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"Might I look about the village a while?\"}\n\
Input: \"I'll have a look at the cattle later\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I'll have a look at the cattle later\"}\n\
Input: \"pick up the stone\" → {\"intent\": \"interact\", \"target\": \"the stone\", \"dialogue\": null}\n\
Input: \"tie a strip of cloth to the thorn bush\" → {\"intent\": \"interact\", \"target\": \"the thorn bush\", \"dialogue\": null}\n\
Input: \"light the candle on the altar\" → {\"intent\": \"interact\", \"target\": \"the candle\", \"dialogue\": null}\n\
Input: \"pour water into the basin\" → {\"intent\": \"interact\", \"target\": \"the basin\", \"dialogue\": null}\n\
Input: \"kneel before the cross\" → {\"intent\": \"interact\", \"target\": \"the cross\", \"dialogue\": null}\n\
Input: \"I came from the coast\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I came from the coast\"}\n\
Input: \"I was at the shore yesterday\" → {\"intent\": \"talk\", \"target\": null, \"dialogue\": \"I was at the shore yesterday\"}\n\
\n\
IMPORTANT: \"interact\" is for any imperative physical-action command — picking up, \
putting down, tying, lighting, pouring, filling, lifting, pumping, digging, placing, \
or touching/using an object. Do NOT classify physical-action imperatives as \"talk\".\n\
\n\
Respond ONLY with valid JSON. No explanation.";

/// Returns `true` if `raw_input` matches one of the canonical look/examine
/// phrases that the local parser recognises as genuine observation commands.
///
/// Used as a post-LLM guard: small quantised models sometimes return `look` or
/// `examine` for conversational input that contains neither word as an
/// imperative verb.  When the LLM says `Look`/`Examine` but this check fails
/// the intent is downgraded to `Unknown` so it falls through to NPC routing
/// instead of printing the location description unexpectedly (#1276).
fn is_genuine_look_input(raw_input: &str) -> bool {
    let lower = raw_input.trim().to_lowercase();
    // Exact matches (fast path — same set as parse_intent_local).
    let exact = ["look", "look around", "l", "examine room", "where am i"];
    if exact.contains(&lower.as_str()) {
        return true;
    }
    // Prefix matches for imperative observation commands:
    //   "look at ...", "look closely ...", "examine ...", "inspect ...",
    //   "study ...", "scrutinise ...", "where am i ..."
    // Conversational uses like "Might I look about..." start with "might"
    // and do not match any of these.
    let prefixes = [
        "look at ",
        "look ",
        "examine ",
        "inspect ",
        "study ",
        "scrutinise ",
        "scrutinize ",
        "where am i",
    ];
    prefixes.iter().any(|p| lower.starts_with(p))
}

/// Parses natural language input into a structured `PlayerIntent`.
///
/// First tries local keyword matching for common commands (movement, look).
/// Falls back to LLM for ambiguous input. If the LLM call fails,
/// returns `IntentKind::Unknown`.
///
/// # Look/Examine guard (#1276)
///
/// After the LLM responds, any `Look` or `Examine` classification is validated
/// against a whitelist of genuine observation-command forms.  If the raw input
/// does not resemble a look command the intent is downgraded to `Unknown` so
/// the input routes to NPC conversation rather than printing the location
/// description blurb unexpectedly.
pub async fn parse_intent(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
) -> Result<PlayerIntent, ParishError> {
    parse_intent_with_profile(
        client,
        raw_input,
        model,
        parish_config::InferenceProfile::for_subrole(parish_config::InferenceSubrole::Intent),
    )
    .await
}

/// Parses intent using a fully resolved runtime inference profile.
pub async fn parse_intent_with_profile(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
    profile: parish_config::InferenceProfile,
) -> Result<PlayerIntent, ParishError> {
    parse_intent_with_profile_and_audit(client, raw_input, model, profile, None).await
}

/// Parses intent and records native provider metadata in the common audit sink.
pub async fn parse_intent_with_profile_and_audit(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
    profile: parish_config::InferenceProfile,
    audit_sink: Option<parish_inference::InferenceAuditSink>,
) -> Result<PlayerIntent, ParishError> {
    // Try local parsing first — no LLM needed for obvious commands
    if let Some(intent) = parse_intent_local(raw_input) {
        return Ok(intent);
    }

    let params = GenerateParams {
        max_tokens: Some(profile.max_output_tokens),
        thinking_level: Some(profile.thinking_level),
        service_tier: Some(profile.service_tier),
        ..GenerateParams::default()
    };
    let audit = parish_inference::DirectInferenceAudit::new(
        audit_sink,
        model,
        raw_input,
        Some(INTENT_SYSTEM_PROMPT),
        parish_config::InferenceSubrole::Intent,
        false,
        params.max_tokens,
        params.thinking_level,
        params.service_tier,
        params.temperature,
        parish_inference::InferencePriority::Interactive,
    );
    let detailed = client
        .generate_detailed_with_format(
            model,
            raw_input,
            Some(INTENT_SYSTEM_PROMPT),
            Some(parish_inference::ResponseFormat::JsonObject),
            params,
        )
        .await
        .and_then(|result| {
            parish_inference::parse_generation_json::<IntentResponse>(result, "intent")
        });
    let result = match detailed {
        Ok((raw, parsed)) => audit.record(Ok(raw)).await.map(|_| parsed),
        Err(error) => {
            let error = audit
                .record(Err(error))
                .await
                .expect_err("auditing must preserve provider errors");
            Err(error)
        }
    };

    match result {
        Ok(resp) => {
            let mut intent = resp.intent.unwrap_or(IntentKind::Unknown);
            // Guard: downgrade spurious Look/Examine classifications (#1276).
            // Small quantised models occasionally classify conversational input
            // (e.g. "hey everybody", "no reason") as Look, which would cause the
            // location description blurb to fire unexpectedly.  Accept Look/Examine
            // only when the raw input actually resembles an observation command.
            if matches!(intent, IntentKind::Look | IntentKind::Examine)
                && !is_genuine_look_input(raw_input)
            {
                // Downgrade spurious Look/Examine — caller routes to NPC
                // conversation instead of printing the location blurb (#1276).
                intent = IntentKind::Unknown;
            }
            Ok(PlayerIntent {
                intent,
                target: resp.target,
                dialogue: resp.dialogue,
                raw: raw_input.to_string(),
            })
        }
        Err(_) => Ok(PlayerIntent {
            intent: IntentKind::Unknown,
            target: None,
            dialogue: None,
            raw: raw_input.to_string(),
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_response_deserialize() {
        let json = r#"{"intent": "move", "target": "the pub", "dialogue": null}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Move));
        assert_eq!(resp.target, Some("the pub".to_string()));
        assert!(resp.dialogue.is_none());
    }
    #[test]
    fn test_intent_response_empty() {
        let json = r#"{}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert!(resp.intent.is_none());
        assert!(resp.target.is_none());
        assert!(resp.dialogue.is_none());
    }

    /// Unit tests for the is_genuine_look_input guard (#1276).
    ///
    /// These cover the validation function directly — LLM integration tests
    /// (which need a live model) live in tests/llm_fallback_integration.rs.
    #[test]
    fn genuine_look_inputs_accepted() {
        // Exact whitelist matches.
        assert!(is_genuine_look_input("look"));
        assert!(is_genuine_look_input("look around"));
        assert!(is_genuine_look_input("l"));
        assert!(is_genuine_look_input("examine room"));
        assert!(is_genuine_look_input("where am i"));
        // Case-insensitive.
        assert!(is_genuine_look_input("LOOK"));
        assert!(is_genuine_look_input("Look Around"));
        assert!(is_genuine_look_input("WHERE AM I"));
        // Prefix matches.
        assert!(is_genuine_look_input("look at the door"));
        assert!(is_genuine_look_input("look closely at the window"));
        assert!(is_genuine_look_input("examine the shelf"));
        assert!(is_genuine_look_input("where am i exactly"));
        // Extended examine verbs.
        assert!(is_genuine_look_input("inspect the stone cross closely"));
        assert!(is_genuine_look_input("study the inscription"));
        assert!(is_genuine_look_input("scrutinise the wall"));
        assert!(is_genuine_look_input("scrutinize the carving"));
    }

    /// Regression (#1276): conversational inputs that Qwen2.5-14B-Instruct-4bit
    /// misclassified as Look must be rejected by the guard.
    #[test]
    fn non_look_inputs_rejected() {
        assert!(!is_genuine_look_input("hey everybody"));
        assert!(!is_genuine_look_input("no reason"));
        assert!(!is_genuine_look_input("I just like boats"));
        assert!(!is_genuine_look_input(
            "Might I look about the village a while?"
        ));
        assert!(!is_genuine_look_input("It looks fine to me"));
        assert!(!is_genuine_look_input(
            "I'll have a look at the cattle later"
        ));
        assert!(!is_genuine_look_input("hello there"));
        assert!(!is_genuine_look_input("tell me about yourself"));
    }

    /// Regression: the demo auto-player's own exemplar ("Might I look about
    /// the village a while?") was being classified as `look` because the
    /// intent system prompt did not distinguish narrative use of the word
    /// "look" from the imperative observation command. That caused
    /// `handle_look` to fire every turn, spamming the location description.
    /// Closes #999.
    #[test]
    fn intent_system_prompt_distinguishes_narrative_look() {
        assert!(
            INTENT_SYSTEM_PROMPT.contains("\"look\" is ONLY for a bare imperative"),
            "intent system prompt lost the narrative-look carve-out"
        );
        assert!(
            INTENT_SYSTEM_PROMPT.contains("Might I look about the village a while?"),
            "intent system prompt lost the narrative-look exemplar"
        );
        assert!(
            INTENT_SYSTEM_PROMPT.contains("I'll have a look at the cattle later"),
            "intent system prompt lost the second narrative-look exemplar"
        );
        // Regression guards for the imperative form.
        assert!(
            INTENT_SYSTEM_PROMPT.contains("\"look around\" → {\"intent\": \"look\""),
            "intent system prompt lost the imperative-look exemplar"
        );
    }

    /// Guard: intent system prompt must carry enough interact examples and
    /// instructions that small quantised models classify physical-action
    /// imperatives as "interact", not "talk" (#1449).
    #[test]
    fn intent_system_prompt_includes_interact_examples() {
        // Core interact instruction.
        assert!(
            INTENT_SYSTEM_PROMPT.contains("\"interact\" is for any imperative physical-action"),
            "intent system prompt lost the interact instruction (#1449)"
        );
        // Original example retained.
        assert!(
            INTENT_SYSTEM_PROMPT.contains("\"pick up the stone\""),
            "intent system prompt lost the pick-up example"
        );
        // Repro examples from #1449.
        assert!(
            INTENT_SYSTEM_PROMPT.contains("tie a strip of cloth"),
            "intent system prompt lost the tie-cloth example (#1449 repro)"
        );
    }
}
