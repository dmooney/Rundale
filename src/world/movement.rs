//! Movement system — resolving movement commands and traversal with time.
//!
//! Handles resolving player movement intents to destinations, computing
//! travel time along paths, and producing narration text for travel.

use super::LocationId;
use super::graph::WorldGraph;

/// The result of resolving a movement command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovementResult {
    /// Player arrived at a destination after the given number of game minutes.
    Arrived {
        /// The destination location id.
        destination: LocationId,
        /// The path taken (including start and end).
        path: Vec<LocationId>,
        /// Total travel time in game minutes.
        minutes: u16,
        /// Narration text describing the journey.
        narration: String,
    },
    /// The target location could not be found.
    NotFound(String),
    /// The player is already at the target location.
    AlreadyHere,
}

/// Resolves a movement intent target to a `MovementResult`.
///
/// Uses fuzzy name matching to find the destination, then BFS to find
/// the shortest path. Travel narration is generated from the first
/// connection's path description.
pub fn resolve_movement(target: &str, graph: &WorldGraph, current: LocationId) -> MovementResult {
    // Try to find the target location
    let destination_id = match graph.find_by_name(target) {
        Some(id) => id,
        None => return MovementResult::NotFound(target.to_string()),
    };

    // Check if already there
    if destination_id == current {
        return MovementResult::AlreadyHere;
    }

    // Find shortest path
    let path = match graph.shortest_path(current, destination_id) {
        Some(p) => p,
        None => return MovementResult::NotFound(target.to_string()),
    };

    // Calculate total travel time
    let minutes = graph.path_travel_time(&path);

    // Build narration from first step's connection description
    let narration = build_travel_narration(&path, graph, minutes);

    MovementResult::Arrived {
        destination: destination_id,
        path,
        minutes,
        narration,
    }
}

/// Builds travel narration text from a path through the world graph.
///
/// For single-hop journeys, uses the connection's path description.
/// For multi-hop journeys, describes the first step with a summary.
fn build_travel_narration(path: &[LocationId], graph: &WorldGraph, total_minutes: u16) -> String {
    if path.len() < 2 {
        return String::new();
    }

    let dest_name = graph
        .get(*path.last().unwrap())
        .map(|l| l.name.as_str())
        .unwrap_or("your destination");

    if path.len() == 2 {
        // Direct connection
        if let Some(conn) = graph.connection_between(path[0], path[1]) {
            return format!(
                "You walk along {}. ({} minutes)",
                conn.path_description, total_minutes
            );
        }
    }

    // Multi-hop: describe the first leg and summarize
    let first_desc = graph
        .connection_between(path[0], path[1])
        .map(|c| c.path_description.as_str())
        .unwrap_or("the road");

    format!(
        "You set off along {} toward {}. ({} minutes)",
        first_desc, dest_name, total_minutes
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::graph::WorldGraph;

    fn test_graph() -> WorldGraph {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A crossroads.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "a short lane"},
                        {"target": 3, "traversal_minutes": 8, "path_description": "a winding boreen"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Darcy's Pub",
                    "description_template": "A pub.",
                    "indoor": true,
                    "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "back to the crossroads"}
                    ]
                },
                {
                    "id": 3,
                    "name": "St. Brigid's Church",
                    "description_template": "A church.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 8, "path_description": "the boreen back"},
                        {"target": 4, "traversal_minutes": 10, "path_description": "a path through the graveyard"}
                    ]
                },
                {
                    "id": 4,
                    "name": "The Fairy Fort",
                    "description_template": "A fairy fort.",
                    "indoor": false,
                    "public": true,
                    "connections": [
                        {"target": 3, "traversal_minutes": 10, "path_description": "back past the church"}
                    ]
                }
            ]
        }"#;
        WorldGraph::load_from_str(json).unwrap()
    }

    #[test]
    fn test_resolve_direct_movement() {
        let graph = test_graph();
        let result = resolve_movement("pub", &graph, LocationId(1));
        match result {
            MovementResult::Arrived {
                destination,
                minutes,
                narration,
                ..
            } => {
                assert_eq!(destination, LocationId(2));
                assert_eq!(minutes, 5);
                assert!(narration.contains("short lane"));
                assert!(narration.contains("5 minutes"));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_multi_hop_movement() {
        let graph = test_graph();
        // From pub to fairy fort: pub -> crossroads -> church -> fairy fort
        let result = resolve_movement("fairy fort", &graph, LocationId(2));
        match result {
            MovementResult::Arrived {
                destination,
                path,
                minutes,
                narration,
                ..
            } => {
                assert_eq!(destination, LocationId(4));
                assert_eq!(path.len(), 4); // pub -> crossroads -> church -> fort
                assert_eq!(minutes, 5 + 8 + 10); // 23 minutes
                assert!(narration.contains("minutes"));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_already_here() {
        let graph = test_graph();
        let result = resolve_movement("crossroads", &graph, LocationId(1));
        assert_eq!(result, MovementResult::AlreadyHere);
    }

    #[test]
    fn test_resolve_not_found() {
        let graph = test_graph();
        let result = resolve_movement("castle", &graph, LocationId(1));
        match result {
            MovementResult::NotFound(name) => assert_eq!(name, "castle"),
            other => panic!("expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let graph = test_graph();
        let result = resolve_movement("DARCY'S PUB", &graph, LocationId(1));
        match result {
            MovementResult::Arrived { destination, .. } => {
                assert_eq!(destination, LocationId(2));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_partial_name() {
        let graph = test_graph();
        let result = resolve_movement("church", &graph, LocationId(1));
        match result {
            MovementResult::Arrived { destination, .. } => {
                assert_eq!(destination, LocationId(3));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_narration_direct() {
        let graph = test_graph();
        let path = vec![LocationId(1), LocationId(2)];
        let narration = build_travel_narration(&path, &graph, 5);
        assert_eq!(narration, "You walk along a short lane. (5 minutes)");
    }

    #[test]
    fn test_narration_multi_hop() {
        let graph = test_graph();
        let path = vec![LocationId(2), LocationId(1), LocationId(3), LocationId(4)];
        let narration = build_travel_narration(&path, &graph, 23);
        assert!(narration.contains("The Fairy Fort"));
        assert!(narration.contains("23 minutes"));
    }

    #[test]
    fn test_narration_empty_path() {
        let graph = test_graph();
        let narration = build_travel_narration(&[], &graph, 0);
        assert!(narration.is_empty());
    }
}
