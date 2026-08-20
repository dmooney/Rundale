//! One-shot model-route probes. The caller must persist `body` before parsing.

use parish_config::{AuthAdapter, InferenceAdapter, OutputLimitField, ResolvedRoute};
use parish_types::ParishError;
use reqwest::{Client, Url};
use serde_json::{Value, json};

const MAX_PROBE_BODY_BYTES: usize = 8 * 1024 * 1024;
const OPENAI_RESPONSES_PROBE_MAX_OUTPUT_TOKENS: u32 = 16;

fn openai_responses_probe_body(model: &str) -> Value {
    json!({
        "model": model,
        "input": "Reply with exactly: ok",
        "stream": false,
        "store": false,
        // The live Responses API rejects values below 16. Keep the probe at
        // that minimum so verification remains cheap without becoming an
        // invalid request.
        "max_output_tokens": OPENAI_RESPONSES_PROBE_MAX_OUTPUT_TOKENS
    })
}

pub struct RawProbeResponse {
    pub request_bytes: Vec<u8>,
    pub status: u16,
    pub provider_request_id: Option<String>,
    pub body: Vec<u8>,
    pub transport_error: Option<String>,
}

pub async fn probe_route_raw(route: &ResolvedRoute) -> Result<RawProbeResponse, ParishError> {
    if route.inference_adapter == InferenceAdapter::Simulator {
        return Ok(RawProbeResponse {
            request_bytes: br#"{"probe":"simulator"}"#.to_vec(),
            status: 200,
            provider_request_id: None,
            body: br#"{"text":"ok","finish_reason":"stop"}"#.to_vec(),
            transport_error: None,
        });
    }
    let (relative, mut body) = match route.inference_adapter {
        InferenceAdapter::OpenaiResponsesV1 => (
            "responses",
            openai_responses_probe_body(&route.key.model_id),
        ),
        InferenceAdapter::OpenaiChatV1 => (
            "chat/completions",
            json!({
                "model": route.key.model_id,
                "messages": [{"role":"user","content":"Reply with exactly: ok"}],
                "stream": false
            }),
        ),
        InferenceAdapter::AnthropicMessages2023_06_01 => (
            "messages",
            json!({
                "model": route.key.model_id,
                "messages": [{"role":"user","content":"Reply with exactly: ok"}],
                "stream": false,
                "max_tokens": 8
            }),
        ),
        InferenceAdapter::GoogleInteractionsV1 => (
            "interactions",
            json!({
                "model": route.key.model_id,
                "input": "Reply with exactly: ok",
                "stream": false,
                "store": false,
                "generation_config": {"max_output_tokens": 8}
            }),
        ),
        InferenceAdapter::Simulator => {
            return Err(ParishError::Config(
                "the simulator adapter cannot be billably probed".into(),
            ));
        }
    };
    if route.inference_adapter == InferenceAdapter::OpenaiChatV1 {
        let field = match route
            .openai_output_limit_field
            .unwrap_or(OutputLimitField::MaxTokens)
        {
            OutputLimitField::MaxTokens => "max_tokens",
            OutputLimitField::MaxCompletionTokens => "max_completion_tokens",
        };
        body[field] = json!(8);
    }
    let url = join_prefix(&route.inference_base_url, relative)?;
    let request_bytes = serde_json::to_vec(&body)
        .map_err(|error| ParishError::Config(format!("serialize probe request: {error}")))?;
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| ParishError::Inference(format!("build probe client: {error}")))?;
    let mut request = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json");
    let credential = route.credential.as_ref().map(|secret| secret.expose());
    request = match (route.auth_adapter, credential) {
        (AuthAdapter::None, _) => request,
        (AuthAdapter::Bearer, Some(key)) => request.bearer_auth(key),
        (AuthAdapter::AnthropicKey, Some(key)) => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        (AuthAdapter::GoogleKey, Some(key)) => request.header("x-goog-api-key", key),
        (_, None) => {
            return Err(ParishError::Config(
                "probe route credential is missing".into(),
            ));
        }
    };
    let mut response = match request.body(request_bytes.clone()).send().await {
        Ok(response) => response,
        Err(error) => {
            return Ok(RawProbeResponse {
                request_bytes,
                status: 0,
                provider_request_id: None,
                body: error.to_string().into_bytes(),
                transport_error: Some(format!("model probe failed: {error}")),
            });
        }
    };
    let status = response.status().as_u16();
    let provider_request_id = ["x-request-id", "request-id", "x-goog-request-id"]
        .iter()
        .find_map(|name| response.headers().get(*name))
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let mut response_body = Vec::new();
    loop {
        let chunk = match response.chunk().await {
            Ok(Some(chunk)) => chunk,
            Ok(None) => break,
            Err(error) => {
                return Ok(RawProbeResponse {
                    request_bytes,
                    status,
                    provider_request_id,
                    body: response_body,
                    transport_error: Some(format!("read model probe: {error}")),
                });
            }
        };
        if response_body.len().saturating_add(chunk.len()) > MAX_PROBE_BODY_BYTES {
            return Ok(RawProbeResponse {
                request_bytes,
                status,
                provider_request_id,
                body: response_body,
                transport_error: Some("model probe response exceeded 8 MiB".into()),
            });
        }
        response_body.extend_from_slice(&chunk);
    }
    Ok(RawProbeResponse {
        request_bytes,
        status,
        provider_request_id,
        body: response_body,
        transport_error: None,
    })
}

fn join_prefix(base: &str, relative: &str) -> Result<Url, ParishError> {
    Url::parse(&format!("{}/", base.trim_end_matches('/')))
        .and_then(|url| url.join(relative))
        .map_err(|error| ParishError::Config(format!("invalid probe route URL: {error}")))
}

pub fn validate_probe_response(
    adapter: InferenceAdapter,
    status: u16,
    body: &[u8],
) -> Result<(String, Option<u64>, Option<u64>), String> {
    if !(200..300).contains(&status) {
        return Err(format!("HTTP {status}"));
    }
    let value: Value = serde_json::from_slice(body).map_err(|error| error.to_string())?;
    let (reason, text, input, output) = match adapter {
        InferenceAdapter::OpenaiResponsesV1 => {
            let output_items = value
                .get("output")
                .and_then(Value::as_array)
                .ok_or_else(|| "probe response omitted output items".to_string())?;
            let mut text = String::new();
            let mut message_count = 0usize;
            let mut text_count = 0usize;
            for item in output_items {
                match item.get("type").and_then(Value::as_str) {
                    Some("reasoning") => continue,
                    Some("message") => {
                        if item
                            .get("role")
                            .and_then(Value::as_str)
                            .is_some_and(|role| role != "assistant")
                        {
                            return Err("probe output message was not from the assistant".into());
                        }
                        message_count += 1;
                    }
                    _ => {
                        return Err(
                            "probe response contained forbidden tool or unknown output".into()
                        );
                    }
                }
                for content in item
                    .get("content")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if content.get("type").and_then(Value::as_str) != Some("output_text") {
                        return Err("probe response contained forbidden non-text output".into());
                    }
                    text_count += 1;
                    text.push_str(
                        content
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| "probe output_text omitted text".to_string())?,
                    );
                }
            }
            if message_count != 1 || text_count == 0 || text.trim().is_empty() {
                return Err(
                    "probe response must contain one assistant message with non-empty output_text"
                        .into(),
                );
            }
            (
                value.get("status").and_then(Value::as_str),
                text,
                value.pointer("/usage/input_tokens").and_then(Value::as_u64),
                value
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64),
            )
        }
        InferenceAdapter::OpenaiChatV1 => {
            let choices = value
                .get("choices")
                .and_then(Value::as_array)
                .ok_or_else(|| "probe response omitted choices".to_string())?;
            if choices.len() != 1 {
                return Err(format!(
                    "probe response must contain exactly one choice, found {}",
                    choices.len()
                ));
            }
            let message = value
                .pointer("/choices/0/message")
                .ok_or_else(|| "probe response omitted assistant message".to_string())?;
            if ["tool_calls", "function_call", "refusal"]
                .iter()
                .any(|field| message.get(*field).is_some_and(|entry| !entry.is_null()))
            {
                return Err("probe response contained forbidden non-text output".into());
            }
            (
                value
                    .pointer("/choices/0/finish_reason")
                    .and_then(Value::as_str),
                message
                    .get("content")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                value
                    .pointer("/usage/prompt_tokens")
                    .and_then(Value::as_u64),
                value
                    .pointer("/usage/completion_tokens")
                    .and_then(Value::as_u64),
            )
        }
        InferenceAdapter::AnthropicMessages2023_06_01 => {
            let content = value
                .get("content")
                .and_then(Value::as_array)
                .ok_or_else(|| "probe response omitted content blocks".to_string())?;
            let mut combined = String::new();
            for block in content {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => combined.push_str(
                        block
                            .get("text")
                            .and_then(Value::as_str)
                            .ok_or_else(|| "probe text block omitted text".to_string())?,
                    ),
                    Some("thinking" | "redacted_thinking") => {}
                    _ => return Err("probe response contained forbidden non-text output".into()),
                }
            }
            (
                value.get("stop_reason").and_then(Value::as_str),
                combined,
                value.pointer("/usage/input_tokens").and_then(Value::as_u64),
                value
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64),
            )
        }
        InferenceAdapter::GoogleInteractionsV1 => {
            let text = crate::google_client::extract_model_output(&value)?;
            (
                value
                    .get("status")
                    .and_then(Value::as_str)
                    .or_else(|| value.pointer("/interaction/status").and_then(Value::as_str)),
                text,
                value
                    .pointer("/usage/total_input_tokens")
                    .and_then(Value::as_u64),
                value
                    .pointer("/usage/total_output_tokens")
                    .and_then(Value::as_u64),
            )
        }
        InferenceAdapter::Simulator => (
            value.get("finish_reason").and_then(Value::as_str),
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            None,
            None,
        ),
    };
    let reason = reason.ok_or_else(|| "probe response omitted terminal reason".to_string())?;
    let successful = matches!(
        reason.to_ascii_lowercase().as_str(),
        "stop" | "end_turn" | "stop_sequence" | "completed" | "complete" | "success"
    );
    if !successful {
        return Err(format!("non-success terminal reason {reason:?}"));
    }
    if text.trim().is_empty() {
        return Err("probe response contained no visible text".into());
    }
    Ok((reason.to_string(), input, output))
}

/// A generic HTTP 404 may mean a bad gateway path, not a missing model. Only
/// adapter-native error codes that explicitly identify the requested model
/// are authoritative enough to affect startup eligibility.
pub fn is_definitive_model_not_found(
    adapter: InferenceAdapter,
    status: u16,
    body: &[u8],
    model_id: &str,
) -> bool {
    if status != 404 {
        return false;
    }
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return false;
    };
    let message_mentions_model = value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .is_some_and(|message| message.contains(model_id));
    let native_code = value
        .pointer("/error/code")
        .or_else(|| value.pointer("/error/type"))
        .or_else(|| value.pointer("/error/status"))
        .and_then(Value::as_str);
    message_mentions_model
        && match adapter {
            InferenceAdapter::OpenaiResponsesV1 | InferenceAdapter::OpenaiChatV1 => {
                native_code == Some("model_not_found")
            }
            InferenceAdapter::AnthropicMessages2023_06_01 => native_code == Some("not_found_error"),
            InferenceAdapter::GoogleInteractionsV1 => native_code == Some("NOT_FOUND"),
            InferenceAdapter::Simulator => false,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_responses_probe_uses_live_minimum_and_disables_storage() {
        let body = openai_responses_probe_body("gpt-test");
        assert_eq!(
            body["max_output_tokens"].as_u64(),
            Some(u64::from(OPENAI_RESPONSES_PROBE_MAX_OUTPUT_TOKENS))
        );
        assert!(body["max_output_tokens"].as_u64().unwrap() >= 16);
        assert_eq!(body["store"].as_bool(), Some(false));
        assert_eq!(body["model"].as_str(), Some("gpt-test"));
    }

    #[test]
    fn openai_probe_rejects_mixed_tool_output() {
        let body = br#"{
            "choices":[{"finish_reason":"stop","message":{"content":"ok","tool_calls":[]}}]
        }"#;
        assert!(validate_probe_response(InferenceAdapter::OpenaiChatV1, 200, body).is_err());
    }

    #[test]
    fn anthropic_probe_rejects_non_text_blocks() {
        let body = br#"{
            "stop_reason":"end_turn",
            "content":[{"type":"text","text":"ok"},{"type":"tool_use","id":"1"}]
        }"#;
        assert!(
            validate_probe_response(InferenceAdapter::AnthropicMessages2023_06_01, 200, body)
                .is_err()
        );
    }

    #[test]
    fn anthropic_probe_quarantines_hidden_thinking_like_runtime() {
        let body = br#"{
            "stop_reason":"end_turn",
            "content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"ok"}]
        }"#;
        assert!(
            validate_probe_response(InferenceAdapter::AnthropicMessages2023_06_01, 200, body)
                .is_ok()
        );
    }

    #[test]
    fn google_probe_reads_interactions_v1_shape() {
        let body = br#"{
            "status":"completed",
            "steps":[{"type":"model_output","content":[{"type":"text","text":"ok"}]}],
            "usage":{"total_input_tokens":3,"total_output_tokens":1}
        }"#;
        let (_, input, output) =
            validate_probe_response(InferenceAdapter::GoogleInteractionsV1, 200, body).unwrap();
        assert_eq!((input, output), (Some(3), Some(1)));
    }

    #[test]
    fn google_probe_rejects_mixed_tool_output_like_runtime() {
        let body = br#"{
            "status":"completed",
            "steps":[
                {"type":"model_output","content":[{"type":"text","text":"ok"}]},
                {"type":"tool_call","name":"search"}
            ]
        }"#;
        assert!(
            validate_probe_response(InferenceAdapter::GoogleInteractionsV1, 200, body).is_err()
        );
    }
}
