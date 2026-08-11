//! Introduction and player-name tracking.
//!
//! Part of the `NpcManager` impl, split out of the former monolithic
//! `manager.rs` (#1200 TD-030). Public method paths are unchanged.

use std::collections::HashSet;

use parish_types::ConversationLog;

use crate::{Npc, NpcId, dialogue_self_identifies_speaker};

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

    /// Conservatively repairs identity knowledge erased by the #1396 restore
    /// regression from retained canonical conversation dialogue.
    ///
    /// Only the same strict final-dialogue detector used by the live apply seam
    /// may add an id. Canonical speaker metadata or the mere existence of an
    /// exchange is never sufficient. Returns the number of newly repaired ids.
    pub fn heal_introductions_from_conversation(&mut self, log: &ConversationLog) -> usize {
        let roster: Vec<(String, String)> = self
            .all_npcs()
            .map(|npc| (npc.name.clone(), npc.occupation.clone()))
            .collect();
        let healed: HashSet<NpcId> = log
            .exchanges_since(0)
            .into_iter()
            .filter_map(|exchange| {
                let npc = self.get(exchange.speaker_id)?;
                dialogue_self_identifies_speaker(
                    &exchange.npc_dialogue,
                    &npc.name,
                    &npc.occupation,
                    &roster,
                )
                .then_some(npc.id)
            })
            .collect();
        let before = self.introduced_npcs.len();
        self.introduced_npcs.extend(healed);
        self.introduced_npcs.len() - before
    }
}
