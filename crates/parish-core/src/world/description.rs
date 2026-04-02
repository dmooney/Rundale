//! Dynamic location description rendering.
//!
//! Interpolates description templates with current game state
//! (time of day, weather, NPCs present).

use super::LocationId;
use super::graph::{LocationData, WorldGraph};
use super::time::TimeOfDay;

/// Renders a location description by interpolating template placeholders.
///
/// Supported placeholders:
/// - `{time}` — current time of day (e.g., "morning", "dusk")
/// - `{weather}` — current weather string (e.g., "overcast", "clear")
/// - `{npcs_present}` — comma-separated list of NPC names, or "no one"
pub fn render_description(
    location: &LocationData,
    time_of_day: TimeOfDay,
    weather: &str,
    npc_names: &[&str],
) -> String {
    let time_str = time_display(time_of_day);
    let weather_str = weather_display(weather);
    let npcs_str = if npc_names.is_empty() {
        "no one".to_string()
    } else {
        npc_names.join(", ")
    };

    location
        .description_template
        .replace("{time}", &time_str)
        .replace("{weather}", &weather_str)
        .replace("{npcs_present}", &npcs_str)
}

/// Formats the list of exits (neighboring locations) from a given location.
///
/// Travel time for each exit is computed from coordinates at the given speed.
/// Returns a string like "You can go to: Darcy's Pub (3 min on foot), The Church (6 min on foot)"
pub fn format_exits(
    location_id: LocationId,
    graph: &WorldGraph,
    speed_m_per_s: f64,
    transport_label: &str,
) -> String {
    let neighbors = graph.neighbors(location_id);
    if neighbors.is_empty() {
        return "There is nowhere to go from here.".to_string();
    }

    let exits: Vec<String> = neighbors
        .iter()
        .filter_map(|(target_id, _conn)| {
            let minutes = graph.edge_travel_minutes(location_id, *target_id, speed_m_per_s);
            graph
                .get(*target_id)
                .map(|loc| format!("{} ({} min {})", loc.name, minutes, transport_label))
        })
        .collect();

    format!("You can go to: {}", exits.join(", "))
}

/// Converts a weather string to a template-friendly form.
///
/// Multi-word weather names are reworded so they read naturally in both
/// adjective position ("The {weather} sky") and predicate position
/// ("The sky is {weather}").
fn weather_display(weather: &str) -> String {
    match weather {
        "Partly Cloudy" => "partly cloudy".to_string(),
        "Light Rain" => "rainy".to_string(),
        "Heavy Rain" => "rain-soaked".to_string(),
        other => other.to_lowercase(),
    }
}

/// Converts a `TimeOfDay` to a human-friendly lowercase string for templates.
fn time_display(tod: TimeOfDay) -> String {
    match tod {
        TimeOfDay::Dawn => "dawn".to_string(),
        TimeOfDay::Morning => "morning".to_string(),
        TimeOfDay::Midday => "midday".to_string(),
        TimeOfDay::Afternoon => "afternoon".to_string(),
        TimeOfDay::Dusk => "dusk".to_string(),
        TimeOfDay::Night => "evening".to_string(),
        TimeOfDay::Midnight => "late night".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::NpcId;
    use crate::world::LocationId;
    use crate::world::graph::LocationData;

    fn test_location() -> LocationData {
        LocationData {
            id: LocationId(1),
            name: "Test Place".to_string(),
            description_template:
                "A place at {time}. Weather: {weather}. People here: {npcs_present}.".to_string(),
            indoor: false,
            public: true,
            connections: vec![],
            associated_npcs: vec![NpcId(1)],
            mythological_significance: None,
            lat: 0.0,
            lon: 0.0,
            aliases: vec![],
        }
    }

    #[test]
    fn test_render_with_all_placeholders() {
        let loc = test_location();
        let result = render_description(&loc, TimeOfDay::Morning, "Overcast", &["Padraig O'Brien"]);
        assert_eq!(
            result,
            "A place at morning. Weather: overcast. People here: Padraig O'Brien."
        );
    }

    #[test]
    fn test_render_no_npcs() {
        let loc = test_location();
        let result = render_description(&loc, TimeOfDay::Dusk, "Clear", &[]);
        assert_eq!(
            result,
            "A place at dusk. Weather: clear. People here: no one."
        );
    }

    #[test]
    fn test_render_multiple_npcs() {
        let loc = test_location();
        let result = render_description(
            &loc,
            TimeOfDay::Midday,
            "Drizzle",
            &["Mary", "Padraig", "Siobhan"],
        );
        assert!(result.contains("Mary, Padraig, Siobhan"));
    }

    #[test]
    fn test_render_all_times() {
        let loc = test_location();
        let times = [
            (TimeOfDay::Dawn, "dawn"),
            (TimeOfDay::Morning, "morning"),
            (TimeOfDay::Midday, "midday"),
            (TimeOfDay::Afternoon, "afternoon"),
            (TimeOfDay::Dusk, "dusk"),
            (TimeOfDay::Night, "evening"),
            (TimeOfDay::Midnight, "late night"),
        ];
        for (tod, expected) in &times {
            let result = render_description(&loc, *tod, "Clear", &[]);
            assert!(
                result.contains(expected),
                "Expected '{}' in result for {:?}: {}",
                expected,
                tod,
                result
            );
        }
    }

    #[test]
    fn test_render_no_placeholders() {
        let loc = LocationData {
            id: LocationId(1),
            name: "Plain".to_string(),
            description_template: "A plain description with no placeholders.".to_string(),
            indoor: false,
            public: true,
            connections: vec![],
            associated_npcs: vec![],
            mythological_significance: None,
            lat: 0.0,
            lon: 0.0,
            aliases: vec![],
        };
        let result = render_description(&loc, TimeOfDay::Morning, "Clear", &[]);
        assert_eq!(result, "A plain description with no placeholders.");
    }

    #[test]
    fn test_format_exits() {
        let json = r#"{
            "locations": [
                {
                    "id": 1, "name": "The Crossroads",
                    "description_template": "X", "indoor": false, "public": true,
                    "lat": 53.618, "lon": -8.095,
                    "connections": [
                        {"target": 2, "path_description": "lane"},
                        {"target": 3, "path_description": "boreen"}
                    ]
                },
                {
                    "id": 2, "name": "Darcy's Pub",
                    "description_template": "X", "indoor": true, "public": true,
                    "lat": 53.6195, "lon": -8.0925,
                    "connections": [{"target": 1, "path_description": "back"}]
                },
                {
                    "id": 3, "name": "The Church",
                    "description_template": "X", "indoor": false, "public": true,
                    "lat": 53.6215, "lon": -8.099,
                    "connections": [{"target": 1, "path_description": "back"}]
                }
            ]
        }"#;
        let graph = WorldGraph::load_from_str(json).unwrap();
        let exits = format_exits(LocationId(1), &graph, 1.25, "on foot");
        assert!(exits.starts_with("You can go to: "));
        assert!(exits.contains("Darcy's Pub"));
        assert!(exits.contains("min on foot"));
        assert!(exits.contains("The Church"));
    }

    #[test]
    fn test_format_exits_empty_graph() {
        let graph = WorldGraph::new();
        let exits = format_exits(LocationId(99), &graph, 1.25, "on foot");
        assert_eq!(exits, "There is nowhere to go from here.");
    }
}
