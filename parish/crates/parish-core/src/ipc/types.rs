//! Serializable IPC types shared between all Parish frontends.
//!
//! These types are sent over Tauri IPC (desktop) or HTTP/WebSocket (web).
//! All fields use `snake_case` (serde defaults) to match the TypeScript
//! interfaces in `ui/src/lib/types.ts`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::npc::LanguageHint;

// ── World snapshot ──────────────────────────────────────────────────────────

/// Authoritative, player-visible projection of one durable task.
///
/// The domain ledger owns lifecycle transitions and bounds text; this DTO only
/// converts stable IDs into frontend-friendly primitives. Completed tasks are
/// intentionally omitted from live snapshots.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlayerTaskSnapshot {
    /// Stable task identifier.
    pub id: u64,
    /// Bounded description of the assigned work.
    pub description: String,
    /// Stable ID of the NPC who assigned the task.
    pub assigned_by: u32,
    /// Stable location ID where the task can be advanced.
    pub location_id: u32,
    /// Current authoritative lifecycle status.
    pub status: parish_types::TaskStatus,
    /// Game time when the task was assigned.
    pub assigned_at: DateTime<Utc>,
    /// Game time when the task first entered `in_progress`.
    pub started_at: Option<DateTime<Utc>>,
    /// Game time when the task was explicitly completed.
    pub completed_at: Option<DateTime<Utc>>,
    /// Most recent bounded player action accepted as relevant to this task.
    pub last_matching_action: Option<String>,
}

impl From<&parish_types::PlayerTask> for PlayerTaskSnapshot {
    fn from(task: &parish_types::PlayerTask) -> Self {
        Self {
            id: task.id.0,
            description: task.description.clone(),
            assigned_by: task.assigned_by.0,
            location_id: task.location.0,
            status: task.status,
            assigned_at: task.assigned_at,
            started_at: task.started_at,
            completed_at: task.completed_at,
            last_matching_action: task.last_matching_action.clone(),
        }
    }
}

impl PlayerTaskSnapshot {
    /// Canonical status label used by text-only IPC consumers.
    pub const fn status_label(&self) -> &'static str {
        match self.status {
            parish_types::TaskStatus::Assigned => "assigned",
            parish_types::TaskStatus::InProgress => "in_progress",
            parish_types::TaskStatus::Completed => "completed",
        }
    }
}

/// A serializable snapshot of the world state sent to the frontend.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WorldSnapshot {
    /// Canonical ID of the player's current location.
    ///
    /// This travels with the prose snapshot so presentation clients do not
    /// have to infer scene identity from an independently-refreshed map.
    #[serde(default)]
    pub location_id: u32,
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
    /// Whether the game clock is currently player-paused.
    pub paused: bool,
    /// Whether the game clock is frozen while waiting on inference.
    #[serde(default)]
    pub inference_paused: bool,
    /// Game time as milliseconds since Unix epoch (for client-side interpolation).
    pub game_epoch_ms: f64,
    /// Clock speed multiplier (1 real second = speed_factor game seconds).
    pub speed_factor: f64,
    /// Pronunciation hints for Irish names relevant to the current location.
    #[serde(default)]
    pub name_hints: Vec<LanguageHint>,
    /// Active durable tasks, oldest assignment first.
    ///
    /// Completed history remains in authoritative state/save data but is not
    /// projected into the live player-status surface.
    #[serde(default)]
    pub active_tasks: Vec<PlayerTaskSnapshot>,
    /// Current day of week (e.g. "Monday", "Saturday").
    pub day_of_week: String,
    /// Whether an NPC conversation turn is currently being processed by the
    /// engine. Surfaced so the web frontend can re-assert `streamingActive`
    /// from authoritative state after a WebSocket reconnect — otherwise a turn
    /// that is in flight but has not yet emitted its next `stream-token`
    /// (slow model, long pause) would leave the input field and quick-travel
    /// chips usable, opening a duplicate-turn window (#1164). Defaults to
    /// `false`; only the snapshot the reconnect resync re-fetches sets it from
    /// `ConversationRuntimeState::conversation_in_progress`.
    #[serde(default)]
    pub turn_in_flight: bool,
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
    /// Number of graph hops from the player's current location.
    #[serde(default)]
    pub hops: u32,
    /// Whether this location is indoors (for tooltip display).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indoor: Option<bool>,
    /// Estimated walking time from the player's current location, in minutes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub travel_minutes: Option<u16>,
    /// Whether the player has visited this location (false = fog-of-war frontier).
    #[serde(default = "default_true")]
    pub visited: bool,
}

fn default_true() -> bool {
    true
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
    /// Edge traversal counts for footprint rendering.
    ///
    /// Each entry is `(source_id, target_id, count)` where the edge is
    /// canonically ordered (smaller id first). Higher counts render as
    /// thicker/lighter "worn path" lines on the map.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_traversals: Vec<(String, String, u32)>,
    /// Human-readable transport mode label (e.g. `"on foot"`).
    pub transport_label: String,
    /// Machine identifier for the active transport mode (e.g. `"walking"`).
    pub transport_id: String,
}

// ── NPC info ────────────────────────────────────────────────────────────────

/// Minimal NPC info for the sidebar.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NpcInfo {
    /// Display name (full name if introduced, brief description otherwise).
    pub name: String,
    /// Canonical real name, used as a stable id for chip dispatch.
    #[serde(default)]
    pub real_name: String,
    /// NPC's occupation.
    pub occupation: String,
    /// NPC's current mood.
    pub mood: String,
    /// Whether the player has been introduced to this NPC.
    pub introduced: bool,
    /// Emoji representation of the mood.
    pub mood_emoji: String,
}

/// One source-consistent replacement payload for reconnecting clients.
///
/// All fields are projected from the same locked world/NPC generation.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ReconnectState {
    /// Canonical player-visible world snapshot.
    pub world: WorldSnapshot,
    /// Map projection from the same world generation.
    pub map: MapData,
    /// Co-located NPC projection from the same world/NPC generation.
    pub npcs: Vec<NpcInfo>,
    /// Process-local context generation used to reject stale presentation data.
    pub context_epoch: u64,
}

// ── Theme palette ───────────────────────────────────────────────────────────

/// CSS hex-string theme palette derived from [`parish_palette::RawPalette`].
///
/// The struct now lives in `parish-types` (the zero-dependency leaf) so the
/// mod loader (`parish-mod`) and this IPC layer can both name it without a
/// dependency cycle. This re-export preserves the historical
/// `parish_core::ipc::ThemePalette` path for every consumer. The
/// `From<RawPalette>` conversion lives in `parish-palette` (where `RawPalette`
/// is local) and is always in scope wherever `RawPalette` is.
pub use parish_types::ThemePalette;

// ── Event payloads ──────────────────────────────────────────────────────────

/// Payload for `stream-token` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamTokenPayload {
    /// The batch of token text to append to the current chat entry.
    pub token: String,
    /// Stable ID for the NPC turn this token batch belongs to. This is the
    /// same value as the placeholder `text-log` entry's `stream_turn_id`, so
    /// the client keys streaming entries on it.
    pub turn_id: u64,
    /// Speaker label for this stream turn.
    pub source: String,
    /// Message id of the `text-log` placeholder this stream fills, when known.
    ///
    /// Carried so a stream that *resumes after a WebSocket reconnect* can
    /// rebind to a reactable `textLog` entry: `StreamManager.reset()` discards
    /// the client's only copy of the placeholder id during the gap, and
    /// without it the rebuilt NPC bubble has no `entry.id` (non-reactable, its
    /// language hints unkeyed) (#1164). Populated for player-initiated NPC
    /// conversation turns; `None` for arrival-reaction streams (which generate
    /// their placeholder id inside the per-runtime emit closure and have no
    /// reconnect-resume contract).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

/// Payload for `stream-turn-end` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamTurnEndPayload {
    /// Stable ID for the NPC turn that has finished streaming tokens.
    pub turn_id: u64,
}

/// Payload for `stream-end` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StreamEndPayload {
    /// Irish word hints extracted from the completed NPC response.
    pub hints: Vec<LanguageHint>,
}

/// Payload for `dialogue-corrected` events.
///
/// Emitted after all post-generation guards run on an NPC turn's `dialogue`
/// field, but only when at least one guard altered the raw model output
/// (i.e. the stored/transcript text differs from what was streamed token-by-token).
/// The UI must replace the accumulated streamed content for `turn_id` with
/// `corrected_text` so the player sees the post-guard canonical dialogue —
/// the same text stored in the conversation log and returned by `/api/transcript`
/// (#1552).
///
/// The event is emitted after `stream-turn-end` (so the stream pump has already
/// seen the raw tokens) and before `stream-end` (so the pump can still be in
/// progress draining its buffer — the handler must flush/replace immediately).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DialogueCorrectedPayload {
    /// Stable ID matching the `turn_id` from the `stream-token` events for this turn.
    pub turn_id: u64,
    /// The post-guard canonical dialogue text.
    pub corrected_text: String,
    /// Stable message ID carried through from the original placeholder (`text-log`
    /// with `stream_turn_id`). Lets the frontend locate the entry by id rather than
    /// by `stream_turn_id` (which is cleared after finalization), avoiding a missed
    /// replacement when `dialogue-corrected` arrives after the stream pump has
    /// already finalized the entry (#1552 parity).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
}

/// Payload for `text-log` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TextLogPayload {
    /// Unique message ID for reaction targeting.
    #[serde(default)]
    pub id: String,
    /// Stable ID for the NPC turn this placeholder belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_turn_id: Option<u64>,
    /// Who produced this text: "player", "system", or the NPC's name.
    pub source: String,
    /// The log entry text.
    pub content: String,
    /// Optional semantic subtype for styling (e.g. `"location"` for arrival descriptions).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtype: Option<String>,
}

/// Payload for `npc-reaction` events.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NpcReactionPayload {
    /// ID of the message being reacted to.
    pub message_id: String,
    /// The reaction emoji.
    pub emoji: String,
    /// Who reacted (NPC name).
    pub source: String,
}

/// Request body for the react-to-message endpoint.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ReactRequest {
    /// Name of the NPC whose message is being reacted to.
    pub npc_name: String,
    /// First ~80 chars of the message being reacted to.
    pub message_snippet: String,
    /// The reaction emoji.
    pub emoji: String,
}

/// Payload for `loading` events.
///
/// When `active` is `true`, the payload may include an animated spinner
/// character, a fun Irish-themed loading phrase, and an RGB colour —
/// driven by [`crate::loading::LoadingAnimation`].
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LoadingPayload {
    /// Whether the loading indicator should be shown.
    pub active: bool,
    /// Current Celtic-cross spinner character (e.g. `"✛"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spinner: Option<String>,
    /// Current fun loading phrase (e.g. `"Consulting the sheep..."`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phrase: Option<String>,
    /// Spinner colour as `[R, G, B]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 3]>,
}

/// A waypoint along a travel path, with screen-friendly coordinates.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TravelWaypoint {
    /// Location ID at this waypoint.
    pub id: String,
    /// WGS-84 latitude.
    pub lat: f64,
    /// WGS-84 longitude.
    pub lon: f64,
}

/// Payload for `travel-start` events, emitted when the player begins moving.
///
/// The frontend uses this to animate a moving dot along the path on the map.
/// The `from` / `to` location names are also consumed by the synchronous
/// `/api/command` drain to populate the response's `travel` field.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TravelStartPayload {
    /// Origin location name (player's location before the move).
    #[serde(default)]
    pub from: String,
    /// Destination location name.
    #[serde(default)]
    pub to: String,
    /// Ordered waypoints from origin to destination (including both endpoints).
    pub waypoints: Vec<TravelWaypoint>,
    /// Total travel duration in game minutes.
    pub duration_minutes: u16,
    /// Destination location ID.
    pub destination: String,
}

// ── Map tile source snapshot ────────────────────────────────────────────────

/// Frontend-facing description of a single tile source.
///
/// Mirrors `parish_config::engine::TileSourceConfig` with the `id` key added
/// so the frontend can build a registry without its own lookup logic. Sent
/// inside the `UiConfigSnapshot` on boot and used by the `/tiles` slash
/// command to render the listing.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TileSourceSnapshot {
    /// Registry key (e.g. "osm", "historic-6inch").
    pub id: String,
    /// Human-readable label shown in `/tiles` listings.
    pub label: String,
    /// XYZ URL template (empty string means "not yet configured").
    pub url: String,
    /// Tile edge length in pixels.
    pub tile_size: u32,
    /// Minimum zoom the source serves tiles for.
    pub minzoom: u32,
    /// Maximum zoom the source serves tiles for.
    pub maxzoom: u32,
    /// Attribution text for MapLibre's attribution control.
    pub attribution: String,
    /// MapLibre `raster-saturation` paint value.
    pub raster_saturation: f32,
    /// MapLibre `raster-opacity` paint value.
    pub raster_opacity: f32,
    /// When true, the frontend sets `scheme: 'tms'` on the source.
    pub tms: bool,
}

impl TileSourceSnapshot {
    /// Builds the frontend-facing list from a `MapConfig`, alphabetical by id.
    ///
    /// Call this at backend boot to populate `UiConfigSnapshot::tile_sources`.
    ///
    /// `has_tile_proxy` selects which URL the frontend will fetch:
    /// - `true`  (parish-server): use `TileSourceConfig::url` — the same-origin
    ///   `/tiles/{id}/...` proxy path served by `tile_routes::get_tile`.
    /// - `false` (parish-tauri / any runtime without a proxy): substitute
    ///   `upstream_url` when set, since the webview has no `/tiles/` handler
    ///   and a proxy path would 404 (regression after PR #955).
    pub fn list_from_map_config(cfg: &parish_config::MapConfig, has_tile_proxy: bool) -> Vec<Self> {
        cfg.tile_sources
            .iter()
            .map(|(id, src)| {
                let url = match (has_tile_proxy, src.upstream_url.as_str()) {
                    // Server hosts the /tiles/ proxy: keep same-origin url.
                    (true, _) => src.url.clone(),
                    // No proxy + no upstream_url (e.g. OSM): keep direct url.
                    (false, "") => src.url.clone(),
                    // No proxy + upstream_url set (e.g. historic S3): substitute.
                    (false, _) => src.upstream_url.clone(),
                };
                Self {
                    id: id.clone(),
                    label: src.label.clone(),
                    url,
                    tile_size: src.tile_size,
                    minzoom: src.minzoom,
                    maxzoom: src.maxzoom,
                    attribution: src.attribution.clone(),
                    raster_saturation: src.raster_saturation,
                    raster_opacity: src.raster_opacity,
                    tms: src.tms,
                }
            })
            .collect()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_snapshot_proxy_mode_uses_url() {
        let cfg = parish_config::MapConfig::default();
        let list = TileSourceSnapshot::list_from_map_config(&cfg, true);
        let historic = list.iter().find(|s| s.id == "historic").expect("historic");
        assert!(
            historic.url.starts_with("/tiles/historic/"),
            "proxy mode must keep the same-origin /tiles/ path, got {:?}",
            historic.url
        );
    }

    #[test]
    fn tile_snapshot_no_proxy_substitutes_upstream() {
        // Regression for Tauri runtime (no /tiles/ route): the snapshot must
        // hand MapLibre an absolute upstream URL, not a dead proxy path.
        let cfg = parish_config::MapConfig::default();
        let list = TileSourceSnapshot::list_from_map_config(&cfg, false);
        let historic = list.iter().find(|s| s.id == "historic").expect("historic");
        assert!(
            historic.url.starts_with("https://"),
            "no-proxy mode must substitute upstream_url, got {:?}",
            historic.url
        );
        assert!(
            historic.url.contains("mapseries-tilesets.s3.amazonaws.com"),
            "expected NLS S3 upstream, got {:?}",
            historic.url
        );
        // OSM has no upstream_url (browser fetches directly) — url must be kept.
        let osm = list.iter().find(|s| s.id == "osm").expect("osm");
        assert!(
            osm.url.starts_with("https://tile.openstreetmap.org/"),
            "OSM url should be passed through unchanged, got {:?}",
            osm.url
        );
    }

    #[test]
    fn theme_palette_from_raw_palette() {
        use parish_palette::{RawColor, RawPalette};
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
            location_id: 1,
            location_name: "Crossroads".to_string(),
            location_description: "A dusty crossroads.".to_string(),
            time_label: "Morning".to_string(),
            hour: 8,
            minute: 30,
            weather: "Clear".to_string(),
            season: "Summer".to_string(),
            festival: None,
            paused: false,
            inference_paused: false,
            game_epoch_ms: 1234567890.0,
            speed_factor: 36.0,
            name_hints: vec![],
            active_tasks: vec![],
            day_of_week: "Monday".to_string(),
            turn_in_flight: false,
        };
        let json = serde_json::to_string(&snap).unwrap();
        let deser: WorldSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.location_id, 1);
        assert_eq!(deser.location_name, "Crossroads");
        assert_eq!(deser.hour, 8);
        assert!(!deser.turn_in_flight);
        assert!(deser.active_tasks.is_empty());

        let mut legacy: serde_json::Value = serde_json::from_str(&json).unwrap();
        legacy.as_object_mut().unwrap().remove("location_id");
        legacy.as_object_mut().unwrap().remove("active_tasks");
        let legacy: WorldSnapshot = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            legacy.location_id, 0,
            "pre-location-id snapshots must deserialize with the neutral sentinel"
        );
        assert!(
            legacy.active_tasks.is_empty(),
            "pre-task snapshots must deserialize with an empty ledger"
        );
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
                hops: 0,
                indoor: Some(true),
                travel_minutes: Some(5),
                visited: true,
            }],
            edges: vec![("1".to_string(), "2".to_string())],
            player_location: "1".to_string(),
            edge_traversals: vec![],
            transport_label: "on foot".to_string(),
            transport_id: "walking".to_string(),
        };
        let json = serde_json::to_string(&data).unwrap();
        assert!(json.contains("Church"));
    }

    #[test]
    fn npc_info_serialization() {
        let info = NpcInfo {
            name: "Seán".to_string(),
            real_name: "Seán Ó Briain".to_string(),
            occupation: "Farmer".to_string(),
            mood: "content".to_string(),
            introduced: true,
            mood_emoji: "😌".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deser: NpcInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.name, "Seán");
        assert_eq!(deser.real_name, "Seán Ó Briain");
    }

    #[test]
    fn event_payload_serialization() {
        let token = StreamTokenPayload {
            token: "hello".to_string(),
            turn_id: 7,
            source: "Siobhan Murphy".to_string(),
            message_id: Some("msg-7".to_string()),
        };
        let json = serde_json::to_string(&token).unwrap();
        assert!(json.contains("hello"));
        assert!(json.contains("turn_id"));
        assert!(json.contains("message_id"));
        assert!(json.contains("msg-7"));

        // `message_id` is omitted from the wire when `None` (arrival-reaction
        // streams) and deserializes back to `None` for older payloads.
        let no_id = StreamTokenPayload {
            token: "hi".to_string(),
            turn_id: 8,
            source: "Peig".to_string(),
            message_id: None,
        };
        let json_no_id = serde_json::to_string(&no_id).unwrap();
        assert!(!json_no_id.contains("message_id"));
        let deser: StreamTokenPayload = serde_json::from_str(&json_no_id).unwrap();
        assert!(deser.message_id.is_none());

        let log = TextLogPayload {
            id: "msg-1".to_string(),
            stream_turn_id: Some(7),
            source: "system".to_string(),
            content: "Welcome".to_string(),
            subtype: None,
        };
        let json = serde_json::to_string(&log).unwrap();
        assert!(json.contains("system"));

        let loading = LoadingPayload {
            active: true,
            spinner: None,
            phrase: None,
            color: None,
        };
        let json = serde_json::to_string(&loading).unwrap();
        assert!(json.contains("true"));
    }
}
