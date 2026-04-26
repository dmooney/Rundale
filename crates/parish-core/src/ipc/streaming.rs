//! Shared NPC token streaming logic for all frontends.
//!
//! Reads tokens from an inference channel, extracts the `dialogue` field from
//! JSON responses incrementally (or streams plain text directly for non-JSON),
//! batches emitted text, and calls a user-provided emit function.

use std::time::{Duration, Instant};

use parish_types::extract_dialogue_from_partial_json;

/// How many milliseconds to batch streaming tokens before emitting.
pub const BATCH_MS: u64 = 16;

/// Reads tokens from `token_rx`, detects whether the stream is JSON or plain
/// text, and emits displayable dialogue incrementally.
///
/// For JSON streams: extracts the `dialogue` field value incrementally via
/// [`extract_dialogue_from_partial_json`], hiding metadata fields from display.
///
/// For plain text streams (e.g. NPC arrival reactions): emits the raw text
/// token-by-token, preserving progressive display.
///
/// Returns the full accumulated response so the caller can parse metadata
/// (mood, action, language hints, etc.) after streaming completes.
pub async fn stream_npc_tokens(
    mut token_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    mut emit_token: impl FnMut(&str),
) -> String {
    let mut accumulated = String::new();
    let mut displayed_len: usize = 0;
    let mut batch = String::new();
    let mut last_emit = Instant::now();
    // None = undecided, true = JSON with dialogue field, false = plain text
    let mut is_json: Option<bool> = None;

    while let Some(token) = token_rx.recv().await {
        accumulated.push_str(&token);

        match is_json {
            None => {
                // Detect mode from the accumulated content so far.
                let trimmed = accumulated.trim_start();
                if trimmed.starts_with('{') {
                    if let Some(dialogue) = extract_dialogue_from_partial_json(&accumulated) {
                        is_json = Some(true);
                        if dialogue.len() > displayed_len {
                            batch.push_str(&dialogue[displayed_len..]);
                            displayed_len = dialogue.len();
                        }
                    }
                    // If starts with '{' but no dialogue field yet, stay undecided
                    // (the field might appear in a later chunk).
                } else if !trimmed.is_empty() {
                    // Non-JSON: stream raw text incrementally
                    is_json = Some(false);
                    batch.push_str(&accumulated[displayed_len..]);
                    displayed_len = accumulated.len();
                }
            }
            Some(true) => {
                if let Some(dialogue) = extract_dialogue_from_partial_json(&accumulated)
                    && dialogue.len() > displayed_len
                {
                    batch.push_str(&dialogue[displayed_len..]);
                    displayed_len = dialogue.len();
                }
            }
            Some(false) => {
                if accumulated.len() > displayed_len {
                    batch.push_str(&accumulated[displayed_len..]);
                    displayed_len = accumulated.len();
                }
            }
        }

        if !batch.is_empty() && last_emit.elapsed() >= Duration::from_millis(BATCH_MS) {
            emit_token(&batch);
            batch.clear();
            last_emit = Instant::now();
        }
    }

    // Flush any remaining batch
    if !batch.is_empty() {
        emit_token(&batch);
        batch.clear();
    }

    // Edge case: if we were still undecided (e.g. only received `{` with no
    // dialogue field and no further tokens), treat it as plain text.
    if is_json.is_none() && !accumulated.is_empty() && displayed_len == 0 {
        emit_token(accumulated.trim());
    }

    accumulated
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn stream_json_dialogue_field() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue": "Hello world!"}"#.to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, r#"{"dialogue": "Hello world!"}"#);
        assert_eq!(collected, "Hello world!");
    }

    #[tokio::test]
    async fn stream_json_incremental() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue": "Hel"#.to_string()).unwrap();
        tx.send(r#"lo wor"#.to_string()).unwrap();
        tx.send(r#"ld!", "mood": "happy"}"#.to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert!(full.contains("Hello world!"));
        assert_eq!(collected, "Hello world!");
    }

    #[tokio::test]
    async fn stream_json_with_metadata_not_leaked() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(
            r#"{"dialogue": "Good morning!", "action": "nods", "mood": "friendly"}"#.to_string(),
        )
        .unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(collected, "Good morning!");
        assert!(!collected.contains("nods"));
        assert!(!collected.contains("friendly"));
    }

    #[tokio::test]
    async fn stream_plain_text_incremental() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Well, ".to_string()).unwrap();
        tx.send("good day ".to_string()).unwrap();
        tx.send("to ye".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Well, good day to ye");
        assert_eq!(collected, "Well, good day to ye");
    }

    #[tokio::test]
    async fn stream_plain_text_single_chunk() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Just plain text response.".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Just plain text response.");
        assert_eq!(collected, "Just plain text response.");
    }

    #[tokio::test]
    async fn stream_empty_channel() {
        let (tx, token_rx) = mpsc::unbounded_channel::<String>();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "");
        assert_eq!(collected, "");
    }

    #[tokio::test]
    async fn stream_json_with_escapes() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue": "He said \"hello\" to me"}"#.to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(collected, r#"He said "hello" to me"#);
    }

    #[tokio::test]
    async fn stream_json_with_unicode() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue": "Sláinte agus fáilte!"}"#.to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(collected, "Sláinte agus fáilte!");
    }

    #[tokio::test]
    async fn stream_json_empty_dialogue() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue": "", "mood": "silent"}"#.to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(collected, "");
    }

    #[tokio::test]
    async fn stream_json_no_space_after_colon() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send(r#"{"dialogue":"Compact JSON!"}"#.to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(collected, "Compact JSON!");
    }
}
