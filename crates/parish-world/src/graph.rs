//! World graph — location graph with connections, pathfinding, and data loading.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. This module provides the `WorldGraph` container,
//! `Connection` edges, and BFS pathfinding.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use strsim::jaro_winkler;

use crate::geo;
use parish_config::WorldConfig;
use parish_types::{LocationId, NpcId, ParishError};

/// Declares whether a map location is grounded in a real place
/// or authored as fiction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GeoKind {
    /// Backed by a real-world place that can be geocoded.
    Real,
    /// Authored location in the world fiction.
    #[default]
    Fictional,
}

/// A connection (edge) between two locations in the world graph.
///
/// Each connection has a target location and a prose description of the path.
/// Travel time is calculated at runtime from coordinates and transport speed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// The destination location.
    pub target: LocationId,
    /// Legacy field — ignored at runtime; travel time is calculated from coordinates.
    #[serde(default, skip_serializing)]
    pub traversal_minutes: Option<u16>,
    /// Prose description of the path (e.g., "a narrow boreen lined with hawthorn").
    pub path_description: String,
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
    /// Whether this location maps to a real place or is fictional.
    #[serde(default)]
    pub geo_kind: GeoKind,
}

/// The world graph: a collection of locations connected by traversable paths.
///
/// Provides lookup, fuzzy name search, neighbor queries, and BFS pathfinding.
#[derive(Debug, Clone)]
pub struct WorldGraph {
    /// All locations keyed by their id.
    locations: HashMap<LocationId, LocationData>,
}

/// Serialization wrapper for loading/saving the world graph as JSON.
#[derive(Serialize, Deserialize)]
struct WorldGraphFile {
    locations: Vec<LocationData>,
}

impl WorldGraph {
    /// Creates a new empty world graph.
    pub fn new() -> Self {
        Self {
            locations: HashMap::new(),
        }
    }

    /// Loads a world graph from a JSON file.
    ///
    /// Validates that all connection targets exist and that connections
    /// are bidirectional.
    pub fn load_from_file(path: &Path) -> Result<Self, ParishError> {
        let contents = std::fs::read_to_string(path)?;
        Self::load_from_str(&contents)
    }

    /// Loads a world graph from a JSON string.
    ///
    /// Validates that all connection targets exist and that connections
    /// are bidirectional.
    pub fn load_from_str(json: &str) -> Result<Self, ParishError> {
        let file: WorldGraphFile = serde_json::from_str(json)?;

        let mut locations = HashMap::new();
        for loc in file.locations {
            if locations.contains_key(&loc.id) {
                return Err(ParishError::WorldGraph(format!(
                    "duplicate location id: {}",
                    loc.id.0
                )));
            }
            locations.insert(loc.id, loc);
        }

        let graph = Self { locations };
        graph.validate()?;
        Ok(graph)
    }

    /// Validates the world graph.
    ///
    /// Checks that:
    /// - All connection targets exist in the graph
    /// - All connections are bidirectional
    /// - There are no orphan nodes (nodes with no connections)
    ///
    /// Called automatically by [`WorldGraph::load_from_str`]; exposed publicly
    /// so the Parish Designer editor can re-run it on an in-memory graph
    /// after edits without reloading the JSON file.
    pub fn validate(&self) -> Result<(), ParishError> {
        for (id, loc) in &self.locations {
            if loc.connections.is_empty() {
                return Err(ParishError::WorldGraph(format!(
                    "orphan location with no connections: {} (id {})",
                    loc.name, id.0
                )));
            }
            for conn in &loc.connections {
                // Check target exists
                if !self.locations.contains_key(&conn.target) {
                    return Err(ParishError::WorldGraph(format!(
                        "location {} (id {}) has connection to non-existent target id {}",
                        loc.name, id.0, conn.target.0
                    )));
                }
                // Check bidirectionality
                let target_loc = &self.locations[&conn.target];
                let has_reverse = target_loc.connections.iter().any(|c| c.target == *id);
                if !has_reverse {
                    return Err(ParishError::WorldGraph(format!(
                        "connection from {} to {} is not bidirectional",
                        loc.name, target_loc.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// Returns a reference to a location by id.
    pub fn get(&self, id: LocationId) -> Option<&LocationData> {
        self.locations.get(&id)
    }

    /// Returns all neighbors of a location with their connections.
    pub fn neighbors(&self, id: LocationId) -> Vec<(LocationId, &Connection)> {
        match self.locations.get(&id) {
            Some(loc) => loc
                .connections
                .iter()
                .map(|conn| (conn.target, conn))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Finds a location by name using case-insensitive fuzzy matching.
    ///
    /// Matching priority (name matches beat alias matches at each level):
    /// 1. Exact name → exact alias
    /// 2. Query in name → query in alias
    /// 3. Name in query → alias in query
    /// 4. Article-stripped name → article-stripped alias
    /// 5. Jaro-Winkler fuzzy score (catches typos and near-misses)
    ///
    /// Common articles ("the", "a", "an") are stripped for fuzzy matching.
    pub fn find_by_name(&self, name: &str) -> Option<LocationId> {
        self.find_by_name_with_config(name, &WorldConfig::default())
    }

    /// Finds a location by name using case-insensitive fuzzy matching,
    /// with a configurable fuzzy threshold from [`WorldConfig`].
    ///
    /// Performance: single pass through all locations, computing `to_lowercase()`
    /// once per name/alias instead of up to 8× in separate priority-level scans.
    /// Fuzzy scores are also computed in the same pass to avoid a redundant 9th scan.
    pub fn find_by_name_with_config(&self, name: &str, config: &WorldConfig) -> Option<LocationId> {
        let lower = name.to_lowercase();
        let stripped = strip_articles(&lower);
        let do_article_strip = stripped != lower;

        // Track best deterministic match: (priority_level, location_id).
        // Lower level number = higher priority.
        let mut best: Option<(u8, LocationId)> = None;
        // Track best fuzzy match as fallback (level 5).
        let mut best_fuzzy: Option<(f64, LocationId)> = None;

        for (id, loc) in &self.locations {
            // Lowercase name once per location (was repeated up to 8× across separate scans)
            let loc_lower = loc.name.to_lowercase();

            // Level 1: Exact name match — can't be beaten, return immediately
            if loc_lower == lower {
                return Some(*id);
            }

            // Lowercase aliases once per location
            let aliases_lower: Vec<String> = loc.aliases.iter().map(|a| a.to_lowercase()).collect();

            // Determine this location's best matching priority level
            let level = if aliases_lower.contains(&lower) {
                1 // Level 1b: Exact alias
            } else if loc_lower.contains(&lower) {
                2 // Level 2: Query in name
            } else if aliases_lower.iter().any(|a| a.contains(&lower)) {
                3 // Level 2b: Query in alias
            } else if lower.contains(loc_lower.as_str()) {
                4 // Level 3: Name in query
            } else if aliases_lower.iter().any(|a| lower.contains(a.as_str())) {
                5 // Level 3b: Alias in query
            } else if do_article_strip {
                let loc_stripped = strip_articles(&loc_lower);
                if loc_stripped.contains(&stripped) || stripped.contains(loc_stripped.as_str()) {
                    6 // Level 4: Article-stripped name
                } else if aliases_lower.iter().any(|a| {
                    let a_stripped = strip_articles(a);
                    a_stripped.contains(&stripped) || stripped.contains(a_stripped.as_str())
                }) {
                    7 // Level 4b: Article-stripped alias
                } else {
                    u8::MAX // No deterministic match
                }
            } else {
                u8::MAX // No deterministic match
            };

            if level < u8::MAX {
                if best.as_ref().is_none_or(|(best_lvl, _)| level < *best_lvl) {
                    best = Some((level, *id));
                }
            } else {
                // Level 5: Jaro-Winkler fuzzy — computed using already-lowercased strings
                let name_score = jaro_winkler(&loc_lower, &stripped);
                let alias_score = aliases_lower
                    .iter()
                    .map(|a| jaro_winkler(a, &stripped))
                    .fold(0.0_f64, f64::max);
                let max_score = name_score.max(alias_score);
                if max_score > best_fuzzy.as_ref().map_or(0.0, |(s, _)| *s) {
                    best_fuzzy = Some((max_score, *id));
                }
            }
        }

        best.map(|(_, id)| id).or_else(|| {
            best_fuzzy
                .filter(|(score, _)| *score >= config.fuzzy_threshold)
                .map(|(_, id)| id)
        })
    }

    /// Finds the shortest path between two locations using BFS.
    ///
    /// Returns `None` if no path exists. The returned path includes
    /// both the start and end locations.
    pub fn shortest_path(&self, from: LocationId, to: LocationId) -> Option<Vec<LocationId>> {
        if from == to {
            return Some(vec![from]);
        }
        if !self.locations.contains_key(&from) || !self.locations.contains_key(&to) {
            return None;
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut predecessors: HashMap<LocationId, LocationId> = HashMap::new();

        visited.insert(from);
        queue.push_back(from);

        while let Some(current) = queue.pop_front() {
            for (neighbor_id, _) in self.neighbors(current) {
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id);
                    predecessors.insert(neighbor_id, current);

                    if neighbor_id == to {
                        // Reconstruct path
                        let mut path = vec![to];
                        let mut node = to;
                        while let Some(&pred) = predecessors.get(&node) {
                            path.push(pred);
                            node = pred;
                        }
                        path.reverse();
                        return Some(path);
                    }

                    queue.push_back(neighbor_id);
                }
            }
        }

        None
    }

    /// Calculates travel time in game minutes between two locations
    /// using haversine distance and the given speed.
    pub fn edge_travel_minutes(&self, from: LocationId, to: LocationId, speed_m_per_s: f64) -> u16 {
        let from_loc = match self.locations.get(&from) {
            Some(loc) => loc,
            None => return 1,
        };
        let to_loc = match self.locations.get(&to) {
            Some(loc) => loc,
            None => return 1,
        };
        let meters = geo::haversine_distance(from_loc.lat, from_loc.lon, to_loc.lat, to_loc.lon);
        geo::meters_to_minutes(meters, speed_m_per_s)
    }

    /// Returns the total traversal time along a path in game minutes.
    ///
    /// Given a sequence of location ids (as returned by `shortest_path`),
    /// calculates the haversine distance for each edge and converts to
    /// minutes at the given travel speed.
    pub fn path_travel_time(&self, path: &[LocationId], speed_m_per_s: f64) -> u16 {
        if path.len() < 2 {
            return 0;
        }

        let mut total = 0u16;
        for window in path.windows(2) {
            total =
                total.saturating_add(self.edge_travel_minutes(window[0], window[1], speed_m_per_s));
        }
        total
    }

    /// Computes travel time from a source to every reachable location in a single
    /// BFS pass.
    ///
    /// Returns a map from `LocationId` to cumulative travel minutes along the
    /// shortest-hop path. The source location has time 0. This replaces N separate
    /// `shortest_path()` + `path_travel_time()` calls with one traversal.
    pub fn travel_times_from(
        &self,
        from: LocationId,
        speed_m_per_s: f64,
    ) -> HashMap<LocationId, u16> {
        let mut times: HashMap<LocationId, u16> = HashMap::new();
        if !self.locations.contains_key(&from) {
            return times;
        }
        times.insert(from, 0);
        let mut queue = VecDeque::new();
        queue.push_back((from, 0u16));
        while let Some((current, current_time)) = queue.pop_front() {
            for (neighbor_id, _) in self.neighbors(current) {
                if let std::collections::hash_map::Entry::Vacant(e) = times.entry(neighbor_id) {
                    let edge = self.edge_travel_minutes(current, neighbor_id, speed_m_per_s);
                    let total = current_time.saturating_add(edge);
                    e.insert(total);
                    queue.push_back((neighbor_id, total));
                }
            }
        }
        times
    }

    /// Computes the hop distance from a source location to every reachable location.
    ///
    /// Returns a map from `LocationId` to the number of graph hops (edges)
    /// required to reach it from `from`. The source location has distance 0.
    /// Unreachable or nonexistent locations are not included.
    pub fn hop_distances(&self, from: LocationId) -> HashMap<LocationId, u32> {
        let mut distances = HashMap::new();
        if !self.locations.contains_key(&from) {
            return distances;
        }
        distances.insert(from, 0);
        let mut queue = VecDeque::new();
        queue.push_back((from, 0u32));
        while let Some((current, depth)) = queue.pop_front() {
            for (neighbor_id, _) in self.neighbors(current) {
                if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor_id) {
                    e.insert(depth + 1);
                    queue.push_back((neighbor_id, depth + 1));
                }
            }
        }
        distances
    }

    /// Returns the connection from one location to another, if they are neighbors.
    pub fn connection_between(&self, from: LocationId, to: LocationId) -> Option<&Connection> {
        self.locations
            .get(&from)?
            .connections
            .iter()
            .find(|c| c.target == to)
    }

    /// Returns the number of locations in the graph.
    pub fn location_count(&self) -> usize {
        self.locations.len()
    }

    /// Returns all location ids in the graph.
    pub fn location_ids(&self) -> Vec<LocationId> {
        self.locations.keys().copied().collect()
    }
}

impl Default for WorldGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Strips common English articles from the beginning of a string.
fn strip_articles(s: &str) -> String {
    let trimmed = s.trim();
    for article in &["the ", "a ", "an "] {
        if let Some(rest) = trimmed.strip_prefix(article) {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_graph_json() -> &'static str {
        r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A quiet crossroads at {time}. The weather is {weather}.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.618,
                    "lon": -8.095,
                    "connections": [
                        {"target": 2, "path_description": "a short lane"},
                        {"target": 3, "path_description": "a winding boreen"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null
                },
                {
                    "id": 2,
                    "name": "Darcy's Pub",
                    "description_template": "The warm interior of Darcy's Pub at {time}.",
                    "indoor": true,
                    "public": true,
                    "lat": 53.6195,
                    "lon": -8.0925,
                    "connections": [
                        {"target": 1, "path_description": "a short lane back to the crossroads"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null,
                    "aliases": ["tavern", "the pub"]
                },
                {
                    "id": 3,
                    "name": "St. Brigid's Church",
                    "description_template": "The old stone church stands in {weather} {time} light.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6215,
                    "lon": -8.099,
                    "connections": [
                        {"target": 1, "path_description": "the boreen back to the crossroads"},
                        {"target": 4, "path_description": "a path through the graveyard"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null,
                    "aliases": ["church", "chapel"]
                },
                {
                    "id": 4,
                    "name": "The Fairy Fort",
                    "description_template": "An ancient ring fort on the hill. {weather}.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.627,
                    "lon": -8.052,
                    "connections": [
                        {"target": 3, "path_description": "the path back past the church"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": "A rath said to be home to the sídhe. Locals avoid it after dark.",
                    "aliases": ["rath", "ring fort", "the rath"]
                }
            ]
        }"#
    }

    #[test]
    fn test_load_from_str() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.location_count(), 4);
    }

    #[test]
    fn test_get_location() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let loc = graph.get(LocationId(1)).unwrap();
        assert_eq!(loc.name, "The Crossroads");
        assert!(!loc.indoor);
        assert!(loc.public);
    }

    #[test]
    fn test_get_nonexistent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(graph.get(LocationId(99)).is_none());
    }

    #[test]
    fn test_neighbors() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let neighbors = graph.neighbors(LocationId(1));
        assert_eq!(neighbors.len(), 2);

        let target_ids: Vec<LocationId> = neighbors.iter().map(|(id, _)| *id).collect();
        assert!(target_ids.contains(&LocationId(2)));
        assert!(target_ids.contains(&LocationId(3)));
    }

    #[test]
    fn test_neighbors_empty() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let neighbors = graph.neighbors(LocationId(99));
        assert!(neighbors.is_empty());
    }

    #[test]
    fn test_find_by_name_exact() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("Darcy's Pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("darcy's pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_partial() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_church() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("church").unwrap();
        assert_eq!(id, LocationId(3));
    }

    #[test]
    fn test_find_by_name_not_found() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(graph.find_by_name("castle").is_none());
    }

    #[test]
    fn test_find_by_name_alias_exact() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("rath").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_substring() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "ring" is a substring of alias "ring fort"
        let id = graph.find_by_name("ring").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_with_article() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("the rath").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_tavern() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("tavern").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_prefers_name_over_alias() {
        // "pub" is both a substring of the name "Darcy's Pub" (level 2)
        // and an alias. Name match (level 2) should win.
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_aliases_default_empty() {
        // Location 1 has no aliases field — should default to empty vec
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let crossroads = graph.get(LocationId(1)).unwrap();
        assert!(crossroads.aliases.is_empty());
    }

    #[test]
    fn test_find_by_name_fuzzy_typo() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "churh" is a typo for "church" — Jaro-Winkler should catch it
        let id = graph.find_by_name("churh").unwrap();
        assert_eq!(id, LocationId(3));
    }

    #[test]
    fn test_find_by_name_fuzzy_no_false_positive() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "xyz" is nothing close to any location — should not match
        assert!(graph.find_by_name("xyz").is_none());
    }

    #[test]
    fn test_shortest_path_adjacent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = graph.shortest_path(LocationId(1), LocationId(2)).unwrap();
        assert_eq!(path, vec![LocationId(1), LocationId(2)]);
    }

    #[test]
    fn test_shortest_path_multi_hop() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = graph.shortest_path(LocationId(2), LocationId(4)).unwrap();
        // 2 -> 1 -> 3 -> 4
        assert_eq!(
            path,
            vec![LocationId(2), LocationId(1), LocationId(3), LocationId(4)]
        );
    }

    #[test]
    fn test_shortest_path_same_location() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = graph.shortest_path(LocationId(1), LocationId(1)).unwrap();
        assert_eq!(path, vec![LocationId(1)]);
    }

    #[test]
    fn test_shortest_path_nonexistent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(graph.shortest_path(LocationId(1), LocationId(99)).is_none());
    }

    #[test]
    fn test_path_travel_time() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = vec![LocationId(2), LocationId(1), LocationId(3), LocationId(4)];
        let time = graph.path_travel_time(&path, 1.25);
        // Computed from haversine distances — should be > 0
        assert!(time > 0, "multi-hop travel time should be positive");
    }

    #[test]
    fn test_path_travel_time_single() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.path_travel_time(&[LocationId(1)], 1.25), 0);
    }

    #[test]
    fn test_path_travel_time_empty() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.path_travel_time(&[], 1.25), 0);
    }

    #[test]
    fn test_edge_travel_minutes() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // Crossroads to Pub: ~230m at 1.25 m/s → ~3 min
        let minutes = graph.edge_travel_minutes(LocationId(1), LocationId(2), 1.25);
        assert!((1..=10).contains(&minutes), "edge time was {minutes}");
    }

    #[test]
    fn test_faster_speed_shorter_time() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = vec![LocationId(1), LocationId(3), LocationId(4)];
        let walk = graph.path_travel_time(&path, 1.25);
        let fast = graph.path_travel_time(&path, 4.0);
        assert!(fast <= walk, "faster speed should give shorter time");
    }

    #[test]
    fn test_connection_between() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let conn = graph
            .connection_between(LocationId(1), LocationId(2))
            .unwrap();
        assert_eq!(conn.path_description, "a short lane");
    }

    #[test]
    fn test_connection_between_none() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(
            graph
                .connection_between(LocationId(1), LocationId(4))
                .is_none()
        );
    }

    #[test]
    fn test_mythological_significance() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let fort = graph.get(LocationId(4)).unwrap();
        assert!(fort.mythological_significance.is_some());
        assert!(
            fort.mythological_significance
                .as_ref()
                .unwrap()
                .contains("sídhe")
        );

        let crossroads = graph.get(LocationId(1)).unwrap();
        assert!(crossroads.mythological_significance.is_none());
    }

    #[test]
    fn test_validation_missing_target() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 99, "traversal_minutes": 5, "path_description": "path"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-existent"));
    }

    #[test]
    fn test_validation_not_bidirectional() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "traversal_minutes": 5, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "path"}]
                },
                {
                    "id": 3,
                    "name": "C",
                    "description_template": "C",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "path"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not bidirectional"));
    }

    #[test]
    fn test_validation_orphan_node() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": []
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("orphan"));
    }

    #[test]
    fn test_validation_duplicate_id() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "loop"}]
                },
                {
                    "id": 1,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "loop"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn test_hop_distances_from_leaf() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let distances = graph.hop_distances(LocationId(4));
        assert_eq!(distances[&LocationId(4)], 0);
        assert_eq!(distances[&LocationId(3)], 1);
        assert_eq!(distances[&LocationId(1)], 2);
        assert_eq!(distances[&LocationId(2)], 3);
        assert_eq!(distances.len(), 4);
    }

    #[test]
    fn test_hop_distances_from_center() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let distances = graph.hop_distances(LocationId(1));
        assert_eq!(distances[&LocationId(1)], 0);
        assert_eq!(distances[&LocationId(2)], 1);
        assert_eq!(distances[&LocationId(3)], 1);
        assert_eq!(distances[&LocationId(4)], 2);
    }

    #[test]
    fn test_hop_distances_nonexistent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let distances = graph.hop_distances(LocationId(99));
        assert!(distances.is_empty());
    }

    #[test]
    fn test_location_ids() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let ids = graph.location_ids();
        assert_eq!(ids.len(), 4);
    }

    #[test]
    fn test_travel_times_from_source() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let times = graph.travel_times_from(LocationId(1), 1.25);
        // Source has zero travel time
        assert_eq!(times[&LocationId(1)], 0);
        // All locations reachable
        assert_eq!(times.len(), 4);
        // Direct neighbors have positive time
        assert!(times[&LocationId(2)] > 0);
        assert!(times[&LocationId(3)] > 0);
        // 2-hop destination should equal the sum of its edge travel times
        // (via shortest-hop path 1 → 3 → 4)
        let expected = graph
            .edge_travel_minutes(LocationId(1), LocationId(3), 1.25)
            .saturating_add(graph.edge_travel_minutes(LocationId(3), LocationId(4), 1.25));
        assert_eq!(times[&LocationId(4)], expected);
    }

    #[test]
    fn test_travel_times_matches_path_travel_time() {
        // Verify that single-pass BFS produces the same result as
        // shortest_path() + path_travel_time() for each location.
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let speed = 1.25;
        let source = LocationId(1);
        let times = graph.travel_times_from(source, speed);

        for id in graph.location_ids() {
            if id == source {
                assert_eq!(times[&id], 0);
                continue;
            }
            let path = graph.shortest_path(source, id).unwrap();
            let expected = graph.path_travel_time(&path, speed);
            assert_eq!(
                times[&id], expected,
                "travel time mismatch for location {}",
                id.0
            );
        }
    }

    #[test]
    fn test_travel_times_nonexistent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let times = graph.travel_times_from(LocationId(99), 1.25);
        assert!(times.is_empty());
    }
}
