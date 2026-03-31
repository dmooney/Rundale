//! Serializable IPC types shared between all Parish frontends.
//!
//! These types are sent over Tauri IPC (desktop) or HTTP/WebSocket (web).
//! All fields use `snake_case` (serde defaults) to match the TypeScript
//! interfaces in `ui/src/lib/types.ts`.

use serde::{Deserialize, Serialize};

use crate::npc::IrishWordHint;
use crate::world::palette::{RawColor, RawPalette};

// ── World snapshot ──────────────────────────────────────────────────────────

/// A serializable snapshot of the world state sent to the frontend.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorldSnapshot {
    /// Name of the player's current location.
    pub location_name: String,
    /// Short prose description of the current location.
    pub location_description: String,
    /// Human-readable time label (e.g. "Morning", "Dusk").
    pub time_label: String,
    /// Current game hour (0–23).
    pub hour: u8,
    /// Current game minute (0–59).
    pub minute: u8,
    /// Current weather description.
    pub weather: String,
    /// Current season name.
    pub season: String,
    /// Optional festival name if today is a festival day.
    pub festival: Option<String>,
    /// Whether the game clock is currently paused.
    pub paused: bool,
    /// Game time as milliseconds since Unix epoch (for client-side interpolation).
    pub game_epoch_ms: f64,
    /// Clock speed multiplier (1 real second = speed_factor game seconds).
    pub speed_factor: f64,
}

// ── Map data ────────────────────────────────────────────────────────────────

/// A location node in the map data.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MapLocation {
    /// Location ID as a string.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// WGS-84 latitude (0.0 if not geocoded).
    pub lat: f64,
    /// WGS-84 longitude (0.0 if not geocoded).
    pub lon: f64,
    /// Whether this location is adjacent to (or is) the player's position.
    pub adjacent: bool,
}

/// The full map graph sent to the frontend.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MapData {
    /// All locations in the graph.
    pub locations: Vec<MapLocation>,
    /// Edges as (source_id, target_id) string pairs.
    pub edges: Vec<(String, String)>,
    /// The player's current location id.
    pub player_location: String,
}

// ── NPC info ────────────────────────────────────────────────────────────────

/// Minimal NPC info for the sidebar.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NpcInfo {
    /// Display name (full name if introduced, brief description otherwise).
    pub name: String,
    /// NPC's occupation.
    pub occupation: String,
    /// NPC's current mood.
    pub mood: String,
    /// Whether the player has been introduced to this NPC.
    pub introduced: bool,
}

// ── Theme palette ───────────────────────────────────────────────────────────

/// CSS hex-string theme palette derived from [`RawPalette`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ThemePalette {
    /// Main background colour (`"#rrggbb"`).
    pub bg: String,
    /// Foreground (text) colour.
    pub fg: String,
    /// Accent colour for highlights and the status bar.
    pub accent: String,
    /// Slightly offset panel background.
    pub panel_bg: String,
    /// Input field background.
    pub input_bg: String,
    /// Border/separator colour.
    pub border: String,
    /// Muted colour for secondary text.
    pub muted: String,
}

impl From<RawPalette> for ThemePalette {
    fn from(raw: RawPalette) -> Self {
        let hex = |c: RawColor| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b);
        ThemePalette {
            bg: hex(raw.bg),
            fg: hex(raw.fg),
            accent: hex(raw.accent),
            panel_bg: hex(raw.panel_bg),
            input_bg: hex(raw.input_bg),
            border: hex(raw.border),
            muted: hex(raw.muted),
        }
    }
}

// ── Event payloads ──────────────────────────────────────────────────────────

/// Payload for `stream-token` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamTokenPayload {
    /// The batch of token text to append to the current chat entry.
    pub token: String,
}

/// Payload for `stream-end` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamEndPayload {
    /// Irish word hints extracted from the completed NPC response.
    pub hints: Vec<IrishWordHint>,
}

/// Payload for `text-log` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TextLogPayload {
    /// Who produced this text: "player", "system", or the NPC's name.
    pub source: String,
    /// The log entry text.
    pub content: String,
}

/// Payload for `loading` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LoadingPayload {
    /// Whether the loading indicator should be shown.
    pub active: bool,
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_palette_from_raw_palette() {
        let raw = RawPalette {
            bg: RawColor::new(10, 20, 30),
            fg: RawColor::new(200, 210, 220),
            accent: RawColor::new(255, 128, 0),
            panel_bg: RawColor::new(15, 25, 35),
            input_bg: RawColor::new(20, 30, 40),
            border: RawColor::new(50, 60, 70),
            muted: RawColor::new(100, 110, 120),
        };
        let palette = ThemePalette::from(raw);
        assert_eq!(palette.bg, "#0a141e");
        assert_eq!(palette.fg, "#c8d2dc");
        assert_eq!(palette.accent, "#ff8000");
    }

    #[test]
    fn world_snapshot_serialization_round_trip() {
        let snap = WorldSnapshot {
            location_name: "Crossroads".to_string(),
            location_description: "A dusty crossroads.".to_string(),
            time_label: "Morning".to_string(),
            hour: 8,
            minute: 30,
            weather: "Clear".to_string(),
            season: "Summer".to_string(),
            festival: None,
            paused: false,
            game_epoch_ms: 1234567890.0,
            speed_factor: 36.0,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let deser: WorldSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.location_name, "Crossroads");
        assert_eq!(deser.hour, 8);
    }

    #[test]
    fn map_data_serialization() {
        let data = MapData {
            locations: vec![MapLocation {
                id: "1".to_string(),
                name: "Church".to_string(),
                lat: 53.0,
                lon: -7.0,
                adjacent: true,
            }],
            edges: vec![("1".to_string(), "2".to_string())],
            player_location: "1".to_string(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("Church"));
    }

    #[test]
    fn npc_info_serialization() {
        let info = NpcInfo {
            name: "Seán".to_string(),
            occupation: "Farmer".to_string(),
            mood: "content".to_string(),
            introduced: true,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deser: NpcInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "Seán");
    }

    #[test]
    fn event_payload_serialization() {
        let token = StreamTokenPayload {
            token: "hello".to_string(),
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("hello"));

        let log = TextLogPayload {
            source: "system".to_string(),
            content: "Welcome".to_string(),
        };
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("system"));

        let loading = LoadingPayload { active: true };
        let json = serde_json::to_string(&loading).unwrap();
        assert!(json.contains("true"));
    }
}
