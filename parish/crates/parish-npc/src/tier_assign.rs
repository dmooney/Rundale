//! Cognitive tier assignment — BFS distance computation and tier transitions.
//!
//! Extracted from `NpcManager` so tier-assignment logic and its tests live in
//! one place. `NpcManager::assign_tiers` is a thin wrapper around [`assign_tiers`].

use std::collections::{HashMap, VecDeque};

use crate::transitions::{deflate_npc_state, inflate_npc_context};
use crate::types::CogTier;
use crate::{Npc, NpcId};
use parish_config::CognitiveTierConfig;
use parish_types::LocationId;
use parish_world::WorldState;
use parish_world::events::GameEvent;
use parish_world::graph::WorldGraph;

/// A tier transition that occurred during [`assign_tiers`].
#[derive(Debug, Clone)]
pub struct TierTransition {
    /// Which NPC changed tier.
    pub npc_id: NpcId,
    /// Name of the NPC.
    pub npc_name: String,
    /// Previous cognitive tier.
    pub old_tier: CogTier,
    /// New cognitive tier.
    pub new_tier: CogTier,
    /// Whether this was a promotion (closer to player).
    pub promoted: bool,
}

/// Assigns cognitive tiers to all NPCs based on BFS distance from the player.
///
/// Writes the new tier map into `tier_assignments`. Performs inflation on
/// promotion and deflation on demotion. Publishes `NpcArrived` on the world
/// event bus for every NPC entering Tier 1.
///
/// `bfs_cache` is keyed by the player's location; passing the same location
/// twice reuses the cached distances (the world graph is immutable during a
/// session). Set it to `None` to force a recompute — e.g. after a graph
/// hot-reload.
pub fn assign_tiers(
    npcs: &mut HashMap<NpcId, Npc>,
    tier_assignments: &mut HashMap<NpcId, CogTier>,
    bfs_cache: &mut Option<(LocationId, HashMap<LocationId, u32>)>,
    world: &WorldState,
    recent_events: &[GameEvent],
) -> Vec<TierTransition> {
    let player_location = world.player_location;
    let graph = &world.graph;
    let game_time = world.clock.now();
    let config = CognitiveTierConfig::default();

    // Reuse cached BFS distances when the player hasn't moved.
    let cache_hit = bfs_cache
        .as_ref()
        .is_some_and(|(loc, _)| *loc == player_location);
    if !cache_hit {
        let distances = bfs_distances(player_location, graph);
        *bfs_cache = Some((player_location, distances));
    }
    let distances = &bfs_cache.as_ref().expect("cache populated above").1;

    // First pass: detect tier changes.
    let mut changes: Vec<(NpcId, CogTier, CogTier)> = Vec::new();
    for npc in npcs.values() {
        use crate::types::NpcState;
        let distance = match npc.state {
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
        };

        let new_tier = match distance {
            Some(d) if d <= config.tier1_max_distance => CogTier::Tier1,
            Some(d) if d <= config.tier2_max_distance => CogTier::Tier2,
            Some(d) if d <= config.tier3_max_distance => CogTier::Tier3,
            _ => CogTier::Tier4,
        };
        let old_tier = tier_assignments
            .get(&npc.id)
            .copied()
            .unwrap_or(CogTier::Tier4);

        if new_tier != old_tier {
            changes.push((npc.id, old_tier, new_tier));
        }
        tier_assignments.insert(npc.id, new_tier);
    }

    // Second pass: handle transitions — single get_mut per NPC to avoid redundant lookups.
    let mut transitions = Vec::new();
    for (npc_id, old_tier, new_tier) in &changes {
        let promoted = tier_rank(*new_tier) < tier_rank(*old_tier);
        let demoted = tier_rank(*new_tier) > tier_rank(*old_tier);

        let (npc_name, tier1_location) = if let Some(npc) = npcs.get_mut(npc_id) {
            if promoted {
                inflate_npc_context(npc, recent_events, game_time);
                tracing::debug!(
                    npc_id = npc_id.0,
                    ?old_tier,
                    ?new_tier,
                    "NPC promoted (inflated)"
                );
            }
            if demoted {
                let summary = deflate_npc_state(npc, recent_events);
                npc.deflated_summary = Some(summary);
                tracing::debug!(
                    npc_id = npc_id.0,
                    ?old_tier,
                    ?new_tier,
                    "NPC demoted (deflated)"
                );
            }
            let tier1_loc = (*new_tier == CogTier::Tier1 && *old_tier != CogTier::Tier1)
                .then_some(npc.location);
            (npc.name.clone(), tier1_loc)
        } else {
            (String::new(), None)
        };

        if let Some(location) = tier1_location {
            world.event_bus.publish(GameEvent::NpcArrived {
                npc_id: *npc_id,
                location,
                timestamp: game_time,
            });
        }

        transitions.push(TierTransition {
            npc_id: *npc_id,
            npc_name,
            old_tier: *old_tier,
            new_tier: *new_tier,
            promoted,
        });
    }

    tracing::debug!(
        player_location = player_location.0,
        tier1 = tier_assignments
            .values()
            .filter(|t| **t == CogTier::Tier1)
            .count(),
        tier2 = tier_assignments
            .values()
            .filter(|t| **t == CogTier::Tier2)
            .count(),
        transitions = transitions.len(),
        "Tier assignment complete"
    );

    transitions
}

/// BFS distances from `source` to all reachable locations.
fn bfs_distances(source: LocationId, graph: &WorldGraph) -> HashMap<LocationId, u32> {
    let mut distances: HashMap<LocationId, u32> = HashMap::new();
    let mut queue: VecDeque<LocationId> = VecDeque::new();
    distances.insert(source, 0);
    queue.push_back(source);
    while let Some(current) = queue.pop_front() {
        let d = distances[&current];
        for (neighbor, _) in graph.neighbors(current) {
            if let std::collections::hash_map::Entry::Vacant(e) = distances.entry(neighbor) {
                e.insert(d + 1);
                queue.push_back(neighbor);
            }
        }
    }
    distances
}

/// Numeric rank for tier comparison (lower = closer to player).
fn tier_rank(tier: CogTier) -> u8 {
    match tier {
        CogTier::Tier1 => 1,
        CogTier::Tier2 => 2,
        CogTier::Tier3 => 3,
        CogTier::Tier4 => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{load_test_graph, make_chain_graph, make_test_npc, make_test_world};
    use parish_world::WorldState;
    use parish_world::events::GameEvent;

    fn run_assign(
        npcs: &mut HashMap<NpcId, Npc>,
        world: &WorldState,
    ) -> (HashMap<NpcId, CogTier>, Vec<TierTransition>) {
        let mut ta = HashMap::new();
        let mut cache = None;
        let transitions = assign_tiers(npcs, &mut ta, &mut cache, world, &[]);
        (ta, transitions)
    }

    #[test]
    fn test_tier_assignment_with_parish_graph() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 2)); // 1 edge from crossroads
        npcs.insert(NpcId(2), make_test_npc(2, 1)); // at crossroads (player here)
        npcs.insert(NpcId(3), make_test_npc(3, 11)); // far away

        let world = make_test_world(graph, 1);
        let (ta, _) = run_assign(&mut npcs, &world);

        assert_eq!(ta.get(&NpcId(2)).copied(), Some(CogTier::Tier1));
        assert_eq!(ta.get(&NpcId(1)).copied(), Some(CogTier::Tier2));
        let fairy_tier = ta[&NpcId(3)];
        assert!(
            matches!(fairy_tier, CogTier::Tier2 | CogTier::Tier3 | CogTier::Tier4),
            "fairy fort should be Tier2–4 based on distance"
        );
    }

    #[test]
    fn test_tier1_and_tier2_lists() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 1)); // at crossroads with player
        npcs.insert(NpcId(2), make_test_npc(2, 2)); // pub, 1 edge away

        let world = make_test_world(graph, 1);
        let (ta, _) = run_assign(&mut npcs, &world);

        let tier1: Vec<NpcId> = ta
            .iter()
            .filter(|(_, t)| **t == CogTier::Tier1)
            .map(|(id, _)| *id)
            .collect();
        assert!(tier1.contains(&NpcId(1)));

        let tier2: Vec<NpcId> = ta
            .iter()
            .filter(|(_, t)| **t == CogTier::Tier2)
            .map(|(id, _)| *id)
            .collect();
        assert!(tier2.contains(&NpcId(2)));
    }

    #[test]
    fn test_tier_assignment_3_vs_4() {
        let graph = make_chain_graph(6);
        let mut npcs = HashMap::new();
        for i in 0..=6 {
            npcs.insert(NpcId(i + 10), make_test_npc(i + 10, i));
        }
        let world = make_test_world(graph, 0);
        let (ta, _) = run_assign(&mut npcs, &world);

        assert_eq!(ta[&NpcId(10)], CogTier::Tier1); // distance 0
        assert_eq!(ta[&NpcId(11)], CogTier::Tier2); // distance 1
        assert_eq!(ta[&NpcId(12)], CogTier::Tier2); // distance 2
        assert_eq!(ta[&NpcId(13)], CogTier::Tier3); // distance 3
        assert_eq!(ta[&NpcId(14)], CogTier::Tier3); // distance 4
        assert_eq!(ta[&NpcId(15)], CogTier::Tier3); // distance 5
        assert_eq!(ta[&NpcId(16)], CogTier::Tier4); // distance 6
    }

    #[test]
    fn test_tier3_npcs() {
        let graph = make_chain_graph(5);
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 3)); // distance 3 → Tier3
        npcs.insert(NpcId(2), make_test_npc(2, 4)); // distance 4 → Tier3
        npcs.insert(NpcId(3), make_test_npc(3, 1)); // distance 1 → Tier2

        let world = make_test_world(graph, 0);
        let (ta, _) = run_assign(&mut npcs, &world);

        let tier3: Vec<NpcId> = ta
            .iter()
            .filter(|(_, t)| **t == CogTier::Tier3)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(tier3.len(), 2);
        assert!(tier3.contains(&NpcId(1)));
        assert!(tier3.contains(&NpcId(2)));
    }

    #[test]
    fn test_tier_promotion_inflates_npc() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 11)); // far — starts Tier3+

        let world = make_test_world(graph.clone(), 1);
        let (ta, _) = run_assign(&mut npcs, &world);
        assert_ne!(ta[&NpcId(1)], CogTier::Tier1);

        // Move NPC to player's location.
        npcs.get_mut(&NpcId(1)).unwrap().location = LocationId(1);
        let events = vec![GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "excited".to_string(),
            timestamp: world.clock.now(),
        }];

        let mut ta2 = ta;
        let mut cache = None;
        assign_tiers(&mut npcs, &mut ta2, &mut cache, &world, &events);

        assert_eq!(ta2[&NpcId(1)], CogTier::Tier1);
        let npc = npcs.get(&NpcId(1)).unwrap();
        let memories = npc.memory.recent(10);
        assert!(!memories.is_empty());
        assert!(memories[0].content.contains("[Context recap]"));
    }

    #[test]
    fn test_tier_demotion_deflates_npc() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 1)); // same as player → Tier1

        let world = make_test_world(graph.clone(), 1);
        let (mut ta, _) = run_assign(&mut npcs, &world);
        assert_eq!(ta[&NpcId(1)], CogTier::Tier1);

        // Move NPC far away.
        npcs.get_mut(&NpcId(1)).unwrap().location = LocationId(11);
        let mut cache = None;
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(npc.deflated_summary.is_some());
        assert_eq!(npc.deflated_summary.as_ref().unwrap().npc_id, NpcId(1));
        assert_eq!(npc.deflated_summary.as_ref().unwrap().mood, "calm");
    }

    // ── BFS cache tests ──────────────────────────────────────────────────────

    #[test]
    fn bfs_cache_same_distances_on_cache_hit() {
        let graph = make_chain_graph(4);
        let mut npcs = HashMap::new();
        for i in 0..=4 {
            npcs.insert(NpcId(i + 10), make_test_npc(i + 10, i));
        }
        let world = make_test_world(graph, 0);

        let mut ta = HashMap::new();
        let mut cache = None;
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        let first: Vec<_> = (0..=4u32).map(|i| ta[&NpcId(i + 10)]).collect();

        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        let second: Vec<_> = (0..=4u32).map(|i| ta[&NpcId(i + 10)]).collect();

        assert_eq!(first, second, "cached BFS must agree with cold computation");
    }

    #[test]
    fn bfs_cache_invalidation_preserves_correctness() {
        let graph = make_chain_graph(4);
        let mut npcs = HashMap::new();
        for i in 0..=4 {
            npcs.insert(NpcId(i + 10), make_test_npc(i + 10, i));
        }
        let world = make_test_world(graph, 0);

        let mut ta = HashMap::new();
        let mut cache = None;
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        let before: Vec<_> = (0..=4u32).map(|i| ta[&NpcId(i + 10)]).collect();

        cache = None; // Invalidate.
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        let after: Vec<_> = (0..=4u32).map(|i| ta[&NpcId(i + 10)]).collect();

        assert_eq!(
            before, after,
            "post-invalidation must agree with pre-invalidation"
        );
    }

    #[test]
    fn bfs_cache_invalidated_on_player_move() {
        let graph = make_chain_graph(6);
        let mut npcs = HashMap::new();
        npcs.insert(NpcId(10), make_test_npc(10, 0));
        npcs.insert(NpcId(16), make_test_npc(16, 6));

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        let mut ta = HashMap::new();
        let mut cache = None;
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        assert_eq!(ta[&NpcId(10)], CogTier::Tier1);
        assert_eq!(ta[&NpcId(16)], CogTier::Tier4);

        // Move player to far end — cache key changes, BFS recomputes automatically.
        world.player_location = LocationId(6);
        assign_tiers(&mut npcs, &mut ta, &mut cache, &world, &[]);
        assert_eq!(
            ta[&NpcId(16)],
            CogTier::Tier1,
            "NPC at player's new location must be Tier1"
        );
        assert_eq!(
            ta[&NpcId(10)],
            CogTier::Tier4,
            "NPC 6 hops away must be Tier4"
        );
    }
}
