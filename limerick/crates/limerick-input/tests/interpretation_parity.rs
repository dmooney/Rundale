//! Cross-runtime interpretation contract (#1993).
//!
//! The desktop runtime calls a provider in-process through
//! `parse_intent_with_profile`; the mobile runtime receives the same Intent
//! role's structured output from a Limerick Endpoint and validates it with
//! `intent_from_structured_output`. Both adapters must yield the same intent
//! for the same model output, and both must consult the Intent role only for
//! input the shared local parser does not recognise.

use limerick_inference::AnyClient;
use limerick_inference::openai_client::OpenAiClient;
use limerick_input::{
    IntentKind, LocalInterpretation, PlayerIntent, intent_from_structured_output,
    intent_system_prompt, interpret_locally, parse_intent,
};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn summary(intent: &PlayerIntent) -> (IntentKind, Option<String>, Option<String>) {
    (
        intent.intent.clone(),
        intent.target.clone(),
        intent.dialogue.clone(),
    )
}

/// Inputs from the current authored mobile world that the local parser does
/// not recognise, paired with the Intent output a model would return.
const INFERRED_CASES: &[(&str, &str)] = &[
    (
        "Let's make for the Letter Office",
        r#"{"intent":"move","target":"the Letter Office","dialogue":null,"atmosphere":null}"#,
    ),
    (
        "Would Peig know anything of the post today?",
        r#"{"intent":"talk","target":"Peig","dialogue":"Would Peig know anything of the post today?","atmosphere":null}"#,
    ),
    (
        "Might I have a word with Róisín about the yarn?",
        r#"{"intent":"talk","target":"Róisín","dialogue":"about the yarn","atmosphere":null}"#,
    ),
    (
        "hey everybody",
        r#"{"intent":"look","target":null,"dialogue":null,"atmosphere":null}"#,
    ),
    (
        "shove the cart aside",
        r#"{"intent":"interact","target":"the cart","dialogue":null,"atmosphere":null}"#,
    ),
];

#[tokio::test]
async fn desktop_and_mobile_adapters_share_intent_semantics() {
    for (input, output) in INFERRED_CASES {
        assert!(
            matches!(
                interpret_locally(input),
                LocalInterpretation::RequiresInference
            ),
            "{input:?} must require the Intent role, not a local parse"
        );

        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            // The desktop request carries the same shared Intent prompt.
            .and(body_string_contains("text adventure input parser"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "choices": [{
                    "message": {"role": "assistant", "content": output},
                    "finish_reason": "stop"
                }]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
        let desktop = parse_intent(&client, input, "test-model").await.unwrap();

        let mobile = intent_from_structured_output(&serde_json::from_str(output).unwrap(), input)
            .expect("valid mobile Intent output");
        assert_eq!(summary(&desktop), summary(&mobile), "{input:?}");
        assert_eq!(desktop.atmosphere, mobile.atmosphere, "{input:?}");
        server.verify().await;
    }
}

#[tokio::test]
async fn locally_recognised_input_makes_no_intent_request_in_either_adapter() {
    // A bogus address proves no network call is made on the desktop path.
    let client = AnyClient::open_ai(OpenAiClient::new("http://127.0.0.1:1", None));
    for input in [
        "go to the Letter Office",
        "go east",
        "look",
        "I came from Roscommon",
    ] {
        let desktop = parse_intent(&client, input, "test-model").await.unwrap();
        let LocalInterpretation::Resolved(mobile) = interpret_locally(input) else {
            panic!("{input:?} should resolve locally");
        };
        assert_eq!(summary(&desktop), summary(&mobile), "{input:?}");
    }
}

#[test]
fn intent_prompt_is_the_shared_constant() {
    assert!(intent_system_prompt().starts_with("You are a text adventure input parser."));
    assert!(intent_system_prompt().ends_with("Respond ONLY with valid JSON. No explanation."));
}
