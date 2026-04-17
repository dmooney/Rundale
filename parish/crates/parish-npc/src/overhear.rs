//! Overhear mechanic — surfaces Tier 2 events to the player.
//!
//! When Tier 2 interactions occur at locations one edge away from
//! the player, snippets are shown as atmospheric text in the log.

use crate::types::Tier2Event;
use parish_types::LocationId;
use parish_world::graph::WorldGraph;

/// Filters Tier 2 events to those the player could overhear.
///
/// An event is overhearable if it occurred at a location exactly 1 edge
/// away from the player's current location. Events at the player's own
/// location are experienced directly, not overheard.
///
/// Returns formatted atmospheric messages for each overhearable event.
pub fn check_overhear(
    events: &[Tier2Event],
    player_location: LocationId,
    graph: &WorldGraph,
) -> Vec<String> {
    let neighbor_ids: Vec<LocationId> = graph
        .neighbors(player_location)
        .into_iter()
        .map(|(id, _)| id)
        .collect();

    let mut messages = Vec::new();
    for event in events {
        // Skip events at the player's location (experienced directly)
        if event.location == player_location {
            continue;
        }
        // Only overhear from adjacent locations
        if !neighbor_ids.contains(&event.location) {
            continue;
        }
        // Get location name for atmospheric description
        let location_name = graph
            .get(event.location)
            .map(|d| d.name.as_str())
            .unwrap_or("nearby");

        let msg = format_overhear_message(&event.summary, location_name);
        messages.push(msg);
    }
    messages
}

/// Formats an overhear event into atmospheric prose.
fn format_overhear_message(summary: &str, location_name: &str) -> String {
    format!(
        "From the direction of {}, you catch a murmur of voices... {}",
        location_name, summary
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_types::NpcId;
    use std::path::Path;

    fn make_event(location: u32, summary: &str) -> Tier2Event {
        Tier2Event {
            location: LocationId(location),
            summary: summary.to_string(),
            participants: vec![NpcId(1), NpcId(2)],
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
            emotion_deltas: Vec::new(),
        }
    }

    #[test]
    fn test_overhear_adjacent_location() {
        let path = Path::new("data/parish.json");
        if !path.exists() {
            return;
        }
        let graph = parish_world::graph::WorldGraph::load_from_file(path).unwrap();

        // Player at crossroads (1), event at pub (2) which is 1 edge away
        let events = vec![make_event(2, "Padraig polishes glasses")];
        let messages = check_overhear(&events, LocationId(1), &graph);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("Darcy's Pub"));
        assert!(messages[0].contains("Padraig polishes glasses"));
    }

    #[test]
    fn test_overhear_same_location_excluded() {
        let path = Path::new("data/parish.json");
        if !path.exists() {
            return;
        }
        let graph = parish_world::graph::WorldGraph::load_from_file(path).unwrap();

        // Event at player's own location — not overheard
        let events = vec![make_event(1, "Something happens here")];
        let messages = check_overhear(&events, LocationId(1), &graph);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_overhear_distant_location_excluded() {
        let path = Path::new("data/parish.json");
        if !path.exists() {
            return;
        }
        let graph = parish_world::graph::WorldGraph::load_from_file(path).unwrap();

        // Player at crossroads (1), event at fairy fort (11) which is far
        // First check they are NOT neighbors
        let neighbors: Vec<LocationId> = graph
            .neighbors(LocationId(1))
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        if neighbors.contains(&LocationId(11)) {
            // If they happen to be neighbors in this graph, skip this test
            return;
        }

        let events = vec![make_event(11, "Distant event")];
        let messages = check_overhear(&events, LocationId(1), &graph);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_format_overhear_message() {
        let msg = format_overhear_message("shared stories", "Darcy's Pub");
        assert!(msg.contains("Darcy's Pub"));
        assert!(msg.contains("shared stories"));
        assert!(msg.contains("murmur of voices"));
    }
}
