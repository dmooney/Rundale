use serde_json::Value;

const VISUAL_SCENES: &str =
    include_str!("../../../apps/ui/static/rundale/illustrated-notebook-v2/visual-scenes.json");

const BANNED_SOURCE_TERMS: &[&str] = &[
    "historical map",
    "map crop",
    "nls",
    "ordnance survey",
    "source map",
    "map reference",
];

const BANNED_PROJECTION_TERMS: &[&str] = &[
    "strict isometric",
    "strict isomorphic",
    "isometric",
    "isomorphic",
];

#[test]
fn runtime_visual_scene_metadata_uses_written_description_source_only() {
    let text = VISUAL_SCENES.to_lowercase();
    for term in BANNED_SOURCE_TERMS {
        assert!(
            !text.contains(term),
            "runtime visual-scene metadata must not contain source-map language: {term}"
        );
    }
}

#[test]
fn runtime_visual_scene_metadata_does_not_require_strict_projection() {
    let text = VISUAL_SCENES.to_lowercase();
    for term in BANNED_PROJECTION_TERMS {
        assert!(
            !text.contains(term),
            "runtime visual-scene metadata must not require strict projection language: {term}"
        );
    }
}

#[test]
fn runtime_visual_scene_metadata_declares_oblique_storybook_camera_and_depth_bands() {
    let value: Value = serde_json::from_str(VISUAL_SCENES).expect("visual-scenes.json parses");
    let scenes = value["scenes"]
        .as_array()
        .expect("visual-scenes.json has scenes array");
    assert!(!scenes.is_empty(), "at least one visual scene is declared");

    for scene in scenes {
        assert!(
            scene["plate_asset"]
                .as_str()
                .is_some_and(|asset| asset.starts_with("/rundale/illustrated-notebook-v2/")),
            "runtime plate must come from the fresh illustrated-notebook-v2 boundary"
        );
        assert!(
            scene["mobile_plate_asset"]
                .as_str()
                .is_some_and(|asset| asset.starts_with("/rundale/illustrated-notebook-v2/")),
            "mobile runtime plate must come from the fresh illustrated-notebook-v2 boundary"
        );
        assert_eq!(
            scene["camera_hint"].as_str(),
            Some("wide elevated oblique illustrated storybook game scene")
        );
        assert!(
            scene["written_visual_summary"]
                .as_str()
                .is_some_and(|summary| !summary.trim().is_empty()),
            "visual scene needs a written summary"
        );
        assert!(
            scene["depth_bands"]
                .as_array()
                .is_some_and(|bands| bands.len() >= 3),
            "visual scene needs depth bands for marker scaling"
        );
        assert!(
            scene["anchors"]["player"].is_object(),
            "visual scene needs a player anchor"
        );
        assert!(
            scene["anchors"]["npcs"]
                .as_array()
                .is_some_and(|anchors| anchors.len() >= 3),
            "visual scene needs several NPC anchors"
        );
        assert!(
            scene["anchors"]["exits"]
                .as_array()
                .is_some_and(|anchors| anchors.len() >= 2),
            "visual scene needs exit label anchors"
        );
    }
}
