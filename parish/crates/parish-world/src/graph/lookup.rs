//! Graph lookup and fuzzy name matching — `get`, `neighbors`,
//! `find_by_name`, `find_by_name_with_config`, `connection_between`,
//! `location_count`, and `location_ids`.

use strsim::jaro_winkler;

use parish_config::WorldConfig;
use parish_types::LocationId;

use super::schema::{Connection, LocationData, WorldGraph};

/// Priority level for name matching, ordered from best match to worst.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchLevel {
    ExactAlias,
    QueryInName,
    QueryInAlias,
    NameInQuery,
    AliasInQuery,
    ArticleName,
    ArticleAlias,
    None,
}

/// Strips common English articles from the beginning of a string.
fn strip_articles(s: &str) -> String {
    let trimmed = s.trim();
    for article in &["the ", "a ", "an "] {
        if let Some(rest) = trimmed.strip_prefix(article) {
            return rest.to_string();
        }
    }
    trimmed.to_string()
}

impl WorldGraph {
    /// Returns a reference to a location by id.
    pub fn get(&self, id: LocationId) -> Option<&LocationData> {
        self.locations.get(&id)
    }

    /// Returns all neighbors of a location with their connections.
    pub fn neighbors(&self, id: LocationId) -> Vec<(LocationId, &Connection)> {
        match self.locations.get(&id) {
            Some(loc) => loc
                .connections
                .iter()
                .map(|conn| (conn.target, conn))
                .collect(),
            None => Vec::new(),
        }
    }

    /// Finds a location by name using case-insensitive fuzzy matching.
    ///
    /// Matching priority (name matches beat alias matches at each level):
    /// 1. Exact name → exact alias
    /// 2. Query in name → query in alias
    /// 3. Name in query → alias in query
    /// 4. Article-stripped name → article-stripped alias
    /// 5. Jaro-Winkler fuzzy score (catches typos and near-misses)
    ///
    /// Common articles ("the", "a", "an") are stripped for fuzzy matching.
    pub fn find_by_name(&self, name: &str) -> Option<LocationId> {
        self.find_by_name_with_config(name, &WorldConfig::default())
    }

    /// Finds a location by name using case-insensitive fuzzy matching,
    /// with a configurable fuzzy threshold from [`WorldConfig`].
    ///
    /// Performance: single pass through all locations, computing `to_lowercase()`
    /// once per name/alias instead of up to 8× in separate priority-level scans.
    /// Fuzzy scores are also computed in the same pass to avoid a redundant 9th scan.
    pub fn find_by_name_with_config(&self, name: &str, config: &WorldConfig) -> Option<LocationId> {
        let lower = name.to_lowercase();
        let stripped = strip_articles(&lower);
        let do_article_strip = stripped != lower;

        let mut best: Option<(MatchLevel, LocationId)> = None;
        let mut best_fuzzy: Option<(f64, LocationId)> = None;

        for (id, loc) in &self.locations {
            let loc_lower = loc.name.to_lowercase();

            if loc_lower == lower {
                return Some(*id);
            }

            let aliases_lower: Vec<String> = loc.aliases.iter().map(|a| a.to_lowercase()).collect();

            let level = if aliases_lower.contains(&lower) {
                MatchLevel::ExactAlias
            } else if loc_lower.contains(&lower) {
                MatchLevel::QueryInName
            } else if aliases_lower.iter().any(|a| a.contains(&lower)) {
                MatchLevel::QueryInAlias
            } else if lower.contains(loc_lower.as_str()) {
                MatchLevel::NameInQuery
            } else if aliases_lower.iter().any(|a| lower.contains(a.as_str())) {
                MatchLevel::AliasInQuery
            } else if do_article_strip {
                let loc_stripped = strip_articles(&loc_lower);
                if loc_stripped.contains(&stripped) || stripped.contains(loc_stripped.as_str()) {
                    MatchLevel::ArticleName
                } else if aliases_lower.iter().any(|a| {
                    let a_stripped = strip_articles(a);
                    a_stripped.contains(&stripped) || stripped.contains(a_stripped.as_str())
                }) {
                    MatchLevel::ArticleAlias
                } else {
                    MatchLevel::None
                }
            } else {
                MatchLevel::None
            };

            if level != MatchLevel::None {
                if best.as_ref().is_none_or(|(best_lvl, _)| level < *best_lvl) {
                    best = Some((level, *id));
                }
            } else {
                let name_score = jaro_winkler(&loc_lower, &stripped);
                let alias_score = aliases_lower
                    .iter()
                    .map(|a| jaro_winkler(a, &stripped))
                    .fold(0.0_f64, f64::max);
                let max_score = name_score.max(alias_score);
                if max_score > best_fuzzy.as_ref().map_or(0.0, |(s, _)| *s) {
                    best_fuzzy = Some((max_score, *id));
                }
            }
        }

        best.map(|(_, id)| id).or_else(|| {
            best_fuzzy
                .filter(|(score, _)| *score >= config.fuzzy_threshold)
                .map(|(_, id)| id)
        })
    }

    /// Returns the connection from one location to another, if they are neighbors.
    pub fn connection_between(&self, from: LocationId, to: LocationId) -> Option<&Connection> {
        self.locations
            .get(&from)?
            .connections
            .iter()
            .find(|c| c.target == to)
    }

    /// Returns the number of locations in the graph.
    pub fn location_count(&self) -> usize {
        self.locations.len()
    }

    /// Returns all location ids in the graph.
    pub fn location_ids(&self) -> Vec<LocationId> {
        self.locations.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::loader::test_graph_json;

    #[test]
    fn test_neighbors() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let neighbors = graph.neighbors(LocationId(1));
        assert_eq!(neighbors.len(), 2);

        let target_ids: Vec<LocationId> = neighbors.iter().map(|(id, _)| *id).collect();
        assert!(target_ids.contains(&LocationId(2)));
        assert!(target_ids.contains(&LocationId(3)));
    }

    #[test]
    fn test_neighbors_empty() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let neighbors = graph.neighbors(LocationId(99));
        assert!(neighbors.is_empty());
    }

    #[test]
    fn test_find_by_name_exact() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("Darcy's Pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("darcy's pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_partial() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_church() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("church").unwrap();
        assert_eq!(id, LocationId(3));
    }

    #[test]
    fn test_find_by_name_not_found() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(graph.find_by_name("castle").is_none());
    }

    #[test]
    fn test_find_by_name_alias_exact() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("rath").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_substring() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "ring" is a substring of alias "ring fort"
        let id = graph.find_by_name("ring").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_with_article() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("the rath").unwrap();
        assert_eq!(id, LocationId(4));
    }

    #[test]
    fn test_find_by_name_alias_tavern() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("tavern").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_find_by_name_prefers_name_over_alias() {
        // "pub" is both a substring of the name "Darcy's Pub" (level 2)
        // and an alias. Name match (level 2) should win.
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let id = graph.find_by_name("pub").unwrap();
        assert_eq!(id, LocationId(2));
    }

    #[test]
    fn test_aliases_default_empty() {
        // Location 1 has no aliases field — should default to empty vec
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let crossroads = graph.get(LocationId(1)).unwrap();
        assert!(crossroads.aliases.is_empty());
    }

    #[test]
    fn test_find_by_name_fuzzy_typo() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "churh" is a typo for "church" — Jaro-Winkler should catch it
        let id = graph.find_by_name("churh").unwrap();
        assert_eq!(id, LocationId(3));
    }

    #[test]
    fn test_find_by_name_fuzzy_no_false_positive() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        // "xyz" is nothing close to any location — should not match
        assert!(graph.find_by_name("xyz").is_none());
    }

    #[test]
    fn test_connection_between() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let conn = graph
            .connection_between(LocationId(1), LocationId(2))
            .unwrap();
        assert_eq!(conn.path_description, "a short lane");
    }

    #[test]
    fn test_connection_between_none() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        assert!(
            graph
                .connection_between(LocationId(1), LocationId(4))
                .is_none()
        );
    }

    #[test]
    fn test_mythological_significance() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let fort = graph.get(LocationId(4)).unwrap();
        assert!(fort.mythological_significance.is_some());
        assert!(
            fort.mythological_significance
                .as_ref()
                .unwrap()
                .contains("sídhe")
        );

        let crossroads = graph.get(LocationId(1)).unwrap();
        assert!(crossroads.mythological_significance.is_none());
    }

    #[test]
    fn test_location_ids() {
        let graph = WorldGraph::load_from_str(test_graph_json()).unwrap();
        let ids = graph.location_ids();
        assert_eq!(ids.len(), 4);
    }
}
