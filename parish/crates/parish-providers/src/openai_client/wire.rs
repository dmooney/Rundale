//! OpenAI-compatible chat-completions wire types.
//!
//! Request body (`ChatCompletionRequest` + `ChatMessage`), the public
//! sampling/format parameter types (`GenerateParams`, `ResponseFormat`,
//! `JsonSchemaSpec`), and the response/stream-chunk deserialisation types.
//! Split out of the monolithic `openai_client` module (#1200) so request /
//! response schema drift is reviewable in isolation.
//!
//! The three parameter types are part of the crate's public API (re-exported
//! from `lib.rs` via `openai_client::{GenerateParams, JsonSchemaSpec,
//! ResponseFormat}`); the request/response structs are `pub(super)`
//! crate-internal protocol details.

use parish_config::ReasoningEffort;
use serde::{Deserialize, Serialize};

/// A single message in the chat completions request.
#[derive(Serialize, Debug)]
pub(super) struct ChatMessage<'a> {
    pub(super) role: &'a str,
    pub(super) content: &'a str,
}

/// Request body for the `/v1/chat/completions` endpoint.
#[derive(Serialize, Debug)]
pub(super) struct ChatCompletionRequest<'a> {
    pub(super) model: &'a str,
    pub(super) messages: Vec<ChatMessage<'a>>,
    pub(super) stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_completion_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    /// OpenAI-compat `frequency_penalty`. Range nominally `[-2.0, 2.0]`,
    /// but the Tier 1 dialogue call site sets `0.5` to break the
    /// degenerate repetition loops Qwen2.5-14B-4bit exhibits without a
    /// penalty (TODO #10 / #23 / #34). vllm-mlx, OpenAI, OpenRouter and
    /// most OpenAI-compat servers honour this field; Ollama ignores it.
    /// `None` omits the key from the wire body entirely.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) frequency_penalty: Option<f32>,
    /// Optional OpenAI-compatible extension used by reasoning-capable local
    /// servers such as vllm-mlx. `None` preserves provider portability;
    /// profiles may set `Some(false)` only after the target has been measured
    /// to require and accept the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) enable_thinking: Option<bool>,
    /// MLX-LM exposes the same switch through chat-template kwargs, while
    /// vllm-mlx consumes the top-level extension above. Profiles set one
    /// semantic knob and the local-compatible wire path emits both shapes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) chat_template_kwargs: Option<ChatTemplateKwargs>,
    /// OpenRouter's provider-neutral reasoning control. The client translates
    /// the existing semantic `enable_thinking` knob into this shape only for
    /// an authenticated OpenRouter host; local servers continue to receive
    /// their native compatibility fields above.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning: Option<ReasoningConfig>,
    /// DeepSeek's native thinking-mode control. Unlike OpenRouter's unified
    /// `reasoning` object, the first-party API expects `thinking.type`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) thinking: Option<DeepSeekThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// First-party DeepSeek and Google's OpenAI-compat endpoint both expose a
    /// top-level effort field, though their supported vocabularies differ.
    pub(super) reasoning_effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) service_tier: Option<&'static str>,
}

#[derive(Serialize, Debug)]
pub(super) struct ChatTemplateKwargs {
    pub(super) enable_thinking: bool,
}

#[derive(Serialize, Debug)]
pub(super) struct ReasoningConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) effort: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) exclude: Option<bool>,
}

#[derive(Serialize, Debug)]
pub(super) struct DeepSeekThinkingConfig {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
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
    /// Optional reasoning-mode control accepted by compatible OpenAI-style
    /// backends. Omitted for providers/models without a measured contract.
    pub enable_thinking: Option<bool>,
    /// Optional provider reasoning effort. OpenRouter translates this into
    /// its unified `reasoning.effort` request object; DeepSeek's first-party
    /// endpoint translates it into native `thinking` + `reasoning_effort`
    /// fields with that API's supported effort vocabulary.
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Google Gemini thinking level. Non-Google providers ignore this field.
    pub thinking_level: Option<crate::google_client::ThinkingLevel>,
    /// Requested Google inference service tier. Defaults to Standard.
    pub service_tier: Option<crate::google_client::ServiceTier>,
    /// Authoritative semantic reasoning intent for a v2 request. Native
    /// adapters use this per call so subrole overrides are not frozen into a
    /// category client. Legacy callers leave it unset.
    pub reasoning_intent: Option<parish_config::ReasoningIntent>,
    pub reasoning_dialect: Option<parish_config::ReasoningDialect>,
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
pub(super) struct ChatCompletionResponse {
    #[serde(default)]
    pub(super) choices: Vec<Choice>,
}

/// A single completion choice.
#[derive(Deserialize, Debug)]
pub(super) struct Choice {
    #[serde(default)]
    pub(super) message: MessageContent,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

/// Message content in a non-streaming response.
#[derive(Deserialize, Debug, Default)]
pub(super) struct MessageContent {
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) function_call: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) refusal: Option<serde_json::Value>,
}

/// A single SSE chunk from a streaming response.
#[derive(Deserialize, Debug)]
pub(super) struct ChatCompletionChunk {
    #[serde(default)]
    pub(super) choices: Vec<StreamChoice>,
}

/// A single choice in a streaming chunk.
#[derive(Deserialize, Debug)]
pub(super) struct StreamChoice {
    #[serde(default)]
    pub(super) delta: Delta,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

/// Delta content in a streaming chunk.
#[derive(Deserialize, Debug, Default)]
pub(super) struct Delta {
    #[serde(default)]
    pub(super) content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) function_call: Option<serde_json::Value>,
    #[serde(default)]
    pub(super) refusal: Option<serde_json::Value>,
}

/// Extracts the text content from a non-streaming response.
pub(super) fn extract_complete_content(resp: &ChatCompletionResponse) -> Result<String, String> {
    if resp.choices.len() != 1 {
        return Err(format!(
            "OpenAI-compatible response contained {} choices; expected exactly one",
            resp.choices.len()
        ));
    }
    let choice = &resp.choices[0];
    if choice.message.tool_calls.is_some()
        || choice.message.function_call.is_some()
        || choice.message.refusal.is_some()
    {
        return Err(
            "OpenAI-compatible response mixed text with a forbidden tool/function/refusal payload"
                .into(),
        );
    }
    match choice.finish_reason.as_deref() {
        Some("stop") => {}
        Some(reason) => {
            return Err(format!(
                "OpenAI-compatible response was incomplete (finish_reason={reason})"
            ));
        }
        None => {
            return Err(
                "OpenAI-compatible response omitted the required finish_reason".to_string(),
            );
        }
    }
    choice
        .message
        .content
        .clone()
        .filter(|content| !content.is_empty())
        .ok_or_else(|| "OpenAI-compatible response contained no text content".to_string())
}
