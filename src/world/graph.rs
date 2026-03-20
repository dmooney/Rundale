//! World graph — location graph with connections, pathfinding, and data loading.
//!
//! The world is a graph of named location nodes connected by edges
//! with traversal times. This module provides the `WorldGraph` container,
//! `Connection` edges, and BFS pathfinding.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::ParishError;
use crate::npc::NpcId;

use super::LocationId;

/// A connection (edge) between two locations in the world graph.
///
/// Each connection has a target location, a traversal time in game minutes,
/// and a prose description of the path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connection {
    /// The destination location.
    pub target: LocationId,
    /// Time in game minutes to traverse this connection.
    pub traversal_minutes: u16,
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
    /// NPCs who live or work at this location.
    #[serde(default)]
    pub associated_npcs: Vec<NpcId>,
    /// Optional mythological significance (fairy forts, holy wells, etc.).
    #[serde(default)]
    pub mythological_significance: Option<String>,
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
    fn validate(&self) -> Result<(), ParishError> {
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
    /// Matching priority: exact match > query in name > name in query >
    /// stripped-article match. Common articles ("the", "a", "an") are
    /// stripped for fuzzy matching.
    pub fn find_by_name(&self, name: &str) -> Option<LocationId> {
        let lower = name.to_lowercase();
        let stripped = strip_articles(&lower);

        // First try exact match (case-insensitive)
        for (id, loc) in &self.locations {
            if loc.name.to_lowercase() == lower {
                return Some(*id);
            }
        }

        // Then try: query contained in location name
        for (id, loc) in &self.locations {
            if loc.name.to_lowercase().contains(&lower) {
                return Some(*id);
            }
        }

        // Then try: location name contained in query
        for (id, loc) in &self.locations {
            let loc_lower = loc.name.to_lowercase();
            if lower.contains(&loc_lower) {
                return Some(*id);
            }
        }

        // Then try with articles stripped from both sides
        if stripped != lower {
            for (id, loc) in &self.locations {
                let loc_stripped = strip_articles(&loc.name.to_lowercase());
                if loc_stripped.contains(&stripped) || stripped.contains(&loc_stripped) {
                    return Some(*id);
                }
            }
        }

        None
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

    /// Returns the total traversal time along a path in game minutes.
    ///
    /// Given a sequence of location ids (as returned by `shortest_path`),
    /// sums the traversal times of each edge along the path.
    pub fn path_travel_time(&self, path: &[LocationId]) -> u16 {
        if path.len() < 2 {
            return 0;
        }

        let mut total = 0u16;
        for window in path.windows(2) {
            let from = window[0];
            let to = window[1];
            if let Some(loc) = self.locations.get(&from)
                && let Some(conn) = loc.connections.iter().find(|c| c.target == to)
            {
                total = total.saturating_add(conn.traversal_minutes);
            }
        }
        total
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
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "a short lane"},
                        {"target": 3, "traversal_minutes": 8, "path_description": "a winding boreen"}
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
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "a short lane back to the crossroads"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null
                },
                {
                    "id": 3,
                    "name": "St. Brigid's Church",
                    "description_template": "The old stone church stands in {weather} {time} light.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 8, "path_description": "the boreen back to the crossroads"},
                        {"target": 4, "traversal_minutes": 10, "path_description": "a path through the graveyard"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": null
                },
                {
                    "id": 4,
                    "name": "The Fairy Fort",
                    "description_template": "An ancient ring fort on the hill. {weather}.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {"target": 3, "traversal_minutes": 10, "path_description": "the path back past the church"}
                    ],
                    "associated_npcs": [],
                    "mythological_significance": "A rath said to be home to the sídhe. Locals avoid it after dark."
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
        let time = graph.path_travel_time(&path);
        // 5 + 8 + 10 = 23
        assert_eq!(time, 23);
    }

    #[test]
    fn test_path_travel_time_single() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.path_travel_time(&[LocationId(1)]), 0);
    }

    #[test]
    fn test_path_travel_time_empty() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.path_travel_time(&[]), 0);
    }

    #[test]
    fn test_connection_between() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let conn = graph
            .connection_between(LocationId(1), LocationId(2))
            .unwrap();
        assert_eq!(conn.traversal_minutes, 5);
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
    fn test_location_ids() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let ids = graph.location_ids();
        assert_eq!(ids.len(), 4);
    }
}
