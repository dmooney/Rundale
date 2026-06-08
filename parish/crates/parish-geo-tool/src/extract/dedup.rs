//! Proximity-based feature de-duplication.
//!
//! Shared by both [`super::extract_features`] (POI pass, 30 m) and
//! [`super::crossroads::extract_crossroads`] (junction pass, 50 m). Split out
//! of the monolithic `extract` module (#1200).

use std::collections::HashSet;

use tracing::debug;

use super::super::osm_model::{GeoFeature, LocationType, haversine_distance};

/// Deduplicates features that are within `threshold_meters` of each other.
///
/// When duplicates are found, keeps the one with the more specific
/// location type (non-Other > Other, named > unnamed).
pub fn deduplicate_by_proximity(features: &mut Vec<GeoFeature>, threshold_meters: f64) {
    let mut to_remove = HashSet::new();

    for i in 0..features.len() {
        if to_remove.contains(&i) {
            continue;
        }
        for j in (i + 1)..features.len() {
            if to_remove.contains(&j) {
                continue;
            }
            let dist = haversine_distance(
                features[i].lat,
                features[i].lon,
                features[j].lat,
                features[j].lon,
            );
            if dist < threshold_meters {
                // Keep the more specific/better-named one
                if features[j].location_type != LocationType::Other
                    && features[i].location_type == LocationType::Other
                {
                    to_remove.insert(i);
                } else {
                    to_remove.insert(j);
                }
            }
        }
    }

    // Remove in reverse index order to preserve indices
    let mut remove_indices: Vec<usize> = to_remove.into_iter().collect();
    remove_indices.sort_unstable_by(|a, b| b.cmp(a));
    for idx in remove_indices {
        let removed = features.remove(idx);
        debug!(
            "deduplicated: removed '{}' (too close to another feature)",
            removed.name
        );
    }
}
