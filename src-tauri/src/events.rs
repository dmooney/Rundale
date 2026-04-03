//! IPC event names and streaming bridge between Rust inference and Svelte frontend.

use std::time::{Duration, Instant};

use parish_core::npc::{
    LanguageHint, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
};
use tauri::Emitter;

// ── Event name constants ─────────────────────────────────────────────────────

/// Event emitted with each streamed NPC response token (batched).
pub const EVENT_STREAM_TOKEN: &str = "stream-token";
/// Event emitted when an NPC response stream ends.
pub const EVENT_STREAM_END: &str = "stream-end";
/// Event emitted to add a line to the chat log.
pub const EVENT_TEXT_LOG: &str = "text-log";
/// Event emitted when world state changes (movement, time tick).
pub const EVENT_WORLD_UPDATE: &str = "world-update";
/// Event emitted to show/hide the loading indicator.
pub const EVENT_LOADING: &str = "loading";
/// Event emitted every 500 ms with the current theme palette.
pub const EVENT_THEME_UPDATE: &str = "theme-update";
/// Event emitted every 2 s with a debug snapshot (only when debug panel is open).
pub const EVENT_DEBUG_UPDATE: &str = "debug-update";
/// Event emitted to tell the frontend to open the save picker modal.
pub const EVENT_SAVE_PICKER: &str = "save-picker";
/// Event emitted to toggle the full map overlay.
pub const EVENT_TOGGLE_MAP: &str = "toggle-full-map";
/// Event emitted when an NPC reacts to a message with an emoji.
pub const EVENT_NPC_REACTION: &str = "npc-reaction";

/// How many milliseconds to batch streaming tokens before emitting.
pub const BATCH_MS: u64 = 16;

// ── Payload types ────────────────────────────────────────────────────────────

/// Payload for `stream-token` events.
#[derive(serde::Serialize, Clone)]
pub struct StreamTokenPayload {
    /// The batch of token text to append to the current chat entry.
    pub token: String,
}

/// Payload for `stream-end` events.
#[derive(serde::Serialize, Clone)]
pub struct StreamEndPayload {
    /// Language hints extracted from the completed NPC response.
    pub hints: Vec<LanguageHint>,
}

/// Payload for `text-log` events.
#[derive(serde::Serialize, Clone)]
pub struct TextLogPayload {
    /// Unique message ID for reaction targeting.
    #[serde(default)]
    pub id: String,
    /// Who produced this text: "player", "system", or the NPC's name.
    pub source: String,
    /// The log entry text.
    pub content: String,
}

/// Payload for `npc-reaction` events.
#[derive(serde::Serialize, Clone)]
pub struct NpcReactionPayload {
    /// ID of the message being reacted to.
    pub message_id: String,
    /// The reaction emoji.
    pub emoji: String,
    /// Who reacted (NPC name).
    pub source: String,
}

/// Payload for `loading` events.
///
/// When `active` is `true`, the payload includes an animated spinner character,
/// a fun Irish-themed loading phrase, and an RGB colour for the spinner —
/// driven by [`parish_core::loading::LoadingAnimation`].
#[derive(serde::Serialize, Clone)]
pub struct LoadingPayload {
    /// Whether the loading indicator should be shown.
    pub active: bool,
    /// Current Celtic-cross spinner character (e.g. `"✛"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spinner: Option<String>,
    /// Current fun loading phrase (e.g. `"Consulting the sheep..."`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
    /// Spinner colour as `[R, G, B]`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
}

// ── Loading animation bridge ─────────────────────────────────────────────

/// Spawns a background task that emits [`LoadingPayload`] events with cycling
/// fun Irish phrases while the player waits for NPC inference.
///
/// Returns a [`tokio_util::sync::CancellationToken`] — drop or cancel it to
/// stop the animation loop and emit a final `active: false` event.
pub fn spawn_loading_animation(app: tauri::AppHandle, cancel: tokio_util::sync::CancellationToken) {
    tokio::spawn(async move {
        use parish_core::loading::LoadingAnimation;

        let mut anim = LoadingAnimation::new();

        // Emit an initial frame immediately
        anim.tick();
        let (r, g, b) = anim.current_color_rgb();
        let _ = app.emit(
            EVENT_LOADING,
            LoadingPayload {
                active: true,
                spinner: Some(anim.spinner_char().to_string()),
                phrase: Some(anim.phrase().to_string()),
                color: Some([r, g, b]),
            },
        );

        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                    anim.tick();
                    let (r, g, b) = anim.current_color_rgb();
                    let _ = app.emit(
                        EVENT_LOADING,
                        LoadingPayload {
                            active: true,
                            spinner: Some(anim.spinner_char().to_string()),
                            phrase: Some(anim.phrase().to_string()),
                            color: Some([r, g, b]),
                        },
                    );
                }
            }
        }

        // Final "off" event
        let _ = app.emit(
            EVENT_LOADING,
            LoadingPayload {
                active: false,
                spinner: None,
                phrase: None,
                color: None,
            },
        );
    });
}

// ── Streaming bridge ─────────────────────────────────────────────────────────

/// Reads tokens from `token_rx`, applies the NPC separator holdback logic,
/// batches them every [`BATCH_MS`] ms, and emits `stream-token` events.
///
/// Returns the full accumulated response text (including the hidden JSON
/// metadata section) so the caller can extract Irish word hints.
pub async fn stream_npc_response(
    app: tauri::AppHandle,
    mut token_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
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
                // Flush everything up to the separator
                if dialogue_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..dialogue_end]);
                }
                displayed_len = dialogue_end;
                separator_found = true;
            } else {
                // Hold back SEPARATOR_HOLDBACK bytes to avoid cutting mid-separator
                let raw_end = accumulated.len().saturating_sub(SEPARATOR_HOLDBACK);
                let safe_end = floor_char_boundary(&accumulated, raw_end);
                if safe_end > displayed_len {
                    batch.push_str(&accumulated[displayed_len..safe_end]);
                    displayed_len = safe_end;
                }
            }
        }
        // After separator is found we keep accumulating to capture the JSON
        // metadata but don't add anything more to the batch.

        if !batch.is_empty() && last_emit.elapsed() >= Duration::from_millis(BATCH_MS) {
            let _ = app.emit(
                EVENT_STREAM_TOKEN,
                StreamTokenPayload {
                    token: batch.clone(),
                },
            );
            batch.clear();
            last_emit = Instant::now();
        }
    }

    // Flush any remaining batch
    if !batch.is_empty() {
        let _ = app.emit(EVENT_STREAM_TOKEN, StreamTokenPayload { token: batch });
    }

    // Flush any remaining displayed text if no separator was ever found.
    // Strip trailing JSON metadata that the model may have appended without
    // a proper `---` separator (common with weaker/free models).
    if !separator_found && displayed_len < accumulated.len() {
        let remaining = &accumulated[displayed_len..];
        let clean = strip_trailing_json(remaining);
        if !clean.is_empty() {
            let _ = app.emit(
                EVENT_STREAM_TOKEN,
                StreamTokenPayload {
                    token: clean.to_string(),
                },
            );
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
fn strip_trailing_json(text: &str) -> &str {
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

    #[test]
    fn test_strip_trailing_json_with_json() {
        let text =
            "(Looks up) Ah, good morning to ye! {\"action\": \"speaks\", \"mood\": \"friendly\"}";
        assert_eq!(
            strip_trailing_json(text),
            "(Looks up) Ah, good morning to ye!"
        );
    }

    #[test]
    fn test_strip_trailing_json_no_json() {
        let text = "Well hello there, stranger!";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn test_strip_trailing_json_braces_in_dialogue() {
        // Curly braces in dialogue that aren't valid JSON should not be stripped
        let text = "The rent is {too high} says I.";
        assert_eq!(strip_trailing_json(text), text);
    }

    #[test]
    fn test_strip_trailing_json_with_newline_separator() {
        let text = "Good day to ye!\n{\"action\": \"nods\", \"mood\": \"content\"}";
        assert_eq!(strip_trailing_json(text), "Good day to ye!");
    }

    #[test]
    fn test_strip_trailing_json_empty() {
        assert_eq!(strip_trailing_json(""), "");
    }

    #[test]
    fn test_strip_trailing_json_only_json() {
        let text = "{\"action\": \"speaks\"}";
        assert_eq!(strip_trailing_json(text), "");
    }
}
