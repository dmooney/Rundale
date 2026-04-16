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
use parish_core::world::graph::{Connection, LocationData, WorldGraph};

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

    // Reassign IDs for generated locations
    let mut id_remap: HashMap<u32, u32> = HashMap::new();
    let mut result = curated;

    for (next_id, mut generated_loc) in (max_curated_id + 1..).zip(filtered_generated) {
        let old_id = generated_loc.data.id.0;
        id_remap.insert(old_id, next_id);
        generated_loc.data.id = LocationId(next_id);
        result.push(generated_loc);
    }

    // Remap connection targets in generated locations
    for loc in &mut result {
        if loc.description_source != DescriptionSource::Curated {
            for conn in &mut loc.data.connections {
                if let Some(&new_id) = id_remap.get(&conn.target.0) {
                    conn.target = LocationId(new_id);
                }
            }
        }
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

/// Creates connections between curated and nearby generated locations.
///
/// Adds bidirectional connections between curated locations and the closest
/// generated locations within `max_distance_m`.
#[allow(dead_code)] // Public API for curated-to-generated linking (future use)
pub fn connect_curated_to_generated(locations: &mut [TrackedLocation], max_distance_m: f64) {
    // Collect indices and coordinates for generated locations with valid coords
    let gen_indices: Vec<(usize, f64, f64)> = locations
        .iter()
        .enumerate()
        .filter(|(_, loc)| loc.description_source != DescriptionSource::Curated && loc.lat != 0.0)
        .map(|(i, loc)| (i, loc.lat, loc.lon))
        .collect();

    if gen_indices.is_empty() {
        return;
    }

    // For each curated location, find closest generated locations
    let curated_indices: Vec<usize> = locations
        .iter()
        .enumerate()
        .filter(|(_, loc)| loc.description_source == DescriptionSource::Curated)
        .map(|(i, _)| i)
        .collect();

    let mut new_connections: Vec<(usize, usize, String, String)> = Vec::new();

    for &ci in &curated_indices {
        let cur = &locations[ci];
        // Find closest generated location (even if curated lacks coords, skip)
        if cur.lat == 0.0 && cur.lon == 0.0 {
            continue;
        }

        for &(gi, glat, glon) in &gen_indices {
            let dist = haversine_distance(cur.lat, cur.lon, glat, glon);
            if dist <= max_distance_m {
                let to_name = locations[gi].data.name.clone();
                let from_name = locations[ci].data.name.clone();

                // Check if connection already exists
                let already_connected = locations[ci]
                    .data
                    .connections
                    .iter()
                    .any(|c| c.target == locations[gi].data.id);

                if !already_connected {
                    new_connections.push((
                        ci,
                        gi,
                        format!("toward {to_name}"),
                        format!("toward {from_name}"),
                    ));
                }
            }
        }
    }

    // Apply connections (bidirectional)
    for (ci, gi, fwd_desc, rev_desc) in new_connections {
        let gen_id = locations[gi].data.id;
        let cur_id = locations[ci].data.id;

        locations[ci].data.connections.push(Connection {
            target: gen_id,
            traversal_minutes: None,
            path_description: fwd_desc,
        });
        locations[gi].data.connections.push(Connection {
            target: cur_id,
            traversal_minutes: None,
            path_description: rev_desc,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::npc::NpcId;

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
                indoor: false,
                public: true,
                lat,
                lon,
                connections: Vec::new(),
                associated_npcs: Vec::<NpcId>::new(),
                mythological_significance: None,
                aliases: vec![],
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
}
