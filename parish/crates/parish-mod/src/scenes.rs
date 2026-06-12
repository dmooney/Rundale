//! Scene data model for the interactive parish diorama.
//!
//! A mod may declare per-location pixel-art scenes in a single `scenes.json`
//! index (mirroring the one-file-per-domain pattern of `world.json` /
//! `npcs.json`). Each scene pins a curated background plate to a location,
//! plus clickable hotspots (exits, doors, objects) and NPC anchor slots. The
//! diorama presentation layer (M2+) composes a live view from this static data
//! plus the running simulation; this module owns only the **schema, loading,
//! and validation**.
//!
//! Design: [`docs/design/ideas/parish-diorama.md`]. The scene index is an
//! optional mod file — a `mod.toml` without a `scenes` key loads unchanged and
//! `GameMod::scenes` is `None`.
//!
//! ## Coordinate convention
//!
//! All positions are **percentages (0–100) of the plate's native
//! dimensions** — `x,y` is a sprite/slot foot-anchor; a `rect` is
//! `[x, y, w, h]`. Percentage coordinates keep hotspots, sprites, and the
//! plate congruent at any display size.

use std::collections::HashMap;
use std::path::Path;

use parish_npc::manager::NpcManager;
use parish_types::error::ParishError;
use parish_types::ids::{LocationId, NpcId};
use parish_world::graph::WorldGraph;
use serde::{Deserialize, Serialize};

/// The parsed `scenes.json` index for one mod.
///
/// Load via [`SceneIndex::load`], which validates every referenced asset path
/// against the mod directory (no traversal, no absolute paths, must live under
/// `assets/`, must exist on disk). Cross-validation against the world graph and
/// NPC roster is a separate, non-fatal step — see [`validate_scenes`].
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneIndex {
    /// One scene per covered location.
    #[serde(default)]
    pub scenes: Vec<SceneDef>,
    /// Per-NPC sprite overrides.
    #[serde(default)]
    pub sprites: Vec<SpriteDef>,
    /// Fallback sprite(s) for NPCs without a dedicated entry.
    #[serde(default)]
    pub fallback_sprites: Option<FallbackSprites>,
}

/// A single location's scene: a background plate plus its interactive layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDef {
    /// World-graph location id this scene renders.
    pub location_id: u32,
    /// Stable, human-readable identifier (e.g. `"darcys-pub"`). Used as the
    /// frontend cache key and the asset sub-directory name.
    pub slug: String,
    /// Relative path to the default background plate (under `assets/`).
    pub plate: String,
    /// Optional alternate plates keyed by variant name (e.g. `"night"`).
    #[serde(default)]
    pub variants: HashMap<String, String>,
    /// Clickable regions: exits, doors, objects, signs.
    #[serde(default)]
    pub hotspots: Vec<Hotspot>,
    /// NPC anchor positions; the simulation decides who fills them.
    #[serde(default)]
    pub slots: Vec<NpcSlot>,
}

/// A clickable region of a plate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hotspot {
    /// Stable identifier, unique within the scene.
    pub id: String,
    /// The region's geometry (percentage coordinates).
    pub shape: HotspotShape,
    /// Player-facing label (tooltip / `aria-label`).
    pub label: String,
    /// What activating the hotspot does.
    pub action: HotspotAction,
}

/// Hotspot geometry. Coordinates are percentages (0–100) of the plate.
///
/// `polygon` is reserved for later authoring; the MVP authors rects only.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotShape {
    /// Axis-aligned rectangle `[x, y, w, h]`.
    Rect([f32; 4]),
    /// Arbitrary polygon as `[[x, y], ...]` vertices (reserved).
    Polygon(Vec<[f32; 2]>),
}

/// What a hotspot (or sprite) click does. Each maps onto an existing input
/// path in the frontend (M3): travel, address-an-NPC, or inspect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HotspotAction {
    /// Travel to another location (by id).
    TravelTo(u32),
    /// Open dialogue addressed to an NPC (by id).
    TalkTo(u32),
    /// Print flavour text to the log; no world change.
    Inspect(String),
}

/// An NPC anchor position within a scene. The art is fixed; *who* stands here
/// comes from the live schedule simulation, never from the plate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpcSlot {
    /// Stable identifier, unique within the scene.
    pub id: String,
    /// Foot-anchor x, percentage (0–100) of plate width.
    pub x: f32,
    /// Foot-anchor y, percentage (0–100) of plate height.
    pub y: f32,
    /// Sprite scale multiplier (1.0 = native).
    #[serde(default = "default_scale")]
    pub scale: f32,
    /// Pins a specific NPC to this slot (publican behind the bar, etc.).
    #[serde(default)]
    pub prefer_npc: Option<u32>,
}

fn default_scale() -> f32 {
    1.0
}

/// A per-NPC sprite override.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteDef {
    /// NPC id this sprite represents.
    pub npc_id: u32,
    /// Relative path to the sprite PNG (under `assets/`).
    pub image: String,
}

/// Fallback sprite(s) used when an NPC has no dedicated [`SpriteDef`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FallbackSprites {
    /// Default sprite for any unlisted NPC.
    #[serde(default)]
    pub default: Option<String>,
}

impl SceneIndex {
    /// Load and validate a scene index from `<mod_dir>/<rel>`.
    ///
    /// Validation performed here (all fatal — surfaced as
    /// [`ParishError::Config`]):
    ///
    /// - the index file itself must resolve inside `mod_dir` (no traversal);
    /// - every referenced asset (plate, each variant, each sprite, the
    ///   fallback) must pass [`crate::assets::canonical_mod_asset_path`] — i.e.
    ///   be relative, live under `assets/`, contain no traversal, and **exist
    ///   on disk**.
    ///
    /// Cross-validation against the world graph / NPC roster (unknown ids,
    /// out-of-range coordinates) is deliberately *not* fatal — see
    /// [`validate_scenes`].
    pub fn load(mod_dir: &Path, rel: &str) -> Result<Self, ParishError> {
        // Guard the index path itself the same way GameMod::load guards every
        // manifest-referenced file: resolve and confirm it stays inside the
        // mod directory before reading.
        let candidate = mod_dir.join(rel);
        let canonical = candidate.canonicalize().map_err(|e| {
            ParishError::Config(format!("failed to resolve {}: {e}", candidate.display()))
        })?;
        if !canonical.starts_with(mod_dir) {
            return Err(ParishError::Config(format!(
                "scenes path {rel} escapes mod directory"
            )));
        }
        let text = std::fs::read_to_string(&canonical).map_err(|e| {
            ParishError::Config(format!("failed to read {}: {e}", canonical.display()))
        })?;
        let index: SceneIndex = serde_json::from_str(&text)
            .map_err(|e| ParishError::Config(format!("failed to parse {rel}: {e}")))?;

        index.validate_asset_paths(mod_dir)?;

        tracing::info!(
            "scenes.json loaded: {} scenes, {} sprites",
            index.scenes.len(),
            index.sprites.len()
        );
        Ok(index)
    }

    /// Fatally validate every asset reference against the mod directory.
    fn validate_asset_paths(&self, mod_dir: &Path) -> Result<(), ParishError> {
        let check = |field: &str, rel: &str| -> Result<(), ParishError> {
            crate::assets::canonical_mod_asset_path(mod_dir, rel)
                .map(|_| ())
                .map_err(|e| ParishError::Config(format!("{field}: {e}")))
        };
        for scene in &self.scenes {
            check(&format!("scene '{}' plate", scene.slug), &scene.plate)?;
            for (variant, path) in &scene.variants {
                check(&format!("scene '{}' variant '{variant}'", scene.slug), path)?;
            }
        }
        for sprite in &self.sprites {
            check(&format!("sprite for npc {}", sprite.npc_id), &sprite.image)?;
        }
        if let Some(fallback) = &self.fallback_sprites
            && let Some(default) = &fallback.default
        {
            check("fallback_sprites.default", default)?;
        }
        Ok(())
    }

    /// Returns the scene for a location id, if one is declared.
    pub fn scene_for(&self, location_id: u32) -> Option<&SceneDef> {
        self.scenes.iter().find(|s| s.location_id == location_id)
    }

    /// Returns the sprite path for an NPC: their dedicated sprite if declared,
    /// else the fallback default, else `None`.
    pub fn sprite_for(&self, npc_id: u32) -> Option<&str> {
        if let Some(sprite) = self.sprites.iter().find(|s| s.npc_id == npc_id) {
            return Some(sprite.image.as_str());
        }
        self.fallback_sprites
            .as_ref()
            .and_then(|f| f.default.as_deref())
    }
}

/// Cross-validate a scene index against the live world graph and NPC roster.
///
/// Returns a list of human-readable warning strings — **never fails**. Mirrors
/// the soft-validation posture of other optional mod files: a scene that names
/// a non-existent location, pins a non-existent NPC, points a `travel_to` at a
/// missing target, addresses a `talk_to` at an unknown NPC, or places a
/// coordinate outside 0–100 is reported but does not block the load. Callers
/// (the parish-core mod-load site) log each warning.
pub fn validate_scenes(index: &SceneIndex, world: &WorldGraph, npcs: &NpcManager) -> Vec<String> {
    let mut warnings = Vec::new();
    let loc_exists = |id: u32| world.get(LocationId(id)).is_some();
    let npc_exists = |id: u32| npcs.npcs().contains_key(&NpcId(id));
    let check_coord = |w: &mut Vec<String>, what: &str, value: f32| {
        if !(0.0..=100.0).contains(&value) {
            w.push(format!("{what}: coordinate {value} is outside 0–100"));
        }
    };

    for scene in &index.scenes {
        if !loc_exists(scene.location_id) {
            warnings.push(format!(
                "scene '{}' references unknown location id {}",
                scene.slug, scene.location_id
            ));
        }
        for hotspot in &scene.hotspots {
            match &hotspot.action {
                HotspotAction::TravelTo(target) => {
                    if !loc_exists(*target) {
                        warnings.push(format!(
                            "scene '{}' hotspot '{}' travels to unknown location id {target}",
                            scene.slug, hotspot.id
                        ));
                    }
                }
                HotspotAction::TalkTo(npc) => {
                    if !npc_exists(*npc) {
                        warnings.push(format!(
                            "scene '{}' hotspot '{}' addresses unknown npc id {npc}",
                            scene.slug, hotspot.id
                        ));
                    }
                }
                HotspotAction::Inspect(_) => {}
            }
            if let HotspotShape::Rect([x, y, w, h]) = &hotspot.shape {
                let label = format!("scene '{}' hotspot '{}'", scene.slug, hotspot.id);
                check_coord(&mut warnings, &label, *x);
                check_coord(&mut warnings, &label, *y);
                check_coord(&mut warnings, &label, *w);
                check_coord(&mut warnings, &label, *h);
            } else if let HotspotShape::Polygon(points) = &hotspot.shape {
                let label = format!("scene '{}' hotspot '{}'", scene.slug, hotspot.id);
                for [px, py] in points {
                    check_coord(&mut warnings, &label, *px);
                    check_coord(&mut warnings, &label, *py);
                }
            }
        }
        for slot in &scene.slots {
            let label = format!("scene '{}' slot '{}'", scene.slug, slot.id);
            check_coord(&mut warnings, &label, slot.x);
            check_coord(&mut warnings, &label, slot.y);
            if let Some(npc) = slot.prefer_npc
                && !npc_exists(npc)
            {
                warnings.push(format!(
                    "scene '{}' slot '{}' prefers unknown npc id {npc}",
                    scene.slug, slot.id
                ));
            }
        }
    }

    for sprite in &index.sprites {
        if !npc_exists(sprite.npc_id) {
            warnings.push(format!(
                "sprite references unknown npc id {}",
                sprite.npc_id
            ));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_JSON: &str = r#"{
        "scenes": [
            {
                "location_id": 2,
                "slug": "darcys-pub",
                "plate": "assets/scenes/darcys-pub/plate.png",
                "variants": { "night": "assets/scenes/darcys-pub/plate_night.png" },
                "hotspots": [
                    { "id": "door", "shape": { "rect": [82.0, 38.0, 14.0, 50.0] },
                      "label": "Out to the Crossroads", "action": { "travel_to": 1 } },
                    { "id": "publican", "shape": { "rect": [45.0, 50.0, 10.0, 30.0] },
                      "label": "The publican", "action": { "talk_to": 1 } },
                    { "id": "hearth", "shape": { "rect": [5.0, 30.0, 18.0, 40.0] },
                      "label": "The hearth", "action": { "inspect": "A turf fire smoulders." } }
                ],
                "slots": [
                    { "id": "behind-bar", "x": 48.0, "y": 55.0, "scale": 1.0, "prefer_npc": 1 },
                    { "id": "bench-left", "x": 22.0, "y": 68.0, "scale": 1.1 }
                ]
            }
        ],
        "sprites": [ { "npc_id": 1, "image": "assets/scenes/sprites/padraig-darcy.png" } ],
        "fallback_sprites": { "default": "assets/scenes/sprites/generic-villager.png" }
    }"#;

    #[test]
    fn parses_every_action_and_shape_variant() {
        let index: SceneIndex = serde_json::from_str(FULL_JSON).unwrap();
        assert_eq!(index.scenes.len(), 1);
        let scene = &index.scenes[0];
        assert_eq!(scene.location_id, 2);
        assert_eq!(scene.slug, "darcys-pub");
        assert_eq!(
            scene.variants.get("night").map(String::as_str),
            Some("assets/scenes/darcys-pub/plate_night.png")
        );
        assert_eq!(scene.hotspots.len(), 3);
        assert_eq!(scene.hotspots[0].action, HotspotAction::TravelTo(1));
        assert_eq!(scene.hotspots[1].action, HotspotAction::TalkTo(1));
        assert_eq!(
            scene.hotspots[2].action,
            HotspotAction::Inspect("A turf fire smoulders.".to_string())
        );
        assert!(matches!(
            scene.hotspots[0].shape,
            HotspotShape::Rect([82.0, 38.0, 14.0, 50.0])
        ));
    }

    #[test]
    fn slot_scale_defaults_to_one() {
        let index: SceneIndex = serde_json::from_str(FULL_JSON).unwrap();
        let slots = &index.scenes[0].slots;
        assert_eq!(slots[0].scale, 1.0); // explicit 1.0
        assert_eq!(slots[0].prefer_npc, Some(1));
        assert_eq!(slots[1].scale, 1.1); // explicit 1.1
        assert_eq!(slots[1].prefer_npc, None); // omitted → None

        // A slot omitting `scale` entirely defaults to 1.0.
        let slot: NpcSlot = serde_json::from_str(r#"{ "id": "x", "x": 5.0, "y": 6.0 }"#).unwrap();
        assert_eq!(slot.scale, 1.0);
        assert_eq!(slot.prefer_npc, None);
    }

    #[test]
    fn roundtrips_through_serde() {
        let index: SceneIndex = serde_json::from_str(FULL_JSON).unwrap();
        let serialized = serde_json::to_string(&index).unwrap();
        let reparsed: SceneIndex = serde_json::from_str(&serialized).unwrap();
        assert_eq!(reparsed.scenes.len(), index.scenes.len());
        assert_eq!(
            reparsed.scenes[0].hotspots[0].action,
            HotspotAction::TravelTo(1)
        );
        assert_eq!(reparsed.sprites[0].npc_id, 1);
    }

    #[test]
    fn polygon_shape_variant_parses() {
        let json = r#"{ "id": "yard", "shape": { "polygon": [[0.0,0.0],[10.0,0.0],[10.0,10.0]] },
            "label": "The yard", "action": { "inspect": "Mud." } }"#;
        let hotspot: Hotspot = serde_json::from_str(json).unwrap();
        assert!(matches!(hotspot.shape, HotspotShape::Polygon(ref p) if p.len() == 3));
    }

    #[test]
    fn empty_index_is_default() {
        let index: SceneIndex = serde_json::from_str("{}").unwrap();
        assert!(index.scenes.is_empty());
        assert!(index.sprites.is_empty());
        assert!(index.fallback_sprites.is_none());
    }

    #[test]
    fn scene_for_hits_and_misses() {
        let index: SceneIndex = serde_json::from_str(FULL_JSON).unwrap();
        assert_eq!(
            index.scene_for(2).map(|s| s.slug.as_str()),
            Some("darcys-pub")
        );
        assert!(index.scene_for(99).is_none());
    }

    #[test]
    fn sprite_for_prefers_dedicated_then_fallback() {
        let index: SceneIndex = serde_json::from_str(FULL_JSON).unwrap();
        // dedicated
        assert_eq!(
            index.sprite_for(1),
            Some("assets/scenes/sprites/padraig-darcy.png")
        );
        // fallback default for an unlisted npc
        assert_eq!(
            index.sprite_for(42),
            Some("assets/scenes/sprites/generic-villager.png")
        );
    }

    #[test]
    fn sprite_for_returns_none_without_fallback() {
        let json = r#"{ "sprites": [ { "npc_id": 1, "image": "assets/x.png" } ] }"#;
        let index: SceneIndex = serde_json::from_str(json).unwrap();
        assert_eq!(index.sprite_for(1), Some("assets/x.png"));
        assert_eq!(index.sprite_for(2), None);
    }
}
