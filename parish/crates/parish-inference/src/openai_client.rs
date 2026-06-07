//! OpenAI-compatible HTTP client for LLM inference.
//!
//! Talks to any provider that implements the OpenAI chat completions API:
//! Ollama (`/v1/chat/completions`), LM Studio, OpenRouter, or any custom
//! OpenAI-compatible endpoint. Uses SSE (Server-Sent Events) for streaming.

use crate::SseResult;
use crate::TOKEN_CHANNEL_CAPACITY;
use crate::client_base::ClientBase;
use crate::rate_limit::InferenceRateLimiter;
use crate::strip_json_fence;
use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// Builds a `reqwest::Client` with the given timeout, falling back to a default
/// client (no timeout) if the builder fails.
///
/// Must not panic at this system boundary (#98): if the TLS backend fails to
/// initialize we log a warning and return a default client so the application
/// degrades gracefully rather than crashing at startup. See
/// `test_openai_client_new_does_not_panic` for the regression guard.
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
    /// Shared HTTP client state (fields, builder methods, rate limiter).
    pub(crate) base: ClientBase,
    /// Path appended to `base_url` to form the chat completions endpoint.
    /// Defaults to `"/v1/chat/completions"`. Override via
    /// [`OpenAiClient::with_completions_path`] for providers that omit the
    /// `/v1` prefix (e.g. GitHub Models uses `"/chat/completions"`).
    completions_path: String,
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
    /// OpenAI-compat `frequency_penalty`. Range nominally `[-2.0, 2.0]`,
    /// but the Tier 1 dialogue call site sets `0.5` to break the
    /// degenerate repetition loops Qwen2.5-14B-4bit exhibits without a
    /// penalty (TODO #10 / #23 / #34). vllm-mlx, OpenAI, OpenRouter and
    /// most OpenAI-compat servers honour this field; Ollama ignores it.
    /// `None` omits the key from the wire body entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    frequency_penalty: Option<f32>,
}

/// Sampling and generation parameters shared across all generate methods.
///
/// Groups the three optional knobs that every generate call accepts so that
/// functions with a `model + prompt + system + token_tx + response_format +
/// GenerateParams` signature stay within Clippy's `too-many-arguments` limit
/// (≤ 7 non-`self` parameters).
#[derive(Debug, Clone, Default)]
pub struct GenerateParams {
    /// Maximum number of tokens the model may emit.  `None` lets the
    /// provider apply its own default ceiling.
    pub max_tokens: Option<u32>,
    /// Sampling temperature.  `None` uses the provider default.
    pub temperature: Option<f32>,
    /// OpenAI-compat `frequency_penalty` (`[-2.0, 2.0]`). Forwarded to
    /// vllm-mlx, LM Studio, OpenAI, and OpenRouter; ignored by Anthropic
    /// and the Simulator (no equivalent).  `None` omits the key from the
    /// wire body entirely.
    pub frequency_penalty: Option<f32>,
}

/// Controls structured output format.
///
/// Wire format follows OpenAI's `response_format` shape, which Ollama and
/// most OpenAI-compat servers accept. LM Studio and vllm-mlx both reject
/// the bare `{"type": "json_object"}` shorthand and require either
/// `{"type": "text"}` or `{"type": "json_schema", "json_schema": {...}}`.
/// Callers should prefer `JsonSchema` and only fall back to `JsonObject`
/// when targeting Ollama specifically. To send "no constraint", pass
/// `None` instead of constructing a `Text` variant.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// `{"type": "json_object"}` — legacy Ollama path. Constrains output
    /// to *some* JSON; the structure is implied by the prompt.
    JsonObject,
    /// `{"type": "json_schema", "json_schema": {"name": ..., "schema": ...}}`
    /// — strict-mode structured output. Required by vllm-mlx, LM Studio,
    /// and OpenAI's structured-outputs feature. The model is constrained
    /// to emit JSON matching the supplied schema.
    JsonSchema { json_schema: JsonSchemaSpec },
}

/// The named-schema payload that sits under `response_format.json_schema`.
#[derive(Serialize, Debug, Clone)]
pub struct JsonSchemaSpec {
    /// Schema name (display label, also a routing key in some servers).
    pub name: String,
    /// The JSON Schema document the model must conform to.
    pub schema: serde_json::Value,
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
struct ChatCompletionChunk {
    #[serde(default)]
    choices: Vec<StreamChoice>,
}

/// A single choice in a streaming chunk.
#[derive(Deserialize, Debug)]
struct StreamChoice {
    #[serde(default)]
    delta: Delta,
    #[serde(default)]
    finish_reason: Option<String>,
}

/// Delta content in a streaming chunk.
#[derive(Deserialize, Debug, Default)]
struct Delta {
    #[serde(default)]
    content: Option<String>,
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
        Self {
            base: ClientBase::new(
                base_url,
                api_key,
                "OpenAI-compatible",
                "OpenAI-compatible streaming",
                config,
            ),
            completions_path: "/v1/chat/completions".to_string(),
        }
    }

    /// Overrides the completions path appended to `base_url`.
    ///
    /// Use this for providers that do not follow the standard `/v1/chat/completions`
    /// convention. For example, GitHub Models uses `"/chat/completions"`:
    /// ```text
    /// client.with_completions_path("/chat/completions")
    /// // → https://models.github.ai/inference/chat/completions
    /// ```
    pub fn with_completions_path(mut self, path: &str) -> Self {
        self.completions_path = path.to_string();
        self
    }

    /// Attaches an outbound rate limiter, returning the modified client.
    ///
    /// All subsequent `generate*` calls will block on the limiter
    /// before issuing the HTTP request. Use [`InferenceRateLimiter::from_config`]
    /// to build a limiter from a `parish.toml` `[rate_limits]` entry.
    pub fn with_rate_limit(self, limiter: InferenceRateLimiter) -> Self {
        Self {
            base: self.base.with_rate_limit(limiter),
            completions_path: self.completions_path,
        }
    }

    /// Convenience: attach a rate limiter only if `limiter` is `Some`.
    ///
    /// Equivalent to `match limiter { Some(l) => self.with_rate_limit(l), None => self }`.
    pub fn maybe_with_rate_limit(self, limiter: Option<InferenceRateLimiter>) -> Self {
        Self {
            base: self.base.maybe_with_rate_limit(limiter),
            completions_path: self.completions_path,
        }
    }

    /// Returns whether this client has a rate limiter attached.
    pub fn has_rate_limiter(&self) -> bool {
        self.base.has_rate_limiter()
    }

    /// Awaits a free slot in the limiter (no-op if unlimited).
    async fn acquire_slot(&self) {
        self.base.acquire_slot().await
    }

    /// Returns the base URL of this client.
    pub fn base_url(&self) -> &str {
        self.base.base_url()
    }

    /// Sends a non-streaming chat completion request and returns the response text.
    ///
    /// Builds a messages array from the prompt and optional system message,
    /// posts to `/v1/chat/completions` with `stream: false`, and extracts
    /// `choices[0].message.content`. Sampling knobs (max tokens, temperature,
    /// frequency penalty) are supplied via [`GenerateParams`].
    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, false, None, params);
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
    /// after the stream completes. Uses `InferenceConfig::streaming_timeout_secs`
    /// as the timeout. Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, true, None, params);
        self.stream_response(body, token_tx).await
    }

    /// Sends a streaming chat completion request with JSON mode enabled.
    ///
    /// Identical to [`generate_stream`] but sets `response_format: json_object`
    /// so the LLM is constrained to return valid JSON. Used for Tier 1 NPC
    /// responses where dialogue is embedded in a JSON structure.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream_json(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(
            model,
            prompt,
            system,
            true,
            Some(ResponseFormat::JsonObject),
            params,
        );
        self.stream_response(body, token_tx).await
    }

    /// Sends a non-streaming request and deserializes the response as structured JSON.
    ///
    /// Equivalent to `generate_json_with_format` with
    /// `response_format = Some(ResponseFormat::JsonObject)`. Kept as a thin
    /// wrapper because most callers want the legacy Ollama-compatible
    /// behaviour; new callers targeting LM Studio / vllm-mlx should use
    /// [`generate_json_with_format`] with a `JsonSchema` instead.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_json<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        params: GenerateParams,
    ) -> Result<T, ParishError> {
        self.generate_json_with_format(
            model,
            prompt,
            system,
            Some(ResponseFormat::JsonObject),
            params,
        )
        .await
    }

    /// Like [`generate`] but lets the caller pick the wire-level
    /// response_format. Pass `None` for unconstrained text, `JsonObject`
    /// for the Ollama-style "some JSON" mode, or `JsonSchema` for strict
    /// schema-guided decoding (vllm-mlx, LM Studio, OpenAI structured
    /// outputs). Returns the raw response content; callers parse JSON
    /// themselves.  Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_text_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, false, response_format, params);
        let resp = self.send_request(&body).await?;
        let completion: ChatCompletionResponse = resp
            .json()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?;
        let trimmed = extract_content(&completion);
        Ok(strip_json_fence(&trimmed).to_string())
    }

    /// Typed counterpart of [`generate_text_with_format`]. Same
    /// trade-offs; deserialises the response into `T`.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_json_with_format<T: DeserializeOwned>(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<T, ParishError> {
        let raw = self
            .generate_text_with_format(model, prompt, system, response_format, params)
            .await?;
        let parsed: T = serde_json::from_str(&raw)?;
        Ok(parsed)
    }

    /// Streaming counterpart to [`generate_json_with_format`]. Same
    /// trade-offs: `JsonObject` for Ollama compat, `JsonSchema` for strict
    /// servers, `None` for unconstrained text streaming.
    /// Sampling knobs are supplied via [`GenerateParams`].
    pub async fn generate_stream_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.acquire_slot().await;
        let body = self.build_request(model, prompt, system, true, response_format, params);
        self.stream_response(body, token_tx).await
    }

    /// Builds a chat completion request body.
    ///
    /// `response_format` is taken verbatim — callers decide whether to send
    /// `text`, `json_object`, or a fully-typed `json_schema` based on what
    /// the target server accepts. See [`ResponseFormat`] for the wire
    /// shapes.
    fn build_request<'a>(
        &self,
        model: &'a str,
        prompt: &'a str,
        system: Option<&'a str>,
        stream: bool,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
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

        ChatCompletionRequest {
            model,
            messages,
            stream,
            response_format,
            max_tokens: params.max_tokens,
            temperature: params.temperature,
            frequency_penalty: params.frequency_penalty,
        }
    }

    /// Shared streaming path: posts the request body, decodes the SSE stream.
    ///
    /// Used by both [`generate_stream`] and [`generate_stream_json`] to
    /// avoid duplicating the HTTP request and SSE parsing loop.
    async fn stream_response(
        &self,
        body: ChatCompletionRequest<'_>,
        token_tx: mpsc::Sender<String>,
    ) -> Result<String, ParishError> {
        let url = format!("{}{}", self.base.base_url, self.completions_path);
        let mut req = self.base.streaming_client.post(&url).json(&body);
        req = self.apply_auth_headers(req);

        let resp = req
            .send()
            .await
            .map_err(|e| ParishError::Network(e.to_string()))?
            .error_for_status()
            .map_err(|e| ParishError::Network(e.to_string()))?;

        read_sse_stream(resp, &token_tx).await
    }

    /// Sends a non-streaming request and returns the raw response.
    async fn send_request(
        &self,
        body: &ChatCompletionRequest<'_>,
    ) -> Result<reqwest::Response, ParishError> {
        let url = format!("{}{}", self.base.base_url, self.completions_path);
        let mut req = self.base.client.post(&url).json(body);
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
        let req = match &self.base.api_key {
            Some(key) => req.header("Authorization", format!("Bearer {}", key)),
            None => req,
        };
        if self.base.base_url.contains("openrouter") {
            req.header("HTTP-Referer", "https://github.com/parish-game/parish")
                .header("X-Title", "Parish")
        } else {
            req
        }
    }
}

/// Reads an SSE response body, parsing data lines and forwarding tokens.
///
/// Shared by [`OpenAiClient::generate_stream`] and
/// [`OpenAiClient::generate_stream_json`] to avoid duplicating the
/// streaming-loop boilerplate (TD-004).
async fn read_sse_stream(
    response: reqwest::Response,
    token_tx: &mpsc::Sender<String>,
) -> Result<String, ParishError> {
    let mut accumulated = String::new();
    let mut line_buf = String::new();
    let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();

    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| ParishError::Network(e.to_string()))?
    {
        line_buf.push_str(&decoder.push(&chunk));

        while let Some(newline_pos) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=newline_pos).collect();
            match process_sse_line(&line, token_tx, &mut accumulated) {
                SseResult::Continue => {}
                SseResult::Done => return Ok(accumulated),
                SseResult::Error(msg) => return Err(ParishError::Inference(msg)),
            }
        }
    }

    line_buf.push_str(&decoder.flush());
    let remaining = line_buf.trim();
    if !remaining.is_empty() {
        match process_sse_line(remaining, token_tx, &mut accumulated) {
            SseResult::Continue => {}
            SseResult::Done => return Ok(accumulated),
            SseResult::Error(msg) => return Err(ParishError::Inference(msg)),
        }
    }

    Ok(accumulated)
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
    /// boundary if reqwest fails to build the underlying client.
    #[test]
    fn test_openai_client_new_does_not_panic() {
        let _ = OpenAiClient::new("http://localhost:11434", None);
    }

    #[test]
    fn test_openai_client_new() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        assert_eq!(client.base_url(), "http://localhost:11434");
        assert!(client.base.api_key.is_none());
    }

    #[test]
    fn test_openai_client_trailing_slash() {
        let client = OpenAiClient::new("http://localhost:11434/", None);
        assert_eq!(client.base_url(), "http://localhost:11434");
    }

    #[test]
    fn test_openai_client_with_api_key() {
        let client = OpenAiClient::new("https://openrouter.ai/api", Some("sk-test"));
        assert_eq!(client.base.api_key.as_deref(), Some("sk-test"));
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
            None,
            GenerateParams::default(),
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
        let req = client.build_request(
            "model",
            "hello",
            None,
            false,
            None,
            GenerateParams::default(),
        );
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
    }

    #[test]
    fn test_build_request_json_mode() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "model",
            "hello",
            None,
            false,
            Some(ResponseFormat::JsonObject),
            GenerateParams::default(),
        );
        let fmt = req.response_format.unwrap();
        let serialized = serde_json::to_value(&fmt).unwrap();
        assert_eq!(serialized["type"], "json_object");
    }

    #[test]
    fn test_build_request_json_schema() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "model",
            "hello",
            None,
            false,
            Some(ResponseFormat::JsonSchema {
                json_schema: JsonSchemaSpec {
                    name: "intent".to_string(),
                    schema: serde_json::json!({"type":"object"}),
                },
            }),
            GenerateParams::default(),
        );
        let fmt = req.response_format.unwrap();
        let serialized = serde_json::to_value(&fmt).unwrap();
        assert_eq!(serialized["type"], "json_schema");
        assert_eq!(serialized["json_schema"]["name"], "intent");
    }

    #[test]
    fn test_build_request_streaming() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "model",
            "hello",
            None,
            true,
            None,
            GenerateParams::default(),
        );
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
            None,
            GenerateParams::default(),
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
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            Some(ResponseFormat::JsonObject),
            GenerateParams::default(),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["response_format"]["type"], "json_object");
    }

    #[test]
    fn test_request_serialization_with_max_tokens() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                max_tokens: Some(300),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["max_tokens"], 300);
    }

    #[test]
    fn test_request_serialization_with_temperature() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                temperature: Some(0.7),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!((json["temperature"].as_f64().unwrap() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_request_serialization_temperature_omitted_when_none() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams::default(),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("temperature").is_none());
    }

    /// TODO #10/#23/#34: `frequency_penalty` must serialize on the wire when
    /// set and must be omitted entirely when `None`, so existing
    /// Ollama-targeted requests (which ignore the field) keep their shape.
    #[test]
    fn test_request_serialization_with_frequency_penalty() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                frequency_penalty: Some(0.5),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!((json["frequency_penalty"].as_f64().unwrap() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_request_serialization_frequency_penalty_omitted_when_none() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams::default(),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("frequency_penalty").is_none());
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_live() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let result = client
            .generate(
                "qwen3:14b",
                "Say hello in one word.",
                None,
                GenerateParams::default(),
            )
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
            .generate_stream(
                "qwen3:14b",
                "Say hello in one word.",
                None,
                tx,
                GenerateParams::default(),
            )
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
                GenerateParams::default(),
            )
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_generate_blocks_when_rate_limiter_exhausted() {
        use std::time::Instant;

        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "Hello!"}}]
                })),
            )
            .mount(&server)
            .await;

        let limiter = InferenceRateLimiter::new(600, 1).expect("limiter");
        let client = OpenAiClient::new(&server.uri(), None).with_rate_limit(limiter);

        let start = Instant::now();
        let _result1 = client
            .generate("test-model", "hi", None, GenerateParams::default())
            .await;
        let elapsed_first = start.elapsed();

        let start = Instant::now();
        let _result2 = client
            .generate("test-model", "hi", None, GenerateParams::default())
            .await;
        let elapsed_second = start.elapsed();

        assert!(
            elapsed_second > elapsed_first,
            "second generate (rate-limited) should take longer: first={:?}, second={:?}",
            elapsed_first,
            elapsed_second,
        );
        assert!(
            elapsed_second >= std::time::Duration::from_millis(50),
            "second call waited {:?}, expected at least 50ms refill wait",
            elapsed_second,
        );
    }
}
