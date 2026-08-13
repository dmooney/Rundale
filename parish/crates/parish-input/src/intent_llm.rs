//! LLM-backed intent parsing.
//!
//! Routes ambiguous free-form input through the configured inference
//! client. Falls back to [`IntentKind::Unknown`] if the LLM call fails.

use parish_inference::{AnyClient, GenerateParams};
use parish_types::ParishError;
use serde::Deserialize;

use crate::intent_local::{
    detect_atmospheric_topic, parse_intent_local, supports_atmospheric_topic_hint,
};
use crate::intent_types::{AtmosphericTopic, IntentKind, PlayerIntent};

/// Raw JSON response from the LLM intent parser.
#[derive(Deserialize)]
pub(crate) struct IntentResponse {
    #[serde(default)]
    pub(crate) intent: Option<IntentKind>,
    #[serde(default)]
    pub(crate) target: Option<String>,
    #[serde(default)]
    pub(crate) dialogue: Option<String>,
    #[serde(default)]
    pub(crate) atmosphere: Option<String>,
}

/// The system prompt used for intent parsing.
const INTENT_SYSTEM_PROMPT: &str = "\
You are a text adventure input parser. Given the player's natural language input, \
determine their intent. Respond with valid JSON containing:\n\
- \"intent\": one of \"move\", \"talk\", \"look\", \"interact\", \"examine\", \"unknown\"\n\
- \"target\": what the action is directed at (string or null)\n\
- \"dialogue\": what the player is saying, if talking (string or null)\n\
- \"atmosphere\": optionally \"listen\", \"omen\", or \"folklore\" when that subject is explicitly present; otherwise null\n\
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
IMPORTANT: \"atmosphere\" supplements the main intent; it never replaces it. A player \
talking to someone about omens remains \"talk\" with atmosphere \"omen\". Use \"listen\" \
only for listening to the wider land/world/place, not listening to a person. Use \
\"folklore\" for explicit folklore, local legends, or qualified old/local/traditional \
tales, not a generic story. Use \"omen\" for explicit omens/portents or seeking a \
supernatural sign, not road signs or signposts.\n\
Clear equivalents include hearing the wind/world for \"listen\", place-bound legends \
or traditions for \"folklore\", and divination or augury for \"omen\".\n\
Input: \"Peig, do you hear what the land is saying?\" → {\"intent\": \"talk\", \"target\": \"Peig\", \"dialogue\": \"do you hear what the land is saying?\", \"atmosphere\": \"listen\"}\n\
Input: \"Peig, have you seen any omens here?\" → {\"intent\": \"talk\", \"target\": \"Peig\", \"dialogue\": \"have you seen any omens here?\", \"atmosphere\": \"omen\"}\n\
Input: \"Peig, what old tales are told about this place?\" → {\"intent\": \"talk\", \"target\": \"Peig\", \"dialogue\": \"what old tales are told about this place?\", \"atmosphere\": \"folklore\"}\n\
Input: \"listen to Mary\" → {\"intent\": \"talk\", \"target\": \"Mary\", \"dialogue\": null, \"atmosphere\": null}\n\
Input: \"read the road signs\" → {\"intent\": \"interact\", \"target\": \"the road signs\", \"dialogue\": null, \"atmosphere\": null}\n\
\n\
Respond ONLY with valid JSON. No explanation.";

/// Accepts an LLM-proposed topic only when the raw text independently
/// contains evidence for that same topic. Deterministic evidence is retained
/// when the model omits the optional field, so atmospheric enrichment does not
/// depend on model size or provider availability.
fn validated_atmospheric_topic(
    proposed: Option<&str>,
    raw_input: &str,
) -> Option<AtmosphericTopic> {
    if let Some(detected) = detect_atmospheric_topic(raw_input) {
        return Some(detected);
    }

    let proposed = match proposed.map(str::trim).map(str::to_ascii_lowercase) {
        Some(value) if value == "listen" => AtmosphericTopic::Listen,
        Some(value) if value == "omen" => AtmosphericTopic::Omen,
        Some(value) if value == "folklore" => AtmosphericTopic::Folklore,
        _ => return None,
    };

    if supports_atmospheric_topic_hint(raw_input, proposed) {
        Some(proposed)
    } else {
        None
    }
}

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
    // Try local parsing first — no LLM needed for obvious commands
    if let Some(intent) = parse_intent_local(raw_input) {
        return Ok(intent);
    }

    let result = client
        .generate_json::<IntentResponse>(
            model,
            raw_input,
            Some(INTENT_SYSTEM_PROMPT),
            GenerateParams::default(),
        )
        .await;

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
                atmosphere: validated_atmospheric_topic(resp.atmosphere.as_deref(), raw_input),
                raw: raw_input.to_string(),
            })
        }
        Err(_) => Ok(PlayerIntent {
            intent: IntentKind::Unknown,
            target: None,
            dialogue: None,
            atmosphere: detect_atmospheric_topic(raw_input),
            raw: raw_input.to_string(),
        }),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_response_deserialize() {
        let json =
            r#"{"intent": "move", "target": "the pub", "dialogue": null, "atmosphere": "omen"}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Move));
        assert_eq!(resp.target, Some("the pub".to_string()));
        assert!(resp.dialogue.is_none());
        assert_eq!(resp.atmosphere.as_deref(), Some("omen"));
    }
    #[test]
    fn test_intent_response_empty() {
        let json = r#"{}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert!(resp.intent.is_none());
        assert!(resp.target.is_none());
        assert!(resp.dialogue.is_none());
        assert!(resp.atmosphere.is_none());
    }

    #[test]
    fn unknown_model_atmosphere_does_not_invalidate_primary_intent() {
        let json = r#"{"intent":"talk","target":"Peig","dialogue":"hello","atmosphere":"mystery"}"#;
        let resp: IntentResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Talk));
        assert_eq!(resp.target.as_deref(), Some("Peig"));
        assert_eq!(resp.atmosphere.as_deref(), Some("mystery"));
        assert_eq!(
            validated_atmospheric_topic(resp.atmosphere.as_deref(), "hello Peig"),
            None
        );
    }

    #[test]
    fn model_atmosphere_adds_only_raw_grounded_synonym_coverage() {
        for (hint, raw, expected) in [
            (
                "listen",
                "Peig, can you hear the wind rising?",
                AtmosphericTopic::Listen,
            ),
            (
                "folklore",
                "What traditions are kept in this parish?",
                AtmosphericTopic::Folklore,
            ),
            (
                "omen",
                "Did the old women practice divination here?",
                AtmosphericTopic::Omen,
            ),
        ] {
            assert_eq!(
                detect_atmospheric_topic(raw),
                None,
                "synonym coverage should be supplied by a grounded model hint"
            );
            assert_eq!(
                validated_atmospheric_topic(Some(hint), raw),
                Some(expected),
                "{hint:?} should be accepted for grounded raw text {raw:?}"
            );
        }
    }

    #[test]
    fn model_atmosphere_rejects_unrelated_and_conflicting_hints() {
        assert_eq!(
            validated_atmospheric_topic(Some("omen"), "Peig, have you seen any omens here?"),
            Some(AtmosphericTopic::Omen)
        );
        assert_eq!(
            validated_atmospheric_topic(Some("omen"), "read the road signs"),
            None
        );
        assert_eq!(
            validated_atmospheric_topic(Some("folklore"), "do you hear what the land is saying?"),
            Some(AtmosphericTopic::Listen),
            "grounded text evidence wins over a conflicting model field"
        );
        assert_eq!(
            validated_atmospheric_topic(None, "what old tales are told here?"),
            Some(AtmosphericTopic::Folklore),
            "model omission must not suppress deterministic evidence"
        );
        assert_eq!(
            validated_atmospheric_topic(Some("omen"), "What traditions are kept in this parish?"),
            None,
            "evidence for another broad topic must not ground the proposed hint"
        );
        assert_eq!(
            validated_atmospheric_topic(Some("mystery"), "Can you hear the wind?"),
            None,
            "unknown model labels must be ignored even when atmospheric words exist"
        );
    }

    #[test]
    fn intent_prompt_keeps_atmosphere_supplemental_and_grounded() {
        assert!(INTENT_SYSTEM_PROMPT.contains("\"atmosphere\" supplements the main intent"));
        assert!(INTENT_SYSTEM_PROMPT.contains("talking to someone about omens remains"));
        assert!(INTENT_SYSTEM_PROMPT.contains("not listening to a person"));
        assert!(INTENT_SYSTEM_PROMPT.contains("not road signs or signposts"));
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
