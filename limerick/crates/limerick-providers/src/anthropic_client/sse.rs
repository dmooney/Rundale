//! Anthropic SSE (Server-Sent Events) stream parsing.
//!
//! Anthropic streams interleave `event: <name>` lines with `data: <json>`
//! lines; the JSON payloads carry a `type` field, so dispatch is on `type`.
//! `process_sse_line` forwards text deltas through the token channel and
//! signals stream completion / error. Split out of the monolithic
//! `anthropic_client` module (#1200).

use serde::Deserialize;
use std::collections::BTreeMap;
use tokio::sync::mpsc;

use crate::{SseResult, TOKEN_CHANNEL_CAPACITY};

/// Processes a single SSE line: dispatches by event `type` field.
///
/// Anthropic SSE streams interleave `event: <name>` lines with
/// `data: <json>` lines. The JSON payloads always carry a `type` field
/// that matches the preceding event name, so we dispatch on `type`
/// directly and ignore the `event:` lines — simpler and tolerant of
/// keepalive or reordering.
#[derive(Debug, Default)]
pub(super) struct AnthropicStreamState {
    stop_reason: Option<String>,
    blocks: BTreeMap<u64, StreamBlockKind>,
    pub(super) completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamBlockKind {
    Text,
    Thinking,
    RedactedThinking,
}

pub(super) fn process_sse_line(
    line: &str,
    token_tx: &mpsc::Sender<String>,
    accumulated: &mut String,
    state: &mut AnthropicStreamState,
) -> SseResult {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with(':') || trimmed.starts_with("event:") {
        return SseResult::Continue;
    }
    let Some(data) = trimmed.strip_prefix("data:").map(str::trim) else {
        return SseResult::Continue;
    };

    let Ok(event) = serde_json::from_str::<StreamEvent>(data) else {
        return SseResult::Error("malformed Anthropic SSE data".to_string());
    };
    if state.completed {
        return SseResult::Error("Anthropic stream contained data after message_stop".to_string());
    }

    match event {
        StreamEvent::ContentBlockDelta { index, delta } => {
            let Some(kind) = state.blocks.get(&index).copied() else {
                return SseResult::Error(format!(
                    "Anthropic content delta referenced unopened block {index}"
                ));
            };
            match delta {
                StreamDelta::TextDelta { text }
                    if kind == StreamBlockKind::Text && !text.is_empty() =>
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
                StreamDelta::TextDelta { .. } if kind == StreamBlockKind::Text => {}
                StreamDelta::ThinkingDelta { .. } if kind == StreamBlockKind::Thinking => {}
                StreamDelta::SignatureDelta { .. }
                    if matches!(
                        kind,
                        StreamBlockKind::Thinking | StreamBlockKind::RedactedThinking
                    ) => {}
                StreamDelta::Other => {
                    return SseResult::Error(
                        "Anthropic stream contained a non-text content delta".into(),
                    );
                }
                _ => {
                    return SseResult::Error(
                        "Anthropic content delta did not match its declared block type".into(),
                    );
                }
            }
            SseResult::Continue
        }
        StreamEvent::MessageDelta { delta } => {
            if state.stop_reason.is_some() {
                return SseResult::Error(
                    "Anthropic stream contained duplicate message_delta terminal metadata".into(),
                );
            }
            state.stop_reason = delta.stop_reason;
            if state.stop_reason.is_none() {
                return SseResult::Error("Anthropic message_delta omitted stop_reason".into());
            }
            SseResult::Continue
        }
        StreamEvent::MessageStop => match state.stop_reason.as_deref() {
            Some("end_turn" | "stop_sequence")
                if !accumulated.is_empty() && state.blocks.is_empty() =>
            {
                state.completed = true;
                SseResult::Done
            }
            Some("end_turn" | "stop_sequence") if !state.blocks.is_empty() => {
                SseResult::Error("Anthropic message_stop arrived with an open content block".into())
            }
            Some("end_turn" | "stop_sequence") => {
                SseResult::Error("Anthropic stream completed with empty text".into())
            }
            Some(reason) => SseResult::Error(format!(
                "Anthropic stream was incomplete or non-textual (stop_reason={reason})"
            )),
            None => {
                SseResult::Error("Anthropic message_stop arrived without a stop_reason".to_string())
            }
        },
        StreamEvent::Error { error } => {
            let msg = format!(
                "Anthropic stream error ({}): {}",
                error.error_type, error.message
            );
            SseResult::Error(msg)
        }
        StreamEvent::ContentBlockStart {
            index,
            content_block,
        } => {
            let kind = match content_block {
                StreamBlock::Text => StreamBlockKind::Text,
                StreamBlock::Thinking => StreamBlockKind::Thinking,
                StreamBlock::RedactedThinking => StreamBlockKind::RedactedThinking,
                StreamBlock::Other => {
                    return SseResult::Error(
                        "Anthropic stream contained a forbidden tool or unknown block".into(),
                    );
                }
            };
            if state.blocks.insert(index, kind).is_some() {
                return SseResult::Error(format!(
                    "Anthropic stream opened content block {index} twice"
                ));
            }
            SseResult::Continue
        }
        StreamEvent::ContentBlockStop { index } => {
            if state.blocks.remove(&index).is_none() {
                return SseResult::Error(format!(
                    "Anthropic stream stopped unopened content block {index}"
                ));
            }
            SseResult::Continue
        }
        StreamEvent::MessageStart | StreamEvent::Ping => SseResult::Continue,
        StreamEvent::Other => {
            SseResult::Error("Anthropic stream contained an unknown event type".into())
        }
    }
}

/// The subset of SSE event payloads we care about.
#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum StreamEvent {
    /// Incremental update to the current content block.
    ContentBlockDelta {
        index: u64,
        #[serde(default)]
        delta: StreamDelta,
    },
    MessageDelta {
        #[serde(default)]
        delta: MessageDelta,
    },
    /// Terminal event; stream is complete.
    MessageStop,
    MessageStart,
    ContentBlockStart {
        index: u64,
        content_block: StreamBlock,
    },
    ContentBlockStop {
        index: u64,
    },
    Ping,
    /// Error event sent mid-stream (e.g. output token limit, internal error).
    Error {
        error: StreamError,
    },
    /// Any other event we don't act on (kept so deserialisation never fails).
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum StreamBlock {
    Text,
    Thinking,
    RedactedThinking,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug, Default)]
pub(super) struct MessageDelta {
    #[serde(default)]
    stop_reason: Option<String>,
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
    ThinkingDelta {
        #[serde(default)]
        #[serde(rename = "thinking")]
        _thinking: String,
    },
    SignatureDelta {
        #[serde(default)]
        #[serde(rename = "signature")]
        _signature: String,
    },
    /// Unknown delta type (e.g. `input_json_delta` for tool use). Ignored.
    #[default]
    #[serde(other)]
    Other,
}
