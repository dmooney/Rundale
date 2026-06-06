//! Target resolution and path-finding for the movement system.
//!
//! Contains the core types ([`MovementResult`], [`WeatherEffect`]) and the
//! two public entry-points: [`resolve_movement`] (weather-unaware) and
//! [`resolve_movement_with_weather`] (weather-aware).

use super::transport_apply::{
    blocked_or_fallback, build_travel_narration, build_weather_narration, weather_adjusted_travel,
};
use crate::graph::{Connection, Hazard, WorldGraph};
use crate::transport::TransportMode;
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
pub(super) fn resolve_target(
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
