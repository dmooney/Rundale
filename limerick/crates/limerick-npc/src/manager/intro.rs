//! Introduction and player-name tracking.
//!
//! Part of the `NpcManager` impl, split out of the former monolithic
//! `manager.rs` (#1200 TD-030). Public method paths are unchanged.

use std::collections::{BTreeMap, HashSet};

use limerick_types::ConversationLog;

use crate::{Npc, NpcId, dialogue_self_identifies_speaker};

use super::NpcManager;

/// Most facts about the player one NPC keeps; the oldest is forgotten first.
pub const MAX_PLAYER_FACTS_PER_NPC: usize = 3;

impl NpcManager {
    // ── Conversation partner (dynamic NPC context design §2, §3.2) ──────────

    /// The person the player is talking to, if any.
    pub fn partner(&self) -> Option<NpcId> {
        self.partner
    }

    /// Makes `id` the partner, or ends the partnership with `None`.
    pub fn set_partner(&mut self, id: Option<NpcId>) {
        self.partner = id;
    }

    /// Ends the partnership unless the partner is present at `location`:
    /// the player moved, or the partner left.
    pub fn end_partnership_unless_at(&mut self, location: limerick_types::LocationId) {
        if let Some(id) = self.partner
            && !self.npcs_at_ids(location).contains(&id)
        {
            self.partner = None;
        }
    }

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

    /// Records that the given NPC was told `name` as the player's name.
    ///
    /// A later introduction to the same NPC replaces the name it knows;
    /// what every other NPC knows is unchanged. A blank name is ignored.
    pub fn teach_player_name(&mut self, id: NpcId, name: impl Into<String>) {
        let name = name.into();
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.player_names_by_npc.insert(id, name.to_string());
    }

    /// The name the player gave the given NPC, if they gave it one.
    pub fn player_name_known_by(&self, id: NpcId) -> Option<&str> {
        self.player_names_by_npc.get(&id).map(String::as_str)
    }

    /// Records that the player told the given NPC `fact` about themselves
    /// (see [`crate::detect_player_facts`]). A repeat is ignored; past
    /// [`MAX_PLAYER_FACTS_PER_NPC`] the oldest is forgotten. Blank is ignored.
    pub fn teach_player_fact(&mut self, id: NpcId, fact: impl Into<String>) {
        let fact = fact.into();
        let fact = fact.trim();
        if fact.is_empty() {
            return;
        }
        let known = self.player_facts_by_npc.entry(id).or_default();
        if known.iter().any(|existing| existing == fact) {
            return;
        }
        known.push(fact.to_string());
        if known.len() > MAX_PLAYER_FACTS_PER_NPC {
            known.remove(0);
        }
    }

    /// What the player told the given NPC about themselves, oldest first.
    pub fn player_facts_known_by(&self, id: NpcId) -> &[String] {
        self.player_facts_by_npc.get(&id).map_or(&[], Vec::as_slice)
    }

    /// Returns a clone of what each NPC was told (for snapshots).
    pub fn player_facts_known(&self) -> BTreeMap<NpcId, Vec<String>> {
        self.player_facts_by_npc.clone()
    }

    /// Restores what each NPC was told (for snapshot restore), keeping each
    /// NPC within the bound.
    pub fn restore_player_facts_known(&mut self, facts: BTreeMap<NpcId, Vec<String>>) {
        self.player_facts_by_npc.clear();
        for (id, facts) in facts {
            for fact in facts {
                self.teach_player_fact(id, fact);
            }
        }
    }

    /// Returns whether the given NPC has been told the player's name.
    pub fn knows_player_name(&self, id: NpcId) -> bool {
        self.player_names_by_npc.contains_key(&id)
    }

    /// Returns a clone of the name each NPC was told (for snapshots).
    pub fn player_names_known(&self) -> BTreeMap<NpcId, String> {
        self.player_names_by_npc.clone()
    }

    /// Restores the name each NPC was told (for snapshot restore). Blank
    /// names are dropped.
    pub fn restore_player_names_known(&mut self, names: BTreeMap<NpcId, String>) {
        self.player_names_by_npc.clear();
        for (id, name) in names {
            self.teach_player_name(id, name);
        }
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
