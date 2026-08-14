//! Graph schema types — `GeoKind`, `RelativeRef`, `Hazard`, `Connection`,
//! `LocationData`, `WorldGraph` (struct + `new`/`Default`), and the
//! JSON serialisation wrapper.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use parish_types::{LocationId, NpcId, ParishError};

/// Declares whether a map location is grounded in a real place,
/// author-pinned to a specific coordinate, or authored as fiction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeoKind {
    /// Backed by a real-world place that can be geocoded at runtime.
    Real,
    /// Author-pinned to an explicit coordinate (e.g. a historic map feature
    /// that modern geocoders would misplace). Never geocoded; acts as an
    /// anchor for relative positioning and fictional realignment.
    Manual,
    /// Authored location in the world fiction.
    #[default]
    Fictional,
}

/// Position expressed as a signed offset (in meters, north and east) from
/// another location. When present, it overrides a location's stored
/// `lat`/`lon` for resolution purposes — the stored pair acts as a cache of
/// the last resolved absolute coordinate.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct RelativeRef {
    /// The location this one is positioned relative to.
    pub anchor: LocationId,
    /// Offset north in meters (negative = south).
    pub dnorth_m: f64,
    /// Offset east in meters (negative = west).
    pub deast_m: f64,
}

/// Weather hazard tag for a connection (edge) in the world graph.
///
/// Marks paths that become dangerous, slow, or impassable under specific
/// weather. The hazard is applied bidirectionally in both connection
/// entries because world-graph edges are authored in both directions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Hazard {
    /// No weather hazard. Default; same as the field being absent.
    #[default]
    None,
    /// Crosses a stream, ford, weir, or low bridge. Impassable in a Storm
    /// (water washes over the crossing) and slower in HeavyRain.
    Flood,
    /// A lakeshore, headland, or open waterline path. Impassable in a
    /// Storm (waves and wind drive you back) and slower in HeavyRain.
    Lakeshore,
    /// A rough track across open bog, gorse, or hilltop with no
    /// landmarks. Slower in Fog (easy to lose the path) and slower in
    /// HeavyRain (ground turns to mire).
    Exposed,
}

impl Hazard {
    /// Returns `true` when this hazard is considered absent / trivial.
    pub fn is_none(&self) -> bool {
        matches!(self, Hazard::None)
    }
}

/// A connection (edge) between two locations in the world graph.
///
/// Each connection has a target location and a prose description of the path.
/// Travel time is calculated at runtime from coordinates and transport speed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// The destination location.
    pub target: LocationId,
    /// Prose description of the path (e.g., "a narrow boreen lined with hawthorn").
    pub path_description: String,
    /// Optional weather hazard tag that gates or slows this edge when the
    /// world weather is severe. Defaults to [`Hazard::None`] when absent
    /// from the JSON so existing mods remain unchanged.
    #[serde(default, skip_serializing_if = "Hazard::is_none")]
    pub hazard: Hazard,
}

/// Extended location data for the world graph.
///
/// Augments the base location with connections, description templates,
/// associated NPCs, and optional mythological significance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationData {
    /// Unique identifier.
    pub id: LocationId,
    /// Human-readable name (e.g., "The Crossroads").
    pub name: String,
    /// Description template with placeholders: `{time}`, `{weather}`, `{npcs_present}`.
    pub description_template: String,
    /// Concrete authored features NPC dialogue must not deny. Unlike the
    /// description template, these are structured semantic facts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub landmarks: Vec<String>,
    /// Whether this location is indoors.
    pub indoor: bool,
    /// Whether this location is publicly accessible.
    pub public: bool,
    /// Connections to neighboring locations.
    pub connections: Vec<Connection>,
    /// WGS-84 latitude (from OSM data; 0.0 if not geocoded).
    #[serde(default)]
    pub lat: f64,
    /// WGS-84 longitude (from OSM data; 0.0 if not geocoded).
    #[serde(default)]
    pub lon: f64,
    /// NPCs who live or work at this location.
    #[serde(default)]
    pub associated_npcs: Vec<NpcId>,
    /// Optional mythological significance (fairy forts, holy wells, etc.).
    #[serde(default)]
    pub mythological_significance: Option<String>,
    /// Alternative names for this location (e.g., "coast" for "Lough Ree Shore").
    ///
    /// Used by fuzzy name matching to support colloquial and semantic synonyms.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Whether this location maps to a real place, is author-pinned, or is fictional.
    #[serde(default)]
    pub geo_kind: GeoKind,
    /// Optional relative-position override. When set, `lat`/`lon` are a
    /// cache derived from `anchor.lat`/`anchor.lon` plus the offset; the
    /// realign tool resolves and rewrites them on each run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_to: Option<RelativeRef>,
    /// Provenance note for `Manual` locations (e.g. "OS 6-inch First
    /// Edition, Roscommon sheet, ca. 1837"). Ignored at runtime; intended
    /// as authoring metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geo_source: Option<String>,
}

impl LocationData {
    /// Validates that lat/lon coordinates are within valid WGS-84 bounds.
    /// Latitude must be in [-90.0, 90.0]; longitude must be in [-180.0, 180.0].
    pub(super) fn validate_coordinates(&self) -> Result<(), ParishError> {
        if !(-90.0..=90.0).contains(&self.lat) {
            return Err(ParishError::WorldGraph(format!(
                "invalid latitude: {} (must be between -90 and 90)",
                self.lat
            )));
        }
        if !(-180.0..=180.0).contains(&self.lon) {
            return Err(ParishError::WorldGraph(format!(
                "invalid longitude: {} (must be between -180 and 180)",
                self.lon
            )));
        }
        Ok(())
    }
}

/// The world graph: a collection of locations connected by traversable paths.
///
/// Provides lookup, fuzzy name search, neighbor queries, and BFS pathfinding.
#[derive(Debug, Clone)]
pub struct WorldGraph {
    /// All locations keyed by their id.
    pub(super) locations: HashMap<LocationId, LocationData>,
}

/// Serialization wrapper for loading/saving the world graph as JSON.
#[derive(Serialize, Deserialize)]
pub(super) struct WorldGraphFile {
    pub(super) locations: Vec<LocationData>,
}

impl WorldGraph {
    /// Creates a new empty world graph.
    pub fn new() -> Self {
        Self {
            locations: HashMap::new(),
        }
    }
}

impl Default for WorldGraph {
    fn default() -> Self {
        Self::new()
    }
}
