//! Portable player-input interpretation.
//!
//! Runtimes that cannot call an inference provider in-process (a host that
//! reaches models only through Limerick Endpoints) still need the same
//! interpretation flow as the desktop parser: deterministic local parsing
//! first, then the Intent role for input the local parser does not recognise,
//! then the shared post-model validation. This module exposes those steps
//! without any transport, so each runtime supplies only its own model call.

use serde::Deserialize;

use crate::intent_llm::{INTENT_SYSTEM_PROMPT, IntentResponse, validated_intent};
use crate::intent_local::parse_intent_local;
use crate::intent_types::{IntentKind, PlayerIntent};

/// Maximum length of a model-supplied `target` or `dialogue` field.
pub const MAX_INTENT_FIELD_CHARS: usize = 512;

/// The first, offline interpretation step for free-form input.
#[derive(Debug, Clone)]
pub enum LocalInterpretation {
    /// The deterministic parser recognised the input; no model call is needed.
    Resolved(PlayerIntent),
    /// The input must be interpreted by the Intent role before any action
    /// executes.
    RequiresInference,
}

/// Runs the deterministic interpretation step used by every runtime before
/// the Intent role is requested.
pub fn interpret_locally(raw_input: &str) -> LocalInterpretation {
    match parse_intent_local(raw_input) {
        Some(intent) => LocalInterpretation::Resolved(intent),
        None => LocalInterpretation::RequiresInference,
    }
}

/// The Intent role's system instructions. Runtimes that publish the role
/// through a remote Endpoint definition must use this text verbatim.
pub fn intent_system_prompt() -> &'static str {
    INTENT_SYSTEM_PROMPT
}

/// Why a structured Intent result was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IntentOutputError {
    /// The value is not the Intent result object (wrong JSON type, unknown
    /// fields, or an unsupported intent label).
    Malformed(String),
    /// The required `intent` field is absent.
    MissingIntent,
    /// A text field exceeds [`MAX_INTENT_FIELD_CHARS`].
    FieldTooLong(&'static str),
}

impl std::fmt::Display for IntentOutputError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(reason) => write!(formatter, "malformed intent result: {reason}"),
            Self::MissingIntent => formatter.write_str("intent result has no intent"),
            Self::FieldTooLong(field) => write!(formatter, "intent result {field} is too long"),
        }
    }
}

impl std::error::Error for IntentOutputError {}

/// The exact structured output accepted from a remote Intent Endpoint.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictIntentOutput {
    intent: Option<IntentKind>,
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    dialogue: Option<String>,
    #[serde(default)]
    atmosphere: Option<String>,
}

/// Validates one structured Intent result and applies the shared engine
/// semantics (look/examine guard, grounded atmosphere).
///
/// Unlike the desktop in-process parser, which treats a failed provider call
/// as `Unknown`, this strict adapter reports malformed output so a caller that
/// owns a durable request can reject it explicitly instead of guessing.
pub fn intent_from_structured_output(
    output: &serde_json::Value,
    raw_input: &str,
) -> Result<PlayerIntent, IntentOutputError> {
    let strict: StrictIntentOutput = serde_json::from_value(output.clone())
        .map_err(|error| IntentOutputError::Malformed(error.to_string()))?;
    let intent = strict.intent.ok_or(IntentOutputError::MissingIntent)?;
    let bounded = |value: Option<String>, field: &'static str| {
        let value = value
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty());
        match value {
            Some(text) if text.chars().count() > MAX_INTENT_FIELD_CHARS => {
                Err(IntentOutputError::FieldTooLong(field))
            }
            other => Ok(other),
        }
    };
    let response = IntentResponse {
        intent: Some(intent),
        target: bounded(strict.target, "target")?,
        dialogue: bounded(strict.dialogue, "dialogue")?,
        atmosphere: strict.atmosphere,
    };
    Ok(validated_intent(response, raw_input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn local_step_resolves_known_commands_and_defers_the_rest() {
        assert!(matches!(
            interpret_locally("go to the Letter Office"),
            LocalInterpretation::Resolved(PlayerIntent {
                intent: IntentKind::Move,
                ..
            })
        ));
        assert!(matches!(
            interpret_locally("Let's make for the Letter Office"),
            LocalInterpretation::RequiresInference
        ));
    }

    #[test]
    fn structured_output_uses_shared_guards() {
        let intent = intent_from_structured_output(
            &json!({"intent": "look", "target": null, "dialogue": null}),
            "hey everybody",
        )
        .unwrap();
        assert_eq!(intent.intent, IntentKind::Unknown, "#1276 guard applies");

        let intent = intent_from_structured_output(
            &json!({"intent": "move", "target": " the Letter Office ", "dialogue": null, "atmosphere": null}),
            "Let's make for the Letter Office",
        )
        .unwrap();
        assert_eq!(intent.intent, IntentKind::Move);
        assert_eq!(intent.target.as_deref(), Some("the Letter Office"));
    }

    #[test]
    fn malformed_or_unsupported_output_is_rejected() {
        for (value, expected) in [
            (json!("move"), "malformed"),
            (json!({"intent": "fly", "target": "moon"}), "malformed"),
            (json!({"intent": "move", "extra": true}), "malformed"),
            (json!({"target": "the Letter Office"}), "missing"),
            (json!({"intent": null}), "missing"),
            (
                json!({"intent": "talk", "target": "x".repeat(MAX_INTENT_FIELD_CHARS + 1)}),
                "too long",
            ),
        ] {
            let error = intent_from_structured_output(&value, "anything").unwrap_err();
            let text = error.to_string();
            assert!(
                text.contains(expected) || (expected == "missing" && text.contains("no intent")),
                "{value}: {text}"
            );
        }
    }
}
