//! Shared NPC token streaming logic for all frontends.
//!
//! Reads tokens from an inference channel, applies separator holdback logic,
//! batches them, and calls a user-provided emit function. This eliminates the
//! duplicate streaming implementations in `src-tauri/src/events.rs` and
//! `crates/parish-server/src/streaming.rs`.

use std::time::{Duration, Instant};

use crate::npc::{SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary};

/// How many milliseconds to batch streaming tokens before emitting.
pub const BATCH_MS: u64 = 16;

/// Reads tokens from `token_rx`, applies the NPC separator holdback logic,
/// batches them every [`BATCH_MS`] ms, and calls `emit_token` with each batch.
///
/// Returns the full accumulated response text (including the hidden JSON
/// metadata section) so the caller can extract Irish word hints.
///
/// The `emit_token` callback receives the batch text to display. Backends
/// wire this to their own event mechanism:
/// - Tauri: `app.emit("stream-token", StreamTokenPayload { token })`
/// - Web server: `bus.emit("stream-token", &StreamTokenPayload { token })`
/// - CLI: `print!("{}", token)`
pub async fn stream_npc_tokens(
    mut token_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    mut emit_token: impl FnMut(&str),
) -> String {
    let mut accumulated = String::new();
    let mut displayed_len: usize = 0;
    let mut separator_found = false;
    let mut batch = String::new();
    let mut last_emit = Instant::now();

    while let Some(token) = token_rx.recv().await {
        accumulated.push_str(&token);

        if !separator_found {
            if let Some((dialogue_end, _meta_start)) = find_response_separator(&accumulated) {
                if dialogue_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..dialogue_end]);
                }
                displayed_len = dialogue_end;
                separator_found = true;
            } else {
                let raw_end = accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                let safe_end = floor_char_boundary(&accumulated, raw_end);
                if safe_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..safe_end]);
                    displayed_len = safe_end;
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

    // Flush any remaining displayed text if no separator was ever found
    if !separator_found && displayed_len < accumulated.len() {
        let remaining = &accumulated[displayed_len..];
        let clean = strip_trailing_json(remaining);
        if !clean.is_empty() {
            emit_token(clean);
        }
    }

    accumulated
}

/// Strips trailing JSON metadata from a response that lacks a `---` separator.
///
/// Some weaker models emit the metadata JSON block directly after dialogue
/// without the expected `---` delimiter. This function finds the last
/// top-level `{...}` block at the end of the text and removes it, returning
/// only the dialogue portion. If no trailing JSON is found, returns the
/// original text trimmed.
pub fn strip_trailing_json(text: &str) -> &str {
    let trimmed = text.trim_end();
    if !trimmed.ends_with('}') {
        return trimmed;
    }
    // Walk backwards to find the matching opening brace
    let mut depth = 0i32;
    let mut json_start = None;
    for (i, ch) in trimmed.char_indices().rev() {
        match ch {
            '}' => depth += 1,
            '{' => {
                depth -= 1;
                if depth == 0 {
                    json_start = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    if let Some(start) = json_start {
        // Only strip if what we found actually parses as JSON
        let candidate = &trimmed[start..];
        if serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
            return trimmed[..start].trim_end();
        }
    }
    trimmed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn stream_simple_tokens() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Hello ".to_string()).unwrap();
        tx.send("world!".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Hello world!");
        assert_eq!(collected, "Hello world!");
    }

    #[tokio::test]
    async fn stream_with_separator() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Dialogue text\n---\n{\"hints\":[]}".to_string())
            .unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Dialogue text\n---\n{\"hints\":[]}");
        // Only dialogue portion should be emitted
        assert!(!collected.contains("hints"));
        assert!(collected.contains("Dialogue text"));
    }

    #[test]
    fn strip_trailing_json_with_json() {
        let text =
            "(Looks up) Ah, good morning to ye! {\"action\": \"speaks\", \"mood\": \"friendly\"}";
        assert_eq!(
            strip_trailing_json(text),
            "(Looks up) Ah, good morning to ye!"
        );
    }

    #[test]
    fn strip_trailing_json_no_json() {
        let text = "Well hello there, stranger!";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn strip_trailing_json_braces_in_dialogue() {
        let text = "The rent is {too high} says I.";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn strip_trailing_json_empty() {
        assert_eq!(strip_trailing_json(""), "");
    }

    #[test]
    fn strip_trailing_json_only_json() {
        let text = "{\"action\": \"speaks\"}";
        assert_eq!(strip_trailing_json(text), "");
    }

    #[tokio::test]
    async fn stream_utf8_multibyte_safety() {
        // Emojis are 4 bytes each. If they land near the SEPARATOR_HOLDBACK
        // boundary, floor_char_boundary must not split them.
        let (tx, token_rx) = mpsc::unbounded_channel();
        // Build a string long enough that the holdback window slices into
        // multi-byte territory: 30 chars of ASCII + emoji cluster.
        let text = "A".repeat(30) + "🎉🍀🎶";
        tx.send(text.clone()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, text, "full accumulation must preserve all bytes");
        // The emitted portion must be valid UTF-8 (no panic) and contain the
        // ASCII prefix. The emojis may or may not be emitted depending on
        // holdback, but whatever IS emitted must be valid.
        assert!(collected.starts_with("AAAAAA"));
    }

    #[tokio::test]
    async fn stream_pure_emoji_tokens() {
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("🎉".to_string()).unwrap();
        tx.send("🍀".to_string()).unwrap();
        tx.send("🎶".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "🎉🍀🎶");
        // Total emitted length (in chars) should be 3 — all emojis survive
        // even if batching delays some.
        assert_eq!(collected.chars().count(), 3);
    }

    // ── Additional coverage for stream_npc_tokens edge cases ────────────────

    #[tokio::test]
    async fn stream_empty_channel_returns_empty_string() {
        let (tx, token_rx) = mpsc::unbounded_channel::<String>();
        drop(tx); // Close immediately without sending.

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "");
        assert_eq!(collected, "");
    }

    #[tokio::test]
    async fn stream_separator_split_across_tokens() {
        // Receiver must stitch the separator together even when it arrives in pieces.
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Dialogue text\n".to_string()).unwrap();
        tx.send("--".to_string()).unwrap();
        tx.send("-\n".to_string()).unwrap();
        tx.send("{\"hints\":[]}".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Dialogue text\n---\n{\"hints\":[]}");
        // No JSON metadata should have leaked into the collected output.
        assert!(!collected.contains("hints"));
        assert!(collected.contains("Dialogue text"));
    }

    #[tokio::test]
    async fn stream_handles_multibyte_utf8_at_holdback_boundary() {
        // The holdback window must never land inside a multi-byte char.
        // Build a long dialogue so the sliding window actually engages, with
        // Irish accented characters (é, á) around the boundary.
        let (tx, token_rx) = mpsc::unbounded_channel();
        let line = "Is fíor-álainn an lá é inniu — gealltanach agus éadrom, caithfidh mé a rá.";
        tx.send(line.to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, line);
        // Output must be valid UTF-8 and contain the full phrase.
        assert!(collected.contains("fíor-álainn"));
    }

    #[tokio::test]
    async fn stream_without_separator_strips_trailing_json() {
        // Weak models sometimes omit the --- separator and emit metadata inline.
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("A fine morning it is. ".to_string()).unwrap();
        tx.send("{\"action\":\"speaks\"}".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let _ = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        // The trailing JSON must be stripped from the emitted text.
        assert!(collected.contains("fine morning"));
        assert!(!collected.contains("action"));
    }

    #[tokio::test]
    async fn stream_short_text_under_holdback_window_still_emits() {
        // Text shorter than SEPARATOR_HOLDBACK should flush through the
        // "no separator found" tail path.
        let (tx, token_rx) = mpsc::unbounded_channel();
        tx.send("Hi.".to_string()).unwrap();
        drop(tx);

        let mut collected = String::new();
        let full = stream_npc_tokens(token_rx, |batch| collected.push_str(batch)).await;
        assert_eq!(full, "Hi.");
        assert_eq!(collected, "Hi.");
    }

    // ── strip_trailing_json edge cases ──────────────────────────────────────

    #[test]
    fn strip_trailing_json_ignores_unmatched_close_brace() {
        // A lone '}' with no matching '{' should not crash; text is returned.
        let text = "punctuation is weird}";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn strip_trailing_json_rejects_invalid_json() {
        // The candidate block looks like JSON but isn't valid — keep the text.
        let text = "the deal is {not, a: valid}";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn strip_trailing_json_handles_whitespace_before_json() {
        let text = "Good evening to ye.   {\"a\":1}";
        assert_eq!(strip_trailing_json(text), "Good evening to ye.");
    }

    #[test]
    fn strip_trailing_json_handles_nested_objects() {
        let text = r#"(smiles) Welcome home. {"action":"speaks","meta":{"mood":"warm"}}"#;
        assert_eq!(strip_trailing_json(text), "(smiles) Welcome home.");
    }
}
