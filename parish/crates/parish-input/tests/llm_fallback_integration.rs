//! Wiremock-based integration tests for `parse_intent` async LLM fallback.
//!
//! Closes the "parish-input async LLM fallback path is unexercised" gap
//! identified in the engine audit (Tier A.2). The inline unit tests only
//! cover `parse_intent_local` (sync keyword matching). These tests spin up
//! a wiremock server and drive the LLM fallback path through success,
//! HTTP error, and malformed-JSON branches.

use parish_inference::AnyClient;
use parish_inference::openai_client::OpenAiClient;
use parish_input::{AtmosphericTopic, IntentKind, parse_intent};
use wiremock::matchers::{body_partial_json, body_string_contains, method, path};
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
async fn unknown_atmosphere_label_preserves_valid_primary_intent() {
    let server = MockServer::start().await;
    mount_intent_response(
        &server,
        r#"{"intent":"talk","target":"Mary","dialogue":"hello there","atmosphere":"mystery"}"#,
    )
    .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "whisper to Mary hello there", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Talk);
    assert_eq!(intent.target.as_deref(), Some("Mary"));
    assert_eq!(intent.dialogue.as_deref(), Some("hello there"));
    assert_eq!(intent.atmosphere, None);
}

#[tokio::test]
async fn grounded_model_atmosphere_extends_synonym_coverage() {
    let server = MockServer::start().await;
    mount_intent_response(
        &server,
        r#"{"intent":"talk","target":"Peig","dialogue":"can you hear the wind rising?","atmosphere":"listen"}"#,
    )
    .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "Peig, can you hear the wind rising?", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Talk);
    assert_eq!(intent.target.as_deref(), Some("Peig"));
    assert_eq!(intent.atmosphere, Some(AtmosphericTopic::Listen));
}

#[tokio::test]
async fn llm_fallback_posts_intent_request_contract() {
    // Verifies the HTTP contract: system prompt present, correct model, input
    // forwarded. Uses a genuine look command so the guard does not downgrade
    // the response (#1276). "look at the old mill" is an imperative observation
    // that matches the is_genuine_look_input whitelist prefix "look at ".
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({
            "model": "intent-model",
            "stream": false
        })))
        .and(body_string_contains(r#""role":"system""#))
        .and(body_string_contains(r#""role":"user""#))
        .and(body_string_contains("look at the old mill"))
        .and(body_string_contains("text adventure input parser"))
        .and(body_string_contains("Respond ONLY with valid JSON"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": r#"{"intent":"look","target":"the old mill","dialogue":null}"#}}]
        })))
        .mount(&server)
        .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    let intent = parse_intent(&client, "look at the old mill", "intent-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Look);
    assert_eq!(intent.target.as_deref(), Some("the old mill"));
    assert!(intent.dialogue.is_none());
}

/// Regression (#1276): a conversational input the LLM misclassifies as Look
/// must be downgraded to Unknown by the is_genuine_look_input guard.
#[tokio::test]
async fn llm_look_misclassification_downgraded_to_unknown() {
    let server = MockServer::start().await;
    // Stub: LLM returns "look" for a clearly conversational input.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"role": "assistant", "content": r#"{"intent":"look","target":null,"dialogue":null}"#}}]
        })))
        .mount(&server)
        .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    // "ponder the old mill" / "hey everybody" / "no reason" — not look commands.
    for input in ["ponder the old mill", "hey everybody", "no reason"] {
        // Re-mount per iteration — wiremock resets between mount calls.
        let intent = parse_intent(&client, input, "intent-model").await.unwrap();
        assert_eq!(
            intent.intent,
            IntentKind::Unknown,
            "'{input}': LLM-Look misclassification must be downgraded to Unknown (#1276)"
        );
    }
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

/// Since #1424 added deterministic local parsing for "inspect <target>",
/// "inspect the stone cross closely" is now classified locally as Examine
/// (target = "the stone cross closely") without an LLM call.
///
/// This test verifies the local-parse path produces `Examine`.  A separate
/// test exercises the LLM fallback path for examine by using an input that
/// does not match any local examine prefix.
#[tokio::test]
async fn local_parse_inspect_produces_examine_intent() {
    // Point at a bogus address to prove no network call is made.
    let client = AnyClient::open_ai(OpenAiClient::new("http://127.0.0.1:1", None));
    let intent = parse_intent(&client, "inspect the stone cross closely", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Examine);
    // Local parser captures the full suffix after the "inspect " prefix.
    assert_eq!(intent.target.as_deref(), Some("the stone cross closely"));
    assert!(intent.dialogue.is_none());
}

/// LLM fallback for examine: "look at X" is not locally classified as Examine
/// (intentionally, to preserve the existing HTTP contract test), so it falls
/// through to the LLM.  The LLM may return "examine"; `is_genuine_look_input`
/// accepts "look at ..." so the result is not downgraded.
#[tokio::test]
async fn llm_fallback_examine_intent() {
    let server = MockServer::start().await;
    mount_intent_response(
        &server,
        r#"{"intent":"examine","target":"the old well","dialogue":null}"#,
    )
    .await;

    let client = AnyClient::open_ai(OpenAiClient::new(&server.uri(), None));
    // "look at the old well" — not locally classified as Examine (we removed
    // "look at " from the local examine prefix list); falls through to LLM.
    // is_genuine_look_input("look at the old well") is true → Examine not downgraded.
    let intent = parse_intent(&client, "look at the old well", "test-model")
        .await
        .unwrap();

    assert_eq!(intent.intent, IntentKind::Examine);
    assert_eq!(intent.target.as_deref(), Some("the old well"));
    assert!(intent.dialogue.is_none());
}
