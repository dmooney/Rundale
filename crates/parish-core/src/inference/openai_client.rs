//! OpenAI-compatible HTTP client for LLM inference.
//!
//! Talks to any provider that implements the OpenAI chat completions API:
//! Ollama (`/v1/chat/completions`), LM Studio, OpenRouter, or any custom
//! OpenAI-compatible endpoint. Uses SSE (Server-Sent Events) for streaming.

use crate::config::InferenceConfig;
use crate::error::ParishError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::mpsc;

/// HTTP client for OpenAI-compatible chat completions endpoints.
///
/// Works with Ollama, LM Studio, OpenRouter, and any provider that
/// implements the `/v1/chat/completions` API. Provides the same
/// logical interface as the legacy Ollama-native client: plain text
/// generation, streaming generation, and structured JSON output.
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
    pub fn new_with_config(
        base_url: &str,
        api_key: Option<&str>,
        config: &InferenceConfig,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .expect("failed to build reqwest client");

        // Pre-build the streaming client once so connection pooling is
        // preserved across streaming calls instead of creating a fresh
        // client (and fresh TCP connections) on every request.
        let streaming_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.streaming_timeout_secs))
            .build()
            .expect("failed to build streaming reqwest client");

        Self {
            client,
            streaming_client,
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.map(|s| s.to_string()),
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
    ) -> Result<String, ParishError> {
        let body = self.build_request(model, prompt, system, false, false, max_tokens);
        let resp = self.send_request(&body).await?;
        let completion: ChatCompletionResponse = resp.json().await?;
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
        token_tx: mpsc::UnboundedSender<String>,
        max_tokens: Option<u32>,
    ) -> Result<String, ParishError> {
        let body = self.build_request(model, prompt, system, true, false, max_tokens);

        let url = format!("{}/v1/chat/completions", self.base_url);
        let mut req = self.streaming_client.post(&url).json(&body);
        req = self.apply_auth_headers(req);

        let resp = req
            .send()
            .await?
            .error_for_status()
            .map_err(|e| ParishError::Inference(e.to_string()))?;

        let mut accumulated = String::new();
        let mut line_buf = String::new();

        let mut response = resp;
        while let Some(chunk) = response.chunk().await? {
            let text = String::from_utf8_lossy(&chunk);
            line_buf.push_str(&text);

            while let Some(newline_pos) = line_buf.find('\n') {
                let line: String = line_buf.drain(..=newline_pos).collect();
                match process_sse_line(&line, &token_tx, &mut accumulated) {
                    SseResult::Continue => {}
                    SseResult::Done => return Ok(accumulated),
                }
            }
        }

        // Process any remaining data in the buffer
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
    ) -> Result<T, ParishError> {
        let body = self.build_request(model, prompt, system, false, true, max_tokens);
        let resp = self.send_request(&body).await?;
        let completion: ChatCompletionResponse = resp.json().await?;
        let content = extract_content(&completion);
        let parsed: T = serde_json::from_str(&content)?;
        Ok(parsed)
    }

    /// Builds a chat completion request body.
    fn build_request<'a>(
        &self,
        model: &'a str,
        prompt: &'a str,
        system: Option<&'a str>,
        stream: bool,
        json_mode: bool,
        max_tokens: Option<u32>,
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
            .await?
            .error_for_status()
            .map_err(|e| ParishError::Inference(e.to_string()))
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
    token_tx: &mpsc::UnboundedSender<String>,
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
                let _ = token_tx.send(text.to_string());
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
    } else if let Some(d) = line.strip_prefix("data:") {
        d
    } else {
        return None;
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
    fn test_build_request_with_system() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request(
            "model",
            "hello",
            Some("you are helpful"),
            false,
            false,
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
        let req = client.build_request("model", "hello", None, false, false, None);
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
    }

    #[test]
    fn test_build_request_json_mode() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("model", "hello", None, false, true, None);
        let fmt = req.response_format.unwrap();
        assert_eq!(fmt.format_type, "json_object");
    }

    #[test]
    fn test_build_request_streaming() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("model", "hello", None, true, false, None);
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
        let req = client.build_request("qwen3:14b", "hello", Some("be brief"), false, false, None);
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
        let req = client.build_request("qwen3:14b", "hello", None, false, true, None);
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["response_format"]["type"], "json_object");
    }

    #[test]
    fn test_request_serialization_with_max_tokens() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let req = client.build_request("qwen3:14b", "hello", None, false, false, Some(300));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["max_tokens"], 300);
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_live() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let result = client
            .generate("qwen3:14b", "Say hello in one word.", None, None)
            .await;
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore] // Requires Ollama running on localhost:11434
    async fn test_generate_stream_live() {
        let client = OpenAiClient::new("http://localhost:11434", None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        let result = client
            .generate_stream("qwen3:14b", "Say hello in one word.", None, tx, None)
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
            )
            .await;
        assert!(result.is_ok());
    }
}
