//! Pure handler functions that build IPC types from game state.
//!
//! These are consumed by both the Tauri desktop backend and the axum web
//! server, keeping game-logic → IPC-type mapping in a single place.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{Datelike, Timelike};

use crate::game_mod::PronunciationEntry;
use crate::npc::anachronism;
use crate::npc::manager::NpcManager;
use crate::npc::mood::mood_emoji;
use crate::npc::ticks;
use crate::npc::{LanguageHint, Npc, NpcId};
use crate::world::description::render_description;
use crate::world::palette::compute_palette;
use crate::world::transport::TransportMode;
use crate::world::{LocationId, WorldState};

use super::types::{MapData, MapLocation, NpcInfo, TextLogPayload, ThemePalette, WorldSnapshot};

/// Builds a [`WorldSnapshot`] from the current world state.
pub fn snapshot_from_world(world: &WorldState, _transport: &TransportMode) -> WorldSnapshot {
    let now = world.clock.now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let tod = world.clock.time_of_day();
    let season = world.clock.season();
    let festival = world.clock.check_festival().map(|f| f.to_string());
    let weather_str = world.weather.to_string();

    let loc = world.current_location();
    let description = if let Some(data) = world.current_location_data() {
        render_description(data, tod, &weather_str, &[])
    } else {
        loc.description.clone()
    };

    let day_of_week = match now.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string();

    WorldSnapshot {
        location_name: loc.name.clone(),
        location_description: description,
        time_label: tod.to_string(),
        hour,
        minute,
        weather: weather_str,
        season: season.to_string(),
        festival,
        paused: world.clock.is_paused() || world.clock.is_inference_paused(),
        game_epoch_ms: now.timestamp_millis() as f64,
        speed_factor: world.clock.speed_factor(),
        name_hints: vec![],
        day_of_week,
    }
}

/// Builds the [`MapData`] with fog-of-war: visited locations plus the frontier.
///
/// Visited locations are fully enriched. The "frontier" — unvisited locations
/// adjacent to any visited location — also appears so the player can see
/// where they could explore next. Frontier locations are marked with
/// `visited: false` and have limited tooltip data.
pub fn build_map_data(world: &WorldState, transport: &TransportMode) -> MapData {
    let speed_m_per_s = transport.speed_m_per_s;
    let player_loc = world.player_location;
    let visited = &world.visited_locations;

    let adjacent_ids: HashSet<LocationId> = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let hop_map = world.graph.hop_distances(player_loc);

    // Frontier: unvisited locations that neighbor at least one visited location
    let mut frontier: HashSet<LocationId> = HashSet::new();
    for &v in visited {
        for (neighbor_id, _) in world.graph.neighbors(v) {
            if !visited.contains(&neighbor_id) {
                frontier.insert(neighbor_id);
            }
        }
    }

    // Build visited locations (fully enriched)
    let mut locations: Vec<MapLocation> = world
        .graph
        .location_ids()
        .into_iter()
        .filter(|id| visited.contains(id))
        .filter_map(|id| world.graph.get(id).map(|data| (id, data)))
        .map(|(id, data)| {
            let travel_minutes = if id == player_loc {
                None
            } else {
                world
                    .graph
                    .shortest_path(player_loc, id)
                    .map(|path| world.graph.path_travel_time(&path, speed_m_per_s))
            };

            MapLocation {
                id: id.0.to_string(),
                name: data.name.clone(),
                lat: data.lat,
                lon: data.lon,
                adjacent: adjacent_ids.contains(&id) || id == player_loc,
                hops: *hop_map.get(&id).unwrap_or(&u32::MAX),
                indoor: Some(data.indoor),
                travel_minutes,
                visited: true,
            }
        })
        .collect();

    // Append frontier locations (limited info)
    for id in &frontier {
        if let Some(data) = world.graph.get(*id) {
            locations.push(MapLocation {
                id: id.0.to_string(),
                name: data.name.clone(),
                lat: data.lat,
                lon: data.lon,
                adjacent: adjacent_ids.contains(id),
                hops: *hop_map.get(id).unwrap_or(&u32::MAX),
                indoor: None,
                travel_minutes: None,
                visited: false,
            });
        }
    }

    // Edges: between any two locations that are both visible (visited or frontier)
    let visible: HashSet<LocationId> = visited.union(&frontier).copied().collect();
    let mut edges: Vec<(String, String)> = Vec::new();
    for loc_id in world.graph.location_ids() {
        if !visible.contains(&loc_id) {
            continue;
        }
        for (neighbor_id, _conn) in world.graph.neighbors(loc_id) {
            if loc_id.0 < neighbor_id.0 && visible.contains(&neighbor_id) {
                edges.push((loc_id.0.to_string(), neighbor_id.0.to_string()));
            }
        }
    }

    // Edge traversal counts for footprint rendering
    let edge_traversals: Vec<(String, String, u32)> = world
        .edge_traversals
        .iter()
        .filter(|((a, b), _)| visible.contains(a) && visible.contains(b))
        .map(|((a, b), count)| (a.0.to_string(), b.0.to_string(), *count))
        .collect();

    MapData {
        locations,
        edges,
        player_location: player_loc.0.to_string(),
        edge_traversals,
        transport_label: transport.label.clone(),
        transport_id: transport.id.clone(),
    }
}

/// Builds a [`TravelStartPayload`] from a movement path.
///
/// Extracts lat/lon coordinates from the world graph for each waypoint
/// so the frontend can animate the player's travel along the path.
pub fn build_travel_start(
    path: &[crate::world::LocationId],
    minutes: u16,
    graph: &crate::world::graph::WorldGraph,
) -> super::types::TravelStartPayload {
    let waypoints = path
        .iter()
        .filter_map(|id| {
            graph.get(*id).map(|data| super::types::TravelWaypoint {
                id: id.0.to_string(),
                lat: data.lat,
                lon: data.lon,
            })
        })
        .collect();

    super::types::TravelStartPayload {
        waypoints,
        duration_minutes: minutes,
        destination: path.last().map(|id| id.0.to_string()).unwrap_or_default(),
    }
}

/// Builds the list of [`NpcInfo`] for NPCs at the player's current location.
pub fn build_npcs_here(world: &WorldState, npc_manager: &NpcManager) -> Vec<NpcInfo> {
    let npcs = npc_manager.npcs_at(world.player_location);
    npcs.into_iter()
        .map(|npc| {
            let introduced = npc_manager.is_introduced(npc.id);
            NpcInfo {
                name: npc_manager.display_name(npc).to_string(),
                occupation: npc.occupation.clone(),
                mood_emoji: mood_emoji(&npc.mood).to_string(),
                mood: npc.mood.clone(),
                introduced,
            }
        })
        .collect()
}

/// Builds the current [`ThemePalette`] from the world clock and weather.
pub fn build_theme(world: &WorldState) -> ThemePalette {
    let now = world.clock.now();
    let raw = compute_palette(
        now.hour(),
        now.minute(),
        world.clock.season(),
        world.weather,
    );
    ThemePalette::from(raw)
}

/// Capitalizes the first character of a string slice.
pub fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

/// Monotonically increasing message ID counter for text-log entries.
static MESSAGE_ID: AtomicU64 = AtomicU64::new(1);

/// Creates a [`TextLogPayload`] with an auto-generated unique message ID.
pub fn text_log(source: impl Into<String>, content: impl Into<String>) -> TextLogPayload {
    TextLogPayload {
        id: format!("msg-{}", MESSAGE_ID.fetch_add(1, Ordering::SeqCst)),
        source: source.into(),
        content: content.into(),
    }
}

/// Irish-themed canned messages shown when NPC inference fails.
///
/// Indexed by `request_id % len` so different attempts get different messages.
pub const INFERENCE_FAILURE_MESSAGES: &[&str] = &[
    "A sudden fog rolls in and swallows the conversation whole.",
    "A crow lands between you, caws loudly, and the moment is lost.",
    "The wind picks up and carries their words clean away.",
    "They open their mouth to speak, but a donkey brays so loud neither of ye can hear a thing.",
    "A clap of thunder rattles the sky and ye both forget what ye were talking about.",
    "They stare at you blankly, as if the thought simply left their head.",
    "A strange silence falls over the parish. Even the birds have stopped.",
];

/// Atmospheric messages displayed when no NPC is present at the current location.
pub const IDLE_MESSAGES: &[&str] = &[
    "The wind stirs, but nothing else.",
    "Only the sound of a distant crow.",
    "A dog barks somewhere beyond the hill.",
    "The clouds shift. The parish carries on.",
    "Somewhere nearby, a door creaks shut.",
    "A wren hops along the stone wall and vanishes.",
    "The smell of turf smoke drifts from a cottage chimney.",
];

/// Helper to mask an API key for display (shows first 4 and last 4 chars).
pub fn mask_key(key: &str) -> String {
    if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "(set, too short to mask)".to_string()
    }
}

// ── NPC conversation setup ──────────────────────────────────────────────────

/// Data needed to start an NPC conversation, returned by [`prepare_npc_conversation`].
#[derive(Debug, Clone)]
pub struct NpcConversationSetup {
    /// Display name of the NPC (for UI labels).
    pub display_name: String,
    /// NPC's unique ID.
    pub npc_id: NpcId,
    /// The assembled system prompt for the LLM.
    pub system_prompt: String,
    /// The assembled context string for the LLM.
    pub context: String,
}

/// Prepares an NPC conversation: finds the NPC, builds prompts, and marks them
/// as introduced. All backends call this before submitting to the inference queue.
///
/// If `target_name` is provided (from an `@mention`), the matching NPC is
/// selected; otherwise falls back to the first NPC at the player's location.
///
/// Returns `None` if no NPC is present at the player's location.
pub fn prepare_npc_conversation(
    world: &WorldState,
    npc_manager: &mut NpcManager,
    raw: &str,
    target_name: Option<&str>,
    improv_enabled: bool,
) -> Option<NpcConversationSetup> {
    let npcs_here = npc_manager.npcs_at(world.player_location);
    if npcs_here.is_empty() {
        return None;
    }

    // If an @mention was provided, try to find that NPC; otherwise first NPC
    let npc: Option<Npc> = if let Some(name) = target_name {
        npc_manager
            .find_by_name(name, world.player_location)
            .cloned()
            .or_else(|| npcs_here.first().cloned().cloned())
    } else {
        npcs_here.first().cloned().cloned()
    };

    let npc = npc?;
    let display_name = npc_manager.display_name(&npc).to_string();
    let npc_id = npc.id;

    let other_npcs: Vec<&Npc> = npcs_here.into_iter().filter(|n| n.id != npc.id).collect();
    let system_prompt = ticks::build_enhanced_system_prompt(&npc, improv_enabled);
    let mut context = ticks::build_enhanced_context(&npc, world, raw, &other_npcs);

    // Check for anachronisms in player input and inject alert into context
    let anachronisms = anachronism::check_input(raw);
    if let Some(alert) = anachronism::format_context_alert(&anachronisms) {
        context.push_str(&alert);
    }

    // Mark NPC as introduced on first conversation
    npc_manager.mark_introduced(npc_id);

    Some(NpcConversationSetup {
        display_name,
        npc_id,
        system_prompt,
        context,
    })
}

// ── Pronunciation hints ────────────────────────────────────────────────────

/// Computes contextual name pronunciation hints for the current location.
///
/// Matches pronunciation entries against the current location name and
/// any introduced NPC names present at the player's location.
pub fn compute_name_hints(
    world: &WorldState,
    npc_manager: &NpcManager,
    pronunciations: &[PronunciationEntry],
) -> Vec<LanguageHint> {
    if pronunciations.is_empty() {
        tracing::debug!("compute_name_hints: no pronunciation entries loaded");
        return vec![];
    }
    let loc = world.current_location();
    let mut names: Vec<&str> = vec![&loc.name];
    let npcs = npc_manager.npcs_at(world.player_location);
    let npc_names: Vec<String> = npcs
        .iter()
        .filter(|n| npc_manager.is_introduced(n.id))
        .map(|n| n.name.clone())
        .collect();
    for name in &npc_names {
        names.push(name);
    }
    let hints: Vec<LanguageHint> = pronunciations
        .iter()
        .filter(|entry| entry.matches_any(&names))
        .map(|entry| entry.to_hint())
        .collect();
    tracing::debug!(
        location = %loc.name,
        npc_names = ?npc_names,
        pronunciation_count = pronunciations.len(),
        matched_hints = hints.len(),
        "compute_name_hints"
    );
    hints
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capitalize_first_works() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("ABC"), "ABC");
    }

    #[test]
    fn mask_key_works() {
        assert_eq!(mask_key("abcdefghij"), "abcd...ghij");
        assert_eq!(mask_key("short"), "(set, too short to mask)");
        assert_eq!(mask_key("123456789"), "1234...6789");
    }

    #[test]
    fn snapshot_from_default_world() {
        let world = WorldState::new();
        let transport = TransportMode::walking();
        let snap = snapshot_from_world(&world, &transport);
        assert!(!snap.location_name.is_empty());
        assert!(snap.hour <= 23);
        assert!(snap.minute <= 59);
        assert!(snap.speed_factor > 0.0);
    }

    #[test]
    fn build_map_data_from_default_world() {
        let world = WorldState::new();
        let map = build_map_data(&world, &TransportMode::walking());
        assert!(!map.player_location.is_empty());
        // At least the player's location should exist
        assert!(
            map.locations.iter().any(|l| l.id == map.player_location) || map.locations.is_empty()
        );
    }

    #[test]
    fn fog_of_war_shows_frontier() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let world = WorldState::from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            let neighbor_count = world.graph.neighbors(start).len();

            let map = build_map_data(&world, &TransportMode::walking());

            // Start location (visited) + its neighbors (frontier)
            assert_eq!(
                map.locations.len(),
                1 + neighbor_count,
                "should show start + frontier neighbors"
            );

            // The start location is visited
            let start_loc = map
                .locations
                .iter()
                .find(|l| l.id == map.player_location)
                .unwrap();
            assert!(start_loc.visited);
            assert!(start_loc.indoor.is_some());
            assert!(start_loc.travel_minutes.is_none());

            // Frontier locations are not visited and have limited info
            let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();
            assert_eq!(frontier.len(), neighbor_count);
            for f in &frontier {
                assert!(f.indoor.is_none(), "frontier should not reveal indoor flag");
                assert!(
                    f.travel_minutes.is_none(),
                    "frontier should not reveal travel time"
                );
            }

            // Edges should connect start to each frontier neighbor
            assert_eq!(map.edges.len(), neighbor_count);
        }
    }

    #[test]
    fn fog_of_war_reveals_after_visit() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let mut world = WorldState::from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            // Visit a neighbor
            let neighbors = world.graph.neighbors(start);
            if let Some((neighbor_id, _)) = neighbors.first() {
                world.mark_visited(*neighbor_id);
                let map = build_map_data(&world, &TransportMode::walking());

                // Visited locations should have visited=true
                let visited: Vec<_> = map.locations.iter().filter(|l| l.visited).collect();
                assert_eq!(visited.len(), 2);

                // The non-player visited location should have travel_minutes
                let other = visited
                    .iter()
                    .find(|l| l.id != map.player_location)
                    .unwrap();
                assert!(other.travel_minutes.is_some());
                assert!(other.indoor.is_some());

                // Frontier locations exist for unvisited neighbors of both visited locs
                let frontier: Vec<_> = map.locations.iter().filter(|l| !l.visited).collect();
                assert!(
                    !frontier.is_empty() || map.locations.len() == 2,
                    "frontier should appear unless all neighbors are visited"
                );
            }
        }
    }

    #[test]
    fn build_npcs_here_empty_manager() {
        let world = WorldState::new();
        let npc_mgr = NpcManager::new();
        let npcs = build_npcs_here(&world, &npc_mgr);
        assert!(npcs.is_empty());
    }

    #[test]
    fn build_theme_returns_hex_colors() {
        let world = WorldState::new();
        let theme = build_theme(&world);
        assert!(theme.bg.starts_with('#'));
        assert_eq!(theme.bg.len(), 7);
        assert!(theme.fg.starts_with('#'));
    }

    #[test]
    fn build_travel_start_basic() {
        use crate::world::graph::WorldGraph;

        let json = r#"{"locations": [
            {"id": 1, "name": "A", "description_template": ".", "indoor": false, "public": true, "lat": 53.6, "lon": -8.1, "connections": [{"target": 2, "path_description": "road"}]},
            {"id": 2, "name": "B", "description_template": ".", "indoor": false, "public": true, "lat": 53.61, "lon": -8.09, "connections": [{"target": 1, "path_description": "back"}]}
        ]}"#;
        let graph = WorldGraph::load_from_str(json).unwrap();
        let path = vec![LocationId(1), LocationId(2)];
        let payload = build_travel_start(&path, 5, &graph);
        assert_eq!(payload.waypoints.len(), 2);
        assert_eq!(payload.waypoints[0].id, "1");
        assert_eq!(payload.waypoints[1].id, "2");
        assert_eq!(payload.duration_minutes, 5);
        assert_eq!(payload.destination, "2");
        assert!((payload.waypoints[0].lat - 53.6).abs() < 0.001);
    }

    #[test]
    fn build_map_data_includes_edge_traversals() {
        use crate::game_mod::{GameMod, find_default_mod};

        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let mut world = WorldState::from_mod(&game_mod).expect("world from mod");
            let start = world.player_location;
            let neighbor_id = world.graph.neighbors(start).first().map(|(id, _)| *id);
            if let Some(neighbor_id) = neighbor_id {
                // Traverse the edge twice
                world.record_path_traversal(&[start, neighbor_id]);
                world.record_path_traversal(&[start, neighbor_id]);
                world.mark_visited(neighbor_id);

                let map = build_map_data(&world, &TransportMode::walking());
                assert!(
                    !map.edge_traversals.is_empty(),
                    "should include edge traversals"
                );
                // Find the traversal for start<->neighbor
                let start_str = start.0.to_string();
                let neighbor_str = neighbor_id.0.to_string();
                let found = map.edge_traversals.iter().any(|(a, b, count)| {
                    ((a == &start_str && b == &neighbor_str)
                        || (a == &neighbor_str && b == &start_str))
                        && *count == 2
                });
                assert!(found, "should find traversal count of 2");
            }
        }
    }
}
