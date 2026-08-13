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
    /// - Every `relative_to.anchor` resolves to an existing location and is
    ///   not a self-reference (TD-002)
    /// - No alias collides (case-insensitively) with another location's alias
    ///   or with a *different* location's canonical name, which would make
    ///   fuzzy name lookup ambiguous (TD-002)
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
        self.validate_relative_anchors()?;
        self.validate_aliases()?;
        Ok(())
    }

    /// Validates that every `relative_to.anchor` points at an existing
    /// location and never at itself (a self-anchor cannot be resolved and a
    /// dangling anchor silently breaks coordinate realignment). TD-002.
    fn validate_relative_anchors(&self) -> Result<(), ParishError> {
        for (id, loc) in &self.locations {
            let Some(rel) = loc.relative_to.as_ref() else {
                continue;
            };
            if rel.anchor == *id {
                return Err(ParishError::WorldGraph(format!(
                    "location {} (id {}) declares relative_to anchored at itself",
                    loc.name, id.0
                )));
            }
            if !self.locations.contains_key(&rel.anchor) {
                return Err(ParishError::WorldGraph(format!(
                    "location {} (id {}) is positioned relative_to non-existent anchor id {}",
                    loc.name, id.0, rel.anchor.0
                )));
            }
        }
        Ok(())
    }

    /// Validates that aliases do not introduce ambiguous fuzzy-lookup targets.
    ///
    /// Two failure modes are rejected (case-insensitive, whitespace-trimmed):
    /// - the same alias appears on two different locations, and
    /// - an alias on one location equals a *different* location's canonical
    ///   name (so typing that name would fuzzy-match two locations).
    ///
    /// TD-002.
    fn validate_aliases(&self) -> Result<(), ParishError> {
        use std::collections::HashMap;

        // Map canonical-name (lowercased) -> owning location id, for the
        // alias-shadows-name check.
        let mut name_owner: HashMap<String, parish_types::LocationId> = HashMap::new();
        for (id, loc) in &self.locations {
            name_owner.insert(loc.name.trim().to_lowercase(), *id);
        }

        // Track which location first claimed each alias.
        let mut alias_owner: HashMap<String, parish_types::LocationId> = HashMap::new();
        // Iterate in stable id order so error messages are deterministic.
        let mut ids: Vec<parish_types::LocationId> = self.locations.keys().copied().collect();
        ids.sort_by_key(|id| id.0);
        for id in ids {
            let loc = &self.locations[&id];
            for alias in &loc.aliases {
                let key = alias.trim().to_lowercase();
                if key.is_empty() {
                    continue;
                }
                if let Some(prev) = alias_owner.get(&key)
                    && *prev != id
                {
                    return Err(ParishError::WorldGraph(format!(
                        "alias {:?} is claimed by both location id {} and id {} — \
                         fuzzy lookup would be ambiguous",
                        alias, prev.0, id.0
                    )));
                }
                if let Some(owner) = name_owner.get(&key)
                    && *owner != id
                {
                    return Err(ParishError::WorldGraph(format!(
                        "alias {:?} on location id {} collides with the canonical name \
                         of location id {}",
                        alias, id.0, owner.0
                    )));
                }
                alias_owner.insert(key, id);
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
        assert!(loc.landmarks.is_empty(), "legacy mods default landmarks");
    }

    #[test]
    fn test_loads_structured_authored_landmarks() {
        let json = test_graph_json().replace(
            "\"description_template\": \"A quiet crossroads at {time}. The weather is {weather}.\"",
            "\"description_template\": \"A quiet crossroads at {time}. The weather is {weather}.\", \"landmarks\": [\"old stone bridge\", \"well\"]",
        );
        let graph = WorldGraph::load_from_str(&json).unwrap();
        assert_eq!(
            graph.get(LocationId(1)).unwrap().landmarks,
            vec!["old stone bridge".to_string(), "well".to_string()]
        );
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

    // ── TD-002: relative_to + alias cross-validation ─────────────────────────

    /// Two-location graph helper with overridable alias / relative_to JSON on
    /// the second node, for the TD-002 validation tests.
    fn two_loc_json(loc2_extra: &str) -> String {
        format!(
            r#"{{
                "locations": [
                    {{
                        "id": 1,
                        "name": "Anchor",
                        "description_template": "A",
                        "indoor": false,
                        "public": true,
                        "lat": 53.6,
                        "lon": -8.1,
                        "connections": [{{"target": 2, "path_description": "path"}}]
                    }},
                    {{
                        "id": 2,
                        "name": "Second",
                        "description_template": "B",
                        "indoor": false,
                        "public": true,
                        "lat": 53.61,
                        "lon": -8.11,
                        "connections": [{{"target": 1, "path_description": "path"}}]
                        {loc2_extra}
                    }}
                ]
            }}"#
        )
    }

    // AC-11: a relative_to anchored at a non-existent location is rejected.
    #[test]
    fn relative_to_dangling_anchor_is_rejected() {
        let json =
            two_loc_json(r#", "relative_to": {"anchor": 99, "dnorth_m": 10.0, "deast_m": 5.0}"#);
        let err = WorldGraph::load_from_str(&json).unwrap_err().to_string();
        assert!(err.contains("non-existent anchor"), "{err}");
        assert!(err.contains("99"), "{err}");
    }

    // AC-11: a relative_to anchored at the location itself is rejected.
    #[test]
    fn relative_to_self_anchor_is_rejected() {
        let json =
            two_loc_json(r#", "relative_to": {"anchor": 2, "dnorth_m": 10.0, "deast_m": 5.0}"#);
        let err = WorldGraph::load_from_str(&json).unwrap_err().to_string();
        assert!(err.contains("anchored at itself"), "{err}");
    }

    // AC-11: a valid relative_to anchor loads fine.
    #[test]
    fn relative_to_valid_anchor_loads() {
        let json =
            two_loc_json(r#", "relative_to": {"anchor": 1, "dnorth_m": 10.0, "deast_m": 5.0}"#);
        assert!(WorldGraph::load_from_str(&json).is_ok());
    }

    // AC-12: the same alias on two locations is rejected (ambiguous lookup).
    #[test]
    fn duplicate_alias_across_locations_is_rejected() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "Anchor",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}],
                    "aliases": ["the spot"]
                },
                {
                    "id": 2,
                    "name": "Second",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}],
                    "aliases": ["The Spot"]
                }
            ]
        }"#;
        let err = WorldGraph::load_from_str(json).unwrap_err().to_string();
        assert!(err.contains("ambiguous"), "{err}");
    }

    // AC-12: an alias that shadows a *different* location's canonical name is
    // rejected (case-insensitive).
    #[test]
    fn alias_colliding_with_other_canonical_name_is_rejected() {
        let json = r#"{
            "locations": [
                {
                    "id": 1,
                    "name": "Anchor",
                    "description_template": "A",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 2, "path_description": "path"}],
                    "aliases": ["second"]
                },
                {
                    "id": 2,
                    "name": "Second",
                    "description_template": "B",
                    "indoor": false,
                    "public": true,
                    "connections": [{"target": 1, "path_description": "path"}]
                }
            ]
        }"#;
        let err = WorldGraph::load_from_str(json).unwrap_err().to_string();
        assert!(err.contains("collides with the canonical name"), "{err}");
    }

    // AC-12: an alias equal to the owner's own canonical name is harmless.
    #[test]
    fn alias_equal_to_own_name_is_allowed() {
        let json = two_loc_json(r#", "aliases": ["Second", "second place"]"#);
        assert!(WorldGraph::load_from_str(&json).is_ok());
    }
}
