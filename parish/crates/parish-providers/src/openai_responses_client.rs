//! Native OpenAI Responses API transport.
//!
//! This is intentionally separate from the OpenAI-compatible Chat
//! Completions client: the request envelope, structured-output shape,
//! terminal response, and SSE event vocabulary are different protocols.

use parish_config::{InferenceConfig, ReasoningEffort};
use parish_types::ParishError;
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::client_base::ClientBase;
use crate::openai_client::{GenerateParams, ResponseFormat};
use crate::rate_limit::InferenceRateLimiter;
use crate::{TOKEN_CHANNEL_CAPACITY, strip_json_fence};

#[derive(Clone)]
pub struct OpenAiResponsesClient {
    base: ClientBase,
}

impl OpenAiResponsesClient {
    pub fn new_with_api_prefix(
        base_url: &str,
        api_key: Option<&str>,
        config: &InferenceConfig,
    ) -> Self {
        Self {
            base: ClientBase::new_preserving_path(
                base_url,
                api_key,
                "OpenAI Responses client",
                "OpenAI Responses streaming client",
                config,
            ),
        }
    }

    pub fn has_rate_limiter(&self) -> bool {
        self.base.has_rate_limiter()
    }

    pub fn maybe_with_rate_limit(mut self, limiter: Option<InferenceRateLimiter>) -> Self {
        self.base = self.base.maybe_with_rate_limit(limiter);
        self
    }

    fn url(&self) -> String {
        format!("{}/responses", self.base.base_url.trim_end_matches('/'))
    }

    fn request_body(
        model: &str,
        prompt: &str,
        system: Option<&str>,
        stream: bool,
        response_format: Option<ResponseFormat>,
        params: &GenerateParams,
    ) -> Result<Value, ParishError> {
        let mut body = json!({
            "model": model,
            "input": prompt,
            "stream": stream,
            "store": false,
        });
        if let Some(instructions) = system.filter(|value| !value.trim().is_empty()) {
            body["instructions"] = json!(instructions);
        }
        if let Some(max_output_tokens) = params.max_tokens {
            body["max_output_tokens"] = json!(max_output_tokens);
        }
        if let Some(temperature) = params.temperature {
            body["temperature"] = json!(temperature);
        }
        if let Some(frequency_penalty) = params.frequency_penalty {
            body["frequency_penalty"] = json!(frequency_penalty);
        }
        if let Some(effort) = params.reasoning_effort.map(responses_effort).transpose()? {
            body["reasoning"] = json!({"effort": effort});
        }
        if let Some(format) = response_format {
            body["text"]["format"] = match format {
                ResponseFormat::JsonObject => json!({"type": "json_object"}),
                ResponseFormat::JsonSchema { json_schema } => json!({
                    "type": "json_schema",
                    "name": json_schema.name,
                    "schema": json_schema.schema,
                    "strict": true,
                }),
            };
        }
        Ok(body)
    }

    fn apply_auth(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.base.api_key {
            Some(key) => request.header("Authorization", format!("Bearer {key}")),
            None => request,
        }
    }

    pub async fn generate_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.base.acquire_slot().await;
        let body = Self::request_body(model, prompt, system, false, response_format, &params)?;
        let response = crate::retry::send_with_retry("openai-responses", || {
            self.apply_auth(self.base.client.post(self.url()).json(&body))
                .send()
        })
        .await?
        .error_for_status()
        .map_err(|error| ParishError::Network(error.to_string()))?;
        let value: Value = response
            .json()
            .await
            .map_err(|error| ParishError::Network(error.to_string()))?;
        extract_completed_text(&value).map(|text| strip_json_fence(&text).to_string())
    }

    pub async fn generate_stream_with_format(
        &self,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        token_tx: mpsc::Sender<String>,
        response_format: Option<ResponseFormat>,
        params: GenerateParams,
    ) -> Result<String, ParishError> {
        self.base.acquire_slot().await;
        let body = Self::request_body(model, prompt, system, true, response_format, &params)?;
        let mut response = crate::retry::send_with_retry("openai-responses", || {
            self.apply_auth(self.base.streaming_client.post(self.url()).json(&body))
                .send()
        })
        .await?
        .error_for_status()
        .map_err(|error| ParishError::Network(error.to_string()))?;
        let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();
        let mut buffer = String::new();
        let mut accumulated = String::new();
        let mut completed = false;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| ParishError::Network(error.to_string()))?
        {
            buffer.push_str(&decoder.push(&chunk));
            while let Some(newline) = buffer.find('\n') {
                let line: String = buffer.drain(..=newline).collect();
                process_responses_sse_line(&line, &token_tx, &mut accumulated, &mut completed)?;
            }
        }
        buffer.push_str(&decoder.flush());
        if !buffer.trim().is_empty() {
            process_responses_sse_line(&buffer, &token_tx, &mut accumulated, &mut completed)?;
        }
        if !completed || accumulated.is_empty() {
            return Err(ParishError::Inference(
                "OpenAI Responses stream ended without a completed non-empty response".into(),
            ));
        }
        Ok(strip_json_fence(&accumulated).to_string())
    }
}

fn responses_effort(effort: ReasoningEffort) -> Result<&'static str, ParishError> {
    Ok(match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Minimal => "minimal",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Xhigh => "xhigh",
        ReasoningEffort::Max => {
            return Err(ParishError::Inference(
                "OpenAI Responses does not support reasoning effort max".into(),
            ));
        }
    })
}

fn extract_completed_text(value: &Value) -> Result<String, ParishError> {
    if value.get("status").and_then(Value::as_str) != Some("completed") {
        return Err(ParishError::Inference(format!(
            "OpenAI Responses response was not completed (status={:?})",
            value.get("status").and_then(Value::as_str)
        )));
    }
    let output = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| ParishError::Inference("OpenAI Responses output was missing".into()))?;
    let mut text = String::new();
    let mut message_count = 0usize;
    let mut text_count = 0usize;
    for item in output {
        match item.get("type").and_then(Value::as_str) {
            // Reasoning is hidden model work. It may be retained for provider
            // accounting but is never presented or treated as response text.
            Some("reasoning") => continue,
            Some("message") => message_count += 1,
            _ => {
                return Err(ParishError::Inference(
                    "OpenAI Responses output contained a forbidden tool or unknown item".into(),
                ));
            }
        }
        if item
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role != "assistant")
        {
            return Err(ParishError::Inference(
                "OpenAI Responses output message was not from the assistant".into(),
            ));
        }
        for content in item
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if content.get("type").and_then(Value::as_str) != Some("output_text") {
                return Err(ParishError::Inference(
                    "OpenAI Responses message contained forbidden non-text content".into(),
                ));
            }
            text_count += 1;
            text.push_str(content.get("text").and_then(Value::as_str).ok_or_else(|| {
                ParishError::Inference("OpenAI Responses output_text omitted text".into())
            })?);
        }
    }
    if message_count != 1 || text_count == 0 || text.trim().is_empty() {
        return Err(ParishError::Inference(
            "OpenAI Responses response must contain one assistant message with non-empty output_text".into(),
        ));
    }
    Ok(text)
}

fn process_responses_sse_line(
    line: &str,
    token_tx: &mpsc::Sender<String>,
    accumulated: &mut String,
    completed: &mut bool,
) -> Result<(), ParishError> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') || trimmed.starts_with("event:") {
        return Ok(());
    }
    let Some(data) = trimmed.strip_prefix("data:").map(str::trim) else {
        return Ok(());
    };
    let event: Value = serde_json::from_str(data).map_err(|error| {
        ParishError::Inference(format!("malformed OpenAI Responses SSE data: {error}"))
    })?;
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| ParishError::Inference("OpenAI Responses SSE event omitted type".into()))?;
    if *completed {
        return Err(ParishError::Inference(
            "OpenAI Responses stream contained data after response.completed".into(),
        ));
    }
    match event_type {
        "response.output_text.delta" => {
            let delta = event.get("delta").and_then(Value::as_str).ok_or_else(|| {
                ParishError::Inference("OpenAI Responses text delta omitted delta".into())
            })?;
            if !delta.is_empty() {
                token_tx.try_send(delta.to_string()).map_err(|_| {
                    ParishError::Inference(format!(
                        "OpenAI Responses token channel exceeded capacity {TOKEN_CHANNEL_CAPACITY}"
                    ))
                })?;
                accumulated.push_str(delta);
            }
        }
        "response.completed" => {
            let terminal = event.get("response").ok_or_else(|| {
                ParishError::Inference("response.completed omitted the terminal response".into())
            })?;
            let terminal_text = extract_completed_text(terminal)?;
            if terminal_text != *accumulated {
                return Err(ParishError::Inference(
                    "OpenAI Responses terminal text disagreed with streamed deltas".into(),
                ));
            }
            *completed = true;
        }
        "response.failed" | "response.incomplete" | "error" => {
            return Err(ParishError::Inference(format!(
                "OpenAI Responses stream terminated with {event_type}"
            )));
        }
        "response.output_item.added" | "response.output_item.done" => {
            let item_type = event
                .get("item")
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
                .ok_or_else(|| ParishError::Inference(format!("{event_type} omitted item type")))?;
            if !matches!(item_type, "message" | "reasoning") {
                return Err(ParishError::Inference(format!(
                    "OpenAI Responses stream contained forbidden output item {item_type}"
                )));
            }
        }
        "response.content_part.added" | "response.content_part.done" => {
            let part_type = event
                .get("part")
                .and_then(|part| part.get("type"))
                .and_then(Value::as_str)
                .ok_or_else(|| ParishError::Inference(format!("{event_type} omitted part type")))?;
            if part_type != "output_text" {
                return Err(ParishError::Inference(format!(
                    "OpenAI Responses stream contained forbidden content part {part_type}"
                )));
            }
        }
        "response.created"
        | "response.in_progress"
        | "response.output_text.done"
        | "response.reasoning_text.delta"
        | "response.reasoning_text.done"
        | "response.reasoning_summary_part.added"
        | "response.reasoning_summary_part.done"
        | "response.reasoning_summary_text.delta"
        | "response.reasoning_summary_text.done" => {}
        "response.refusal.delta" | "response.refusal.done" => {
            return Err(ParishError::Inference(
                "OpenAI Responses stream contained a refusal".into(),
            ));
        }
        other => {
            return Err(ParishError::Inference(format!(
                "unknown OpenAI Responses SSE event {other}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_uses_responses_schema_and_reasoning() {
        let body = OpenAiResponsesClient::request_body(
            "gpt-5.5",
            "hello",
            Some("system"),
            false,
            Some(ResponseFormat::JsonSchema {
                json_schema: crate::JsonSchemaSpec {
                    name: "answer".into(),
                    schema: json!({"type":"object"}),
                },
            }),
            &GenerateParams {
                max_tokens: Some(512),
                reasoning_effort: Some(ReasoningEffort::Xhigh),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(body["max_output_tokens"], 512);
        assert_eq!(body["reasoning"]["effort"], "xhigh");
        assert_eq!(body["store"], false);
        assert_eq!(body["text"]["format"]["type"], "json_schema");
        assert!(body.get("messages").is_none());
    }

    #[test]
    fn request_serializes_every_accepted_sampling_and_off_control() {
        let body = OpenAiResponsesClient::request_body(
            "gpt-5.5",
            "hello",
            None,
            false,
            None,
            &GenerateParams {
                temperature: Some(0.25),
                frequency_penalty: Some(-0.5),
                reasoning_effort: Some(ReasoningEffort::None),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(body["temperature"], 0.25);
        assert_eq!(body["frequency_penalty"], -0.5);
        assert_eq!(body["reasoning"]["effort"], "none");

        assert!(
            OpenAiResponsesClient::request_body(
                "gpt-5.5",
                "hello",
                None,
                false,
                None,
                &GenerateParams {
                    reasoning_effort: Some(ReasoningEffort::Max),
                    ..Default::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn terminal_parser_rejects_tools_and_incomplete_responses() {
        assert!(extract_completed_text(&json!({"status":"incomplete","output":[]})).is_err());
        assert!(
            extract_completed_text(&json!({
                "status":"completed",
                "output":[{"type":"function_call"}]
            }))
            .is_err()
        );
    }

    #[test]
    fn terminal_parser_quarantines_hidden_reasoning() {
        assert_eq!(
            extract_completed_text(&json!({
                "status":"completed",
                "output":[
                    {"type":"reasoning","summary":[]},
                    {"type":"message","role":"assistant","content":[{"type":"output_text","text":"ok"}]}
                ]
            }))
            .unwrap(),
            "ok"
        );
    }

    #[test]
    fn terminal_parser_concatenates_multiple_output_text_parts() {
        assert_eq!(
            extract_completed_text(&json!({
                "status":"completed",
                "output":[{"type":"message","role":"assistant","content":[
                    {"type":"output_text","text":"hello "},
                    {"type":"output_text","text":"world"}
                ]}]
            }))
            .unwrap(),
            "hello world"
        );
    }

    #[test]
    fn stream_requires_terminal_body_to_match_deltas() {
        let (tx, _rx) = mpsc::channel(4);
        let mut text = String::new();
        let mut completed = false;
        process_responses_sse_line(
            r#"data: {"type":"response.output_text.delta","delta":"ok"}"#,
            &tx,
            &mut text,
            &mut completed,
        )
        .unwrap();
        process_responses_sse_line(
            r#"data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"ok"}]}]}}"#,
            &tx,
            &mut text,
            &mut completed,
        )
        .unwrap();
        assert!(completed);
        assert_eq!(text, "ok");
    }
}
