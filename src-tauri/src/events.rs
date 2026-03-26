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

    // Flush any remaining displayed text if no separator was ever found
    if !separator_found && displayed_len < accumulated.len() {
        let remaining = accumulated[displayed_len..].to_string();
        if !remaining.is_empty() {
            let _ = app.emit(EVENT_STREAM_TOKEN, StreamTokenPayload { token: remaining });
        }
    }

    accumulated
}
