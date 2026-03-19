//! NPC manager — owns all NPCs and assigns cognitive tiers.
//!
//! The `NpcManager` is the central hub for NPC lifecycle: it holds
//! all NPCs, assigns cognitive fidelity tiers based on distance
//! from the player, and provides accessors for querying NPCs
//! by location or id.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::Npc;
use super::schedule::NpcState;
use crate::error::ParishError;
use crate::world::LocationId;
use crate::world::graph::WorldGraph;

use super::NpcId;

/// Cognitive fidelity tier for an NPC.
///
/// Determines how much computational effort is spent simulating
/// an NPC's behavior. Lower tiers = more fidelity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CogTier {
    /// Full LLM inference — NPC is at the same location as the player.
    Tier1,
    /// Lighter LLM inference — NPC is 1-2 edges away.
    Tier2,
    /// Rule-based simulation — NPC is 3+ edges away.
    Tier3,
    /// Schedule-only — NPC is far away, just follows schedule.
    Tier4,
}

/// Manages all NPCs in the game world.
///
/// Handles tier assignment, location queries, and NPC loading.
pub struct NpcManager {
    /// All NPCs keyed by their id.
    npcs: HashMap<NpcId, Npc>,
    /// Current cognitive tier assignment for each NPC.
    tier_assignments: HashMap<NpcId, CogTier>,
}

/// Serialization wrapper for loading NPCs from JSON.
#[derive(Serialize, Deserialize)]
struct NpcFile {
    npcs: Vec<Npc>,
}

impl NpcManager {
    /// Creates a new empty NPC manager.
    pub fn new() -> Self {
        Self {
            npcs: HashMap::new(),
            tier_assignments: HashMap::new(),
        }
    }

    /// Loads NPCs from a JSON file.
    pub fn load_from_file(path: &Path) -> Result<Self, ParishError> {
        let contents = std::fs::read_to_string(path)?;
        Self::load_from_str(&contents)
    }

    /// Loads NPCs from a JSON string.
    pub fn load_from_str(json: &str) -> Result<Self, ParishError> {
        let file: NpcFile = serde_json::from_str(json)?;
        let mut npcs = HashMap::new();
        for npc in file.npcs {
            npcs.insert(npc.id, npc);
        }
        Ok(Self {
            npcs,
            tier_assignments: HashMap::new(),
        })
    }

    /// Adds an NPC to the manager.
    pub fn add(&mut self, npc: Npc) {
        self.npcs.insert(npc.id, npc);
    }

    /// Returns a reference to an NPC by id.
    pub fn get(&self, id: NpcId) -> Option<&Npc> {
        self.npcs.get(&id)
    }

    /// Returns a mutable reference to an NPC by id.
    pub fn get_mut(&mut self, id: NpcId) -> Option<&mut Npc> {
        self.npcs.get_mut(&id)
    }

    /// Returns all NPCs currently at the given location.
    pub fn npcs_at(&self, location: LocationId) -> Vec<&Npc> {
        self.npcs
            .values()
            .filter(|npc| npc.state.is_at(location))
            .collect()
    }

    /// Returns mutable references to all NPCs at the given location.
    pub fn npcs_at_mut(&mut self, location: LocationId) -> Vec<&mut Npc> {
        self.npcs
            .values_mut()
            .filter(|npc| npc.state.is_at(location))
            .collect()
    }

    /// Returns the number of NPCs.
    pub fn count(&self) -> usize {
        self.npcs.len()
    }

    /// Returns all NPC ids.
    pub fn ids(&self) -> Vec<NpcId> {
        self.npcs.keys().copied().collect()
    }

    /// Returns the cognitive tier for an NPC.
    pub fn tier(&self, id: NpcId) -> Option<CogTier> {
        self.tier_assignments.get(&id).copied()
    }

    /// Returns all NPC ids assigned to the given tier.
    pub fn npcs_in_tier(&self, tier: CogTier) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, t)| **t == tier)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Assigns cognitive tiers to all NPCs based on distance from the player.
    ///
    /// - Tier 1: Same location as player
    /// - Tier 2: 1-2 edges away
    /// - Tier 3: 3+ edges away
    /// - Tier 4: Unreachable or very far, or in transit
    pub fn assign_tiers(&mut self, player_location: LocationId, graph: &WorldGraph) {
        self.tier_assignments.clear();

        for (id, npc) in &self.npcs {
            let tier = match &npc.state {
                NpcState::InTransit { .. } => CogTier::Tier4,
                NpcState::Present(npc_loc) => {
                    if *npc_loc == player_location {
                        CogTier::Tier1
                    } else {
                        match graph.shortest_path(player_location, *npc_loc) {
                            Some(path) => {
                                let distance = path.len() - 1; // edges, not nodes
                                match distance {
                                    0 => CogTier::Tier1, // same location
                                    1..=2 => CogTier::Tier2,
                                    _ => CogTier::Tier3,
                                }
                            }
                            None => CogTier::Tier4,
                        }
                    }
                }
            };
            self.tier_assignments.insert(*id, tier);
        }
    }

    /// Returns an iterator over all NPCs.
    pub fn iter(&self) -> impl Iterator<Item = (&NpcId, &Npc)> {
        self.npcs.iter()
    }

    /// Returns a mutable iterator over all NPCs.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&NpcId, &mut Npc)> {
        self.npcs.iter_mut()
    }

    /// Returns the name of an NPC by id, or "Unknown" if not found.
    pub fn name_of(&self, id: NpcId) -> &str {
        self.npcs
            .get(&id)
            .map(|n| n.name.as_str())
            .unwrap_or("Unknown")
    }
}

impl Default for NpcManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::memory::ShortTermMemory;
    use crate::npc::schedule::DailySchedule;
    use crate::world::graph::WorldGraph;

    fn test_npc(id: u32, name: &str, location: LocationId) -> Npc {
        Npc {
            id: NpcId(id),
            name: name.to_string(),
            age: 40,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            mood: "neutral".to_string(),
            home: location,
            workplace: None,
            state: NpcState::Present(location),
            schedule: DailySchedule {
                weekday: vec![],
                weekend: vec![],
                overrides: std::collections::HashMap::new(),
            },
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
        }
    }

    fn test_graph() -> WorldGraph {
        WorldGraph::load_from_str(
            r#"{
            "locations": [
                {
                    "id": 1, "name": "A", "description_template": "A",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "path"},
                        {"target": 3, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 2, "name": "B", "description_template": "B",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "path"},
                        {"target": 4, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 3, "name": "C", "description_template": "C",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 1, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 4, "name": "D", "description_template": "D",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 2, "traversal_minutes": 5, "path_description": "path"},
                        {"target": 5, "traversal_minutes": 5, "path_description": "path"}
                    ]
                },
                {
                    "id": 5, "name": "E", "description_template": "E",
                    "indoor": false, "public": true,
                    "connections": [
                        {"target": 4, "traversal_minutes": 5, "path_description": "path"}
                    ]
                }
            ]
        }"#,
        )
        .unwrap()
    }

    #[test]
    fn test_manager_add_and_get() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(1)));
        assert_eq!(mgr.count(), 1);
        assert_eq!(mgr.get(NpcId(1)).unwrap().name, "Padraig");
    }

    #[test]
    fn test_manager_get_mut() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(1)));
        let npc = mgr.get_mut(NpcId(1)).unwrap();
        npc.mood = "happy".to_string();
        assert_eq!(mgr.get(NpcId(1)).unwrap().mood, "happy");
    }

    #[test]
    fn test_manager_npcs_at() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(1)));
        mgr.add(test_npc(2, "Siobhan", LocationId(1)));
        mgr.add(test_npc(3, "Tommy", LocationId(2)));

        let at_loc1 = mgr.npcs_at(LocationId(1));
        assert_eq!(at_loc1.len(), 2);
        let at_loc2 = mgr.npcs_at(LocationId(2));
        assert_eq!(at_loc2.len(), 1);
        assert_eq!(at_loc2[0].name, "Tommy");
    }

    #[test]
    fn test_tier_assignment_same_location() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(1)));
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier1));
    }

    #[test]
    fn test_tier_assignment_one_edge() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(2))); // 1 edge from loc 1
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier2));
    }

    #[test]
    fn test_tier_assignment_two_edges() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(4))); // 1->2->4 = 2 edges
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier2));
    }

    #[test]
    fn test_tier_assignment_three_edges() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(5))); // 1->2->4->5 = 3 edges
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier3));
    }

    #[test]
    fn test_tier_assignment_in_transit() {
        let mut mgr = NpcManager::new();
        let mut npc = test_npc(1, "Padraig", LocationId(1));
        npc.state = NpcState::InTransit {
            from: LocationId(1),
            to: LocationId(2),
            arrives_at: chrono::Utc::now(),
        };
        mgr.add(npc);
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);
        assert_eq!(mgr.tier(NpcId(1)), Some(CogTier::Tier4));
    }

    #[test]
    fn test_npcs_in_tier() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "A", LocationId(1)));
        mgr.add(test_npc(2, "B", LocationId(1)));
        mgr.add(test_npc(3, "C", LocationId(2)));
        let graph = test_graph();
        mgr.assign_tiers(LocationId(1), &graph);

        let tier1 = mgr.npcs_in_tier(CogTier::Tier1);
        assert_eq!(tier1.len(), 2);
        let tier2 = mgr.npcs_in_tier(CogTier::Tier2);
        assert_eq!(tier2.len(), 1);
    }

    #[test]
    fn test_manager_name_of() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "Padraig", LocationId(1)));
        assert_eq!(mgr.name_of(NpcId(1)), "Padraig");
        assert_eq!(mgr.name_of(NpcId(99)), "Unknown");
    }

    #[test]
    fn test_manager_default() {
        let mgr = NpcManager::default();
        assert_eq!(mgr.count(), 0);
    }

    #[test]
    fn test_manager_ids() {
        let mut mgr = NpcManager::new();
        mgr.add(test_npc(1, "A", LocationId(1)));
        mgr.add(test_npc(2, "B", LocationId(2)));
        let mut ids = mgr.ids();
        ids.sort_by_key(|id| id.0);
        assert_eq!(ids, vec![NpcId(1), NpcId(2)]);
    }
}
