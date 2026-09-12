//! Level-of-detail (LOD) filtering for geographic features.
//!
//! Controls location density by distance from a center point or by
//! administrative level. Denser areas get every building; sparser areas
//! keep only notable points of interest.

use clap::ValueEnum;

use super::osm_model::{GeoFeature, LocationType};

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
    use crate::test_utils::make_feature;

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
}
