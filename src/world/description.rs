//! Dynamic location description rendering.
//!
//! Interpolates description templates with current game state
//! (time of day, weather, NPCs present).

use super::graph::LocationData;
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
        };
        let result = render_description(&loc, TimeOfDay::Morning, "Clear", &[]);
        assert_eq!(result, "A plain description with no placeholders.");
    }
}
