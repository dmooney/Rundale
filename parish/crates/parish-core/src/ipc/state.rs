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
/// - `parish-engine` uses a subset of this pattern but does not yet reference this type
#[derive(Clone)]
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
    /// The last raw player input / action submitted to the engine.
    ///
    /// Captured at the top of `handle_game_input` (before intent parsing) so a
    /// bug report can carry the exact text that triggered the failing turn
    /// (#1331). `None` until the player submits their first input. Runtime-only
    /// — never persisted to save state.
    pub last_player_input: Option<String>,
    /// Normalized first-sentence openers already shown to the player at the
    /// current location, across all turns (#1492 — cross-NPC opener de-dup).
    ///
    /// Populated by [`handle_npc_conversation`] from every NPC reply emitted
    /// at the current location. Cleared when the player moves (via
    /// [`Self::sync_location`]). Used to extend the same opener dedup that
    /// previously only worked within a single multi-NPC turn to also cover
    /// sequential single-NPC turns — catching "What brings ye to these parts?"
    /// repeated across 4+ separate conversations at the same location.
    ///
    /// Capped at 32 entries so it cannot grow unboundedly during a long stay at
    /// one location.
    pub seen_openers_this_location: Vec<String>,
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
            last_player_input: None,
            seen_openers_this_location: Vec::new(),
        }
    }

    /// Atomically claims the conversation for a new player- or idle-triggered
    /// NPC turn (#1379 turn serialization).
    ///
    /// Returns `true` and sets `conversation_in_progress` when no turn was
    /// already in flight; returns `false` (leaving the flag set) when a turn is
    /// already streaming, so the caller must abandon the late turn. Because the
    /// check and set happen under a single `&mut self` borrow (the caller holds
    /// the conversation mutex across both), two concurrent turns cannot both
    /// observe an idle conversation and proceed — only the first wins.
    ///
    /// On the abandon paths *before* streaming actually begins, the winner must
    /// call [`Self::end_turn`] to release the claim.
    pub fn try_begin_turn(&mut self) -> bool {
        if self.conversation_in_progress {
            return false;
        }
        self.conversation_in_progress = true;
        true
    }

    /// Releases the conversation claim taken by [`Self::try_begin_turn`].
    pub fn end_turn(&mut self) {
        self.conversation_in_progress = false;
    }

    /// Records the raw player input most recently submitted to the engine.
    ///
    /// Blank input is ignored so a stray empty submission never overwrites a
    /// meaningful prior action. Survives location changes — it is the last
    /// *action*, not a per-scene transcript line.
    pub fn record_player_input(&mut self, raw: &str) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            self.last_player_input = Some(trimmed.to_string());
        }
    }

    /// Clears the transcript and seen-opener set when the player moves to a new
    /// location (#1492).
    pub fn sync_location(&mut self, location: LocationId) {
        if self.location != Some(location) {
            self.location = Some(location);
            self.transcript.clear();
            self.seen_openers_this_location.clear();
        }
    }

    /// Records a normalized NPC opener shown to the player at the current
    /// location, for cross-turn cross-NPC opener de-duplication (#1492).
    ///
    /// Ignores empty strings. Caps the set at 32 entries (oldest evicted) so
    /// long stays at one location do not grow it unboundedly.
    pub fn record_opener(&mut self, opener: String) {
        if opener.is_empty() {
            return;
        }
        if let Some(pos) = self
            .seen_openers_this_location
            .iter()
            .position(|x| x == &opener)
        {
            // Already present — move to back (LRU refresh) so the eviction order
            // reflects recency rather than first-seen, and no duplicate is added.
            self.seen_openers_this_location.remove(pos);
        } else if self.seen_openers_this_location.len() >= 32 {
            self.seen_openers_this_location.remove(0);
        }
        self.seen_openers_this_location.push(opener);
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
    /// Browser-safe URL or data URL for the active mod's app icon override.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_icon_url: Option<String>,
    /// Browser-safe URL or data URL for the active mod's small favicon.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favicon_url: Option<String>,
    /// Map overlay style requested by the active mod, if any (e.g. `"grid"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_overlay: Option<String>,
    /// True if no base mod is currently loaded, indicating that the frontend
    /// should display a non-dismissable mod selector overlay to the user.
    #[serde(default)]
    pub base_mod_required: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::ConversationLine;

    fn line(text: impl Into<String>) -> ConversationLine {
        ConversationLine {
            speaker: "speaker".to_string(),
            text: text.into(),
        }
    }

    #[test]
    fn sync_location_clears_transcript_only_when_location_changes() {
        let mut state = ConversationRuntimeState::new();
        let crossroads = LocationId(1);
        let chapel = LocationId(2);

        state.sync_location(crossroads);
        state.push_line(line("hello at the crossroads"));
        state.sync_location(crossroads);
        assert_eq!(
            state.transcript.len(),
            1,
            "same-location sync should preserve local conversation context"
        );

        state.sync_location(chapel);
        assert_eq!(state.location, Some(chapel));
        assert!(
            state.transcript.is_empty(),
            "moving to a new location should clear stale local transcript"
        );
    }

    #[test]
    fn push_line_ignores_blank_text_and_caps_transcript() {
        let mut state = ConversationRuntimeState::new();

        state.push_line(line("   \n\t  "));
        assert!(state.transcript.is_empty());

        for idx in 0..14 {
            state.push_line(line(format!("line {idx}")));
        }

        assert_eq!(state.transcript.len(), 12);
        assert_eq!(state.transcript.front().unwrap().text, "line 2");
        assert_eq!(state.transcript.back().unwrap().text, "line 13");
    }
}
