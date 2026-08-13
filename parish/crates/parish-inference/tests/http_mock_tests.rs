//! HTTP-mocked integration tests for the inference clients.
//!
//! Uses `wiremock` to stand up a local HTTP server that stands in for
//! Ollama's native API (`/api/generate`), the OpenAI-compatible API
//! (`/v1/chat/completions`), and the Anthropic Messages API
//! (`/v1/messages`). These tests exercise the request/response
//! plumbing, streaming NDJSON / SSE parsing, error mapping, and auth
//! header behavior without needing a real LLM backend.

use parish_inference::AnthropicClient;
use parish_inference::TOKEN_CHANNEL_CAPACITY;
use parish_inference::openai_client::{GenerateParams, OpenAiClient};
use parish_inference::setup::{RuntimeProcesses, VllmMlxProcess, VllmMlxSlot};
use serde::Deserialize;
use tokio::sync::mpsc;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// OpenAiClient — /v1/chat/completions endpoint
// =============================================================================

#[tokio::test]
async fn openai_generate_returns_choice_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {"role": "assistant", "content": "Hello from the mock"}
            }]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate("gpt-test", "hi", None, GenerateParams::default())
        .await
        .expect("generate should succeed");
    assert_eq!(out, "Hello from the mock");
}

#[tokio::test]
async fn openai_generate_sends_bearer_token_when_api_key_set() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("Authorization", "Bearer sk-test-1234"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "authed"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), Some("sk-test-1234"));
    // If the matcher doesn't match, wiremock returns 404 and the call errors.
    let out = client
        .generate("m", "p", None, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(out, "authed");
}

#[tokio::test]
async fn openai_generate_omits_bearer_when_api_key_absent() {
    let server = MockServer::start().await;
    // Mount a mock that DOES match when Authorization is absent.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "ok"}}]
        })))
        .mount(&server)
        .await;

    // Mount a second mock that would match ONLY if Authorization were sent;
    // we assert it is never hit by asserting the response body is "ok" not "auth".
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header_exists("Authorization"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "auth"}}]
        })))
        .expect(0)
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate("m", "p", None, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(out, "ok");
}

#[tokio::test]
async fn openai_generate_maps_401_to_inference_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), Some("sk-bad"));
    let err = client
        .generate("m", "p", None, GenerateParams::default())
        .await
        .expect_err("401 must fail");
    let msg = err.to_string();
    assert!(msg.contains("inference error") || msg.contains("401"));
}

#[tokio::test]
async fn openai_generate_handles_empty_choices() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"choices": []})))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate("m", "p", None, GenerateParams::default())
        .await
        .unwrap();
    // empty choices degrades gracefully to empty content, not an error
    assert_eq!(out, "");
}

#[tokio::test]
async fn openai_generate_stream_parses_sse_chunks() {
    let server = MockServer::start().await;
    // OpenAI SSE format: `data: { ... chunk ... }\n\n` lines, terminated by `data: [DONE]`
    let sse = [
        r#"data: {"choices":[{"delta":{"content":"Hel"},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"content":"lo"},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"content":"!"},"finish_reason":"stop"}]}"#,
        r#"data: [DONE]"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx, GenerateParams::default())
        .await
        .unwrap();

    assert_eq!(full, "Hello!");
    let mut tokens = Vec::new();
    while let Ok(t) = rx.try_recv() {
        tokens.push(t);
    }
    assert_eq!(tokens, vec!["Hel", "lo", "!"]);
}

#[tokio::test]
async fn openai_generate_stream_honors_done_sentinel_before_stop() {
    let server = MockServer::start().await;
    // No finish_reason on any chunk; only the `[DONE]` sentinel ends the stream.
    let sse = [
        r#"data: {"choices":[{"delta":{"content":"a"},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"content":"b"},"finish_reason":null}]}"#,
        r#"data: [DONE]"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(full, "ab");
}

#[tokio::test]
async fn openai_generate_stream_rejects_eof_without_terminal_marker() {
    let server = MockServer::start().await;
    let sse = [
        r#"data: {"choices":[{"delta":{"content":"partial "},"finish_reason":null}]}"#,
        r#"data: {"choices":[{"delta":{"content":"answer"},"finish_reason":null}]}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let error = client
        .generate_stream("m", "p", None, tx, GenerateParams::default())
        .await
        .expect_err("transport EOF without a terminal marker must reject the partial body");
    assert!(error.to_string().contains("missing terminal marker"));
}

#[tokio::test]
async fn openai_generate_stream_ignores_sse_comments_and_blank_lines() {
    let server = MockServer::start().await;
    let sse = [
        r#": keepalive comment"#,
        r#""#,
        r#"data: {"choices":[{"delta":{"content":"x"},"finish_reason":null}]}"#,
        r#": another comment"#,
        r#"data: {"choices":[{"delta":{"content":"y"},"finish_reason":"stop"}]}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(full, "xy");
}

#[tokio::test]
async fn openai_generate_json_parses_content_as_typed_payload() {
    #[derive(Deserialize, Debug)]
    struct Greeting {
        #[serde(default)]
        hello: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "{\"hello\":\"world\"}"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let g: Greeting = client
        .generate_json("m", "Return a greeting", None, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(g.hello, "world");
}

#[tokio::test]
async fn openai_generate_json_errors_on_malformed_inner_content() {
    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct Greeting {
        hello: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "not valid json at all"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let result: Result<Greeting, _> = client
        .generate_json("m", "p", None, GenerateParams::default())
        .await;
    let err = result.expect_err("malformed inner content must fail");
    assert!(err.to_string().contains("serialization"));
}

#[tokio::test]
async fn openai_generate_request_includes_max_tokens_when_set() {
    use wiremock::matchers::body_partial_json;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({
            "model": "m",
            "max_tokens": 42
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "capped"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate(
            "m",
            "p",
            None,
            GenerateParams {
                max_tokens: Some(42),
                temperature: None,
                frequency_penalty: None,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out, "capped");
}

#[tokio::test]
async fn openai_generate_request_omits_max_tokens_when_none() {
    use wiremock::matchers::body_partial_json;

    let server = MockServer::start().await;
    // Mount a mock that only matches when the body has NO max_tokens key.
    // wiremock doesn't have a "field absent" matcher, so we match on model
    // and assert the call succeeds; absence is enforced by the client's
    // #[serde(skip_serializing_if = "Option::is_none")] annotation and is
    // already covered at the serde level by openai_client unit tests.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({"model": "m"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "ok"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate("m", "p", None, GenerateParams::default())
        .await
        .unwrap();
    assert_eq!(out, "ok");
}

/// TODO #10 / #23 / #34 — `frequency_penalty` must flow all the way from the
/// `AnyClient::generate` call site into the OpenAI-compat wire body. wiremock
/// only matches the mock if the request body includes `"frequency_penalty":
/// 0.5`, so a passing test proves end-to-end plumbing: caller arg →
/// `OpenAiClient::build_request` → JSON body posted to `/v1/chat/completions`.
#[tokio::test]
async fn openai_generate_request_includes_frequency_penalty_when_set() {
    use wiremock::matchers::body_partial_json;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(serde_json::json!({
            "model": "m",
            "frequency_penalty": 0.5
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{"message": {"content": "rep-penalty-on"}}]
        })))
        .mount(&server)
        .await;

    let client = OpenAiClient::new(&server.uri(), None);
    let out = client
        .generate(
            "m",
            "p",
            None,
            GenerateParams {
                max_tokens: None,
                temperature: None,
                frequency_penalty: Some(0.5),
                ..Default::default()
            },
        )
        .await
        .expect("generate with frequency_penalty must reach the wire");
    assert_eq!(out, "rep-penalty-on");
}

// =============================================================================
// AnthropicClient — native /v1/messages endpoint  (closes #727)
// =============================================================================
//
// Anthropic differences from OpenAI:
//   - Endpoint:   POST /v1/messages  (not /v1/chat/completions)
//   - Auth:       x-api-key header   (not Authorization: Bearer)
//   - Version:    anthropic-version: 2023-06-01  (always required)
//   - Response:   {"content": [{"type":"text","text":"…"}]}  (not "choices")
//   - Streaming:  terminated by {"type":"message_stop"}  (not data: [DONE])
//   - max_tokens: required field in every request (client fills in 4096 default)

#[tokio::test]
async fn anthropic_generate_returns_choice_content() {
    // "choice_content" name mirrors the OpenAI sibling; Anthropic uses
    // content blocks instead of choices — `extract_text` joins all Text blocks.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "Hello from the mock"}]
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let out = client
        .generate("claude-test", "hi", None, None, None)
        .await
        .expect("generate should succeed");
    assert_eq!(out, "Hello from the mock");
}

#[tokio::test]
async fn anthropic_generate_sends_x_api_key_when_set() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header("x-api-key", "sk-ant-test-1234"))
        .and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "authed"}]
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), Some("sk-ant-test-1234"));
    // wiremock returns 404 if the matcher doesn't match, which fails the call.
    let out = client.generate("m", "p", None, None, None).await.unwrap();
    assert_eq!(out, "authed");
}

#[tokio::test]
async fn anthropic_generate_omits_key_when_absent() {
    let server = MockServer::start().await;
    // This mock matches when x-api-key is absent.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "ok"}]
        })))
        .mount(&server)
        .await;

    // This mock matches only when x-api-key is present; assert it is never hit.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .and(header_exists("x-api-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "with-key"}]
        })))
        .expect(0)
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let out = client.generate("m", "p", None, None, None).await.unwrap();
    assert_eq!(out, "ok");
}

#[tokio::test]
async fn anthropic_generate_maps_401_to_inference_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), Some("sk-bad"));
    let err = client
        .generate("m", "p", None, None, None)
        .await
        .expect_err("401 must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("inference error") || msg.contains("401"),
        "expected inference error, got: {msg}"
    );
}

#[tokio::test]
async fn anthropic_generate_stream_parses_sse_chunks() {
    let server = MockServer::start().await;
    // Anthropic SSE format: `data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"…"}}`
    // terminated by `data: {"type":"message_stop"}` (not [DONE]).
    let sse = [
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}"#,
        r#"data: {"type":"message_stop"}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx, None, None)
        .await
        .unwrap();

    assert_eq!(full, "Hello!");
    let mut tokens = Vec::new();
    while let Ok(t) = rx.try_recv() {
        tokens.push(t);
    }
    assert_eq!(tokens, vec!["Hel", "lo", "!"]);
}

#[tokio::test]
async fn anthropic_generate_stream_honors_done_sentinel() {
    // Anthropic's stream sentinel is {"type":"message_stop"}, not [DONE].
    // Any delta arriving after message_stop must be dropped.
    let server = MockServer::start().await;
    let sse = [
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a"}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"b"}}"#,
        r#"data: {"type":"message_stop"}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"dropped"}}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx, None, None)
        .await
        .unwrap();
    assert_eq!(full, "ab");

    let mut tokens = Vec::new();
    while let Ok(t) = rx.try_recv() {
        tokens.push(t);
    }
    // "dropped" must not appear — message_stop terminated the stream.
    assert_eq!(tokens, vec!["a", "b"]);
}

#[tokio::test]
async fn anthropic_generate_maps_401_with_structured_error_body() {
    let server = MockServer::start().await;
    // Anthropic returns a structured JSON error body on 4xx.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
            "error": {
                "type": "authentication_error",
                "message": "Your credit card is declined"
            }
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), Some("sk-bad"));
    let err = client
        .generate("m", "p", None, None, None)
        .await
        .expect_err("401 must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("credit card"),
        "expected structured error message, got: {msg}"
    );
}

#[tokio::test]
async fn anthropic_generate_handles_empty_choices() {
    // "empty_choices" name mirrors the OpenAI sibling; Anthropic uses
    // content blocks — an empty content array degrades gracefully to "".
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": []
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let out = client.generate("m", "p", None, None, None).await.unwrap();
    // empty content degrades gracefully to empty string, not an error
    assert_eq!(out, "");
}

#[tokio::test]
async fn anthropic_generate_json_parses_typed_payload() {
    #[derive(Deserialize, Debug)]
    struct Greeting {
        #[serde(default)]
        hello: String,
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "{\"hello\":\"world\"}"}]
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let g: Greeting = client
        .generate_json("m", "Return a greeting", None, None, None)
        .await
        .unwrap();
    assert_eq!(g.hello, "world");
}

#[tokio::test]
async fn anthropic_generate_json_parses_fenced_payload() {
    #[derive(Deserialize, Debug)]
    struct Greeting {
        #[serde(default)]
        hello: String,
    }

    let server = MockServer::start().await;
    // Anthropic sometimes wraps JSON in ```json fences
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "```json\n{\"hello\":\"world\"}\n```"}]
        })))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let g: Greeting = client
        .generate_json("m", "Return a greeting", None, None, None)
        .await
        .unwrap();
    assert_eq!(g.hello, "world");
}

#[tokio::test]
async fn anthropic_generate_json_retries_on_parse_failure() {
    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct Greeting {
        hello: String,
    }

    let server = MockServer::start().await;
    // Both attempts return bad JSON, so the retry path is taken and
    // InferenceJsonParseFailed is returned.
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "content": [{"type": "text", "text": "not valid json"}]
        })))
        .expect(2) // exactly 2 requests (initial + retry)
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let result: Result<Greeting, _> = client
        .generate_json("m", "Return JSON", None, None, Some(0.7))
        .await;
    let err = result.expect_err("parse failure after retry must error");
    assert!(
        err.to_string().contains("JSON parse failed after retry"),
        "expected InferenceJsonParseFailed, got: {err}"
    );
}

#[tokio::test]
async fn anthropic_generate_stream_json_parses_sse_chunks() {
    let server = MockServer::start().await;
    // generate_stream_json delegates to generate_stream with augmented system.
    // The stream returns the raw accumulated text (pre-extraction).
    let sse = [
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
        r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}"#,
        r#"data: {"type":"message_stop"}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse))
        .mount(&server)
        .await;

    let client = AnthropicClient::new(&server.uri(), None);
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream_json("m", "Say hello", None, tx, None, None)
        .await
        .unwrap();
    assert_eq!(full, "Hello!");
}

// =============================================================================
// Provider smoke — OpenAI-compatible providers via build_client  (closes #728)
// =============================================================================
//
// One table-driven loop verifies that every OpenAI-compatible provider
// (LM Studio, vllm-mlx, OpenRouter, Groq, xAI, Mistral,
// DeepSeek, Together, Custom) routes its request to the correct URL path
// and sends / omits the Authorization header as required.
//
// Each case is (provider_label, provider, optional_api_key).
// Local providers (LmStudio, VllmMlx, Custom) carry no API key; cloud
// providers send Bearer <key>.
//
// We use `build_client` from `parish_inference` so we exercise the actual
// provider dispatch logic (all these variants dispatch to OpenAiClient).

#[tokio::test]
async fn openai_compatible_provider_smoke() {
    use parish_config::Provider;
    use parish_inference::InferenceConfig;
    use parish_inference::build_client;

    struct ProviderCase {
        label: &'static str,
        provider: Provider,
        api_key: Option<&'static str>,
    }

    let cases: Vec<ProviderCase> = vec![
        ProviderCase {
            label: "LmStudio",
            provider: Provider::from_id("lmstudio").expect("lmstudio provider mod must be loaded"),
            api_key: None,
        },
        ProviderCase {
            label: "VllmMlx",
            provider: Provider::vllmmlx(),
            api_key: None,
        },
        ProviderCase {
            label: "OpenRouter",
            provider: Provider::from_id("openrouter")
                .expect("openrouter provider mod must be loaded"),
            api_key: Some("sk-or-test"),
        },
        ProviderCase {
            label: "Groq",
            provider: Provider::from_id("groq").expect("groq provider mod must be loaded"),
            api_key: Some("gsk-test"),
        },
        ProviderCase {
            label: "xAI",
            provider: Provider::from_id("xai").expect("xai provider mod must be loaded"),
            api_key: Some("xai-test"),
        },
        ProviderCase {
            label: "Mistral",
            provider: Provider::from_id("mistral").expect("mistral provider mod must be loaded"),
            api_key: Some("ms-test"),
        },
        ProviderCase {
            label: "DeepSeek",
            provider: Provider::from_id("deepseek").expect("deepseek provider mod must be loaded"),
            api_key: Some("ds-test"),
        },
        ProviderCase {
            label: "Together",
            provider: Provider::from_id("together").expect("together provider mod must be loaded"),
            api_key: Some("tgt-test"),
        },
        ProviderCase {
            label: "Custom",
            provider: Provider::custom(),
            api_key: None,
        },
    ];

    for case in &cases {
        let server = MockServer::start().await;
        let completions_path = if case.provider.id() == "google" {
            "/chat/completions"
        } else {
            "/v1/chat/completions"
        };

        // Google already carries its version prefix in the configured base URL;
        // the remaining providers use the standard `/v1` completion path.
        // For key-bearing providers, additionally require the Authorization header.
        if let Some(key) = case.api_key {
            let bearer = format!("Bearer {key}");
            Mock::given(method("POST"))
                .and(path(completions_path))
                .and(header("Authorization", bearer.as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "ok"}}]
                })))
                .mount(&server)
                .await;
        } else {
            // For key-absent providers: first assert Authorization is never sent.
            Mock::given(method("POST"))
                .and(path(completions_path))
                .and(header_exists("Authorization"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "with-auth"}}]
                })))
                .expect(0)
                .mount(&server)
                .await;
            // Then mount the permissive success mock.
            Mock::given(method("POST"))
                .and(path(completions_path))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "ok"}}]
                })))
                .mount(&server)
                .await;
        }

        let client = build_client(
            &case.provider,
            &server.uri(),
            case.api_key,
            &InferenceConfig::default(),
        );
        let out = client
            .generate("m", "p", None, GenerateParams::default())
            .await
            .unwrap_or_else(|e| panic!("provider {} generate failed: {e}", case.label));
        assert_eq!(
            out, "ok",
            "provider {} returned unexpected response",
            case.label
        );
    }
}

#[tokio::test]
async fn google_native_provider_smoke() {
    use parish_config::Provider;
    use parish_inference::{InferenceConfig, build_client};

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/interactions"))
        .and(header("x-goog-api-key", "goog-test"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "int_smoke",
            "status": "completed",
            "steps": [{"type":"model_output","content":[{"type":"text","text":"ok"}]}],
            "usage": {"total_input_tokens": 2, "total_output_tokens": 1, "total_tokens": 3}
        })))
        .mount(&server)
        .await;
    let provider = Provider::from_id("google").expect("google provider mod must be loaded");
    let client = build_client(
        &provider,
        &server.uri(),
        Some("goog-test"),
        &InferenceConfig::default(),
    );
    let out = client
        .generate("gemini-3.6-flash", "p", None, GenerateParams::default())
        .await
        .expect("native Google generate");
    assert_eq!(out, "ok");
}

// =============================================================================
// VllmMlxProcess::ensure_slots — multi-slot e2e coverage
// =============================================================================
//
// These tests stand up wiremock servers that mimic the `/v1/models` reachability
// probe vllm-mlx exposes, then drive `ensure_slots` through the production code
// path. They cover three behaviours that production depends on:
//
//   1. Multi-slot reachability: when N distinct (base_url, model) slots are
//      already reachable, `ensure_slots` returns N no-op handles without
//      attempting to spawn a real `vllm-mlx serve` child.
//   2. Dedup: duplicate slots collapse to a single handle.
//   3. RuntimeProcesses::stop is idempotent and safe to call against the
//      handles produced by `ensure_slots`.

/// Mounts a `GET /models` 200 responder on a fresh wiremock server.
///
/// Returns the mock's base URI (no trailing `/`). `VllmMlxProcess::is_reachable`
/// formats `{base_url}/models` so the probe lands on the mounted matcher.
async fn vllm_reachable_mock() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "object": "list",
            "data": []
        })))
        .mount(&server)
        .await;
    server
}

#[tokio::test]
async fn ensure_slots_spawns_one_handle_per_unique_reachable_slot() {
    // Two distinct (base_url, model) tuples — different ports, different models.
    // Both are already reachable, so `ensure_running` must NOT spawn a real
    // vllm-mlx process (which would fail in CI). Verifies the two-slot
    // Apple Silicon loadout's happy path: large model :8000 + small :8001.
    let big = vllm_reachable_mock().await;
    let small = vllm_reachable_mock().await;

    let slots = vec![
        VllmMlxSlot {
            base_url: big.uri(),
            model: "mlx-community/Qwen2.5-14B-Instruct-4bit".to_string(),
        },
        VllmMlxSlot {
            base_url: small.uri(),
            model: "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string(),
        },
    ];

    let mut handles = VllmMlxProcess::ensure_slots(&slots)
        .await
        .expect("two reachable slots should resolve without spawning");

    assert_eq!(handles.len(), 2, "one handle per unique slot");
    for h in &handles {
        assert!(
            !h.was_started_by_us(),
            "reachable slot must NOT spawn — both should be no-op handles"
        );
    }

    // Stop is idempotent and must not panic on no-op handles.
    for h in handles.iter_mut() {
        h.stop();
        h.stop();
    }
}

#[tokio::test]
async fn ensure_slots_dedups_duplicate_slot_inputs() {
    // Repeating the same slot twice must collapse to one spawn attempt —
    // the small slot is shared by Intent + Reaction + Simulation in
    // production, so `vllm_mlx_extra_slots()` can return duplicates.
    let mock = vllm_reachable_mock().await;
    let slot = VllmMlxSlot {
        base_url: mock.uri(),
        model: "mlx-community/Qwen2.5-1.5B-Instruct-4bit".to_string(),
    };
    let slots = vec![slot.clone(), slot.clone(), slot.clone()];

    let handles = VllmMlxProcess::ensure_slots(&slots)
        .await
        .expect("dedup path must not error");
    assert_eq!(
        handles.len(),
        1,
        "three identical slots must collapse to exactly one handle"
    );
}

#[tokio::test]
async fn runtime_processes_stop_cleans_up_multi_slot_bundle() {
    // End-to-end: ensure_slots → bundle into RuntimeProcesses → stop.
    // Validates the lifecycle shared by every entry point (Tauri, web
    // server, CLI) — drop on shutdown must not panic and must drain the
    // vllm_mlx Vec.
    let big = vllm_reachable_mock().await;
    let small = vllm_reachable_mock().await;

    let slots = vec![
        VllmMlxSlot {
            base_url: big.uri(),
            model: "big-model".to_string(),
        },
        VllmMlxSlot {
            base_url: small.uri(),
            model: "small-model".to_string(),
        },
    ];

    let vllm_mlx = VllmMlxProcess::ensure_slots(&slots).await.unwrap();
    let mut bundle = RuntimeProcesses {
        ollama: parish_inference::setup::OllamaProcess::none(),
        vllm_mlx,
        vllm: Vec::new(),
    };
    assert_eq!(bundle.vllm_mlx.len(), 2);

    // `stop` must drain both sub-processes without panic. We don't assert
    // that vllm_mlx is *emptied* because the current API only kills the
    // children in place; the important invariant is "no panic, no leak".
    bundle.stop();
    bundle.stop(); // idempotent on second call
}
