//! Merge logic — combines auto-generated locations with hand-authored data.
//!
//! Supports two modes:
//! - **Merge**: Hand-authored (curated) locations are preserved and take priority.
//!   Generated locations fill gaps and connect to existing ones.
//! - **Replace**: Generate everything fresh. Existing data is overwritten.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;

use parish_core::world::LocationId;
use parish_core::world::graph::{LocationData, WorldGraph};

use super::descriptions::DescriptionSource;
use super::osm_model::haversine_distance;

/// A location with provenance tracking.
#[derive(Debug, Clone)]
pub struct TrackedLocation {
    /// The location data in game format.
    pub data: LocationData,
    /// How the description was generated.
    pub description_source: DescriptionSource,
    /// OSM element id (None for curated locations).
    pub osm_id: Option<i64>,
    /// Latitude (for distance calculations during merge).
    pub lat: f64,
    /// Longitude (for distance calculations during merge).
    pub lon: f64,
}

/// Loads existing hand-authored locations from a parish.json file.
///
/// All loaded locations are marked as `Curated` and preserved during merge.
pub fn load_existing(path: &Path) -> Result<Vec<TrackedLocation>> {
    let graph = WorldGraph::load_from_file(path)
        .with_context(|| format!("failed to load existing parish file: {}", path.display()))?;

    let mut locations = Vec::new();
    for loc_id in graph.location_ids() {
        if let Some(data) = graph.get(loc_id) {
            locations.push(TrackedLocation {
                data: data.clone(),
                description_source: DescriptionSource::Curated,
                osm_id: None,
                lat: data.lat,
                lon: data.lon,
            });
        }
    }

    info!(
        "loaded {} curated locations from {}",
        locations.len(),
        path.display()
    );
    Ok(locations)
}

/// Merges generated locations with existing curated locations.
///
/// - Curated locations are always preserved with their original IDs.
/// - Generated locations that are too close to a curated location (within
///   `proximity_threshold_m`) are dropped.
/// - Generated locations get new IDs starting after the highest curated ID.
/// - Connections between curated and generated locations are created where
///   they are geographically close.
pub fn merge_locations(
    curated: Vec<TrackedLocation>,
    generated: Vec<TrackedLocation>,
    proximity_threshold_m: f64,
) -> Vec<TrackedLocation> {
    let max_curated_id = curated.iter().map(|loc| loc.data.id.0).max().unwrap_or(0);

    // Filter out generated locations too close to curated ones
    let filtered_generated: Vec<TrackedLocation> = generated
        .into_iter()
        .filter(|generated_loc| {
            // Skip proximity check for curated locations without coordinates
            let dominated = curated.iter().any(|cur| {
                if cur.lat == 0.0 && cur.lon == 0.0 {
                    // Fall back to name matching for curated without coords
                    cur.data.name.to_lowercase() == generated_loc.data.name.to_lowercase()
                } else {
                    haversine_distance(cur.lat, cur.lon, generated_loc.lat, generated_loc.lon)
                        < proximity_threshold_m
                }
            });
            if dominated {
                info!(
                    "dropping generated '{}' — too close to curated location",
                    generated_loc.data.name
                );
            }
            !dominated
        })
        .collect();

    let curated_ids: std::collections::HashSet<LocationId> =
        curated.iter().map(|loc| loc.data.id).collect();

    // Reassign IDs for generated locations
    let mut id_remap: HashMap<u32, u32> = HashMap::new();
    let mut result = curated;

    for (next_id, mut generated_loc) in (max_curated_id + 1..).zip(filtered_generated) {
        let old_id = generated_loc.data.id.0;
        if !curated_ids.contains(&LocationId(old_id)) {
            id_remap.insert(old_id, next_id);
        }
        generated_loc.data.id = LocationId(next_id);
        result.push(generated_loc);
    }

    let valid_ids: std::collections::HashSet<LocationId> =
        result.iter().map(|loc| loc.data.id).collect();

    // Remap and prune connection targets across all retained locations. Curated
    // locations may point at generated locations, so they need the same remap
    // and dropped-target cleanup as generated locations.
    for loc in &mut result {
        for conn in &mut loc.data.connections {
            if let Some(&new_id) = id_remap.get(&conn.target.0) {
                conn.target = LocationId(new_id);
            }
        }
        loc.data
            .connections
            .retain(|conn| valid_ids.contains(&conn.target));
    }

    result
}

/// Determines the starting ID offset for generated locations.
///
/// If merging, returns max existing ID + 1. Otherwise returns the
/// specified offset or 1.
pub fn determine_id_offset(merge_path: Option<&Path>, explicit_offset: Option<u32>) -> Result<u32> {
    if let Some(offset) = explicit_offset {
        return Ok(offset);
    }

    if let Some(path) = merge_path
        && path.exists()
    {
        let existing = load_existing(path)?;
        let max_id = existing.iter().map(|l| l.data.id.0).max().unwrap_or(0);
        return Ok(max_id + 1);
    }

    Ok(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::npc::NpcId;
    use parish_core::world::graph::GeoKind;

    fn make_tracked(
        id: u32,
        name: &str,
        source: DescriptionSource,
        lat: f64,
        lon: f64,
    ) -> TrackedLocation {
        TrackedLocation {
            data: LocationData {
                id: LocationId(id),
                name: name.to_string(),
                description_template: format!("{name} description. It is {{time}}."),
                landmarks: vec![],
                indoor: false,
                public: true,
                lat,
                lon,
                connections: Vec::new(),
                associated_npcs: Vec::<NpcId>::new(),
                mythological_significance: None,
                aliases: vec![],
                geo_kind: GeoKind::Real,
                relative_to: None,
                geo_source: None,
            },
            description_source: source,
            osm_id: None,
            lat,
            lon,
        }
    }

    #[test]
    fn test_merge_preserves_curated() {
        let curated = vec![make_tracked(
            1,
            "Church",
            DescriptionSource::Curated,
            0.0,
            0.0,
        )];
        let generated = vec![make_tracked(
            1,
            "Pub",
            DescriptionSource::Template,
            53.5,
            -8.0,
        )];

        let result = merge_locations(curated, generated, 50.0);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].data.name, "Church");
        assert_eq!(result[0].data.id, LocationId(1));
        assert_eq!(result[1].data.name, "Pub");
        assert_eq!(result[1].data.id, LocationId(2)); // Reassigned
    }

    #[test]
    fn test_merge_drops_duplicate_by_name() {
        let curated = vec![make_tracked(
            1,
            "The Church",
            DescriptionSource::Curated,
            0.0,
            0.0,
        )];
        let generated = vec![make_tracked(
            100,
            "the church",
            DescriptionSource::Template,
            53.5,
            -8.0,
        )];

        let result = merge_locations(curated, generated, 50.0);
        assert_eq!(result.len(), 1); // Duplicate dropped
        assert_eq!(result[0].data.name, "The Church");
    }

    #[test]
    fn test_merge_drops_by_proximity() {
        let curated = vec![make_tracked(
            1,
            "Church",
            DescriptionSource::Curated,
            53.5,
            -8.0,
        )];
        let generated = vec![
            make_tracked(
                100,
                "Nearby Thing",
                DescriptionSource::Template,
                53.5001,
                -8.0,
            ), // ~11m
            make_tracked(101, "Far Thing", DescriptionSource::Template, 53.6, -8.0), // ~11km
        ];

        let result = merge_locations(curated, generated, 50.0);
        assert_eq!(result.len(), 2); // Church + Far Thing
        assert_eq!(result[1].data.name, "Far Thing");
    }

    #[test]
    fn test_determine_id_offset_default() {
        let offset = determine_id_offset(None, None).unwrap();
        assert_eq!(offset, 1);
    }

    #[test]
    fn test_determine_id_offset_explicit() {
        let offset = determine_id_offset(None, Some(100)).unwrap();
        assert_eq!(offset, 100);
    }

    #[test]
    fn test_merge_remaps_generated_connection_targets() {
        let curated = vec![make_tracked(
            1,
            "Church",
            DescriptionSource::Curated,
            53.5,
            -8.0,
        )];

        // Generated locations with connections to each other
        let mut gen_a = make_tracked(100, "Pub A", DescriptionSource::Template, 53.6, -8.0);
        let mut gen_b = make_tracked(101, "Pub B", DescriptionSource::Template, 53.7, -8.0);
        gen_a
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: parish_core::world::LocationId(101),
                path_description: "to B".to_string(),
                hazard: Default::default(),
            });
        gen_b
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: parish_core::world::LocationId(100),
                path_description: "to A".to_string(),
                hazard: Default::default(),
            });

        let generated = vec![gen_a, gen_b];
        let result = merge_locations(curated, generated, 50.0);

        assert_eq!(result.len(), 3); // Church + Pub A + Pub B
        // Generated IDs should be remapped from 100,101 → 2,3
        let pub_a = result.iter().find(|l| l.data.name == "Pub A").unwrap();
        let pub_b = result.iter().find(|l| l.data.name == "Pub B").unwrap();
        assert_eq!(pub_a.data.id, parish_core::world::LocationId(2));
        assert_eq!(pub_b.data.id, parish_core::world::LocationId(3));

        // Connection targets should also be remapped
        assert_eq!(pub_a.data.connections.len(), 1);
        assert_eq!(
            pub_a.data.connections[0].target,
            parish_core::world::LocationId(3)
        );
        assert_eq!(pub_b.data.connections.len(), 1);
        assert_eq!(
            pub_b.data.connections[0].target,
            parish_core::world::LocationId(2)
        );
    }

    #[test]
    fn test_merge_prunes_connections_to_dropped_generated_locations() {
        let curated = vec![make_tracked(
            1,
            "Curated Church",
            DescriptionSource::Curated,
            53.5,
            -8.0,
        )];

        let mut kept = make_tracked(100, "Kept Pub", DescriptionSource::Template, 53.6, -8.0);
        kept.data
            .connections
            .push(parish_core::world::graph::Connection {
                target: LocationId(101),
                path_description: "to dropped duplicate".to_string(),
                hazard: Default::default(),
            });
        let dropped = make_tracked(
            101,
            "Generated Church Duplicate",
            DescriptionSource::Template,
            53.5001,
            -8.0,
        );

        let result = merge_locations(curated, vec![kept, dropped], 50.0);

        assert_eq!(result.len(), 2);
        let kept = result
            .iter()
            .find(|loc| loc.data.name == "Kept Pub")
            .unwrap();
        assert_eq!(kept.data.id, LocationId(2));
        assert!(
            kept.data.connections.is_empty(),
            "kept generated node must not retain an edge to dropped old id 101"
        );
    }

    #[test]
    fn test_merge_remaps_and_prunes_curated_connection_targets() {
        let mut curated = make_tracked(1, "Curated Church", DescriptionSource::Curated, 53.5, -8.0);
        curated
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: LocationId(100),
                path_description: "to kept generated".to_string(),
                hazard: Default::default(),
            });
        curated
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: LocationId(101),
                path_description: "to dropped duplicate".to_string(),
                hazard: Default::default(),
            });

        let kept = make_tracked(100, "Kept Pub", DescriptionSource::Template, 53.6, -8.0);
        let dropped = make_tracked(
            101,
            "Generated Church Duplicate",
            DescriptionSource::Template,
            53.5001,
            -8.0,
        );

        let result = merge_locations(vec![curated], vec![kept, dropped], 50.0);

        let curated = result
            .iter()
            .find(|loc| loc.data.name == "Curated Church")
            .unwrap();
        assert_eq!(curated.data.connections.len(), 1);
        assert_eq!(curated.data.connections[0].target, LocationId(2));
    }

    #[test]
    fn test_merge_does_not_remap_curated_connection_on_generated_id_collision() {
        let mut curated_origin =
            make_tracked(1, "Curated Church", DescriptionSource::Curated, 53.5, -8.0);
        curated_origin
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: LocationId(100),
                path_description: "to curated market".to_string(),
                hazard: Default::default(),
            });
        curated_origin
            .data
            .connections
            .push(parish_core::world::graph::Connection {
                target: LocationId(200),
                path_description: "to generated quay".to_string(),
                hazard: Default::default(),
            });

        let curated_target = make_tracked(
            100,
            "Curated Market",
            DescriptionSource::Curated,
            53.51,
            -8.0,
        );
        let colliding_generated = make_tracked(
            100,
            "Generated Market",
            DescriptionSource::Template,
            53.7,
            -8.0,
        );
        let generated_quay = make_tracked(
            200,
            "Generated Quay",
            DescriptionSource::Template,
            53.8,
            -8.0,
        );

        let result = merge_locations(
            vec![curated_origin, curated_target],
            vec![colliding_generated, generated_quay],
            50.0,
        );

        let curated = result
            .iter()
            .find(|loc| loc.data.name == "Curated Church")
            .unwrap();
        assert_eq!(
            curated
                .data
                .connections
                .iter()
                .map(|conn| conn.target)
                .collect::<Vec<_>>(),
            vec![LocationId(100), LocationId(102)]
        );
    }

    #[test]
    fn test_determine_id_offset_from_existing_file() {
        use parish_core::world::graph::{Connection, GeoKind, LocationData};
        use serde::Serialize;

        #[derive(Serialize)]
        struct TempWorldFile {
            locations: Vec<LocationData>,
        }

        let file = TempWorldFile {
            locations: vec![
                LocationData {
                    id: parish_core::world::LocationId(5),
                    name: "Old Church".to_string(),
                    description_template: "A church.".to_string(),
                    landmarks: vec![],
                    indoor: false,
                    public: true,
                    lat: 53.5,
                    lon: -8.0,
                    connections: vec![Connection {
                        target: parish_core::world::LocationId(12),
                        path_description: "to pub".to_string(),
                        hazard: Default::default(),
                    }],
                    associated_npcs: vec![],
                    mythological_significance: None,
                    aliases: vec![],
                    geo_kind: GeoKind::Real,
                    relative_to: None,
                    geo_source: None,
                },
                LocationData {
                    id: parish_core::world::LocationId(12),
                    name: "Old Pub".to_string(),
                    description_template: "A pub.".to_string(),
                    landmarks: vec![],
                    indoor: false,
                    public: true,
                    lat: 53.6,
                    lon: -8.0,
                    connections: vec![Connection {
                        target: parish_core::world::LocationId(5),
                        path_description: "to church".to_string(),
                        hazard: Default::default(),
                    }],
                    associated_npcs: vec![],
                    mythological_significance: None,
                    aliases: vec![],
                    geo_kind: GeoKind::Real,
                    relative_to: None,
                    geo_source: None,
                },
            ],
        };

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.json");
        let json = serde_json::to_string(&file).unwrap();
        std::fs::write(&path, json).unwrap();

        let offset = determine_id_offset(Some(&path), None).unwrap();
        assert_eq!(offset, 13); // max(5,12) + 1
    }
}
