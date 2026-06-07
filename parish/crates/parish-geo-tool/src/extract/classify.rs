//! OSM tag classification and name generation.
//!
//! Maps an [`OsmElement`]'s tags onto a game-relevant [`LocationType`]
//! ([`classify_element`]) and derives a display name when the element lacks
//! an explicit `name` tag ([`generate_name`]). Split out of the monolithic
//! `extract` module (#1200).

use std::collections::HashMap;

use super::super::osm_model::{LocationType, OsmElement};

fn classify_historic(tags: &HashMap<String, String>) -> Option<LocationType> {
    let historic = tags.get("historic")?;
    match historic.as_str() {
        "ring_fort" | "rath" | "cashel" | "crannog" => Some(LocationType::RingFort),
        "standing_stone" | "ogham_stone" | "stone_circle" | "megalith" => {
            Some(LocationType::StandingStone)
        }
        "holy_well" => Some(LocationType::Well),
        "castle" | "ruins" | "monument" => Some(LocationType::Ruin),
        _ => Some(LocationType::Other),
    }
}

fn classify_amenity(tags: &HashMap<String, String>) -> Option<LocationType> {
    let amenity = tags.get("amenity")?;
    match amenity.as_str() {
        "pub" | "bar" | "restaurant" => Some(LocationType::Pub),
        "place_of_worship" => Some(LocationType::Church),
        "school" => Some(LocationType::School),
        "post_office" => Some(LocationType::PostOffice),
        "grave_yard" => Some(LocationType::Graveyard),
        _ => None,
    }
}

fn classify_building(
    tags: &HashMap<String, String>,
    element: &OsmElement,
) -> Option<LocationType> {
    let building = tags.get("building")?;
    match building.as_str() {
        "church" | "chapel" | "cathedral" => Some(LocationType::Church),
        "farm" | "farmhouse" | "barn" => Some(LocationType::Farm),
        _ => {
            if element.name().is_some() {
                Some(LocationType::Other)
            } else {
                None
            }
        }
    }
}

fn classify_natural(tags: &HashMap<String, String>) -> Option<LocationType> {
    let natural = tags.get("natural")?;
    match natural.as_str() {
        "water" => Some(LocationType::Waterside),
        "wetland" => Some(LocationType::Bog),
        "wood" => Some(LocationType::Woodland),
        "peak" | "hill" => Some(LocationType::Hill),
        "spring" => Some(LocationType::Well),
        _ => None,
    }
}

fn classify_waterway(tags: &HashMap<String, String>) -> Option<LocationType> {
    let waterway = tags.get("waterway")?;
    match waterway.as_str() {
        "river" | "stream" | "canal" => Some(LocationType::Waterside),
        _ => None,
    }
}

fn classify_landuse(tags: &HashMap<String, String>) -> Option<LocationType> {
    let landuse = tags.get("landuse")?;
    match landuse.as_str() {
        "farmyard" | "farmland" => Some(LocationType::Farm),
        "cemetery" => Some(LocationType::Graveyard),
        _ => None,
    }
}

fn classify_man_made(tags: &HashMap<String, String>) -> Option<LocationType> {
    let man_made = tags.get("man_made")?;
    match man_made.as_str() {
        "bridge" => Some(LocationType::Bridge),
        "kiln" => Some(LocationType::LimeKiln),
        "watermill" | "windmill" => Some(LocationType::Mill),
        "pier" | "quay" => Some(LocationType::Harbour),
        _ => None,
    }
}

fn classify_place(tags: &HashMap<String, String>) -> Option<LocationType> {
    let place = tags.get("place")?;
    match place.as_str() {
        "hamlet" | "village" | "isolated_dwelling" | "locality" | "townland" | "town" => {
            Some(LocationType::NamedPlace)
        }
        _ => None,
    }
}

/// Classifies an OSM element into a game-relevant location type.
///
/// Returns `None` if the element doesn't map to any game-relevant type.
pub fn classify_element(element: &OsmElement) -> Option<LocationType> {
    let tags = &element.tags;

    classify_historic(tags)
        .or_else(|| classify_amenity(tags))
        .or_else(|| classify_building(tags, element))
        .or_else(|| {
            if tags.contains_key("shop") {
                Some(LocationType::Shop)
            } else {
                None
            }
        })
        .or_else(|| classify_natural(tags))
        .or_else(|| classify_waterway(tags))
        .or_else(|| classify_landuse(tags))
        .or_else(|| classify_man_made(tags))
        .or_else(|| {
            if tags.get("craft").is_some_and(|v| v == "blacksmith") {
                return Some(LocationType::Forge);
            }
            if tags.get("ford").is_some_and(|v| v == "yes") {
                return Some(LocationType::Bridge);
            }
            None
        })
        .or_else(|| classify_place(tags))
        .or_else(|| {
            if tags
                .get("leisure")
                .is_some_and(|v| v == "harbour" || v == "marina")
            {
                return Some(LocationType::Harbour);
            }
            if tags.get("tourism").is_some_and(|v| v == "hotel") {
                return Some(LocationType::Pub);
            }
            None
        })
}

/// Generates a name for a feature from OSM tags or its location type.
///
/// Priority: explicit name tag > generated from type + context > None.
pub fn generate_name(element: &OsmElement, location_type: LocationType) -> Option<String> {
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
