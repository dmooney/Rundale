//! OpenStreetMap data model for geographic feature extraction.
//!
//! Represents OSM elements (nodes, ways, relations) as returned by the
//! Overpass API, along with game-relevant classifications derived from
//! OSM tags.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A raw element returned by the Overpass API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsmElement {
    /// Element type: "node", "way", or "relation".
    #[serde(rename = "type")]
    pub element_type: String,
    /// Unique OSM identifier.
    pub id: i64,
    /// Latitude (nodes only, or center for ways with `out center`).
    #[serde(default)]
    pub lat: Option<f64>,
    /// Longitude (nodes only, or center for ways with `out center`).
    #[serde(default)]
    pub lon: Option<f64>,
    /// Center point (for ways/relations queried with `out center`).
    #[serde(default)]
    pub center: Option<LatLon>,
    /// OSM tags (key-value metadata).
    #[serde(default)]
    pub tags: HashMap<String, String>,
    /// Node references (ways only).
    #[serde(default)]
    pub nodes: Option<Vec<i64>>,
    /// Geometry coordinates (ways queried with `out geom`).
    #[serde(default)]
    pub geometry: Option<Vec<LatLon>>,
    /// Relation members.
    #[serde(default)]
    pub members: Option<Vec<RelationMember>>,
}

/// A latitude/longitude coordinate pair.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct LatLon {
    /// Latitude in decimal degrees.
    pub lat: f64,
    /// Longitude in decimal degrees.
    pub lon: f64,
}

/// A member of an OSM relation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationMember {
    /// Member element type.
    #[serde(rename = "type")]
    pub member_type: String,
    /// Referenced element id.
    #[serde(rename = "ref")]
    pub member_ref: i64,
    /// Role within the relation.
    #[serde(default)]
    pub role: String,
}

/// The top-level Overpass API JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverpassResponse {
    /// API version.
    #[serde(default)]
    pub version: Option<f64>,
    /// Response elements.
    pub elements: Vec<OsmElement>,
}

/// Classification of an OSM feature into a game-relevant location type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocationType {
    /// Pub, tavern, or inn.
    Pub,
    /// Church, chapel, or abbey.
    Church,
    /// Shop or market.
    Shop,
    /// School or hedge school.
    School,
    /// Post office or letter office.
    PostOffice,
    /// Farmhouse or agricultural holding.
    Farm,
    /// Crossroads or road junction.
    Crossroads,
    /// Bridge.
    Bridge,
    /// Well, spring, or holy well.
    Well,
    /// Lake shore or river bank.
    Waterside,
    /// Bog or marsh.
    Bog,
    /// Woodland or copse.
    Woodland,
    /// Ring fort, rath, or cashel.
    RingFort,
    /// Standing stone or megalithic monument.
    StandingStone,
    /// Graveyard or cemetery.
    Graveyard,
    /// Mill (corn, flour, linen).
    Mill,
    /// Forge or smithy.
    Forge,
    /// Lime kiln.
    LimeKiln,
    /// Town square or village green.
    Square,
    /// Harbour, quay, or pier.
    Harbour,
    /// Road segment (used for connections, not as a primary location).
    Road,
    /// Generic named place or townland.
    NamedPlace,
    /// Hilltop, ridge, or elevated point.
    Hill,
    /// Ruin or abandoned structure.
    Ruin,
    /// Other — unclassified feature that still warrants a location node.
    Other,
}

impl LocationType {
    /// Whether this location type is typically indoors.
    pub fn is_indoor(self) -> bool {
        matches!(
            self,
            Self::Pub
                | Self::Church
                | Self::Shop
                | Self::School
                | Self::PostOffice
                | Self::Mill
                | Self::Forge
        )
    }

    /// Whether this location type is typically publicly accessible.
    pub fn is_public(self) -> bool {
        !matches!(self, Self::Farm)
    }
}

/// A geographic feature extracted from OSM and classified for the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeoFeature {
    /// OSM element id (for deduplication and provenance tracking).
    pub osm_id: i64,
    /// OSM element type.
    pub osm_type: String,
    /// Latitude of the feature.
    pub lat: f64,
    /// Longitude of the feature.
    pub lon: f64,
    /// Name of the feature (from OSM `name` tag, or generated).
    pub name: String,
    /// Irish-language name if available.
    #[serde(default)]
    pub name_ga: Option<String>,
    /// Game location type classification.
    pub location_type: LocationType,
    /// Raw OSM tags for additional context.
    pub tags: HashMap<String, String>,
    /// Whether this was a hand-authored (curated) location.
    #[serde(default)]
    pub curated: bool,
}

impl OsmElement {
    /// Returns the effective latitude of this element.
    ///
    /// For nodes, uses the `lat` field directly. For ways and relations,
    /// uses the `center` point if available.
    pub fn effective_lat(&self) -> Option<f64> {
        self.lat.or(self.center.map(|c| c.lat))
    }

    /// Returns the effective longitude of this element.
    ///
    /// For nodes, uses the `lon` field directly. For ways and relations,
    /// uses the `center` point if available.
    pub fn effective_lon(&self) -> Option<f64> {
        self.lon.or(self.center.map(|c| c.lon))
    }

    /// Returns the value of a tag, if present.
    pub fn tag(&self, key: &str) -> Option<&str> {
        self.tags.get(key).map(|s| s.as_str())
    }

    /// Returns the name tag, preferring `name:en`, falling back to `name`.
    pub fn name(&self) -> Option<&str> {
        self.tag("name:en").or_else(|| self.tag("name"))
    }

    /// Returns the Irish-language name if available.
    pub fn name_ga(&self) -> Option<&str> {
        self.tag("name:ga")
    }
}

/// Calculate the Haversine distance between two lat/lon points in meters.
pub fn haversine_distance(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (dlat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();

    EARTH_RADIUS_M * c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_haversine_known_distance() {
        // Kiltoom crossroads to Hodson Bay — approximately 2.5 km
        let dist = haversine_distance(53.507, -7.985, 53.489, -7.979);
        assert!(dist > 1500.0 && dist < 3500.0, "distance was {dist}m");
    }

    #[test]
    fn test_haversine_same_point() {
        let dist = haversine_distance(53.5, -8.0, 53.5, -8.0);
        assert!(dist.abs() < 0.001);
    }

    #[test]
    fn test_location_type_indoor() {
        assert!(LocationType::Pub.is_indoor());
        assert!(LocationType::Church.is_indoor());
        assert!(LocationType::Shop.is_indoor());
        assert!(!LocationType::Crossroads.is_indoor());
        assert!(!LocationType::Farm.is_indoor());
        assert!(!LocationType::Bog.is_indoor());
    }

    #[test]
    fn test_location_type_public() {
        assert!(LocationType::Pub.is_public());
        assert!(LocationType::Church.is_public());
        assert!(!LocationType::Farm.is_public());
    }

    #[test]
    fn test_osm_element_effective_coords() {
        let node = OsmElement {
            element_type: "node".to_string(),
            id: 1,
            lat: Some(53.5),
            lon: Some(-8.0),
            center: None,
            tags: HashMap::new(),
            nodes: None,
            geometry: None,
            members: None,
        };
        assert_eq!(node.effective_lat(), Some(53.5));
        assert_eq!(node.effective_lon(), Some(-8.0));

        let way = OsmElement {
            element_type: "way".to_string(),
            id: 2,
            lat: None,
            lon: None,
            center: Some(LatLon {
                lat: 53.6,
                lon: -7.9,
            }),
            tags: HashMap::new(),
            nodes: None,
            geometry: None,
            members: None,
        };
        assert_eq!(way.effective_lat(), Some(53.6));
        assert_eq!(way.effective_lon(), Some(-7.9));
    }

    #[test]
    fn test_osm_element_name() {
        let mut tags = HashMap::new();
        tags.insert("name".to_string(), "Darcy's Pub".to_string());
        let elem = OsmElement {
            element_type: "node".to_string(),
            id: 1,
            lat: Some(53.5),
            lon: Some(-8.0),
            center: None,
            tags,
            nodes: None,
            geometry: None,
            members: None,
        };
        assert_eq!(elem.name(), Some("Darcy's Pub"));
    }
}
