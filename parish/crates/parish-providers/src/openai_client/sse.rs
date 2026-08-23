//! OpenAI-compatible SSE (Server-Sent Events) stream parsing.
//!
//! `read_sse_stream` drives the chunked HTTP body, splitting it into lines
//! and feeding each through `process_sse_line`, which forwards delta content
//! through the token channel and detects the `[DONE]` / `finish_reason:stop`
//! terminators. Split out of the monolithic `openai_client` module (#1200).

use tokio::sync::mpsc;

use super::wire::ChatCompletionChunk;
use crate::{SseResult, TOKEN_CHANNEL_CAPACITY};

/// Reads an SSE response body, parsing data lines and forwarding tokens.
///
/// Shared by [`super::OpenAiClient::generate_stream`] and
/// [`super::OpenAiClient::generate_stream_json`] to avoid duplicating the
/// streaming-loop boilerplate (TD-004).
pub(super) async fn read_sse_stream(
    response: reqwest::Response,
    token_tx: &mpsc::Sender<String>,
) -> Result<String, parish_types::ParishError> {
    let mut accumulated = String::new();
    let mut line_buf = String::new();
    let mut decoder = crate::utf8_stream::Utf8StreamDecoder::new();
    let mut saw_stop = false;

    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| parish_types::ParishError::Network(e.to_string()))?
    {
        line_buf.push_str(&decoder.push(&chunk));

        while let Some(newline_pos) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=newline_pos).collect();
            if matches!(parse_sse_line(&line), Some(SseData::Done)) {
                return if saw_stop && !accumulated.is_empty() {
                    Ok(accumulated)
                } else {
                    Err(parish_types::ParishError::Inference(
                        "OpenAI stream reached [DONE] without non-empty content and finish_reason=stop".into(),
                    ))
                };
            }
            if saw_stop && parse_sse_line(&line).is_some() {
                return Err(parish_types::ParishError::Inference(
                    "OpenAI stream contained data after finish_reason=stop before [DONE]".into(),
                ));
            }
            match process_sse_line(&line, token_tx, &mut accumulated) {
                SseResult::Continue => {}
                SseResult::Done => saw_stop = true,
                SseResult::Error(msg) => return Err(parish_types::ParishError::Inference(msg)),
            }
        }
    }

    line_buf.push_str(&decoder.flush());
    let remaining = line_buf.trim();
    if !remaining.is_empty() {
        if matches!(parse_sse_line(remaining), Some(SseData::Done)) {
            return if saw_stop && !accumulated.is_empty() {
                Ok(accumulated)
            } else {
                Err(parish_types::ParishError::Inference(
                    "OpenAI stream reached [DONE] without non-empty content and finish_reason=stop"
                        .into(),
                ))
            };
        }
        if saw_stop && parse_sse_line(remaining).is_some() {
            return Err(parish_types::ParishError::Inference(
                "OpenAI stream contained data after finish_reason=stop before [DONE]".into(),
            ));
        }
        match process_sse_line(remaining, token_tx, &mut accumulated) {
            SseResult::Continue => {}
            SseResult::Done => saw_stop = true,
            SseResult::Error(msg) => return Err(parish_types::ParishError::Inference(msg)),
        }
    }

    Err(parish_types::ParishError::Inference(if saw_stop {
        "stream ended after finish_reason=stop without [DONE]".to_string()
    } else {
        "stream ended without a complete response (missing terminal marker)".to_string()
    }))
}

/// Processes a single SSE line: extracts content, sends tokens, detects completion.
pub(super) fn process_sse_line(
    line: &str,
    token_tx: &mpsc::Sender<String>,
    accumulated: &mut String,
) -> SseResult {
    let Some(data) = parse_sse_line(line) else {
        return SseResult::Continue;
    };
    match data {
        SseData::Done => {
            SseResult::Error("stream ended at [DONE] without finish_reason=stop".to_string())
        }
        SseData::Chunk(chunk_data) => {
            if chunk_data.choices.iter().any(|choice| {
                choice.delta.tool_calls.is_some()
                    || choice.delta.function_call.is_some()
                    || choice.delta.refusal.is_some()
            }) {
                return SseResult::Error(
                    "OpenAI stream contained a forbidden tool/function/refusal delta".into(),
                );
            }
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
            if let Some(reason) = chunk_data
                .choices
                .first()
                .and_then(|c| c.finish_reason.as_deref())
            {
                if reason == "stop" {
                    return SseResult::Done;
                }
                return SseResult::Error(format!(
                    "stream ended without a complete response (finish_reason={reason})"
                ));
            }
            SseResult::Continue
        }
        SseData::Malformed(error) => {
            SseResult::Error(format!("malformed OpenAI-compatible SSE data: {error}"))
        }
    }
}

/// Parsed SSE data from a streaming line.
pub(super) enum SseData {
    /// The `[DONE]` sentinel, indicating stream end.
    Done,
    /// A parsed chunk of streaming data.
    Chunk(ChatCompletionChunk),
    /// A `data:` record existed but did not match the advertised wire schema.
    Malformed(String),
}

/// Parses a single SSE line from a streaming response.
///
/// Handles the `data: ` prefix (with or without space), `[DONE]` sentinel,
/// and `: ` keepalive comments. Returns `None` for empty lines, comments,
/// or unparseable data.
pub(super) fn parse_sse_line(line: &str) -> Option<SseData> {
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

    Some(match serde_json::from_str::<ChatCompletionChunk>(data) {
        Ok(chunk) => SseData::Chunk(chunk),
        Err(error) => SseData::Malformed(error.to_string()),
    })
}
