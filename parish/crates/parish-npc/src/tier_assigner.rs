//! Pure helpers backing [`crate::manager::NpcManager::assign_tiers`].
//!
//! This module owns the side-effect-free steps of tier assignment:
//!
//! - BFS over the world graph from the player's location.
//! - Mapping a graph distance to a [`CogTier`].
//! - Comparing tier ranks (lower rank == closer to the player).
//! - Computing the per-NPC distance, taking in-transit state into account.
//! - Iterating all NPCs and producing a list of tier changes.
//!
//! All side-effects (memory inflation, deflated summaries, event publication,
//! `tier_assignments` mutation) live in `NpcManager::apply_tier_changes`.
//! See GitHub issue #697.
//!
//! ## Why split this out?
//!
//! `assign_tiers` is called every player tick. Keeping the pure decision logic
//! in its own module makes it cheap to unit test and to reason about: BFS
//! topology, tier-boundary classification, and tier-rank ordering can all be
//! exercised without constructing an `NpcManager` or a full `WorldState`.

use std::collections::{HashMap, VecDeque};

use parish_config::CognitiveTierConfig;
use parish_types::{LocationId, NpcId};
use parish_world::graph::WorldGraph;

use crate::Npc;
use crate::types::{CogTier, NpcState};

/// Old-vs-new tier pair for an NPC whose assignment changed during a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TierChange {
    /// Identifier of the NPC whose tier changed.
    pub npc_id: NpcId,
    /// Tier the NPC held before this pass.
    pub old_tier: CogTier,
    /// Tier the NPC holds after this pass.
    pub new_tier: CogTier,
}

/// Computes BFS distances from `source` to every reachable location in `graph`.
///
/// Unreachable nodes are simply absent from the returned map.
pub fn bfs_distances(source: LocationId, graph: &WorldGraph) -> HashMap<LocationId, u32> {
    let mut distances: HashMap<LocationId, u32> = HashMap::new();
    let mut queue: VecDeque<LocationId> = VecDeque::new();

    distances.insert(source, 0);
    queue.push_back(source);

    while let Some(current) = queue.pop_front() {
        let current_dist = distances[&current];
        for (neighbor, _) in graph.neighbors(current) {
            if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor) {
                e.insert(current_dist + 1);
                queue.push_back(neighbor);
            }
        }
    }

    distances
}

/// Maps a graph distance to a [`CogTier`] using the supplied thresholds.
///
/// `None` (unreachable) and any distance beyond `tier3_max_distance` collapse
/// to [`CogTier::Tier4`].
pub fn tier_for_distance(distance: Option<u32>, config: &CognitiveTierConfig) -> CogTier {
    match distance {
        Some(d) if d <= config.tier1_max_distance => CogTier::Tier1,
        Some(d) if d <= config.tier2_max_distance => CogTier::Tier2,
        Some(d) if d <= config.tier3_max_distance => CogTier::Tier3,
        _ => CogTier::Tier4,
    }
}

/// Maps a [`CogTier`] to a numeric rank for comparison.
///
/// Lower rank means closer to the player and therefore higher cognitive
/// fidelity. Used to detect promotions vs. demotions during tier assignment.
pub fn tier_rank(tier: CogTier) -> u8 {
    match tier {
        CogTier::Tier1 => 1,
        CogTier::Tier2 => 2,
        CogTier::Tier3 => 3,
        CogTier::Tier4 => 4,
    }
}

/// Returns the BFS distance to use when classifying `npc`.
///
/// `Present` NPCs simply look up their current location. NPCs `InTransit`
/// take the closer of the origin and destination — this avoids spurious
/// promotions/demotions while a long walk is in progress and keeps the
/// classification stable as the player moves around the path.
pub fn npc_distance(npc: &Npc, distances: &HashMap<LocationId, u32>) -> Option<u32> {
    match npc.state {
        NpcState::Present => distances.get(&npc.location).copied(),
        NpcState::InTransit { from, to, .. } => {
            let d_from = distances.get(&from).copied();
            let d_to = distances.get(&to).copied();
            match (d_from, d_to) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::Npc;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::types::{Intelligence, NpcState};

    /// Walks all NPCs and computes their new tier given the BFS distances from
    /// the player.
    ///
    /// Returns one [`TierChange`] per NPC whose tier differs from
    /// `current_assignments`. NPCs whose tier is unchanged are omitted. NPCs
    /// that have never been assigned default to [`CogTier::Tier4`].
    ///
    /// Production code uses a single-pass inline loop in `assign_tiers`; this
    /// helper exists only to keep the unit tests self-contained.
    fn compute_tier_changes(
        npcs: &HashMap<NpcId, Npc>,
        distances: &HashMap<LocationId, u32>,
        current_assignments: &HashMap<NpcId, CogTier>,
        config: &CognitiveTierConfig,
    ) -> Vec<TierChange> {
        let mut changes = Vec::new();
        for npc in npcs.values() {
            let distance = npc_distance(npc, distances);
            let new_tier = tier_for_distance(distance, config);
            let old_tier = current_assignments
                .get(&npc.id)
                .copied()
                .unwrap_or(CogTier::Tier4);
            if new_tier != old_tier {
                changes.push(TierChange {
                    npc_id: npc.id,
                    old_tier,
                    new_tier,
                });
            }
        }
        changes
    }

    fn make_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test".to_string(),
            intelligence: Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
            deflated_summary: None,
            reaction_log: crate::reactions::ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    fn chain_graph(n: u32) -> WorldGraph {
        let locations: Vec<serde_json::Value> = (0..=n)
            .map(|i| {
                let mut conns = Vec::new();
                if i > 0 {
                    conns.push(serde_json::json!({
                        "target": i - 1,
                        "path_description": "a path"
                    }));
                }
                if i < n {
                    conns.push(serde_json::json!({
                        "target": i + 1,
                        "path_description": "a path"
                    }));
                }
                serde_json::json!({
                    "id": i,
                    "name": format!("Loc {}", i),
                    "description_template": "Test",
                    "indoor": false,
                    "public": true,
                    "connections": conns
                })
            })
            .collect();
        let json = serde_json::json!({"locations": locations}).to_string();
        WorldGraph::load_from_str(&json).unwrap()
    }

    #[test]
    fn bfs_visits_every_node_in_a_chain() {
        let graph = chain_graph(4);
        let distances = bfs_distances(LocationId(0), &graph);
        for i in 0..=4u32 {
            assert_eq!(distances.get(&LocationId(i)).copied(), Some(i));
        }
    }

    #[test]
    fn bfs_unreachable_nodes_are_absent() {
        // Build a small chain so the graph passes orphan-location validation,
        // then probe a location id that is not part of the graph.
        let graph = chain_graph(2);
        let distances = bfs_distances(LocationId(0), &graph);
        assert_eq!(distances.get(&LocationId(0)).copied(), Some(0));
        assert_eq!(distances.get(&LocationId(1)).copied(), Some(1));
        // A location id that doesn't exist in the graph must be absent.
        assert_eq!(distances.get(&LocationId(99)), None);
    }

    #[test]
    fn tier_for_distance_classifies_each_band() {
        let cfg = CognitiveTierConfig::default();
        // Default boundaries: tier1=0, tier2<=2, tier3<=5, tier4 otherwise.
        assert_eq!(tier_for_distance(Some(0), &cfg), CogTier::Tier1);
        assert_eq!(tier_for_distance(Some(1), &cfg), CogTier::Tier2);
        assert_eq!(tier_for_distance(Some(2), &cfg), CogTier::Tier2);
        assert_eq!(tier_for_distance(Some(3), &cfg), CogTier::Tier3);
        assert_eq!(tier_for_distance(Some(5), &cfg), CogTier::Tier3);
        assert_eq!(tier_for_distance(Some(6), &cfg), CogTier::Tier4);
        // None (unreachable) collapses to Tier 4 — matches the legacy behaviour.
        assert_eq!(tier_for_distance(None, &cfg), CogTier::Tier4);
    }

    #[test]
    fn tier_rank_orders_tiers_by_proximity() {
        // Lower rank = closer to player = higher fidelity.
        assert!(tier_rank(CogTier::Tier1) < tier_rank(CogTier::Tier2));
        assert!(tier_rank(CogTier::Tier2) < tier_rank(CogTier::Tier3));
        assert!(tier_rank(CogTier::Tier3) < tier_rank(CogTier::Tier4));
        // Concrete values match the historical contract.
        assert_eq!(tier_rank(CogTier::Tier1), 1);
        assert_eq!(tier_rank(CogTier::Tier4), 4);
    }

    #[test]
    fn npc_distance_uses_present_location() {
        let mut distances = HashMap::new();
        distances.insert(LocationId(5), 3u32);
        let npc = make_npc(1, 5);
        assert_eq!(npc_distance(&npc, &distances), Some(3));
    }

    #[test]
    fn npc_distance_in_transit_takes_minimum_of_endpoints() {
        let mut distances = HashMap::new();
        distances.insert(LocationId(5), 3u32);
        distances.insert(LocationId(6), 7u32);
        let mut npc = make_npc(1, 5);
        npc.state = NpcState::InTransit {
            from: LocationId(5),
            to: LocationId(6),
            arrives_at: chrono::Utc::now(),
        };
        assert_eq!(npc_distance(&npc, &distances), Some(3));
    }

    #[test]
    fn compute_tier_changes_emits_only_diffs() {
        let cfg = CognitiveTierConfig::default();
        let graph = chain_graph(6);
        let distances = bfs_distances(LocationId(0), &graph);

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(10), make_npc(10, 0)); // dist 0 → Tier 1
        npcs.insert(NpcId(11), make_npc(11, 6)); // dist 6 → Tier 4

        // Pretend NpcId(10) was already Tier 1 before this pass — no change.
        // NpcId(11) was Tier 1 before — must report a demotion to Tier 4.
        let mut prev = HashMap::new();
        prev.insert(NpcId(10), CogTier::Tier1);
        prev.insert(NpcId(11), CogTier::Tier1);

        let changes = compute_tier_changes(&npcs, &distances, &prev, &cfg);
        assert_eq!(changes.len(), 1);
        let change = changes[0];
        assert_eq!(change.npc_id, NpcId(11));
        assert_eq!(change.old_tier, CogTier::Tier1);
        assert_eq!(change.new_tier, CogTier::Tier4);
    }

    #[test]
    fn compute_tier_changes_treats_missing_assignment_as_tier4() {
        // Matches the legacy `tier_assignments.get(&id).copied().unwrap_or(Tier4)`.
        let cfg = CognitiveTierConfig::default();
        let graph = chain_graph(2);
        let distances = bfs_distances(LocationId(0), &graph);

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(10), make_npc(10, 0)); // dist 0 → Tier 1

        let prev: HashMap<NpcId, CogTier> = HashMap::new();
        let changes = compute_tier_changes(&npcs, &distances, &prev, &cfg);
        // Missing → defaulted to Tier 4; new tier is Tier 1; so change must be reported.
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].old_tier, CogTier::Tier4);
        assert_eq!(changes[0].new_tier, CogTier::Tier1);
    }
}
