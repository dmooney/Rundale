//! OpenAI-compatible HTTP client for LLM inference.
//!
//! Talks to any provider that implements the OpenAI chat completions API:
//! Ollama (`/v1/chat/completions`), LM Studio, OpenRouter, or any custom
//! OpenAI-compatible endpoint. Uses SSE (Server-Sent Events) for streaming.
//!
//! Structure (#1200 decomposition): the former single module is split into
//! - [`wire`] — request/response schema types and the public parameter
//!   types (`GenerateParams`, `ResponseFormat`, `JsonSchemaSpec`);
//! - [`sse`] — the streaming read loop and line parser;
//! - this `mod.rs` — the [`OpenAiClient`] type and its `generate*` methods.
//!
//! The public parameter types are re-exported here (and from `lib.rs`) so the
//! paths `openai_client::{GenerateParams, ResponseFormat, JsonSchemaSpec}`
//! are unchanged.

use crate::client_base::ClientBase;
use crate::rate_limit::InferenceRateLimiter;
use crate::strip_json_fence;
use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::de::DeserializeOwned;
use std::time::Duration;
use tokio::sync::mpsc;

mod sse;
mod wire;

pub use wire::{GenerateParams, JsonSchemaSpec, ResponseFormat};

use sse::read_sse_stream;
use wire::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, ChatTemplateKwargs,
    DeepSeekThinkingConfig, ReasoningConfig, extract_content,
};

/// Builds a `reqwest::Client` with the given timeout, falling back to a default
/// client (no timeout) if the builder fails.
///
/// Must not panic at this system boundary (#98): if the TLS backend fails to
/// initialize we log a warning and return a default client so the application
/// degrades gracefully rather than crashing at startup. See
/// `test_openai_client_new_does_not_panic` for the regression guard.
///
/// `pub` so `parish_inference::setup` (reachability probes, warmup) and
/// `parish_inference::validate` can reuse the same hardened builder rather
/// than constructing bare `reqwest::Client`s with their own panic surface.
pub fn build_client_or_fallback(timeout: Duration, label: &'static str) -> reqwest::Client {
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

    /// Match OpenRouter by parsed hostname, never by an attacker-controlled
    /// substring in a path or lookalike hostname.
    fn is_openrouter(&self) -> bool {
        reqwest::Url::parse(self.base.base_url())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| host == "openrouter.ai" || host.ends_with(".openrouter.ai"))
    }

    /// Match DeepSeek's first-party API by parsed hostname. Third-party
    /// OpenAI-compatible hosts serving DeepSeek models must keep their own
    /// wire contract instead of receiving DeepSeek-only request fields.
    fn is_deepseek(&self) -> bool {
        reqwest::Url::parse(self.base.base_url())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| host == "api.deepseek.com")
    }

    /// Match Google's first-party Gemini OpenAI-compat endpoint by hostname.
    fn is_google_generative_ai(&self) -> bool {
        reqwest::Url::parse(self.base.base_url())
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| host == "generativelanguage.googleapis.com")
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

        let (enable_thinking, chat_template_kwargs, reasoning, thinking, reasoning_effort) =
            if self.is_openrouter() {
                let reasoning = params
                    .reasoning_effort
                    .map(|effort| ReasoningConfig {
                        effort: Some(effort.as_str()),
                        enabled: None,
                        exclude: (effort == parish_config::ReasoningEffort::None).then_some(true),
                    })
                    .or_else(|| {
                        params.enable_thinking.map(|enabled| {
                            if enabled {
                                ReasoningConfig {
                                    effort: None,
                                    enabled: Some(true),
                                    exclude: None,
                                }
                            } else {
                                ReasoningConfig {
                                    effort: Some("none"),
                                    enabled: None,
                                    exclude: Some(true),
                                }
                            }
                        })
                    });
                (None, None, reasoning, None, None)
            } else if self.is_deepseek() {
                let explicit_effort = params.reasoning_effort;
                let enabled = explicit_effort
                    .is_some_and(|effort| effort != parish_config::ReasoningEffort::None)
                    || (explicit_effort.is_none() && params.enable_thinking == Some(true));
                let disabled = explicit_effort == Some(parish_config::ReasoningEffort::None)
                    || (explicit_effort.is_none() && params.enable_thinking == Some(false));
                let thinking = if enabled {
                    Some(DeepSeekThinkingConfig { kind: "enabled" })
                } else if disabled {
                    Some(DeepSeekThinkingConfig { kind: "disabled" })
                } else {
                    None
                };
                // DeepSeek V4 currently exposes high/max. Its API documents
                // low/medium as high aliases and xhigh as a max alias.
                let reasoning_effort = explicit_effort.and_then(|effort| match effort {
                    parish_config::ReasoningEffort::None => None,
                    parish_config::ReasoningEffort::Minimal
                    | parish_config::ReasoningEffort::Low
                    | parish_config::ReasoningEffort::Medium
                    | parish_config::ReasoningEffort::High => Some("high"),
                    parish_config::ReasoningEffort::Xhigh | parish_config::ReasoningEffort::Max => {
                        Some("max")
                    }
                });
                (None, None, None, thinking, reasoning_effort)
            } else if self.is_google_generative_ai() {
                // Gemini's OpenAI-compat endpoint accepts a top-level effort.
                // Gemini 3 cannot disable thinking; clamp provider-neutral levels
                // above its documented maximum and omit an explicit `none`.
                let reasoning_effort = params.reasoning_effort.and_then(|effort| match effort {
                    parish_config::ReasoningEffort::None => None,
                    parish_config::ReasoningEffort::Minimal => Some("minimal"),
                    parish_config::ReasoningEffort::Low => Some("low"),
                    parish_config::ReasoningEffort::Medium => Some("medium"),
                    parish_config::ReasoningEffort::High
                    | parish_config::ReasoningEffort::Xhigh
                    | parish_config::ReasoningEffort::Max => Some("high"),
                });
                (None, None, None, None, reasoning_effort)
            } else {
                let chat_template_kwargs = params
                    .enable_thinking
                    .map(|enable_thinking| ChatTemplateKwargs { enable_thinking });
                (
                    params.enable_thinking,
                    chat_template_kwargs,
                    None,
                    None,
                    None,
                )
            };
        ChatCompletionRequest {
            model,
            messages,
            stream,
            response_format,
            max_tokens: params.max_tokens,
            // Gemini 3.6 Flash and 3.5 Flash-Lite deprecate sampling controls
            // on the first-party API. Google instructs callers to remove
            // temperature; frequency_penalty is not part of its documented
            // OpenAI-compat parameter surface either.
            temperature: if self.is_google_generative_ai()
                && matches!(model, "gemini-3.6-flash" | "gemini-3.5-flash-lite")
            {
                None
            } else {
                params.temperature
            },
            frequency_penalty: if self.is_google_generative_ai() {
                None
            } else {
                params.frequency_penalty
            },
            enable_thinking,
            chat_template_kwargs,
            reasoning,
            thinking,
            reasoning_effort,
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

        // Retry covers only this initial request/response-status phase —
        // once `read_sse_stream` has consumed bytes the request is not
        // retryable (#1366 §3.4).
        let resp = crate::retry::send_with_retry("openai", || {
            let req = self.base.streaming_client.post(&url).json(&body);
            self.apply_auth_headers(req).send()
        })
        .await?
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

        crate::retry::send_with_retry("openai", || {
            let req = self.base.client.post(&url).json(body);
            self.apply_auth_headers(req).send()
        })
        .await?
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
        if self.is_openrouter() {
            req.header("HTTP-Referer", "https://github.com/parish-game/parish")
                .header("X-Title", "Parish")
        } else {
            req
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TOKEN_CHANNEL_CAPACITY;
    use sse::{SseData, parse_sse_line, process_sse_line};
    use wire::ChatCompletionChunk;

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
    fn test_process_sse_line_rejects_length_terminated_partial_response() {
        let (token_tx, _token_rx) = tokio::sync::mpsc::channel(1);
        let mut accumulated = String::from(r#"{"dialogue":"cut off"#);
        let line = r#"data: {"choices":[{"delta":{},"finish_reason":"length"}]}"#;

        match process_sse_line(line, &token_tx, &mut accumulated) {
            crate::SseResult::Error(message) => {
                assert!(message.contains("finish_reason=length"));
            }
            _ => panic!("length termination must not be accepted as success"),
        }
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

    #[test]
    fn test_request_serialization_with_enable_thinking_false() {
        let client = OpenAiClient::new("http://localhost:8010", None);
        let req = client.build_request(
            "mlx-community/Qwen3.5-9B-MLX-4bit",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                enable_thinking: Some(false),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["enable_thinking"], false);
        assert_eq!(json["chat_template_kwargs"]["enable_thinking"], false);
        assert!(json.get("reasoning").is_none());
    }

    #[test]
    fn test_openrouter_translates_enable_thinking_false_to_reasoning_none() {
        let client = OpenAiClient::new("https://openrouter.ai/api/v1", None);
        let req = client.build_request(
            "moonshotai/kimi-k2.5:nitro",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                enable_thinking: Some(false),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["reasoning"]["effort"], "none");
        assert_eq!(json["reasoning"]["exclude"], true);
        assert!(json.get("enable_thinking").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn test_openrouter_serializes_explicit_max_reasoning_effort() {
        let client = OpenAiClient::new("https://openrouter.ai/api/v1", None);
        let req = client.build_request(
            "deepseek/deepseek-v4-flash-0731",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                enable_thinking: Some(true),
                reasoning_effort: Some(parish_config::ReasoningEffort::Max),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["reasoning"]["effort"], "max");
        assert!(json["reasoning"].get("enabled").is_none());
        assert!(json.get("enable_thinking").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
        assert!(json.get("thinking").is_none());
        assert!(json.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_deepseek_serializes_native_high_reasoning_fields() {
        let client = OpenAiClient::new("https://api.deepseek.com", None);
        let req = client.build_request(
            "deepseek-v4-flash",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                enable_thinking: Some(true),
                reasoning_effort: Some(parish_config::ReasoningEffort::Medium),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["thinking"]["type"], "enabled");
        assert_eq!(json["reasoning_effort"], "high");
        assert!(json.get("reasoning").is_none());
        assert!(json.get("enable_thinking").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
    }

    #[test]
    fn test_deepseek_serializes_native_max_alias_and_disable() {
        let client = OpenAiClient::new("https://api.deepseek.com/v1", None);
        let max_req = client.build_request(
            "deepseek-v4-flash",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                reasoning_effort: Some(parish_config::ReasoningEffort::Xhigh),
                ..Default::default()
            },
        );
        let max_json = serde_json::to_value(&max_req).unwrap();
        assert_eq!(max_json["thinking"]["type"], "enabled");
        assert_eq!(max_json["reasoning_effort"], "max");

        let disabled_req = client.build_request(
            "deepseek-v4-flash",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                reasoning_effort: Some(parish_config::ReasoningEffort::None),
                ..Default::default()
            },
        );
        let disabled_json = serde_json::to_value(&disabled_req).unwrap();
        assert_eq!(disabled_json["thinking"]["type"], "disabled");
        assert!(disabled_json.get("reasoning_effort").is_none());
    }

    #[test]
    fn test_google_latest_models_use_native_effort_without_deprecated_sampling() {
        let client = OpenAiClient::new(
            "https://generativelanguage.googleapis.com/v1beta/openai",
            None,
        );
        let req = client.build_request(
            "gemini-3.6-flash",
            "hello",
            None,
            false,
            None,
            GenerateParams {
                temperature: Some(0.7),
                frequency_penalty: Some(0.5),
                enable_thinking: Some(true),
                reasoning_effort: Some(parish_config::ReasoningEffort::Max),
                ..Default::default()
            },
        );
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["reasoning_effort"], "high");
        assert!(json.get("temperature").is_none());
        assert!(json.get("frequency_penalty").is_none());
        assert!(json.get("reasoning").is_none());
        assert!(json.get("enable_thinking").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn test_openrouter_host_detection_rejects_lookalikes() {
        assert!(OpenAiClient::new("https://openrouter.ai/api/v1", None).is_openrouter());
        assert!(OpenAiClient::new("https://api.openrouter.ai/v1", None).is_openrouter());
        assert!(!OpenAiClient::new("https://openrouter.ai.evil.example/v1", None).is_openrouter());
        assert!(!OpenAiClient::new("https://example.test/openrouter.ai/v1", None).is_openrouter());
    }

    #[test]
    fn test_request_serialization_enable_thinking_omitted_when_none() {
        let client = OpenAiClient::new("http://localhost:8010", None);
        let req = client.build_request(
            "qwen3:14b",
            "hello",
            None,
            false,
            None,
            GenerateParams::default(),
        );
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("enable_thinking").is_none());
        assert!(json.get("chat_template_kwargs").is_none());
        assert!(json.get("reasoning").is_none());
        assert!(json.get("thinking").is_none());
        assert!(json.get("reasoning_effort").is_none());
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
        #[derive(serde::Deserialize, Debug)]
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

    /// #1366 §3.4 — a 429 with `Retry-After: 0` is retried and the second
    /// attempt's body is returned; the caller never sees the rate-limit error.
    #[tokio::test]
    async fn test_generate_retries_429_then_succeeds() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "choices": [{"message": {"content": "Hello!"}}]
                })),
            )
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&server.uri(), None);
        let result = client
            .generate("test-model", "hi", None, GenerateParams::default())
            .await;

        assert_eq!(result.expect("retried request should succeed"), "Hello!");
        let requests = server.received_requests().await.expect("recording on");
        assert_eq!(requests.len(), 2, "expected exactly one retry");
    }

    /// Non-transient 4xx must NOT be retried — a caller bug is not load.
    #[tokio::test]
    async fn test_generate_does_not_retry_400() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(400))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&server.uri(), None);
        let result = client
            .generate("test-model", "hi", None, GenerateParams::default())
            .await;

        assert!(result.is_err());
        let requests = server.received_requests().await.expect("recording on");
        assert_eq!(requests.len(), 1, "400 must not be retried");
    }

    /// A provider that never recovers exhausts the retry budget (initial
    /// request + 3 retries) and the final 429 surfaces as an error.
    #[tokio::test]
    async fn test_generate_gives_up_after_max_retries() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::path("/v1/chat/completions"))
            .respond_with(wiremock::ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .mount(&server)
            .await;

        let client = OpenAiClient::new(&server.uri(), None);
        let result = client
            .generate("test-model", "hi", None, GenerateParams::default())
            .await;

        assert!(result.is_err());
        let requests = server.received_requests().await.expect("recording on");
        assert_eq!(requests.len(), 4, "initial request + MAX_RETRIES attempts");
    }
}
