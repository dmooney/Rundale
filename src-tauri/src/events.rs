//! IPC event names and streaming bridge between Rust inference and Svelte frontend.

use std::time::{Duration, Instant};

use parish_core::npc::{
    IrishWordHint, SEPARATOR_HOLDBACK, find_response_separator, floor_char_boundary,
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
    /// Irish word hints extracted from the completed NPC response.
    pub hints: Vec<IrishWordHint>,
}

/// Payload for `text-log` events.
#[derive(serde::Serialize, Clone)]
pub struct TextLogPayload {
    /// Who produced this text: "player", "system", or the NPC's name.
    pub source: String,
    /// The log entry text.
    pub content: String,
}

/// Payload for `loading` events.
#[derive(serde::Serialize, Clone)]
pub struct LoadingPayload {
    /// Whether the loading indicator should be shown.
    pub active: bool,
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

    // Flush any remaining displayed text if no separator was ever found
    if !separator_found && displayed_len < accumulated.len() {
        let remaining = accumulated[displayed_len..].to_string();
        if !remaining.is_empty() {
            let _ = app.emit(EVENT_STREAM_TOKEN, StreamTokenPayload { token: remaining });
        }
    }

    accumulated
}
