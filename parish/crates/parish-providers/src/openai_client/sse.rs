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

    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| parish_types::ParishError::Network(e.to_string()))?
    {
        line_buf.push_str(&decoder.push(&chunk));

        while let Some(newline_pos) = line_buf.find('\n') {
            let line: String = line_buf.drain(..=newline_pos).collect();
            match process_sse_line(&line, token_tx, &mut accumulated) {
                SseResult::Continue => {}
                SseResult::Done => return Ok(accumulated),
                SseResult::Error(msg) => return Err(parish_types::ParishError::Inference(msg)),
            }
        }
    }

    line_buf.push_str(&decoder.flush());
    let remaining = line_buf.trim();
    if !remaining.is_empty() {
        match process_sse_line(remaining, token_tx, &mut accumulated) {
            SseResult::Continue => {}
            SseResult::Done => return Ok(accumulated),
            SseResult::Error(msg) => return Err(parish_types::ParishError::Inference(msg)),
        }
    }

    Err(parish_types::ParishError::Inference(
        "stream ended without a complete response (missing terminal marker)".to_string(),
    ))
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
        SseData::Done => SseResult::Done,
        SseData::Chunk(chunk_data) => {
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
    }
}

/// Parsed SSE data from a streaming line.
pub(super) enum SseData {
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

    serde_json::from_str::<ChatCompletionChunk>(data)
        .ok()
        .map(SseData::Chunk)
}
