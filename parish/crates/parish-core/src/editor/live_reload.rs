//! Live world hot-reload helpers for the Parish Designer.
//!
//! The editor itself stays isolated from gameplay, but after a successful
//! save the host application may choose to refresh its in-memory world graph
//! from disk so map/location edits are immediately visible in the running game.

use crate::game_mod::{GameMod, world_state_from_mod};
use crate::world::WorldState;
use parish_types::ParishError;

/// Replaces the live world's graph/location data with a freshly-loaded copy
/// while preserving runtime progress such as clock, weather, visited nodes,
/// footprints, conversation history, and the current player location when
/// it still exists in the edited graph.
pub fn reload_world_graph_preserving_runtime(
    world: &mut WorldState,
    game_mod: &GameMod,
) -> Result<(), ParishError> {
    let fresh_world = world_state_from_mod(game_mod)?;
    apply_world_graph_refresh(world, fresh_world);
    Ok(())
}

fn apply_world_graph_refresh(world: &mut WorldState, fresh_world: WorldState) {
    let fallback_player_location = fresh_world.player_location;
    world.graph = fresh_world.graph;
    world.locations = fresh_world.locations;

    if !world.locations.contains_key(&world.player_location) {
        world.player_location = fallback_player_location;
    }

    world
        .visited_locations
        .retain(|id| world.locations.contains_key(id));
    world.visited_locations.insert(world.player_location);

    world
        .edge_traversals
        .retain(|(a, b), _| world.locations.contains_key(a) && world.locations.contains_key(b));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::LocationId;
    use std::fs;
    use tempfile::TempDir;

    fn write_mod(root: &std::path::Path, world_json: &str) {
        fs::create_dir_all(root.join("prompts")).unwrap();
        fs::write(root.join("world.json"), world_json).unwrap();
        fs::write(root.join("npcs.json"), "[]").unwrap();
        fs::write(root.join("encounters.json"), "{}").unwrap();
        fs::write(root.join("festivals.json"), "[]").unwrap();
        fs::write(
            root.join("loading.toml"),
            r#"
spinner_frames = ["|", "/", "-", "\\"]
spinner_colors = [[200, 180, 100], [100, 200, 100]]
phrases = ["Loading...", "Please wait..."]
"#,
        )
        .unwrap();
        fs::write(
            root.join("ui.toml"),
            r##"
[sidebar]
hints_label = "Focail"

[theme.palette]
accent = "#aabbcc"
"##,
        )
        .unwrap();
        fs::write(
            root.join("anachronisms.json"),
            r#"{ "context_alert_prefix": "", "context_alert_suffix": "", "terms": [] }"#,
        )
        .unwrap();
        fs::write(root.join("pronunciations.json"), r#"{ "names": [] }"#).unwrap();
        fs::write(root.join("prompts/tier1_system.txt"), "system").unwrap();
        fs::write(root.join("prompts/tier1_context.txt"), "context").unwrap();
        fs::write(root.join("prompts/tier2_system.txt"), "system").unwrap();
        fs::write(root.join("prompts/tier2_context.txt"), "context").unwrap();
        fs::write(root.join("prompts/tier3_system.txt"), "system").unwrap();
        fs::write(root.join("prompts/tier3_context.txt"), "context").unwrap();
        fs::write(root.join("prompts/tier4_system.txt"), "system").unwrap();
        fs::write(root.join("prompts/tier4_context.txt"), "context").unwrap();
        fs::write(
            root.join("mod.toml"),
            r#"
[mod]
id = "test"
name = "Test"
title = "Test"
version = "0.1.0"
description = "Test mod"

[setting]
start_date = "1822-01-01T08:00:00Z"
start_location = 1
period_year = 1822

[files]
world = "world.json"
npcs = "npcs.json"
encounters = "encounters.json"
festivals = "festivals.json"
anachronisms = "anachronisms.json"
pronunciations = "pronunciations.json"
loading = "loading.toml"
ui = "ui.toml"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
        )
        .unwrap();
    }

    fn initial_world_json() -> &'static str {
        r#"
{
  "locations": [
    {
      "id": 1,
      "name": "Start",
      "description_template": "Start at {time}.",
      "indoor": false,
      "public": true,
      "connections": [
        { "target": 2, "path_description": "lane" }
      ],
      "lat": 53.5,
      "lon": -8.1,
      "associated_npcs": [],
      "aliases": []
    },
    {
      "id": 2,
      "name": "Old Mill",
      "description_template": "Mill at {time}.",
      "indoor": false,
      "public": true,
      "connections": [
        { "target": 1, "path_description": "lane" }
      ],
      "lat": 53.51,
      "lon": -8.11,
      "associated_npcs": [],
      "aliases": []
    }
  ]
}
"#
    }

    fn updated_world_json() -> &'static str {
        r#"
{
  "locations": [
    {
      "id": 1,
      "name": "Start",
      "description_template": "Start at {time}.",
      "indoor": false,
      "public": true,
      "connections": [
        { "target": 3, "path_description": "road" }
      ],
      "lat": 53.5,
      "lon": -8.1,
      "associated_npcs": [],
      "aliases": []
    },
    {
      "id": 3,
      "name": "New Chapel",
      "description_template": "Chapel at {time}.",
      "indoor": true,
      "public": true,
      "connections": [
        { "target": 1, "path_description": "road" }
      ],
      "lat": 53.52,
      "lon": -8.12,
      "associated_npcs": [],
      "aliases": []
    }
  ]
}
"#
    }

    #[test]
    fn reload_world_graph_preserves_runtime_and_prunes_removed_locations() {
        let dir = TempDir::new().unwrap();
        write_mod(dir.path(), initial_world_json());
        let game_mod = GameMod::load(dir.path()).unwrap();
        let mut world = world_state_from_mod(&game_mod).unwrap();
        world.player_location = LocationId(2);
        world.visited_locations.insert(LocationId(2));
        world.visited_locations.insert(LocationId(999));
        world
            .edge_traversals
            .insert((LocationId(1), LocationId(2)), 3);
        world
            .edge_traversals
            .insert((LocationId(2), LocationId(999)), 1);

        write_mod(dir.path(), updated_world_json());
        let updated_mod = GameMod::load(dir.path()).unwrap();
        reload_world_graph_preserving_runtime(&mut world, &updated_mod).unwrap();

        assert_eq!(
            world.player_location,
            LocationId(1),
            "removed current location should fall back to the mod start location"
        );
        assert!(world.locations.contains_key(&LocationId(1)));
        assert!(world.locations.contains_key(&LocationId(3)));
        assert!(!world.locations.contains_key(&LocationId(2)));
        assert_eq!(world.current_location().name, "Start");
        assert!(world.visited_locations.contains(&LocationId(1)));
        assert!(!world.visited_locations.contains(&LocationId(2)));
        assert!(!world.visited_locations.contains(&LocationId(999)));
        assert!(
            world.edge_traversals.is_empty(),
            "footprints for removed edges should be discarded"
        );
    }
}
