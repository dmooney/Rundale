//! Integration tests for the world graph and parish data.
//!
//! These tests validate the actual `data/parish.json` file and test
//! end-to-end scenarios: loading, pathfinding, movement, encounters,
//! and dynamic descriptions.

use std::path::Path;

use parish::world::LocationId;
use parish::world::description::render_description;
use parish::world::encounter::check_encounter;
use parish::world::graph::WorldGraph;
use parish::world::movement::{MovementResult, resolve_movement};
use parish::world::time::TimeOfDay;
use parish::world::transport::TransportMode;

fn load_parish_graph() -> WorldGraph {
    let path = Path::new("../../mods/kilteevan-1820/world.json");
    WorldGraph::load_from_file(path)
        .expect("mods/kilteevan-1820/world.json should load and validate")
}

fn walking() -> TransportMode {
    TransportMode::walking()
}

#[test]
fn test_parish_json_loads() {
    let graph = load_parish_graph();
    assert!(
        graph.location_count() >= 12,
        "expected at least 12 locations, got {}",
        graph.location_count()
    );
}

#[test]
fn test_parish_json_location_count() {
    let graph = load_parish_graph();
    assert_eq!(graph.location_count(), 22);
}

#[test]
fn test_parish_crossroads_is_hub() {
    let graph = load_parish_graph();
    let neighbors = graph.neighbors(LocationId(1));
    assert!(
        neighbors.len() >= 4,
        "crossroads should have at least 4 connections, got {}",
        neighbors.len()
    );
}

#[test]
fn test_parish_all_connections_bidirectional() {
    // This is validated on load, but explicitly verify
    let graph = load_parish_graph();
    for loc_id in graph.location_ids() {
        for (neighbor_id, _) in graph.neighbors(loc_id) {
            let reverse = graph.neighbors(neighbor_id);
            let has_reverse = reverse.iter().any(|(id, _)| *id == loc_id);
            assert!(
                has_reverse,
                "connection from {:?} to {:?} is not bidirectional",
                loc_id, neighbor_id
            );
        }
    }
}

#[test]
fn test_parish_find_pub() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("pub").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "Darcy's Pub");
}

#[test]
fn test_parish_find_church() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("church").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "St. Brigid's Church");
}

#[test]
fn test_parish_find_fairy_fort() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("fairy").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "The Fairy Fort");
}

#[test]
fn test_parish_path_crossroads_to_pub() {
    let graph = load_parish_graph();
    let path = graph.shortest_path(LocationId(1), LocationId(2)).unwrap();
    assert_eq!(path, vec![LocationId(1), LocationId(2)]);
    let time = graph.path_travel_time(&path, 1.25);
    assert!(
        time >= 1 && time <= 10,
        "Crossroads→Pub should be 1-10 min, got {time}"
    );
}

#[test]
fn test_parish_path_pub_to_fairy_fort() {
    let graph = load_parish_graph();
    let path = graph.shortest_path(LocationId(2), LocationId(11)).unwrap();
    // Should be a multi-hop path
    assert!(path.len() >= 3);
    // Total time should be reasonable
    let time = graph.path_travel_time(&path, 1.25);
    assert!(
        time > 0 && time < 120,
        "travel time should be reasonable: {}",
        time
    );
}

#[test]
fn test_parish_all_locations_reachable() {
    let graph = load_parish_graph();
    let ids = graph.location_ids();
    // Every location should be reachable from the crossroads
    for id in &ids {
        let path = graph.shortest_path(LocationId(1), *id);
        assert!(
            path.is_some(),
            "location {:?} should be reachable from the crossroads",
            id
        );
    }
}

#[test]
fn test_movement_go_to_pub() {
    let graph = load_parish_graph();
    let result = resolve_movement("the pub", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            assert_eq!(destination, LocationId(2));
            assert!(
                minutes >= 1 && minutes <= 10,
                "pub should be 1-10 min walk, got {minutes}"
            );
            assert!(narration.contains("on foot"));
        }
        other => panic!("expected Arrived at pub, got {:?}", other),
    }
}

#[test]
fn test_movement_go_to_church() {
    let graph = load_parish_graph();
    let result = resolve_movement("church", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            ..
        } => {
            assert_eq!(destination, LocationId(3));
            assert!(
                minutes >= 1 && minutes <= 15,
                "church should be 1-15 min walk, got {minutes}"
            );
        }
        other => panic!("expected Arrived at church, got {:?}", other),
    }
}

#[test]
fn test_movement_already_here() {
    let graph = load_parish_graph();
    let result = resolve_movement("crossroads", &graph, LocationId(1), &walking());
    assert_eq!(result, MovementResult::AlreadyHere);
}

#[test]
fn test_movement_not_found() {
    let graph = load_parish_graph();
    let result = resolve_movement("hogwarts", &graph, LocationId(1), &walking());
    match result {
        MovementResult::NotFound(name) => assert_eq!(name, "hogwarts"),
        other => panic!("expected NotFound, got {:?}", other),
    }
}

#[test]
fn test_movement_time_advancement() {
    let graph = load_parish_graph();
    let result = resolve_movement("pub", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived { minutes, .. } => {
            use chrono::{TimeZone, Utc};
            use parish::world::time::GameClock;

            let mut clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 8, 0, 0).unwrap());
            clock.advance(minutes as i64);
            // Clock should have advanced by the computed minutes
            assert!(minutes >= 1, "should advance at least 1 minute");
        }
        other => panic!("expected Arrived, got {:?}", other),
    }
}

#[test]
fn test_encounter_probability_distribution() {
    // Run 10000 trials at midday (threshold 0.20)
    let mut hits = 0;
    for i in 0..10000 {
        let roll = i as f64 / 10000.0;
        if check_encounter(TimeOfDay::Midday, roll).is_some() {
            hits += 1;
        }
    }
    // Should be exactly 2000 (20%)
    assert_eq!(hits, 2000);
}

#[test]
fn test_dynamic_description_rendering() {
    let graph = load_parish_graph();
    let loc = graph.get(LocationId(1)).unwrap();
    let desc = render_description(loc, TimeOfDay::Morning, "Overcast", &[]);
    assert!(desc.contains("morning"), "should contain time: {}", desc);
    assert!(
        desc.contains("overcast"),
        "should contain weather: {}",
        desc
    );
    assert!(
        !desc.contains("{time}"),
        "should not contain raw placeholder: {}",
        desc
    );
    assert!(
        !desc.contains("{weather}"),
        "should not contain raw placeholder: {}",
        desc
    );
}

#[test]
fn test_parish_description_templates_have_placeholders() {
    let graph = load_parish_graph();
    for id in graph.location_ids() {
        let loc = graph.get(id).unwrap();
        assert!(
            loc.description_template.contains("{time}"),
            "location '{}' should have {{time}} placeholder",
            loc.name
        );
        assert!(
            loc.description_template.contains("{weather}"),
            "location '{}' should have {{weather}} placeholder",
            loc.name
        );
    }
}

#[test]
fn test_parish_computed_travel_times_reasonable() {
    let graph = load_parish_graph();
    for id in graph.location_ids() {
        for (target_id, _) in graph.neighbors(id) {
            let minutes = graph.edge_travel_minutes(id, target_id, 1.25);
            assert!(
                minutes >= 1 && minutes <= 60,
                "travel time {} min from {:?} to {:?} should be 1-60 minutes",
                minutes,
                id,
                target_id
            );
        }
    }
}

#[test]
fn test_parish_indoor_locations() {
    let graph = load_parish_graph();
    let pub_loc = graph.get(LocationId(2)).unwrap();
    assert!(pub_loc.indoor, "Darcy's Pub should be indoor");

    let crossroads = graph.get(LocationId(1)).unwrap();
    assert!(!crossroads.indoor, "The Crossroads should be outdoor");
}

#[test]
fn test_parish_mythological_locations() {
    let graph = load_parish_graph();

    // The Fairy Fort should have mythological significance
    let fort = graph.get(LocationId(11)).unwrap();
    assert!(
        fort.mythological_significance.is_some(),
        "The Fairy Fort should have mythological significance"
    );

    // The Crossroads should have mythological significance
    let crossroads = graph.get(LocationId(1)).unwrap();
    assert!(
        crossroads.mythological_significance.is_some(),
        "The Crossroads should have mythological significance"
    );

    // Darcy's Pub should not
    let pub_loc = graph.get(LocationId(2)).unwrap();
    assert!(
        pub_loc.mythological_significance.is_none(),
        "Darcy's Pub should not have mythological significance"
    );
}

// ── Alias matching integration tests ────────────────────────────────────────

#[test]
fn test_parish_find_by_alias_coast() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("coast").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "Lough Ree Shore");
}

#[test]
fn test_parish_find_by_alias_store() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("store").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "Connolly's Shop");
}

#[test]
fn test_parish_find_by_alias_rath() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("rath").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "The Fairy Fort");
}

#[test]
fn test_parish_find_by_alias_post_office() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("post office").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "The Letter Office");
}

#[test]
fn test_parish_find_by_alias_town() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("town").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "Kilteevan Village");
}

#[test]
fn test_parish_find_by_alias_bog() {
    let graph = load_parish_graph();
    let id = graph.find_by_name("bog").unwrap();
    let loc = graph.get(id).unwrap();
    assert_eq!(loc.name, "The Bog Road");
}

#[test]
fn test_movement_go_to_coast() {
    let graph = load_parish_graph();
    let result = resolve_movement("the coast", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived {
            destination,
            narration,
            ..
        } => {
            assert_eq!(destination, LocationId(7));
            assert!(narration.contains("on foot"));
        }
        other => panic!("expected Arrived at Lough Ree Shore, got {:?}", other),
    }
}
