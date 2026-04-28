//! Connection generation — builds edges between location nodes using road network data.
//!
//! Uses the OSM road network to determine which locations are connected.
//! Ensures the resulting graph is connected and all edges are bidirectional.
//! Traversal times are calculated at runtime from coordinates by parish-core.

use std::collections::{HashMap, HashSet, VecDeque};

use tracing::{debug, info, warn};

use super::osm_model::{GeoFeature, LatLon, OverpassResponse, haversine_distance};

/// A generated connection between two features.
#[derive(Debug, Clone)]
pub struct GeneratedConnection {
    /// Source feature index.
    pub from_idx: usize,
    /// Target feature index.
    pub to_idx: usize,
    /// Generated path description.
    pub path_description: String,
    /// Reverse path description (for bidirectional edge).
    pub reverse_path_description: String,
}

/// Road segment extracted from OSM data, used for proximity-based connections.
#[derive(Debug, Clone)]
struct RoadSegment {
    /// Points along the road geometry.
    points: Vec<LatLon>,
    /// Highway classification (e.g., "primary", "track", "path").
    highway_type: String,
    /// Road name, if any.
    name: Option<String>,
}

/// Generates connections between features using the road network.
///
/// For each pair of nearby features, determines if they are connected by
/// a road and calculates the walking distance along that road. If no road
/// connects nearby features, generates direct walking connections for
/// features within a maximum distance threshold.
pub fn generate_connections(
    features: &[GeoFeature],
    road_response: &OverpassResponse,
    max_direct_distance_m: f64,
) -> Vec<GeneratedConnection> {
    let road_segments = extract_road_segments(road_response);
    info!(
        "extracted {} road segments for connection generation",
        road_segments.len()
    );

    let mut connections = Vec::new();
    let mut connected_pairs: HashSet<(usize, usize)> = HashSet::new();

    // For each feature, find nearby features connected by roads
    for i in 0..features.len() {
        for j in (i + 1)..features.len() {
            let direct_dist = haversine_distance(
                features[i].lat,
                features[i].lon,
                features[j].lat,
                features[j].lon,
            );

            // Skip if too far apart for a direct connection
            if direct_dist > max_direct_distance_m {
                continue;
            }

            // Check if a road connects these features
            let road_dist = find_road_distance(&features[i], &features[j], &road_segments);

            let (desc, rev_desc) = if let Some((_, segment)) = road_dist {
                let desc = generate_path_description(&segment, &features[i], &features[j]);
                let rev_desc = generate_path_description(&segment, &features[j], &features[i]);
                (desc, rev_desc)
            } else {
                let desc = generate_direct_description(&features[i], &features[j]);
                let rev_desc = generate_direct_description(&features[j], &features[i]);
                (desc, rev_desc)
            };

            let pair = (i.min(j), i.max(j));
            if connected_pairs.insert(pair) {
                connections.push(GeneratedConnection {
                    from_idx: i,
                    to_idx: j,
                    path_description: desc,
                    reverse_path_description: rev_desc,
                });
            }
        }
    }

    // Ensure connectivity: if there are disconnected components, add bridge edges
    ensure_connectivity(features, &mut connections);

    info!("generated {} connections", connections.len());
    connections
}

/// Extracts road segments from the Overpass road response.
fn extract_road_segments(response: &OverpassResponse) -> Vec<RoadSegment> {
    let mut segments = Vec::new();

    for element in &response.elements {
        if element.element_type != "way" {
            continue;
        }
        let Some(ref geometry) = element.geometry else {
            continue;
        };
        if geometry.len() < 2 {
            continue;
        }

        let highway_type = element.tag("highway").unwrap_or("unclassified").to_string();
        let name = element.name().map(|s| s.to_string());

        segments.push(RoadSegment {
            points: geometry.clone(),
            highway_type,
            name,
        });
    }

    segments
}

/// Finds the road distance between two features, if a road passes near both.
///
/// A road is considered to connect two features if both are within 100m of
/// some point on the road. Returns the along-road distance and the road segment.
fn find_road_distance(
    a: &GeoFeature,
    b: &GeoFeature,
    segments: &[RoadSegment],
) -> Option<(f64, RoadSegment)> {
    const SNAP_THRESHOLD_M: f64 = 100.0;

    let mut best: Option<(f64, &RoadSegment)> = None;

    for segment in segments {
        // Find closest point on segment to feature A
        let snap_a = closest_point_on_road(&segment.points, a.lat, a.lon);
        if snap_a.distance > SNAP_THRESHOLD_M {
            continue;
        }

        // Find closest point on segment to feature B
        let snap_b = closest_point_on_road(&segment.points, b.lat, b.lon);
        if snap_b.distance > SNAP_THRESHOLD_M {
            continue;
        }

        // Calculate along-road distance between the two snap points
        let road_dist =
            along_road_distance(&segment.points, snap_a.segment_idx, snap_b.segment_idx);

        // Add the snap distances (off-road walk to the road)
        let total_dist = road_dist + snap_a.distance + snap_b.distance;

        if best.is_none() || total_dist < best.unwrap().0 {
            best = Some((total_dist, segment));
        }
    }

    best.map(|(dist, seg)| (dist, seg.clone()))
}

/// Result of snapping a point to the nearest road segment.
struct SnapResult {
    /// Distance from the point to the nearest road point, in meters.
    distance: f64,
    /// Index of the closest road segment point.
    segment_idx: usize,
}

/// Finds the closest point on a road to a given coordinate.
fn closest_point_on_road(points: &[LatLon], lat: f64, lon: f64) -> SnapResult {
    let mut best_dist = f64::MAX;
    let mut best_idx = 0;

    for (i, point) in points.iter().enumerate() {
        let dist = haversine_distance(lat, lon, point.lat, point.lon);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }

    SnapResult {
        distance: best_dist,
        segment_idx: best_idx,
    }
}

/// Calculates the along-road distance between two segment indices.
fn along_road_distance(points: &[LatLon], idx_a: usize, idx_b: usize) -> f64 {
    let start = idx_a.min(idx_b);
    let end = idx_a.max(idx_b);

    let mut total = 0.0;
    for i in start..end {
        if i + 1 < points.len() {
            total += haversine_distance(
                points[i].lat,
                points[i].lon,
                points[i + 1].lat,
                points[i + 1].lon,
            );
        }
    }
    total
}

/// Generates a path description based on the road type.
fn generate_path_description(segment: &RoadSegment, _from: &GeoFeature, to: &GeoFeature) -> String {
    let road_type_desc = match segment.highway_type.as_str() {
        "primary" | "secondary" => "the road",
        "tertiary" => "a country road",
        "unclassified" | "residential" => "a narrow road",
        "track" => "a rough track",
        "path" | "footway" => "a path",
        "bridleway" => "a boreen",
        "service" => "a lane",
        _ => "the way",
    };

    if let Some(ref road_name) = segment.name {
        format!("{road_type_desc} along {road_name} toward {}", to.name)
    } else {
        format!("{road_type_desc} toward {}", to.name)
    }
}

/// Generates a description for a direct (off-road) connection.
fn generate_direct_description(_from: &GeoFeature, to: &GeoFeature) -> String {
    format!("across the fields toward {}", to.name)
}

/// Ensures the feature graph is fully connected.
///
/// Uses BFS to find connected components. If there are multiple components,
/// adds the shortest possible bridge edges between them.
fn ensure_connectivity(features: &[GeoFeature], connections: &mut Vec<GeneratedConnection>) {
    if features.is_empty() {
        return;
    }

    // Build adjacency from current connections
    let mut adj: HashMap<usize, Vec<usize>> = HashMap::new();
    for conn in connections.iter() {
        adj.entry(conn.from_idx).or_default().push(conn.to_idx);
        adj.entry(conn.to_idx).or_default().push(conn.from_idx);
    }

    // Find connected components via BFS
    let mut visited = vec![false; features.len()];
    let mut components: Vec<Vec<usize>> = Vec::new();

    for start in 0..features.len() {
        if visited[start] {
            continue;
        }
        let mut component = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(start);
        visited[start] = true;

        while let Some(node) = queue.pop_front() {
            component.push(node);
            if let Some(neighbors) = adj.get(&node) {
                for &neighbor in neighbors {
                    if !visited[neighbor] {
                        visited[neighbor] = true;
                        queue.push_back(neighbor);
                    }
                }
            }
        }

        components.push(component);
    }

    if components.len() <= 1 {
        return;
    }

    warn!(
        "graph has {} disconnected components — adding bridge connections",
        components.len()
    );

    // Connect each component to the next by finding the closest pair of nodes
    for i in 0..components.len() - 1 {
        let comp_a = &components[i];
        let comp_b = &components[i + 1];

        let mut best_dist = f64::MAX;
        let mut best_pair = (0, 0);

        for &a in comp_a {
            for &b in comp_b {
                let dist = haversine_distance(
                    features[a].lat,
                    features[a].lon,
                    features[b].lat,
                    features[b].lon,
                );
                if dist < best_dist {
                    best_dist = dist;
                    best_pair = (a, b);
                }
            }
        }

        debug!(
            "bridging components: {} <-> {} ({}m)",
            features[best_pair.0].name, features[best_pair.1].name, best_dist as u64
        );

        connections.push(GeneratedConnection {
            from_idx: best_pair.0,
            to_idx: best_pair.1,
            path_description: generate_direct_description(
                &features[best_pair.0],
                &features[best_pair.1],
            ),
            reverse_path_description: generate_direct_description(
                &features[best_pair.1],
                &features[best_pair.0],
            ),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_model::{GeoFeature, LocationType};

    fn make_feature(name: &str, lat: f64, lon: f64) -> GeoFeature {
        GeoFeature {
            osm_id: 0,
            osm_type: "node".to_string(),
            lat,
            lon,
            name: name.to_string(),
            name_ga: None,
            location_type: LocationType::Pub,
            tags: HashMap::new(),
            curated: false,
        }
    }

    #[test]
    fn test_generate_connections_nearby() {
        let features = vec![
            make_feature("A", 53.5000, -8.0000),
            make_feature("B", 53.5010, -8.0000), // ~111m away
        ];
        let road_response = OverpassResponse {
            version: Some(0.6),
            elements: vec![],
        };

        let conns = generate_connections(&features, &road_response, 500.0);
        assert_eq!(conns.len(), 1);
        assert_eq!(conns[0].from_idx, 0);
        assert_eq!(conns[0].to_idx, 1);
    }

    #[test]
    fn test_generate_connections_too_far() {
        let features = vec![
            make_feature("A", 53.5, -8.0),
            make_feature("B", 54.0, -8.0), // ~55km away
        ];
        let road_response = OverpassResponse {
            version: Some(0.6),
            elements: vec![],
        };

        // With 500m max, no connection but bridge added for connectivity
        let conns = generate_connections(&features, &road_response, 500.0);
        // Should still get 1 connection (bridge for connectivity)
        assert_eq!(conns.len(), 1);
    }

    #[test]
    fn test_ensure_connectivity_single_component() {
        let features = vec![make_feature("A", 53.5, -8.0), make_feature("B", 53.5, -8.0)];
        let mut connections = vec![GeneratedConnection {
            from_idx: 0,
            to_idx: 1,
            path_description: "test".to_string(),
            reverse_path_description: "test".to_string(),
        }];

        let original_len = connections.len();
        ensure_connectivity(&features, &mut connections);
        // No new connections added
        assert_eq!(connections.len(), original_len);
    }

    #[test]
    fn test_ensure_connectivity_bridges_components() {
        let features = vec![
            make_feature("A", 53.5000, -8.0000),
            make_feature("B", 53.5001, -8.0000),
            make_feature("C", 53.6000, -8.0000), // far away, disconnected
        ];
        // Only A-B connected
        let mut connections = vec![GeneratedConnection {
            from_idx: 0,
            to_idx: 1,
            path_description: "test".to_string(),
            reverse_path_description: "test".to_string(),
        }];

        ensure_connectivity(&features, &mut connections);
        // Should add a bridge connection to C
        assert_eq!(connections.len(), 2);
    }

    #[test]
    fn test_along_road_distance() {
        let points = vec![
            LatLon {
                lat: 53.5000,
                lon: -8.0000,
            },
            LatLon {
                lat: 53.5010,
                lon: -8.0000,
            },
            LatLon {
                lat: 53.5020,
                lon: -8.0000,
            },
        ];

        let dist = along_road_distance(&points, 0, 2);
        // Should be approximately 222m (two ~111m segments)
        assert!(dist > 200.0 && dist < 250.0, "distance was {dist}");
    }

    #[test]
    fn test_path_description_generation() {
        let segment = RoadSegment {
            points: vec![],
            highway_type: "track".to_string(),
            name: None,
        };
        let from = make_feature("Church", 53.5, -8.0);
        let to = make_feature("Pub", 53.5, -8.0);

        let desc = generate_path_description(&segment, &from, &to);
        assert!(desc.contains("rough track"));
        assert!(desc.contains("Pub"));
    }

    #[test]
    fn test_path_description_with_road_name() {
        let segment = RoadSegment {
            points: vec![],
            highway_type: "tertiary".to_string(),
            name: Some("Bog Road".to_string()),
        };
        let from = make_feature("A", 53.5, -8.0);
        let to = make_feature("B", 53.5, -8.0);

        let desc = generate_path_description(&segment, &from, &to);
        assert!(desc.contains("Bog Road"));
    }
}
