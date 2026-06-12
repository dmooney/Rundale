//! Shared scene-state handler for the interactive parish diorama.
//!
//! Per rule #12 the orchestration that turns the static scene index
//! (`parish-mod` `SceneIndex`) plus the live simulation into a renderable
//! [`SceneState`] is defined **once** here, backend-agnostic, and adapted by
//! thin wiring in each entry point (`parish-server` route, `parish-tauri`
//! command, `parish-engine` `/scene` CLI). The single runtime-specific concern
//! — how an asset's relative path becomes a URL the frontend can load — is
//! injected as the `asset_url` closure (an HTTP route on the server, a data URL
//! over IPC on Tauri).
//!
//! Like [`super::engine_state`], this reads the same live `WorldState` +
//! `NpcManager` the other handlers use, so the rendered scene can never drift
//! from the simulation. The slot-assignment and variant-selection logic is
//! factored into pure helpers ([`select_variant`], [`assign_slots`]) that are
//! deterministic and directly unit-testable.
//!
//! Gating: the `diorama` feature flag is checked here on every fetch (single
//! source of truth — the frontend never holds a stale flag from mount time).
//! During build-out it is opt-in (`is_enabled`); M6 flips it to a default-on
//! kill switch (`!is_disabled`).

use serde::{Deserialize, Serialize};

use crate::config::FeatureFlags;
use crate::game_mod::scenes::{HotspotAction, HotspotShape, NpcSlot, SceneDef, SceneIndex};
use crate::npc::manager::NpcManager;
use crate::npc::mood::mood_emoji;
use crate::world::WorldState;
use parish_types::Weather;
use parish_types::time::TimeOfDay;

/// Current schema version of [`SceneState`]. Bump on any breaking change.
pub const SCENE_STATE_SCHEMA_VERSION: u32 = 1;

/// The feature flag gating the diorama presentation layer.
pub const DIORAMA_FLAG: &str = "diorama";

/// A clickable region of the rendered scene, projected for the frontend.
///
/// Mirrors the static `parish-mod` `Hotspot` one-to-one; kept as a distinct
/// IPC DTO so the wire shape is stable independent of the on-disk schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneHotspotView {
    /// Stable identifier, unique within the scene.
    pub id: String,
    /// Region geometry (percentage coordinates).
    pub shape: HotspotShape,
    /// Player-facing label (tooltip / `aria-label`).
    pub label: String,
    /// What activating the hotspot does.
    pub action: HotspotAction,
}

/// One NPC placed in a scene slot, identity-resolved for rendering.
///
/// **Introduction semantics (gameplay correctness):** an NPC is anonymous
/// until the player has been introduced. `display_name` is the brief
/// description ("an older man behind the bar") until then; `real_name` is
/// `Some` only after introduction. The diorama must never name an NPC the
/// dialogue system would still keep anonymous.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneNpcView {
    /// Stable NPC id.
    pub id: u32,
    /// Brief description until introduced, real name after.
    pub display_name: String,
    /// Canonical real name — populated only once introduced.
    pub real_name: Option<String>,
    /// Whether the player has been introduced to this NPC.
    pub introduced: bool,
    /// Mood emoji for at-a-glance display.
    pub mood_emoji: String,
    /// Resolved sprite URL (HTTP route or data URL).
    pub sprite_url: String,
    /// Foot-anchor x, percentage (0–100) of plate width.
    pub x: f32,
    /// Foot-anchor y, percentage (0–100) of plate height.
    pub y: f32,
    /// Sprite scale multiplier.
    pub scale: f32,
    /// Whether to render the sprite horizontally mirrored (NPCs on the right
    /// half of the plate face inward). Deterministic from slot position.
    pub flip: bool,
}

/// The renderable scene for the player's current location.
///
/// `None` from [`build_scene_state`] (flag off, or no scene for the current
/// location) means the frontend falls back to the existing text+map layout —
/// which doubles as the graceful per-location fallback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneState {
    /// Schema version so consumers can detect shape changes.
    pub schema_version: u32,
    /// The location this scene renders.
    pub location_id: u32,
    /// Resolved URL of the active plate (variant-selected).
    pub plate_url: String,
    /// Active variant name (`"day"`, `"night"`, a weather key, ...).
    pub variant: String,
    /// Whether the location is indoors (frontend suppresses rain overlays).
    pub indoor: bool,
    /// Clickable regions.
    pub hotspots: Vec<SceneHotspotView>,
    /// NPCs seated in slots, in slot declaration order.
    pub npcs: Vec<SceneNpcView>,
    /// Display names of present NPCs beyond slot capacity (or without art).
    pub overflow_npcs: Vec<String>,
}

/// Maps weather to its plate-variant key, if any. Reserved for future
/// weather-variant art; today no plate declares these keys, so selection
/// falls through to the time-of-day variant.
fn weather_variant_key(weather: Weather) -> Option<&'static str> {
    match weather {
        Weather::LightRain | Weather::HeavyRain | Weather::Storm => Some("rain"),
        Weather::Fog => Some("fog"),
        Weather::Clear | Weather::PartlyCloudy | Weather::Overcast => None,
    }
}

/// Evening = dusk through the small hours, when the `night` plate applies.
fn is_evening(tod: TimeOfDay) -> bool {
    matches!(
        tod,
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight
    )
}

/// Selects the active variant key for a scene. **Pure + deterministic.**
///
/// - Weather variants apply only **outdoors** — indoor scenes suppress them
///   (it does not rain inside Darcy's Pub), per the design's indoor-weather
///   rule. The time-of-day tint still applies indoors.
/// - The `night` variant applies indoors and outdoors in the evening.
/// - Returns a key that is guaranteed present in `scene.variants`, or `"day"`
///   (which maps to the base plate).
pub fn select_variant(scene: &SceneDef, tod: TimeOfDay, weather: Weather, indoor: bool) -> String {
    if !indoor
        && let Some(key) = weather_variant_key(weather)
        && scene.variants.contains_key(key)
    {
        return key.to_string();
    }
    if is_evening(tod) && scene.variants.contains_key("night") {
        return "night".to_string();
    }
    "day".to_string()
}

/// A present NPC whose sprite has already resolved, ready for seating.
#[derive(Debug, Clone)]
struct SeatCandidate {
    id: u32,
    display_name: String,
    real_name: Option<String>,
    introduced: bool,
    mood_emoji: String,
    sprite_url: String,
}

/// Deterministically assigns present NPCs to scene slots. **Pure.**
///
/// Pass 1 seats every slot whose `prefer_npc` is among the present candidates
/// (publican behind the bar, smith at the anvil). Pass 2 fills the remaining
/// slots, in slot declaration order, with the remaining candidates already
/// ordered by id. Candidates left over once every slot is full are returned as
/// `overflow` (their display names). Given identical inputs the output is
/// identical.
///
/// `candidates` MUST be pre-sorted by id for deterministic pass-2 seating.
fn assign_slots(
    slots: &[NpcSlot],
    candidates: &[SeatCandidate],
) -> (Vec<SceneNpcView>, Vec<String>) {
    let mut filled: Vec<Option<usize>> = vec![None; slots.len()]; // slot -> candidate idx
    let mut used = vec![false; candidates.len()];

    // Pass 1: pin preferred occupants. NPC ids are unique among the present
    // candidates, so locating by id is unambiguous.
    for (si, slot) in slots.iter().enumerate() {
        if let Some(pref) = slot.prefer_npc
            && let Some(ci) = candidates.iter().position(|c| c.id == pref)
            && !used[ci]
        {
            filled[si] = Some(ci);
            used[ci] = true;
        }
    }

    // Pass 2: fill remaining slots with remaining candidates (id order).
    let mut next = 0usize;
    for slot_fill in filled.iter_mut() {
        if slot_fill.is_some() {
            continue;
        }
        while next < candidates.len() && used[next] {
            next += 1;
        }
        if next < candidates.len() {
            *slot_fill = Some(next);
            used[next] = true;
            next += 1;
        }
    }

    // Build seated views in slot declaration order.
    let mut npcs = Vec::new();
    for (si, slot) in slots.iter().enumerate() {
        if let Some(ci) = filled[si] {
            let c = &candidates[ci];
            npcs.push(SceneNpcView {
                id: c.id,
                display_name: c.display_name.clone(),
                real_name: c.real_name.clone(),
                introduced: c.introduced,
                mood_emoji: c.mood_emoji.clone(),
                sprite_url: c.sprite_url.clone(),
                x: slot.x,
                y: slot.y,
                scale: slot.scale,
                flip: slot.x > 50.0,
            });
        }
    }

    // Overflow: any candidate never seated, in id order.
    let overflow = candidates
        .iter()
        .enumerate()
        .filter(|(i, _)| !used[*i])
        .map(|(_, c)| c.display_name.clone())
        .collect();

    (npcs, overflow)
}

/// Builds the renderable scene for the player's current location, or `None`
/// when the diorama flag is off or no scene is declared for the location.
///
/// The only runtime-specific concern is `asset_url`: it maps an asset's
/// mod-relative path (`assets/scenes/...`) to a URL the frontend can load.
/// Returning `None` from it for the plate collapses the whole scene to the
/// text fallback (a scene with no renderable plate is not useful); for a
/// sprite it drops that NPC to `overflow_npcs` (present but undrawable).
pub fn build_scene_state(
    world: &WorldState,
    npcs: &NpcManager,
    scenes: &SceneIndex,
    flags: &FeatureFlags,
    asset_url: &dyn Fn(&str) -> Option<String>,
) -> Option<SceneState> {
    if !flags.is_enabled(DIORAMA_FLAG) {
        return None;
    }

    let location_id = world.player_location.0;
    let scene = scenes.scene_for(location_id)?;

    let indoor = world
        .current_location_data()
        .map(|d| d.indoor)
        .unwrap_or(false);

    let variant = select_variant(scene, world.clock.time_of_day(), world.weather, indoor);
    let plate_rel = if variant == "day" {
        scene.plate.as_str()
    } else {
        // select_variant only returns a key that exists in `variants`.
        scene
            .variants
            .get(&variant)
            .map(String::as_str)
            .unwrap_or(scene.plate.as_str())
    };
    let plate_url = asset_url(plate_rel)?;

    let hotspots = scene
        .hotspots
        .iter()
        .map(|h| SceneHotspotView {
            id: h.id.clone(),
            shape: h.shape.clone(),
            label: h.label.clone(),
            action: h.action.clone(),
        })
        .collect();

    // Present NPCs, sorted by id for deterministic seating. Resolve each
    // sprite up front; a present NPC with no resolvable sprite is named in
    // overflow rather than seated (it cannot be drawn).
    let mut present = npcs.npcs_at(world.player_location);
    present.sort_by_key(|n| n.id.0);

    let mut candidates = Vec::new();
    let mut undrawable = Vec::new();
    for npc in &present {
        let introduced = npcs.is_introduced(npc.id);
        let display_name = npcs.display_name(npc).to_string();
        let sprite_url = scenes.sprite_for(npc.id.0).and_then(asset_url);
        match sprite_url {
            Some(url) => candidates.push(SeatCandidate {
                id: npc.id.0,
                display_name,
                real_name: introduced.then(|| npc.name.clone()),
                introduced,
                mood_emoji: mood_emoji(&npc.mood).to_string(),
                sprite_url: url,
            }),
            None => undrawable.push(display_name),
        }
    }

    let (seated, mut overflow) = assign_slots(&scene.slots, &candidates);
    overflow.extend(undrawable);

    Some(SceneState {
        schema_version: SCENE_STATE_SCHEMA_VERSION,
        location_id,
        plate_url,
        variant,
        indoor,
        hotspots,
        npcs: seated,
        overflow_npcs: overflow,
    })
}

/// Renders a human-readable scene summary for the `/scene` debug command,
/// shared by the headless REPL and the script harness (rule #12 — one
/// definition, both backends). Uses an identity `asset_url` so the printed
/// plate/sprite paths are the mod-relative paths, which is what a debugging
/// author wants to see. Returns a message (never `None`) so the command always
/// prints something explanatory.
pub fn render_scene_debug(
    world: &WorldState,
    npcs: &NpcManager,
    scenes: Option<&SceneIndex>,
    flags: &FeatureFlags,
) -> String {
    let Some(scenes) = scenes else {
        return "/scene: the active mod declares no scenes.".to_string();
    };
    let identity = |rel: &str| Some(rel.to_string());
    match build_scene_state(world, npcs, scenes, flags, &identity) {
        None => {
            if !flags.is_enabled(DIORAMA_FLAG) {
                "/scene: the diorama flag is off (enable with `/flag enable diorama`).".to_string()
            } else {
                "/scene: no scene declared for the current location (text fallback).".to_string()
            }
        }
        Some(s) => {
            let mut lines = vec![format!(
                "/scene location_id={} variant={} indoor={} plate={}",
                s.location_id, s.variant, s.indoor, s.plate_url
            )];
            lines.push(format!("  hotspots: {}", s.hotspots.len()));
            for n in &s.npcs {
                lines.push(format!(
                    "  slot: npc#{} \"{}\" introduced={} x={} y={} flip={}",
                    n.id, n.display_name, n.introduced, n.x, n.y, n.flip
                ));
            }
            if !s.overflow_npcs.is_empty() {
                lines.push(format!("  overflow: {}", s.overflow_npcs.join(", ")));
            }
            lines.join("\n")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_mod::scenes::{Hotspot, SceneDef};
    use std::collections::HashMap;

    fn slot(id: &str, x: f32, prefer: Option<u32>) -> NpcSlot {
        NpcSlot {
            id: id.to_string(),
            x,
            y: 60.0,
            scale: 1.0,
            prefer_npc: prefer,
        }
    }

    fn candidate(id: u32) -> SeatCandidate {
        SeatCandidate {
            id,
            display_name: format!("npc-{id}"),
            real_name: None,
            introduced: false,
            mood_emoji: "🙂".to_string(),
            sprite_url: format!("/asset/{id}.png"),
        }
    }

    fn scene_with_variants(keys: &[&str]) -> SceneDef {
        let mut variants = HashMap::new();
        for k in keys {
            variants.insert(k.to_string(), format!("assets/scenes/plate_{k}.png"));
        }
        SceneDef {
            location_id: 1,
            slug: "test".to_string(),
            plate: "assets/scenes/plate.png".to_string(),
            variants,
            hotspots: vec![],
            slots: vec![],
        }
    }

    // ── select_variant ──────────────────────────────────────────────────────

    #[test]
    fn variant_defaults_to_day() {
        let s = scene_with_variants(&[]);
        assert_eq!(
            select_variant(&s, TimeOfDay::Midday, Weather::Clear, false),
            "day"
        );
    }

    #[test]
    fn variant_night_in_evening_when_present() {
        let s = scene_with_variants(&["night"]);
        for tod in [TimeOfDay::Dusk, TimeOfDay::Night, TimeOfDay::Midnight] {
            assert_eq!(select_variant(&s, tod, Weather::Clear, false), "night");
        }
        // Daytime keeps the base plate even when a night variant exists.
        assert_eq!(
            select_variant(&s, TimeOfDay::Morning, Weather::Clear, false),
            "day"
        );
    }

    #[test]
    fn variant_night_missing_falls_back_to_day() {
        let s = scene_with_variants(&[]);
        assert_eq!(
            select_variant(&s, TimeOfDay::Night, Weather::Clear, false),
            "day"
        );
    }

    #[test]
    fn weather_variant_outdoors_only() {
        let s = scene_with_variants(&["rain", "night"]);
        // Outdoors + rainy → weather variant.
        assert_eq!(
            select_variant(&s, TimeOfDay::Midday, Weather::HeavyRain, false),
            "rain"
        );
        // Indoors + rainy → weather suppressed, falls through to time-of-day.
        assert_eq!(
            select_variant(&s, TimeOfDay::Midday, Weather::HeavyRain, true),
            "day"
        );
        assert_eq!(
            select_variant(&s, TimeOfDay::Night, Weather::HeavyRain, true),
            "night"
        );
    }

    // ── assign_slots determinism ────────────────────────────────────────────

    #[test]
    fn slots_prefer_npc_pinned_first() {
        let slots = vec![
            slot("a", 20.0, None),
            slot("bar", 50.0, Some(7)),
            slot("c", 80.0, None),
        ];
        let cands = vec![candidate(3), candidate(7), candidate(9)];
        let (seated, overflow) = assign_slots(&slots, &cands);
        assert!(overflow.is_empty());
        // npc 7 pinned to the "bar" slot regardless of id order.
        assert_eq!(seated.iter().find(|n| n.id == 7).unwrap().x, 50.0);
        // remaining (3, 9) fill a and c in id order.
        let by_slot: Vec<u32> = seated.iter().map(|n| n.id).collect();
        // declaration order: a(20)->3, bar(50)->7, c(80)->9
        assert_eq!(by_slot, vec![3, 7, 9]);
    }

    #[test]
    fn slots_overflow_when_more_npcs_than_slots() {
        let slots = vec![slot("a", 20.0, None)];
        let cands = vec![candidate(1), candidate(2), candidate(3)];
        let (seated, overflow) = assign_slots(&slots, &cands);
        assert_eq!(seated.len(), 1);
        assert_eq!(seated[0].id, 1); // lowest id seats first
        assert_eq!(overflow, vec!["npc-2".to_string(), "npc-3".to_string()]);
    }

    #[test]
    fn slots_assignment_is_deterministic() {
        let slots = vec![slot("a", 20.0, Some(2)), slot("b", 80.0, None)];
        let cands = vec![candidate(1), candidate(2), candidate(3)];
        let a = assign_slots(&slots, &cands);
        let b = assign_slots(&slots, &cands);
        assert_eq!(a.0, b.0);
        assert_eq!(a.1, b.1);
    }

    #[test]
    fn flip_set_for_right_half() {
        let slots = vec![slot("left", 20.0, None), slot("right", 80.0, None)];
        let cands = vec![candidate(1), candidate(2)];
        let (seated, _) = assign_slots(&slots, &cands);
        assert!(!seated[0].flip); // x=20 → no flip
        assert!(seated[1].flip); // x=80 → flip
    }

    #[test]
    fn empty_slots_seats_nobody_but_keeps_overflow() {
        let cands = vec![candidate(1), candidate(2)];
        let (seated, overflow) = assign_slots(&[], &cands);
        assert!(seated.is_empty());
        assert_eq!(overflow.len(), 2);
    }

    // ── build_scene_state gating ────────────────────────────────────────────

    fn index_for_location_1() -> SceneIndex {
        let mut scene = scene_with_variants(&[]);
        scene.hotspots = vec![Hotspot {
            id: "door".to_string(),
            shape: HotspotShape::Rect([10.0, 10.0, 5.0, 5.0]),
            label: "Door".to_string(),
            action: HotspotAction::TravelTo(2),
        }];
        SceneIndex {
            scenes: vec![scene],
            sprites: vec![],
            fallback_sprites: None,
        }
    }

    #[test]
    fn flag_off_returns_none() {
        let world = WorldState::new();
        let npcs = NpcManager::new();
        let index = index_for_location_1();
        let flags = FeatureFlags::default(); // diorama not enabled
        let url = |rel: &str| Some(format!("/asset/{rel}"));
        assert!(build_scene_state(&world, &npcs, &index, &flags, &url).is_none());
    }

    #[test]
    fn flag_on_unplated_location_returns_none() {
        let world = WorldState::new(); // player at The Crossroads (id 1)
        let npcs = NpcManager::new();
        // Index has only a scene for a *different* location.
        let mut scene = scene_with_variants(&[]);
        scene.location_id = 999;
        let index = SceneIndex {
            scenes: vec![scene],
            sprites: vec![],
            fallback_sprites: None,
        };
        let mut flags = FeatureFlags::default();
        flags.enable(DIORAMA_FLAG);
        let url = |rel: &str| Some(format!("/asset/{rel}"));
        assert!(build_scene_state(&world, &npcs, &index, &flags, &url).is_none());
    }

    #[test]
    fn flag_on_plated_location_returns_scene() {
        let world = WorldState::new(); // The Crossroads, id 1, outdoor
        let npcs = NpcManager::new();
        let index = index_for_location_1();
        let mut flags = FeatureFlags::default();
        flags.enable(DIORAMA_FLAG);
        let url = |rel: &str| Some(format!("/asset/{rel}"));
        let state = build_scene_state(&world, &npcs, &index, &flags, &url).unwrap();
        assert_eq!(state.location_id, 1);
        assert_eq!(state.variant, "day");
        assert!(!state.indoor);
        assert_eq!(state.plate_url, "/asset/assets/scenes/plate.png");
        assert_eq!(state.hotspots.len(), 1);
        assert_eq!(state.hotspots[0].action, HotspotAction::TravelTo(2));
        assert!(state.npcs.is_empty()); // no NPCs in a bare world
        assert_eq!(state.schema_version, SCENE_STATE_SCHEMA_VERSION);
    }

    #[test]
    fn render_debug_reports_flag_off_then_scene() {
        let world = WorldState::new();
        let npcs = NpcManager::new();
        let index = index_for_location_1();
        let mut flags = FeatureFlags::default();
        // Flag off → explanatory message.
        let off = render_scene_debug(&world, &npcs, Some(&index), &flags);
        assert!(off.contains("diorama flag is off"), "got: {off}");
        // Flag on + plated → scene summary with location + variant.
        flags.enable(DIORAMA_FLAG);
        let on = render_scene_debug(&world, &npcs, Some(&index), &flags);
        assert!(on.contains("location_id=1"), "got: {on}");
        assert!(on.contains("variant=day"), "got: {on}");
        // No scenes at all → distinct message.
        let none = render_scene_debug(&world, &npcs, None, &flags);
        assert!(none.contains("no scenes"), "got: {none}");
    }

    #[test]
    fn plate_unresolvable_collapses_to_none() {
        let world = WorldState::new();
        let npcs = NpcManager::new();
        let index = index_for_location_1();
        let mut flags = FeatureFlags::default();
        flags.enable(DIORAMA_FLAG);
        let url = |_rel: &str| None; // nothing resolves
        assert!(build_scene_state(&world, &npcs, &index, &flags, &url).is_none());
    }
}
