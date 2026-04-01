//! Pure handler functions that build IPC types from game state.
//!
//! These are consumed by both the Tauri desktop backend and the axum web
//! server, keeping game-logic → IPC-type mapping in a single place.

use std::collections::HashSet;

use chrono::Timelike;

use crate::npc::manager::NpcManager;
use crate::world::description::{format_exits, render_description};
use crate::world::palette::compute_palette;
use crate::world::transport::TransportMode;
use crate::world::{LocationId, WorldState};

use super::types::{MapData, MapLocation, NpcInfo, ThemePalette, WorldSnapshot};

/// Builds a [`WorldSnapshot`] from the current world state.
pub fn snapshot_from_world(world: &WorldState, transport: &TransportMode) -> WorldSnapshot {
    let now = world.clock.now();
    let hour = now.hour() as u8;
    let minute = now.minute() as u8;
    let tod = world.clock.time_of_day();
    let season = world.clock.season();
    let festival = world.clock.check_festival().map(|f| f.to_string());
    let weather_str = world.weather.to_string();

    let loc = world.current_location();
    let description = if let Some(data) = world.current_location_data() {
        let desc = render_description(data, tod, &weather_str, &[]);
        let exits = format_exits(
            world.player_location,
            &world.graph,
            transport.speed_m_per_s,
            &transport.label,
        );
        format!("{}\n\n{}", desc, exits)
    } else {
        loc.description.clone()
    };

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
    }
}

/// Builds the [`MapData`] filtered by fog-of-war (only visited locations).
///
/// Only locations the player has visited are included. Edges are filtered
/// to only include connections between visited locations. Each location
/// is enriched with tooltip data (indoor flag, travel time).
pub fn build_map_data(world: &WorldState, speed_m_per_s: f64) -> MapData {
    let player_loc = world.player_location;
    let visited = &world.visited_locations;

    let adjacent_ids: HashSet<LocationId> = world
        .graph
        .neighbors(player_loc)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let hop_map = world.graph.hop_distances(player_loc);

    let locations: Vec<MapLocation> = world
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
            }
        })
        .collect();

    let mut edges: Vec<(String, String)> = Vec::new();
    for loc_id in world.graph.location_ids() {
        if !visited.contains(&loc_id) {
            continue;
        }
        for (neighbor_id, _conn) in world.graph.neighbors(loc_id) {
            if loc_id.0 < neighbor_id.0 && visited.contains(&neighbor_id) {
                edges.push((loc_id.0.to_string(), neighbor_id.0.to_string()));
            }
        }
    }

    MapData {
        locations,
        edges,
        player_location: player_loc.0.to_string(),
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

/// Helper to mask an API key for display (shows first 4 and last 4 chars).
pub fn mask_key(key: &str) -> String {
    if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "(set, too short to mask)".to_string()
    }
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
        let map = build_map_data(&world, 1.25);
        assert!(!map.player_location.is_empty());
        // At least the player's location should exist
        assert!(
            map.locations.iter().any(|l| l.id == map.player_location) || map.locations.is_empty()
        );
    }

    #[test]
    fn fog_of_war_only_sends_visited() {
        use crate::game_mod::{GameMod, find_default_mod};
        if let Some(mod_dir) = find_default_mod() {
            let game_mod = GameMod::load(&mod_dir).expect("should load default mod");
            let world = WorldState::from_mod(&game_mod).expect("world from mod");
            // Only the start location is visited initially
            let map = build_map_data(&world, 1.25);
            assert_eq!(
                map.locations.len(),
                1,
                "only start location should be visible"
            );
            assert_eq!(map.locations[0].id, map.player_location);
            assert!(
                map.edges.is_empty(),
                "no edges when only 1 visited location"
            );
            // Indoor field should be set
            assert!(map.locations[0].indoor.is_some());
            // Travel time to self should be None
            assert!(map.locations[0].travel_minutes.is_none());
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
                let map = build_map_data(&world, 1.25);
                assert_eq!(map.locations.len(), 2);
                assert!(
                    !map.edges.is_empty(),
                    "edge between visited pair should appear"
                );
                // The non-player location should have travel_minutes set
                let other = map
                    .locations
                    .iter()
                    .find(|l| l.id != map.player_location)
                    .unwrap();
                assert!(other.travel_minutes.is_some());
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
}
