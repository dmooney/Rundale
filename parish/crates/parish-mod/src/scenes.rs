//! Scene-diorama schema and loader.
//!
//! Scenes are optional mod data: a mod may point `[files].scenes` at a JSON
//! index that declares background plates, clickable hotspots, and NPC sprite
//! anchors. Loading validates asset paths strictly; cross-reference checks
//! against the world graph and NPC roster are warnings because scene data is a
//! presentation layer, not a reason to reject otherwise-playable content.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parish_npc::manager::NpcManager;
use parish_types::{LocationId, NpcId, ParishError};
use parish_world::graph::WorldGraph;
use serde::{Deserialize, Serialize};

use crate::assets;

/// Top-level scene index loaded from `scenes.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SceneIndex {
    /// Per-location scene definitions.
    #[serde(default)]
    pub scenes: Vec<SceneDef>,
    /// NPC-specific sprite definitions.
    #[serde(default)]
    pub sprites: Vec<SpriteDef>,
    /// Fallback sprite assets keyed by role, e.g. `"default"`.
    #[serde(default)]
    pub fallback_sprites: BTreeMap<String, String>,
}

/// Diorama definition for one world location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneDef {
    pub location_id: LocationId,
    pub slug: String,
    pub plate: String,
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default)]
    pub hotspots: Vec<Hotspot>,
    #[serde(default)]
    pub slots: Vec<NpcSlot>,
}

/// Clickable region within a scene plate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hotspot {
    pub id: String,
    pub shape: HotspotShape,
    pub label: String,
    pub action: HotspotAction,
}

/// Hotspot geometry in percent coordinates over the plate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotShape {
    Rect([f32; 4]),
    Polygon(Vec<[f32; 2]>),
}

/// What happens when the player activates a hotspot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotAction {
    TravelTo(LocationId),
    TalkTo(NpcId),
    Inspect(String),
}

/// Preferred position for an NPC sprite in a scene.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NpcSlot {
    pub id: String,
    pub x: f32,
    pub y: f32,
    #[serde(default = "default_slot_scale")]
    pub scale: f32,
    #[serde(default)]
    pub prefer_npc: Option<NpcId>,
}

/// Sprite asset for a named NPC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpriteDef {
    pub npc_id: NpcId,
    pub image: String,
}

fn default_slot_scale() -> f32 {
    1.0
}

impl SceneIndex {
    /// Load a scene index relative to `mod_dir`, validating every referenced
    /// asset through the same guarded resolver used by other mod assets.
    pub fn load(mod_dir: &Path, rel: &str) -> Result<Self, ParishError> {
        let index_path = canonical_mod_file_path(mod_dir, rel)?;
        let text = std::fs::read_to_string(&index_path).map_err(|e| {
            ParishError::Config(format!("failed to read {}: {e}", index_path.display()))
        })?;
        let index: Self = serde_json::from_str(&text)
            .map_err(|e| ParishError::Config(format!("failed to parse {rel}: {e}")))?;
        index.validate_assets(mod_dir)?;
        Ok(index)
    }

    /// Returns the scene for a world location id, if one is declared.
    pub fn scene_for(&self, location_id: LocationId) -> Option<&SceneDef> {
        self.scenes
            .iter()
            .find(|scene| scene.location_id == location_id)
    }

    /// Returns the sprite definition for an NPC id, if one is declared.
    pub fn sprite_for(&self, npc_id: NpcId) -> Option<&SpriteDef> {
        self.sprites.iter().find(|sprite| sprite.npc_id == npc_id)
    }

    /// Number of sprite asset references, including fallback sprites.
    pub fn sprite_asset_count(&self) -> usize {
        self.sprites.len() + self.fallback_sprites.len()
    }

    /// Human-readable load summary used by debug/proof tooling.
    pub fn load_summary(&self, rel: &str) -> String {
        format!(
            "{rel} loaded: {} scenes, {} sprites",
            self.scenes.len(),
            self.sprite_asset_count()
        )
    }

    fn validate_assets(&self, mod_dir: &Path) -> Result<(), ParishError> {
        for scene in &self.scenes {
            validate_scene_asset(
                mod_dir,
                &format!("scene '{}'.plate", scene.slug),
                &scene.plate,
            )?;
            for (variant, path) in &scene.variants {
                validate_scene_asset(
                    mod_dir,
                    &format!("scene '{}'.variants.{variant}", scene.slug),
                    path,
                )?;
            }
        }

        for sprite in &self.sprites {
            validate_scene_asset(
                mod_dir,
                &format!("sprite npc {}", sprite.npc_id.0),
                &sprite.image,
            )?;
        }

        for (name, path) in &self.fallback_sprites {
            validate_scene_asset(mod_dir, &format!("fallback sprite '{name}'"), path)?;
        }

        Ok(())
    }
}

/// Cross-validates scene ids and coordinates against loaded game data.
pub fn validate_scenes(scenes: &SceneIndex, world: &WorldGraph, npcs: &NpcManager) -> Vec<String> {
    let mut warnings = Vec::new();

    for scene in &scenes.scenes {
        if world.get(scene.location_id).is_none() {
            warnings.push(format!(
                "scene '{}' references unknown location id {}",
                scene.slug, scene.location_id.0
            ));
        }

        for hotspot in &scene.hotspots {
            validate_shape_coords(scene, hotspot, &mut warnings);
            match &hotspot.action {
                HotspotAction::TravelTo(target) if world.get(*target).is_none() => {
                    warnings.push(format!(
                        "scene '{}' hotspot '{}' travels to unknown location id {}",
                        scene.slug, hotspot.id, target.0
                    ));
                }
                HotspotAction::TalkTo(npc_id) if !npcs.npcs().contains_key(npc_id) => {
                    warnings.push(format!(
                        "scene '{}' hotspot '{}' talks to unknown NPC id {}",
                        scene.slug, hotspot.id, npc_id.0
                    ));
                }
                _ => {}
            }
        }

        for slot in &scene.slots {
            if !percent(slot.x) || !percent(slot.y) {
                warnings.push(format!(
                    "scene '{}' slot '{}' has out-of-range coordinates ({}, {})",
                    scene.slug, slot.id, slot.x, slot.y
                ));
            }
            if slot.scale <= 0.0 {
                warnings.push(format!(
                    "scene '{}' slot '{}' has non-positive scale {}",
                    scene.slug, slot.id, slot.scale
                ));
            }
            if let Some(npc_id) = slot.prefer_npc
                && !npcs.npcs().contains_key(&npc_id)
            {
                warnings.push(format!(
                    "scene '{}' slot '{}' prefers unknown NPC id {}",
                    scene.slug, slot.id, npc_id.0
                ));
            }
        }
    }

    for sprite in &scenes.sprites {
        if !npcs.npcs().contains_key(&sprite.npc_id) {
            warnings.push(format!(
                "sprite '{}' references unknown NPC id {}",
                sprite.image, sprite.npc_id.0
            ));
        }
    }

    warnings
}

fn canonical_mod_file_path(mod_dir: &Path, rel: &str) -> Result<PathBuf, ParishError> {
    let mod_dir = mod_dir
        .canonicalize()
        .map_err(|e| ParishError::Config(format!("mod directory not found: {e}")))?;
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(ParishError::Config(format!(
            "manifest path {rel} must be relative"
        )));
    }
    let candidate = mod_dir.join(rel_path);
    let canonical = candidate.canonicalize().map_err(|e| {
        ParishError::Config(format!("failed to resolve {}: {e}", candidate.display()))
    })?;
    if !canonical.starts_with(&mod_dir) {
        return Err(ParishError::Config(format!(
            "manifest path {rel} escapes mod directory"
        )));
    }
    Ok(canonical)
}

fn validate_scene_asset(mod_dir: &Path, field: &str, rel: &str) -> Result<(), ParishError> {
    assets::canonical_mod_asset_path(mod_dir, rel)
        .map(|_| ())
        .map_err(|e| ParishError::Config(format!("{field}: {e}")))
}

fn validate_shape_coords(scene: &SceneDef, hotspot: &Hotspot, warnings: &mut Vec<String>) {
    match &hotspot.shape {
        HotspotShape::Rect([x, y, w, h]) => {
            if !percent(*x) || !percent(*y) || !percent(*w) || !percent(*h) {
                warnings.push(format!(
                    "scene '{}' hotspot '{}' has out-of-range rect [{}, {}, {}, {}]",
                    scene.slug, hotspot.id, x, y, w, h
                ));
            } else if x + w > 100.0 || y + h > 100.0 {
                warnings.push(format!(
                    "scene '{}' hotspot '{}' rect exceeds scene bounds [{}, {}, {}, {}]",
                    scene.slug, hotspot.id, x, y, w, h
                ));
            }
        }
        HotspotShape::Polygon(points) => {
            for [x, y] in points {
                if !percent(*x) || !percent(*y) {
                    warnings.push(format!(
                        "scene '{}' hotspot '{}' has out-of-range polygon point ({}, {})",
                        scene.slug, hotspot.id, x, y
                    ));
                }
            }
        }
    }
}

fn percent(value: f32) -> bool {
    (0.0..=100.0).contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    use parish_npc::{Npc, manager::NpcManager};
    use parish_world::graph::WorldGraph;
    use tempfile::TempDir;

    fn write_asset(root: &Path, rel: &str) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"placeholder").unwrap();
    }

    fn write_scene_index(root: &Path, body: &str) {
        fs::write(root.join("scenes.json"), body).unwrap();
    }

    fn valid_scene_json() -> &'static str {
        r#"{
            "scenes": [
                {
                    "location_id": 2,
                    "slug": "darcys-pub",
                    "plate": "assets/scenes/darcys-pub/plate.png",
                    "variants": { "night": "assets/scenes/darcys-pub/plate_night.png" },
                    "hotspots": [
                        {
                            "id": "door",
                            "shape": { "rect": [82.0, 38.0, 14.0, 50.0] },
                            "label": "Out to the Crossroads",
                            "action": { "travel_to": 1 }
                        },
                        {
                            "id": "hearth",
                            "shape": { "rect": [5.0, 30.0, 18.0, 40.0] },
                            "label": "The hearth",
                            "action": { "inspect": "A turf fire smoulders." }
                        }
                    ],
                    "slots": [
                        { "id": "behind-bar", "x": 48.0, "y": 55.0, "scale": 1.0, "prefer_npc": 1 },
                        { "id": "bench-left", "x": 22.0, "y": 68.0 }
                    ]
                }
            ],
            "sprites": [
                { "npc_id": 1, "image": "assets/scenes/sprites/padraig-darcy.png" }
            ],
            "fallback_sprites": {
                "default": "assets/scenes/sprites/generic-villager.png"
            }
        }"#
    }

    fn scene_mod() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_asset(root, "assets/scenes/darcys-pub/plate.png");
        write_asset(root, "assets/scenes/darcys-pub/plate_night.png");
        write_asset(root, "assets/scenes/sprites/padraig-darcy.png");
        write_asset(root, "assets/scenes/sprites/generic-villager.png");
        write_scene_index(root, valid_scene_json());
        tmp
    }

    #[test]
    fn load_parses_scene_schema_and_lookup_helpers() {
        let tmp = scene_mod();

        let index = SceneIndex::load(tmp.path(), "scenes.json").unwrap();

        assert_eq!(index.scenes.len(), 1);
        assert_eq!(index.sprites.len(), 1);
        assert_eq!(index.sprite_asset_count(), 2);
        assert_eq!(
            index.load_summary("scenes.json"),
            "scenes.json loaded: 1 scenes, 2 sprites"
        );
        assert!(index.scene_for(LocationId(2)).is_some());
        assert!(index.scene_for(LocationId(99)).is_none());
        assert_eq!(
            index
                .sprite_for(NpcId(1))
                .map(|sprite| sprite.image.as_str()),
            Some("assets/scenes/sprites/padraig-darcy.png")
        );
        assert!(index.sprite_for(NpcId(99)).is_none());
    }

    #[test]
    fn scene_schema_roundtrips_rect_and_action_variants() {
        let index: SceneIndex = serde_json::from_str(valid_scene_json()).unwrap();
        let encoded = serde_json::to_string(&index).unwrap();
        let decoded: SceneIndex = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, index);
        assert!(matches!(
            decoded.scenes[0].hotspots[0].shape,
            HotspotShape::Rect([82.0, 38.0, 14.0, 50.0])
        ));
        assert_eq!(
            decoded.scenes[0].hotspots[0].action,
            HotspotAction::TravelTo(LocationId(1))
        );
        assert_eq!(
            decoded.scenes[0].hotspots[1].action,
            HotspotAction::Inspect("A turf fire smoulders.".to_string())
        );
    }

    #[test]
    fn scene_assets_reject_traversal_absolute_non_assets_and_missing_files() {
        for (asset, expected) in [
            ("../escape.png", "must live under assets/"),
            ("/tmp/escape.png", "must be relative"),
            ("plates/plate.png", "must live under assets/"),
            ("assets/scenes/missing.png", "failed to resolve"),
        ] {
            let tmp = scene_mod();
            let body = valid_scene_json().replace("assets/scenes/darcys-pub/plate.png", asset);
            write_scene_index(tmp.path(), &body);

            let err = SceneIndex::load(tmp.path(), "scenes.json")
                .expect_err("invalid asset path should reject")
                .to_string();
            assert!(err.contains(expected), "expected {expected:?}, got {err}");
        }
    }

    #[test]
    fn scene_index_path_must_stay_inside_mod_directory() {
        let outer = TempDir::new().unwrap();
        let root = outer.path().join("mod");
        fs::create_dir_all(&root).unwrap();
        fs::write(outer.path().join("scenes.json"), "{}").unwrap();

        let err = SceneIndex::load(&root, "../scenes.json")
            .expect_err("scene index traversal should reject")
            .to_string();
        assert!(err.contains("escapes mod directory"), "got: {err}");
    }

    #[test]
    fn cross_validation_warns_without_rejecting_unknown_ids_and_bad_coords() {
        let index: SceneIndex = serde_json::from_str(
            r#"{
                "scenes": [
                    {
                        "location_id": 99,
                        "slug": "bad-scene",
                        "plate": "assets/scenes/bad/plate.png",
                        "hotspots": [
                            {
                                "id": "bad-exit",
                                "shape": { "rect": [95.0, 95.0, 10.0, 10.0] },
                                "label": "Bad exit",
                                "action": { "travel_to": 98 }
                            },
                            {
                                "id": "bad-talk",
                                "shape": { "polygon": [[10.0, 10.0], [101.0, 20.0]] },
                                "label": "Bad talk",
                                "action": { "talk_to": 97 }
                            }
                        ],
                        "slots": [
                            { "id": "bad-slot", "x": -1.0, "y": 101.0, "scale": 0.0, "prefer_npc": 96 }
                        ]
                    }
                ],
                "sprites": [
                    { "npc_id": 95, "image": "assets/scenes/sprites/missing-person.png" }
                ]
            }"#,
        )
        .unwrap();
        let world = WorldGraph::new();
        let npcs = NpcManager::new();

        let warnings = validate_scenes(&index, &world, &npcs);

        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unknown location id 99")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("unknown location id 98")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("unknown NPC id 97")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("rect exceeds scene bounds")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("out-of-range polygon point")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("out-of-range coordinates")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("non-positive scale")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("unknown NPC id 96")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("unknown NPC id 95")),
            "{warnings:?}"
        );
    }

    #[test]
    fn cross_validation_accepts_known_ids_and_valid_coords() {
        let index: SceneIndex = serde_json::from_str(valid_scene_json()).unwrap();
        let world = WorldGraph::load_from_str(
            r#"{
                "locations": [
                    {
                        "id": 1,
                        "name": "The Crossroads",
                        "description_template": "roads",
                        "indoor": false,
                        "public": true,
                        "connections": [{"target": 2, "path_description": "road"}]
                    },
                    {
                        "id": 2,
                        "name": "Darcy's Pub",
                        "description_template": "pub",
                        "indoor": true,
                        "public": true,
                        "connections": [{"target": 1, "path_description": "road"}]
                    }
                ]
            }"#,
        )
        .unwrap();
        let mut npcs = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(1);
        npcs.add_npc(npc);

        let warnings = validate_scenes(&index, &world, &npcs);

        assert!(warnings.is_empty(), "{warnings:?}");
    }
}
