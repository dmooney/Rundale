use std::collections::HashMap;

use super::osm_model::{GeoFeature, LocationType};

#[cfg(test)]
pub(crate) fn make_feature(name: &str, loc_type: LocationType, lat: f64, lon: f64) -> GeoFeature {
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
