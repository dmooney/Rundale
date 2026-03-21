//! Feature extraction — classifies OSM elements into game-relevant location types.
//!
//! Takes raw Overpass API responses and produces a deduplicated list of
//! [`GeoFeature`]s, each classified by [`LocationType`] based on OSM tags.

use std::collections::{HashMap, HashSet};

use tracing::{debug, warn};

use super::osm_model::{GeoFeature, LocationType, OsmElement, OverpassResponse};

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

/// Classifies an OSM element into a game-relevant location type.
///
/// Returns `None` if the element doesn't map to any game-relevant type.
pub fn classify_element(element: &OsmElement) -> Option<LocationType> {
    let tags = &element.tags;

    // Historic features (highest priority — most game-relevant)
    if let Some(historic) = tags.get("historic") {
        return match historic.as_str() {
            "ring_fort" | "rath" | "cashel" | "crannog" => Some(LocationType::RingFort),
            "standing_stone" | "ogham_stone" | "stone_circle" | "megalith" => {
                Some(LocationType::StandingStone)
            }
            "holy_well" => Some(LocationType::Well),
            "castle" | "ruins" | "monument" => Some(LocationType::Ruin),
            _ => Some(LocationType::Other),
        };
    }

    // Amenities
    if let Some(amenity) = tags.get("amenity") {
        return match amenity.as_str() {
            "pub" | "bar" | "restaurant" => Some(LocationType::Pub),
            "place_of_worship" => Some(LocationType::Church),
            "school" => Some(LocationType::School),
            "post_office" => Some(LocationType::PostOffice),
            "grave_yard" => Some(LocationType::Graveyard),
            _ => None,
        };
    }

    // Buildings
    if let Some(building) = tags.get("building") {
        return match building.as_str() {
            "church" | "chapel" | "cathedral" => Some(LocationType::Church),
            "farm" | "farmhouse" | "barn" => Some(LocationType::Farm),
            _ => {
                // Named buildings are worth keeping
                if element.name().is_some() {
                    Some(LocationType::Other)
                } else {
                    None
                }
            }
        };
    }

    // Shops
    if tags.contains_key("shop") {
        return Some(LocationType::Shop);
    }

    // Natural features
    if let Some(natural) = tags.get("natural") {
        return match natural.as_str() {
            "water" => Some(LocationType::Waterside),
            "wetland" => Some(LocationType::Bog),
            "wood" => Some(LocationType::Woodland),
            "peak" | "hill" => Some(LocationType::Hill),
            "spring" => Some(LocationType::Well),
            _ => None,
        };
    }

    // Waterways
    if let Some(waterway) = tags.get("waterway") {
        return match waterway.as_str() {
            "river" | "stream" | "canal" => Some(LocationType::Waterside),
            _ => None,
        };
    }

    // Land use
    if let Some(landuse) = tags.get("landuse") {
        return match landuse.as_str() {
            "farmyard" | "farmland" => Some(LocationType::Farm),
            "cemetery" => Some(LocationType::Graveyard),
            _ => None,
        };
    }

    // Man-made features
    if let Some(man_made) = tags.get("man_made") {
        return match man_made.as_str() {
            "bridge" => Some(LocationType::Bridge),
            "kiln" => Some(LocationType::LimeKiln),
            "watermill" | "windmill" => Some(LocationType::Mill),
            "pier" | "quay" => Some(LocationType::Harbour),
            _ => None,
        };
    }

    // Craft
    if let Some(craft) = tags.get("craft")
        && craft == "blacksmith"
    {
        return Some(LocationType::Forge);
    }

    // Ford
    if tags.get("ford").is_some_and(|v| v == "yes") {
        return Some(LocationType::Bridge); // Fords serve similar role as bridges
    }

    // Places
    if let Some(place) = tags.get("place") {
        return match place.as_str() {
            "hamlet" | "village" | "isolated_dwelling" | "locality" | "townland" | "town" => {
                Some(LocationType::NamedPlace)
            }
            _ => None,
        };
    }

    // Leisure / tourism
    if tags
        .get("leisure")
        .is_some_and(|v| v == "harbour" || v == "marina")
    {
        return Some(LocationType::Harbour);
    }

    if tags.get("tourism").is_some_and(|v| v == "hotel") {
        return Some(LocationType::Pub); // Inns in 1820
    }

    None
}

/// Generates a name for a feature from OSM tags or its location type.
///
/// Priority: explicit name tag > generated from type + context > None.
fn generate_name(element: &OsmElement, location_type: LocationType) -> Option<String> {
    // Use explicit name if available
    if let Some(name) = element.name() {
        return Some(name.to_string());
    }

    // Generate a name based on type and nearby context
    match location_type {
        LocationType::Crossroads => Some("A Crossroads".to_string()),
        LocationType::Bridge => {
            // Try to use the road name
            if let Some(road) = element.tag("highway") {
                Some(format!("Bridge on the {road}"))
            } else {
                Some("A Bridge".to_string())
            }
        }
        LocationType::Well => Some("A Holy Well".to_string()),
        LocationType::RingFort => Some("A Ring Fort".to_string()),
        LocationType::StandingStone => Some("A Standing Stone".to_string()),
        LocationType::LimeKiln => Some("A Lime Kiln".to_string()),
        LocationType::Forge => Some("The Forge".to_string()),
        LocationType::Farm => {
            // Unnamed farms are too generic
            None
        }
        LocationType::Bog => Some("The Bog".to_string()),
        _ => None,
    }
}

/// Deduplicates features that are within `threshold_meters` of each other.
///
/// When duplicates are found, keeps the one with the more specific
/// location type (non-Other > Other, named > unnamed).
fn deduplicate_by_proximity(features: &mut Vec<GeoFeature>, threshold_meters: f64) {
    use super::osm_model::haversine_distance;

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

/// Extracts junction nodes (crossroads) from road network data.
///
/// A junction is a node that appears in 3 or more ways (roads meeting at a point).
/// These make natural location nodes in the world graph.
pub fn extract_crossroads(road_response: &OverpassResponse) -> Vec<GeoFeature> {
    let mut node_count: HashMap<i64, usize> = HashMap::new();
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();

    for element in &road_response.elements {
        if element.element_type != "way" {
            continue;
        }

        // Count how many roads each node belongs to
        if let Some(ref geometry) = element.geometry {
            // Use geometry nodes
            for (i, point) in geometry.iter().enumerate() {
                // We need node IDs from the `nodes` array
                if let Some(ref nodes) = element.nodes
                    && i < nodes.len()
                {
                    *node_count.entry(nodes[i]).or_insert(0) += 1;
                    node_coords.insert(nodes[i], (point.lat, point.lon));
                }
            }
        } else if let Some(ref nodes) = element.nodes {
            for &node_id in nodes {
                *node_count.entry(node_id).or_insert(0) += 1;
            }
        }
    }

    // Nodes appearing in 3+ ways are junctions
    let mut crossroads = Vec::new();
    for (node_id, count) in &node_count {
        if *count >= 3 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osm_model::OsmElement;

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
}
