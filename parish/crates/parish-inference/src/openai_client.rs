//! OpenAI-compatible HTTP client for LLM inference.
//!
//! Talks to any provider that implements the OpenAI chat completions API:
//! Ollama (`/v1/chat/completions`), LM Studio, OpenRouter, or any custom
//! OpenAI-compatible endpoint. Uses SSE (Server-Sent Events) for streaming.

use crate::TOKEN_CHANNEL_CAPACITY;
use crate::rate_limit::InferenceRateLimiter;
use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// Builds a `reqwest::Client` with the given timeout, falling back to a default
/// client (no timeout) if the builder fails.
///
/// Historically this call used `.expect()` which would panic if the TLS
/// backend failed to initialize (#98). We now log a warning and return a
/// default client so the application can degrade gracefully rather than
/// crashing at startup.
pub(crate) fn build_client_or_fallback(timeout: Duration, label: &'static str) -> reqwest::Client {
    match reqwest::Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!(
                "failed to build {label} reqwest client ({err}); falling back to default client with no timeout",
            );
            reqwest::Client::new()
        }
    }
}

/// HTTP client for OpenAI-compatible chat completions endpoints.
///
/// Works with Ollama, LM Studio, OpenRouter, and any provider that
/// implements the `/v1/chat/completions` API. Provides the same
/// logical interface as the legacy Ollama-native client: plain text
/// generation, streaming generation, and structured JSON output.
///
/// Optionally holds an [`InferenceRateLimiter`] applied to every
/// outbound request — when set, calls block until the limiter has
/// a free slot, transparently throttling per-provider request rates
/// without any caller awareness.
#[derive(Clone)]
pub struct OpenAiClient {
    /// HTTP client with default timeout for non-streaming requests.
    client: reqwest::Client,
    /// HTTP client with longer timeout for streaming requests.
    /// Reused across calls to preserve connection pooling.
    streaming_client: reqwest::Client,
    /// Base URL (e.g. "http://localhost:11434" or "https://openrouter.ai/api").
    base_url: String,
    /// Optional API key for authenticated providers.
    api_key: Option<String>,
    /// Optional outbound request rate limiter. `None` means unlimited.
    rate_limiter: Option<InferenceRateLimiter>,
}

/// A single message in the chat completions request.
#[derive(Serialize, Debug)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

/// Request body for the `/v1/chat/completions` endpoint.
#[derive(Serialize, Debug)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

/// Controls structured output format.
#[derive(Serialize, Debug)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

/// Non-streaming response from chat completions.
#[derive(Deserialize, Debug)]
struct ChatCompletionResponse {
    #[serde(default)]
    choices: Vec<Choice>,
}

/// A single completion choice.
#[derive(Deserialize, Debug)]
struct Choice {
    #[serde(default)]
    message: MessageContent,
}

/// Message content in a non-streaming response.
#[derive(Deserialize, Debug, Default)]
struct MessageContent {
    #[serde(default)]
    content: Option<String>,
}

/// A single SSE chunk from a streaming response.
#[derive(Deserialize, Debug)]
pub(crate) struct ChatCompletionChunk {
    #[serde(default)]
    pub(crate) choices: Vec<StreamChoice>,
}

/// A single choice in a streaming chunk.
#[derive(Deserialize, Debug)]
pub(crate) struct StreamChoice {
    #[serde(default)]
    pub(crate) delta: Delta,
    #[serde(default)]
    pub(crate) finish_reason: Option<String>,
}

/// Delta content in a streaming chunk.
#[derive(Deserialize, Debug, Default)]
pub(crate) struct Delta {
    #[serde(default)]
    pub(crate) content: Option<String>,
}

impl OpenAiClient {
    /// Creates a new client for an OpenAI-compatible endpoint using default timeouts.
    ///
    /// The `base_url` should be the root URL without `/v1/chat/completions`
    /// (e.g. "http://localhost:11434" for Ollama, "https://openrouter.ai/api"
    /// for OpenRouter). The `/v1/chat/completions` path is appended
    /// automatically.
    pub fn new(base_url: &str, api_key: Option<&str>) -> Self {
        Self::new_with_config(base_url, api_key, &InferenceConfig::default())
    }

    /// Creates a new client with timeouts sourced from `InferenceConfig`.
    ///
    /// Uses `config.timeout_secs` for the default HTTP client and stores
    /// `config.streaming_timeout_secs` for streaming request clients.
    ///
    /// If the underlying `reqwest` builder fails (e.g. a TLS backend is
    /// unavailable), this falls back to a default `reqwest::Client` with
    /// no configured timeout rather than panicking, and emits a warning
    /// via `tracing`. See issue #98.
    pub fn new_with_config(
        base_url: &str,
        api_key: Option<&str>,
        config: &InferenceConfig,
    ) -> Self {
        let client = build_client_or_fallback(
            Duration::from_secs(config.timeout_secs),
            "OpenAI-compatible",
        );

        // Pre-build the streaming client once so connection pooling is
        // preserved across streaming calls instead of creating a fresh
        // client (and fresh TCP connections) on every request.
        let streaming_client = build_client_or_fallback(
            Duration::from_secs(config.streaming_timeout_secs),
            "OpenAI-compatible streaming",
        );

        // Normalize the base URL: strip a trailing slash, and also strip a
        // trailing `/v1` (with or without slash) because the endpoint paths
        // below unconditionally append `/v1/chat/completions`. Users who set
        // `PARISH_BASE_URL=https://api.groq.com/openai/v1` would otherwise get
        // `https://api.groq.com/openai/v1/v1/chat/completions` (404).
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
    ///
    /// All subsequent `generate*` calls will block on the limiter
    /// before issuing the HTTP request. Use [`InferenceRateLimiter::from_config`]
    /// to build a limiter from a `parish.toml` `[rate_limits]` entry.
    pub fn with_rate_limit(mut self, limiter: InferenceRateLimiter) -> Self {
        self.rate_limiter = Some(limiter);
        self
    }

    /// Convenience: attach a rate limiter only if `limiter` is `Some`.
    ///
    /// Equivalent to `match limiter { Some(l) => self.with_rate_limit(l), None => self }`.
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

    /// Awaits a free slot in the limiter (no-op if unlimited).
    async fn acquire_slot(&self) {
        if let Some(rl) = &self.rate_limiter {
            rl.acquire().await;
        }
    }

    /// Returns the base URL of this client.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Sends a non-streaming chat completion request and returns the response text.
    ///
    /// Builds a messages array from the prompt and optional system message,
    /// posts to `/v1/chat/completions` with `stream: false`, and extracts
    /// `choices[0].message.content`. An optional `max_tokens` cap prevents
    /// excessively long responses.
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, false, false, max_tokens, temperature);
        let resp = self.send_request(&body).await?;
        let completion: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?;
        Ok(extract_content(&completion))
    }

    /// Sends a streaming chat completion request, forwarding tokens as they arrive.
    ///
    /// Posts to `/v1/chat/completions` with `stream: true`. Parses SSE
    /// (Server-Sent Events) data lines, extracts delta content, and sends
    /// each token through `token_tx`. Returns the full accumulated text
    /// after the stream completes. Uses a 5-minute timeout. An optional
    /// `max_tokens` cap prevents excessively long responses.
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, true, false, max_tokens, temperature);

        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut req = self.streaming_client.post(&url).json(&body);
        req = self.apply_auth_headers(req);

        let resp = req
            .send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| ParishError::Network(e.to_string()))?;

        let mut accumulated = String::new();
        let mut line_buf = String::new();
        let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();

        let mut response = resp;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
        {
            // Decode incrementally so multi-byte characters split across
            // HTTP chunk boundaries aren't mangled into U+FFFD (#223).
            line_buf.push_str(&decoder.push(&chunk));

            while let Some(newline_pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=newline_pos).collect();
                match process_sse_line(&line, &token_tx, &mut accumulated) {
                    SseResult::Continue => {}
                    SseResult::Done => return Ok(accumulated),
                }
            }
        }

        // Flush any trailing incomplete bytes, then process any remaining line.
        line_buf.push_str(&decoder.flush());
        let remaining = line_buf.trim();
        if !remaining.is_empty() {
            process_sse_line(remaining, &token_tx, &mut accumulated);
        }

        Ok(accumulated)
    }

    /// Sends a streaming chat completion request with JSON mode enabled.
    ///
    /// Identical to [`generate_stream`] but sets `response_format: json_object`
    /// so the LLM is constrained to return valid JSON. Used for Tier 1 NPC
    /// responses where dialogue is embedded in a JSON structure.
    pub async fn generate_stream_json(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, true, true, max_tokens, temperature);

        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut req = self.streaming_client.post(&url).json(&body);
        req = self.apply_auth_headers(req);

        let resp = req
            .send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| ParishError::Network(e.to_string()))?;

        let mut accumulated = String::new();
        let mut line_buf = String::new();
        let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();

        let mut response = resp;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
        {
            line_buf.push_str(&decoder.push(&chunk));

            while let Some(newline_pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=newline_pos).collect();
                match process_sse_line(&line, &token_tx, &mut accumulated) {
                    SseResult::Continue => {}
                    SseResult::Done => return Ok(accumulated),
                }
            }
        }

        line_buf.push_str(&decoder.flush());
        let remaining = line_buf.trim();
        if !remaining.is_empty() {
            process_sse_line(remaining, &token_tx, &mut accumulated);
        }

        Ok(accumulated)
    }

    /// Sends a non-streaming request and deserializes the response as structured JSON.
    ///
    /// Requests JSON output via `response_format: {"type": "json_object"}` and
    /// parses the response content into the target type `T`. Use
    /// `#[serde(default)]` on optional fields in `T` for robustness. An
    /// optional `max_tokens` cap prevents excessively long responses.
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> Result<T, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, false, true, max_tokens, temperature);
        let resp = self.send_request(&body).await?;
        let completion: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?;
        let content = extract_content(&completion);
        let parsed: T = serde_json::from_str(&content)?;
        Ok(parsed)
    }

    /// Builds a chat completion request body.
    #[allow(clippy::too_many_arguments)] // builder pattern with all params explicit
    fn build_request<'a>(
        &self,
        model: &'a str,
        prompt: &'a str,
        system: Option<&'a str>,
        stream: bool,
        json_mode: bool,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
    ) -> ChatCompletionRequest<'a> {
        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(ChatMessage {
                role: "system",
                content: sys,
            });
        }
        messages.push(ChatMessage {
            role: "user",
            content: prompt,
        });

        let response_format = if json_mode {
            Some(ResponseFormat {
                format_type: "json_object".to_string(),
            })
        } else {
            None
        };

        ChatCompletionRequest {
            model,
            messages,
            stream,
            response_format,
            max_tokens,
            temperature,
        }
    }

    /// Sends a non-streaming request and returns the raw response.
    async fn send_request(
        &self,
        body: &ChatCompletionRequest<'_>,
    ) -> Result<reqwest::Response, ParishError> {
        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut req = self.client.post(&url).json(body);
        req = self.apply_auth_headers(req);

        req.send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| ParishError::Network(e.to_string()))
    }

    /// Applies authorization and provider-specific headers to a request.
    ///
    /// OpenRouter-specific headers (`HTTP-Referer`, `X-Title`) are only sent
    /// when the base URL targets OpenRouter, avoiding client fingerprinting
    /// on other providers.
    fn apply_auth_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = match &self.api_key {
            Some(key) => req.header("Authorization", format!("Bearer {}", key)),
            None => req,
        };
        if self.base_url.contains("openrouter") {
            req.header("HTTP-Referer", "https://github.com/parish-game/parish")
                .header("X-Title", "Parish")
        } else {
            req
        }
    }
}

/// Result of processing a single SSE line.
enum SseResult {
    /// Continue reading more lines.
    Continue,
    /// Stream is complete.
    Done,
}

/// Processes a single SSE line: extracts content, sends tokens, detects completion.
fn process_sse_line(
    line: &str,
    token_tx: &mpsc::Sender<String>,
    accumulated: &mut String,
) -> SseResult {
    let Some(data) = parse_sse_line(line) else {
        return SseResult::Continue;
    };
    match data {
        SseData::Done => SseResult::Done,
        SseData::Chunk(chunk_data) => {
            if let Some(text) = chunk_data
                .choices
                .first()
                .and_then(|c| c.delta.content.as_deref())
                .filter(|t| !t.is_empty())
            {
                if token_tx.try_send(text.to_string()).is_err() {
                    tracing::warn!(
                        "token streaming channel full (capacity {}); token dropped — \
                         consumer is not keeping up with LLM output (#83)",
                        TOKEN_CHANNEL_CAPACITY,
                    );
                }
                accumulated.push_str(text);
            }
            if chunk_data
                .choices
                .first()
                .and_then(|c| c.finish_reason.as_deref())
                == Some("stop")
            {
                return SseResult::Done;
            }
            SseResult::Continue
        }
    }
}

/// Parsed SSE data from a streaming line.
enum SseData {
    /// The `[DONE]` sentinel, indicating stream end.
    Done,
    /// A parsed chunk of streaming data.
    Chunk(ChatCompletionChunk),
}

/// Parses a single SSE line from a streaming response.
///
/// Handles the `data: ` prefix (with or without space), `[DONE]` sentinel,
/// and `: ` keepalive comments. Returns `None` for empty lines, comments,
/// or unparseable data.
fn parse_sse_line(line: &str) -> Option<SseData> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    // SSE comment (keepalive)
    if line.starts_with(": ") || line == ":" {
        return None;
    }

    // Strip the "data: " or "data:" prefix
    let data = if let Some(d) = line.strip_prefix("data: ") {
        d
    } else {
        line.strip_prefix("data:")?
    };

    let data = data.trim();

    if data == "[DONE]" {
        return Some(SseData::Done);
    }

    serde_json::from_str::<ChatCompletionChunk>(data)
        .ok()
        .map(SseData::Chunk)
}

/// Extracts the text content from a non-streaming response.
fn extract_content(resp: &ChatCompletionResponse) -> String {
    resp.choices
        .first()
        .and_then(|c| c.message.content.as_deref())
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression test for #98: the helper must never panic, even when
    /// given an extreme timeout. The normal reqwest build path always
    /// succeeds on a healthy system, so this mainly proves the function
    /// is invokable and returns a usable client.
    #[test]
    fn test_build_client_or_fallback_returns_client() {
        let client = build_client_or_fallback(Duration::from_secs(30), "test");
        // Build a request builder to prove the returned client is usable.
        let _ = client.get("http://127.0.0.1:1/ping");
    }

    /// Regression test for #98: constructors must not panic at a system
    /// boundary. Previously `.expect()` would abort the whole process
    /// if reqwest failed to build.
    #[test]
    fn test_openai_client_new_does_not_panic() {
        let _ = OpenAiClient::new("http://localhost:11434", None);
    }

    #[test]
    fn test_openai_client_new() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        assert_eq!(client.base_url(), "http://localhost:11434");
        assert!(client.api_key.is_none());
    }

    #[test]
    fn test_openai_client_trailing_slash() {
        let client = OpenAiClient::new("http://localhost:11434/", None);
        assert_eq!(client.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_openai_client_with_api_key() {
        let client = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
        assert_eq!(client.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn test_openai_client_starts_without_rate_limiter() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        assert!(!client.has_rate_limiter());
    }

    #[test]
    fn test_with_rate_limit_attaches_limiter() {
        let limiter = InferenceRateLimiter::new(60, 5).expect("limiter");
        let client = OpenAiClient::new("http://localhost:11434", None).with_rate_limit(limiter);
        assert!(client.has_rate_limiter());
    }

    #[test]
    fn test_maybe_with_rate_limit_some() {
        let limiter = InferenceRateLimiter::new(60, 5);
        let client =
            OpenAiClient::new("http://localhost:11434", None).maybe_with_rate_limit(limiter);
        assert!(client.has_rate_limiter());
    }

    #[test]
    fn test_maybe_with_rate_limit_none_is_noop() {
        let client = OpenAiClient::new("http://localhost:11434", None).maybe_with_rate_limit(None);
        assert!(!client.has_rate_limiter());
    }

    #[tokio::test]
    async fn test_acquire_slot_is_noop_without_limiter() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        // Should return immediately and not panic.
        client.acquire_slot().await;
    }

    #[tokio::test]
    async fn test_acquire_slot_blocks_when_limiter_exhausted() {
        // 600/min = 10/sec; burst 1.
        let limiter = InferenceRateLimiter::new(600, 1).expect("limiter");
        let client = OpenAiClient::new("http://localhost:11434", None).with_rate_limit(limiter);
        client.acquire_slot().await; // consume burst
        let start = std::time::Instant::now();
        client.acquire_slot().await; // must wait ~100ms
        assert!(start.elapsed() >= std::time::Duration::from_millis(50));
    }

    #[test]
    fn test_build_request_with_system() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "model",
            "hello",
            Some("you are helpful"),
            false,
            false,
            None,
            None,
        );
        assert_eq!(req.model, "model");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[0].content, "you are helpful");
        assert_eq!(req.messages[1].role, "user");
        assert_eq!(req.messages[1].content, "hello");
        assert!(!req.stream);
        assert!(req.response_format.is_none());
    }

    #[test]
    fn test_build_request_without_system() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("model", "hello", None, false, false, None, None);
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
    }

    #[test]
    fn test_build_request_json_mode() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("model", "hello", None, false, true, None, None);
        let fmt = req.response_format.unwrap();
        assert_eq!(fmt.format_type, "json_object");
    }

    #[test]
    fn test_build_request_streaming() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("model", "hello", None, true, false, None, None);
        assert!(req.stream);
    }

    #[test]
    fn test_chat_completion_response_deserialize() {
        let json = r#"{"choices":[{"message":{"content":"Hello!"}}]}"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_content(&resp), "Hello!");
    }

    #[test]
    fn test_chat_completion_response_empty_choices() {
        let json = r#"{"choices":[]}"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_content(&resp), "");
    }

    #[test]
    fn test_chat_completion_response_null_content() {
        let json = r#"{"choices":[{"message":{"content":null}}]}"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_content(&resp), "");
    }

    #[test]
    fn test_chat_completion_response_missing_fields() {
        let json = r#"{}"#;
        let resp: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(extract_content(&resp), "");
    }

    #[test]
    fn test_chat_completion_chunk_deserialize() {
        let json = r#"{"choices":[{"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert_eq!(chunk.choices[0].delta.content.as_deref(), Some("Hello"));
        assert!(chunk.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_chat_completion_chunk_finish() {
        let json = r#"{"choices":[{"delta":{},"finish_reason":"stop"}]}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices[0].delta.content.is_none());
        assert_eq!(chunk.choices[0].finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn test_chat_completion_chunk_empty() {
        let json = r#"{}"#;
        let chunk: ChatCompletionChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.choices.is_empty());
    }

    #[test]
    fn test_parse_sse_line_data() {
        let line = r#"data: {"choices":[{"delta":{"content":"hi"}}]}"#;
        match parse_sse_line(line).unwrap() {
            SseData::Chunk(c) => {
                assert_eq!(c.choices[0].delta.content.as_deref(), Some("hi"));
            }
            SseData::Done => panic!("expected chunk"),
        }
    }

    #[test]
    fn test_parse_sse_line_data_no_space() {
        let line = r#"data:{"choices":[{"delta":{"content":"hi"}}]}"#;
        match parse_sse_line(line).unwrap() {
            SseData::Chunk(c) => {
                assert_eq!(c.choices[0].delta.content.as_deref(), Some("hi"));
            }
            SseData::Done => panic!("expected chunk"),
        }
    }

    #[test]
    fn test_parse_sse_line_done() {
        assert!(matches!(
            parse_sse_line("data: [DONE]").unwrap(),
            SseData::Done
        ));
    }

    #[test]
    fn test_parse_sse_line_empty() {
        assert!(parse_sse_line("").is_none());
        assert!(parse_sse_line("   ").is_none());
    }

    #[test]
    fn test_parse_sse_line_comment() {
        assert!(parse_sse_line(": keepalive").is_none());
        assert!(parse_sse_line(":").is_none());
    }

    #[test]
    fn test_parse_sse_line_not_data() {
        assert!(parse_sse_line("event: message").is_none());
    }

    #[test]
    fn test_parse_sse_line_invalid_json() {
        assert!(parse_sse_line("data: {invalid}").is_none());
    }

    #[test]
    fn test_request_serialization() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            Some("be brief"),
            false,
            false,
            None,
            None,
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "qwen3:14b");
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][0]["content"], "be brief");
        assert_eq!(json["messages"][1]["role"], "user");
        assert_eq!(json["messages"][1]["content"], "hello");
        assert_eq!(json["stream"], false);
        assert!(json.get("response_format").is_none());
        assert!(json.get("max_tokens").is_none());
    }

    #[test]
    fn test_request_serialization_json_mode() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("qwen3:14b", "hello", None, false, true, None, None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["response_format"]["type"], "json_object");
    }

    #[test]
    fn test_request_serialization_with_max_tokens() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("qwen3:14b", "hello", None, false, false, Some(300), None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["max_tokens"], 300);
    }

    #[test]
    fn test_request_serialization_with_temperature() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("qwen3:14b", "hello", None, false, false, None, Some(0.7));
        let json = serde_json::to_value(&req).unwrap();
        assert!((json["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_request_serialization_temperature_omitted_when_none() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("qwen3:14b", "hello", None, false, false, None, None);
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("temperature").is_none());
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_live() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let result = client
            .generate("qwen3:14b", "Say hello in one word.", None, None, None)
            .await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_stream_live() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let (tx, mut rx) = mpsc::channel(TOKEN_CHANNEL_CAPACITY);
        let result = client
            .generate_stream("qwen3:14b", "Say hello in one word.", None, tx, None, None)
            .await;
        assert!(result.is_ok());

        // Verify tokens were sent
        let mut tokens = Vec::new();
        while let Ok(t) = rx.try_recv() {
            tokens.push(t);
        }
        assert!(!tokens.is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_json_live() {
        #[derive(Deserialize, Debug)]
        #[allow(dead_code)] // used only for JSON deserialization test
        struct TestResponse {
            #[serde(default)]
            greeting: String,
        }
        let client = OpenAiClient::new("http://localhost:11434", None);
        let result: Result<TestResponse, _> = client
            .generate_json(
                "qwen3:14b",
                "Return a JSON object with a 'greeting' field containing 'hello'.",
                None,
                None,
                None,
            )
            .await;
        assert!(result.is_ok());
    }
}
