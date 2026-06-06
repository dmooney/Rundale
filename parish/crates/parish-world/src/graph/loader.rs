//! Graph loading and validation — JSON deserialisation, duplicate-id checks,
//! coordinate validation, bidirectionality enforcement, and orphan detection.

use std::collections::HashMap;
use std::path::Path;

use parish_types::ParishError;

use super::schema::{WorldGraph, WorldGraphFile};

impl WorldGraph {
    /// Loads a world graph from a JSON file.
    ///
    /// Validates that all connection targets exist and that connections
    /// are bidirectional.
    pub fn load_from_file(path: &Path) -> Result<Self, ParishError> {
        let contents = std::fs::read_to_string(path)?;
        Self::load_from_str(&contents)
    }

    /// Loads a world graph from a JSON string.
    ///
    /// Validates that all connection targets exist and that connections
    /// are bidirectional.
    pub fn load_from_str(json: &str) -> Result<Self, ParishError> {
        let file: WorldGraphFile = serde_json::from_str(json)?;

        let mut locations = HashMap::new();
        for loc in file.locations {
            if locations.contains_key(&loc.id) {
                return Err(ParishError::WorldGraph(format!(
                    "duplicate location id: {}",
                    loc.id.0
                )));
            }
            loc.validate_coordinates()?;
            locations.insert(loc.id, loc);
        }

        let graph = Self { locations };
        graph.validate()?;
        Ok(graph)
    }

    /// Validates the world graph.
    ///
    /// Checks that:
    /// - All connection targets exist in the graph
    /// - All connections are bidirectional
    /// - There are no orphan nodes (nodes with no connections)
    ///
    /// Called automatically by [`WorldGraph::load_from_str`]; exposed publicly
    /// so the Parish Designer editor can re-run it on an in-memory graph
    /// after edits without reloading the JSON file.
    pub fn validate(&self) -> Result<(), ParishError> {
        for (id, loc) in &self.locations {
            if loc.connections.is_empty() {
                return Err(ParishError::WorldGraph(format!(
                    "orphan location with no connections: {} (id {})",
                    loc.name, id.0
                )));
            }
            for conn in &loc.connections {
                // Check target exists
                if !self.locations.contains_key(&conn.target) {
                    return Err(ParishError::WorldGraph(format!(
                        "location {} (id {}) has connection to non-existent target id {}",
                        loc.name, id.0, conn.target.0
                    )));
                }
                // Check bidirectionality
                let target_loc = &self.locations[&conn.target];
                let has_reverse = target_loc.connections.iter().any(|c| c.target == *id);
                if !has_reverse {
                    return Err(ParishError::WorldGraph(format!(
                        "connection from {} to {} is not bidirectional",
                        loc.name, target_loc.name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Test helper — exposed only to sibling test modules within this crate.
#[cfg(test)]
pub(super) fn test_graph_json() -> &'static str {
    r#"{
        "locations": [
            {
                "id": 1,
                "name": "The Crossroads",
                "description_template": "A quiet crossroads at {time}. The weather is {weather}.",
                "indoor": false,
                "public": true,
                "lat": 53.618,
                "lon": -8.095,
                "connections": [
                    {"target": 2, "path_description": "a short lane"},
                    {"target": 3, "path_description": "a winding boreen"}
                ],
                "associated_npcs": [],
                "mythological_significance": null
            },
            {
                "id": 2,
                "name": "Darcy's Pub",
                "description_template": "The warm interior of Darcy's Pub at {time}.",
                "indoor": true,
                "public": true,
                "lat": 53.6195,
                "lon": -8.0925,
                "connections": [
                    {"target": 1, "path_description": "a short lane back to the crossroads"}
                ],
                "associated_npcs": [],
                "mythological_significance": null,
                "aliases": ["tavern", "the pub"]
            },
            {
                "id": 3,
                "name": "St. Brigid's Church",
                "description_template": "The old stone church stands in {weather} {time} light.",
                "indoor": false,
                "public": true,
                "lat": 53.6215,
                "lon": -8.099,
                "connections": [
                    {"target": 1, "path_description": "the boreen back to the crossroads"},
                    {"target": 4, "path_description": "a path through the graveyard"}
                ],
                "associated_npcs": [],
                "mythological_significance": null,
                "aliases": ["church", "chapel"]
            },
            {
                "id": 4,
                "name": "The Fairy Fort",
                "description_template": "An ancient ring fort on the hill. {weather}.",
                "indoor": false,
                "public": true,
                "lat": 53.627,
                "lon": -8.052,
                "connections": [
                    {"target": 3, "path_description": "the path back past the church"}
                ],
                "associated_npcs": [],
                "mythological_significance": "A rath said to be home to the sídhe. Locals avoid it after dark.",
                "aliases": ["rath", "ring fort", "the rath"]
            }
        ]
    }"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::{GeoKind, Hazard, RelativeRef};
    use parish_types::LocationId;

    #[test]
    fn test_load_from_str() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert_eq!(graph.location_count(), 4);
    }

    #[test]
    fn test_get_location() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let loc = graph.get(LocationId(1)).unwrap();
        assert_eq!(loc.name, "The Crossroads");
        assert!(!loc.indoor);
        assert!(loc.public);
    }

    #[test]
    fn test_get_nonexistent() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(graph.get(LocationId(99)).is_none());
    }

    #[test]
    fn test_geo_metadata_defaults_to_fictional_without_source_or_relative_anchor() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let crossroads = graph.get(LocationId(1)).unwrap();

        assert_eq!(crossroads.geo_kind, GeoKind::Fictional);
        assert!(crossroads.relative_to.is_none());
        assert!(crossroads.geo_source.is_none());

        let conn = graph
            .connection_between(LocationId(1), LocationId(2))
            .unwrap();
        assert_eq!(conn.hazard, Hazard::None);
    }

    #[test]
    fn test_geo_metadata_round_trips_explicit_fields() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "Anchor",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6,
                    "lon": -8.1,
                    "geo_kind": "manual",
                    "geo_source": "OS 6-inch First Edition",
                    "connections": [
                        {"target": 2, "path_description": "path", "hazard": "flood"}
                    ]
                },
                {
                    "id": 2,
                    "name": "Relative",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6009,
                    "lon": -8.1003,
                    "geo_kind": "fictional",
                    "relative_to": {
                        "anchor": 1,
                        "dnorth_m": 100.0,
                        "deast_m": -20.0
                    },
                    "connections": [
                        {"target": 1, "path_description": "path", "hazard": "flood"}
                    ]
                }
            ]
        }"#;

        let graph = WorldGraph::load_from_str(json).unwrap();
        let anchor = graph.get(LocationId(1)).unwrap();
        assert_eq!(anchor.geo_kind, GeoKind::Manual);
        assert_eq!(
            anchor.geo_source.as_deref(),
            Some("OS 6-inch First Edition")
        );

        let relative = graph.get(LocationId(2)).unwrap();
        assert_eq!(relative.geo_kind, GeoKind::Fictional);
        assert_eq!(
            relative.relative_to,
            Some(RelativeRef {
                anchor: LocationId(1),
                dnorth_m: 100.0,
                deast_m: -20.0,
            })
        );

        assert_eq!(
            graph
                .connection_between(LocationId(1), LocationId(2))
                .unwrap()
                .hazard,
            Hazard::Flood
        );
    }

    #[test]
    fn test_validation_missing_target() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 99, "path_description": "path"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("non-existent"));
    }

    #[test]
    fn test_validation_not_bidirectional() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                },
                {
                    "id": 3,
                    "name": "C",
                    "description_template": "C",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not bidirectional"));
    }

    #[test]
    fn test_validation_orphan_node() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": []
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("orphan"));
    }

    #[test]
    fn test_validation_duplicate_id() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "loop"}]
                },
                {
                    "id": 1,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "loop"}]
                }
            ]
        }"#;
        let result = WorldGraph::load_from_str(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn test_validation_rejects_invalid_latitude() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "lat": 90.1,
                    "lon": -8.1,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6,
                    "lon": -8.1,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;

        let err = WorldGraph::load_from_str(json).unwrap_err().to_string();
        assert!(err.contains("invalid latitude"), "{err}");
        assert!(err.contains("90.1"), "{err}");
    }

    #[test]
    fn test_validation_rejects_invalid_longitude() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "A",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "lat": 53.6,
                    "lon": -180.1,
                    "connections": [{"target": 2, "path_description": "path"}]
                },
                {
                    "id": 2,
                    "name": "B",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "lat": 53.7,
                    "lon": -8.1,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;

        let err = WorldGraph::load_from_str(json).unwrap_err().to_string();
        assert!(err.contains("invalid longitude"), "{err}");
        assert!(err.contains("-180.1"), "{err}");
    }
}
