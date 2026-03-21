//! Level-of-detail (LOD) filtering for geographic features.
//!
//! Controls location density by distance from a center point or by
//! administrative level. Denser areas get every building; sparser areas
//! keep only notable points of interest.

use clap::ValueEnum;

use super::osm_model::{GeoFeature, LocationType, haversine_distance};

/// Level of detail for location extraction.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DetailLevel {
    /// Every identifiable feature (buildings, wells, individual farms).
    Full,
    /// Notable POIs only (churches, pubs, schools, historic sites, named places).
    Notable,
    /// Towns and major landmarks only.
    Sparse,
}

/// Filters features by detail level.
///
/// - `Full`: keeps everything
/// - `Notable`: keeps features with significant location types or explicit names
/// - `Sparse`: keeps only towns, churches, and major historic sites
pub fn filter_by_detail(features: Vec<GeoFeature>, level: DetailLevel) -> Vec<GeoFeature> {
    match level {
        DetailLevel::Full => features,
        DetailLevel::Notable => features.into_iter().filter(is_notable).collect(),
        DetailLevel::Sparse => features.into_iter().filter(is_sparse_worthy).collect(),
    }
}

/// Filters features by distance from a center point with progressive LOD.
///
/// Within `inner_radius_m`, keeps all features. Between `inner_radius_m`
/// and `outer_radius_m`, keeps only notable features. Beyond `outer_radius_m`,
/// keeps only sparse-worthy features.
#[allow(dead_code)] // Public API for distance-based LOD (future use)
pub fn filter_by_distance(
    features: Vec<GeoFeature>,
    center_lat: f64,
    center_lon: f64,
    inner_radius_m: f64,
    outer_radius_m: f64,
) -> Vec<GeoFeature> {
    features
        .into_iter()
        .filter(|f| {
            let dist = haversine_distance(center_lat, center_lon, f.lat, f.lon);
            if dist <= inner_radius_m {
                true // Full detail
            } else if dist <= outer_radius_m {
                is_notable(f) // Medium detail
            } else {
                is_sparse_worthy(f) // Low detail
            }
        })
        .collect()
}

/// Returns true if a feature is "notable" — worth including at medium detail.
fn is_notable(feature: &GeoFeature) -> bool {
    matches!(
        feature.location_type,
        LocationType::Pub
            | LocationType::Church
            | LocationType::Shop
            | LocationType::School
            | LocationType::PostOffice
            | LocationType::Crossroads
            | LocationType::Bridge
            | LocationType::Well
            | LocationType::Waterside
            | LocationType::Bog
            | LocationType::RingFort
            | LocationType::StandingStone
            | LocationType::Graveyard
            | LocationType::Mill
            | LocationType::Forge
            | LocationType::LimeKiln
            | LocationType::Square
            | LocationType::Harbour
            | LocationType::Hill
            | LocationType::Ruin
            | LocationType::NamedPlace
    )
}

/// Returns true if a feature is significant enough for sparse detail.
fn is_sparse_worthy(feature: &GeoFeature) -> bool {
    matches!(
        feature.location_type,
        LocationType::Church
            | LocationType::Pub
            | LocationType::NamedPlace
            | LocationType::Harbour
            | LocationType::RingFort
            | LocationType::Ruin
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_feature(name: &str, loc_type: LocationType, lat: f64, lon: f64) -> GeoFeature {
        GeoFeature {
            osm_id: 0,
            osm_type: "node".to_string(),
            lat,
            lon,
            name: name.to_string(),
            name_ga: None,
            location_type: loc_type,
            tags: HashMap::new(),
            curated: false,
        }
    }

    #[test]
    fn test_filter_full_keeps_everything() {
        let features = vec![
            make_feature("Farm", LocationType::Farm, 53.5, -8.0),
            make_feature("Pub", LocationType::Pub, 53.5, -8.0),
            make_feature("Other", LocationType::Other, 53.5, -8.0),
        ];
        let result = filter_by_detail(features, DetailLevel::Full);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_notable_drops_farms_and_other() {
        let features = vec![
            make_feature("Farm", LocationType::Farm, 53.5, -8.0),
            make_feature("Pub", LocationType::Pub, 53.5, -8.0),
            make_feature("Other", LocationType::Other, 53.5, -8.0),
        ];
        let result = filter_by_detail(features, DetailLevel::Notable);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "Pub");
    }

    #[test]
    fn test_filter_sparse_keeps_few() {
        let features = vec![
            make_feature("Farm", LocationType::Farm, 53.5, -8.0),
            make_feature("Pub", LocationType::Pub, 53.5, -8.0),
            make_feature("Church", LocationType::Church, 53.5, -8.0),
            make_feature("Fort", LocationType::RingFort, 53.5, -8.0),
            make_feature("School", LocationType::School, 53.5, -8.0),
        ];
        let result = filter_by_detail(features, DetailLevel::Sparse);
        assert_eq!(result.len(), 3); // Pub, Church, Fort
    }

    #[test]
    fn test_filter_by_distance() {
        // Center at 53.5, -8.0
        let features = vec![
            // Close (0m) — keep all
            make_feature("Close Farm", LocationType::Farm, 53.5, -8.0),
            // Medium (~1.1km) — keep only notable
            make_feature("Medium Farm", LocationType::Farm, 53.51, -8.0),
            make_feature("Medium Church", LocationType::Church, 53.51, -8.0),
            // Far (~11km) — keep only sparse
            make_feature("Far School", LocationType::School, 53.6, -8.0),
            make_feature("Far Village", LocationType::NamedPlace, 53.6, -8.0),
        ];

        let result = filter_by_distance(features, 53.5, -8.0, 500.0, 5000.0);

        let names: Vec<&str> = result.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"Close Farm")); // Inner radius: keep all
        assert!(!names.contains(&"Medium Farm")); // Middle: farm not notable
        assert!(names.contains(&"Medium Church")); // Middle: church is notable
        assert!(!names.contains(&"Far School")); // Outer: school not sparse-worthy
        assert!(names.contains(&"Far Village")); // Outer: named place is sparse-worthy
    }
}
