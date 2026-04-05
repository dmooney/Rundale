//! IPC event names and streaming bridge between Rust inference and Svelte frontend.

use parish_core::npc::LanguageHint;
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
/// Event emitted when the player begins traveling between locations.
pub const EVENT_TRAVEL_START: &str = "travel-start";

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

// LoadingPayload is now shared via parish_core::ipc::LoadingPayload
pub use parish_core::ipc::LoadingPayload;

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
///
/// Delegates to [`parish_core::ipc::stream_npc_tokens`] for the core logic.
pub async fn stream_npc_response(
    app: tauri::AppHandle,
    token_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
) -> String {
    parish_core::ipc::stream_npc_tokens(token_rx, |batch| {
        let _ = app.emit(
            EVENT_STREAM_TOKEN,
            StreamTokenPayload {
                token: batch.to_string(),
            },
        );
    })
    .await
}
