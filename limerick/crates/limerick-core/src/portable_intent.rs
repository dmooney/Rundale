//! Portable player-intent interpretation shared by desktop and mobile.
//!
//! Leaf semantics live in `limerick-input`. This module is the composition
//! seam that mobile (and later thin desktop adapters) call without pulling in
//! `game_loop` / IPC / process-local inference.

use limerick_input::{
    INTENT_SYSTEM_PROMPT, IntentKind, IntentResponse, PlayerIntent, parse_intent_local,
    player_intent_from_json, player_intent_from_response,
};

/// Endpoint role string for structured intent inference.
pub const INTENT_ENDPOINT_ROLE: &str = "intent";

/// Endpoint role string for NPC dialogue generation.
pub const DIALOGUE_ENDPOINT_ROLE: &str = "npc_dialogue";

/// Returns the shared Intent Endpoint / desktop system prompt text.
pub fn intent_system_prompt() -> &'static str {
    INTENT_SYSTEM_PROMPT
}

/// Offline local interpretation. Returns `None` when the input requires the
/// Intent Endpoint (or desktop Intent LLM) path.
pub fn interpret_local(raw_input: &str) -> Option<PlayerIntent> {
    parse_intent_local(raw_input)
}

/// Validates a structured Intent Endpoint / LLM JSON candidate.
pub fn interpret_candidate_json(raw_input: &str, json: &str) -> PlayerIntent {
    player_intent_from_json(raw_input, json)
}

/// Validates an already-deserialized Intent Endpoint payload.
pub fn interpret_candidate(raw_input: &str, response: IntentResponse) -> PlayerIntent {
    player_intent_from_response(raw_input, response)
}

/// Short receipt text describing the action that will execute.
pub fn interpretation_receipt(intent: &PlayerIntent) -> String {
    match intent.intent {
        IntentKind::Move => match intent.target.as_deref() {
            Some(target) if !target.trim().is_empty() => format!("Travel to {target}."),
            _ => "Travel.".to_string(),
        },
        IntentKind::Talk => match intent.target.as_deref() {
            Some(target) if !target.trim().is_empty() => format!("Speak with {target}."),
            _ => "Speak.".to_string(),
        },
        IntentKind::Look => "Look around.".to_string(),
        IntentKind::Examine => match intent.target.as_deref() {
            Some(target) if !target.trim().is_empty() => format!("Examine {target}."),
            _ => "Examine the surroundings.".to_string(),
        },
        IntentKind::Interact => match intent.target.as_deref() {
            Some(target) if !target.trim().is_empty() => format!("Interact with {target}."),
            _ => format!("Act: {}.", intent.raw.trim()),
        },
        IntentKind::Unknown => "Continue in conversation.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inference_requiring_phrase_is_not_local() {
        assert!(interpret_local("take me to the Letter Office").is_none());
        assert!(interpret_local("bring me to Connolly Cottage").is_none());
    }

    #[test]
    fn local_move_matches_desktop_semantics() {
        let intent = interpret_local("walk to the Letter Office").expect("local move");
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("the Letter Office"));
    }

    #[test]
    fn candidate_json_applies_look_guard() {
        let intent = interpret_candidate_json(
            "Might I look about the village a while?",
            r#"{"intent":"look","target":null,"dialogue":null}"#,
        );
        assert_eq!(intent.intent, IntentKind::Unknown);
    }

    #[test]
    fn candidate_json_accepts_inferred_move() {
        let intent = interpret_candidate_json(
            "take me to the Letter Office",
            r#"{"intent":"move","target":"the Letter Office","dialogue":null}"#,
        );
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("the Letter Office"));
        assert_eq!(
            interpretation_receipt(&intent),
            "Travel to the Letter Office."
        );
    }

    /// Desktop adapters call `parse_intent_local` / `player_intent_from_*`
    /// directly; mobile goes through this seam. Prove identical semantics for
    /// the supported action set, including one inference-requiring phrase.
    #[test]
    fn desktop_and_mobile_adapters_share_intent_semantics() {
        let cases: &[(&str, Option<&str>)] = &[
            ("walk to the Letter Office", Some("move")),
            ("look around", Some("look")),
            ("examine the well", Some("examine")),
            ("pick up the stone", Some("interact")),
            ("take me to the Letter Office", None),
            ("ask Peig about the old church", None),
        ];
        for &(input, local_kind) in cases {
            let local = interpret_local(input);
            let desktop_local = parse_intent_local(input);
            assert_eq!(
                local.as_ref().map(|intent| intent.intent.clone()),
                desktop_local.as_ref().map(|intent| intent.intent.clone()),
                "local seam diverged for {input}"
            );
            assert_eq!(
                local.as_ref().and_then(|intent| intent.target.clone()),
                desktop_local
                    .as_ref()
                    .and_then(|intent| intent.target.clone()),
                "local target diverged for {input}"
            );
            match local_kind {
                Some("move") => assert_eq!(local.unwrap().intent, IntentKind::Move),
                Some("look") => assert_eq!(local.unwrap().intent, IntentKind::Look),
                Some("examine") => assert_eq!(local.unwrap().intent, IntentKind::Examine),
                Some("interact") => assert_eq!(local.unwrap().intent, IntentKind::Interact),
                None => assert!(local.is_none(), "{input} must require Intent inference"),
                Some(other) => panic!("unexpected kind fixture {other}"),
            }
        }

        let inferred_json =
            r#"{"intent":"move","target":"Letter Office","dialogue":null,"atmosphere":null}"#;
        let mobile = interpret_candidate_json("take me to the Letter Office", inferred_json);
        let desktop = player_intent_from_json("take me to the Letter Office", inferred_json);
        assert_eq!(mobile.intent, desktop.intent);
        assert_eq!(mobile.target, desktop.target);
        assert_eq!(mobile.intent, IntentKind::Move);

        let talk_json =
            r#"{"intent":"talk","target":"Peig","dialogue":"ask Peig about the old church"}"#;
        let mobile_talk = interpret_candidate_json("ask Peig about the old church", talk_json);
        let desktop_talk = player_intent_from_json("ask Peig about the old church", talk_json);
        assert_eq!(mobile_talk.intent, desktop_talk.intent);
        assert_eq!(mobile_talk.target, desktop_talk.target);
        assert_eq!(mobile_talk.intent, IntentKind::Talk);
    }

    #[test]
    fn malformed_candidate_json_does_not_invent_move() {
        let intent = interpret_candidate_json("take me to the Letter Office", "{not-json");
        assert_eq!(intent.intent, IntentKind::Unknown);
        let empty = interpret_candidate_json("take me to the Letter Office", "{}");
        assert_eq!(empty.intent, IntentKind::Unknown);
    }
}
