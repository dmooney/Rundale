//! Weather/transport application and narration building for the movement system.
//!
//! Contains helpers for computing weather-adjusted travel times, building
//! blocked-route fallbacks, and generating narration text.

use super::resolve::{MovementResult, WeatherEffect, weather_effect};
use crate::graph::WorldGraph;
use crate::transport::TransportMode;
use parish_types::{LocationId, Weather, minute_word};

/// Computes weather-adjusted travel time and slowdown notes for a path.
pub(super) fn weather_adjusted_travel(
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
pub(super) fn blocked_or_fallback(
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

/// Builds travel narration text from a path through the world graph.
///
/// For single-hop journeys, uses the connection's path description.
/// For multi-hop journeys, describes the first step with a summary.
/// Includes the transport label (e.g., "on foot") in the time display.
pub(super) fn build_travel_narration(
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
                "You {} along {}. ({} {} {})",
                verb,
                conn.path_description,
                total_minutes,
                minute_word(total_minutes),
                transport.label
            );
        }
    }

    // Multi-hop: describe the first leg and summarize
    let first_desc = graph
        .connection_between(path[0], path[1])
        .map(|c| c.path_description.as_str())
        .unwrap_or("the road");

    format!(
        "You set off along {} toward {}. ({} {} {})",
        first_desc,
        dest_name,
        total_minutes,
        minute_word(total_minutes),
        transport.label
    )
}

/// Builds travel narration that appends weather-caused detour notes.
///
/// When the route crosses one or more hazard-tagged edges whose effect
/// is `Slowed`, the distinct notes are joined with semicolons and
/// appended in parentheses so the player sees why their journey took
/// longer than usual.
pub(super) fn build_weather_narration(
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
