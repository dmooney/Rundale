//! Overhear mechanic — surface nearby NPC interactions to the player.
//!
//! When Tier 2 events occur at locations adjacent to the player,
//! snippets are made available for display in the TUI.

use crate::world::LocationId;
use crate::world::graph::WorldGraph;

use super::tier::Tier2Event;

/// Checks which Tier 2 events can be overheard by the player.
///
/// An event is overhearable if it occurred at a location exactly
/// 1 edge away from the player's current location.
pub fn check_overhear(
    events: &[Tier2Event],
    player_location: LocationId,
    graph: &WorldGraph,
) -> Vec<String> {
    let neighbors: Vec<LocationId> = graph
        .neighbors(player_location)
        .iter()
        .map(|(id, _)| *id)
        .collect();

    events
        .iter()
        .filter(|event| neighbors.contains(&event.location))
        .map(|event| {
            let loc_name = graph
                .get(event.location)
                .map(|l| l.name.as_str())
                .unwrap_or("nearby");
            format!(
                "You catch a few words drifting from the direction of {}... {}",
                loc_name, event.summary
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::NpcId;

    fn test_graph() -> WorldGraph {
        WorldGraph::load_from_str(
            r#"{
            "locations": [
                {
                    "id": 1, "name": "The Crossroads", "description_template": "A",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "path"},
                        {"target": 3, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 2, "name": "Darcy's Pub", "description_template": "B",
                    "indoor": true, "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 3, "name": "St. Brigid's Church", "description_template": "C",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "path"},
                        {"target": 4, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 4, "name": "The Fairy Fort", "description_template": "D",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 3, "traversal_minutes": 5, "path_description": "path"}
                    ]
                }
            ]
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn test_overhear_adjacent() {
        let graph = test_graph();
        let events = vec![Tier2Event {
            location: LocationId(2), // Pub is 1 edge from Crossroads
            participants: vec![NpcId(1), NpcId(2)],
            summary: "Padraig and Siobhan share a laugh over tea.".to_string(),
            relationship_changes: vec![],
        }];

        let overheard = check_overhear(&events, LocationId(1), &graph);
        assert_eq!(overheard.len(), 1);
        assert!(overheard[0].contains("Darcy's Pub"));
        assert!(overheard[0].contains("share a laugh"));
    }

    #[test]
    fn test_overhear_too_far() {
        let graph = test_graph();
        let events = vec![Tier2Event {
            location: LocationId(4), // Fairy Fort is 2 edges from Crossroads
            participants: vec![NpcId(1)],
            summary: "Someone mutters at the fort.".to_string(),
            relationship_changes: vec![],
        }];

        let overheard = check_overhear(&events, LocationId(1), &graph);
        assert!(overheard.is_empty());
    }

    #[test]
    fn test_overhear_same_location() {
        let graph = test_graph();
        let events = vec![Tier2Event {
            location: LocationId(1), // Same location — not "overheard", player sees directly
            participants: vec![NpcId(1)],
            summary: "Something happens here.".to_string(),
            relationship_changes: vec![],
        }];

        let overheard = check_overhear(&events, LocationId(1), &graph);
        assert!(overheard.is_empty());
    }

    #[test]
    fn test_overhear_multiple_events() {
        let graph = test_graph();
        let events = vec![
            Tier2Event {
                location: LocationId(2),
                participants: vec![NpcId(1)],
                summary: "First event.".to_string(),
                relationship_changes: vec![],
            },
            Tier2Event {
                location: LocationId(3),
                participants: vec![NpcId(2)],
                summary: "Second event.".to_string(),
                relationship_changes: vec![],
            },
            Tier2Event {
                location: LocationId(4), // too far
                participants: vec![NpcId(3)],
                summary: "Third event.".to_string(),
                relationship_changes: vec![],
            },
        ];

        let overheard = check_overhear(&events, LocationId(1), &graph);
        assert_eq!(overheard.len(), 2);
    }

    #[test]
    fn test_overhear_empty_events() {
        let graph = test_graph();
        let overheard = check_overhear(&[], LocationId(1), &graph);
        assert!(overheard.is_empty());
    }
}
