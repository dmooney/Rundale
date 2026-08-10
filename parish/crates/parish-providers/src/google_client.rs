//! Native Google Gemini Interactions API transport.
//!
//! The Google provider deliberately does not use the OpenAI compatibility
//! endpoint. It speaks the stable v1 Interactions wire format, keeps requests
//! stateless (`store: false`), and retains provider-reported usage so callers
//! can distinguish visible output, thinking, and implicitly cached input.

use std::time::{Instant, SystemTime};

use parish_config::InferenceConfig;
use parish_types::ParishError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::client_base::ClientBase;
use crate::openai_client::{GenerateParams, ResponseFormat};
use crate::rate_limit::InferenceRateLimiter;

pub use parish_config::{ServiceTier, ThinkingLevel};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub input_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub thought_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderMetadata {
    pub provider: String,
    pub api_mode: String,
    pub model: String,
    pub interaction_id: Option<String>,
    pub http_status: Option<u16>,
    pub terminal_status: Option<String>,
    pub requested_service_tier: Option<ServiceTier>,
    pub effective_service_tier: Option<String>,
    pub retry_count: u32,
    pub usage: ProviderUsage,
    pub ttft_ms: Option<u64>,
    pub duration_ms: u64,
    pub stream_chunks: u64,
}

impl ProviderMetadata {
    pub fn unavailable(model: &str) -> Self {
        Self {
            provider: "unknown".to_string(),
            api_mode: "legacy".to_string(),
            model: model.to_string(),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub text: String,
    pub metadata: ProviderMetadata,
}

#[derive(Debug, Clone)]
pub struct ProviderCallError {
    pub message: String,
    pub partial_text: String,
    pub metadata: Box<ProviderMetadata>,
}

impl std::fmt::Display for ProviderCallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProviderCallError {}

impl From<ProviderCallError> for ParishError {
    fn from(value: ProviderCallError) -> Self {
        ParishError::Inference(value.message)
    }
}

#[derive(Clone)]
pub struct GoogleClient {
    base: ClientBase,
}

impl GoogleClient {
    pub fn new(base_url: &str, api_key: Option<&str>) -> Self {
        Self::new_with_config(base_url, api_key, &InferenceConfig::default())
    }

    pub fn new_with_config(
        base_url: &str,
        api_key: Option<&str>,
        config: &InferenceConfig,
    ) -> Self {
        Self {
            base: ClientBase::new(
                base_url,
                api_key,
                "Google Interactions",
                "Google Interactions streaming",
                config,
            ),
        }
    }

    pub fn with_rate_limit(self, limiter: InferenceRateLimiter) -> Self {
        Self {
            base: self.base.with_rate_limit(limiter),
        }
    }

    pub fn maybe_with_rate_limit(self, limiter: Option<InferenceRateLimiter>) -> Self {
        Self {
            base: self.base.maybe_with_rate_limit(limiter),
        }
    }

    pub fn has_rate_limiter(&self) -> bool {
        self.base.has_rate_limiter()
    }

    pub fn base_url(&self) -> &str {
        self.base.base_url()
    }

    fn interactions_url(&self) -> String {
        format!(
            "{}/v1/interactions",
            self.base.base_url.trim_end_matches('/')
        )
    }

    fn request_body(
        model: &str,
        prompt: &str,
        system: Option<&str>,
        stream: bool,
        response_format: Option<ResponseFormat>,
        params: &GenerateParams,
    ) -> Value {
        let mut body = json!({
            "model": model,
            "input": prompt,
            "stream": stream,
            "store": false,
            "generation_config": {
                "thinking_level": params.thinking_level.unwrap_or_default(),
            }
        });
        // Standard is Google's default and must be requested by omitting the
        // field. Priority is sent only for an explicit override.
        if params.service_tier == Some(ServiceTier::Priority) {
            body["service_tier"] = json!(ServiceTier::Priority);
        }
        if let Some(system) = system {
            body["system_instruction"] = Value::String(system.to_string());
        }
        body["generation_config"]["max_output_tokens"] = json!(effective_google_cap(params));
        if let Some(format) = response_format {
            body["response_format"] = match format {
                ResponseFormat::JsonObject => json!({
                    "type": "text",
                    "mime_type": "application/json"
                }),
                ResponseFormat::JsonSchema { json_schema } => json!({
                    "type": "text",
                    "mime_type": "application/json",
                    "schema": json_schema.schema
                }),
            };
        }
        body
    }

    fn request(&self, client: &reqwest::Client, body: &Value) -> reqwest::RequestBuilder {
        let mut request = client.post(self.interactions_url()).json(body);
        if let Some(key) = self.base.api_key.as_deref() {
            request = request.header("x-goog-api-key", key);
        }
        request
    }

    pub async fn generate_detailed_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<GenerationResult, ProviderCallError> {
        self.base.acquire_slot().await;
        let started = Instant::now();
        let body = Self::request_body(model, prompt, system, false, response_format, &params);
        let (response, retries) = self
            .send_retryable(&self.base.client, &body, model, &params, started)
            .await?;
        let status = response.status().as_u16();
        let effective_tier = header_string(response.headers(), "x-gemini-service-tier");
        let value: Value = response.json().await.map_err(|error| ProviderCallError {
            message: format!("Google Interactions response JSON: {error}"),
            partial_text: String::new(),
            metadata: Box::new(base_metadata(
                model,
                &params,
                started,
                retries,
                Some(status),
                effective_tier.clone(),
            )),
        })?;
        finish_non_streaming(
            value,
            model,
            &params,
            started,
            retries,
            status,
            effective_tier,
        )
    }

    pub async fn generate_stream_detailed_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<GenerationResult, ProviderCallError> {
        self.base.acquire_slot().await;
        let started = Instant::now();
        let body = Self::request_body(model, prompt, system, true, response_format, &params);
        let (response, retries) = self
            .send_retryable(&self.base.streaming_client, &body, model, &params, started)
            .await?;
        let status = response.status().as_u16();
        let effective_tier = header_string(response.headers(), "x-gemini-service-tier");
        read_google_sse(
            response,
            token_tx,
            GoogleStreamContext {
                model,
                params: &params,
                started,
                retries,
                http_status: status,
                effective_tier,
            },
        )
        .await
    }

    async fn send_retryable(
        &self,
        client: &reqwest::Client,
        body: &Value,
        model: &str,
        params: &GenerateParams,
        started: Instant,
    ) -> Result<(reqwest::Response, u32), ProviderCallError> {
        let mut retries = 0;
        let mut transient_retries = 0;
        loop {
            let response =
                self.request(client, body)
                    .send()
                    .await
                    .map_err(|error| ProviderCallError {
                        message: format!("Google Interactions network error: {error}"),
                        partial_text: String::new(),
                        metadata: Box::new(base_metadata(
                            model, params, started, retries, None, None,
                        )),
                    })?;
            let status = response.status();
            if status.is_success() {
                return Ok((response, retries));
            }
            let retry_after =
                crate::retry::retry_after_delay(response.headers(), SystemTime::now());
            let effective_tier = header_string(response.headers(), "x-gemini-service-tier");
            let code = status.as_u16();
            let text = response.text().await.unwrap_or_default();
            if matches!(code, 429 | 503) && transient_retries < 2 {
                transient_retries += 1;
                retries += 1;
                tokio::time::sleep(retry_after.unwrap_or_else(|| {
                    crate::retry::jittered(crate::retry::base_backoff(transient_retries - 1))
                }))
                .await;
                continue;
            }
            return Err(ProviderCallError {
                message: format!("Google Interactions HTTP {code}: {}", excerpt(&text)),
                partial_text: String::new(),
                metadata: Box::new(base_metadata(
                    model,
                    params,
                    started,
                    retries,
                    Some(code),
                    effective_tier,
                )),
            });
        }
    }

    pub async fn generate(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.generate_detailed_with_format(model, prompt, system, None, params)
            .await
            .map(|result| result.text)
            .map_err(Into::into)
    }

    pub async fn generate_stream(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.generate_stream_detailed_with_format(model, prompt, system, token_tx, None, params)
            .await
            .map(|result| result.text)
            .map_err(Into::into)
    }
}

fn finish_non_streaming(
    value: Value,
    model: &str,
    params: &GenerateParams,
    started: Instant,
    retries: u32,
    http_status: u16,
    effective_tier: Option<String>,
) -> Result<GenerationResult, ProviderCallError> {
    let mut metadata = base_metadata(
        model,
        params,
        started,
        retries,
        Some(http_status),
        effective_tier,
    );
    hydrate_metadata(&mut metadata, &value);
    let text = extract_model_output(&value);
    validate_terminal(text, metadata)
}

struct GoogleStreamContext<'a> {
    model: &'a str,
    params: &'a GenerateParams,
    started: Instant,
    retries: u32,
    http_status: u16,
    effective_tier: Option<String>,
}

async fn read_google_sse(
    mut response: reqwest::Response,
    token_tx: mpsc::Sender<String>,
    context: GoogleStreamContext<'_>,
) -> Result<GenerationResult, ProviderCallError> {
    let GoogleStreamContext {
        model,
        params,
        started,
        retries,
        http_status,
        effective_tier,
    } = context;
    let mut metadata = base_metadata(
        model,
        params,
        started,
        retries,
        Some(http_status),
        effective_tier,
    );
    let mut text = String::new();
    let mut buffer = String::new();
    let mut event_data = Vec::<String>::new();
    let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();
    let mut active_model_output: Option<Option<u64>> = None;
    let mut saw_terminal = false;

    while let Some(chunk) = response.chunk().await.map_err(|error| ProviderCallError {
        message: format!("Google Interactions stream error: {error}"),
        partial_text: text.clone(),
        metadata: Box::new(metadata.clone()),
    })? {
        buffer.push_str(&decoder.push(&chunk));
        while let Some(pos) = buffer.find('\n') {
            let line: String = buffer.drain(..=pos).collect();
            if let Some(value) = collect_sse_line(&line, &mut event_data)
                .map_err(|error| malformed_sse_error(error, &text, &metadata))?
            {
                process_event(
                    value,
                    &token_tx,
                    &mut text,
                    &mut metadata,
                    started,
                    &mut active_model_output,
                    &mut saw_terminal,
                )
                .await?;
            }
        }
    }
    buffer.push_str(&decoder.flush());
    if !buffer.is_empty()
        && let Some(value) = collect_sse_line(&buffer, &mut event_data)
            .map_err(|error| malformed_sse_error(error, &text, &metadata))?
    {
        process_event(
            value,
            &token_tx,
            &mut text,
            &mut metadata,
            started,
            &mut active_model_output,
            &mut saw_terminal,
        )
        .await?;
    }
    if !event_data.is_empty()
        && let Some(value) = parse_sse_data(&event_data.join("\n"))
            .map_err(|error| malformed_sse_error(error, &text, &metadata))?
    {
        process_event(
            value,
            &token_tx,
            &mut text,
            &mut metadata,
            started,
            &mut active_model_output,
            &mut saw_terminal,
        )
        .await?;
    }
    metadata.duration_ms = started.elapsed().as_millis() as u64;
    if !saw_terminal {
        return Err(ProviderCallError {
            message: "Google Interactions stream ended before interaction.completed".to_string(),
            partial_text: text,
            metadata: Box::new(metadata),
        });
    }
    validate_terminal(text, metadata)
}

fn collect_sse_line(
    line: &str,
    data: &mut Vec<String>,
) -> Result<Option<Value>, serde_json::Error> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.is_empty() {
        if data.is_empty() {
            return Ok(None);
        }
        let joined = data.join("\n");
        data.clear();
        return parse_sse_data(&joined);
    }
    if line.starts_with(':') || line.starts_with("event:") || line.starts_with("id:") {
        return Ok(None);
    }
    if let Some(value) = line.strip_prefix("data:") {
        data.push(value.trim_start().to_string());
    }
    Ok(None)
}

fn parse_sse_data(data: &str) -> Result<Option<Value>, serde_json::Error> {
    // The live v1 Interactions stream emits the conventional SSE sentinel
    // after `interaction.completed`. It is framing, not a JSON event. If it
    // arrives without a completion event the caller still fails its explicit
    // `saw_terminal` check, so ignoring it cannot turn a truncated stream into
    // success.
    if data.trim() == "[DONE]" {
        Ok(None)
    } else {
        serde_json::from_str(data).map(Some)
    }
}

fn malformed_sse_error(
    error: serde_json::Error,
    partial_text: &str,
    metadata: &ProviderMetadata,
) -> ProviderCallError {
    ProviderCallError {
        message: format!("Google Interactions malformed SSE event: {error}"),
        partial_text: partial_text.to_string(),
        metadata: Box::new(metadata.clone()),
    }
}

async fn process_event(
    value: Value,
    token_tx: &mpsc::Sender<String>,
    accumulated: &mut String,
    metadata: &mut ProviderMetadata,
    started: Instant,
    active_model_output: &mut Option<Option<u64>>,
    saw_terminal: &mut bool,
) -> Result<(), ProviderCallError> {
    hydrate_metadata(metadata, &value);
    let event_type = value
        .get("event_type")
        .or_else(|| value.get("type"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(event_type, "error" | "interaction.error") {
        metadata.terminal_status = Some("failed".to_string());
        return Err(ProviderCallError {
            message: format!(
                "Google Interactions error event: {}",
                provider_error_message(&value)
            ),
            partial_text: accumulated.clone(),
            metadata: Box::new(metadata.clone()),
        });
    }
    if let Some(status) = event_type.strip_prefix("interaction.")
        && matches!(
            status,
            "completed"
                | "failed"
                | "cancelled"
                | "incomplete"
                | "budget_exceeded"
                | "requires_action"
                | "refusal"
        )
    {
        *saw_terminal = true;
        if metadata.terminal_status.is_none() {
            metadata.terminal_status = Some(status.to_string());
        }
    }
    let visible = if event_type == "step.start" {
        let is_model_output = value
            .get("step")
            .and_then(|step| step.get("type"))
            .and_then(Value::as_str)
            == Some("model_output");
        *active_model_output = is_model_output.then(|| event_step_index(&value));
        value
            .get("step")
            .filter(|_| is_model_output)
            .map(extract_content)
            .unwrap_or_default()
    } else if event_type == "step.delta" {
        let delta = value.get("delta").unwrap_or(&Value::Null);
        let matching_step = active_model_output.is_some_and(|active_index| {
            let delta_index = event_step_index(&value);
            active_index.is_none() || delta_index.is_none() || active_index == delta_index
        });
        match (matching_step, delta.get("type").and_then(Value::as_str)) {
            (true, Some("text" | "text_delta")) => delta
                .get("text")
                .or_else(|| delta.get("delta"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            _ => String::new(),
        }
    } else if event_type == "step.stop" || event_type == "step.completed" {
        let stopped_index = event_step_index(&value);
        if active_model_output.is_some_and(|active_index| {
            active_index.is_none() || stopped_index.is_none() || active_index == stopped_index
        }) {
            *active_model_output = None;
        }
        String::new()
    } else {
        String::new()
    };
    if !visible.is_empty() {
        if metadata.ttft_ms.is_none() {
            metadata.ttft_ms = Some(started.elapsed().as_millis() as u64);
        }
        metadata.stream_chunks += 1;
        token_tx
            .send(visible.clone())
            .await
            .map_err(|_| ProviderCallError {
                message: "Google Interactions output consumer disconnected".to_string(),
                partial_text: accumulated.clone(),
                metadata: Box::new(metadata.clone()),
            })?;
        accumulated.push_str(&visible);
    }
    Ok(())
}

fn event_step_index(value: &Value) -> Option<u64> {
    value
        .get("step_index")
        .or_else(|| value.get("index"))
        .or_else(|| value.get("step").and_then(|step| step.get("index")))
        .or_else(|| value.get("delta").and_then(|delta| delta.get("step_index")))
        .and_then(Value::as_u64)
}

fn provider_error_message(value: &Value) -> String {
    value
        .get("error")
        .and_then(|error| error.get("message").or_else(|| error.get("detail")))
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("unknown provider error")
        .chars()
        .take(500)
        .collect()
}

fn extract_model_output(value: &Value) -> String {
    value
        .get("steps")
        .and_then(Value::as_array)
        .map(|steps| {
            steps
                .iter()
                .filter(|step| step.get("type").and_then(Value::as_str) == Some("model_output"))
                .map(extract_content)
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn extract_content(step: &Value) -> String {
    step.get("content")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| item.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|item| item.get("text").and_then(Value::as_str))
                .collect::<String>()
        })
        .unwrap_or_default()
}

fn hydrate_metadata(metadata: &mut ProviderMetadata, value: &Value) {
    if let Some(id) = value
        .get("interaction_id")
        .or_else(|| value.get("id"))
        .or_else(|| {
            value
                .get("interaction")
                .and_then(|interaction| interaction.get("interaction_id"))
        })
        .or_else(|| {
            value
                .get("interaction")
                .and_then(|interaction| interaction.get("id"))
        })
        .and_then(Value::as_str)
    {
        metadata.interaction_id = Some(id.to_string());
    }
    if let Some(status) = value.get("status").and_then(Value::as_str).or_else(|| {
        value
            .get("interaction")
            .and_then(|v| v.get("status"))
            .and_then(Value::as_str)
    }) {
        metadata.terminal_status = Some(status.to_string());
    }
    if let Some(tier) = value
        .get("service_tier")
        .or_else(|| {
            value
                .get("interaction")
                .and_then(|interaction| interaction.get("service_tier"))
        })
        .and_then(Value::as_str)
    {
        metadata.effective_service_tier = Some(tier.to_string());
    }
    let usage = value
        .get("usage")
        .or_else(|| value.get("metadata").and_then(|v| v.get("total_usage")))
        .or_else(|| value.get("interaction").and_then(|v| v.get("usage")))
        .or_else(|| {
            value
                .get("interaction")
                .and_then(|v| v.get("metadata"))
                .and_then(|v| v.get("total_usage"))
        });
    if let Some(usage) = usage {
        metadata.usage.input_tokens = u64_field(usage, "total_input_tokens");
        metadata.usage.cached_tokens = u64_field(usage, "total_cached_tokens");
        metadata.usage.thought_tokens = u64_field(usage, "total_thought_tokens");
        metadata.usage.output_tokens = u64_field(usage, "total_output_tokens");
        metadata.usage.total_tokens = u64_field(usage, "total_tokens");
    }
}

fn validate_terminal(
    text: String,
    mut metadata: ProviderMetadata,
) -> Result<GenerationResult, ProviderCallError> {
    if metadata.terminal_status.as_deref() != Some("completed") {
        let status = metadata
            .terminal_status
            .clone()
            .unwrap_or_else(|| "missing completion".to_string());
        return Err(ProviderCallError {
            message: format!("Google Interactions did not complete: {status}"),
            partial_text: text,
            metadata: Box::new(metadata),
        });
    }
    if text.trim().is_empty() {
        return Err(ProviderCallError {
            message: "Google Interactions completed with empty model output".to_string(),
            partial_text: text,
            metadata: Box::new(metadata),
        });
    }
    metadata.duration_ms = metadata.duration_ms.max(1);
    Ok(GenerationResult { text, metadata })
}

fn base_metadata(
    model: &str,
    params: &GenerateParams,
    started: Instant,
    retries: u32,
    status: Option<u16>,
    tier: Option<String>,
) -> ProviderMetadata {
    ProviderMetadata {
        provider: "google".to_string(),
        api_mode: "google-interactions-v1".to_string(),
        model: model.to_string(),
        http_status: status,
        requested_service_tier: Some(params.service_tier.unwrap_or_default()),
        effective_service_tier: tier,
        retry_count: retries,
        duration_ms: started.elapsed().as_millis() as u64,
        ..ProviderMetadata::default()
    }
}

fn u64_field(value: &Value, key: &str) -> Option<u64> {
    value.get(key).and_then(Value::as_u64)
}
fn effective_google_cap(params: &GenerateParams) -> u32 {
    let thinking = params.thinking_level.unwrap_or_default();
    params
        .max_tokens
        .unwrap_or(match thinking {
            ThinkingLevel::Minimal | ThinkingLevel::Low => 1_024,
            ThinkingLevel::Medium => 4_096,
            ThinkingLevel::High => 8_192,
        })
        .clamp(1, 65_536)
}
fn header_string(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(ToOwned::to_owned)
}
fn excerpt(text: &str) -> String {
    text.chars().take(500).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn request_is_stateless_native_and_omits_sampling() {
        let body = GoogleClient::request_body(
            "gemini-3.6-flash",
            "hello",
            Some("system"),
            false,
            Some(ResponseFormat::JsonObject),
            &GenerateParams {
                max_tokens: Some(1024),
                thinking_level: Some(ThinkingLevel::Low),
                service_tier: Some(ServiceTier::Standard),
                ..GenerateParams::default()
            },
        );
        assert_eq!(body["store"], false);
        assert_eq!(body["generation_config"]["thinking_level"], "low");
        assert_eq!(body["generation_config"]["max_output_tokens"], 1024);
        assert_eq!(body["response_format"]["mime_type"], "application/json");
        assert!(body.get("temperature").is_none());
        assert!(body.get("service_tier").is_none());
        assert!(body.get("previous_interaction_id").is_none());

        let priority = GoogleClient::request_body(
            "gemini-3.6-flash",
            "hello",
            None,
            false,
            None,
            &GenerateParams {
                service_tier: Some(ServiceTier::Priority),
                ..GenerateParams::default()
            },
        );
        assert_eq!(priority["service_tier"], "priority");
    }

    #[test]
    fn stable_prefix_is_byte_identical_while_turn_suffix_changes() {
        let params = GenerateParams::default();
        let first = GoogleClient::request_body(
            "gemini-3.6-flash",
            "dynamic turn one",
            Some("stable identity and world grounding"),
            false,
            None,
            &params,
        );
        let second = GoogleClient::request_body(
            "gemini-3.6-flash",
            "dynamic turn two",
            Some("stable identity and world grounding"),
            false,
            None,
            &params,
        );
        assert_eq!(first["system_instruction"], second["system_instruction"]);
        assert_ne!(first["input"], second["input"]);
        assert_eq!(first["store"], false);
        assert_eq!(second["store"], false);
    }

    #[test]
    fn non_streaming_extracts_usage_and_visible_output() {
        let value = json!({
            "id":"int_123", "status":"completed",
            "steps":[{"type":"thought","content":[{"type":"text","text":"secret"}]},{"type":"model_output","content":[{"type":"text","text":"hello"}]}],
            "usage":{"total_input_tokens":100,"total_cached_tokens":80,"total_thought_tokens":12,"total_output_tokens":2,"total_tokens":114}
        });
        let result = finish_non_streaming(
            value,
            "gemini-3.6-flash",
            &GenerateParams::default(),
            Instant::now(),
            0,
            200,
            Some("standard".into()),
        )
        .unwrap();
        assert_eq!(result.text, "hello");
        assert_eq!(result.metadata.usage.cached_tokens, Some(80));
        assert!(!result.text.contains("secret"));
    }

    #[test]
    fn explicit_standard_serializes() {
        assert_eq!(
            serde_json::to_value(ServiceTier::Standard).unwrap(),
            "standard"
        );
    }

    #[test]
    fn explicit_caps_are_not_silently_floored() {
        let params = GenerateParams {
            max_tokens: Some(200),
            thinking_level: Some(ThinkingLevel::Medium),
            ..GenerateParams::default()
        };
        assert_eq!(effective_google_cap(&params), 200);
    }

    #[test]
    fn incomplete_and_budget_exceeded_are_errors_with_usage() {
        for status in [
            "incomplete",
            "budget_exceeded",
            "cancelled",
            "requires_action",
        ] {
            let value = json!({
                "id": "int_bad",
                "status": status,
                "steps": [{"type":"model_output","content":[{"type":"text","text":"partial"}]}],
                "usage": {"total_input_tokens": 10, "total_output_tokens": 2}
            });
            let error = finish_non_streaming(
                value,
                "gemini-3.6-flash",
                &GenerateParams::default(),
                Instant::now(),
                0,
                200,
                None,
            )
            .expect_err("non-completed terminal status must fail");
            assert_eq!(error.metadata.terminal_status.as_deref(), Some(status));
            assert_eq!(error.metadata.usage.input_tokens, Some(10));
            assert_eq!(error.partial_text, "partial");
        }
    }

    #[tokio::test]
    async fn native_request_uses_google_auth_and_reports_cache_usage() {
        let server = MockServer::start().await;
        let expected = GoogleClient::request_body(
            "gemini-3.6-flash",
            "hello",
            Some("system"),
            false,
            None,
            &GenerateParams::default(),
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .and(header("x-goog-api-key", "test-key"))
            .and(body_json(expected))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "int_test",
                "status": "completed",
                "steps": [{"type":"model_output","content":[{"type":"text","text":"Hello"}]}],
                "usage": {"total_input_tokens": 9000, "total_cached_tokens": 8000, "total_output_tokens": 2, "total_thought_tokens": 5, "total_tokens": 9007}
            })))
            .mount(&server)
            .await;

        let result = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                Some("system"),
                None,
                GenerateParams::default(),
            )
            .await
            .unwrap();
        assert_eq!(result.text, "Hello");
        assert_eq!(result.metadata.interaction_id.as_deref(), Some("int_test"));
        assert_eq!(result.metadata.usage.cached_tokens, Some(8000));
    }

    #[tokio::test]
    async fn retries_429_with_retry_after_then_retains_retry_count() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "0"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":"retry-ok", "status":"completed",
                "steps":[{"type":"model_output","content":[{"type":"text","text":"ok"}]}],
                "usage":{"total_input_tokens":1,"total_output_tokens":1}
            })))
            .mount(&server)
            .await;
        let result = GoogleClient::new(&format!("{}/v1", server.uri()), Some("key"))
            .generate_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                None,
                GenerateParams::default(),
            )
            .await
            .expect("429 should retry before visible output");
        assert_eq!(result.metadata.retry_count, 1);
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn exhausts_two_503_retries_and_reports_terminal_http_metadata() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "0"))
            .mount(&server)
            .await;
        let error = GoogleClient::new(&format!("{}/v1", server.uri()), Some("key"))
            .generate_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("retry exhaustion must surface");
        assert_eq!(error.metadata.http_status, Some(503));
        assert_eq!(error.metadata.retry_count, 2);
        assert_eq!(server.received_requests().await.unwrap().len(), 3);
    }

    #[test]
    fn refusal_is_a_first_class_terminal_status() {
        let value = json!({"id":"refused", "status":"refusal", "steps":[]});
        let error = finish_non_streaming(
            value,
            "gemini-3.6-flash",
            &GenerateParams::default(),
            Instant::now(),
            0,
            200,
            None,
        )
        .expect_err("refusal is not a completion");
        assert_eq!(error.metadata.terminal_status.as_deref(), Some("refusal"));
    }

    #[tokio::test]
    async fn unsupported_explicit_priority_is_reported_without_fallback() {
        let server = MockServer::start().await;
        let priority_params = GenerateParams {
            service_tier: Some(ServiceTier::Priority),
            ..GenerateParams::default()
        };
        let priority_body = GoogleClient::request_body(
            "gemini-3.6-flash",
            "hello",
            None,
            false,
            None,
            &priority_params,
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .and(body_json(priority_body))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message": "The value 'priority' is not supported for 'service_tier'."}
            })))
            .expect(1)
            .mount(&server)
            .await;

        let client = GoogleClient::new(&server.uri(), Some("test-key"));
        let error = client
            .generate_detailed_with_format("gemini-3.6-flash", "hello", None, None, priority_params)
            .await
            .unwrap_err();
        assert!(error.message.contains("HTTP 400"));
        assert!(error.message.contains("service_tier"));
        assert_eq!(error.metadata.retry_count, 0);
        assert_eq!(
            error.metadata.requested_service_tier,
            Some(ServiceTier::Priority)
        );
    }

    #[tokio::test]
    async fn streaming_keeps_initial_step_text_and_ignores_thoughts() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"event_type\":\"interaction.created\",\"interaction_id\":\"int_stream\"}\n\n",
            "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"thought\",\"content\":[{\"type\":\"text\",\"text\":\"secret\"}]}}\n\n",
            "data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"text\",\"text\":\"secret-too\"}}\n\n",
            "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"Hel\"}]}}\n\n",
            "data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"text\",\"text\":\"lo\"}}\n\n",
            "data: {\"event_type\":\"interaction.completed\",\"interaction_id\":\"int_stream\",\"status\":\"completed\",\"metadata\":{\"total_usage\":{\"total_input_tokens\":10,\"total_output_tokens\":2,\"total_thought_tokens\":3,\"total_tokens\":15}}}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, mut rx) = mpsc::channel(8);
        let result = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .unwrap();
        let mut streamed = String::new();
        while let Some(chunk) = rx.recv().await {
            streamed.push_str(&chunk);
        }
        assert_eq!(streamed, "Hello");
        assert_eq!(result.text, "Hello");
        assert!(!result.text.contains("secret"));
        assert_eq!(result.metadata.stream_chunks, 2);
        assert_eq!(result.metadata.usage.thought_tokens, Some(3));
    }

    #[tokio::test]
    async fn streaming_requires_interaction_completed_even_after_visible_text() {
        let server = MockServer::start().await;
        let sse = "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, _rx) = mpsc::channel(8);
        let error = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("missing completion event must fail");
        assert_eq!(error.partial_text, "partial");
        assert!(error.message.contains("interaction.completed"));
    }

    #[tokio::test]
    async fn done_sentinel_cannot_replace_required_completion_event() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, _rx) = mpsc::channel(8);
        let error = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("SSE sentinel is not a terminal interaction event");
        assert!(error.message.contains("interaction.completed"));
        assert_eq!(error.partial_text, "partial");
    }

    #[tokio::test]
    async fn streaming_rejects_malformed_sse_instead_of_dropping_it() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}\n\n",
            "data: {not-json}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, _rx) = mpsc::channel(8);
        let error = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("malformed SSE must fail");
        assert_eq!(error.partial_text, "partial");
        assert!(error.message.contains("malformed SSE"));
    }

    #[tokio::test]
    async fn streaming_ignores_deltas_from_a_different_step_index() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"event_type\":\"step.start\",\"step_index\":4,\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"A\"}]}}\n\n",
            "data: {\"event_type\":\"step.delta\",\"step_index\":5,\"delta\":{\"type\":\"text\",\"text\":\"secret\"}}\n\n",
            "data: {\"event_type\":\"step.delta\",\"step_index\":4,\"delta\":{\"type\":\"text\",\"text\":\"B\"}}\n\n",
            "data: {\"event_type\":\"interaction.completed\",\"status\":\"completed\",\"service_tier\":\"standard\",\"usage\":{\"total_output_tokens\":2}}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, mut rx) = mpsc::channel(8);
        let result = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .unwrap();
        let mut streamed = String::new();
        while let Some(chunk) = rx.recv().await {
            streamed.push_str(&chunk);
        }
        assert_eq!(streamed, "AB");
        assert_eq!(result.text, "AB");
        assert_eq!(
            result.metadata.effective_service_tier.as_deref(),
            Some("standard")
        );
    }

    #[tokio::test]
    async fn streaming_reports_failed_terminal_event() {
        let server = MockServer::start().await;
        let sse = "data: {\"event_type\":\"interaction.failed\"}\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, _rx) = mpsc::channel(8);
        let error = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("failed terminal event must fail");
        assert_eq!(error.metadata.terminal_status.as_deref(), Some("failed"));
        assert!(error.message.contains("failed"));
    }

    #[tokio::test]
    async fn streaming_surfaces_explicit_error_event_with_partial_output() {
        let server = MockServer::start().await;
        let sse = concat!(
            "data: {\"event_type\":\"step.start\",\"step\":{\"type\":\"model_output\",\"content\":[{\"type\":\"text\",\"text\":\"partial\"}]}}\n\n",
            "data: {\"event_type\":\"error\",\"error\":{\"message\":\"quota exhausted\"}}\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(sse),
            )
            .mount(&server)
            .await;
        let (tx, _rx) = mpsc::channel(8);
        let error = GoogleClient::new(&server.uri(), Some("test-key"))
            .generate_stream_detailed_with_format(
                "gemini-3.6-flash",
                "hello",
                None,
                tx,
                None,
                GenerateParams::default(),
            )
            .await
            .expect_err("explicit error event must fail immediately");
        assert_eq!(error.partial_text, "partial");
        assert!(error.message.contains("quota exhausted"));
        assert_eq!(error.metadata.terminal_status.as_deref(), Some("failed"));
    }

    #[test]
    fn nested_completion_metadata_is_retained() {
        let mut metadata = ProviderMetadata::default();
        hydrate_metadata(
            &mut metadata,
            &json!({
                "interaction": {
                    "id": "int_nested",
                    "status": "completed",
                    "service_tier": "standard",
                    "metadata": {
                        "total_usage": {
                            "total_input_tokens": 40,
                            "total_cached_tokens": 20,
                            "total_output_tokens": 3,
                            "total_thought_tokens": 7,
                            "total_tokens": 50
                        }
                    }
                }
            }),
        );
        assert_eq!(metadata.interaction_id.as_deref(), Some("int_nested"));
        assert_eq!(metadata.terminal_status.as_deref(), Some("completed"));
        assert_eq!(metadata.effective_service_tier.as_deref(), Some("standard"));
        assert_eq!(metadata.usage.cached_tokens, Some(20));
    }
}
