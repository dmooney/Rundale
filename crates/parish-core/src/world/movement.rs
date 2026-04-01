//! Movement system — resolving movement commands and traversal with time.
//!
//! Handles resolving player movement intents to destinations, computing
//! travel time along paths, and producing narration text for travel.

use super::LocationId;
use super::graph::WorldGraph;
use super::transport::TransportMode;

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
/// the shortest path. Travel time is calculated from coordinates using
/// the given transport mode's speed. Narration includes the transport label.
pub fn resolve_movement(
    target: &str,
    graph: &WorldGraph,
    current: LocationId,
    transport: &TransportMode,
) -> MovementResult {
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

    // Calculate total travel time from coordinates
    let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);

    // Build narration from first step's connection description
    let narration = build_travel_narration(&path, graph, minutes, transport);

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
/// Includes the transport label (e.g., "on foot") in the time display.
fn build_travel_narration(
    path: &[LocationId],
    graph: &WorldGraph,
    total_minutes: u16,
    transport: &TransportMode,
) -> String {
    if path.len() < 2 {
        return String::new();
    }

    let verb = if transport.id == "walking" {
        "walk"
    } else {
        "travel"
    };

    let dest_name = graph
        .get(*path.last().unwrap())
        .map(|l| l.name.as_str())
        .unwrap_or("your destination");

    if path.len() == 2 {
        // Direct connection
        if let Some(conn) = graph.connection_between(path[0], path[1]) {
            return format!(
                "You {} along {}. ({} minutes {})",
                verb, conn.path_description, total_minutes, transport.label
            );
        }
    }

    // Multi-hop: describe the first leg and summarize
    let first_desc = graph
        .connection_between(path[0], path[1])
        .map(|c| c.path_description.as_str())
        .unwrap_or("the road");

    format!(
        "You set off along {} toward {}. ({} minutes {})",
        first_desc, dest_name, total_minutes, transport.label
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::graph::WorldGraph;
    use crate::world::transport::TransportMode;

    fn walking() -> TransportMode {
        TransportMode::walking()
    }

    fn test_graph() -> WorldGraph {
        // Use real-ish coordinates so haversine gives meaningful results.
        // Crossroads: 53.618, -8.095
        // Pub: 53.6195, -8.0925  (~230m away → ~3 min walking)
        // Church: 53.6215, -8.099  (~450m away → ~6 min walking)
        // Fort: 53.627, -8.052  (~3km from church → large)
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "The Crossroads",
                    "description_template": "A crossroads.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.618,
                    "lon": -8.095,
                    "connections": [
                        {"target": 2, "path_description": "a short lane"},
                        {"target": 3, "path_description": "a winding boreen"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Darcy's Pub",
                    "description_template": "A pub.",
                    "indoor": true,
                    "public": true,
                    "lat": 53.6195,
                    "lon": -8.0925,
                    "connections": [
                        {"target": 1, "path_description": "back to the crossroads"}
                    ]
                },
                {
                    "id": 3,
                    "name": "St. Brigid's Church",
                    "description_template": "A church.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6215,
                    "lon": -8.099,
                    "connections": [
                        {"target": 1, "path_description": "the boreen back"},
                        {"target": 4, "path_description": "a path through the graveyard"}
                    ]
                },
                {
                    "id": 4,
                    "name": "The Fairy Fort",
                    "description_template": "A fairy fort.",
                    "indoor": false,
                    "public": true,
                    "lat": 53.627,
                    "lon": -8.052,
                    "connections": [
                        {"target": 3, "path_description": "back past the church"}
                    ]
                }
            ]
        }"#;
        WorldGraph::load_from_str(json).unwrap()
    }

    #[test]
    fn test_resolve_direct_movement() {
        let graph = test_graph();
        let result = resolve_movement("pub", &graph, LocationId(1), &walking());
        match result {
            MovementResult::Arrived {
                destination,
                minutes,
                narration,
                ..
            } => {
                assert_eq!(destination, LocationId(2));
                assert!(minutes >= 1 && minutes <= 10, "minutes was {minutes}");
                assert!(narration.contains("short lane"));
                assert!(narration.contains("on foot"));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_multi_hop_movement() {
        let graph = test_graph();
        // From pub to fairy fort: pub -> crossroads -> church -> fairy fort
        let result = resolve_movement("fairy fort", &graph, LocationId(2), &walking());
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
                assert!(
                    minutes >= 5,
                    "multi-hop should take several minutes, got {minutes}"
                );
                assert!(narration.contains("on foot"));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_already_here() {
        let graph = test_graph();
        let result = resolve_movement("crossroads", &graph, LocationId(1), &walking());
        assert_eq!(result, MovementResult::AlreadyHere);
    }

    #[test]
    fn test_resolve_not_found() {
        let graph = test_graph();
        let result = resolve_movement("castle", &graph, LocationId(1), &walking());
        match result {
            MovementResult::NotFound(name) => assert_eq!(name, "castle"),
            other => panic!("expected NotFound, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_case_insensitive() {
        let graph = test_graph();
        let result = resolve_movement("DARCY'S PUB", &graph, LocationId(1), &walking());
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
        let result = resolve_movement("church", &graph, LocationId(1), &walking());
        match result {
            MovementResult::Arrived { destination, .. } => {
                assert_eq!(destination, LocationId(3));
            }
            other => panic!("expected Arrived, got {:?}", other),
        }
    }

    #[test]
    fn test_narration_direct_walking() {
        let graph = test_graph();
        let transport = walking();
        let path = vec![LocationId(1), LocationId(2)];
        let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
        let narration = build_travel_narration(&path, &graph, minutes, &transport);
        assert!(narration.starts_with("You walk along a short lane."));
        assert!(narration.contains("on foot"));
    }

    #[test]
    fn test_narration_direct_non_walking() {
        let graph = test_graph();
        let transport = TransportMode {
            id: "jaunting_car".to_string(),
            label: "in a jaunting car".to_string(),
            speed_m_per_s: 4.0,
        };
        let path = vec![LocationId(1), LocationId(2)];
        let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
        let narration = build_travel_narration(&path, &graph, minutes, &transport);
        assert!(narration.starts_with("You travel along a short lane."));
        assert!(narration.contains("in a jaunting car"));
    }

    #[test]
    fn test_narration_multi_hop() {
        let graph = test_graph();
        let transport = walking();
        let path = vec![LocationId(2), LocationId(1), LocationId(3), LocationId(4)];
        let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
        let narration = build_travel_narration(&path, &graph, minutes, &transport);
        assert!(narration.contains("The Fairy Fort"));
        assert!(narration.contains("on foot"));
    }

    #[test]
    fn test_narration_empty_path() {
        let graph = test_graph();
        let narration = build_travel_narration(&[], &graph, 0, &walking());
        assert!(narration.is_empty());
    }

    #[test]
    fn test_faster_transport_takes_less_time() {
        let graph = test_graph();
        let walk = walking();
        let fast = TransportMode {
            id: "jaunting_car".to_string(),
            label: "in a jaunting car".to_string(),
            speed_m_per_s: 4.0,
        };
        let path = vec![LocationId(1), LocationId(3), LocationId(4)];
        let walk_time = graph.path_travel_time(&path, walk.speed_m_per_s);
        let fast_time = graph.path_travel_time(&path, fast.speed_m_per_s);
        assert!(
            fast_time <= walk_time,
            "jaunting car ({fast_time} min) should be <= walking ({walk_time} min)"
        );
    }
}
