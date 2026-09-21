//! Portable intent interpretation contract.
//!
//! This module owns the *runtime-independent* half of intent inference: the
//! system prompt, the wire payload shape, and the validation that turns a
//! model payload into a typed [`PlayerIntent`]. Nothing here performs I/O, so
//! every adapter — the desktop inference client in [`crate::intent_llm`] and
//! the mobile Limerick Endpoints transport in `limerick_core::mobile` — shares
//! one set of intent semantics instead of maintaining a second parser.
//!
//! Adapters are expected to call [`crate::parse_intent_local`] first; only
//! input the local parser does not recognise needs an inference round trip.

use serde::{Deserialize, Serialize};

use crate::intent_local::{detect_atmospheric_topic, supports_atmospheric_topic_hint};
use crate::intent_types::{AtmosphericTopic, IntentKind, PlayerIntent};

/// The system prompt used for intent parsing.
///
/// Owned here so every runtime — desktop provider client and mobile Endpoint
/// alike — classifies player input against the same instructions. The mobile
/// Intent Endpoint contract embeds this exact text; `intent_endpoint_contract`
/// in `limerick-core` fails if the published contract drifts from it.
pub const INTENT_SYSTEM_PROMPT: &str = "\
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

/// The structured payload an intent inference call is required to return.
///
/// This is the wire shape shared by the desktop provider client and the
/// mobile Intent Endpoint contract. Every field is optional so a partial or
/// unfamiliar response degrades to [`IntentKind::Unknown`] instead of failing
/// the player's request.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct IntentPayload {
    #[serde(default)]
    pub intent: Option<IntentKind>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub dialogue: Option<String>,
    #[serde(default)]
    pub atmosphere: Option<String>,
}

/// Why a transport-supplied intent payload could not be interpreted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentInterpretationError {
    /// The payload was not valid JSON matching [`IntentPayload`].
    Malformed(String),
}

impl std::fmt::Display for IntentInterpretationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(detail) => write!(formatter, "malformed intent payload: {detail}"),
        }
    }
}

impl std::error::Error for IntentInterpretationError {}

/// Applies the shared validation to a model-proposed intent payload.
///
/// The guards are deliberately identical for every adapter:
///
/// * a missing or unrecognised `intent` degrades to [`IntentKind::Unknown`],
///   which callers route to conversation exactly as the desktop game loop does;
/// * `Look`/`Examine` is accepted only when the raw text actually looks like an
///   observation command (#1276);
/// * `atmosphere` is accepted only when the raw text independently supports it.
pub fn interpret_intent_payload(raw_input: &str, payload: IntentPayload) -> PlayerIntent {
    let mut intent = payload.intent.unwrap_or(IntentKind::Unknown);
    // Guard: downgrade spurious Look/Examine classifications (#1276).
    // Small quantised models occasionally classify conversational input
    // (e.g. "hey everybody", "no reason") as Look, which would cause the
    // location description blurb to fire unexpectedly.  Accept Look/Examine
    // only when the raw input actually resembles an observation command.
    if matches!(intent, IntentKind::Look | IntentKind::Examine) && !is_genuine_look_input(raw_input)
    {
        // Downgrade spurious Look/Examine — caller routes to NPC
        // conversation instead of printing the location blurb (#1276).
        intent = IntentKind::Unknown;
    }
    PlayerIntent {
        intent,
        target: payload.target,
        dialogue: payload.dialogue,
        atmosphere: validated_atmospheric_topic(payload.atmosphere.as_deref(), raw_input),
        raw: raw_input.to_string(),
    }
}

/// Parses and validates a raw JSON intent payload from a transport.
///
/// Used by adapters whose transport hands back bytes rather than a typed
/// value. Malformed JSON is reported rather than silently downgraded so the
/// caller can reject the attempt instead of committing a guessed action.
pub fn interpret_intent_json(
    raw_input: &str,
    payload: &str,
) -> Result<PlayerIntent, IntentInterpretationError> {
    let parsed: IntentPayload = serde_json::from_str(payload)
        .map_err(|error| IntentInterpretationError::Malformed(error.to_string()))?;
    Ok(interpret_intent_payload(raw_input, parsed))
}

/// The intent used when inference is unavailable or failed.
///
/// Deterministic atmospheric evidence is still retained so enrichment does not
/// depend on provider availability.
pub fn unresolved_intent(raw_input: &str) -> PlayerIntent {
    PlayerIntent {
        intent: IntentKind::Unknown,
        target: None,
        dialogue: None,
        atmosphere: detect_atmospheric_topic(raw_input),
        raw: raw_input.to_string(),
    }
}

/// Accepts an LLM-proposed topic only when the raw text independently
/// contains evidence for that same topic. Deterministic evidence is retained
/// when the model omits the optional field, so atmospheric enrichment does not
/// depend on model size or provider availability.
pub(crate) fn validated_atmospheric_topic(
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
pub(crate) fn is_genuine_look_input(raw_input: &str) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The portable seam must reproduce the desktop parser's typed result for
    /// the same payload, including the #1276 look guard, so a mobile adapter
    /// cannot drift into a second set of intent rules.
    #[test]
    fn payload_interpretation_matches_desktop_semantics() {
        let moved = interpret_intent_payload(
            "I'd best call on the letter office",
            IntentPayload {
                intent: Some(IntentKind::Move),
                target: Some("Letter Office".to_string()),
                ..IntentPayload::default()
            },
        );
        assert_eq!(moved.intent, IntentKind::Move);
        assert_eq!(moved.target.as_deref(), Some("Letter Office"));
        assert_eq!(moved.raw, "I'd best call on the letter office");

        // Spurious Look on conversational text downgrades to Unknown so the
        // caller routes to conversation rather than printing the location blurb.
        let spurious = interpret_intent_payload(
            "hey everybody",
            IntentPayload {
                intent: Some(IntentKind::Look),
                ..IntentPayload::default()
            },
        );
        assert_eq!(spurious.intent, IntentKind::Unknown);

        let genuine = interpret_intent_payload(
            "look around",
            IntentPayload {
                intent: Some(IntentKind::Look),
                ..IntentPayload::default()
            },
        );
        assert_eq!(genuine.intent, IntentKind::Look);
    }

    #[test]
    fn missing_intent_field_degrades_to_unknown() {
        let intent = interpret_intent_payload("who is about?", IntentPayload::default());
        assert_eq!(intent.intent, IntentKind::Unknown);
        assert!(intent.target.is_none());
    }

    #[test]
    fn json_seam_parses_and_validates() {
        let intent = interpret_intent_json(
            "I'd best call on the letter office",
            r#"{"intent":"move","target":"Letter Office","dialogue":null,"atmosphere":null}"#,
        )
        .expect("well-formed payload");
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("Letter Office"));
    }

    /// A transport that returns something other than the contract payload must
    /// be reported, not silently downgraded — the caller rejects the attempt
    /// instead of committing a guessed action.
    #[test]
    fn json_seam_reports_malformed_payloads() {
        let error = interpret_intent_json("anything", "not json at all")
            .expect_err("malformed payload must be reported");
        assert!(matches!(error, IntentInterpretationError::Malformed(_)));
        assert!(error.to_string().contains("malformed intent payload"));

        assert!(
            interpret_intent_json("anything", r#"{"intent":"teleport"}"#).is_err(),
            "an unknown intent label is not part of the contract"
        );
    }

    #[test]
    fn unresolved_intent_keeps_deterministic_atmosphere() {
        let intent = unresolved_intent("what old tales are told here?");
        assert_eq!(intent.intent, IntentKind::Unknown);
        assert_eq!(intent.atmosphere, Some(AtmosphericTopic::Folklore));
    }
}
