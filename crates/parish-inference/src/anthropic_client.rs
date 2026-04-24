//! Native Anthropic Messages API client.
//!
//! Unlike [`crate::openai_client::OpenAiClient`], this client talks to
//! Anthropic's native `/v1/messages` endpoint, which is **not** compatible
//! with the OpenAI chat completions schema:
//!
//! - Auth uses `x-api-key` (not `Authorization: Bearer`)
//! - A required `anthropic-version` header pins the API revision
//! - The system prompt is a top-level `system` string, not a message
//! - Responses are `content: [{type:"text", text:"..."}]` blocks
//! - `max_tokens` is required (not optional)
//! - Streaming uses named SSE events (`content_block_delta`, `message_stop`, …)
//!
//! The public method surface (`generate`, `generate_stream`, `generate_json`)
//! mirrors [`crate::openai_client::OpenAiClient`] so callers can dispatch
//! through [`crate::AnyClient`] without branching.

use crate::openai_client::build_client_or_fallback;
use crate::rate_limit::InferenceRateLimiter;
use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// Required Anthropic API version header value.
///
/// Anthropic pins request/response shape to this date. Bump only when the
/// request builder and response deserializer have been updated to match.
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default `max_tokens` when the caller passes `None`.
///
/// Anthropic requires `max_tokens` on every request. This default is large
/// enough for streamed dialogue and JSON metadata but well under model
/// context limits.
const DEFAULT_MAX_TOKENS: u32 = 4096;

/// HTTP client for Anthropic's native Messages API (`/v1/messages`).
///
/// Holds separate `reqwest::Client`s for streaming and non-streaming
/// requests so connection pooling and timeouts can be tuned
/// independently, matching [`crate::openai_client::OpenAiClient`].
///
/// Optionally carries an [`InferenceRateLimiter`] that throttles every
/// outbound request; when `None`, requests are unlimited.
#[derive(Clone)]
pub struct AnthropicClient {
    /// HTTP client with default timeout for non-streaming requests.
    client: reqwest::Client,
    /// HTTP client with longer timeout for streaming requests.
    streaming_client: reqwest::Client,
    /// Base URL (e.g. `https://api.anthropic.com`).
    base_url: String,
    /// API key — sent in the `x-api-key` header. Required in practice.
    api_key: Option<String>,
    /// Optional outbound request rate limiter. `None` means unlimited.
    rate_limiter: Option<InferenceRateLimiter>,
}

impl AnthropicClient {
    /// Creates a new client with default timeouts.
    pub fn new(base_url: &str, api_key: Option<&str>) -> Self {
        Self::new_with_config(base_url, api_key, &InferenceConfig::default())
    }

    /// Creates a new client with timeouts sourced from `InferenceConfig`.
    ///
    /// Matches [`crate::openai_client::OpenAiClient::new_with_config`] in
    /// behaviour: uses `config.timeout_secs` for non-streaming requests,
    /// `config.streaming_timeout_secs` for streaming, and falls back to a
    /// default `reqwest::Client` (no timeout) if the builder fails at a
    /// system boundary rather than panicking (issue #98).
    pub fn new_with_config(
        base_url: &str,
        api_key: Option<&str>,
        config: &InferenceConfig,
    ) -> Self {
        let client =
            build_client_or_fallback(Duration::from_secs(config.timeout_secs), "Anthropic");
        let streaming_client = build_client_or_fallback(
            Duration::from_secs(config.streaming_timeout_secs),
            "Anthropic streaming",
        );

        // Normalise: strip trailing `/` and an optional trailing `/v1` so
        // users can set either `https://api.anthropic.com` or
        // `https://api.anthropic.com/v1` without the endpoint being
        // doubled when we append `/v1/messages`.
        let normalized = {
            let trimmed = base_url.trim_end_matches('/');
            trimmed.strip_suffix("/v1").unwrap_or(trimmed).to_string()
        };

        Self {
            client,
            streaming_client,
            base_url: normalized,
            api_key: api_key.map(|s| s.to_string()),
            rate_limiter: None,
        }
    }

    /// Attaches an outbound rate limiter, returning the modified client.
    pub fn with_rate_limit(mut self, limiter: InferenceRateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Convenience: attach a rate limiter only if `limiter` is `Some`.
    pub fn maybe_with_rate_limit(self, limiter: Option<InferenceRateLimiter>) -> Self {
        match limiter {
            Some(l) => self.with_rate_limit(l),
            None => self,
        }
    }

    /// Returns whether this client has a rate limiter attached.
    pub fn has_rate_limiter(&self) -> bool {
        self.rate_limiter.is_some()
    }

    /// Returns the base URL of this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Awaits a free slot in the limiter (no-op if unlimited).
    async fn acquire_slot(&self) {
        if let Some(rl) = &self.rate_limiter {
            rl.acquire().await;
        }
    }

    /// Builds a `MessagesRequest` from the generic `generate*` args.
    ///
    /// `max_tokens` falls back to [`DEFAULT_MAX_TOKENS`] because Anthropic
    /// rejects requests that omit it. System prompt becomes the top-level
    /// `system` field (not a message).
    fn build_request<'a>(
        &self,
        model: &'a str,
        prompt: &'a str,
        system: Option<&'a str>,
        stream: bool,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> MessagesRequest<'a> {
        MessagesRequest {
            model,
            messages: vec![Message {
                role: "user",
                content: prompt,
            }],
            system,
            max_tokens: max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
            temperature,
            stream,
        }
    }

    /// Applies Anthropic's required headers to a request.
    fn apply_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req.header("anthropic-version", ANTHROPIC_VERSION);
        match &self.api_key {
            Some(key) => req.header("x-api-key", key),
            None => req,
        }
    }

    /// Sends a non-streaming request and returns the raw response.
    ///
    /// On non-2xx status, reads the response body and attempts to extract
    /// Anthropic's error message so callers see actionable diagnostics
    /// instead of a bare HTTP status code.
    async fn send_request(
        &self,
        body: &MessagesRequest<'_>,
    ) -> Result<reqwest::Response, ParishError> {
        let url = format!("{}/v1/messages", self.base_url);
        let req = self.apply_headers(self.client.post(&url).json(body));
        let response = req
            .send()
            .await
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let detail = extract_api_error_message(&body_text).unwrap_or_else(|| body_text.clone());
            return Err(ParishError::Inference(format!(
                "Anthropic API error (HTTP {status}): {detail}"
            )));
        }

        Ok(response)
    }

    /// Sends a non-streaming messages request and returns the response text.
    ///
    /// An omitted `max_tokens` is replaced with [`DEFAULT_MAX_TOKENS`] — a
    /// quirk of the native API, which rejects the field's absence.
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, false, max_tokens, temperature);
        let resp = self.send_request(&body).await?;
        let parsed: MessagesResponse = resp.json().await?;
        Ok(extract_text(&parsed))
    }

    /// Sends a non-streaming request and deserializes the response as JSON.
    ///
    /// Anthropic has no `response_format` equivalent, so the caller's
    /// system prompt is augmented with an instruction to emit JSON only.
    /// The raw text is then parsed via `serde_json`.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        // Append a JSON-only instruction to the system prompt. If the
        // caller didn't supply one, the instruction stands on its own.
        const JSON_SUFFIX: &str =
            "\n\nRespond ONLY with a single JSON object. No prose, no code fences, no commentary.";
        let augmented_system = match system {
            Some(s) => format!("{s}{JSON_SUFFIX}"),
            None => JSON_SUFFIX.trim_start().to_string(),
        };

        let raw = self
            .generate(
                model,
                prompt,
                Some(augmented_system.as_str()),
                max_tokens,
                temperature,
            )
            .await?;
        let trimmed = strip_json_fence(&raw);
        let parsed: T = serde_json::from_str(trimmed)?;
        Ok(parsed)
    }
}

/// Strips Markdown code-fence wrappers that some models emit around JSON.
///
/// Anthropic's JSON-only instruction is usually respected, but handling
/// the common ` ```json\n…\n``` ` escape hatch keeps the parse robust.
fn strip_json_fence(raw: &str) -> &str {
    let t = raw.trim();
    if let Some(inner) = t.strip_prefix("```json") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    if let Some(inner) = t.strip_prefix("```") {
        return inner
            .trim_start_matches('\n')
            .trim_end_matches("```")
            .trim();
    }
    t
}

// --- Streaming ----------------------------------------------------------

impl AnthropicClient {
    /// Streams a messages request, forwarding text deltas as they arrive.
    ///
    /// Posts to `/v1/messages` with `stream: true` and parses the native
    /// Anthropic SSE event stream (see [`process_sse_line`]). Each text
    /// delta is sent through `token_tx` as it arrives, and the full
    /// accumulated response is returned when the stream terminates with
    /// a `message_stop` event (or when the HTTP body ends).
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, true, max_tokens, temperature);

        let url = format!("{}/v1/messages", self.base_url);
        let req = self.apply_headers(self.streaming_client.post(&url).json(&body));
        let response = req
            .send()
            .await
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            let detail = extract_api_error_message(&body_text).unwrap_or_else(|| body_text.clone());
            return Err(ParishError::Inference(format!(
                "Anthropic API error (HTTP {status}): {detail}"
            )));
        }

        let mut accumulated = String::new();
        let mut line_buf = String::new();
        let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();

        let mut response = response;
        while let Some(chunk) = response.chunk().await? {
            line_buf.push_str(&decoder.push(&chunk));

            while let Some(newline_pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=newline_pos).collect();
                match process_sse_line(&line, &token_tx, &mut accumulated) {
                    SseResult::Continue => {}
                    SseResult::Done => return Ok(accumulated),
                    SseResult::Error(msg) => return Err(ParishError::Inference(msg)),
                }
            }
        }

        line_buf.push_str(&decoder.flush());
        let remaining = line_buf.trim();
        if !remaining.is_empty()
            && let SseResult::Error(msg) = process_sse_line(remaining, &token_tx, &mut accumulated)
        {
            return Err(ParishError::Inference(msg));
        }

        Ok(accumulated)
    }
}

/// Result of processing a single SSE line.
enum SseResult {
    /// Continue reading more lines.
    Continue,
    /// Stream is complete (saw `message_stop`).
    Done,
    /// An error event was received mid-stream.
    Error(String),
}

/// Processes a single SSE line: dispatches by event `type` field.
///
/// Anthropic SSE streams interleave `event: <name>` lines with
/// `data: <json>` lines. The JSON payloads always carry a `type` field
/// that matches the preceding event name, so we dispatch on `type`
/// directly and ignore the `event:` lines — simpler and tolerant of
/// keepalive or reordering.
fn process_sse_line(
    line: &str,
    token_tx: &mpsc::UnboundedSender<String>,
    accumulated: &mut String,
) -> SseResult {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') || trimmed.starts_with("event:") {
        return SseResult::Continue;
    }
    let Some(data) = trimmed.strip_prefix("data:").map(str::trim) else {
        return SseResult::Continue;
    };

    let Ok(event) = serde_json::from_str::<StreamEvent>(data) else {
        return SseResult::Continue;
    };

    match event {
        StreamEvent::ContentBlockDelta { delta } => {
            if let StreamDelta::TextDelta { text } = delta
                && !text.is_empty()
            {
                let _ = token_tx.send(text.clone());
                accumulated.push_str(&text);
            }
            SseResult::Continue
        }
        StreamEvent::MessageStop => SseResult::Done,
        StreamEvent::Error { error } => {
            let msg = format!(
                "Anthropic stream error ({}): {}",
                error.error_type, error.message
            );
            SseResult::Error(msg)
        }
        StreamEvent::Other => SseResult::Continue,
    }
}

/// The subset of SSE event payloads we care about.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamEvent {
    /// Incremental update to the current content block.
    ContentBlockDelta {
        #[serde(default)]
        delta: StreamDelta,
    },
    /// Terminal event; stream is complete.
    MessageStop,
    /// Error event sent mid-stream (e.g. output token limit, internal error).
    Error { error: StreamError },
    /// Any other event we don't act on (kept so deserialisation never fails).
    #[serde(other)]
    Other,
}

/// Error payload inside an `error` SSE event.
#[derive(Deserialize, Debug)]
struct StreamError {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

/// Delta payload inside a `content_block_delta` event.
#[derive(Deserialize, Debug, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
enum StreamDelta {
    /// Streamed text fragment from a text content block.
    TextDelta {
        #[serde(default)]
        text: String,
    },
    /// Unknown delta type (e.g. `input_json_delta` for tool use). Ignored.
    #[default]
    #[serde(other)]
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_construction_does_not_panic() {
        // Regression for #98 parity — constructors should never abort.
        let _ = AnthropicClient::new("https://api.anthropic.com", None);
    }

    #[test]
    fn test_base_url_normalisation_trailing_slash() {
        let c = AnthropicClient::new("https://api.anthropic.com/", None);
        assert_eq!(c.base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn test_base_url_normalisation_strips_v1() {
        let c = AnthropicClient::new("https://api.anthropic.com/v1", None);
        assert_eq!(c.base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn test_base_url_normalisation_strips_v1_with_slash() {
        let c = AnthropicClient::new("https://api.anthropic.com/v1/", None);
        assert_eq!(c.base_url(), "https://api.anthropic.com");
    }

    #[test]
    fn test_client_starts_without_rate_limiter() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        assert!(!c.has_rate_limiter());
    }

    #[test]
    fn test_with_rate_limit_attaches_limiter() {
        let limiter = InferenceRateLimiter::new(60, 5).expect("limiter");
        let c = AnthropicClient::new("https://api.anthropic.com", None).with_rate_limit(limiter);
        assert!(c.has_rate_limiter());
    }

    #[test]
    fn test_maybe_with_rate_limit_some() {
        let limiter = InferenceRateLimiter::new(60, 5);
        let c =
            AnthropicClient::new("https://api.anthropic.com", None).maybe_with_rate_limit(limiter);
        assert!(c.has_rate_limiter());
    }

    #[test]
    fn test_maybe_with_rate_limit_none_is_noop() {
        let c = AnthropicClient::new("https://api.anthropic.com", None).maybe_with_rate_limit(None);
        assert!(!c.has_rate_limiter());
    }

    #[test]
    fn test_build_request_with_system() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request(
            "claude-sonnet-4-5",
            "hi",
            Some("be brief"),
            false,
            None,
            None,
        );
        assert_eq!(req.model, "claude-sonnet-4-5");
        assert_eq!(req.system, Some("be brief"));
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert_eq!(req.messages[0].content, "hi");
        assert!(!req.stream);
        assert_eq!(req.max_tokens, DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn test_build_request_without_system() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, false, None, None);
        assert!(req.system.is_none());
    }

    #[test]
    fn test_build_request_respects_explicit_max_tokens() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, false, Some(128), None);
        assert_eq!(req.max_tokens, 128);
    }

    #[test]
    fn test_request_serialisation_stream_omitted_when_false() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, false, None, None);
        let json = serde_json::to_value(&req).unwrap();
        // `stream: false` is omitted to keep requests minimal.
        assert!(json.get("stream").is_none());
    }

    #[test]
    fn test_request_serialisation_stream_true() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, true, None, None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["stream"], true);
    }

    #[test]
    fn test_request_serialisation_system_top_level_not_role() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", Some("sys"), false, None, None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["system"], "sys");
        assert_eq!(json["messages"][0]["role"], "user");
        // There must NOT be a "system"-role message — that's the key
        // schema difference from OpenAI's chat completions API.
        assert_eq!(json["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_request_serialisation_temperature_omitted_when_none() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, false, None, None);
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("temperature").is_none());
    }

    #[test]
    fn test_request_serialisation_temperature_included_when_set() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        let req = c.build_request("claude-sonnet-4-5", "hi", None, false, None, Some(0.7));
        let json = serde_json::to_value(&req).unwrap();
        assert!((json["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_response_single_text_block() {
        let json = r#"{"content":[{"type":"text","text":"Hello!"}]}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&resp), "Hello!");
    }

    #[test]
    fn test_response_multiple_text_blocks_are_concatenated() {
        let json = r#"{"content":[
            {"type":"text","text":"Hello"},
            {"type":"text","text":", world"}
        ]}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&resp), "Hello, world");
    }

    #[test]
    fn test_response_ignores_non_text_blocks() {
        let json = r#"{"content":[
            {"type":"text","text":"say hi"},
            {"type":"tool_use","id":"x","name":"y","input":{}}
        ]}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&resp), "say hi");
    }

    #[test]
    fn test_response_empty_content() {
        let json = r#"{"content":[]}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&resp), "");
    }

    #[test]
    fn test_response_missing_content_field() {
        let json = r#"{}"#;
        let resp: MessagesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_text(&resp), "");
    }

    #[test]
    fn test_strip_json_fence_plain() {
        assert_eq!(strip_json_fence(r#"{"a":1}"#), r#"{"a":1}"#);
    }

    #[test]
    fn test_strip_json_fence_markdown() {
        assert_eq!(strip_json_fence("```json\n{\"a\":1}\n```"), r#"{"a":1}"#);
    }

    #[test]
    fn test_strip_json_fence_untagged() {
        assert_eq!(strip_json_fence("```\n{\"a\":1}\n```"), r#"{"a":1}"#);
    }

    // --- SSE parser tests ----------------------------------------------

    struct SseOutput {
        acc: String,
        tokens: Vec<String>,
        done: bool,
        error: Option<String>,
    }

    fn run_sse(lines: &[&str]) -> SseOutput {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut acc = String::new();
        let mut done = false;
        let mut error = None;
        for line in lines {
            match process_sse_line(line, &tx, &mut acc) {
                SseResult::Continue => {}
                SseResult::Done => {
                    done = true;
                    break;
                }
                SseResult::Error(msg) => {
                    error = Some(msg);
                    break;
                }
            }
        }
        drop(tx);
        let mut tokens = Vec::new();
        while let Ok(t) = rx.try_recv() {
            tokens.push(t);
        }
        SseOutput {
            acc,
            tokens,
            done,
            error,
        }
    }

    #[test]
    fn test_sse_content_block_delta_emits_text() {
        let SseOutput {
            acc, tokens, done, ..
        } = run_sse(&[
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hel"}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"lo"}}"#,
        ]);
        assert_eq!(acc, "Hello");
        assert_eq!(tokens, vec!["Hel".to_string(), "lo".to_string()]);
        assert!(!done);
    }

    #[test]
    fn test_sse_message_stop_terminates() {
        let SseOutput {
            acc, tokens, done, ..
        } = run_sse(&[
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
            r#"data: {"type":"message_stop"}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ignored"}}"#,
        ]);
        assert_eq!(acc, "hi");
        assert_eq!(tokens, vec!["hi".to_string()]);
        assert!(done);
    }

    #[test]
    fn test_sse_ignores_noise_events() {
        let SseOutput { acc, tokens, .. } = run_sse(&[
            "event: ping",
            r#"data: {"type":"ping"}"#,
            r#"data: {"type":"message_start","message":{}}"#,
            r#"data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}"#,
            r#"data: {"type":"content_block_stop","index":0}"#,
            r#"data: {"type":"message_delta","delta":{}}"#,
        ]);
        assert_eq!(acc, "x");
        assert_eq!(tokens, vec!["x".to_string()]);
    }

    #[test]
    fn test_sse_ignores_non_text_deltas() {
        let SseOutput { acc, tokens, .. } = run_sse(&[
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{"}}"#,
        ]);
        assert_eq!(acc, "");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_sse_tolerates_blank_and_comment_lines() {
        let SseOutput { acc, tokens, .. } = run_sse(&[
            "",
            "   ",
            ": keepalive",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ok"}}"#,
        ]);
        assert_eq!(acc, "ok");
        assert_eq!(tokens, vec!["ok".to_string()]);
    }

    #[test]
    fn test_sse_tolerates_invalid_json() {
        let SseOutput { acc, tokens, .. } = run_sse(&[
            "data: {not json",
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"recovered"}}"#,
        ]);
        assert_eq!(acc, "recovered");
        assert_eq!(tokens, vec!["recovered".to_string()]);
    }

    #[test]
    fn test_sse_error_event_returns_error() {
        let SseOutput {
            acc, error, done, ..
        } = run_sse(&[
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}"#,
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
            r#"data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"ignored"}}"#,
        ]);
        assert_eq!(acc, "partial");
        assert!(!done);
        let err = error.expect("should have received an error");
        assert!(err.contains("overloaded_error"), "got: {err}");
        assert!(err.contains("Overloaded"), "got: {err}");
    }

    #[test]
    fn test_sse_error_event_without_prior_content() {
        let SseOutput { acc, error, .. } = run_sse(&[
            r#"data: {"type":"error","error":{"type":"invalid_request_error","message":"max_tokens exceeded"}}"#,
        ]);
        assert_eq!(acc, "");
        let err = error.expect("should have received an error");
        assert!(err.contains("max_tokens exceeded"), "got: {err}");
    }

    #[test]
    fn test_extract_api_error_message_valid() {
        let body = r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: 1000000 > 8192"}}"#;
        let msg = extract_api_error_message(body);
        assert_eq!(msg.as_deref(), Some("max_tokens: 1000000 > 8192"));
    }

    #[test]
    fn test_extract_api_error_message_missing_fields() {
        assert!(extract_api_error_message("{}").is_none());
        assert!(extract_api_error_message(r#"{"error":{}}"#).is_none());
        assert!(extract_api_error_message("not json").is_none());
    }

    #[test]
    fn test_extract_api_error_message_non_string_message() {
        let body = r#"{"error":{"message":42}}"#;
        assert!(extract_api_error_message(body).is_none());
    }

    #[tokio::test]
    async fn test_acquire_slot_noop_without_limiter() {
        let c = AnthropicClient::new("https://api.anthropic.com", None);
        c.acquire_slot().await;
    }

    // --- Live smoke tests (opt-in) -------------------------------------

    #[tokio::test]
    #[ignore] // requires ANTHROPIC_API_KEY
    async fn test_generate_live() {
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            return;
        };
        let c = AnthropicClient::new("https://api.anthropic.com", Some(&key));
        let result = c
            .generate(
                "claude-sonnet-4-5",
                "Say hello in one word.",
                None,
                Some(32),
                None,
            )
            .await;
        assert!(result.is_ok(), "got err: {:?}", result.err());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore] // requires ANTHROPIC_API_KEY
    async fn test_generate_stream_live() {
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            return;
        };
        let c = AnthropicClient::new("https://api.anthropic.com", Some(&key));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let result = c
            .generate_stream(
                "claude-sonnet-4-5",
                "Say hello in one word.",
                None,
                tx,
                Some(32),
                None,
            )
            .await;
        assert!(result.is_ok(), "got err: {:?}", result.err());
        let mut tokens = Vec::new();
        while let Ok(t) = rx.try_recv() {
            tokens.push(t);
        }
        assert!(!tokens.is_empty(), "expected at least one streamed token");
    }

    #[tokio::test]
    #[ignore] // requires ANTHROPIC_API_KEY
    async fn test_generate_json_live() {
        #[derive(Deserialize, Debug)]
        #[allow(dead_code)]
        struct TestResp {
            #[serde(default)]
            greeting: String,
        }
        let Ok(key) = std::env::var("ANTHROPIC_API_KEY") else {
            return;
        };
        let c = AnthropicClient::new("https://api.anthropic.com", Some(&key));
        let result: Result<TestResp, _> = c
            .generate_json(
                "claude-sonnet-4-5",
                "Return {\"greeting\":\"hello\"}.",
                None,
                Some(64),
                None,
            )
            .await;
        assert!(result.is_ok(), "got err: {:?}", result.err());
    }
}

// --- Request types ------------------------------------------------------

/// A single turn in the conversation. Only `user`/`assistant` roles —
/// the system prompt is a top-level field, not a message.
#[derive(Serialize, Debug)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

/// Request body for `POST /v1/messages`.
#[derive(Serialize, Debug)]
struct MessagesRequest<'a> {
    model: &'a str,
    messages: Vec<Message<'a>>,
    /// Top-level system prompt. Anthropic does not accept a `system`-role
    /// message; passing one would be treated as an unknown role.
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<&'a str>,
    /// Required by Anthropic — see [`DEFAULT_MAX_TOKENS`].
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    /// Only serialise when `true`; omitted flag defaults to non-streaming.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    stream: bool,
}

// --- Response types -----------------------------------------------------

/// Non-streaming response from `POST /v1/messages`.
#[derive(Deserialize, Debug, Default)]
struct MessagesResponse {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

/// One block in the response `content` array. Anthropic returns multiple
/// block types; we only emit text from `text` blocks and ignore others
/// (e.g. `tool_use`) for now.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentBlock {
    /// A span of plain text — the only kind we extract today.
    Text { text: String },
    /// Any other block type (tool_use, tool_result, …). Kept so
    /// deserialization doesn't fail when models return them.
    #[serde(other)]
    Other,
}

/// Concatenates every `Text` block into one string, in order.
///
/// Models occasionally split a response across multiple text blocks
/// (especially after tool use). Joining them preserves the full reply.
fn extract_text(resp: &MessagesResponse) -> String {
    let mut out = String::new();
    for block in &resp.content {
        if let ContentBlock::Text { text } = block {
            out.push_str(text);
        }
    }
    out
}

/// Attempts to extract the human-readable error message from an Anthropic
/// API error response body (`{"type":"error","error":{"type":"…","message":"…"}}`).
fn extract_api_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let msg = v.get("error")?.get("message")?.as_str()?;
    Some(msg.to_string())
}
