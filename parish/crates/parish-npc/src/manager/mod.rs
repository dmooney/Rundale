//! Central NPC coordinator.
//!
//! Owns all NPCs, tracks cognitive tiers, and manages introduction state.
//! Heavy subsystems — schedule resolution, tier assignment, banshee, tier4
//! event application — live in their own modules; the methods here are thin
//! wrappers that delegate and expose the stable public API.

use std::collections::{HashMap, HashSet, VecDeque};

use chrono::{DateTime, Utc};

use crate::types::CogTier;
use crate::{Npc, NpcId};
use parish_types::LocationId;

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

#[derive(Clone)]
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

mod intro;
mod lookup;
mod tiers;

#[cfg(test)]
mod tests;

impl NpcManager {
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
}

impl Default for NpcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns the unique NPC matching `predicate`, or `None` if zero or
/// multiple match. Helper for `find_by_role_at` — refusing on ambiguity
/// keeps the resolver from silently picking the wrong person.
pub(super) fn unique_match<'a, F>(npcs: &[&'a Npc], predicate: F) -> Option<&'a Npc>
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
pub(super) fn role_alias(needle_lower: &str) -> Option<&'static str> {
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
