//! The facts behind the living-world proofs (issue 2195, product spec §17
//! Milestone 6).
//!
//! The tiny world proves five behaviours: memory, gossip, a task, a
//! weather-dependent behaviour and a scheduled movement. Two of them need
//! authored facts the other mod files cannot hold: the one gossip fact with
//! its starting source and intended path, and the one task with the states it
//! passes through. A mod declares them as `[files] living_world`. The
//! scheduled movement and the weather behaviour are not repeated here: they
//! come from an NPC's schedule in `npcs.json` and the engine's weather rules,
//! and the canonical world sheet names them for testers.
//!
//! The mod's `world-sheet.txt` states the same facts, and
//! `limerick-engine/tests/world_sheet.rs` fails when the two disagree.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use limerick_types::{GossipNetwork, MAX_TASK_DESCRIPTION_CHARS, NpcId};
use serde::{Deserialize, Serialize};

/// The one gossip fact the tiny world starts with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GossipFact {
    /// The fact, as the source would tell it.
    pub content: String,
    /// The NPC who knows it at the start, and nobody else.
    pub source: u32,
    /// The one intended NPC-to-NPC path, beginning with the source. An NPC
    /// who is not on the path should never come to know the fact.
    pub path: Vec<u32>,
    /// When the source learned it, before the game starts.
    pub heard_at: DateTime<Utc>,
}

/// The one task an NPC can assign the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskFact {
    /// The NPC who assigns it.
    pub assigned_by: u32,
    /// The location where it is advanced and completed.
    pub location: u32,
    /// The work, as it is recorded in the player's task ledger.
    pub description: String,
    /// What happens when the task becomes assigned.
    pub assigned: String,
    /// What the player does to make it in progress.
    pub in_progress: String,
    /// What the player does to complete it.
    pub completed: String,
    /// How later interaction reflects completion.
    pub afterwards: String,
}

/// The authored living-world facts of a mod (`[files] living_world`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LivingWorld {
    /// The one initial gossip fact, if the mod has one.
    #[serde(default)]
    pub gossip: Option<GossipFact>,
    /// The one assignable task, if the mod has one.
    #[serde(default)]
    pub task: Option<TaskFact>,
}

impl LivingWorld {
    /// Parses a `[files] living_world` body.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|error| error.to_string())
    }

    /// Puts the initial gossip fact into `network`, known to its source alone.
    ///
    /// Does nothing when the mod has no gossip fact.
    pub fn seed_gossip(&self, network: &mut GossipNetwork) {
        if let Some(gossip) = &self.gossip {
            network.create(
                gossip.content.clone(),
                NpcId(gossip.source),
                gossip.heard_at,
            );
        }
    }

    /// Checks the facts against the mod's NPC and location ids.
    ///
    /// Run by `world_state_from_mod` on the production load path, where the
    /// catalogues are available (`GameMod::load` cannot, because it loads
    /// neither NPCs nor the world graph), and by the world-sheet test.
    pub fn validate(&self, npcs: &BTreeSet<u32>, locations: &BTreeSet<u32>) -> Result<(), String> {
        if let Some(gossip) = &self.gossip {
            if gossip.content.trim().is_empty() {
                return Err("gossip content is blank".into());
            }
            if gossip.path.first() != Some(&gossip.source) {
                return Err("gossip path must begin with its source".into());
            }
            if gossip.path.len() < 2 {
                return Err("gossip path must reach at least one other NPC".into());
            }
            let distinct: BTreeSet<_> = gossip.path.iter().collect();
            if distinct.len() != gossip.path.len() {
                return Err("gossip path visits an NPC twice".into());
            }
            if let Some(unknown) = gossip.path.iter().find(|id| !npcs.contains(id)) {
                return Err(format!("gossip path names unknown NPC {unknown}"));
            }
        }
        if let Some(task) = &self.task {
            if !npcs.contains(&task.assigned_by) {
                return Err(format!(
                    "task is assigned by unknown NPC {}",
                    task.assigned_by
                ));
            }
            if !locations.contains(&task.location) {
                return Err(format!("task names unknown location {}", task.location));
            }
            let description = task.description.trim();
            if description.is_empty() {
                return Err("task description is blank".into());
            }
            if description.chars().count() > MAX_TASK_DESCRIPTION_CHARS {
                return Err(format!(
                    "task description exceeds {MAX_TASK_DESCRIPTION_CHARS} characters"
                ));
            }
            for (name, text) in [
                ("assigned", &task.assigned),
                ("in_progress", &task.in_progress),
                ("completed", &task.completed),
                ("afterwards", &task.afterwards),
            ] {
                if text.trim().is_empty() {
                    return Err(format!("task {name} state is blank"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{
        "gossip": {"content": "A buyer paid well.", "source": 2, "path": [2, 3],
                   "heard_at": "1820-03-19T15:00:00Z"},
        "task": {"assigned_by": 3, "location": 2, "description": "Carry word to Peig.",
                 "assigned": "a", "in_progress": "b", "completed": "c", "afterwards": "d"}
    }"#;

    fn ids(values: &[u32]) -> BTreeSet<u32> {
        values.iter().copied().collect()
    }

    #[test]
    fn parses_validates_and_seeds_the_gossip_to_its_source_alone() {
        let facts = LivingWorld::parse(BODY).expect("parses");
        facts
            .validate(&ids(&[1, 2, 3]), &ids(&[1, 2, 3]))
            .expect("valid");
        let mut network = GossipNetwork::new();
        facts.seed_gossip(&mut network);
        assert_eq!(network.len(), 1);
        assert_eq!(network.known_by(NpcId(2)).len(), 1);
        assert!(network.known_by(NpcId(3)).is_empty());
    }

    #[test]
    fn an_empty_body_is_a_world_without_proof_facts() {
        let facts = LivingWorld::parse("{}").expect("parses");
        let mut network = GossipNetwork::new();
        facts.seed_gossip(&mut network);
        assert!(network.is_empty());
        assert_eq!(facts, LivingWorld::default());
        facts.validate(&ids(&[]), &ids(&[])).expect("valid");
    }

    #[test]
    fn rejects_unknown_fields_and_bad_references() {
        assert!(LivingWorld::parse(r#"{"memory": {}}"#).is_err());
        let facts = LivingWorld::parse(BODY).expect("parses");
        assert!(facts.validate(&ids(&[2, 3]), &ids(&[1, 3])).is_err());
        assert!(facts.validate(&ids(&[1, 2]), &ids(&[1, 2, 3])).is_err());

        let mut gossip = facts.clone();
        gossip.gossip.as_mut().unwrap().path = vec![3, 2];
        assert!(gossip.validate(&ids(&[1, 2, 3]), &ids(&[1, 2, 3])).is_err());
        gossip.gossip.as_mut().unwrap().path = vec![2];
        assert!(gossip.validate(&ids(&[1, 2, 3]), &ids(&[1, 2, 3])).is_err());
        gossip.gossip.as_mut().unwrap().path = vec![2, 3, 2];
        assert!(gossip.validate(&ids(&[1, 2, 3]), &ids(&[1, 2, 3])).is_err());

        let mut task = facts;
        task.task.as_mut().unwrap().description = "   ".into();
        assert!(task.validate(&ids(&[1, 2, 3]), &ids(&[1, 2, 3])).is_err());
    }
}
