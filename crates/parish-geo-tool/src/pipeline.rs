//! Pipeline — orchestrates the full geo-data conversion workflow.
//!
//! Coordinates the download → extract → connect → describe → merge → output
//! pipeline, managing configuration and error handling across stages.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tracing::info;

use parish_core::world::LocationId;
use parish_core::world::graph::{Connection, GeoKind, LocationData};

use super::AdminLevel;
use super::cache::ResponseCache;
use super::connections::generate_connections;
use super::descriptions::{generate_description, generate_mythological_significance};
use super::extract::{extract_crossroads, extract_features};
use super::lod::{DetailLevel, filter_by_detail};
use super::merge::{self, TrackedLocation};
use super::osm_model::GeoFeature;
use super::output;
use super::overpass::{self, BoundingBox, OverpassClient};

/// Configuration for the pipeline.
pub struct PipelineConfig {
    /// Named area to query.
    pub area: Option<String>,
    /// Bounding box to query.
    pub bbox: Option<BoundingBox>,
    /// Administrative level.
    pub level: AdminLevel,
    /// Detail level.
    pub detail: DetailLevel,
    /// Path to existing parish.json for merging.
    pub merge_path: Option<PathBuf>,
    /// Output file path.
    pub output_path: PathBuf,
    /// Cache directory.
    pub cache_dir: PathBuf,
    /// Skip cache.
    pub no_cache: bool,
    /// Dry run mode.
    pub dry_run: bool,
    /// Explicit ID offset.
    pub id_offset: Option<u32>,
    /// Maximum locations to generate (0 = unlimited).
    pub max_locations: usize,
}

/// Maximum direct distance (in meters) for generating connections.
/// Features further apart than this won't be directly connected
/// (they'll connect through intermediate nodes).
const MAX_CONNECTION_DISTANCE_M: f64 = 2000.0;

/// Runs the full geo-data conversion pipeline.
pub async fn run(config: PipelineConfig) -> Result<()> {
    // Validate inputs
    if config.area.is_none() && config.bbox.is_none() {
        bail!("must specify either --area or --bbox");
    }

    // Dry run mode — just show queries and exit
    if config.dry_run {
        let queries = overpass::dry_run_queries(config.area.as_deref(), config.bbox, config.level);
        println!("=== Dry run — queries that would be executed ===\n");
        for (label, query) in &queries {
            println!("--- {label} ---");
            println!("{query}\n");
        }
        println!("Total queries: {}", queries.len());
        return Ok(());
    }

    // Set up cache
    let cache =
        ResponseCache::new(&config.cache_dir).context("failed to initialize response cache")?;
    let client = OverpassClient::new(cache, config.no_cache);

    // Stage 1: Download OSM data
    info!("stage 1: downloading OSM data");
    let (poi_response, road_response) = if let Some(ref area_name) = config.area {
        let pois = client
            .query_area_pois(area_name, config.level)
            .await
            .context("failed to query POIs")?;
        let roads = client
            .query_area_roads(area_name, config.level)
            .await
            .context("failed to query roads")?;
        (pois, roads)
    } else if let Some(bbox) = config.bbox {
        let pois = client
            .query_bbox_pois(bbox)
            .await
            .context("failed to query POIs")?;
        let roads = client
            .query_bbox_roads(bbox)
            .await
            .context("failed to query roads")?;
        (pois, roads)
    } else {
        unreachable!("validated above");
    };

    println!(
        "Downloaded: {} POI elements, {} road elements",
        poi_response.elements.len(),
        road_response.elements.len()
    );

    // Stage 2: Extract features
    info!("stage 2: extracting features");
    let mut features = extract_features(&poi_response);
    println!("Extracted: {} location features", features.len());

    // Extract crossroads from road network
    let crossroads = extract_crossroads(&road_response);
    println!("Extracted: {} crossroads", crossroads.len());
    features.extend(crossroads);

    // Stage 3: Apply LOD filtering
    info!("stage 3: applying LOD filter ({:?})", config.detail);
    features = filter_by_detail(features, config.detail);
    println!("After LOD filter: {} features", features.len());

    // Apply max locations limit
    if config.max_locations > 0 && features.len() > config.max_locations {
        features.truncate(config.max_locations);
        println!(
            "Truncated to {} features (--max-locations)",
            config.max_locations
        );
    }

    if features.is_empty() {
        bail!("no features extracted — check your area name and admin level");
    }

    // Stage 4: Generate connections
    info!("stage 4: generating connections");
    let connections = generate_connections(&features, &road_response, MAX_CONNECTION_DISTANCE_M);
    println!("Generated: {} connections", connections.len());

    // Stage 5: Build location data
    info!("stage 5: building location data with descriptions");
    let id_offset = merge::determine_id_offset(config.merge_path.as_deref(), config.id_offset)?;
    let mut tracked_locations = build_locations(&features, &connections, id_offset);
    println!("Built: {} locations", tracked_locations.len());

    // Stage 6: Merge with existing data (if requested)
    if let Some(ref merge_path) = config.merge_path
        && merge_path.exists()
    {
        info!(
            "stage 6: merging with existing data from {}",
            merge_path.display()
        );
        let curated = merge::load_existing(merge_path)?;
        println!("Merging with {} curated locations", curated.len());
        tracked_locations = merge::merge_locations(curated, tracked_locations, 50.0);
        println!("After merge: {} total locations", tracked_locations.len());
    }

    // Stage 7: Write output
    info!("stage 7: writing output");
    output::write_output(&config.output_path, &tracked_locations)?;

    // Stage 8: Validate
    info!("stage 8: validating output");
    match output::validate_output(&config.output_path) {
        Ok(()) => println!("Validation: PASSED"),
        Err(e) => {
            println!("Validation: FAILED — {e}");
            println!("Output was written but may not load correctly in the game.");
            println!(
                "This usually means the connection graph has issues (orphans, missing targets, or non-bidirectional edges)."
            );
        }
    }

    // Summary
    output::print_summary(&tracked_locations);

    Ok(())
}

/// Converts extracted features and connections into tracked location data.
fn build_locations(
    features: &[GeoFeature],
    connections: &[super::connections::GeneratedConnection],
    id_offset: u32,
) -> Vec<TrackedLocation> {
    // Build connection lists per feature index
    let mut conn_map: std::collections::HashMap<usize, Vec<Connection>> =
        std::collections::HashMap::new();

    for conn in connections {
        let from_id = LocationId(
            u32::try_from(conn.from_idx).expect("feature index fits in u32") + id_offset,
        );
        let to_id =
            LocationId(u32::try_from(conn.to_idx).expect("feature index fits in u32") + id_offset);

        // Forward connection
        conn_map.entry(conn.from_idx).or_default().push(Connection {
            target: to_id,
            traversal_minutes: None,
            path_description: conn.path_description.clone(),
        });

        // Reverse connection (bidirectional)
        conn_map.entry(conn.to_idx).or_default().push(Connection {
            target: from_id,
            traversal_minutes: None,
            path_description: conn.reverse_path_description.clone(),
        });
    }

    features
        .iter()
        .enumerate()
        .map(|(idx, feature)| {
            let id = LocationId(u32::try_from(idx).expect("feature index fits in u32") + id_offset);
            let (description_template, description_source) = generate_description(feature);
            let mythological_significance = generate_mythological_significance(feature);
            let connections = conn_map.remove(&idx).unwrap_or_default();

            TrackedLocation {
                data: LocationData {
                    id,
                    name: feature.name.clone(),
                    description_template,
                    indoor: feature.location_type.is_indoor(),
                    public: feature.location_type.is_public(),
                    lat: feature.lat,
                    lon: feature.lon,
                    connections,
                    associated_npcs: vec![],
                    mythological_significance,
                    aliases: vec![],
                    geo_kind: GeoKind::Real,
                    relative_to: None,
                    geo_source: None,
                },
                description_source,
                osm_id: Some(feature.osm_id),
                lat: feature.lat,
                lon: feature.lon,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_model::{GeoFeature, LocationType};

    #[test]
    fn test_build_locations_basic() {
        let features = vec![
            GeoFeature {
                osm_id: 100,
                osm_type: "node".to_string(),
                lat: 53.5,
                lon: -8.0,
                name: "Church".to_string(),
                name_ga: None,
                location_type: LocationType::Church,
                tags: std::collections::HashMap::new(),
                curated: false,
            },
            GeoFeature {
                osm_id: 200,
                osm_type: "node".to_string(),
                lat: 53.501,
                lon: -8.0,
                name: "Pub".to_string(),
                name_ga: None,
                location_type: LocationType::Pub,
                tags: std::collections::HashMap::new(),
                curated: false,
            },
        ];

        let connections = vec![super::super::connections::GeneratedConnection {
            from_idx: 0,
            to_idx: 1,
            path_description: "a path to the pub".to_string(),
            reverse_path_description: "a path to the church".to_string(),
        }];

        let result = build_locations(&features, &connections, 1);

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].data.id, LocationId(1));
        assert_eq!(result[0].data.name, "Church");
        assert!(result[0].data.indoor); // Church is indoor
        assert_eq!(result[0].data.connections.len(), 1);
        assert_eq!(result[0].data.connections[0].target, LocationId(2));

        assert_eq!(result[1].data.id, LocationId(2));
        assert_eq!(result[1].data.name, "Pub");
        assert_eq!(result[1].data.connections.len(), 1);
        assert_eq!(result[1].data.connections[0].target, LocationId(1));

        // Verify geo-coordinates are propagated to LocationData
        assert!((result[0].data.lat - 53.5).abs() < f64::EPSILON);
        assert!((result[0].data.lon - -8.0).abs() < f64::EPSILON);
        assert!((result[1].data.lat - 53.501).abs() < f64::EPSILON);
    }

    #[test]
    fn test_build_locations_with_offset() {
        let features = vec![GeoFeature {
            osm_id: 100,
            osm_type: "node".to_string(),
            lat: 53.5,
            lon: -8.0,
            name: "Church".to_string(),
            name_ga: None,
            location_type: LocationType::Church,
            tags: std::collections::HashMap::new(),
            curated: false,
        }];

        let result = build_locations(&features, &[], 50);
        assert_eq!(result[0].data.id, LocationId(50));
    }

    #[test]
    fn test_build_locations_mythological() {
        let features = vec![GeoFeature {
            osm_id: 100,
            osm_type: "node".to_string(),
            lat: 53.5,
            lon: -8.0,
            name: "The Rath".to_string(),
            name_ga: None,
            location_type: LocationType::RingFort,
            tags: std::collections::HashMap::new(),
            curated: false,
        }];

        let result = build_locations(&features, &[], 1);
        assert!(result[0].data.mythological_significance.is_some());
        assert!(
            result[0]
                .data
                .mythological_significance
                .as_ref()
                .unwrap()
                .contains("sídhe")
        );
    }
}
