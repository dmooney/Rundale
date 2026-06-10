//! Introduction and player-name tracking.
//!
//! Part of the `NpcManager` impl, split out of the former monolithic
//! `manager.rs` (#1200 TD-030). Public method paths are unchanged.

use std::collections::HashSet;

use crate::{Npc, NpcId};

use super::NpcManager;

impl NpcManager {
    // ── Introduction / name tracking ─────────────────────────────────────────

    /// Marks an NPC as having introduced themselves to the player.
    pub fn mark_introduced(&mut self, id: NpcId) {
        self.introduced_npcs.insert(id);
    }

    /// Returns whether the player has been introduced to the given NPC.
    pub fn is_introduced(&self, id: NpcId) -> bool {
        self.introduced_npcs.contains(&id)
    }

    /// Returns a clone of the set of introduced NPC ids.
    pub fn introduced_set(&self) -> HashSet<NpcId> {
        self.introduced_npcs.clone()
    }

    /// Records that the given NPC has learned the player's name.
    pub fn teach_player_name(&mut self, id: NpcId) {
        self.npcs_who_know_player_name.insert(id);
    }

    /// Returns whether the given NPC knows the player's name.
    pub fn knows_player_name(&self, id: NpcId) -> bool {
        self.npcs_who_know_player_name.contains(&id)
    }

    /// Returns a clone of the set of NPC ids that know the player's name.
    pub fn player_name_known_set(&self) -> HashSet<NpcId> {
        self.npcs_who_know_player_name.clone()
    }

    /// Restores the set of NPC ids that know the player's name (for snapshot restore).
    pub fn restore_player_name_known(&mut self, ids: HashSet<NpcId>) {
        self.npcs_who_know_player_name = ids;
    }

    /// Returns the display name for an NPC: their name if introduced,
    /// or their brief description if not yet met.
    pub fn display_name<'a>(&self, npc: &'a Npc) -> &'a str {
        npc.display_name(self.is_introduced(npc.id))
    }

    /// Returns the number of NPCs that have introduced themselves to the player.
    pub fn introduced_count(&self) -> usize {
        self.introduced_npcs.len()
    }

    /// Restores the introduced-NPC set from a snapshot.
    pub fn restore_introduced_set(&mut self, set: HashSet<NpcId>) {
        self.introduced_npcs = set;
    }

    /// Clears the in-memory introduced-NPC set so the current session starts
    /// with no pre-known introductions (#1396).
    ///
    /// Call this after `snapshot.restore(…)` when the
    /// `npc-dialogue-grounding` feature flag is enabled (default-on).
    /// The save file still persists the set for schema stability; this method
    /// only resets the live in-memory state.
    pub fn clear_introduced_for_session(&mut self) {
        self.introduced_npcs.clear();
    }
}
