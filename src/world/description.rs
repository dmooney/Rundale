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
    let weather_str = weather.to_lowercase();
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
/// Returns a string like "You can go to: Darcy's Pub (3 min), St. Brigid's Church (5 min)"
pub fn format_exits(location_id: LocationId, graph: &WorldGraph) -> String {
    let neighbors = graph.neighbors(location_id);
    if neighbors.is_empty() {
        return "There is nowhere to go from here.".to_string();
    }

    let exits: Vec<String> = neighbors
        .iter()
        .filter_map(|(target_id, conn)| {
            graph
                .get(*target_id)
                .map(|loc| format!("{} ({} min)", loc.name, conn.traversal_minutes))
        })
        .collect();

    format!("You can go to: {}", exits.join(", "))
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
            lat: 53.618,
            lon: -8.095,
            connections: vec![],
            associated_npcs: vec![NpcId(1)],
            mythological_significance: None,
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
            lat: 53.618,
            lon: -8.095,
            connections: vec![],
            associated_npcs: vec![],
            mythological_significance: None,
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
                    "connections": [
                        {"target": 2, "traversal_minutes": 3, "path_description": "lane"},
                        {"target": 3, "traversal_minutes": 5, "path_description": "boreen"}
                    ]
                },
                {
                    "id": 2, "name": "Darcy's Pub",
                    "description_template": "X", "indoor": true, "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 3, "path_description": "back"}]
                },
                {
                    "id": 3, "name": "The Church",
                    "description_template": "X", "indoor": false, "public": true,
                    "connections": [{"target": 1, "traversal_minutes": 5, "path_description": "back"}]
                }
            ]
        }"#;
        let graph = WorldGraph::load_from_str(json).unwrap();
        let exits = format_exits(LocationId(1), &graph);
        assert!(exits.starts_with("You can go to: "));
        assert!(exits.contains("Darcy's Pub (3 min)"));
        assert!(exits.contains("The Church (5 min)"));
    }

    #[test]
    fn test_format_exits_empty_graph() {
        let graph = WorldGraph::new();
        let exits = format_exits(LocationId(99), &graph);
        assert_eq!(exits, "There is nowhere to go from here.");
    }
}
