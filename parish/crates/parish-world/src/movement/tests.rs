//! Tests for the movement system.

use super::resolve::{
    MovementResult, WeatherEffect, resolve_movement, resolve_movement_with_weather, weather_effect,
};
use super::transport_apply::build_travel_narration;
use crate::graph::{Connection, Hazard, WorldGraph};
use crate::transport::TransportMode;
use parish_types::{LocationId, Weather};

fn walking() -> TransportMode {
    TransportMode::walking()
}

fn test_graph() -> WorldGraph {
    // Use real-ish coordinates so haversine gives meaningful results.
    // Crossroads: 53.618, -8.095
    // Pub: 53.6195, -8.0925  (~230m away → ~3 min walking)
    // Church: 53.6215, -8.099  (~450m away → ~6 min walking)
    // Fort: 53.627, -8.052  (~3km from church → large)
    let json = r#"{
        "locations": [
            {
                "id": 1,
                "name": "The Crossroads",
                "description_template": "A crossroads.",
                "indoor": false,
                "public": true,
                "lat": 53.618,
                "lon": -8.095,
                "connections": [
                    {"target": 2, "path_description": "a short lane"},
                    {"target": 3, "path_description": "a winding boreen"}
                ]
            },
            {
                "id": 2,
                "name": "Darcy's Pub",
                "description_template": "A pub.",
                "indoor": true,
                "public": true,
                "lat": 53.6195,
                "lon": -8.0925,
                "connections": [
                    {"target": 1, "path_description": "back to the crossroads"}
                ]
            },
            {
                "id": 3,
                "name": "St. Brigid's Church",
                "description_template": "A church.",
                "indoor": false,
                "public": true,
                "lat": 53.6215,
                "lon": -8.099,
                "connections": [
                    {"target": 1, "path_description": "the boreen back"},
                    {"target": 4, "path_description": "a path through the graveyard"}
                ]
            },
            {
                "id": 4,
                "name": "The Fairy Fort",
                "description_template": "A fairy fort.",
                "indoor": false,
                "public": true,
                "lat": 53.627,
                "lon": -8.052,
                "connections": [
                    {"target": 3, "path_description": "back past the church"}
                ]
            }
        ]
    }"#;
    WorldGraph::load_from_str(json).unwrap()
}

#[test]
fn test_resolve_direct_movement() {
    let graph = test_graph();
    let result = resolve_movement("pub", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived {
            destination,
            minutes,
            narration,
            ..
        } => {
            assert_eq!(destination, LocationId(2));
            assert!((1..=10).contains(&minutes), "minutes was {minutes}");
            assert!(narration.contains("short lane"));
            assert!(narration.contains("on foot"));
        }
        other => panic!("expected Arrived, got {:?}", other),
    }
}

#[test]
fn test_resolve_multi_hop_movement() {
    let graph = test_graph();
    // From pub to fairy fort: pub -> crossroads -> church -> fairy fort
    let result = resolve_movement("fairy fort", &graph, LocationId(2), &walking());
    match result {
        MovementResult::Arrived {
            destination,
            path,
            minutes,
            narration,
            ..
        } => {
            assert_eq!(destination, LocationId(4));
            assert_eq!(path.len(), 4); // pub -> crossroads -> church -> fort
            assert!(
                minutes >= 5,
                "multi-hop should take several minutes, got {minutes}"
            );
            assert!(narration.contains("on foot"));
        }
        other => panic!("expected Arrived, got {:?}", other),
    }
}

#[test]
fn test_resolve_already_here() {
    let graph = test_graph();
    let result = resolve_movement("crossroads", &graph, LocationId(1), &walking());
    assert_eq!(result, MovementResult::AlreadyHere);
}

#[test]
fn test_resolve_not_found() {
    let graph = test_graph();
    let result = resolve_movement("castle", &graph, LocationId(1), &walking());
    match result {
        MovementResult::NotFound(name) => assert_eq!(name, "castle"),
        other => panic!("expected NotFound, got {:?}", other),
    }
}

#[test]
fn test_resolve_case_insensitive() {
    let graph = test_graph();
    let result = resolve_movement("DARCY'S PUB", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived { destination, .. } => {
            assert_eq!(destination, LocationId(2));
        }
        other => panic!("expected Arrived, got {:?}", other),
    }
}

#[test]
fn test_resolve_partial_name() {
    let graph = test_graph();
    let result = resolve_movement("church", &graph, LocationId(1), &walking());
    match result {
        MovementResult::Arrived { destination, .. } => {
            assert_eq!(destination, LocationId(3));
        }
        other => panic!("expected Arrived, got {:?}", other),
    }
}

#[test]
fn test_narration_direct_walking() {
    let graph = test_graph();
    let transport = walking();
    let path = vec![LocationId(1), LocationId(2)];
    let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
    let narration = build_travel_narration(&path, &graph, minutes, &transport);
    assert!(narration.starts_with("You walk along a short lane."));
    assert!(narration.contains("on foot"));
}

#[test]
fn test_narration_direct_non_walking() {
    let graph = test_graph();
    let transport = TransportMode {
        id: "jaunting_car".to_string(),
        label: "in a jaunting car".to_string(),
        speed_m_per_s: 4.0,
    };
    let path = vec![LocationId(1), LocationId(2)];
    let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
    let narration = build_travel_narration(&path, &graph, minutes, &transport);
    assert!(narration.starts_with("You travel along a short lane."));
    assert!(narration.contains("in a jaunting car"));
}

#[test]
fn test_narration_multi_hop() {
    let graph = test_graph();
    let transport = walking();
    let path = vec![LocationId(2), LocationId(1), LocationId(3), LocationId(4)];
    let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);
    let narration = build_travel_narration(&path, &graph, minutes, &transport);
    assert!(narration.contains("The Fairy Fort"));
    assert!(narration.contains("on foot"));
}

#[test]
fn test_narration_empty_path() {
    let graph = test_graph();
    let narration = build_travel_narration(&[], &graph, 0, &walking());
    assert!(narration.is_empty());
}

#[test]
fn test_faster_transport_takes_less_time() {
    let graph = test_graph();
    let walk = walking();
    let fast = TransportMode {
        id: "jaunting_car".to_string(),
        label: "in a jaunting car".to_string(),
        speed_m_per_s: 4.0,
    };
    let path = vec![LocationId(1), LocationId(3), LocationId(4)];
    let walk_time = graph.path_travel_time(&path, walk.speed_m_per_s);
    let fast_time = graph.path_travel_time(&path, fast.speed_m_per_s);
    assert!(
        fast_time <= walk_time,
        "jaunting car ({fast_time} min) should be <= walking ({walk_time} min)"
    );
}

// ── Weather-gated movement tests ────────────────────────────────────────

/// A four-node graph where the crossroads -> church edge is flood-prone.
/// In a storm, travel from the pub to the fairy fort must still be
/// possible because there's no alternative route — the resolver should
/// surface the blocker, not silently route through it.
fn hazard_graph() -> WorldGraph {
    let json = r#"{
        "locations": [
            {
                "id": 1, "name": "The Crossroads",
                "description_template": "A crossroads.",
                "indoor": false, "public": true,
                "lat": 53.618, "lon": -8.095,
                "connections": [
                    {"target": 2, "path_description": "a short lane"},
                    {"target": 3, "path_description": "a winding boreen over a ford", "hazard": "flood"},
                    {"target": 5, "path_description": "the long road around"}
                ]
            },
            {
                "id": 2, "name": "Darcy's Pub",
                "description_template": "A pub.",
                "indoor": true, "public": true,
                "lat": 53.6195, "lon": -8.0925,
                "connections": [
                    {"target": 1, "path_description": "back to the crossroads"}
                ]
            },
            {
                "id": 3, "name": "St. Brigid's Church",
                "description_template": "A church.",
                "indoor": false, "public": true,
                "lat": 53.6215, "lon": -8.099,
                "connections": [
                    {"target": 1, "path_description": "the boreen back over the ford", "hazard": "flood"},
                    {"target": 4, "path_description": "through gorse", "hazard": "exposed"},
                    {"target": 5, "path_description": "the long road back"}
                ]
            },
            {
                "id": 4, "name": "The Fairy Fort",
                "description_template": "A fairy fort.",
                "indoor": false, "public": true,
                "lat": 53.627, "lon": -8.052,
                "connections": [
                    {"target": 3, "path_description": "back through gorse", "hazard": "exposed"}
                ]
            },
            {
                "id": 5, "name": "The Long Road",
                "description_template": "Just a waypoint.",
                "indoor": false, "public": true,
                "lat": 53.620, "lon": -8.080,
                "connections": [
                    {"target": 1, "path_description": "back to the crossroads the long way"},
                    {"target": 3, "path_description": "the long road to the church"}
                ]
            }
        ]
    }"#;
    WorldGraph::load_from_str(json).unwrap()
}

#[test]
fn test_clear_weather_matches_legacy_resolver() {
    let graph = hazard_graph();
    let legacy = resolve_movement("church", &graph, LocationId(2), &walking());
    let weather_aware =
        resolve_movement_with_weather("church", &graph, LocationId(2), &walking(), Weather::Clear);
    // Destinations and path equality under clear weather.
    match (legacy, weather_aware) {
        (
            MovementResult::Arrived {
                destination: d1,
                path: p1,
                ..
            },
            MovementResult::Arrived {
                destination: d2,
                path: p2,
                ..
            },
        ) => {
            assert_eq!(d1, d2);
            assert_eq!(p1, p2);
        }
        (a, b) => panic!("mismatch: {:?} vs {:?}", a, b),
    }
}

#[test]
fn test_storm_reroutes_around_flooded_ford() {
    let graph = hazard_graph();
    // Pub -> Church: direct goes via the ford, but we have the long road.
    let result =
        resolve_movement_with_weather("church", &graph, LocationId(2), &walking(), Weather::Storm);
    match result {
        MovementResult::Arrived {
            destination, path, ..
        } => {
            assert_eq!(destination, LocationId(3));
            // Must route around the ford, so the path must include the long road (id 5).
            assert!(
                path.contains(&LocationId(5)),
                "storm should reroute via the long road, got {:?}",
                path
            );
            assert!(
                !path.windows(2).any(|w| {
                    (w[0] == LocationId(1) && w[1] == LocationId(3))
                        || (w[0] == LocationId(3) && w[1] == LocationId(1))
                }),
                "path must not use the flooded ford, got {:?}",
                path
            );
        }
        other => panic!("expected Arrived via reroute, got {:?}", other),
    }
}

#[test]
fn test_storm_blocks_when_no_alternative() {
    // Two-node graph where the only connection is flood-prone.
    let json = r#"{
        "locations": [
            {
                "id": 1, "name": "Home",
                "description_template": "Home.",
                "indoor": false, "public": true,
                "lat": 53.618, "lon": -8.095,
                "connections": [
                    {"target": 2, "path_description": "a ford crossing", "hazard": "flood"}
                ]
            },
            {
                "id": 2, "name": "Across",
                "description_template": "Across.",
                "indoor": false, "public": true,
                "lat": 53.620, "lon": -8.093,
                "connections": [
                    {"target": 1, "path_description": "a ford crossing", "hazard": "flood"}
                ]
            }
        ]
    }"#;
    let graph = WorldGraph::load_from_str(json).unwrap();
    let result =
        resolve_movement_with_weather("across", &graph, LocationId(1), &walking(), Weather::Storm);
    match result {
        MovementResult::BlockedByWeather {
            destination,
            weather,
            hazard,
            reason,
        } => {
            assert_eq!(destination, LocationId(2));
            assert_eq!(weather, Weather::Storm);
            assert_eq!(hazard, Hazard::Flood);
            assert!(reason.to_lowercase().contains("stream"));
        }
        other => panic!("expected BlockedByWeather, got {:?}", other),
    }
}

#[test]
fn test_heavy_rain_slows_but_does_not_block() {
    let graph = hazard_graph();
    let clear =
        resolve_movement_with_weather("church", &graph, LocationId(1), &walking(), Weather::Clear);
    let rain = resolve_movement_with_weather(
        "church",
        &graph,
        LocationId(1),
        &walking(),
        Weather::HeavyRain,
    );
    match (clear, rain) {
        (
            MovementResult::Arrived {
                minutes: m_clear, ..
            },
            MovementResult::Arrived {
                minutes: m_rain,
                narration,
                ..
            },
        ) => {
            assert!(
                m_rain >= m_clear,
                "heavy rain should be at least as slow: clear {} min, rain {} min",
                m_clear,
                m_rain
            );
            assert!(
                narration.to_lowercase().contains("rising water")
                    || narration.to_lowercase().contains("weather:"),
                "narration should mention weather note, got: {}",
                narration
            );
        }
        other => panic!("expected both Arrived, got {:?}", other),
    }
}

#[test]
fn test_fog_slows_exposed_path() {
    let graph = hazard_graph();
    // Crossroads (1) -> Fairy Fort (4) goes via Church -> gorse (exposed).
    let clear = resolve_movement_with_weather(
        "fairy fort",
        &graph,
        LocationId(1),
        &walking(),
        Weather::Clear,
    );
    let fog = resolve_movement_with_weather(
        "fairy fort",
        &graph,
        LocationId(1),
        &walking(),
        Weather::Fog,
    );
    match (clear, fog) {
        (
            MovementResult::Arrived {
                minutes: m_clear, ..
            },
            MovementResult::Arrived {
                minutes: m_fog,
                narration,
                ..
            },
        ) => {
            assert!(
                m_fog > m_clear,
                "fog should slow the exposed leg: clear {} min, fog {} min",
                m_clear,
                m_fog
            );
            assert!(
                narration.to_lowercase().contains("fog")
                    || narration.to_lowercase().contains("path"),
                "fog narration should mention the condition, got: {}",
                narration
            );
        }
        other => panic!("expected both Arrived, got {:?}", other),
    }
}

#[test]
fn test_weather_effect_table() {
    // Every hazard behaves sensibly under its matching weather.
    let flood_conn = Connection {
        target: LocationId(2),
        path_description: String::new(),
        hazard: Hazard::Flood,
    };
    assert!(matches!(
        weather_effect(&flood_conn, Weather::Storm),
        WeatherEffect::Impassable { .. }
    ));
    assert!(matches!(
        weather_effect(&flood_conn, Weather::HeavyRain),
        WeatherEffect::Slowed { .. }
    ));
    assert_eq!(
        weather_effect(&flood_conn, Weather::Clear),
        WeatherEffect::Clear
    );

    let lake_conn = Connection {
        hazard: Hazard::Lakeshore,
        ..flood_conn.clone()
    };
    assert!(matches!(
        weather_effect(&lake_conn, Weather::Storm),
        WeatherEffect::Impassable { .. }
    ));

    let exposed_conn = Connection {
        hazard: Hazard::Exposed,
        ..flood_conn.clone()
    };
    assert!(matches!(
        weather_effect(&exposed_conn, Weather::Fog),
        WeatherEffect::Slowed { .. }
    ));
    // Exposed paths are not impassable even in a storm — just slower.
    assert!(matches!(
        weather_effect(&exposed_conn, Weather::Storm),
        WeatherEffect::Slowed { .. }
    ));
}
