//! Output generation — converts tracked locations to the parish.json format.
//!
//! Takes the final list of [`TrackedLocation`]s and writes a validated
//! parish.json file that the game engine can load directly.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

use parish_core::world::graph::LocationData;

use super::descriptions::DescriptionSource;
use super::merge::TrackedLocation;

/// Extended location data with provenance metadata.
///
/// Written alongside the standard parish.json for tooling use.
/// The game engine reads the standard format; this adds metadata
/// for the parish-geo-tool pipeline.
#[derive(Debug, Serialize, Deserialize)]
pub struct LocationMetadata {
    /// Location ID.
    pub id: u32,
    /// How the description was generated.
    pub description_source: DescriptionSource,
    /// OSM element id, if sourced from OSM.
    pub osm_id: Option<i64>,
    /// Latitude.
    pub lat: f64,
    /// Longitude.
    pub lon: f64,
}

/// Metadata file written alongside parish.json for tooling.
#[derive(Debug, Serialize, Deserialize)]
pub struct ParishMetadata {
    /// Tool version that generated this data.
    pub generator: String,
    /// Generation timestamp.
    pub generated_at: String,
    /// Per-location metadata.
    pub locations: Vec<LocationMetadata>,
}

/// The standard parish.json file format (game-loadable).
#[derive(Debug, Serialize, Deserialize)]
struct ParishFile {
    locations: Vec<LocationData>,
}

/// Writes the final parish.json and metadata files.
///
/// - `output_path`: path for the game-loadable parish.json
/// - `locations`: the final merged/generated location list
///
/// Also writes a `<output_path>.meta.json` with provenance metadata.
pub fn write_output(output_path: &Path, locations: &[TrackedLocation]) -> Result<()> {
    // Extract game-format location data
    let location_data: Vec<LocationData> = locations.iter().map(|loc| loc.data.clone()).collect();

    let parish_file = ParishFile {
        locations: location_data,
    };

    // Write game-loadable JSON
    let json =
        serde_json::to_string_pretty(&parish_file).context("failed to serialize parish data")?;

    // Ensure parent directory exists
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory: {}", parent.display()))?;
    }

    std::fs::write(output_path, &json)
        .with_context(|| format!("failed to write output: {}", output_path.display()))?;

    info!(
        "wrote {} locations to {}",
        locations.len(),
        output_path.display()
    );

    // Write metadata sidecar
    let meta_path = output_path.with_extension("meta.json");
    let metadata = ParishMetadata {
        generator: format!("parish-geo-tool {}", env!("CARGO_PKG_VERSION")),
        generated_at: chrono::Utc::now().to_rfc3339(),
        locations: locations
            .iter()
            .map(|loc| LocationMetadata {
                id: loc.data.id.0,
                description_source: loc.description_source,
                osm_id: loc.osm_id,
                lat: loc.lat,
                lon: loc.lon,
            })
            .collect(),
    };

    let meta_json =
        serde_json::to_string_pretty(&metadata).context("failed to serialize metadata")?;
    std::fs::write(&meta_path, &meta_json)
        .with_context(|| format!("failed to write metadata: {}", meta_path.display()))?;

    info!("wrote metadata to {}", meta_path.display());

    Ok(())
}

/// Validates that the generated output can be loaded by the game engine.
///
/// Performs the same validation as `WorldGraph::load_from_str` to catch
/// issues before they reach the game.
pub fn validate_output(output_path: &Path) -> Result<()> {
    let json = std::fs::read_to_string(output_path).with_context(|| {
        format!(
            "failed to read output for validation: {}",
            output_path.display()
        )
    })?;

    parish_core::world::graph::WorldGraph::load_from_str(&json)
        .context("generated parish data failed validation")?;

    info!("output validation passed");
    Ok(())
}

/// Prints a summary of the generated data to stdout.
pub fn print_summary(locations: &[TrackedLocation]) {
    let curated_count = locations
        .iter()
        .filter(|l| l.description_source == DescriptionSource::Curated)
        .count();
    let template_count = locations
        .iter()
        .filter(|l| l.description_source == DescriptionSource::Template)
        .count();
    let total_connections: usize = locations.iter().map(|l| l.data.connections.len()).sum();

    println!("\n=== parish-geo-tool output summary ===");
    println!("Total locations: {}", locations.len());
    println!("  Curated (hand-authored): {curated_count}");
    println!("  Template (auto-generated): {template_count}");
    println!(
        "Total connections: {total_connections} (bidirectional pairs: {})",
        total_connections / 2
    );

    // Count by location type (from metadata)
    let mut type_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for loc in locations {
        let type_name = if loc.description_source == DescriptionSource::Curated {
            "curated".to_string()
        } else {
            "generated".to_string()
        };
        *type_counts.entry(type_name).or_insert(0) += 1;
    }

    println!("================================\n");
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::world::LocationId;
    use parish_core::world::graph::{Connection, GeoKind};

    fn make_tracked_locations() -> Vec<TrackedLocation> {
        vec![
            TrackedLocation {
                data: LocationData {
                    id: LocationId(1),
                    name: "Place A".to_string(),
                    description_template: "A at {time}. Weather: {weather}.".to_string(),
                    indoor: false,
                    public: true,
                    lat: 53.5,
                    lon: -8.0,
                    connections: vec![Connection {
                        target: LocationId(2),
                        traversal_minutes: None,
                        path_description: "a path to B".to_string(),
                        hazard: Default::default(),
                        compass_heading: None,
                    }],
                    associated_npcs: vec![],
                    mythological_significance: None,
                    aliases: vec![],
                    geo_kind: GeoKind::Real,
                    relative_to: None,
                    geo_source: None,
                },
                description_source: DescriptionSource::Template,
                osm_id: Some(12345),
                lat: 53.5,
                lon: -8.0,
            },
            TrackedLocation {
                data: LocationData {
                    id: LocationId(2),
                    name: "Place B".to_string(),
                    description_template: "B at {time}. Weather: {weather}.".to_string(),
                    indoor: true,
                    public: true,
                    lat: 53.501,
                    lon: -8.0,
                    connections: vec![Connection {
                        target: LocationId(1),
                        traversal_minutes: None,
                        path_description: "a path to A".to_string(),
                        hazard: Default::default(),
                        compass_heading: None,
                    }],
                    associated_npcs: vec![],
                    mythological_significance: None,
                    aliases: vec![],
                    geo_kind: GeoKind::Real,
                    relative_to: None,
                    geo_source: None,
                },
                description_source: DescriptionSource::Template,
                osm_id: Some(67890),
                lat: 53.501,
                lon: -8.0,
            },
        ]
    }

    #[test]
    fn test_write_and_validate_output() {
        let dir = tempfile::tempdir().unwrap();
        let output_path = dir.path().join("test_parish.json");
        let locations = make_tracked_locations();

        write_output(&output_path, &locations).unwrap();
        assert!(output_path.exists());

        // Validate the output
        validate_output(&output_path).unwrap();

        // Check metadata was written
        let meta_path = output_path.with_extension("meta.json");
        assert!(meta_path.exists());

        // Verify metadata content
        let meta_json = std::fs::read_to_string(&meta_path).unwrap();
        let metadata: ParishMetadata = serde_json::from_str(&meta_json).unwrap();
        assert_eq!(metadata.locations.len(), 2);
        assert_eq!(metadata.locations[0].id, 1);
        assert_eq!(metadata.locations[0].osm_id, Some(12345));
    }

    #[test]
    fn test_print_summary_does_not_panic() {
        let locations = make_tracked_locations();
        print_summary(&locations); // Just verify it doesn't panic
    }
}
