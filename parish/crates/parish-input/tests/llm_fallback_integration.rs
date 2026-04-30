//! Wiremock-based integration tests for `parse_intent` async LLM fallback.
//!
//! Closes the "parish-input async LLM fallback path is unexercised" gap
//! identified in the engine audit (Tier A.2). The inline unit tests only
//! cover `parse_intent_local` (sync keyword matching). These tests spin up
//! a wiremock server and drive the LLM fallback path through success,
//! HTTP error, and malformed-JSON branches.

use parish_inference::AnyClient;
use parish_inference::openai_client::OpenAiClient;
use parish_input::{IntentKind, parse_intent};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Mount a `/v1/chat/completions` response with the given JSON content string.
async fn mount_intent_response(server: &MockServer, content: &str) {
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": content}}]
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn local_parse_bypasses_llm() {
    // "go to the pub" is a known local pattern — no HTTP call needed.
    // Point at a bogus address to prove no network call is made.
    let client = AnyClient::open_ai(OpenAiClient::new("http://127.0.0.1:1", None));
    let intent = parse_intent(&client, "go to the pub", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Move);
    assert_eq!(intent.target.as_deref(), Some("the pub"));
}

#[tokio::test]
async fn llm_fallback_success_returns_parsed_intent() {
    let server = MockServer::start().await;
    mount_intent_response(
        &server,
        r#"{"intent":"talk","target":"Mary","dialogue":"hello there"}"#,
    )
    .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    // "tell Mary hello there" — likely not matched by local parser
    let intent = parse_intent(&client, "whisper to Mary hello there", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Talk);
    assert_eq!(intent.target.as_deref(), Some("Mary"));
    assert_eq!(intent.dialogue.as_deref(), Some("hello there"));
}

#[tokio::test]
async fn llm_fallback_http_error_returns_unknown() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "do something strange", "test-model")
        .await
        .unwrap();

    assert_eq!(
        intent.intent,
        IntentKind::Unknown,
        "HTTP errors must silently fall back to Unknown"
    );
    assert_eq!(intent.raw, "do something strange");
}

#[tokio::test]
async fn llm_fallback_malformed_json_returns_unknown() {
    let server = MockServer::start().await;
    mount_intent_response(&server, "not valid json at all").await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "do something weird", "test-model")
        .await
        .unwrap();

    assert_eq!(
        intent.intent,
        IntentKind::Unknown,
        "malformed JSON must silently fall back to Unknown"
    );
}

#[tokio::test]
async fn llm_fallback_missing_intent_field_defaults_to_unknown() {
    let server = MockServer::start().await;
    mount_intent_response(&server, r#"{"target":"Mary"}"#).await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "do something with Mary", "test-model")
        .await
        .unwrap();

    assert_eq!(
        intent.intent,
        IntentKind::Unknown,
        "missing intent field must default to Unknown via serde(default)"
    );
    assert_eq!(intent.target.as_deref(), Some("Mary"));
}

#[tokio::test]
async fn llm_fallback_examine_intent() {
    let server = MockServer::start().await;
    mount_intent_response(
        &server,
        r#"{"intent":"examine","target":"the stone cross","dialogue":null}"#,
    )
    .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "inspect the stone cross closely", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Examine);
    assert_eq!(intent.target.as_deref(), Some("the stone cross"));
    assert!(intent.dialogue.is_none());
}
