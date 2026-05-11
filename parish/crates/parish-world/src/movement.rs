//! Movement system — resolving movement commands and traversal with time.
//!
//! Handles resolving player movement intents to destinations, computing
//! travel time along paths, and producing narration text for travel.
//!
//! Severe weather can block certain connections (marked with
//! [`Hazard`](super::graph::Hazard)) and slow others. The weather-aware
//! entry point is [`resolve_movement_with_weather`]; the legacy
//! [`resolve_movement`] is preserved as a `Weather::Clear` wrapper so
//! tests and older callers keep working.

use super::graph::{Connection, Hazard, WorldGraph};
use super::transport::TransportMode;
use parish_types::{LocationId, Weather};

/// The result of resolving a movement command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MovementResult {
    /// Player arrived at a destination after the given number of game minutes.
    Arrived {
        /// The destination location id.
        destination: LocationId,
        /// The path taken (including start and end).
        path: Vec<LocationId>,
        /// Total travel time in game minutes.
        minutes: u16,
        /// Narration text describing the journey.
        narration: String,
    },
    /// The target location could not be found.
    NotFound(String),
    /// The player is already at the target location.
    AlreadyHere,
    /// A route to the destination exists in fair weather, but the current
    /// weather has closed every path. The player learns *why* they cannot
    /// go, and is expected to wait the weather out.
    BlockedByWeather {
        /// The intended destination.
        destination: LocationId,
        /// The hazard that blocked the journey.
        hazard: Hazard,
        /// The current weather causing the block.
        weather: Weather,
        /// Player-facing refusal text.
        reason: String,
    },
}

/// How the current weather affects a single connection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WeatherEffect {
    /// Edge is open and travels at normal speed.
    Clear,
    /// Edge is open, but effective speed is multiplied by `factor` (< 1.0).
    Slowed {
        /// Multiplier to apply to the base transport speed (e.g. 0.5 = half speed).
        factor: f64,
        /// Short prose phrase to splice into narration.
        note: &'static str,
    },
    /// Edge is fully impassable under the current weather.
    Impassable {
        /// Short reason shown to the player when the path is refused.
        reason: &'static str,
    },
}

/// Evaluates how the current weather affects travel along `conn`.
///
/// Edge rules:
/// - [`Hazard::Flood`] + [`Weather::Storm`]      → impassable.
/// - [`Hazard::Flood`] + [`Weather::HeavyRain`]  → slowed 0.6×, rising water.
/// - [`Hazard::Lakeshore`] + [`Weather::Storm`]  → impassable.
/// - [`Hazard::Lakeshore`] + [`Weather::HeavyRain`] → slowed 0.7×, spray.
/// - [`Hazard::Exposed`] + [`Weather::Fog`]      → slowed 0.6×, lost path.
/// - [`Hazard::Exposed`] + [`Weather::HeavyRain`] → slowed 0.75×, mire.
/// - [`Hazard::Exposed`] + [`Weather::Storm`]    → slowed 0.5×, squalls.
/// - Anything else → clear.
pub fn weather_effect(conn: &Connection, weather: Weather) -> WeatherEffect {
    match (conn.hazard, weather) {
        (Hazard::Flood, Weather::Storm) => WeatherEffect::Impassable {
            reason: "The stream has burst its banks — the crossing is underwater and impassable.",
        },
        (Hazard::Flood, Weather::HeavyRain) => WeatherEffect::Slowed {
            factor: 0.6,
            note: "picking your way across rising water",
        },
        (Hazard::Lakeshore, Weather::Storm) => WeatherEffect::Impassable {
            reason: "The lake is a fury of whitecaps. Spray and wind drive you back from the shore.",
        },
        (Hazard::Lakeshore, Weather::HeavyRain) => WeatherEffect::Slowed {
            factor: 0.7,
            note: "head down against the lake-spray",
        },
        (Hazard::Exposed, Weather::Fog) => WeatherEffect::Slowed {
            factor: 0.6,
            note: "feeling your way through the fog, losing the path more than once",
        },
        (Hazard::Exposed, Weather::HeavyRain) => WeatherEffect::Slowed {
            factor: 0.75,
            note: "boots sucking in the mire",
        },
        (Hazard::Exposed, Weather::Storm) => WeatherEffect::Slowed {
            factor: 0.5,
            note: "bent double against the wind",
        },
        _ => WeatherEffect::Clear,
    }
}

/// Looks up a destination by name and checks for identity.
///
/// Returns `Ok(destination_id)` when the target resolves to a different
/// location than the current one. Returns `Err(MovementResult)` for
/// early-exit cases (not found or already here).
fn resolve_target(
    target: &str,
    graph: &WorldGraph,
    current: LocationId,
) -> Result<LocationId, MovementResult> {
    let destination_id = match graph.find_by_name(target) {
        Some(id) => id,
        None => return Err(MovementResult::NotFound(target.to_string())),
    };
    if destination_id == current {
        return Err(MovementResult::AlreadyHere);
    }
    Ok(destination_id)
}

/// Resolves a movement intent target to a `MovementResult`.
///
/// Uses fuzzy name matching to find the destination, then BFS to find
/// the shortest path. Travel time is calculated from coordinates using
/// the given transport mode's speed. Narration includes the transport label.
pub fn resolve_movement(
    target: &str,
    graph: &WorldGraph,
    current: LocationId,
    transport: &TransportMode,
) -> MovementResult {
    let destination_id = match resolve_target(target, graph, current) {
        Ok(id) => id,
        Err(result) => return result,
    };

    // Find shortest path
    let path = match graph.shortest_path(current, destination_id) {
        Some(p) => p,
        None => return MovementResult::NotFound(target.to_string()),
    };

    // Calculate total travel time from coordinates
    let minutes = graph.path_travel_time(&path, transport.speed_m_per_s);

    // Build narration from first step's connection description
    let narration = build_travel_narration(&path, graph, minutes, transport);

    MovementResult::Arrived {
        destination: destination_id,
        path,
        minutes,
        narration,
    }
}

/// Resolves a movement intent under the current weather.
///
/// Behaves like [`resolve_movement`] but routes around edges that are
/// impassable in the current weather and applies per-edge speed
/// multipliers for slowed edges. If every route to the destination is
/// impassable, returns [`MovementResult::BlockedByWeather`] so the
/// caller can explain the obstacle and let the player wait it out.
///
/// When `Weather::Clear` is passed (or the graph has no hazard tags),
/// the result is identical to [`resolve_movement`].
/// Computes weather-adjusted travel time and slowdown notes for a path.
fn weather_adjusted_travel(
    path: &[LocationId],
    graph: &WorldGraph,
    transport: &TransportMode,
    weather: Weather,
) -> (u16, Vec<&'static str>) {
    let mut total_minutes: u16 = 0;
    let mut notes: Vec<&'static str> = Vec::new();
    for window in path.windows(2) {
        let base = graph.edge_travel_minutes(window[0], window[1], transport.speed_m_per_s);
        let (edge_minutes, note) =
            if let Some(conn) = graph.connection_between(window[0], window[1]) {
                match weather_effect(conn, weather) {
                    WeatherEffect::Clear => (base, None),
                    WeatherEffect::Slowed { factor, note } => {
                        let scaled = ((base as f64 / factor).ceil() as u16).max(base);
                        (scaled, Some(note))
                    }
                    WeatherEffect::Impassable { .. } => (base, None),
                }
            } else {
                (base, None)
            };
        total_minutes = total_minutes.saturating_add(edge_minutes);
        if let Some(n) = note
            && !notes.contains(&n)
        {
            notes.push(n);
        }
    }
    (total_minutes, notes)
}

/// Handles the case where filtered pathfinding found no route: if a
/// fair-weather path exists, determines whether to block or fall through.
fn blocked_or_fallback(
    current: LocationId,
    destination_id: LocationId,
    graph: &WorldGraph,
    transport: &TransportMode,
    weather: Weather,
) -> MovementResult {
    let full_path = match graph.shortest_path(current, destination_id) {
        Some(p) => p,
        None => {
            return MovementResult::NotFound(
                graph
                    .get(destination_id)
                    .map_or_else(|| destination_id.0.to_string(), |l| l.name.clone()),
            );
        }
    };

    for window in full_path.windows(2) {
        if let Some(conn) = graph.connection_between(window[0], window[1])
            && let WeatherEffect::Impassable { reason } = weather_effect(conn, weather)
        {
            return MovementResult::BlockedByWeather {
                destination: destination_id,
                hazard: conn.hazard,
                weather,
                reason: reason.to_string(),
            };
        }
    }

    let minutes = graph.path_travel_time(&full_path, transport.speed_m_per_s);
    let narration = build_travel_narration(&full_path, graph, minutes, transport);
    MovementResult::Arrived {
        destination: destination_id,
        path: full_path,
        minutes,
        narration,
    }
}

pub fn resolve_movement_with_weather(
    target: &str,
    graph: &WorldGraph,
    current: LocationId,
    transport: &TransportMode,
    weather: Weather,
) -> MovementResult {
    let destination_id = match resolve_target(target, graph, current) {
        Ok(id) => id,
        Err(result) => return result,
    };

    let path = match graph.shortest_path_filtered(current, destination_id, |_from, _to, c| {
        !matches!(weather_effect(c, weather), WeatherEffect::Impassable { .. })
    }) {
        Some(p) => p,
        None => {
            return blocked_or_fallback(current, destination_id, graph, transport, weather);
        }
    };

    let (minutes, notes) = weather_adjusted_travel(&path, graph, transport, weather);
    let narration = build_weather_narration(&path, graph, minutes, transport, &notes);

    MovementResult::Arrived {
        destination: destination_id,
        path,
        minutes,
        narration,
    }
}

/// Builds travel narration text from a path through the world graph.
///
/// For single-hop journeys, uses the connection's path description.
/// For multi-hop journeys, describes the first step with a summary.
/// Includes the transport label (e.g., "on foot") in the time display.
fn build_travel_narration(
    path: &[LocationId],
    graph: &WorldGraph,
    total_minutes: u16,
    transport: &TransportMode,
) -> String {
    if path.len() < 2 {
        return String::new();
    }

    let verb = if transport.id == "walking" {
        "walk"
    } else {
        "travel"
    };

    let dest_name = path
        .last()
        .and_then(|id| graph.get(*id))
        .map(|l| l.name.as_str())
        .unwrap_or("your destination");

    if path.len() == 2 {
        // Direct connection
        if let Some(conn) = graph.connection_between(path[0], path[1]) {
            return format!(
                "You {} along {}. ({} minutes {})",
                verb, conn.path_description, total_minutes, transport.label
            );
        }
    }

    // Multi-hop: describe the first leg and summarize
    let first_desc = graph
        .connection_between(path[0], path[1])
        .map(|c| c.path_description.as_str())
        .unwrap_or("the road");

    format!(
        "You set off along {} toward {}. ({} minutes {})",
        first_desc, dest_name, total_minutes, transport.label
    )
}

/// Builds travel narration that appends weather-caused detour notes.
///
/// When the route crosses one or more hazard-tagged edges whose effect
/// is `Slowed`, the distinct notes are joined with semicolons and
/// appended in parentheses so the player sees why their journey took
/// longer than usual.
fn build_weather_narration(
    path: &[LocationId],
    graph: &WorldGraph,
    total_minutes: u16,
    transport: &TransportMode,
    notes: &[&'static str],
) -> String {
    let base = build_travel_narration(path, graph, total_minutes, transport);
    if notes.is_empty() || base.is_empty() {
        return base;
    }
    format!("{} (The weather: {}.)", base, notes.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::WorldGraph;
    use crate::transport::TransportMode;

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
        let weather_aware = resolve_movement_with_weather(
            "church",
            &graph,
            LocationId(2),
            &walking(),
            Weather::Clear,
        );
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
        let result = resolve_movement_with_weather(
            "church",
            &graph,
            LocationId(2),
            &walking(),
            Weather::Storm,
        );
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
        let result = resolve_movement_with_weather(
            "across",
            &graph,
            LocationId(1),
            &walking(),
            Weather::Storm,
        );
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
        let clear = resolve_movement_with_weather(
            "church",
            &graph,
            LocationId(1),
            &walking(),
            Weather::Clear,
        );
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
}
