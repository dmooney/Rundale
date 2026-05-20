//! Central NPC coordinator.
//!
//! Owns all NPCs, tracks cognitive tiers, and manages introduction state.
//! Heavy subsystems — schedule resolution, tier assignment, banshee, tier4
//! event application — live in their own modules; the methods here are thin
//! wrappers that delegate and expose the stable public API.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use chrono::{DateTime, Utc};

use crate::data::load_npcs_from_file;
use crate::types::{CogTier, NpcState};
use crate::{Npc, NpcId};
use parish_config::CognitiveTierConfig;
use parish_types::{LocationId, ParishError, Weather};
use parish_world::WorldState;
use parish_world::events::{EventBus, GameEvent};
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

// Re-export subsystem types so callers keep their existing import paths.
pub use crate::schedule::{ScheduleEvent, ScheduleEventKind};
pub use crate::tier_assign::TierTransition;

/// Central coordinator for all NPC state and behavior.
///
/// Owns all NPCs, assigns cognitive tiers based on distance from the
/// player, and advances NPC schedules so they move between locations
/// according to their daily routines.
///
/// Also tracks which NPCs have been introduced to the player. Before
/// introduction, NPCs are referred to by a brief anonymous description
/// (e.g., "a priest") rather than by name.
/// Scheduling state for a single cognitive tier.
#[derive(Debug, Clone, Default)]
pub struct TierTickState {
    /// Game time of the last tick for this tier (None if never ticked).
    pub last_game_time: Option<DateTime<Utc>>,
    /// Whether a tick for this tier is currently in-flight.
    pub in_flight: bool,
}

/// Capacity of [`NpcManager::reaction_emoji_buffer`]. Eight slots is the
/// window the issue #995 detector samples — large enough to dilute a
/// single stray same-emoji burst, small enough that a sustained run is
/// caught within a handful of player turns.
pub const REACTION_EMOJI_BUFFER_CAPACITY: usize = 8;

pub struct NpcManager {
    /// All NPCs keyed by their unique id.
    npcs: HashMap<NpcId, Npc>,
    /// Current cognitive tier assignment for each NPC.
    tier_assignments: HashMap<NpcId, CogTier>,
    /// Scheduling state for Tier 2.
    tier2_state: TierTickState,
    /// Scheduling state for Tier 3.
    tier3_state: TierTickState,
    /// Scheduling state for Tier 4.
    tier4_state: TierTickState,
    /// Set of NPC ids that have introduced themselves to the player.
    introduced_npcs: HashSet<NpcId>,
    /// Set of NPC ids that know the player's name.
    npcs_who_know_player_name: HashSet<NpcId>,
    /// Ring buffer of the last 5 Tier 4 life-event descriptions (newest last).
    recent_tier4_events: VecDeque<String>,
    /// Rolling window of the most recent NPC-reaction emoji (newest last),
    /// capped at [`REACTION_EMOJI_BUFFER_CAPACITY`].
    ///
    /// Feeds [`crate::quality::detect_emoji_monoculture`] each time
    /// [`Self::record_reaction_emoji`] is called. Issue #995 showed
    /// small-model reaction inference collapsing onto one or two safe
    /// emoji at temp=0; the buffer + detector are the sensor for the
    /// regression.
    reaction_emoji_buffer: VecDeque<String>,
    /// Tracks whether the last detector call already fired a WARN so
    /// the same crossing isn't re-logged on every subsequent push.
    /// Cleared once the buffer drops back below the threshold.
    reaction_monoculture_active: bool,
    /// Cached BFS distances from the last player location.
    ///
    /// Stored as `(player_location, distances)`. When `assign_tiers` is called
    /// with the same player location as the cached key the BFS is skipped —
    /// the world graph never mutates in place during a session, so the
    /// distances are stable until the player moves.
    ///
    /// Call `invalidate_bfs_cache` whenever the graph is replaced wholesale
    /// (e.g. after an editor live-reload or snapshot restore).
    bfs_distances_cache: Option<(LocationId, HashMap<LocationId, u32>)>,
}

impl NpcManager {
    /// Creates an empty NpcManager.
    pub fn new() -> Self {
        Self {
            npcs: HashMap::new(),
            tier_assignments: HashMap::new(),
            tier2_state: TierTickState::default(),
            tier3_state: TierTickState::default(),
            tier4_state: TierTickState::default(),
            introduced_npcs: HashSet::new(),
            npcs_who_know_player_name: HashSet::new(),
            recent_tier4_events: VecDeque::with_capacity(crate::tier4::RING_BUFFER_CAPACITY),
            reaction_emoji_buffer: VecDeque::with_capacity(REACTION_EMOJI_BUFFER_CAPACITY),
            reaction_monoculture_active: false,
            bfs_distances_cache: None,
        }
    }

    // ── Reaction-quality sensor (issue #995) ─────────────────────────────────

    /// Records an emitted NPC reaction emoji into the rolling diversity
    /// buffer and runs [`crate::quality::detect_emoji_monoculture`].
    ///
    /// When the detector reports a fresh monoculture crossing, emits a
    /// `tracing::warn!` event with `site="reactions"`,
    /// `kind="reaction-emoji-monoculture"`, and the detector's
    /// human-readable diversity detail. Subsequent pushes that stay in
    /// monoculture do not re-emit (debounced); the next WARN fires only
    /// after the buffer falls below the threshold and crosses back
    /// above it.
    ///
    /// Called from every runtime's reaction-persist callback so the
    /// sensor sees every reaction regardless of entry point (CLI,
    /// server, Tauri).
    pub fn record_reaction_emoji(&mut self, emoji: &str) {
        self.reaction_emoji_buffer.push_back(emoji.to_string());
        if self.reaction_emoji_buffer.len() > REACTION_EMOJI_BUFFER_CAPACITY {
            self.reaction_emoji_buffer.pop_front();
        }

        // Skip the snapshot allocation while the buffer is too small for
        // the detector to draw a conclusion — this function runs once per
        // reacting NPC per player turn, so the early-out matters on busy
        // locations.
        if self.reaction_emoji_buffer.len() < crate::quality::DEFAULT_EMOJI_MIN_SAMPLES {
            return;
        }

        let snapshot: Vec<&str> = self
            .reaction_emoji_buffer
            .iter()
            .map(String::as_str)
            .collect();
        match crate::quality::detect_emoji_monoculture(&snapshot) {
            Some(issue) if !self.reaction_monoculture_active => {
                self.reaction_monoculture_active = true;
                tracing::warn!(
                    site = "reactions",
                    kind = issue.kind.as_str(),
                    detail = %issue.detail,
                    sample_count = self.reaction_emoji_buffer.len(),
                    "NPC reaction emoji diversity below threshold"
                );
            }
            Some(_) => {
                // Already flagged; stay quiet until diversity recovers.
            }
            None => {
                self.reaction_monoculture_active = false;
            }
        }
    }

    /// Returns the current reaction-emoji diversity buffer (oldest first).
    ///
    /// Exposed for diagnostics and tests; production callers don't need
    /// to inspect the buffer directly.
    pub fn reaction_emoji_buffer(&self) -> Vec<String> {
        self.reaction_emoji_buffer.iter().cloned().collect()
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
    pub fn add_npc(&mut self, npc: Npc) {
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

    /// Finds an NPC at a location by name (case-insensitive).
    ///
    /// Tries exact match first, then first-name prefix match.
    pub fn find_by_name(&self, name: &str, location: LocationId) -> Option<&Npc> {
        let npcs = self.npcs_at(location);
        let lower = name.to_lowercase();
        let mut prefix_match: Option<&Npc> = None;
        for &npc in &npcs {
            let name_lower = npc.name.to_lowercase();
            let display_lower = self.display_name(npc).to_lowercase();
            if name_lower == lower || display_lower == lower {
                return Some(npc);
            }
            if prefix_match.is_none()
                && (name_lower
                    .split_whitespace()
                    .next()
                    .is_some_and(|first| first == lower)
                    || display_lower
                        .split_whitespace()
                        .next()
                        .is_some_and(|first| first == lower))
            {
                prefix_match = Some(npc);
            }
        }
        prefix_match
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

    /// Returns a mutable reference to the internal NPC map.
    pub fn npcs_mut(&mut self) -> &mut HashMap<NpcId, Npc> {
        &mut self.npcs
    }

    /// Returns the NPCs that a given NPC "knows" — relationships, memory
    /// participants, and co-residents at home/workplace.
    ///
    /// Returns `(NpcId, name, occupation)` tuples, deduplicated.
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
                Some((id, other.name.clone(), other.occupation.clone()))
            })
            .collect()
    }

    /// Returns the number of NPCs managed.
    pub fn npc_count(&self) -> usize {
        self.npcs.len()
    }

    // ── Tier queries ─────────────────────────────────────────────────────────

    /// Returns the current cognitive tier for an NPC.
    pub fn tier_of(&self, id: NpcId) -> Option<CogTier> {
        self.tier_assignments.get(&id).copied()
    }

    /// Returns the ids of all NPCs assigned to the given cognitive tier.
    pub fn npcs_in_tier(&self, tier: CogTier) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, t)| **t == tier)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Groups Tier 2 NPCs by their current location.
    pub fn tier2_groups(&self) -> HashMap<LocationId, Vec<NpcId>> {
        let mut groups: HashMap<LocationId, Vec<NpcId>> = HashMap::new();
        for (id, tier) in &self.tier_assignments {
            if *tier == CogTier::Tier2
                && let Some(npc) = self.npcs.get(id)
                && matches!(npc.state, NpcState::Present)
            {
                groups.entry(npc.location).or_default().push(*id);
            }
        }
        groups
    }

    // ── Tier tick state management ───────────────────────────────────────────

    /// Returns whether enough game time has elapsed for a Tier 2 tick.
    pub fn needs_tier2_tick(&self, current_game_time: DateTime<Utc>) -> bool {
        self.needs_tier2_tick_with_config(current_game_time, &CognitiveTierConfig::default())
    }

    /// Returns whether enough game time has elapsed for a Tier 2 tick,
    /// using the given cognitive tier config for the tick interval.
    pub fn needs_tier2_tick_with_config(
        &self,
        current_game_time: DateTime<Utc>,
        config: &CognitiveTierConfig,
    ) -> bool {
        match self.tier2_state.last_game_time {
            None => true,
            Some(last) => {
                current_game_time.signed_duration_since(last).num_minutes()
                    >= config.tier2_tick_interval_minutes
            }
        }
    }

    /// Returns the game time of the last Tier 2 tick, if any.
    pub fn last_tier2_game_time(&self) -> Option<DateTime<Utc>> {
        self.tier2_state.last_game_time
    }

    /// Records that a Tier 2 tick has been performed at the given game time.
    pub fn record_tier2_tick(&mut self, time: DateTime<Utc>) {
        self.tier2_state.last_game_time = Some(time);
    }

    /// Returns whether a Tier 2 tick is currently in-flight.
    pub fn tier2_in_flight(&self) -> bool {
        self.tier2_state.in_flight
    }

    /// Sets whether a Tier 2 tick is currently in-flight.
    pub fn set_tier2_in_flight(&mut self, in_flight: bool) {
        self.tier2_state.in_flight = in_flight;
    }

    /// Returns whether enough game time has elapsed for a Tier 3 tick.
    pub fn needs_tier3_tick(&self, current_game_time: DateTime<Utc>) -> bool {
        self.needs_tier3_tick_with_config(current_game_time, &CognitiveTierConfig::default())
    }

    /// Returns whether enough game time has elapsed for a Tier 3 tick,
    /// using the given cognitive tier config for the tick interval.
    pub fn needs_tier3_tick_with_config(
        &self,
        current_game_time: DateTime<Utc>,
        config: &CognitiveTierConfig,
    ) -> bool {
        match self.tier3_state.last_game_time {
            None => true,
            Some(last) => {
                current_game_time.signed_duration_since(last).num_hours()
                    >= config.tier3_tick_interval_hours
            }
        }
    }

    /// Returns the game time of the last Tier 3 tick, if any.
    pub fn last_tier3_game_time(&self) -> Option<DateTime<Utc>> {
        self.tier3_state.last_game_time
    }

    /// Records that a Tier 3 tick has been performed at the given game time.
    pub fn record_tier3_tick(&mut self, time: DateTime<Utc>) {
        self.tier3_state.last_game_time = Some(time);
    }

    /// Returns whether a Tier 3 tick is currently in-flight.
    pub fn tier3_in_flight(&self) -> bool {
        self.tier3_state.in_flight
    }

    /// Sets whether a Tier 3 tick is currently in-flight.
    pub fn set_tier3_in_flight(&mut self, in_flight: bool) {
        self.tier3_state.in_flight = in_flight;
    }

    /// Returns whether enough game time has elapsed for a Tier 4 tick.
    pub fn needs_tier4_tick(&self, current_game_time: DateTime<Utc>) -> bool {
        self.needs_tier4_tick_with_config(current_game_time, &CognitiveTierConfig::default())
    }

    /// Returns whether enough game time has elapsed for a Tier 4 tick,
    /// using the given cognitive tier config for the tick interval.
    pub fn needs_tier4_tick_with_config(
        &self,
        current_game_time: DateTime<Utc>,
        config: &CognitiveTierConfig,
    ) -> bool {
        match self.tier4_state.last_game_time {
            None => true,
            Some(last) => {
                current_game_time.signed_duration_since(last).num_days()
                    >= config.tier4_tick_interval_days
            }
        }
    }

    /// Returns the game time of the last Tier 4 tick, if any.
    pub fn last_tier4_game_time(&self) -> Option<DateTime<Utc>> {
        self.tier4_state.last_game_time
    }

    /// Records that a Tier 4 tick has been performed at the given game time.
    pub fn record_tier4_tick(&mut self, time: DateTime<Utc>) {
        self.tier4_state.last_game_time = Some(time);
    }

    /// Returns the ring buffer of recent Tier 4 life-event descriptions (newest last).
    pub fn recent_tier4_events(&self) -> &VecDeque<String> {
        &self.recent_tier4_events
    }

    // ── Subsystem wrappers ───────────────────────────────────────────────────

    /// Advances NPC schedules based on the current game time.
    ///
    /// See [`crate::schedule::tick_schedules`] for full documentation.
    pub fn tick_schedules(
        &mut self,
        clock: &GameClock,
        graph: &WorldGraph,
        weather: Weather,
        event_bus: &parish_types::events::EventBus,
    ) -> Vec<ScheduleEvent> {
        crate::schedule::tick_schedules(&mut self.npcs, clock, graph, weather, event_bus)
    }

    /// Assigns cognitive tiers to all NPCs based on BFS distance from the player.
    ///
    /// See [`crate::tier_assign::assign_tiers`] for full documentation.
    pub fn assign_tiers(
        &mut self,
        world: &WorldState,
        recent_events: &[GameEvent],
    ) -> Vec<TierTransition> {
        crate::tier_assign::assign_tiers(
            &mut self.npcs,
            &mut self.tier_assignments,
            &mut self.bfs_distances_cache,
            world,
            recent_events,
        )
    }

    /// Silently populates `tier_assignments` from the current world +
    /// NPC state without publishing any `GameEvent` or running
    /// inflation/deflation. Call this once after rebuilding the
    /// manager from a snapshot — see
    /// [`crate::tier_assign::seed_tier_state`] for the rationale.
    pub fn seed_tier_state(&mut self, world: &WorldState) {
        crate::tier_assign::seed_tier_state(
            &self.npcs,
            &mut self.tier_assignments,
            &mut self.bfs_distances_cache,
            world,
        );
    }

    /// Applies the results of a Tier 4 tick to NPC state.
    ///
    /// See [`crate::tier4::apply_events`] for full documentation.
    pub fn apply_tier4_events(
        &mut self,
        events: &[crate::tier4::Tier4Event],
        timestamp: DateTime<Utc>,
        banshee_enabled: bool,
    ) -> Vec<GameEvent> {
        crate::tier4::apply_events(
            &mut self.npcs,
            &mut self.recent_tier4_events,
            events,
            timestamp,
            banshee_enabled,
        )
    }

    /// Runs the banshee tick, heralding imminent deaths and finalising doomed NPCs.
    ///
    /// See [`crate::banshee::tick`] for full documentation.
    pub fn tick_banshee(
        &mut self,
        clock: &GameClock,
        graph: &WorldGraph,
        world_text_log: &mut Vec<String>,
        event_bus: &EventBus,
        player_loc: LocationId,
    ) -> crate::banshee::BansheeReport {
        crate::banshee::tick(
            &mut self.npcs,
            &mut self.recent_tier4_events,
            clock,
            graph,
            world_text_log,
            event_bus,
            player_loc,
        )
    }
}

impl Default for NpcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the unique NPC matching `predicate`, or `None` if zero or
/// multiple match. Helper for `find_by_role_at` — refusing on ambiguity
/// keeps the resolver from silently picking the wrong person.
fn unique_match<'a, F>(npcs: &[&'a Npc], predicate: F) -> Option<&'a Npc>
where
    F: Fn(&Npc) -> bool,
{
    let mut hit: Option<&Npc> = None;
    for &npc in npcs {
        if predicate(npc) {
            if hit.is_some() {
                return None;
            }
            hit = Some(npc);
        }
    }
    hit
}

/// Maps common Irish 1820 role-vocatives to a canonical occupation token.
///
/// Returns the token to look for inside the NPC's `occupation` field. The
/// caller already case-folded the input.
fn role_alias(needle_lower: &str) -> Option<&'static str> {
    match needle_lower {
        // Catholic clergy — "Father", "Fr", "Fr.", "Parson", "Reverend" all
        // address a priest in period dialogue. Rundale's data uses
        // occupation labels like "Parish Priest" / "Curate".
        "father" | "fr" | "fr." | "parson" | "reverend" => Some("priest"),
        // Constabulary — historical Irish parish addressed peace officers
        // as "Constable" or "Officer".
        "officer" => Some("constable"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{load_test_graph, make_chain_graph, make_test_npc, make_test_world};
    use crate::types::Relationship;
    use chrono::{Duration, TimeZone};

    #[test]
    fn test_manager_new_empty() {
        let mgr = NpcManager::new();
        assert_eq!(mgr.npc_count(), 0);
    }

    #[test]
    fn test_introduction_tracking() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));

        assert!(!mgr.is_introduced(NpcId(1)));
        mgr.mark_introduced(NpcId(1));
        assert!(mgr.is_introduced(NpcId(1)));
        assert!(!mgr.is_introduced(NpcId(2)));
    }

    #[test]
    fn test_display_name_uses_introduction_state() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));
        let npc = mgr.get(NpcId(1)).unwrap().clone();

        assert_eq!(mgr.display_name(&npc), "a person");
        mgr.mark_introduced(NpcId(1));
        let npc = mgr.get(NpcId(1)).unwrap().clone();
        assert_eq!(mgr.display_name(&npc), "NPC 1");
    }

    #[test]
    fn test_add_and_get_npc() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));

        assert_eq!(mgr.npc_count(), 1);
        assert!(mgr.get(NpcId(1)).is_some());
        assert_eq!(mgr.get(NpcId(1)).unwrap().name, "NPC 1");
        assert!(mgr.get(NpcId(99)).is_none());
    }

    #[test]
    fn test_npcs_at_location() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));
        mgr.add_npc(make_test_npc(2, 2));
        mgr.add_npc(make_test_npc(3, 3));

        assert_eq!(mgr.npcs_at(LocationId(2)).len(), 2);
        assert_eq!(mgr.npcs_at(LocationId(3)).len(), 1);
        assert!(mgr.npcs_at(LocationId(99)).is_empty());
    }

    #[test]
    fn test_in_transit_excluded_from_npcs_at() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.state = NpcState::InTransit {
            from: LocationId(2),
            to: LocationId(3),
            arrives_at: chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(),
        };
        mgr.add_npc(npc);

        assert!(mgr.npcs_at(LocationId(2)).is_empty());
        assert!(mgr.npcs_at(LocationId(3)).is_empty());
    }

    #[test]
    fn test_default_manager() {
        let mgr = NpcManager::default();
        assert_eq!(mgr.npc_count(), 0);
    }

    #[test]
    fn test_find_by_name_exact_match() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.name = "Padraig Darcy".to_string();
        mgr.add_npc(npc);
        mgr.mark_introduced(NpcId(1));

        let found = mgr.find_by_name("Padraig Darcy", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_find_by_name_case_insensitive() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.name = "Padraig Darcy".to_string();
        mgr.add_npc(npc);
        mgr.mark_introduced(NpcId(1));

        assert!(mgr.find_by_name("padraig darcy", LocationId(2)).is_some());
    }

    #[test]
    fn test_find_by_name_first_name_match() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.name = "Padraig Darcy".to_string();
        mgr.add_npc(npc);
        mgr.mark_introduced(NpcId(1));

        let found = mgr.find_by_name("Padraig", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_find_by_name_wrong_location() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.name = "Padraig Darcy".to_string();
        mgr.add_npc(npc);
        mgr.mark_introduced(NpcId(1));

        assert!(mgr.find_by_name("Padraig", LocationId(99)).is_none());
    }

    #[test]
    fn test_find_by_name_no_match() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));
        mgr.mark_introduced(NpcId(1));

        assert!(mgr.find_by_name("Nobody", LocationId(2)).is_none());
    }

    #[test]
    fn test_find_by_role_at_unique_match_resolves() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.occupation = "Widow".to_string();
        mgr.add_npc(npc);

        let found = mgr.find_by_role_at("Widow", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_find_by_role_at_case_insensitive() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.occupation = "Father".to_string();
        mgr.add_npc(npc);

        assert!(mgr.find_by_role_at("father", LocationId(2)).is_some());
        assert!(mgr.find_by_role_at("FATHER", LocationId(2)).is_some());
    }

    #[test]
    fn test_find_by_role_at_ambiguous_returns_none() {
        let mut mgr = NpcManager::new();
        let mut a = make_test_npc(1, 2);
        a.occupation = "Farmer".to_string();
        let mut b = make_test_npc(2, 2);
        b.occupation = "Farmer".to_string();
        mgr.add_npc(a);
        mgr.add_npc(b);

        assert!(
            mgr.find_by_role_at("Farmer", LocationId(2)).is_none(),
            "two NPCs share the role — resolver must refuse to guess"
        );
    }

    #[test]
    fn test_find_by_role_at_wrong_location_returns_none() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.occupation = "Widow".to_string();
        mgr.add_npc(npc);

        assert!(mgr.find_by_role_at("Widow", LocationId(99)).is_none());
    }

    #[test]
    fn test_find_by_role_at_token_overlap_priest_matches_parish_priest() {
        // Player addresses "Priest" — Rundale data uses "Parish Priest".
        let mut mgr = NpcManager::new();
        let mut tierney = make_test_npc(1, 2);
        tierney.occupation = "Parish Priest".to_string();
        mgr.add_npc(tierney);

        let found = mgr.find_by_role_at("Priest", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_find_by_role_at_token_overlap_constable_matches_retired_constable() {
        let mut mgr = NpcManager::new();
        let mut flanagan = make_test_npc(1, 2);
        flanagan.occupation = "Retired Constable".to_string();
        mgr.add_npc(flanagan);

        assert!(mgr.find_by_role_at("Constable", LocationId(2)).is_some());
    }

    #[test]
    fn test_find_by_role_at_alias_father_routes_to_priest() {
        // "Father, a word" — period-correct vocative for a Catholic priest.
        let mut mgr = NpcManager::new();
        let mut tierney = make_test_npc(1, 2);
        tierney.occupation = "Parish Priest".to_string();
        mgr.add_npc(tierney);

        let found = mgr.find_by_role_at("Father", LocationId(2));
        assert!(found.is_some(), "Father vocative should resolve to priest");
        assert_eq!(found.unwrap().id, NpcId(1));

        // "Fr." / "Fr" abbreviations.
        assert!(mgr.find_by_role_at("Fr.", LocationId(2)).is_some());
        assert!(mgr.find_by_role_at("fr", LocationId(2)).is_some());
    }

    #[test]
    fn test_find_by_role_at_alias_refuses_when_ambiguous_across_priests() {
        let mut mgr = NpcManager::new();
        let mut priest = make_test_npc(1, 2);
        priest.occupation = "Parish Priest".to_string();
        let mut curate = make_test_npc(2, 2);
        curate.occupation = "Curate Priest".to_string();
        mgr.add_npc(priest);
        mgr.add_npc(curate);

        assert!(
            mgr.find_by_role_at("Father", LocationId(2)).is_none(),
            "two priests share the vocative — must refuse"
        );
    }

    #[test]
    fn test_find_by_role_at_unknown_alias_does_not_match() {
        let mut mgr = NpcManager::new();
        let mut publican = make_test_npc(1, 2);
        publican.occupation = "Publican".to_string();
        mgr.add_npc(publican);

        // "Sir" is not a registered alias and isn't a token of "Publican".
        assert!(mgr.find_by_role_at("Sir", LocationId(2)).is_none());
    }

    #[test]
    fn test_find_by_role_at_empty_input_returns_none() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.occupation = "Widow".to_string();
        mgr.add_npc(npc);

        assert!(mgr.find_by_role_at("", LocationId(2)).is_none());
        assert!(mgr.find_by_role_at("   ", LocationId(2)).is_none());
    }

    #[test]
    fn test_find_by_name_unintroduced_uses_brief_description() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.brief_description = "an older man behind the bar".to_string();
        mgr.add_npc(npc);

        let found = mgr.find_by_name("an older man behind the bar", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_known_roster_unions_home_and_work_matches() {
        let mut mgr = NpcManager::new();
        let mut subject = make_test_npc(1, 10);
        subject.home = Some(LocationId(10));
        subject.workplace = Some(LocationId(20));
        mgr.add_npc(subject.clone());

        let mut home_mate = make_test_npc(2, 30);
        home_mate.home = Some(LocationId(10));
        home_mate.workplace = Some(LocationId(30));
        mgr.add_npc(home_mate);

        let mut work_mate = make_test_npc(3, 20);
        work_mate.home = Some(LocationId(40));
        work_mate.workplace = Some(LocationId(20));
        mgr.add_npc(work_mate);

        let mut visitor = make_test_npc(4, 10);
        visitor.home = Some(LocationId(50));
        mgr.add_npc(visitor);

        let mut both = make_test_npc(5, 10);
        both.home = Some(LocationId(10));
        both.workplace = Some(LocationId(20));
        mgr.add_npc(both);

        let mut stranger = make_test_npc(6, 99);
        stranger.home = Some(LocationId(99));
        stranger.workplace = Some(LocationId(98));
        mgr.add_npc(stranger);

        let roster = mgr.known_roster(&subject);
        let ids: HashSet<NpcId> = roster.iter().map(|(id, _, _)| *id).collect();

        assert!(ids.contains(&NpcId(2)), "home-mate should be in roster");
        assert!(ids.contains(&NpcId(3)), "work-mate should be in roster");
        assert!(
            ids.contains(&NpcId(4)),
            "co-present at home should be in roster"
        );
        assert!(
            ids.contains(&NpcId(5)),
            "sharing both home and work should be in roster"
        );
        assert!(
            !ids.contains(&NpcId(6)),
            "unrelated NPC must not be in roster"
        );
        assert!(
            !ids.contains(&NpcId(1)),
            "subject must not be in its own roster"
        );
        assert_eq!(ids.len(), roster.len(), "no duplicates");
    }

    #[test]
    fn test_load_from_file() {
        let path = std::path::Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let mgr = NpcManager::load_from_file(path).unwrap();
        assert_eq!(mgr.npc_count(), 23);
    }

    // ── Tick state management ────────────────────────────────────────────────

    #[test]
    fn test_needs_tier2_tick() {
        let mgr = NpcManager::new();
        let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier2_tick(now));
    }

    #[test]
    fn test_tier2_tick_interval() {
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        mgr.record_tier2_tick(t0);

        assert!(!mgr.needs_tier2_tick(t0 + Duration::minutes(3)));
        assert!(mgr.needs_tier2_tick(t0 + Duration::minutes(5)));
        assert!(mgr.needs_tier2_tick(t0 + Duration::minutes(10)));
    }

    #[test]
    fn test_needs_tier2_tick_with_config_custom_interval() {
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        mgr.record_tier2_tick(t0);

        let config = CognitiveTierConfig {
            tier2_tick_interval_minutes: 10,
            ..CognitiveTierConfig::default()
        };
        assert!(!mgr.needs_tier2_tick_with_config(t0 + Duration::minutes(5), &config));
        assert!(mgr.needs_tier2_tick_with_config(t0 + Duration::minutes(10), &config));
    }

    #[test]
    fn test_needs_tier2_tick_with_config_first_tick() {
        let mgr = NpcManager::new();
        let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        let config = CognitiveTierConfig {
            tier2_tick_interval_minutes: 10,
            ..CognitiveTierConfig::default()
        };
        assert!(mgr.needs_tier2_tick_with_config(now, &config));
    }

    #[test]
    fn test_tier2_in_flight_tracking() {
        let mut mgr = NpcManager::new();
        assert!(!mgr.tier2_in_flight());
        mgr.set_tier2_in_flight(true);
        assert!(mgr.tier2_in_flight());
        mgr.set_tier2_in_flight(false);
        assert!(!mgr.tier2_in_flight());
    }

    #[test]
    fn test_tier3_tick_interval() {
        let config = CognitiveTierConfig::default();
        let mgr = NpcManager::new();
        let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier3_tick_with_config(now, &config));
    }

    #[test]
    fn test_tier3_tick_not_yet_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier3_tick(t0);
        let t1 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(!mgr.needs_tier3_tick_with_config(t1, &config));
    }

    #[test]
    fn test_tier3_tick_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier3_tick(t0);
        let t1 = chrono::Utc.with_ymd_and_hms(1820, 3, 21, 0, 0, 0).unwrap();
        assert!(mgr.needs_tier3_tick_with_config(t1, &config));
    }

    #[test]
    fn test_tier3_in_flight_tracking() {
        let mut mgr = NpcManager::new();
        assert!(!mgr.tier3_in_flight());
        mgr.set_tier3_in_flight(true);
        assert!(mgr.tier3_in_flight());
        mgr.set_tier3_in_flight(false);
        assert!(!mgr.tier3_in_flight());
    }

    #[test]
    fn test_tier4_tick_never_ticked() {
        let mgr = NpcManager::new();
        let now = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick(now));
        assert!(mgr.last_tier4_game_time().is_none());
    }

    #[test]
    fn test_tier4_tick_not_yet_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier4_tick(t0);
        let t1 = chrono::Utc.with_ymd_and_hms(1820, 4, 19, 0, 0, 0).unwrap();
        assert!(!mgr.needs_tier4_tick_with_config(t1, &config));
        assert_eq!(mgr.last_tier4_game_time(), Some(t0));
    }

    #[test]
    fn test_tier4_tick_due_after_interval() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = chrono::Utc.with_ymd_and_hms(1820, 1, 1, 0, 0, 0).unwrap();
        mgr.record_tier4_tick(t0);
        let t1 = chrono::Utc.with_ymd_and_hms(1820, 4, 1, 0, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick_with_config(t1, &config));
    }

    // ── remove_npc reference-scrubbing (#339) ────────────────────────────────

    #[test]
    fn remove_npc_scrubs_all_references() {
        let mut mgr = NpcManager::new();
        for id in [10, 20, 30] {
            mgr.add_npc(make_test_npc(id, 0));
        }
        mgr.tier_assignments.insert(NpcId(20), CogTier::Tier1);
        mgr.introduced_npcs.insert(NpcId(20));
        mgr.npcs_who_know_player_name.insert(NpcId(20));

        mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
            NpcId(20),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );
        mgr.npcs.get_mut(&NpcId(30)).unwrap().relationships.insert(
            NpcId(20),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );
        mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
            NpcId(30),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );

        let removed = mgr.remove_npc(NpcId(20));
        assert!(removed.is_some());

        assert!(mgr.get(NpcId(20)).is_none());
        assert!(!mgr.tier_assignments.contains_key(&NpcId(20)));
        assert!(!mgr.introduced_npcs.contains(&NpcId(20)));
        assert!(!mgr.npcs_who_know_player_name.contains(&NpcId(20)));

        let n10 = mgr.get(NpcId(10)).unwrap();
        assert!(!n10.relationships.contains_key(&NpcId(20)));
        assert!(n10.relationships.contains_key(&NpcId(30)));
        let n30 = mgr.get(NpcId(30)).unwrap();
        assert!(!n30.relationships.contains_key(&NpcId(20)));
    }

    #[test]
    fn remove_npc_returns_none_for_missing_id() {
        let mut mgr = NpcManager::new();
        assert!(mgr.remove_npc(NpcId(9_999_999)).is_none());
    }

    // ── Integration tests (manager coordinates subsystems) ───────────────────

    /// tier2_groups depends on both assign_tiers (writes tier_assignments)
    /// and npcs state — tested here as an integration of both.
    #[test]
    fn test_tier2_groups() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));
        mgr.add_npc(make_test_npc(2, 2));
        mgr.add_npc(make_test_npc(3, 3));

        let world = make_test_world(graph, 1);
        mgr.assign_tiers(&world, &[]);

        let groups = mgr.tier2_groups();
        assert_eq!(groups.get(&LocationId(2)).map(|v| v.len()), Some(2));
    }

    #[test]
    fn test_tier2_dispatch_wiring_cycle() {
        use parish_world::WorldState;

        let graph = make_chain_graph(4);
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(20, 2)); // distance 2 → Tier2

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(20)), Some(CogTier::Tier2));

        let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

        assert!(mgr.needs_tier2_tick(now));
        assert!(!mgr.tier2_in_flight());
        assert!(mgr.needs_tier2_tick(now) && !mgr.tier2_in_flight());

        mgr.set_tier2_in_flight(true);
        assert!(!mgr.needs_tier2_tick(now) || mgr.tier2_in_flight());

        let groups = mgr.tier2_groups();
        assert!(!groups.is_empty());

        mgr.record_tier2_tick(now);
        mgr.set_tier2_in_flight(false);

        assert_eq!(mgr.last_tier2_game_time(), Some(now));
        assert!(!mgr.tier2_in_flight());
        assert!(!mgr.needs_tier2_tick(now));
    }

    #[test]
    fn test_tier3_dispatch_wiring_cycle() {
        use crate::ticks::tier3_snapshot_from_npc;
        use parish_world::WorldState;

        let graph = make_chain_graph(6);
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(10, 4)); // distance 4 → Tier3

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(10)), Some(CogTier::Tier3));

        let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

        assert!(mgr.needs_tier3_tick(now));
        assert!(!mgr.tier3_in_flight());
        mgr.set_tier3_in_flight(true);
        assert!(!mgr.needs_tier3_tick(now) || mgr.tier3_in_flight());

        let tier3_ids = mgr.npcs_in_tier(CogTier::Tier3);
        assert!(!tier3_ids.is_empty());
        let npc_names: std::collections::HashMap<_, _> =
            mgr.all_npcs().map(|n| (n.id, n.name.clone())).collect();
        let snapshots: Vec<_> = tier3_ids
            .iter()
            .filter_map(|id| mgr.get(*id))
            .map(|npc| tier3_snapshot_from_npc(npc, &world.graph, &npc_names))
            .collect();
        assert!(!snapshots.is_empty());

        mgr.record_tier3_tick(now);
        mgr.set_tier3_in_flight(false);

        assert_eq!(mgr.last_tier3_game_time(), Some(now));
        assert!(!mgr.tier3_in_flight());
        assert!(!mgr.needs_tier3_tick(now));
    }

    #[test]
    fn test_tier4_dispatch_wiring_cycle() {
        use crate::tier4::tick_tier4;
        use parish_world::WorldState;
        use std::collections::HashSet;

        let graph = make_chain_graph(6);
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(99, 6)); // distance 6 → Tier4

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(99)), Some(CogTier::Tier4));

        let now = chrono::Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick(now));

        let tier4_ids: HashSet<NpcId> = mgr.npcs_in_tier(CogTier::Tier4).into_iter().collect();
        let events = {
            let mut tier4_refs: Vec<&mut Npc> = mgr
                .npcs_mut()
                .values_mut()
                .filter(|n| tier4_ids.contains(&n.id))
                .collect();
            let season = world.clock.season();
            let game_date = now.date_naive();
            let mut rng = rand::rng();
            tick_tier4(&mut tier4_refs, season, game_date, &mut rng)
        };
        let game_events = mgr.apply_tier4_events(&events, now, true);
        for evt in game_events {
            world.event_bus.publish(evt);
        }
        mgr.record_tier4_tick(now);

        assert_eq!(mgr.last_tier4_game_time(), Some(now));
        assert!(!mgr.needs_tier4_tick(now));
    }

    // ── Reaction-emoji diversity sensor (issue #995) ────────────────────────

    #[test]
    fn reaction_emoji_buffer_caps_at_capacity() {
        let mut mgr = NpcManager::new();
        for _ in 0..(REACTION_EMOJI_BUFFER_CAPACITY + 4) {
            mgr.record_reaction_emoji("🤔");
        }
        assert_eq!(
            mgr.reaction_emoji_buffer().len(),
            REACTION_EMOJI_BUFFER_CAPACITY,
            "buffer must cap at REACTION_EMOJI_BUFFER_CAPACITY"
        );
    }

    #[test]
    fn reaction_emoji_diverse_history_does_not_flag() {
        // Eight distinct emoji → distinct_count=8, dominant_ratio=1/8 = 0.125
        // → detector returns None, no WARN, no active state.
        let mut mgr = NpcManager::new();
        for e in &["🤔", "😊", "😢", "😡", "😏", "👀", "🍺", "✝️"] {
            mgr.record_reaction_emoji(e);
        }
        assert!(
            !mgr.reaction_monoculture_active,
            "diverse buffer must leave the sensor un-flagged"
        );
    }

    #[test]
    fn reaction_emoji_monoculture_flips_active_state() {
        let mut mgr = NpcManager::new();
        for _ in 0..REACTION_EMOJI_BUFFER_CAPACITY {
            mgr.record_reaction_emoji("🤔");
        }
        assert!(
            mgr.reaction_monoculture_active,
            "sustained same-emoji push must flip the sensor to active"
        );
    }

    #[test]
    fn reaction_emoji_monoculture_clears_when_diversity_returns() {
        let mut mgr = NpcManager::new();
        for _ in 0..REACTION_EMOJI_BUFFER_CAPACITY {
            mgr.record_reaction_emoji("🤔");
        }
        assert!(mgr.reaction_monoculture_active);

        // Flush the buffer with distinct emoji until ratio falls back below 0.7.
        for e in &["😊", "😢", "😡", "😏", "👀", "🍺", "✝️", "😳"] {
            mgr.record_reaction_emoji(e);
        }
        assert!(
            !mgr.reaction_monoculture_active,
            "recovering diversity must clear the sensor so it can fire again later"
        );
    }
}
