//! Scene-diorama schema and loader.
//!
//! Scenes are optional mod data: a mod may point `[files].scenes` at a JSON
//! index that declares background plates, clickable hotspots, and NPC sprite
//! anchors. Loading validates asset paths strictly; cross-reference checks
//! against the world graph and NPC roster are warnings because scene data is a
//! presentation layer, not a reason to reject otherwise-playable content.

use std::collections::{BTreeMap, BTreeSet};
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
    /// Reusable visual atoms referenced by scene layers.
    #[serde(default)]
    pub assets: Vec<SceneAsset>,
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
    #[serde(default = "default_native_size")]
    pub native_size: [u32; 2],
    #[serde(default)]
    pub underlay: Option<String>,
    pub plate: String,
    #[serde(default)]
    pub variants: BTreeMap<String, String>,
    #[serde(default)]
    pub layers: Vec<SceneLayer>,
    #[serde(default)]
    pub hotspots: Vec<Hotspot>,
    #[serde(default)]
    pub slots: Vec<NpcSlot>,
}

/// Reusable compositor asset declared once and placed by scene layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneAsset {
    pub id: String,
    #[serde(default = "default_asset_kind")]
    pub kind: String,
    pub image: String,
    #[serde(default = "default_asset_anchor")]
    pub anchor: [f32; 2],
}

/// A visual atom placed into one scene at runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLayer {
    pub id: String,
    pub asset: String,
    pub x: f32,
    pub y: f32,
    pub z: i32,
    #[serde(default = "default_layer_scale")]
    pub scale: f32,
    #[serde(default = "default_layer_opacity")]
    pub opacity: f32,
    #[serde(default)]
    pub flip: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub animation: Option<SceneLayerAnimation>,
    #[serde(default)]
    pub labels: Vec<SceneLayerLabel>,
}

/// Optional ambient animation applied to a single compositor layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLayerAnimation {
    pub mode: SceneLayerAnimationMode,
    #[serde(default)]
    pub amplitude_x: f32,
    #[serde(default)]
    pub amplitude_y: f32,
    #[serde(default)]
    pub alpha: f32,
    #[serde(default = "default_animation_period_ms")]
    pub period_ms: u32,
    #[serde(default)]
    pub phase: f32,
}

/// Ambient animation style for a raster scene atom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneLayerAnimationMode {
    Drift,
    Shimmer,
    Flicker,
}

/// Runtime text painted onto a compositor layer, e.g. a wayfinding sign.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneLayerLabel {
    pub text: String,
    pub anchor: [f32; 2],
    #[serde(default)]
    pub rotation: f32,
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

fn default_native_size() -> [u32; 2] {
    [1280, 720]
}

fn default_asset_kind() -> String {
    "prop".to_string()
}

fn default_asset_anchor() -> [f32; 2] {
    [50.0, 100.0]
}

fn default_layer_scale() -> f32 {
    1.0
}

fn default_layer_opacity() -> f32 {
    1.0
}

fn default_animation_period_ms() -> u32 {
    4000
}

const MAX_LAYER_LABEL_CHARS: usize = 32;
const MAX_ABS_Z: i32 = 10_000;
const MIN_ANIMATION_PERIOD_MS: u32 = 250;
const MAX_ANIMATION_PERIOD_MS: u32 = 60_000;
const MAX_ANIMATION_AMPLITUDE_PX: f32 = 24.0;
const MAX_ANIMATION_ALPHA: f32 = 0.5;

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

    /// Returns a compositor asset by id, if one is declared.
    pub fn asset_for(&self, asset_id: &str) -> Option<&SceneAsset> {
        self.assets.iter().find(|asset| asset.id == asset_id)
    }

    /// Returns the sprite definition for an NPC id, if one is declared.
    pub fn sprite_for(&self, npc_id: NpcId) -> Option<&SpriteDef> {
        self.sprites.iter().find(|sprite| sprite.npc_id == npc_id)
    }

    /// Number of sprite asset references, including fallback sprites.
    pub fn sprite_asset_count(&self) -> usize {
        self.sprites.len() + self.fallback_sprites.len()
    }

    /// Number of compositor layer instances across all scenes.
    pub fn layer_count(&self) -> usize {
        self.scenes.iter().map(|scene| scene.layers.len()).sum()
    }

    /// Human-readable load summary used by debug/proof tooling.
    pub fn load_summary(&self, rel: &str) -> String {
        format!(
            "{rel} loaded: {} scenes, {} layers, {} assets, {} sprites",
            self.scenes.len(),
            self.layer_count(),
            self.assets.len(),
            self.sprite_asset_count()
        )
    }

    fn validate_assets(&self, mod_dir: &Path) -> Result<(), ParishError> {
        let mut asset_ids = BTreeSet::new();
        for asset in &self.assets {
            if asset.id.trim().is_empty() {
                return Err(ParishError::Config(
                    "scene asset id must not be empty".to_string(),
                ));
            }
            if !asset_ids.insert(asset.id.as_str()) {
                return Err(ParishError::Config(format!(
                    "duplicate scene asset id '{}'",
                    asset.id
                )));
            }
            validate_scene_asset(
                mod_dir,
                &format!("asset '{}'.image", asset.id),
                &asset.image,
            )?;
            validate_percent_pair(&format!("asset '{}'.anchor", asset.id), asset.anchor)?;
        }

        let mut scene_location_ids = BTreeSet::new();
        let mut scene_slugs = BTreeSet::new();
        for scene in &self.scenes {
            if !scene_location_ids.insert(scene.location_id) {
                return Err(ParishError::Config(format!(
                    "duplicate scene location id {}",
                    scene.location_id.0
                )));
            }
            if scene.slug.trim().is_empty() {
                return Err(ParishError::Config(
                    "scene slug must not be empty".to_string(),
                ));
            }
            if !scene_slugs.insert(scene.slug.as_str()) {
                return Err(ParishError::Config(format!(
                    "duplicate scene slug '{}'",
                    scene.slug
                )));
            }
            validate_native_size(scene)?;
            validate_scene_asset(
                mod_dir,
                &format!("scene '{}'.plate", scene.slug),
                &scene.plate,
            )?;
            if let Some(underlay) = &scene.underlay {
                validate_scene_asset(
                    mod_dir,
                    &format!("scene '{}'.underlay", scene.slug),
                    underlay,
                )?;
            }
            for (variant, path) in &scene.variants {
                validate_scene_asset(
                    mod_dir,
                    &format!("scene '{}'.variants.{variant}", scene.slug),
                    path,
                )?;
            }

            let mut layer_ids = BTreeSet::new();
            let mut hotspot_ids = BTreeSet::new();
            let mut slot_ids = BTreeSet::new();
            for layer in &scene.layers {
                if layer.id.trim().is_empty() {
                    return Err(ParishError::Config(format!(
                        "scene '{}' layer id must not be empty",
                        scene.slug
                    )));
                }
                if !layer_ids.insert(layer.id.as_str()) {
                    return Err(ParishError::Config(format!(
                        "scene '{}' duplicate layer id '{}'",
                        scene.slug, layer.id
                    )));
                }
                if self.asset_for(&layer.asset).is_none() {
                    return Err(ParishError::Config(format!(
                        "scene '{}' layer '{}' references unknown asset '{}'",
                        scene.slug, layer.id, layer.asset
                    )));
                }
                validate_layer(scene, layer)?;
            }
            for hotspot in &scene.hotspots {
                if hotspot.id.trim().is_empty() {
                    return Err(ParishError::Config(format!(
                        "scene '{}' hotspot id must not be empty",
                        scene.slug
                    )));
                }
                if !hotspot_ids.insert(hotspot.id.as_str()) {
                    return Err(ParishError::Config(format!(
                        "scene '{}' duplicate hotspot id '{}'",
                        scene.slug, hotspot.id
                    )));
                }
            }
            for slot in &scene.slots {
                if slot.id.trim().is_empty() {
                    return Err(ParishError::Config(format!(
                        "scene '{}' slot id must not be empty",
                        scene.slug
                    )));
                }
                if !slot_ids.insert(slot.id.as_str()) {
                    return Err(ParishError::Config(format!(
                        "scene '{}' duplicate slot id '{}'",
                        scene.slug, slot.id
                    )));
                }
            }
        }

        let mut sprite_ids = BTreeSet::new();
        for sprite in &self.sprites {
            if !sprite_ids.insert(sprite.npc_id) {
                return Err(ParishError::Config(format!(
                    "duplicate scene sprite npc id {}",
                    sprite.npc_id.0
                )));
            }
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

        for layer in &scene.layers {
            if let Some(asset) = scenes.asset_for(&layer.asset)
                && asset.kind == "wayfinding_sign"
            {
                validate_wayfinding_labels(scene, layer, world, &mut warnings);
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

fn validate_native_size(scene: &SceneDef) -> Result<(), ParishError> {
    if scene.native_size[0] == 0 || scene.native_size[1] == 0 {
        return Err(ParishError::Config(format!(
            "scene '{}' native_size must be positive",
            scene.slug
        )));
    }
    Ok(())
}

fn validate_percent_pair(field: &str, pair: [f32; 2]) -> Result<(), ParishError> {
    if !percent(pair[0]) || !percent(pair[1]) {
        return Err(ParishError::Config(format!(
            "{field} has out-of-range coordinates ({}, {})",
            pair[0], pair[1]
        )));
    }
    Ok(())
}

fn validate_layer(scene: &SceneDef, layer: &SceneLayer) -> Result<(), ParishError> {
    if !percent(layer.x) || !percent(layer.y) {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has out-of-range coordinates ({}, {})",
            scene.slug, layer.id, layer.x, layer.y
        )));
    }
    if layer.z < -MAX_ABS_Z || layer.z > MAX_ABS_Z {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has invalid z-order {}",
            scene.slug, layer.id, layer.z
        )));
    }
    if layer.scale <= 0.0 {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has non-positive scale {}",
            scene.slug, layer.id, layer.scale
        )));
    }
    if !(0.0..=1.0).contains(&layer.opacity) {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has out-of-range opacity {}",
            scene.slug, layer.id, layer.opacity
        )));
    }
    if let Some(animation) = &layer.animation {
        validate_animation(scene, layer, animation)?;
    }
    for label in &layer.labels {
        if label.text.chars().count() > MAX_LAYER_LABEL_CHARS {
            return Err(ParishError::Config(format!(
                "scene '{}' layer '{}' label '{}' exceeds {} chars",
                scene.slug, layer.id, label.text, MAX_LAYER_LABEL_CHARS
            )));
        }
        validate_percent_pair(
            &format!("scene '{}' layer '{}' label anchor", scene.slug, layer.id),
            label.anchor,
        )?;
    }
    Ok(())
}

fn validate_animation(
    scene: &SceneDef,
    layer: &SceneLayer,
    animation: &SceneLayerAnimation,
) -> Result<(), ParishError> {
    if animation.period_ms < MIN_ANIMATION_PERIOD_MS
        || animation.period_ms > MAX_ANIMATION_PERIOD_MS
    {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has out-of-range animation period {}ms",
            scene.slug, layer.id, animation.period_ms
        )));
    }
    for (field, value) in [
        ("amplitude_x", animation.amplitude_x),
        ("amplitude_y", animation.amplitude_y),
    ] {
        if !value.is_finite() || value.abs() > MAX_ANIMATION_AMPLITUDE_PX {
            return Err(ParishError::Config(format!(
                "scene '{}' layer '{}' has out-of-range animation {field} {}",
                scene.slug, layer.id, value
            )));
        }
    }
    if !animation.alpha.is_finite() || !(0.0..=MAX_ANIMATION_ALPHA).contains(&animation.alpha) {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has out-of-range animation alpha {}",
            scene.slug, layer.id, animation.alpha
        )));
    }
    if !animation.phase.is_finite() || !(0.0..=1.0).contains(&animation.phase) {
        return Err(ParishError::Config(format!(
            "scene '{}' layer '{}' has out-of-range animation phase {}",
            scene.slug, layer.id, animation.phase
        )));
    }
    Ok(())
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

fn validate_wayfinding_labels(
    scene: &SceneDef,
    layer: &SceneLayer,
    world: &WorldGraph,
    warnings: &mut Vec<String>,
) {
    for label in &layer.labels {
        let known = world
            .location_ids()
            .into_iter()
            .filter_map(|id| world.get(id))
            .any(|loc| loc.name.eq_ignore_ascii_case(label.text.trim()));
        if !known {
            warnings.push(format!(
                "scene '{}' layer '{}' wayfinding label '{}' does not match a known location",
                scene.slug, layer.id, label.text
            ));
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
            "assets": [
                {
                    "id": "pub-underlay",
                    "kind": "underlay",
                    "image": "assets/scenes/darcys-pub/plate.png",
                    "anchor": [50.0, 100.0]
                },
                {
                    "id": "pub-sign",
                    "kind": "wayfinding_sign",
                    "image": "assets/scenes/darcys-pub/plate_night.png",
                    "anchor": [50.0, 88.0]
                }
            ],
            "scenes": [
                {
                    "location_id": 2,
                    "slug": "darcys-pub",
                    "native_size": [1280, 720],
                    "underlay": "assets/scenes/darcys-pub/plate.png",
                    "plate": "assets/scenes/darcys-pub/plate.png",
                    "variants": { "night": "assets/scenes/darcys-pub/plate_night.png" },
                    "layers": [
                        {
                            "id": "underlay",
                            "asset": "pub-underlay",
                            "x": 50.0,
                            "y": 50.0,
                            "z": 0,
                            "scale": 1.0
                        },
                        {
                            "id": "sign",
                            "asset": "pub-sign",
                            "x": 84.0,
                            "y": 42.0,
                            "z": 40,
                            "scale": 0.75,
                            "opacity": 1.0,
                            "animation": {
                                "mode": "flicker",
                                "alpha": 0.08,
                                "period_ms": 1400,
                                "phase": 0.25
                            },
                            "labels": [
                                { "text": "The Crossroads", "anchor": [50.0, 48.0], "rotation": -1.0 }
                            ]
                        }
                    ],
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
        assert_eq!(index.assets.len(), 2);
        assert_eq!(index.layer_count(), 2);
        assert_eq!(index.sprites.len(), 1);
        assert_eq!(index.sprite_asset_count(), 2);
        assert_eq!(
            index.load_summary("scenes.json"),
            "scenes.json loaded: 1 scenes, 2 layers, 2 assets, 2 sprites"
        );
        assert!(index.scene_for(LocationId(2)).is_some());
        assert!(index.scene_for(LocationId(99)).is_none());
        assert_eq!(
            index.asset_for("pub-sign").map(|asset| asset.kind.as_str()),
            Some("wayfinding_sign")
        );
        assert!(index.asset_for("missing").is_none());
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
        assert_eq!(decoded.scenes[0].native_size, [1280, 720]);
        assert_eq!(
            decoded.scenes[0].underlay.as_deref(),
            Some("assets/scenes/darcys-pub/plate.png")
        );
        assert_eq!(decoded.scenes[0].layers[1].z, 40);
        let animation = decoded.scenes[0].layers[1].animation.as_ref().unwrap();
        assert_eq!(animation.mode, SceneLayerAnimationMode::Flicker);
        assert_eq!(animation.alpha, 0.08);
        assert_eq!(animation.period_ms, 1400);
        assert_eq!(animation.phase, 0.25);
        assert_eq!(decoded.scenes[0].layers[1].labels[0].text, "The Crossroads");
    }

    #[test]
    fn legacy_plate_only_scene_defaults_compositor_fields() {
        let index: SceneIndex = serde_json::from_str(
            r#"{
                "scenes": [
                    {
                        "location_id": 2,
                        "slug": "legacy-pub",
                        "plate": "assets/scenes/darcys-pub/plate.png"
                    }
                ]
            }"#,
        )
        .unwrap();

        assert!(index.assets.is_empty());
        assert_eq!(index.scenes[0].native_size, [1280, 720]);
        assert_eq!(index.scenes[0].underlay, None);
        assert!(index.scenes[0].layers.is_empty());
    }

    #[test]
    fn scene_layer_animation_validation_rejects_out_of_bounds_values() {
        for (field, value, expected) in [
            (
                "period_ms",
                serde_json::json!(249),
                "animation period 249ms",
            ),
            (
                "amplitude_x",
                serde_json::json!(25.0),
                "animation amplitude_x 25",
            ),
            (
                "amplitude_y",
                serde_json::json!(-25.0),
                "animation amplitude_y -25",
            ),
            ("alpha", serde_json::json!(0.75), "animation alpha 0.75"),
            ("phase", serde_json::json!(1.25), "animation phase 1.25"),
        ] {
            let tmp = scene_mod();
            let mut value_json: serde_json::Value =
                serde_json::from_str(valid_scene_json()).unwrap();
            value_json["scenes"][0]["layers"][1]["animation"][field] = value;
            write_scene_index(tmp.path(), &serde_json::to_string(&value_json).unwrap());

            let err = SceneIndex::load(tmp.path(), "scenes.json")
                .expect_err("invalid animation should reject")
                .to_string();
            assert!(err.contains(expected), "expected {expected:?}, got {err}");
        }
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
    fn scene_schema_rejects_duplicate_authoring_ids() {
        fn assert_duplicate(case: &str, edit: impl FnOnce(&mut serde_json::Value), expected: &str) {
            let tmp = scene_mod();
            let mut value: serde_json::Value = serde_json::from_str(valid_scene_json()).unwrap();
            edit(&mut value);
            write_scene_index(tmp.path(), &serde_json::to_string(&value).unwrap());

            let err = SceneIndex::load(tmp.path(), "scenes.json")
                .expect_err(&format!("{case} should reject duplicate ids"))
                .to_string();
            assert!(err.contains(expected), "expected {expected:?}, got {err}");
        }

        assert_duplicate(
            "scene-location",
            |value| {
                let first = value["scenes"][0].clone();
                value["scenes"].as_array_mut().unwrap().push(first);
            },
            "duplicate scene location id 2",
        );
        assert_duplicate(
            "scene-slug",
            |value| {
                let mut second = value["scenes"][0].clone();
                second["location_id"] = serde_json::json!(3);
                value["scenes"].as_array_mut().unwrap().push(second);
            },
            "duplicate scene slug 'darcys-pub'",
        );
        assert_duplicate(
            "hotspot",
            |value| {
                value["scenes"][0]["hotspots"][1]["id"] = serde_json::json!("door");
            },
            "duplicate hotspot id 'door'",
        );
        assert_duplicate(
            "slot",
            |value| {
                value["scenes"][0]["slots"][1]["id"] = serde_json::json!("behind-bar");
            },
            "duplicate slot id 'behind-bar'",
        );
        assert_duplicate(
            "sprite",
            |value| {
                let first = value["sprites"][0].clone();
                value["sprites"].as_array_mut().unwrap().push(first);
            },
            "duplicate scene sprite npc id 1",
        );
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
    fn compositor_required_data_rejects_invalid_assets_layers_and_labels() {
        for (case, needle, replacement, expected) in [
            (
                "duplicate-asset",
                "\"id\": \"pub-underlay\"",
                "\"id\": \"pub-underlay\"",
                "duplicate scene asset id 'pub-underlay'",
            ),
            (
                "bad-asset-path",
                "\"image\": \"assets/scenes/darcys-pub/plate.png\"",
                "\"image\": \"../escape.png\"",
                "must live under assets/",
            ),
            (
                "unknown-layer-asset",
                "\"asset\": \"pub-underlay\"",
                "\"asset\": \"missing-asset\"",
                "references unknown asset 'missing-asset'",
            ),
            (
                "duplicate-layer",
                "\"id\": \"sign\"",
                "\"id\": \"underlay\"",
                "duplicate layer id 'underlay'",
            ),
            ("bad-z", "\"z\": 40", "\"z\": 10001", "invalid z-order"),
            (
                "bad-negative-z",
                "\"z\": 40",
                "\"z\": -10001",
                "invalid z-order",
            ),
            (
                "bad-x",
                "\"x\": 84.0",
                "\"x\": 101.0",
                "out-of-range coordinates",
            ),
            (
                "bad-opacity",
                "\"opacity\": 1.0",
                "\"opacity\": 2.0",
                "out-of-range opacity",
            ),
            (
                "bad-scale",
                "\"scale\": 0.75",
                "\"scale\": 0.0",
                "non-positive scale",
            ),
            (
                "long-label",
                "\"text\": \"The Crossroads\"",
                "\"text\": \"This wayfinding label is intentionally far too long\"",
                "exceeds 32 chars",
            ),
        ] {
            let tmp = scene_mod();
            let mut body = valid_scene_json().to_string();
            if case == "duplicate-asset" {
                body = body.replacen("\"id\": \"pub-sign\"", "\"id\": \"pub-underlay\"", 1);
            } else {
                body = body.replacen(needle, replacement, 1);
            }
            write_scene_index(tmp.path(), &body);

            let err = SceneIndex::load(tmp.path(), "scenes.json")
                .expect_err("invalid compositor data should reject")
                .to_string();
            assert!(err.contains(expected), "expected {expected:?}, got {err}");
        }
    }

    #[test]
    fn cross_validation_warns_without_rejecting_unknown_ids_and_bad_coords() {
        let index: SceneIndex = serde_json::from_str(
            r#"{
                "assets": [
                    {
                        "id": "bad-sign-asset",
                        "kind": "wayfinding_sign",
                        "image": "assets/scenes/sprites/missing-person.png"
                    }
                ],
                "scenes": [
                    {
                        "location_id": 99,
                        "slug": "bad-scene",
                        "plate": "assets/scenes/bad/plate.png",
                        "layers": [
                            {
                                "id": "bad-sign",
                                "asset": "bad-sign-asset",
                                "x": 20.0,
                                "y": 20.0,
                                "z": 5,
                                "labels": [
                                    { "text": "Nowhere Road", "anchor": [50.0, 50.0] }
                                ]
                            }
                        ],
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
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("wayfinding label 'Nowhere Road'")),
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

    #[test]
    fn real_rundale_kilteevan_uses_layered_png_atoms() {
        let rundale_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let scenes = SceneIndex::load(&rundale_dir, "scenes.json").unwrap();
        let scene = scenes
            .scene_for(LocationId(15))
            .expect("Kilteevan scene should be declared");

        assert_eq!(scene.slug, "kilteevan-village");
        assert!(
            scene.layers.len() >= 8,
            "Kilteevan should be a layered compositor scene, got {} layer(s)",
            scene.layers.len()
        );

        let atom_layers = scene
            .layers
            .iter()
            .filter_map(|layer| scenes.asset_for(&layer.asset).map(|asset| (layer, asset)))
            .filter(|(_, asset)| asset.image != scene.plate)
            .filter(|(_, asset)| asset.image.contains("/kilteevan-village/atoms/"))
            .collect::<Vec<_>>();

        assert!(
            atom_layers.len() >= 8,
            "expected multiple non-plate Kilteevan atom layers, got {atom_layers:?}"
        );
        assert!(
            atom_layers.iter().all(|(_, asset)| {
                asset
                    .image
                    .starts_with("assets/scenes/kilteevan-village/atoms/")
                    && asset.image.ends_with(".png")
                    && !asset.image.ends_with(".svg")
            }),
            "Kilteevan atom layers should all be PNG assets: {atom_layers:?}"
        );
        assert!(
            atom_layers
                .iter()
                .any(|(layer, asset)| layer.id == "contact-shadows"
                    && asset.image.ends_with("contact-shadows.png")
                    && asset.kind == "shadow"),
            "Kilteevan should include a transparent contact-shadow atom"
        );
        assert!(
            atom_layers
                .iter()
                .any(|(layer, asset)| layer.id == "well-ground-patch"
                    && asset.image.ends_with("ground-patch.png")
                    && asset.kind == "terrain_patch"),
            "Kilteevan terrain patches must stay non-ground atoms so Pixi does not stretch them"
        );
    }

    #[test]
    fn real_rundale_crossroads_and_pub_use_layered_png_atoms() {
        let rundale_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let scenes = SceneIndex::load(&rundale_dir, "scenes.json").unwrap();

        for (location_id, slug, min_layers, expected_hotspots, expected_slots) in [
            (
                LocationId(1),
                "the-crossroads",
                14,
                [
                    "pub-lane",
                    "church-boreen",
                    "kilteevan-road",
                    "stone-wall",
                    "crossroads-signpost",
                ]
                .as_slice(),
                ["roadside-left", "roadside-right", "wall-gossip"].as_slice(),
            ),
            (
                LocationId(2),
                "darcys-pub",
                8,
                ["front-door", "hearth", "bar", "settle-bench"].as_slice(),
                ["behind-bar", "bench-left", "bench-right"].as_slice(),
            ),
        ] {
            let scene = scenes
                .scene_for(location_id)
                .unwrap_or_else(|| panic!("{slug} scene should be declared"));
            assert_eq!(scene.slug, slug);
            assert!(
                scene.layers.len() >= min_layers,
                "{slug} should be a layered compositor scene, got {} layer(s)",
                scene.layers.len()
            );
            assert!(
                scene.layers.iter().all(|layer| layer.id != "pixel-plate"),
                "{slug} should not use a live pixel-plate layer"
            );

            let atom_layers = scene
                .layers
                .iter()
                .filter_map(|layer| scenes.asset_for(&layer.asset).map(|asset| (layer, asset)))
                .collect::<Vec<_>>();

            assert!(
                atom_layers.iter().all(|(_, asset)| {
                    asset
                        .image
                        .starts_with(&format!("assets/scenes/{slug}/atoms/"))
                        && asset.image.ends_with(".png")
                        && !asset.image.ends_with(".svg")
                        && !asset.image.contains("pixel-plate")
                }),
                "{slug} live layers should all be PNG atom assets: {atom_layers:?}"
            );
            assert!(
                expected_hotspots
                    .iter()
                    .all(|id| scene.hotspots.iter().any(|hotspot| hotspot.id == *id)),
                "{slug} lost expected hotspots: {:?}",
                scene
                    .hotspots
                    .iter()
                    .map(|hotspot| hotspot.id.as_str())
                    .collect::<Vec<_>>()
            );
            assert!(
                expected_slots
                    .iter()
                    .all(|id| scene.slots.iter().any(|slot| slot.id == *id)),
                "{slug} lost expected slots: {:?}",
                scene
                    .slots
                    .iter()
                    .map(|slot| slot.id.as_str())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn real_rundale_crossroads_reuses_small_sprite_kit_atoms() {
        let rundale_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let scenes = SceneIndex::load(&rundale_dir, "scenes.json").unwrap();
        let scene = scenes
            .scene_for(LocationId(1))
            .expect("Crossroads scene should be declared");
        let mut usage_by_asset: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        let mut kit_layer_count = 0;

        for layer in &scene.layers {
            let Some(asset) = scenes.asset_for(&layer.asset) else {
                continue;
            };
            if !asset
                .image
                .starts_with("assets/scenes/the-crossroads/atoms/kit/")
            {
                continue;
            }

            let bytes = std::fs::read(rundale_dir.join(&asset.image)).unwrap_or_else(|err| {
                panic!(
                    "failed to read Crossroads kit atom '{}': {err}",
                    asset.image
                )
            });
            assert_eq!(&bytes[1..4], b"PNG", "{} should be a PNG", asset.image);
            let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
            let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());

            assert_ne!([layer.x, layer.y], [50.0, 50.0], "{}", layer.id);
            assert!(
                width < 360 && height < 240,
                "{} should be a small reusable atom, got {width}x{height}",
                asset.image
            );
            usage_by_asset
                .entry(layer.asset.clone())
                .or_default()
                .insert(format!("{:.3},{:.3}", layer.x, layer.y));
            kit_layer_count += 1;
        }

        assert!(
            kit_layer_count >= 4,
            "Crossroads should include several small kit atom layers"
        );
        assert!(
            usage_by_asset
                .values()
                .any(|distinct_positions| distinct_positions.len() >= 3),
            "at least one Crossroads kit atom should be reused in three positions: {usage_by_asset:?}"
        );
    }

    #[test]
    fn real_rundale_declares_named_png_npc_sprites() {
        let rundale_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let scenes = SceneIndex::load(&rundale_dir, "scenes.json").unwrap();

        assert_eq!(scenes.sprite_asset_count(), 4);
        for (npc_id, image) in [
            (NpcId(1), "assets/scenes/sprites/padraig-darcy.png"),
            (NpcId(8), "assets/scenes/sprites/niamh-darcy.png"),
            (NpcId(22), "assets/scenes/sprites/peig-hannigan.png"),
        ] {
            assert_eq!(
                scenes
                    .sprite_for(npc_id)
                    .map(|sprite| sprite.image.as_str()),
                Some(image)
            );
        }
        assert_eq!(
            scenes.fallback_sprites.get("default").map(String::as_str),
            Some("assets/scenes/sprites/generic-villager.png")
        );
        assert!(scenes.sprites.iter().all(|sprite| {
            sprite.image.starts_with("assets/scenes/sprites/")
                && sprite.image.ends_with(".png")
                && !sprite.image.ends_with(".svg")
        }));
    }
}
