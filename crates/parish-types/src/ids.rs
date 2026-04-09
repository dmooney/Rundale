//! Core identifier types and NPC string-processing utilities.
//!
//! Contains the foundational ID newtypes (`LocationId`, `NpcId`),
//! the `Location` struct, `LanguageHint`, and streaming separator helpers
//! extracted from the world and NPC modules.

use serde::{Deserialize, Serialize};

/// Unique identifier for a location in the world graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocationId(pub u32);

/// Unique identifier for an NPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NpcId(pub u32);

/// A named location in the game world.
///
/// Locations are nodes in the world graph. Each has a textual
/// description, flags indicating whether it is indoors and/or
/// public, and geographic coordinates for map placement.
#[derive(Debug, Clone)]
pub struct Location {
    /// Unique identifier.
    pub id: LocationId,
    /// Human-readable name (e.g. "The Crossroads").
    pub name: String,
    /// Prose description shown when the player arrives.
    pub description: String,
    /// Whether this location is indoors.
    pub indoor: bool,
    /// Whether this location is publicly accessible.
    pub public: bool,
    /// Latitude in decimal degrees (WGS 84).
    pub lat: f64,
    /// Longitude in decimal degrees (WGS 84).
    pub lon: f64,
}

/// A pronunciation hint for a word in the setting's secondary language.
///
/// Extracted from NPC response metadata and displayed in the
/// pronunciation sidebar to help players with unfamiliar words.
/// The mod's prompt template instructs the LLM to produce these.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct LanguageHint {
    /// The word as it appears in text.
    pub word: String,
    /// Approximate English phonetic pronunciation.
    pub pronunciation: String,
    /// English meaning or gloss.
    #[serde(default)]
    pub meaning: Option<String>,
}

/// Backward-compatible alias for [`LanguageHint`].
pub type IrishWordHint = LanguageHint;

/// Maximum number of bytes to hold back during streaming to detect
/// the separator pattern. Must be large enough to catch ` --- ` inline
/// or `  ---\n` on its own line, even when preceded by text on the same line.
pub const SEPARATOR_HOLDBACK: usize = 24;

/// Rounds a byte offset down to the nearest UTF-8 char boundary in `s`.
///
/// If `pos` is already a char boundary, returns it unchanged. Otherwise
/// scans backwards to the nearest valid boundary. Returns 0 if no valid
/// boundary is found before `pos`.
pub fn floor_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// Finds the separator between dialogue and metadata in an NPC response.
///
/// Looks for `---` as a separator. Handles two cases:
/// 1. `---` on its own line (with optional whitespace)
/// 2. `---` appearing inline, e.g. `(smiles) --- {"action": ...}`
///
/// Returns `Some((dialogue_end, metadata_start))` — the byte offset where
/// dialogue ends and where metadata begins (after the separator).
/// Returns `None` if no separator is found.
pub fn find_response_separator(text: &str) -> Option<(usize, usize)> {
    // First try: --- on its own line
    let mut byte_offset = 0;
    for line in text.split('\n') {
        if line.trim() == "---" {
            let dialogue_end = byte_offset;
            let metadata_start = (byte_offset + line.len() + 1).min(text.len());
            return Some((dialogue_end, metadata_start));
        }
        byte_offset += line.len() + 1; // +1 for the \n
    }

    // Second try: --- appearing inline (e.g. "text --- {json}")
    // Look for " --- " or " ---\n" pattern
    if let Some(pos) = text.find(" --- ") {
        let dialogue_end = pos;
        let metadata_start = pos + 5; // skip " --- "
        return Some((dialogue_end, metadata_start));
    }
    if let Some(pos) = text.find(" ---\n") {
        let dialogue_end = pos;
        let metadata_start = pos + 5;
        return Some((dialogue_end, metadata_start));
    }

    None
}
