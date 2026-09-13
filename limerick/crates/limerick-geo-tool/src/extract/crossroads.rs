//! Crossroads (road-junction) extraction from the road network.
//!
//! A junction is a node shared by 3+ ways; each becomes a natural location
//! node in the world graph. Split out of the monolithic `extract` module
//! (#1200).

use std::collections::{HashMap, HashSet};

use tracing::warn;

use super::super::osm_model::{GeoFeature, LocationType, OverpassResponse};
use super::dedup::deduplicate_by_proximity;

/// Extracts junction nodes (crossroads) from road network data.
///
/// A junction is a node that appears in 3 or more ways (roads meeting at a point).
/// These make natural location nodes in the world graph.
pub fn extract_crossroads(road_response: &OverpassResponse) -> Vec<GeoFeature> {
    let mut node_ways: HashMap<i64, HashSet<i64>> = HashMap::new();
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    for element in &road_response.elements {
        if element.element_type != "way" {
            continue;
        }

        // Count unique ways each node belongs to (deduplicate repeated nodes within one way)
        if let Some(ref geometry) = element.geometry {
            if let Some(ref nodes) = element.nodes {
                let mut seen = HashSet::new();
                for (i, point) in geometry.iter().enumerate() {
                    if i < nodes.len() && seen.insert(nodes[i]) {
                        node_ways.entry(nodes[i]).or_default().insert(element.id);
                        node_coords.insert(nodes[i], (point.lat, point.lon));
                    }
                }
            }
        } else if let Some(ref nodes) = element.nodes {
            let unique_nodes: HashSet<i64> = nodes.iter().copied().collect();
            for node_id in unique_nodes {
                node_ways.entry(node_id).or_default().insert(element.id);
            }
        }
    }

    // Nodes belonging to 3+ unique ways are junctions
    let mut crossroads = Vec::new();
    for (node_id, ways) in &node_ways {
        if ways.len() >= 3 {
            if let Some(&(lat, lon)) = node_coords.get(node_id) {
                crossroads.push(GeoFeature {
                    osm_id: *node_id,
                    osm_type: "node".to_string(),
                    lat,
                    lon,
                    name: "A Crossroads".to_string(),
                    name_ga: None,
                    location_type: LocationType::Crossroads,
                    tags: HashMap::new(),
                    curated: false,
                });
            } else {
                warn!("junction node {node_id} has no coordinates — skipping");
            }
        }
    }

    // Deduplicate crossroads within 50m of each other
    deduplicate_by_proximity(&mut crossroads, 50.0);

    crossroads
}
