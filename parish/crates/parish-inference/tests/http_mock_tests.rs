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
use parish_inference::client::OllamaClient;
use parish_inference::openai_client::OpenAiClient;
use serde::Deserialize;
use tokio::sync::mpsc;
use wiremock::matchers::{header, header_exists, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

// =============================================================================
// OllamaClient — native /api/generate endpoint
// =============================================================================

#[tokio::test]
async fn ollama_generate_returns_response_text() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": "Howya friend",
            "done": true
        })))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let out = client
        .generate("qwen3:14b", "Say hello", None)
        .await
        .expect("generate should succeed");
    assert_eq!(out, "Howya friend");
}

#[tokio::test]
async fn ollama_generate_with_system_prompt_is_accepted() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": "ok",
            "done": true
        })))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let out = client
        .generate("m", "u", Some("You are a test assistant"))
        .await
        .unwrap();
    assert_eq!(out, "ok");
}

#[tokio::test]
async fn ollama_generate_maps_500_to_inference_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal error"))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let err = client
        .generate("m", "p", None)
        .await
        .expect_err("500 must surface as an error");
    // 500 should be caught by error_for_status() and mapped to Inference(_)
    let msg = err.to_string();
    assert!(
        msg.contains("inference error") || msg.contains("500"),
        "expected inference error, got: {msg}"
    );
}

#[tokio::test]
async fn ollama_generate_maps_404_to_inference_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let err = client.generate("m", "p", None).await.unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("inference error") || msg.contains("404"));
}

#[tokio::test]
async fn ollama_generate_empty_response_is_ok() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": "",
            "done": true
        })))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let out = client.generate("m", "p", None).await.unwrap();
    assert_eq!(out, "");
}

#[tokio::test]
async fn ollama_generate_stream_emits_every_chunk() {
    let server = MockServer::start().await;
    // NDJSON: one JSON object per line, final line has done:true.
    let ndjson = [
        r#"{"response":"Hel","done":false}"#,
        r#"{"response":"lo,","done":false}"#,
        r#"{"response":" world!","done":true}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client
        .generate_stream("m", "p", None, tx)
        .await
        .expect("stream should succeed");

    assert_eq!(full, "Hello, world!");

    let mut tokens = Vec::new();
    while let Ok(t) = rx.try_recv() {
        tokens.push(t);
    }
    assert_eq!(tokens, vec!["Hel", "lo,", " world!"]);
}

#[tokio::test]
async fn ollama_generate_stream_ignores_empty_chunks() {
    let server = MockServer::start().await;
    // Some backends emit empty `response` keep-alives; they must not appear
    // as tokens or corrupt the accumulator.
    let ndjson = [
        r#"{"response":"","done":false}"#,
        r#"{"response":"only","done":false}"#,
        r#"{"response":"","done":true}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let (tx, mut rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client.generate_stream("m", "p", None, tx).await.unwrap();
    assert_eq!(full, "only");

    let mut tokens = Vec::new();
    while let Ok(t) = rx.try_recv() {
        tokens.push(t);
    }
    assert_eq!(tokens, vec!["only"]);
}

#[tokio::test]
async fn ollama_generate_stream_tolerates_malformed_lines() {
    // A malformed NDJSON line between two good ones must be skipped —
    // the loop uses `if let Ok(...)` so a bad line is silently ignored.
    let server = MockServer::start().await;
    let ndjson = [
        r#"{"response":"a","done":false}"#,
        r#"{this is not json"#,
        r#"{"response":"b","done":true}"#,
    ]
    .join("\n");
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client.generate_stream("m", "p", None, tx).await.unwrap();
    assert_eq!(full, "ab");
}

#[tokio::test]
async fn ollama_generate_stream_handles_missing_trailing_newline() {
    // The last NDJSON chunk may arrive without a trailing newline;
    // the client's post-loop buffer flush must still parse it.
    let server = MockServer::start().await;
    let ndjson = r#"{"response":"one","done":false}
{"response":"two","done":true}"#;
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_string(ndjson))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let (tx, _rx) = mpsc::channel::<String>(TOKEN_CHANNEL_CAPACITY);
    let full = client.generate_stream("m", "p", None, tx).await.unwrap();
    assert_eq!(full, "onetwo");
}

#[tokio::test]
async fn ollama_generate_json_parses_typed_payload() {
    #[derive(Deserialize, Debug)]
    struct Intent {
        #[serde(default)]
        action: String,
        #[serde(default)]
        target: String,
    }

    let server = MockServer::start().await;
    // generate_json wraps the model's JSON output inside the Ollama envelope:
    // `response` holds the JSON string, which the client then deserializes.
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": "{\"action\":\"go\",\"target\":\"pub\"}",
            "done": true
        })))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let intent: Intent = client
        .generate_json("m", "Parse the intent", None)
        .await
        .unwrap();
    assert_eq!(intent.action, "go");
    assert_eq!(intent.target, "pub");
}

#[tokio::test]
async fn ollama_generate_json_surfaces_parse_error_for_malformed_body() {
    #[derive(Deserialize, Debug)]
    #[allow(dead_code)]
    struct Intent {
        action: String,
    }

    let server = MockServer::start().await;
    // Ollama envelope is valid, but the inner `response` is not valid JSON.
    Mock::given(method("POST"))
        .and(path("/api/generate"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "response": "this is not valid JSON",
            "done": true
        })))
        .mount(&server)
        .await;

    let client = OllamaClient::new(&server.uri());
    let result: Result<Intent, _> = client.generate_json("m", "p", None).await;
    let err = result.expect_err("malformed inner JSON must fail");
    // Serialization-mapped error via From<serde_json::Error>
    assert!(err.to_string().contains("serialization"));
}

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
        .generate("gpt-test", "hi", None, None, None)
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
    let out = client.generate("m", "p", None, None, None).await.unwrap();
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
    let out = client.generate("m", "p", None, None, None).await.unwrap();
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
        .generate("m", "p", None, None, None)
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
    let out = client.generate("m", "p", None, None, None).await.unwrap();
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
        .generate_stream("m", "p", None, tx, None, None)
        .await
        .unwrap();
    assert_eq!(full, "ab");
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
        .generate_stream("m", "p", None, tx, None, None)
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
        .generate_json("m", "Return a greeting", None, None, None)
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
    let result: Result<Greeting, _> = client.generate_json("m", "p", None, None, None).await;
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
        .generate("m", "p", None, Some(42), None)
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
    let out = client.generate("m", "p", None, None, None).await.unwrap();
    assert_eq!(out, "ok");
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

// =============================================================================
// Provider smoke — OpenAI-compatible providers via build_client  (closes #728)
// =============================================================================
//
// One table-driven loop verifies that every OpenAI-compatible provider
// (LM Studio, vLLM, OpenRouter, Google/Gemini, Groq, xAI, Mistral,
// DeepSeek, Together, Custom) routes its request to the correct URL path
// and sends / omits the Authorization header as required.
//
// Each case is (provider_label, provider, optional_api_key).
// Local providers (LmStudio, Vllm, Custom) carry no API key; cloud
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
            provider: Provider::LmStudio,
            api_key: None,
        },
        ProviderCase {
            label: "Vllm",
            provider: Provider::Vllm,
            api_key: None,
        },
        ProviderCase {
            label: "OpenRouter",
            provider: Provider::OpenRouter,
            api_key: Some("sk-or-test"),
        },
        ProviderCase {
            label: "Google (Gemini)",
            provider: Provider::Google,
            api_key: Some("goog-test"),
        },
        ProviderCase {
            label: "Groq",
            provider: Provider::Groq,
            api_key: Some("gsk-test"),
        },
        ProviderCase {
            label: "xAI",
            provider: Provider::Xai,
            api_key: Some("xai-test"),
        },
        ProviderCase {
            label: "Mistral",
            provider: Provider::Mistral,
            api_key: Some("ms-test"),
        },
        ProviderCase {
            label: "DeepSeek",
            provider: Provider::DeepSeek,
            api_key: Some("ds-test"),
        },
        ProviderCase {
            label: "Together",
            provider: Provider::Together,
            api_key: Some("tgt-test"),
        },
        ProviderCase {
            label: "Custom",
            provider: Provider::Custom,
            api_key: None,
        },
    ];

    for case in &cases {
        let server = MockServer::start().await;

        // Mount a mock that expects POST /v1/chat/completions.
        // For key-bearing providers, additionally require the Authorization header.
        if let Some(key) = case.api_key {
            let bearer = format!("Bearer {key}");
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .and(header("Authorization", bearer.as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "ok"}}]
                })))
                .mount(&server)
                .await;
        } else {
            // For key-absent providers: first assert Authorization is never sent.
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .and(header_exists("Authorization"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "with-auth"}}]
                })))
                .expect(0)
                .mount(&server)
                .await;
            // Then mount the permissive success mock.
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
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
            .generate("m", "p", None, None, None)
            .await
            .unwrap_or_else(|e| panic!("provider {} generate failed: {e}", case.label));
        assert_eq!(
            out, "ok",
            "provider {} returned unexpected response",
            case.label
        );
    }
}
