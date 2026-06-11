//! Anthropic SSE (Server-Sent Events) stream parsing.
//!
//! Anthropic streams interleave `event: <name>` lines with `data: <json>`
//! lines; the JSON payloads carry a `type` field, so dispatch is on `type`.
//! `process_sse_line` forwards text deltas through the token channel and
//! signals stream completion / error. Split out of the monolithic
//! `anthropic_client` module (#1200).

use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{SseResult, TOKEN_CHANNEL_CAPACITY};

/// Processes a single SSE line: dispatches by event `type` field.
///
/// Anthropic SSE streams interleave `event: <name>` lines with
/// `data: <json>` lines. The JSON payloads always carry a `type` field
/// that matches the preceding event name, so we dispatch on `type`
/// directly and ignore the `event:` lines — simpler and tolerant of
/// keepalive or reordering.
pub(super) fn process_sse_line(
    line: &str,
    token_tx: &mpsc::Sender<String>,
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
                if token_tx.try_send(text.clone()).is_err() {
                    tracing::warn!(
                        "token streaming channel full (capacity {}); token dropped — \
                         consumer is not keeping up with LLM output (#83)",
                        TOKEN_CHANNEL_CAPACITY,
                    );
                }
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
pub(super) enum StreamEvent {
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
pub(super) struct StreamError {
    #[serde(rename = "type")]
    pub(super) error_type: String,
    pub(super) message: String,
}

/// Delta payload inside a `content_block_delta` event.
#[derive(Deserialize, Debug, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum StreamDelta {
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
