//! System prompt and response schema for the vision-reading pass.
//!
//! The discover pipeline stitches 2×2 tile groups into one image and asks
//! a vision-capable LLM to return every named or unnamed man-made feature
//! it can see on the historic map. The schema here mirrors what the
//! discovery orchestrator (`super::discover`) expects.

use serde::{Deserialize, Serialize};

/// System prompt for the vision pass. Focused on what matters for a 1820s
/// Irish living-world simulation: named settlements, churches, mills,
/// forges, pubs/inns, holy wells, ring forts, bridges, and crossroads.
pub const VISION_SYSTEM_PROMPT: &str = "You are a historical cartography assistant reading a scanned \
1830s Ordnance Survey 6-inch map of rural Ireland. You see hand-engraved \
topography, buildings, and labels in period script. Transcribe labels \
exactly as printed, preserving spelling and punctuation (no modernisation). \
Never invent features that are not visible on the map.";

/// Short human-readable hint about the coordinate frame of `px`/`py`.
pub const PIXEL_FRAME_HINT: &str = "Pixel coordinates (px, py) are measured \
from the top-left of the supplied image; x increases rightwards, y downwards. \
The image is a 512x512 pixel raster made by stitching four adjacent 256x256 \
map tiles.";

/// Builds the per-call user instruction. The caller supplies fresh
/// instructions per chunk because the pixel frame is chunk-local.
pub fn user_instruction() -> String {
    format!(
        "{hint}\n\n\
Return a JSON object with a single field `features` — an array of feature \
objects. Each feature object has these fields:\n\n\
- `px` (integer, 0..512): horizontal pixel offset within the stitched image\n\
- `py` (integer, 0..512): vertical pixel offset within the stitched image\n\
- `label_text` (string or null): the exact label as printed on the map, or \
null if the feature has no printed label\n\
- `feature_kind` (string, one of: `village`, `church`, `mill`, `forge`, \
`school`, `pub_or_inn`, `holy_well`, `ring_fort`, `farmstead`, `crossroads`, \
`bridge`, `graveyard`, `other`)\n\
- `confidence` (number, 0.0..1.0): how confident you are that this feature \
is correctly identified\n\
- `connected_px_segments` (array of objects with `to_px` and `to_py`): pixel \
endpoints of road or path segments visible on the map that link this feature \
to adjacent features; may be empty\n\n\
Include every distinct labelled or unlabelled building cluster, small \
hamlet, church, mill, forge, school, and named crossroads. Do not list \
individual tombstones, generic field boundaries, or labelled fields. \
If no features are visible return an empty array.",
        hint = PIXEL_FRAME_HINT,
    )
}

/// A single feature extracted by the vision model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionFeature {
    /// Pixel x within the stitched chunk image.
    pub px: u32,
    /// Pixel y within the stitched chunk image.
    pub py: u32,
    /// Label transcription, or `None` for unlabelled features.
    #[serde(default)]
    pub label_text: Option<String>,
    /// Coarse feature classification.
    pub feature_kind: VisionFeatureKind,
    /// Confidence in the identification (0..1).
    #[serde(default = "default_confidence")]
    pub confidence: f32,
    /// Road/path endpoints linking this feature to visible neighbours.
    #[serde(default)]
    pub connected_px_segments: Vec<PxSegment>,
}

fn default_confidence() -> f32 {
    0.5
}

/// Pixel-space endpoint of a road/path segment visible on the map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PxSegment {
    pub to_px: u32,
    pub to_py: u32,
}

/// Coarse feature classification. Maps loosely to the existing
/// `LocationType` enum in `osm_model.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionFeatureKind {
    Village,
    Church,
    Mill,
    Forge,
    School,
    PubOrInn,
    HolyWell,
    RingFort,
    Farmstead,
    Crossroads,
    Bridge,
    Graveyard,
    Other,
}

/// Top-level schema for the vision JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisionResponse {
    #[serde(default)]
    pub features: Vec<VisionFeature>,
}

impl VisionFeatureKind {
    /// Maps a vision-kind to the game's OSM `LocationType`.
    pub fn to_location_type(self) -> super::super::osm_model::LocationType {
        use super::super::osm_model::LocationType as L;
        match self {
            Self::Village => L::NamedPlace,
            Self::Church => L::Church,
            Self::Mill => L::Mill,
            Self::Forge => L::Forge,
            Self::School => L::School,
            Self::PubOrInn => L::Pub,
            Self::HolyWell => L::Well,
            Self::RingFort => L::RingFort,
            Self::Farmstead => L::Farm,
            Self::Crossroads => L::Crossroads,
            Self::Bridge => L::Bridge,
            Self::Graveyard => L::Graveyard,
            Self::Other => L::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vision_response_parses_minimal_payload() {
        let json = r#"{
            "features": [
                {
                    "px": 128,
                    "py": 96,
                    "label_text": "Kilteevan",
                    "feature_kind": "village",
                    "confidence": 0.9,
                    "connected_px_segments": [
                        { "to_px": 150, "to_py": 110 }
                    ]
                },
                {
                    "px": 300,
                    "py": 200,
                    "label_text": null,
                    "feature_kind": "farmstead",
                    "confidence": 0.6
                }
            ]
        }"#;
        let parsed: VisionResponse = serde_json::from_str(json).expect("parse");
        assert_eq!(parsed.features.len(), 2);
        assert_eq!(parsed.features[0].label_text.as_deref(), Some("Kilteevan"));
        assert_eq!(parsed.features[0].feature_kind, VisionFeatureKind::Village);
        assert_eq!(parsed.features[0].connected_px_segments.len(), 1);
        assert!(parsed.features[1].label_text.is_none());
        assert_eq!(parsed.features[1].connected_px_segments.len(), 0);
    }

    #[test]
    fn test_vision_response_defaults_when_missing_fields() {
        // features array missing defaults to empty; missing confidence uses default.
        let parsed: VisionResponse = serde_json::from_str("{}").expect("parse");
        assert!(parsed.features.is_empty());
    }

    #[test]
    fn test_kind_maps_to_location_type() {
        use super::super::super::osm_model::LocationType;
        assert_eq!(
            VisionFeatureKind::Church.to_location_type(),
            LocationType::Church
        );
        assert_eq!(
            VisionFeatureKind::HolyWell.to_location_type(),
            LocationType::Well
        );
        assert_eq!(
            VisionFeatureKind::PubOrInn.to_location_type(),
            LocationType::Pub
        );
        assert_eq!(
            VisionFeatureKind::Farmstead.to_location_type(),
            LocationType::Farm
        );
    }

    #[test]
    fn test_user_instruction_mentions_schema_fields() {
        let text = user_instruction();
        for field in ["px", "py", "label_text", "feature_kind", "confidence"] {
            assert!(text.contains(field), "instruction missing `{field}`");
        }
    }
}
