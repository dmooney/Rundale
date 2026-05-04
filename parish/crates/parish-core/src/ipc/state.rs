//! Shared runtime state types used by all three Parish backends.
//!
//! These types are byte-for-byte identical in the Tauri desktop backend
//! (`parish-tauri/src/lib.rs`) and the axum web server
//! (`parish-server/src/state.rs`). Moving them here is the first step of
//! #696 — eliminating game-loop triplication.

use std::time::Instant;

use crate::ipc::ConversationLine;
use crate::world::LocationId;

// ── ConversationRuntimeState ────────────────────────────────────────────────

/// Runtime conversation/session state used for multi-NPC continuity and idle
/// timers.
///
/// Shared across all three runtimes:
/// - `parish-tauri` (desktop Tauri app)
/// - `parish-server` (axum web server)
/// - `parish-cli` uses a subset of this pattern but does not yet reference this type
pub struct ConversationRuntimeState {
    /// Player location associated with the current transcript.
    pub location: Option<LocationId>,
    /// Recent dialogue at the current location (ring buffer, cap 12).
    pub transcript: std::collections::VecDeque<ConversationLine>,
    /// Last wall-clock moment when the player submitted input.
    pub last_player_activity: Instant,
    /// Last wall-clock moment when anyone said something in the local
    /// conversation.
    pub last_spoken_at: Instant,
    /// Whether a player- or idle-triggered NPC exchange is currently running.
    pub conversation_in_progress: bool,
}

impl Default for ConversationRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConversationRuntimeState {
    /// Creates a fresh state, initialising all timers to `Instant::now()`.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            location: None,
            transcript: std::collections::VecDeque::with_capacity(16),
            last_player_activity: now,
            last_spoken_at: now,
            conversation_in_progress: false,
        }
    }

    /// Clears the transcript when the player moves to a new location.
    pub fn sync_location(&mut self, location: LocationId) {
        if self.location != Some(location) {
            self.location = Some(location);
            self.transcript.clear();
        }
    }

    /// Appends a line to the local transcript, trimming blank lines and
    /// capping the ring buffer at 12 entries.
    pub fn push_line(&mut self, line: ConversationLine) {
        if line.text.trim().is_empty() {
            return;
        }
        if self.transcript.len() >= 12 {
            self.transcript.pop_front();
        }
        self.transcript.push_back(line);
    }
}

// ── SaveState ────────────────────────────────────────────────────────────────

/// Current save state for display in the status bar of the frontend.
///
/// Serialised and sent via `get_save_state` / `ui-config` IPC channels.
#[derive(serde::Serialize, Clone)]
pub struct SaveState {
    /// Filename of the current save file (e.g. `"parish_001.db"`), or `None`
    /// if the game has not been saved yet.
    pub filename: Option<String>,
    /// Database id of the current branch, or `None`.
    pub branch_id: Option<i64>,
    /// Human-readable name of the current branch, or `None`.
    pub branch_name: Option<String>,
}

// ── UiConfigSnapshot ─────────────────────────────────────────────────────────

/// UI configuration snapshot sent to the frontend on boot and on
/// `/api/ui-config`.
///
/// Sourced from the loaded [`GameMod`](crate::game_mod::GameMod)'s `ui.toml`
/// or the compiled-in defaults if no mod is loaded.
#[derive(serde::Serialize, Clone)]
pub struct UiConfigSnapshot {
    /// Label for the language-hints sidebar panel.
    pub hints_label: String,
    /// Default accent colour (CSS hex string, e.g. `"#c8a96e"`).
    pub default_accent: String,
    /// Splash text displayed on game start (Zork-style).
    pub splash_text: String,
    /// Id of the currently-active map tile source (matches a `tile_sources`
    /// key).
    pub active_tile_source: String,
    /// Registry of available map tile sources, sorted alphabetically by id.
    pub tile_sources: Vec<crate::ipc::TileSourceSnapshot>,
    /// How many seconds of inactivity before the game auto-pauses.
    pub auto_pause_timeout_seconds: u64,
}
