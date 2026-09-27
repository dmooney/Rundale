//! Feature extraction — classifies OSM elements into game-relevant location types.
//!
//! Takes raw Overpass API responses and produces a deduplicated list of
//! [`GeoFeature`]s, each classified by [`LocationType`] based on OSM tags.
//!
//! Structure (#1200 decomposition): the former single module is split into
//! - [`classify`] — OSM-tag → [`LocationType`] classification + name generation;
//! - [`dedup`] — proximity de-duplication shared by both extraction passes;
//! - [`crossroads`] — road-junction extraction;
//! - this `mod.rs` — the POI pass [`extract_features`].
//!
//! `classify`/`crossroads` items are re-exported flat so the existing
//! `super::extract::{extract_features, extract_crossroads}` paths used by
//! `pipeline.rs` are unchanged.

mod classify;
mod crossroads;
mod dedup;

pub use classify::classify_element;
pub use crossroads::extract_crossroads;

use std::collections::HashSet;

use tracing::debug;

use self::classify::generate_name;
use self::dedup::deduplicate_by_proximity;
use super::osm_model::{GeoFeature, LocationType, OverpassResponse};

/// Extracts game-relevant geographic features from an Overpass POI response.
///
/// Classifies each element by OSM tags, filters out unnamed/unlocatable
/// features, and deduplicates by proximity (within 30m).
pub fn extract_features(response: &OverpassResponse) -> Vec<GeoFeature> {
    let mut features = Vec::new();
    let mut seen_osm_ids: HashSet<i64> = HashSet::new();

    for element in &response.elements {
        // Skip elements we've already processed (dedup by OSM id)
        if !seen_osm_ids.insert(element.id) {
            continue;
        }

        // Must have coordinates
        let Some(lat) = element.effective_lat() else {
            debug!("skipping element {} — no coordinates", element.id);
            continue;
        };
        let Some(lon) = element.effective_lon() else {
            continue;
        };

        // Classify the element
        let Some(location_type) = classify_element(element) else {
            debug!(
                "skipping element {} — unclassifiable tags: {:?}",
                element.id, element.tags
            );
            continue;
        };

        // Skip road-type elements (used for connections, not locations)
        if location_type == LocationType::Road {
            continue;
        }

        // Generate a name
        let name = match generate_name(element, location_type) {
            Some(n) => n,
            None => {
                debug!(
                    "skipping element {} ({:?}) — could not generate name",
                    element.id, location_type
                );
                continue;
            }
        };

        features.push(GeoFeature {
            osm_id: element.id,
            osm_type: element.element_type.clone(),
            lat,
            lon,
            name,
            name_ga: element.name_ga().map(|s| s.to_string()),
            location_type,
            tags: element.tags.clone(),
            curated: false,
        });
    }

    // Deduplicate by proximity (features within 30m of each other)
    deduplicate_by_proximity(&mut features, 30.0);

    features
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_model::{LatLon, OsmElement};
    use std::collections::HashMap;

    fn make_element(tags: Vec<(&str, &str)>) -> OsmElement {
        let mut tag_map = HashMap::new();
        for (k, v) in tags {
            tag_map.insert(k.to_string(), v.to_string());
        }
        OsmElement {
            element_type: "node".to_string(),
            id: 1,
            lat: Some(53.5),
            lon: Some(-8.0),
            center: None,
            tags: tag_map,
            nodes: None,
            geometry: None,
            members: None,
        }
    }

    #[test]
    fn test_classify_pub() {
        let elem = make_element(vec![("amenity", "pub"), ("name", "The Local")]);
        assert_eq!(classify_element(&elem), Some(LocationType::Pub));
    }

    #[test]
    fn test_classify_church() {
        let elem = make_element(vec![("amenity", "place_of_worship")]);
        assert_eq!(classify_element(&elem), Some(LocationType::Church));
    }

    #[test]
    fn test_classify_ring_fort() {
        let elem = make_element(vec![("historic", "ring_fort")]);
        assert_eq!(classify_element(&elem), Some(LocationType::RingFort));
    }

    #[test]
    fn test_classify_holy_well() {
        let elem = make_element(vec![("historic", "holy_well")]);
        assert_eq!(classify_element(&elem), Some(LocationType::Well));
    }

    #[test]
    fn test_classify_farm() {
        let elem = make_element(vec![("building", "farmhouse")]);
        assert_eq!(classify_element(&elem), Some(LocationType::Farm));
    }

    #[test]
    fn test_classify_bog() {
        let elem = make_element(vec![("natural", "wetland")]);
        assert_eq!(classify_element(&elem), Some(LocationType::Bog));
    }

    #[test]
    fn test_classify_named_place() {
        let elem = make_element(vec![("place", "townland"), ("name", "Kiltoom")]);
        assert_eq!(classify_element(&elem), Some(LocationType::NamedPlace));
    }

    #[test]
    fn test_classify_unknown() {
        let elem = make_element(vec![("highway", "motorway")]);
        assert_eq!(classify_element(&elem), None);
    }

    #[test]
    fn test_classify_documented_osm_branches() {
        let cases = [
            (vec![("amenity", "bar")], Some(LocationType::Pub)),
            (vec![("amenity", "restaurant")], Some(LocationType::Pub)),
            (
                vec![("amenity", "grave_yard")],
                Some(LocationType::Graveyard),
            ),
            (vec![("building", "chapel")], Some(LocationType::Church)),
            (vec![("building", "cathedral")], Some(LocationType::Church)),
            (vec![("building", "barn")], Some(LocationType::Farm)),
            (
                vec![("building", "yes"), ("name", "Named Building")],
                Some(LocationType::Other),
            ),
            (vec![("historic", "cashel")], Some(LocationType::RingFort)),
            (
                vec![("historic", "ogham_stone")],
                Some(LocationType::StandingStone),
            ),
            (vec![("historic", "monument")], Some(LocationType::Ruin)),
            (vec![("natural", "wood")], Some(LocationType::Woodland)),
            (vec![("natural", "hill")], Some(LocationType::Hill)),
            (vec![("natural", "spring")], Some(LocationType::Well)),
            (vec![("waterway", "stream")], Some(LocationType::Waterside)),
            (vec![("waterway", "canal")], Some(LocationType::Waterside)),
            (vec![("landuse", "farmyard")], Some(LocationType::Farm)),
            (vec![("landuse", "cemetery")], Some(LocationType::Graveyard)),
            (vec![("man_made", "watermill")], Some(LocationType::Mill)),
            (vec![("man_made", "windmill")], Some(LocationType::Mill)),
            (vec![("man_made", "quay")], Some(LocationType::Harbour)),
            (vec![("craft", "blacksmith")], Some(LocationType::Forge)),
            (vec![("ford", "yes")], Some(LocationType::Bridge)),
            (vec![("leisure", "marina")], Some(LocationType::Harbour)),
            (vec![("tourism", "hotel")], Some(LocationType::Pub)),
        ];

        for (tags, expected) in cases {
            let elem = make_element(tags.clone());
            assert_eq!(classify_element(&elem), expected, "tags: {tags:?}");
        }
    }

    #[test]
    fn test_generate_name_explicit() {
        let elem = make_element(vec![("name", "St. Brigid's Church")]);
        assert_eq!(
            generate_name(&elem, LocationType::Church),
            Some("St. Brigid's Church".to_string())
        );
    }

    #[test]
    fn test_generate_name_ring_fort() {
        let elem = make_element(vec![("historic", "ring_fort")]);
        assert_eq!(
            generate_name(&elem, LocationType::RingFort),
            Some("A Ring Fort".to_string())
        );
    }

    #[test]
    fn test_generate_name_unnamed_farm_returns_none() {
        let elem = make_element(vec![("building", "farm")]);
        assert_eq!(generate_name(&elem, LocationType::Farm), None);
    }

    #[test]
    fn test_deduplicate_by_proximity() {
        let mut features = vec![
            GeoFeature {
                osm_id: 1,
                osm_type: "node".to_string(),
                lat: 53.5000,
                lon: -8.0000,
                name: "Feature A".to_string(),
                name_ga: None,
                location_type: LocationType::Church,
                tags: HashMap::new(),
                curated: false,
            },
            GeoFeature {
                osm_id: 2,
                osm_type: "node".to_string(),
                lat: 53.5001, // ~11m away
                lon: -8.0000,
                name: "Feature B".to_string(),
                name_ga: None,
                location_type: LocationType::Other,
                tags: HashMap::new(),
                curated: false,
            },
            GeoFeature {
                osm_id: 3,
                osm_type: "node".to_string(),
                lat: 53.6000, // far away
                lon: -8.0000,
                name: "Feature C".to_string(),
                name_ga: None,
                location_type: LocationType::Pub,
                tags: HashMap::new(),
                curated: false,
            },
        ];

        deduplicate_by_proximity(&mut features, 30.0);

        // Feature A and B are within 30m — B (Other) should be removed, A (Church) kept
        assert_eq!(features.len(), 2);
        assert_eq!(features[0].name, "Feature A");
        assert_eq!(features[1].name, "Feature C");
    }

    #[test]
    fn test_extract_features_filters_unnamed() {
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "node".to_string(),
                    id: 1,
                    lat: Some(53.5),
                    lon: Some(-8.0),
                    center: None,
                    tags: {
                        let mut t = HashMap::new();
                        t.insert("building".to_string(), "yes".to_string());
                        t
                    },
                    nodes: None,
                    geometry: None,
                    members: None,
                },
                OsmElement {
                    element_type: "node".to_string(),
                    id: 2,
                    lat: Some(53.5),
                    lon: Some(-8.0),
                    center: None,
                    tags: {
                        let mut t = HashMap::new();
                        t.insert("amenity".to_string(), "pub".to_string());
                        t.insert("name".to_string(), "The Local".to_string());
                        t
                    },
                    nodes: None,
                    geometry: None,
                    members: None,
                },
            ],
        };

        let features = extract_features(&response);
        // First element has building=yes but no name → filtered
        // Second element is a named pub → kept
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].name, "The Local");
    }

    #[test]
    fn test_extract_features_filters_no_coords() {
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![OsmElement {
                element_type: "node".to_string(),
                id: 1,
                lat: None,
                lon: None,
                center: None,
                tags: {
                    let mut t = HashMap::new();
                    t.insert("amenity".to_string(), "pub".to_string());
                    t.insert("name".to_string(), "Nowhere Pub".to_string());
                    t
                },
                nodes: None,
                geometry: None,
                members: None,
            }],
        };

        let features = extract_features(&response);
        assert!(
            features.is_empty(),
            "element with no coords should be filtered"
        );
    }

    #[test]
    fn test_extract_features_filters_unclassifiable() {
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![OsmElement {
                element_type: "node".to_string(),
                id: 1,
                lat: Some(53.5),
                lon: Some(-8.0),
                center: None,
                tags: {
                    let mut t = HashMap::new();
                    t.insert("highway".to_string(), "secondary".to_string());
                    t.insert("name".to_string(), "Main Road".to_string());
                    t
                },
                nodes: None,
                geometry: None,
                members: None,
            }],
        };

        let features = extract_features(&response);
        // highway=secondary → classify_element returns None → filtered
        assert!(
            features.is_empty(),
            "unclassifiable element should be filtered"
        );
    }

    #[test]
    fn test_extract_features_deduplicates_osm_ids() {
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "node".to_string(),
                    id: 1,
                    lat: Some(53.5),
                    lon: Some(-8.0),
                    center: None,
                    tags: {
                        let mut t = HashMap::new();
                        t.insert("amenity".to_string(), "pub".to_string());
                        t.insert("name".to_string(), "The Local".to_string());
                        t
                    },
                    nodes: None,
                    geometry: None,
                    members: None,
                },
                OsmElement {
                    element_type: "node".to_string(),
                    id: 1, // Duplicate OSM id
                    lat: Some(53.5),
                    lon: Some(-8.0),
                    center: None,
                    tags: {
                        let mut t = HashMap::new();
                        t.insert("amenity".to_string(), "pub".to_string());
                        t.insert("name".to_string(), "The Local".to_string());
                        t
                    },
                    nodes: None,
                    geometry: None,
                    members: None,
                },
            ],
        };

        let features = extract_features(&response);
        assert_eq!(features.len(), 1);
    }

    // ── extract_crossroads tests ────────────────────────────────────────────

    #[test]
    fn test_extract_crossroads_empty_roads() {
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![],
        };
        let crossroads = extract_crossroads(&response);
        assert!(crossroads.is_empty());
    }

    #[test]
    fn test_extract_crossroads_two_way_junction_not_crossroads() {
        // Two ways sharing one node — only 2 unique ways, not a crossroads.
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "way".to_string(),
                    id: 100,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![1, 2]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.5,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                    ]),
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 200,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 3]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.502,
                            lon: -8.0,
                        },
                    ]),
                    members: None,
                },
            ],
        };
        let crossroads = extract_crossroads(&response);
        assert!(
            crossroads.is_empty(),
            "2-way junction should not be a crossroads"
        );
    }

    #[test]
    fn test_extract_crossroads_three_way_junction() {
        // Three ways meeting at node 2 — a proper crossroads.
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "way".to_string(),
                    id: 100,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![1, 2]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.5,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                    ]),
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 200,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 3]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.502,
                            lon: -8.0,
                        },
                    ]),
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 300,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 4]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.001,
                        },
                    ]),
                    members: None,
                },
            ],
        };
        let crossroads = extract_crossroads(&response);
        assert_eq!(
            crossroads.len(),
            1,
            "3-way junction should produce one crossroads"
        );
        assert_eq!(crossroads[0].osm_id, 2);
        assert_eq!(crossroads[0].name, "A Crossroads");
        assert_eq!(crossroads[0].location_type, LocationType::Crossroads);
    }

    #[test]
    fn test_extract_crossroads_deduplicates_repeated_node_in_same_way() {
        // A way looping back on itself: node 2 appears twice in the same way.
        // If we counted appearances, node 2 would have count 2 from this single way.
        // With 2 such ways sharing node 2, appearances = 4 but unique ways = 2.
        // We should NOT produce a crossroads (unique ways = 2).
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "way".to_string(),
                    id: 100,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![1, 2, 3, 2]), // node 2 repeated
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.5,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.502,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                    ]),
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 200,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 4]),
                    geometry: Some(vec![
                        LatLon {
                            lat: 53.501,
                            lon: -8.0,
                        },
                        LatLon {
                            lat: 53.501,
                            lon: -8.001,
                        },
                    ]),
                    members: None,
                },
            ],
        };
        let crossroads = extract_crossroads(&response);
        assert!(
            crossroads.is_empty(),
            "repeated node in same way should not inflate way count"
        );
    }

    #[test]
    fn test_extract_crossroads_without_geometry() {
        // Ways with nodes but no geometry — crossroads should still be found
        // but skipped due to missing coordinates.
        let response = OverpassResponse {
            version: Some(0.6),
            elements: vec![
                OsmElement {
                    element_type: "way".to_string(),
                    id: 100,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![1, 2]),
                    geometry: None,
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 200,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 3]),
                    geometry: None,
                    members: None,
                },
                OsmElement {
                    element_type: "way".to_string(),
                    id: 300,
                    lat: None,
                    lon: None,
                    center: None,
                    tags: HashMap::new(),
                    nodes: Some(vec![2, 4]),
                    geometry: None,
                    members: None,
                },
            ],
        };
        let crossroads = extract_crossroads(&response);
        // No geometry means no coordinates, so crossroads are skipped
        assert!(
            crossroads.is_empty(),
            "crossroads without geometry coords should be skipped"
        );
    }
}
