//! NPC storage/CRUD and name/role lookup.
//!
//! Part of the `NpcManager` impl, split out of the former monolithic
//! `manager.rs` (#1200 TD-030). Public method paths are unchanged.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::data::load_npcs_from_file;
use crate::types::NpcState;
use crate::{Npc, NpcId, RelationshipToneHint};
use parish_types::{LocationId, ParishError};

use super::{NpcManager, role_alias, unique_match};

impl NpcManager {
    // ── NPC storage / CRUD ───────────────────────────────────────────────────

    /// Loads NPCs from a JSON data file.
    pub fn load_from_file(path: &Path) -> Result<Self, ParishError> {
        let npcs_vec = load_npcs_from_file(path)?;
        let mut manager = Self::new();
        for npc in npcs_vec {
            manager.add_npc(npc);
        }
        Ok(manager)
    }

    /// Adds an NPC to the manager.
    pub fn add_npc(&mut self, mut npc: Npc) {
        // Treat every insertion as a new live incarnation. This is what makes
        // an identical in-memory snapshot restore distinguishable from the
        // pre-restore NPC for asynchronous Tier-2 results.
        npc.reset_authored_activity_observation();
        npc.refresh_grounding_revision();
        self.npcs.insert(npc.id, npc);
    }

    /// Removes a deceased NPC and scrubs every dangling reference to it
    /// from the rest of the roster (#339).
    ///
    /// A bare `self.npcs.remove(id)` would leave stale entries in
    /// `tier_assignments`, `introduced_npcs`, `npcs_who_know_player_name`,
    /// and every surviving NPC's `relationships` map. Call this from every
    /// death-handling path instead. Returns the removed NPC if it existed.
    pub fn remove_npc(&mut self, id: NpcId) -> Option<Npc> {
        let removed = self.npcs.remove(&id);
        self.tier_assignments.remove(&id);
        self.introduced_npcs.remove(&id);
        self.npcs_who_know_player_name.remove(&id);
        for npc in self.npcs.values_mut() {
            npc.relationships.remove(&id);
        }
        removed
    }

    /// Invalidates the cached BFS distances.
    ///
    /// Must be called whenever the world graph is replaced wholesale — for
    /// example after an editor live-reload or a snapshot restore — so the
    /// next `assign_tiers` call recomputes distances from scratch.
    pub fn invalidate_bfs_cache(&mut self) {
        self.bfs_distances_cache = None;
    }

    /// Returns a reference to an NPC by id.
    pub fn get(&self, id: NpcId) -> Option<&Npc> {
        self.npcs.get(&id)
    }

    /// Returns post-generation tone hints for the speaker's known relationships.
    ///
    /// Keeps relationship-to-name projection owned by the NPC manager so
    /// runtime and script harness dialogue guards do not duplicate roster
    /// lookup logic.
    pub fn relationship_tone_hints(&self, speaker_id: NpcId) -> Vec<RelationshipToneHint> {
        self.get(speaker_id)
            .map(|speaker| {
                speaker
                    .relationships
                    .iter()
                    .filter_map(|(target_id, rel)| {
                        self.get(*target_id).map(|target| RelationshipToneHint {
                            target_name: target.name.clone(),
                            kind: rel.kind,
                            strength: rel.strength,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns a mutable reference to an NPC by id.
    pub fn get_mut(&mut self, id: NpcId) -> Option<&mut Npc> {
        self.npcs.get_mut(&id)
    }

    /// Returns references to all NPCs currently present at the given location.
    pub fn npcs_at(&self, location: LocationId) -> Vec<&Npc> {
        self.npcs
            .values()
            .filter(|npc| matches!(npc.state, NpcState::Present) && npc.location == location)
            .collect()
    }

    /// Returns the ids of all NPCs currently present at the given location.
    pub fn npcs_at_ids(&self, location: LocationId) -> Vec<NpcId> {
        self.npcs
            .values()
            .filter(|npc| matches!(npc.state, NpcState::Present) && npc.location == location)
            .map(|npc| npc.id)
            .collect()
    }

    /// Finds an NPC at a location by display or canonical name (case-insensitive).
    ///
    /// Tries exact display/canonical match first, then introduced first-name
    /// match. Canonical exact matches are kept for explicit recipient lists
    /// such as UI chip selections; free-text mention parsing is responsible
    /// for not exposing hidden names before introduction. Ambiguous matches
    /// return `None` rather than guessing.
    pub fn find_by_name(&self, name: &str, location: LocationId) -> Option<&Npc> {
        let npcs = self.npcs_at(location);
        let lower = name.trim().to_lowercase();
        if lower.is_empty() {
            return None;
        }

        let exact_matches: Vec<&Npc> = npcs
            .iter()
            .copied()
            .filter(|npc| {
                self.display_name(npc).to_lowercase() == lower || npc.name.to_lowercase() == lower
            })
            .collect();
        match exact_matches.as_slice() {
            [npc] => return Some(npc),
            [] => {}
            _ => return None,
        }

        let first_name_matches: Vec<&Npc> = npcs
            .iter()
            .copied()
            .filter(|npc| {
                self.is_introduced(npc.id)
                    && npc
                        .name
                        .to_lowercase()
                        .split_whitespace()
                        .next()
                        .is_some_and(|first| first == lower)
            })
            .collect();
        match first_name_matches.as_slice() {
            [npc] => Some(npc),
            _ => None,
        }
    }

    /// Finds an NPC at a location by occupation/role (case-insensitive).
    ///
    /// Returns `Some` only if exactly one co-located NPC matches — protects
    /// against silently routing to the wrong person when a role is shared
    /// (e.g. two farmers at the same farm). Used as a fallback by
    /// `resolve_npc_targets` so human players can address NPCs by
    /// role-vocative ("Father", "Priest", "Widow", "Constable") when the
    /// reference is unambiguous (issue #998).
    ///
    /// Matching tiers, tried in order until one returns a unique hit:
    /// 1. Exact case-insensitive equality (`"Widow" == "Widow"`).
    /// 2. Whole-word token overlap (`"Priest"` matches `"Parish Priest"`,
    ///    `"Constable"` matches `"Retired Constable"`).
    /// 3. Built-in vocative aliases (`"Father" → priest occupations`).
    ///
    /// Ambiguous at any tier returns `None` so the caller's "no one here by
    /// that name" path fires instead of guessing.
    pub fn find_by_role_at(&self, role: &str, location: LocationId) -> Option<&Npc> {
        let needle = role.trim();
        if needle.is_empty() {
            return None;
        }
        let npcs = self.npcs_at(location);

        // Tier 1: exact case-insensitive equality.
        if let Some(hit) = unique_match(&npcs, |npc| npc.occupation.eq_ignore_ascii_case(needle)) {
            return Some(hit);
        }

        // Tier 2: needle matches any whole word in the occupation.
        let needle_lower = needle.to_ascii_lowercase();
        if let Some(hit) = unique_match(&npcs, |npc| {
            npc.occupation
                .split_whitespace()
                .any(|tok| tok.eq_ignore_ascii_case(&needle_lower))
        }) {
            return Some(hit);
        }

        // Tier 3: built-in vocative aliases (Irish 1820 Catholic context).
        if let Some(canonical) = role_alias(&needle_lower)
            && let Some(hit) = unique_match(&npcs, |npc| {
                npc.occupation
                    .split_whitespace()
                    .any(|tok| tok.eq_ignore_ascii_case(canonical))
            })
        {
            return Some(hit);
        }

        None
    }

    /// Finds an NPC by exact name (case-insensitive), searching all NPCs.
    pub fn find_by_name_mut(&mut self, name: &str) -> Option<&mut Npc> {
        let lower = name.to_lowercase();
        self.npcs
            .values_mut()
            .find(|n| n.name.to_lowercase() == lower)
    }

    /// Returns an iterator over all NPCs.
    pub fn all_npcs(&self) -> impl Iterator<Item = &Npc> {
        self.npcs.values()
    }

    /// Returns a shared reference to the internal NPC map.
    pub fn npcs(&self) -> &HashMap<NpcId, Npc> {
        &self.npcs
    }

    /// Returns a mutable reference to the internal NPC map.
    pub fn npcs_mut(&mut self) -> &mut HashMap<NpcId, Npc> {
        &mut self.npcs
    }

    /// Returns the NPCs that a given NPC "knows" — relationships, memory
    /// participants, and co-residents at home/workplace.
    ///
    /// Returns `(NpcId, name, descriptor)` tuples, deduplicated. The descriptor
    /// is `"<pronouns>, <age>, <occupation>"` (pronouns omitted when unknown) so
    /// the dialogue prompt grounds the model in each person's gender and age and
    /// it never has to guess from a name (#1506).
    pub fn known_roster(&self, npc: &Npc) -> Vec<(NpcId, String, String)> {
        let mut known_ids: HashSet<NpcId> = HashSet::new();
        for target_id in npc.relationships.keys() {
            known_ids.insert(*target_id);
        }
        for entry in npc.memory.entries() {
            for &pid in &entry.participants {
                if pid != npc.id && pid != NpcId(0) {
                    known_ids.insert(pid);
                }
            }
        }
        if npc.home.is_some() || npc.workplace.is_some() {
            for other in self.npcs.values() {
                if other.id == npc.id {
                    continue;
                }
                let home_match = match npc.home {
                    Some(home) => other.home == Some(home) || other.location == home,
                    None => false,
                };
                let work_match = match npc.workplace {
                    Some(work) => other.workplace == Some(work) || other.location == work,
                    None => false,
                };
                if home_match || work_match {
                    known_ids.insert(other.id);
                }
            }
        }
        known_ids
            .into_iter()
            .filter_map(|id| {
                let other = self.npcs.get(&id)?;
                // Descriptor grounds the model in pronouns + age so it never
                // guesses gender from a name (#1506).
                let descriptor = if other.pronouns.trim().is_empty() {
                    format!("{}, {}", other.age, other.occupation)
                } else {
                    format!("{}, {}, {}", other.pronouns, other.age, other.occupation)
                };
                Some((id, other.name.clone(), descriptor))
            })
            .collect()
    }

    /// Returns the number of NPCs managed.
    pub fn npc_count(&self) -> usize {
        self.npcs.len()
    }
}
