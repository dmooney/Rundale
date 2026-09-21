//! LLM-backed intent parsing.
//!
//! Routes ambiguous free-form input through the configured inference
//! client. Falls back to [`IntentKind::Unknown`] if the LLM call fails.
//!
//! The prompt, payload shape, and post-response validation live in
//! [`crate::intent_contract`] so the mobile Endpoint adapter interprets player
//! input with exactly these semantics rather than a parallel rule set.

use limerick_inference::{AnyClient, GenerateParams};
use limerick_types::LimerickError;

use crate::intent_contract::{
    INTENT_SYSTEM_PROMPT, IntentPayload, interpret_intent_payload, unresolved_intent,
};
use crate::intent_local::parse_intent_local;
use crate::intent_types::PlayerIntent;

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
) -> Result<PlayerIntent, LimerickError> {
    parse_intent_with_profile(
        client,
        raw_input,
        model,
        limerick_config::InferenceProfile::for_subrole(limerick_config::InferenceSubrole::Intent),
    )
    .await
}

/// Parses intent using a fully resolved runtime inference profile.
pub async fn parse_intent_with_profile(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
    profile: limerick_config::InferenceProfile,
) -> Result<PlayerIntent, LimerickError> {
    parse_intent_with_profile_and_audit(client, raw_input, model, profile, None).await
}

/// Parses intent and records native provider metadata in the common audit sink.
pub async fn parse_intent_with_profile_and_audit(
    client: &AnyClient,
    raw_input: &str,
    model: &str,
    profile: limerick_config::InferenceProfile,
    audit_sink: Option<limerick_inference::InferenceAuditSink>,
) -> Result<PlayerIntent, LimerickError> {
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
    let audit = limerick_inference::DirectInferenceAudit::new(
        audit_sink,
        model,
        raw_input,
        Some(INTENT_SYSTEM_PROMPT),
        limerick_config::InferenceSubrole::Intent,
        false,
        params.max_tokens,
        params.thinking_level,
        params.service_tier,
        params.temperature,
        limerick_inference::InferencePriority::Interactive,
    );
    let detailed = client
        .generate_detailed_with_format(
            model,
            raw_input,
            Some(INTENT_SYSTEM_PROMPT),
            Some(limerick_inference::ResponseFormat::JsonObject),
            params,
        )
        .await
        .and_then(|result| {
            limerick_inference::parse_generation_json::<IntentPayload>(result, "intent")
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
        Ok(payload) => Ok(interpret_intent_payload(raw_input, payload)),
        Err(_) => Ok(unresolved_intent(raw_input)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intent_contract::{is_genuine_look_input, validated_atmospheric_topic};
    use crate::intent_local::detect_atmospheric_topic;
    use crate::intent_types::{AtmosphericTopic, IntentKind};

    #[test]
    fn test_intent_response_deserialize() {
        let json =
            r#"{"intent": "move", "target": "the pub", "dialogue": null, "atmosphere": "omen"}"#;
        let resp: IntentPayload = serde_json::from_str(json).unwrap();
        assert_eq!(resp.intent, Some(IntentKind::Move));
        assert_eq!(resp.target, Some("the pub".to_string()));
        assert!(resp.dialogue.is_none());
        assert_eq!(resp.atmosphere.as_deref(), Some("omen"));
    }
    #[test]
    fn test_intent_response_empty() {
        let json = r#"{}"#;
        let resp: IntentPayload = serde_json::from_str(json).unwrap();
        assert!(resp.intent.is_none());
        assert!(resp.target.is_none());
        assert!(resp.dialogue.is_none());
        assert!(resp.atmosphere.is_none());
    }

    #[test]
    fn unknown_model_atmosphere_does_not_invalidate_primary_intent() {
        let json = r#"{"intent":"talk","target":"Peig","dialogue":"hello","atmosphere":"mystery"}"#;
        let resp: IntentPayload = serde_json::from_str(json).unwrap();
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
