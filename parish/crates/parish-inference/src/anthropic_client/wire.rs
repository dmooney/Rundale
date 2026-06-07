//! Anthropic Messages API wire types and constants.
//!
//! Request body (`MessagesRequest` + `Message`/`SystemBlock`/`CacheControl`)
//! and response body (`MessagesResponse` + `ContentBlock`) for
//! `POST /v1/messages`, plus the two protocol constants and the small
//! response-extraction helpers. Split out of the monolithic
//! `anthropic_client` module (#1200) so request/response schema drift is
//! reviewable in isolation.
//!
//! Visibility is `pub(super)`: these types are crate-internal protocol
//! details consumed by [`super::AnthropicClient`] and its tests, never part
//! of the public API.

use serde::{Deserialize, Serialize};

/// Required Anthropic API version header value.
///
/// Anthropic pins request/response shape to this date. Bump only when the
/// request builder and response deserializer have been updated to match.
pub(super) const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Default `max_tokens` when the caller passes `None`.
///
/// Anthropic requires `max_tokens` on every request. This default is large
/// enough for streamed dialogue and JSON metadata but well under model
/// context limits.
pub(super) const DEFAULT_MAX_TOKENS: u32 = 4096;

// --- Request types ------------------------------------------------------

/// A single turn in the conversation. Only `user`/`assistant` roles —
/// the system prompt is a top-level field, not a message.
#[derive(Serialize, Debug)]
pub(super) struct Message<'a> {
    pub(super) role: &'a str,
    pub(super) content: &'a str,
}

/// Cache-control breakpoint for Anthropic's prompt caching.
///
/// Setting `type = "ephemeral"` on a system-prompt block marks that block
/// as a prefix-cache checkpoint, allowing Anthropic to reuse the KV-cache
/// of the static persona text across turns for BYOK cloud deployments.
/// This reduces per-turn token cost for the long static prefix (~600+ tokens).
///
/// Reference: https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching
#[derive(Serialize, Debug)]
pub(super) struct CacheControl {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
}

/// One block in the structured `system` array.
///
/// Anthropic supports a list of content blocks as the top-level `system`
/// field. Sending `cache_control: {type: "ephemeral"}` on the persona block
/// activates Anthropic's prompt caching for the stable persona prefix.
#[derive(Serialize, Debug)]
pub(super) struct SystemBlock<'a> {
    #[serde(rename = "type")]
    pub(super) kind: &'static str,
    pub(super) text: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) cache_control: Option<CacheControl>,
}

impl<'a> SystemBlock<'a> {
    /// Creates a system block with Anthropic's ephemeral cache-control breakpoint.
    pub(super) fn with_cache_control(text: &'a str) -> Self {
        Self {
            kind: "text",
            text,
            cache_control: Some(CacheControl { kind: "ephemeral" }),
        }
    }
}

/// Request body for `POST /v1/messages`.
#[derive(Serialize, Debug)]
pub(super) struct MessagesRequest<'a> {
    pub(super) model: &'a str,
    pub(super) messages: Vec<Message<'a>>,
    /// Top-level system prompt as a structured block list.
    ///
    /// Anthropic supports both a bare string and a list of typed blocks.
    /// We always use the block form so we can attach `cache_control:
    /// {type: "ephemeral"}` to the static persona prefix, enabling Anthropic's
    /// prompt caching for BYOK cloud deployments (issue #1152, finding 4).
    ///
    /// When `None`, the field is omitted (no system prompt).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) system: Option<Vec<SystemBlock<'a>>>,
    /// Required by Anthropic — see [`DEFAULT_MAX_TOKENS`].
    pub(super) max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) temperature: Option<f32>,
    /// Only serialise when `true`; omitted flag defaults to non-streaming.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub(super) stream: bool,
}

// --- Response types -----------------------------------------------------

/// Non-streaming response from `POST /v1/messages`.
#[derive(Deserialize, Debug, Default)]
pub(super) struct MessagesResponse {
    #[serde(default)]
    pub(super) content: Vec<ContentBlock>,
}

/// One block in the response `content` array. Anthropic returns multiple
/// block types; we only emit text from `text` blocks and ignore others
/// (e.g. `tool_use`) for now.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum ContentBlock {
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
pub(super) fn extract_text(resp: &MessagesResponse) -> String {
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
pub(super) fn extract_api_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let msg = v.get("error")?.get("message")?.as_str()?;
    Some(msg.to_string())
}
