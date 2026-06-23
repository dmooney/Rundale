//! Backend-agnostic scene-diorama state builder.
//!
//! The diorama layer is presentation data loaded from the active game mod, but
//! the decisions about which scene is active, which plate variant is selected,
//! and how NPCs are assigned to declared slots must be identical in every
//! runtime. This module keeps that logic in `parish-core`; server, Tauri,
//! headless, and MCP adapters only translate asset references into transport-
//! specific URLs.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::config::FeatureFlags;
use crate::game_mod::{HotspotAction, HotspotShape, SceneIndex, SceneLayer};
use crate::npc::manager::NpcManager;
use crate::npc::mood::mood_emoji;
use crate::npc::{Npc, NpcId};
use crate::world::time::TimeOfDay;
use crate::world::{Weather, WorldState};

/// Current schema version of [`SceneState`]. Bump on any breaking change.
pub const SCENE_STATE_SCHEMA_VERSION: u32 = 1;

/// Serializable view model for the active diorama scene.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneState {
    pub schema_version: u32,
    pub location_id: u32,
    pub location_name: String,
    pub indoor: bool,
    pub slug: String,
    pub native_size: [u32; 2],
    pub underlay_url: Option<String>,
    pub plate_url: String,
    pub variant: String,
    pub weather_overlay: Option<String>,
    pub layers: Vec<SceneLayerView>,
    pub hotspots: Vec<SceneHotspotView>,
    pub slots: Vec<SceneSlotView>,
    pub npcs: Vec<SceneNpcView>,
    pub overflow_npcs: Vec<SceneOverflowNpc>,
}

/// Clickable hotspot serialized for the frontend/agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneHotspotView {
    pub id: String,
    pub label: String,
    pub shape: HotspotShape,
    pub action: HotspotAction,
}

/// Runtime-composed visual layer serialized for compositor clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneLayerView {
    pub id: String,
    pub asset_id: String,
    pub kind: String,
    pub asset_url: String,
    pub x: f32,
    pub y: f32,
    pub z: i32,
    pub scale: f32,
    pub opacity: f32,
    pub flip: bool,
    pub anchor: [f32; 2],
    pub labels: Vec<SceneLayerLabelView>,
}

/// Runtime text painted onto a scene layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneLayerLabelView {
    pub text: String,
    pub anchor: [f32; 2],
    pub rotation: f32,
}

/// Declared NPC slot, including whether an NPC was assigned to it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneSlotView {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub prefer_npc: Option<u32>,
    pub occupied_npc_id: Option<u32>,
}

/// Assigned NPC sprite view in scene coordinates.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SceneNpcView {
    pub npc_id: u32,
    pub slot_id: String,
    pub display_name: String,
    pub real_name: Option<String>,
    pub introduced: bool,
    pub mood: String,
    pub mood_emoji: String,
    pub sprite_url: Option<String>,
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub flip: bool,
}

/// Present NPC that did not fit any declared slot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SceneOverflowNpc {
    pub npc_id: u32,
    pub display_name: String,
    pub real_name: Option<String>,
    pub introduced: bool,
}

/// Builds scene state with mod-relative asset references.
///
/// This function performs no I/O and deliberately leaves `plate_url` /
/// `sprite_url` as authored `assets/scenes/...` paths. Runtime adapters can
/// safely call it while holding state locks, then map the asset references to
/// HTTP URLs or data URLs after those locks are released.
pub fn build_scene_state_relative(
    world: &WorldState,
    npc_manager: &NpcManager,
    scenes: Option<&SceneIndex>,
    flags: &FeatureFlags,
) -> Option<SceneState> {
    if !flags.is_enabled("diorama") {
        return None;
    }

    let scenes = scenes?;
    let scene = scenes.scene_for(world.player_location)?;
    let loc = world.current_location();
    let indoor = world
        .current_location_data()
        .map(|data| data.indoor)
        .unwrap_or(loc.indoor);
    let variant = selected_variant(world, scene.variants.contains_key("night"));
    let plate_url = if variant == "night" {
        scene
            .variants
            .get("night")
            .cloned()
            .unwrap_or_else(|| scene.plate.clone())
    } else {
        scene.plate.clone()
    };
    let weather_overlay = if indoor || world.weather == Weather::Clear {
        None
    } else {
        Some(world.weather.to_string())
    };

    let mut present = npc_manager.npcs_at(world.player_location);
    present.sort_by_key(|npc| npc.id.0);

    let mut assigned: Vec<Option<NpcId>> = vec![None; scene.slots.len()];
    let mut used = BTreeSet::new();

    for (idx, slot) in scene.slots.iter().enumerate() {
        let Some(preferred) = slot.prefer_npc else {
            continue;
        };
        if used.contains(&preferred) {
            continue;
        }
        if present.iter().any(|npc| npc.id == preferred) {
            assigned[idx] = Some(preferred);
            used.insert(preferred);
        }
    }

    for npc in &present {
        if used.contains(&npc.id) {
            continue;
        }
        if let Some((idx, _)) = assigned
            .iter()
            .enumerate()
            .find(|(_, occupant)| occupant.is_none())
        {
            assigned[idx] = Some(npc.id);
            used.insert(npc.id);
        }
    }

    let slots = scene
        .slots
        .iter()
        .zip(assigned.iter())
        .map(|(slot, occupant)| SceneSlotView {
            id: slot.id.clone(),
            x: slot.x,
            y: slot.y,
            scale: slot.scale,
            prefer_npc: slot.prefer_npc.map(|id| id.0),
            occupied_npc_id: occupant.map(|id| id.0),
        })
        .collect();

    let npcs = scene
        .slots
        .iter()
        .zip(assigned.iter())
        .filter_map(|(slot, occupant)| {
            let id = (*occupant)?;
            let npc = present.iter().find(|npc| npc.id == id)?;
            Some(scene_npc_view(
                npc_manager,
                scenes,
                npc,
                &slot.id,
                slot.x,
                slot.y,
                slot.scale,
            ))
        })
        .collect();

    let overflow_npcs = present
        .into_iter()
        .filter(|npc| !used.contains(&npc.id))
        .map(|npc| scene_overflow_view(npc_manager, npc))
        .collect();

    Some(SceneState {
        schema_version: SCENE_STATE_SCHEMA_VERSION,
        location_id: world.player_location.0,
        location_name: loc.name.clone(),
        indoor,
        slug: scene.slug.clone(),
        native_size: scene.native_size,
        underlay_url: scene.underlay.clone(),
        plate_url,
        variant: variant.to_string(),
        weather_overlay,
        layers: scene_layers(scenes, &scene.layers),
        hotspots: scene
            .hotspots
            .iter()
            .map(|hotspot| SceneHotspotView {
                id: hotspot.id.clone(),
                label: hotspot.label.clone(),
                shape: hotspot.shape.clone(),
                action: hotspot.action.clone(),
            })
            .collect(),
        slots,
        npcs,
        overflow_npcs,
    })
}

/// Rewrites mod-relative asset references in an already-built scene state.
///
/// Returning `None` for the plate invalidates the whole state because there is
/// no diorama without a background plate. Missing sprite assets only clear that
/// NPC's sprite URL; the rest of the scene can still be rendered or inspected.
pub fn map_scene_state_asset_urls(
    mut state: SceneState,
    asset_url: &dyn Fn(&str) -> Option<String>,
) -> Option<SceneState> {
    state.plate_url = asset_url(&state.plate_url)?;
    if let Some(underlay) = state.underlay_url.take() {
        state.underlay_url = asset_url(&underlay);
    }
    for layer in &mut state.layers {
        layer.asset_url = asset_url(&layer.asset_url)?;
    }
    for npc in &mut state.npcs {
        if let Some(sprite) = npc.sprite_url.take() {
            npc.sprite_url = asset_url(&sprite);
        }
    }
    Some(state)
}

/// Builds scene state and maps mod-relative asset references through `asset_url`.
pub fn build_scene_state(
    world: &WorldState,
    npc_manager: &NpcManager,
    scenes: Option<&SceneIndex>,
    flags: &FeatureFlags,
    asset_url: &dyn Fn(&str) -> Option<String>,
) -> Option<SceneState> {
    let state = build_scene_state_relative(world, npc_manager, scenes, flags)?;
    map_scene_state_asset_urls(state, asset_url)
}

/// Renders a stable, script-friendly text form for `/scene`.
pub fn render_scene_state_text(state: Option<&SceneState>) -> String {
    let Some(state) = state else {
        return "No active diorama scene.".to_string();
    };

    let hotspots = if state.hotspots.is_empty() {
        "(none)".to_string()
    } else {
        state
            .hotspots
            .iter()
            .map(|hotspot| hotspot.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let overflow = if state.overflow_npcs.is_empty() {
        "(none)".to_string()
    } else {
        state
            .overflow_npcs
            .iter()
            .map(|npc| format!("{}: {}", npc.npc_id, npc.display_name))
            .collect::<Vec<_>>()
            .join(", ")
    };

    let mut lines = vec![
        "[SCENE]".to_string(),
        format!("  location_id: {}", state.location_id),
        format!("  location_name: {}", state.location_name),
        format!("  slug: {}", state.slug),
        format!(
            "  native_size: {}x{}",
            state.native_size[0], state.native_size[1]
        ),
        format!(
            "  underlay_url: {}",
            state.underlay_url.as_deref().unwrap_or("(none)")
        ),
        format!("  variant: {}", state.variant),
        format!("  plate_url: {}", state.plate_url),
        format!(
            "  weather_overlay: {}",
            state.weather_overlay.as_deref().unwrap_or("(none)")
        ),
        "  layers:".to_string(),
    ];

    if state.layers.is_empty() {
        lines.push("    (none)".to_string());
    } else {
        for layer in &state.layers {
            let labels = if layer.labels.is_empty() {
                "(none)".to_string()
            } else {
                layer
                    .labels
                    .iter()
                    .map(|label| label.text.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            lines.push(format!(
                "    z={}: {} asset={} kind={} url={} at=({}, {}) scale={} opacity={} labels={}",
                layer.z,
                layer.id,
                layer.asset_id,
                layer.kind,
                layer.asset_url,
                layer.x,
                layer.y,
                layer.scale,
                layer.opacity,
                labels
            ));
        }
    }

    lines.extend([format!("  hotspots: {hotspots}"), "  slots:".to_string()]);

    if state.slots.is_empty() {
        lines.push("    (none)".to_string());
    } else {
        for slot in &state.slots {
            let preferred = slot
                .prefer_npc
                .map(|id| format!("preferred_npc: {id}"))
                .unwrap_or_else(|| "preferred_npc: (none)".to_string());
            match state.npcs.iter().find(|npc| npc.slot_id == slot.id) {
                Some(npc) => lines.push(format!(
                    "    {}: {} (npc_id: {}, introduced: {}, {}, sprite: {})",
                    slot.id,
                    npc.display_name,
                    npc.npc_id,
                    if npc.introduced { "yes" } else { "no" },
                    preferred,
                    npc.sprite_url.as_deref().unwrap_or("(none)")
                )),
                None => lines.push(format!("    {}: (empty, {})", slot.id, preferred)),
            }
        }
    }
    lines.push(format!("  overflow_npcs: {overflow}"));
    lines.join("\n")
}

fn selected_variant(world: &WorldState, has_night_variant: bool) -> &'static str {
    if has_night_variant
        && matches!(
            world.clock.time_of_day(),
            TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight
        )
    {
        "night"
    } else {
        "day"
    }
}

fn scene_layers(scenes: &SceneIndex, layers: &[SceneLayer]) -> Vec<SceneLayerView> {
    let mut views: Vec<SceneLayerView> = layers
        .iter()
        .filter_map(|layer| {
            let asset = scenes.asset_for(&layer.asset)?;
            Some(SceneLayerView {
                id: layer.id.clone(),
                asset_id: layer.asset.clone(),
                kind: asset.kind.clone(),
                asset_url: asset.image.clone(),
                x: layer.x,
                y: layer.y,
                z: layer.z,
                scale: layer.scale,
                opacity: layer.opacity,
                flip: layer.flip,
                anchor: asset.anchor,
                labels: layer
                    .labels
                    .iter()
                    .map(|label| SceneLayerLabelView {
                        text: label.text.clone(),
                        anchor: label.anchor,
                        rotation: label.rotation,
                    })
                    .collect(),
            })
        })
        .collect();
    views.sort_by(|a, b| a.z.cmp(&b.z).then_with(|| a.id.cmp(&b.id)));
    views
}

fn scene_npc_view(
    npc_manager: &NpcManager,
    scenes: &SceneIndex,
    npc: &Npc,
    slot_id: &str,
    x: f32,
    y: f32,
    scale: f32,
) -> SceneNpcView {
    let introduced = npc_manager.is_introduced(npc.id);
    SceneNpcView {
        npc_id: npc.id.0,
        slot_id: slot_id.to_string(),
        display_name: npc_manager.display_name(npc).to_string(),
        real_name: introduced.then(|| npc.name.clone()),
        introduced,
        mood: npc.mood.clone(),
        mood_emoji: mood_emoji(&npc.mood).to_string(),
        sprite_url: scenes
            .sprite_for(npc.id)
            .map(|sprite| sprite.image.clone())
            .or_else(|| scenes.fallback_sprites.get("default").cloned()),
        x,
        y,
        scale,
        flip: false,
    }
}

fn scene_overflow_view(npc_manager: &NpcManager, npc: &Npc) -> SceneOverflowNpc {
    let introduced = npc_manager.is_introduced(npc.id);
    SceneOverflowNpc {
        npc_id: npc.id.0,
        display_name: npc_manager.display_name(npc).to_string(),
        real_name: introduced.then(|| npc.name.clone()),
        introduced,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use super::*;
    use crate::game_mod::{
        Hotspot, NpcSlot, SceneAsset, SceneDef, SceneLayer, SceneLayerLabel, SpriteDef,
    };
    use crate::npc::Npc;
    use crate::npc::types::NpcState;
    use crate::world::LocationId;

    fn flags_on() -> FeatureFlags {
        let mut flags = FeatureFlags::default();
        flags.enable("diorama");
        flags
    }

    fn world_at(location_id: u32) -> WorldState {
        let path =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale/world.json");
        let mut world = WorldState::from_parish_file(&path, LocationId(15)).unwrap();
        world.player_location = LocationId(location_id);
        world
    }

    fn scene_index() -> SceneIndex {
        let mut fallback_sprites = BTreeMap::new();
        fallback_sprites.insert(
            "default".to_string(),
            "assets/scenes/sprites/generic-villager.png".to_string(),
        );

        let mut crossroads_variants = BTreeMap::new();
        crossroads_variants.insert(
            "night".to_string(),
            "assets/scenes/the-crossroads/plate-night.png".to_string(),
        );

        SceneIndex {
            assets: vec![
                SceneAsset {
                    id: "crossroads-underlay".to_string(),
                    kind: "underlay".to_string(),
                    image: "assets/scenes/the-crossroads/plate.png".to_string(),
                    anchor: [50.0, 100.0],
                },
                SceneAsset {
                    id: "pub-underlay".to_string(),
                    kind: "underlay".to_string(),
                    image: "assets/scenes/darcys-pub/plate.png".to_string(),
                    anchor: [50.0, 100.0],
                },
                SceneAsset {
                    id: "wayfinding-sign".to_string(),
                    kind: "wayfinding_sign".to_string(),
                    image: "assets/scenes/sprites/generic-villager.png".to_string(),
                    anchor: [50.0, 100.0],
                },
            ],
            scenes: vec![
                SceneDef {
                    location_id: LocationId(1),
                    slug: "the-crossroads".to_string(),
                    native_size: [1280, 720],
                    underlay: Some("assets/scenes/the-crossroads/plate.png".to_string()),
                    plate: "assets/scenes/the-crossroads/plate.png".to_string(),
                    variants: crossroads_variants,
                    layers: vec![
                        SceneLayer {
                            id: "wayfinding".to_string(),
                            asset: "wayfinding-sign".to_string(),
                            x: 58.0,
                            y: 50.0,
                            z: 40,
                            scale: 0.8,
                            opacity: 1.0,
                            flip: false,
                            labels: vec![SceneLayerLabel {
                                text: "Darcy's Pub".to_string(),
                                anchor: [50.0, 35.0],
                                rotation: -1.0,
                            }],
                        },
                        SceneLayer {
                            id: "underlay".to_string(),
                            asset: "crossroads-underlay".to_string(),
                            x: 50.0,
                            y: 50.0,
                            z: 0,
                            scale: 1.0,
                            opacity: 1.0,
                            flip: false,
                            labels: vec![],
                        },
                    ],
                    hotspots: vec![Hotspot {
                        id: "pub-lane".to_string(),
                        shape: HotspotShape::Rect([1.0, 2.0, 3.0, 4.0]),
                        label: "Lane to the pub".to_string(),
                        action: HotspotAction::TravelTo(LocationId(2)),
                    }],
                    slots: vec![
                        NpcSlot {
                            id: "roadside-left".to_string(),
                            x: 25.0,
                            y: 60.0,
                            scale: 1.0,
                            prefer_npc: None,
                        },
                        NpcSlot {
                            id: "roadside-right".to_string(),
                            x: 60.0,
                            y: 60.0,
                            scale: 1.0,
                            prefer_npc: None,
                        },
                    ],
                },
                SceneDef {
                    location_id: LocationId(2),
                    slug: "darcys-pub".to_string(),
                    native_size: [1280, 720],
                    underlay: Some("assets/scenes/darcys-pub/plate.png".to_string()),
                    plate: "assets/scenes/darcys-pub/plate.png".to_string(),
                    variants: BTreeMap::new(),
                    layers: vec![
                        SceneLayer {
                            id: "door-sign".to_string(),
                            asset: "wayfinding-sign".to_string(),
                            x: 84.0,
                            y: 42.0,
                            z: 50,
                            scale: 0.7,
                            opacity: 1.0,
                            flip: false,
                            labels: vec![SceneLayerLabel {
                                text: "The Crossroads".to_string(),
                                anchor: [50.0, 48.0],
                                rotation: 0.0,
                            }],
                        },
                        SceneLayer {
                            id: "underlay".to_string(),
                            asset: "pub-underlay".to_string(),
                            x: 50.0,
                            y: 50.0,
                            z: 0,
                            scale: 1.0,
                            opacity: 1.0,
                            flip: false,
                            labels: vec![],
                        },
                    ],
                    hotspots: vec![Hotspot {
                        id: "front-door".to_string(),
                        shape: HotspotShape::Rect([0.0, 0.0, 10.0, 10.0]),
                        label: "Front door".to_string(),
                        action: HotspotAction::TravelTo(LocationId(1)),
                    }],
                    slots: vec![
                        NpcSlot {
                            id: "behind-bar".to_string(),
                            x: 50.0,
                            y: 52.0,
                            scale: 1.0,
                            prefer_npc: Some(NpcId(1)),
                        },
                        NpcSlot {
                            id: "bench-left".to_string(),
                            x: 24.0,
                            y: 68.0,
                            scale: 1.05,
                            prefer_npc: None,
                        },
                        NpcSlot {
                            id: "bench-right".to_string(),
                            x: 69.0,
                            y: 66.0,
                            scale: 1.0,
                            prefer_npc: None,
                        },
                    ],
                },
            ],
            sprites: vec![SpriteDef {
                npc_id: NpcId(2),
                image: "assets/scenes/sprites/tailor.png".to_string(),
            }],
            fallback_sprites,
        }
    }

    fn present_npc(id: u32, name: &str, brief: &str, location: u32) -> Npc {
        let mut npc = Npc::new_test_npc();
        npc.id = NpcId(id);
        npc.name = name.to_string();
        npc.brief_description = brief.to_string();
        npc.location = LocationId(location);
        npc.state = NpcState::Present;
        npc.mood = "content".to_string();
        npc
    }

    fn manager_with(npcs: Vec<Npc>) -> NpcManager {
        let mut manager = NpcManager::new();
        for npc in npcs {
            manager.add_npc(npc);
        }
        manager
    }

    #[test]
    fn scene_state_is_flag_gated() {
        let world = world_at(2);
        let npcs = manager_with(vec![present_npc(
            1,
            "Padraig Darcy",
            "an older publican",
            2,
        )]);
        assert!(
            build_scene_state_relative(
                &world,
                &npcs,
                Some(&scene_index()),
                &FeatureFlags::default()
            )
            .is_none()
        );
    }

    #[test]
    fn scene_state_absent_without_scene_for_location() {
        let world = world_at(15);
        let npcs = manager_with(vec![]);
        assert!(
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).is_none()
        );
    }

    #[test]
    fn deterministic_slots_fill_preferred_then_sorted_then_overflow() {
        let world = world_at(2);
        let npcs = manager_with(vec![
            present_npc(4, "Fourth", "a fourth villager", 2),
            present_npc(2, "Second", "a second villager", 2),
            present_npc(1, "Preferred", "a preferred publican", 2),
            present_npc(3, "Third", "a third villager", 2),
        ]);

        let first =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();
        let second =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();

        let assigned: Vec<(&str, u32)> = first
            .npcs
            .iter()
            .map(|npc| (npc.slot_id.as_str(), npc.npc_id))
            .collect();
        assert_eq!(
            assigned,
            vec![("behind-bar", 1), ("bench-left", 2), ("bench-right", 3)]
        );
        assert_eq!(
            first
                .overflow_npcs
                .iter()
                .map(|npc| npc.npc_id)
                .collect::<Vec<_>>(),
            vec![4]
        );
        assert_eq!(first, second);
    }

    #[test]
    fn scene_state_includes_ordered_compositor_layers() {
        let world = world_at(2);
        let npcs = manager_with(vec![]);

        let state =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();

        assert_eq!(state.native_size, [1280, 720]);
        assert_eq!(
            state.underlay_url.as_deref(),
            Some("assets/scenes/darcys-pub/plate.png")
        );
        assert_eq!(
            state
                .layers
                .iter()
                .map(|layer| (layer.id.as_str(), layer.z))
                .collect::<Vec<_>>(),
            vec![("underlay", 0), ("door-sign", 50)]
        );
        assert_eq!(state.layers[1].asset_id, "wayfinding-sign");
        assert_eq!(state.layers[1].kind, "wayfinding_sign");
        assert_eq!(state.layers[1].labels[0].text, "The Crossroads");
    }

    #[test]
    fn introduction_semantics_hide_real_name_until_introduced() {
        let world = world_at(2);
        let mut npcs = manager_with(vec![present_npc(
            1,
            "Padraig Darcy",
            "an older man behind the bar",
            2,
        )]);
        let hidden =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();
        assert_eq!(hidden.npcs[0].display_name, "an older man behind the bar");
        assert_eq!(hidden.npcs[0].real_name, None);

        npcs.mark_introduced(NpcId(1));
        let visible =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();
        assert_eq!(visible.npcs[0].display_name, "Padraig Darcy");
        assert_eq!(visible.npcs[0].real_name.as_deref(), Some("Padraig Darcy"));
    }

    #[test]
    fn dusk_uses_night_variant_when_available() {
        let mut world = world_at(1);
        world.clock.advance(9 * 60);
        let npcs = manager_with(vec![]);
        let state =
            build_scene_state_relative(&world, &npcs, Some(&scene_index()), &flags_on()).unwrap();
        assert_eq!(state.variant, "night");
        assert_eq!(
            state.plate_url,
            "assets/scenes/the-crossroads/plate-night.png"
        );
    }

    #[test]
    fn indoor_locations_suppress_weather_overlay() {
        let mut pub_world = world_at(2);
        pub_world.weather = Weather::HeavyRain;
        let npcs = manager_with(vec![]);
        let pub_state =
            build_scene_state_relative(&pub_world, &npcs, Some(&scene_index()), &flags_on())
                .unwrap();
        assert_eq!(pub_state.weather_overlay, None);

        let mut road_world = world_at(1);
        road_world.weather = Weather::HeavyRain;
        let road_state =
            build_scene_state_relative(&road_world, &npcs, Some(&scene_index()), &flags_on())
                .unwrap();
        assert_eq!(road_state.weather_overlay.as_deref(), Some("Heavy Rain"));
    }

    #[test]
    fn asset_mapper_rewrites_plate_and_sprites() {
        let world = world_at(2);
        let npcs = manager_with(vec![present_npc(2, "Tailor", "a quiet tailor", 2)]);
        let state = build_scene_state(&world, &npcs, Some(&scene_index()), &flags_on(), &|rel| {
            Some(format!("/api/scene-asset/{rel}?v=1"))
        })
        .unwrap();
        assert_eq!(
            state.plate_url,
            "/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1"
        );
        assert_eq!(
            state.underlay_url.as_deref(),
            Some("/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1")
        );
        assert_eq!(
            state.layers[0].asset_url,
            "/api/scene-asset/assets/scenes/darcys-pub/plate.png?v=1"
        );
        assert_eq!(
            state.layers[1].asset_url,
            "/api/scene-asset/assets/scenes/sprites/generic-villager.png?v=1"
        );
        assert_eq!(
            state.npcs[0].sprite_url.as_deref(),
            Some("/api/scene-asset/assets/scenes/sprites/tailor.png?v=1")
        );
    }
}
