//! Graph pathfinding and travel-time calculations — BFS shortest-path,
//! filtered BFS, edge travel minutes, path travel time, travel-times-from,
//! and hop distances.

use std::collections::{HashMap, HashSet, VecDeque};

use parish_types::LocationId;

use super::schema::{Connection, WorldGraph};
use crate::geo;

impl WorldGraph {
    /// Finds the shortest path between two locations using BFS, skipping
    /// any edge where `allow(from, to, &connection)` returns `false`.
    ///
    /// Used by the weather-aware movement resolver to route around
    /// flooded fords, storm-lashed lakeshore paths, and fogbound open
    /// bog tracks. The closure is called once per candidate edge; the
    /// path is reconstructed only from edges that passed the filter.
    pub fn shortest_path_filtered<F>(
        &self,
        from: LocationId,
        to: LocationId,
        allow: F,
    ) -> Option<Vec<LocationId>>
    where
        F: Fn(LocationId, LocationId, &Connection) -> bool,
    {
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
            for (neighbor_id, conn) in self.neighbors(current) {
                if !allow(current, neighbor_id, conn) {
                    continue;
                }
                if !visited.contains(&neighbor_id) {
                    visited.insert(neighbor_id);
                    predecessors.insert(neighbor_id, current);

                    if neighbor_id == to {
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

    /// Finds the shortest path between two locations using BFS.
    ///
    /// Returns `None` if no path exists. The returned path includes
    /// both the start and end locations.
    pub fn shortest_path(&self, from: LocationId, to: LocationId) -> Option<Vec<LocationId>> {
        self.shortest_path_filtered(from, to, |_, _, _| true)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::loader::test_graph_json;

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
    fn test_shortest_path_filtered_empty_filter() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(
            graph
                .shortest_path_filtered(LocationId(1), LocationId(2), |_, _, _| false)
                .is_none()
        );
    }

    #[test]
    fn test_shortest_path_filtered_same_location() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let path = graph
            .shortest_path_filtered(LocationId(1), LocationId(1), |_, _, _| false)
            .unwrap();
        assert_eq!(path, vec![LocationId(1)]);
    }

    #[test]
    fn test_shortest_path_filtered_nonexistent_target() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(
            graph
                .shortest_path_filtered(LocationId(1), LocationId(99), |_, _, _| true)
                .is_none()
        );
    }

    #[test]
    fn test_shortest_path_filtered_only_target_edges() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // Only allow edges that end at the target (id 2).
        let path = graph
            .shortest_path_filtered(LocationId(1), LocationId(2), |_, to, _| to == LocationId(2))
            .unwrap();
        assert_eq!(path, vec![LocationId(1), LocationId(2)]);
    }

    #[test]
    fn test_shortest_path_filtered_always_true_matches_unfiltered() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let unfiltered = graph.shortest_path(LocationId(2), LocationId(4)).unwrap();
        let filtered = graph
            .shortest_path_filtered(LocationId(2), LocationId(4), |_, _, _| true)
            .unwrap();
        assert_eq!(unfiltered, filtered);
    }

    #[test]
    fn test_shortest_path_uses_bfs_hops_not_fastest_travel_time() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "Start",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.0,
                    "connections": [
                        {"target": 2, "path_description": "short-hop detour"},
                        {"target": 3, "path_description": "longer-hop direct road"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Far Detour",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "lat": 10.0,
                    "lon": 0.0,
                    "connections": [
                        {"target": 1, "path_description": "short-hop detour"},
                        {"target": 5, "path_description": "short-hop detour"}
                    ]
                },
                {
                    "id": 3,
                    "name": "Near Middle One",
                    "description_template": "C",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.001,
                    "connections": [
                        {"target": 1, "path_description": "longer-hop direct road"},
                        {"target": 4, "path_description": "longer-hop direct road"}
                    ]
                },
                {
                    "id": 4,
                    "name": "Near Middle Two",
                    "description_template": "D",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.002,
                    "connections": [
                        {"target": 3, "path_description": "longer-hop direct road"},
                        {"target": 5, "path_description": "longer-hop direct road"}
                    ]
                },
                {
                    "id": 5,
                    "name": "Finish",
                    "description_template": "E",
                    "indoor": false,
                    "public": true,
                    "lat": 0.0,
                    "lon": 0.003,
                    "connections": [
                        {"target": 2, "path_description": "short-hop detour"},
                        {"target": 4, "path_description": "longer-hop direct road"}
                    ]
                }
            ]
        }"#;
        let graph = WorldGraph::load_from_str(json).unwrap();
        let shorter_by_hops = vec![LocationId(1), LocationId(2), LocationId(5)];
        let longer_by_hops = vec![LocationId(1), LocationId(3), LocationId(4), LocationId(5)];

        assert_eq!(
            graph.shortest_path(LocationId(1), LocationId(5)).unwrap(),
            shorter_by_hops
        );
        assert!(
            graph.path_travel_time(&shorter_by_hops, 1.25)
                > graph.path_travel_time(&longer_by_hops, 1.25),
            "this contract is hop count, not a weighted route search"
        );
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
