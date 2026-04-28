//! Central NPC coordinator.
//!
//! Manages all NPCs in the world, assigns cognitive tiers based on
//! proximity to the player, advances NPC schedules, and provides
//! queries for NPCs at specific locations.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

use crate::data::load_npcs_from_file;
use crate::transitions::{deflate_npc_state, inflate_npc_context};
use crate::types::{CogTier, NpcState};
use crate::{Npc, NpcId};
use parish_config::CognitiveTierConfig;
use parish_types::LocationId;
use parish_types::ParishError;
use parish_world::WorldState;
use parish_world::events::GameEvent;
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

/// An event produced by NPC schedule ticking.
#[derive(Debug, Clone)]
pub struct ScheduleEvent {
    /// Id of the NPC this event concerns.
    pub npc_id: NpcId,
    /// Name of the NPC.
    pub npc_name: String,
    /// What happened.
    pub kind: ScheduleEventKind,
}

/// The kind of schedule event.
#[derive(Debug, Clone)]
pub enum ScheduleEventKind {
    /// NPC departed from a location.
    Departed {
        /// Location they left.
        from: LocationId,
        /// Location they're heading to.
        to: LocationId,
        /// Name of the destination.
        to_name: String,
        /// Travel time in minutes.
        minutes: u16,
    },
    /// NPC arrived at a location.
    Arrived {
        /// Location they arrived at.
        location: LocationId,
        /// Name of the location.
        location_name: String,
    },
}

impl ScheduleEvent {
    /// Formats this event as a short debug log string.
    pub fn debug_string(&self) -> String {
        match &self.kind {
            ScheduleEventKind::Departed {
                to_name, minutes, ..
            } => {
                format!("{} heading to {} ({}min)", self.npc_name, to_name, minutes)
            }
            ScheduleEventKind::Arrived { location_name, .. } => {
                format!("{} arrived at {}", self.npc_name, location_name)
            }
        }
    }
}

/// A tier transition that occurred during `assign_tiers`.
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

/// Central coordinator for all NPC state and behavior.
///
/// Owns all NPCs, assigns cognitive tiers based on distance from the
/// player, and advances NPC schedules so they move between locations
/// according to their daily routines.
///
/// Also tracks which NPCs have been introduced to the player. Before
/// introduction, NPCs are referred to by a brief anonymous description
/// (e.g., "a priest") rather than by name.
pub struct NpcManager {
    /// All NPCs keyed by their unique id.
    npcs: HashMap<NpcId, Npc>,
    /// Current cognitive tier assignment for each NPC.
    tier_assignments: HashMap<NpcId, CogTier>,
    /// Game time of the last Tier 2 tick (None if never ticked).
    last_tier2_game_time: Option<DateTime<Utc>>,
    /// Whether a Tier 2 background inference is currently in-flight.
    tier2_in_flight: bool,
    /// Game time of the last Tier 3 tick (None if never ticked).
    last_tier3_game_time: Option<DateTime<Utc>>,
    /// Whether a Tier 3 batch inference is currently in-flight.
    tier3_in_flight: bool,
    /// Game time of the last Tier 4 tick (None if never ticked).
    last_tier4_game_time: Option<DateTime<Utc>>,
    /// Set of NPC ids that have introduced themselves to the player.
    introduced_npcs: HashSet<NpcId>,
    /// Set of NPC ids that know the player's name (learned via dialogue or gossip).
    npcs_who_know_player_name: HashSet<NpcId>,
    /// Ring buffer of the last 5 Tier 4 life-event descriptions (newest last).
    recent_tier4_events: VecDeque<String>,
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
            last_tier2_game_time: None,
            tier2_in_flight: false,
            last_tier3_game_time: None,
            tier3_in_flight: false,
            last_tier4_game_time: None,
            introduced_npcs: HashSet::new(),
            npcs_who_know_player_name: HashSet::new(),
            recent_tier4_events: VecDeque::with_capacity(5),
            bfs_distances_cache: None,
        }
    }

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
    /// A bare `self.npcs.remove(id)` would leave:
    ///
    /// - `tier_assignments` still keyed by the dead id (handled by
    ///   the existing death sites, included here for symmetry).
    /// - `introduced_npcs` / `npcs_who_know_player_name` still
    ///   containing the dead id (display-name lookups would still
    ///   greet the deceased; gossip would still walk them).
    /// - Every surviving NPC's `relationships` map still pointing
    ///   at the dead id, leading to dereferences-of-nothing in
    ///   gossip and tier-2 affinity scans.
    ///
    /// Call this from every death-handling path instead of the bare
    /// `self.npcs.remove(...)`. Returns the removed NPC if it existed,
    /// matching `HashMap::remove` semantics.
    pub fn remove_npc(&mut self, id: NpcId) -> Option<Npc> {
        let removed = self.npcs.remove(&id);
        self.tier_assignments.remove(&id);
        self.introduced_npcs.remove(&id);
        self.npcs_who_know_player_name.remove(&id);
        // Scrub the dead id from every surviving NPC's relationships.
        // Cheap: the map is small per NPC, and remove is a no-op when
        // the key is absent.
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
    ///
    /// There is no need to call this when the player moves; the cache key
    /// is the player's location and a location mismatch is detected
    /// automatically inside `assign_tiers`.
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
    ///
    /// NPCs that are in transit are excluded.
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
    /// Matches against the NPC's display name (full name if introduced,
    /// brief description otherwise). Checks for exact match first, then
    /// falls back to first-name prefix matching (e.g., "Padraig" matches
    /// "Padraig Darcy").
    pub fn find_by_name(&self, name: &str, location: LocationId) -> Option<&Npc> {
        let npcs = self.npcs_at(location);
        let lower = name.to_lowercase();

        // Single-pass: lowercase each NPC's name and display name once, then
        // check both exact and prefix match. This avoids the previous two-pass
        // approach which lowercased every NPC name twice (once per pass).
        let mut prefix_match: Option<&Npc> = None;
        for &npc in &npcs {
            let name_lower = npc.name.to_lowercase();
            let display_lower = self.display_name(npc).to_lowercase();

            // Exact match takes priority — return immediately
            if name_lower == lower || display_lower == lower {
                return Some(npc);
            }

            // First-name prefix match — remember first hit but keep looking
            // for an exact match
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

    /// Finds an NPC by exact name (case-insensitive), searching all NPCs.
    ///
    /// Returns a mutable reference for updating reaction logs, mood, etc.
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

    /// Returns the NPCs that a given NPC "knows" — the union of relationships,
    /// memory participants, and co-residents at home/workplace.
    ///
    /// Returns a vec of `(NpcId, name, occupation)` tuples, deduplicated.
    pub fn known_roster(&self, npc: &Npc) -> Vec<(NpcId, String, String)> {
        let mut known_ids: std::collections::HashSet<NpcId> = std::collections::HashSet::new();

        // 1. NPCs in relationships
        for target_id in npc.relationships.keys() {
            known_ids.insert(*target_id);
        }

        // 2. NPCs mentioned in short-term memory (by participant ID)
        for entry in npc.memory.entries() {
            for &pid in &entry.participants {
                if pid != npc.id && pid != NpcId(0) {
                    known_ids.insert(pid);
                }
            }
        }

        // 4. NPCs at the same home or workplace location.
        //
        // Perf: single pass over `self.npcs` instead of two separate scans.
        // Called once per NPC dialogue setup (see ipc/handlers.rs::build_enhanced_system_prompt),
        // which is a hot path during conversations. For N NPCs this halves the
        // HashMap traversals from 2N to N per call.
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

        // Resolve to (id, name, occupation) tuples
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

    /// Assigns cognitive tiers to all NPCs based on BFS distance from the player.
    ///
    /// Uses the default [`CognitiveTierConfig`] for distance thresholds.
    ///
    /// When an NPC's tier changes, inflation (promotion) or deflation
    /// (demotion) is performed to manage narrative context. Tier 1
    /// arrivals are published as [`GameEvent::NpcArrived`] on the
    /// world's event bus.
    pub fn assign_tiers(
        &mut self,
        world: &WorldState,
        recent_events: &[GameEvent],
    ) -> Vec<TierTransition> {
        let player_location = world.player_location;
        let graph = &world.graph;
        let game_time = world.clock.now();
        let config = CognitiveTierConfig::default();
        // BFS from player location — reuse cached result when the player
        // has not moved since the last call.  The graph is immutable during
        // a session, so distances are stable as long as the source is the same.
        let cache_hit = self
            .bfs_distances_cache
            .as_ref()
            .is_some_and(|(loc, _)| *loc == player_location);
        if !cache_hit {
            let distances = bfs_distances(player_location, graph);
            self.bfs_distances_cache = Some((player_location, distances));
        }
        // SAFETY: we just ensured the cache is populated above.
        let distances = &self
            .bfs_distances_cache
            .as_ref()
            .expect("cache populated above")
            .1;

        // First pass: compute new tier assignments and detect changes
        let mut changes: Vec<(NpcId, CogTier, CogTier)> = Vec::new();

        for npc in self.npcs.values() {
            let distance = match npc.state {
                NpcState::Present => distances.get(&npc.location).copied(),
                NpcState::InTransit { from, to, .. } => {
                    // Use the closer of from/to
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

            let old_tier = self
                .tier_assignments
                .get(&npc.id)
                .copied()
                .unwrap_or(CogTier::Tier4);

            if new_tier != old_tier {
                changes.push((npc.id, old_tier, new_tier));
            }

            self.tier_assignments.insert(npc.id, new_tier);
        }

        // Second pass: handle tier transitions (inflate/deflate)
        let mut transitions = Vec::new();

        for (npc_id, old_tier, new_tier) in &changes {
            let promoted = tier_rank(*new_tier) < tier_rank(*old_tier);
            let demoted = tier_rank(*new_tier) > tier_rank(*old_tier);
            let npc_name = self
                .npcs
                .get(npc_id)
                .map(|n| n.name.clone())
                .unwrap_or_default();

            if promoted && let Some(npc) = self.npcs.get_mut(npc_id) {
                inflate_npc_context(npc, recent_events, game_time);
                tracing::debug!(
                    npc_id = npc_id.0,
                    old_tier = ?old_tier,
                    new_tier = ?new_tier,
                    "NPC promoted (inflated)"
                );
            }

            if demoted && let Some(npc) = self.npcs.get(npc_id) {
                let summary = deflate_npc_state(npc, recent_events);
                if let Some(npc_mut) = self.npcs.get_mut(npc_id) {
                    npc_mut.deflated_summary = Some(summary);
                }
                tracing::debug!(
                    npc_id = npc_id.0,
                    old_tier = ?old_tier,
                    new_tier = ?new_tier,
                    "NPC demoted (deflated)"
                );
            }

            // Publish arrival events for NPCs entering Tier 1
            if *new_tier == CogTier::Tier1
                && *old_tier != CogTier::Tier1
                && let Some(npc) = self.npcs.get(npc_id)
            {
                world.event_bus.publish(GameEvent::NpcArrived {
                    npc_id: *npc_id,
                    location: npc.location,
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
            tier1 = self.tier1_npcs().len(),
            tier2 = self.tier2_npcs().len(),
            transitions = transitions.len(),
            "Tier assignment complete"
        );

        transitions
    }

    /// Returns the current cognitive tier for an NPC.
    pub fn tier_of(&self, id: NpcId) -> Option<CogTier> {
        self.tier_assignments.get(&id).copied()
    }

    /// Returns the ids of all NPCs assigned to Tier 1.
    pub fn tier1_npcs(&self) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, tier)| **tier == CogTier::Tier1)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Returns the ids of all NPCs assigned to Tier 2.
    pub fn tier2_npcs(&self) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, tier)| **tier == CogTier::Tier2)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Advances NPC schedules based on the current game time.
    ///
    /// For each NPC that is `Present` and whose schedule says they should
    /// be somewhere else, starts transit. For NPCs that are `InTransit`
    /// and whose arrival time has passed, completes the move.
    ///
    /// Returns a list of structured schedule events describing what happened.
    pub fn tick_schedules(
        &mut self,
        clock: &GameClock,
        graph: &WorldGraph,
        weather: parish_types::Weather,
    ) -> Vec<ScheduleEvent> {
        let now = clock.now();
        let current_hour = now.hour() as u8;
        let season = clock.season();
        let day_type = clock.day_type();
        let mut events = Vec::new();

        // Pre-collect cuaird targets: for each NPC, gather friend home locations.
        let cuaird_targets: HashMap<NpcId, Vec<LocationId>> = self
            .npcs
            .iter()
            .map(|(id, npc)| {
                let friends: Vec<LocationId> = npc
                    .relationships
                    .iter()
                    .filter(|(_, r)| r.strength > 0.3)
                    .filter_map(|(friend_id, _)| self.npcs.get(friend_id).and_then(|f| f.home))
                    .collect();
                (*id, friends)
            })
            .collect();

        let npc_ids: Vec<NpcId> = self.npcs.keys().copied().collect();

        for id in npc_ids {
            let Some(npc) = self.npcs.get(&id) else {
                continue;
            };

            match &npc.state {
                NpcState::Present => {
                    if let Some(mut desired) = npc.desired_location(current_hour, season, day_type)
                    {
                        // Cuaird override: rotate visiting location by day-of-year
                        if let Some(entry) = npc.schedule_entry(current_hour, season, day_type)
                            && entry.cuaird
                            && let Some(friends) = cuaird_targets.get(&id)
                            && !friends.is_empty()
                        {
                            let day_of_year = now.ordinal() as usize;
                            desired = friends[day_of_year % friends.len()];
                        }
                        // Weather shelter override: NPCs seek indoor locations in bad weather
                        let dominated_by_rain = matches!(
                            weather,
                            parish_types::Weather::LightRain
                                | parish_types::Weather::HeavyRain
                                | parish_types::Weather::Storm
                        );
                        if dominated_by_rain {
                            let is_farmer = npc.occupation.to_lowercase() == "farmer";
                            let dest_is_outdoor =
                                graph.get(desired).map(|d| !d.indoor).unwrap_or(false);

                            // Farmers tolerate light rain
                            let needs_shelter = if is_farmer {
                                !matches!(weather, parish_types::Weather::LightRain)
                                    && dest_is_outdoor
                            } else {
                                dest_is_outdoor
                            };

                            if needs_shelter {
                                // Override to home if it's indoor, otherwise stay put
                                if let Some(home) = npc.home {
                                    let home_is_indoor =
                                        graph.get(home).map(|d| d.indoor).unwrap_or(false);
                                    if home_is_indoor {
                                        desired = home;
                                    } else {
                                        continue; // No indoor option, stay put
                                    }
                                } else {
                                    continue; // No home, stay put
                                }
                            }
                        }

                        if desired != npc.location
                            && let Some(path) = graph.shortest_path(npc.location, desired)
                        {
                            // NPCs walk at ~1.25 m/s (~4.5 km/h)
                            let travel_minutes = graph.path_travel_time(&path, 1.25);
                            let arrives_at = now + Duration::minutes(travel_minutes as i64);
                            let from = npc.location;
                            let npc_name = npc.name.clone();
                            let dest_name = graph
                                .get(desired)
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| "?".to_string());
                            events.push(ScheduleEvent {
                                npc_id: id,
                                npc_name: npc_name.clone(),
                                kind: ScheduleEventKind::Departed {
                                    from,
                                    to: desired,
                                    to_name: dest_name,
                                    minutes: travel_minutes,
                                },
                            });
                            tracing::debug!(
                                npc = %npc_name,
                                from = from.0,
                                to = desired.0,
                                minutes = travel_minutes,
                                "NPC starting transit"
                            );
                            let Some(npc) = self.npcs.get_mut(&id) else {
                                continue;
                            };
                            npc.state = NpcState::InTransit {
                                from,
                                to: desired,
                                arrives_at,
                            };
                        }
                    }
                }
                NpcState::InTransit { to, arrives_at, .. } => {
                    if now >= *arrives_at {
                        let destination = *to;
                        let npc_name = npc.name.clone();
                        let dest_name = graph
                            .get(destination)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| "?".to_string());
                        events.push(ScheduleEvent {
                            npc_id: id,
                            npc_name: npc_name.clone(),
                            kind: ScheduleEventKind::Arrived {
                                location: destination,
                                location_name: dest_name,
                            },
                        });
                        tracing::debug!(
                            npc = %npc_name,
                            location = destination.0,
                            "NPC arrived"
                        );
                        let Some(npc) = self.npcs.get_mut(&id) else {
                            continue;
                        };
                        npc.location = destination;
                        npc.state = NpcState::Present;
                    }
                }
            }
        }

        events
    }

    /// Returns whether enough game time has elapsed for a Tier 2 tick,
    /// using the given cognitive tier config for the tick interval.
    pub fn needs_tier2_tick_with_config(
        &self,
        current_game_time: DateTime<Utc>,
        config: &CognitiveTierConfig,
    ) -> bool {
        match self.last_tier2_game_time {
            None => true,
            Some(last) => {
                let elapsed = current_game_time.signed_duration_since(last);
                elapsed.num_minutes() >= config.tier2_tick_interval_minutes
            }
        }
    }

    /// Returns the game time of the last Tier 2 tick, if any.
    pub fn last_tier2_game_time(&self) -> Option<DateTime<Utc>> {
        self.last_tier2_game_time
    }

    /// Returns whether enough game time has elapsed for a Tier 2 tick.
    ///
    /// Tier 2 ticks run every 5 game-minutes.
    pub fn needs_tier2_tick(&self, current_game_time: DateTime<Utc>) -> bool {
        self.needs_tier2_tick_with_config(current_game_time, &CognitiveTierConfig::default())
    }

    /// Records that a Tier 2 tick has been performed at the given game time.
    pub fn record_tier2_tick(&mut self, time: DateTime<Utc>) {
        self.last_tier2_game_time = Some(time);
    }

    /// Returns whether a Tier 2 tick is currently in-flight.
    pub fn tier2_in_flight(&self) -> bool {
        self.tier2_in_flight
    }

    /// Sets whether a Tier 2 tick is currently in-flight.
    pub fn set_tier2_in_flight(&mut self, in_flight: bool) {
        self.tier2_in_flight = in_flight;
    }

    /// Returns the ids of all NPCs assigned to Tier 3.
    pub fn tier3_npcs(&self) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, tier)| **tier == CogTier::Tier3)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Returns whether enough game time has elapsed for a Tier 3 tick.
    ///
    /// Tier 3 ticks every 1 in-game day (24 game-hours).
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
        match self.last_tier3_game_time {
            None => true,
            Some(last) => {
                let elapsed = current_game_time.signed_duration_since(last);
                elapsed.num_hours() >= config.tier3_tick_interval_hours
            }
        }
    }

    /// Returns the game time of the last Tier 3 tick, if any.
    pub fn last_tier3_game_time(&self) -> Option<DateTime<Utc>> {
        self.last_tier3_game_time
    }

    /// Records that a Tier 3 tick has been performed at the given game time.
    pub fn record_tier3_tick(&mut self, time: DateTime<Utc>) {
        self.last_tier3_game_time = Some(time);
    }

    /// Returns whether a Tier 3 tick is currently in-flight.
    pub fn tier3_in_flight(&self) -> bool {
        self.tier3_in_flight
    }

    /// Sets whether a Tier 3 tick is currently in-flight.
    pub fn set_tier3_in_flight(&mut self, in_flight: bool) {
        self.tier3_in_flight = in_flight;
    }

    /// Returns the ids of all NPCs assigned to Tier 4.
    pub fn tier4_npcs(&self) -> Vec<NpcId> {
        self.tier_assignments
            .iter()
            .filter(|(_, tier)| **tier == CogTier::Tier4)
            .map(|(id, _)| *id)
            .collect()
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
        match self.last_tier4_game_time {
            None => true,
            Some(last) => {
                let elapsed = current_game_time.signed_duration_since(last).num_days();
                elapsed >= config.tier4_tick_interval_days
            }
        }
    }

    /// Returns the game time of the last Tier 4 tick, if any.
    pub fn last_tier4_game_time(&self) -> Option<DateTime<Utc>> {
        self.last_tier4_game_time
    }

    /// Records that a Tier 4 tick has been performed at the given game time.
    pub fn record_tier4_tick(&mut self, time: DateTime<Utc>) {
        self.last_tier4_game_time = Some(time);
    }

    /// Returns the number of NPCs that have introduced themselves to the player.
    pub fn introduced_count(&self) -> usize {
        self.introduced_npcs.len()
    }

    /// Restores the introduced-NPC set from a snapshot.
    pub fn restore_introduced_set(&mut self, set: HashSet<NpcId>) {
        self.introduced_npcs = set;
    }

    /// Returns the ring buffer of recent Tier 4 life-event descriptions (newest last).
    pub fn recent_tier4_events(&self) -> &VecDeque<String> {
        &self.recent_tier4_events
    }

    /// Applies the results of a Tier 4 tick to NPC state.
    ///
    /// Returns a list of `GameEvent`s to publish on the event bus.
    pub fn apply_tier4_events(
        &mut self,
        events: &[crate::tier4::Tier4Event],
        timestamp: DateTime<Utc>,
        banshee_enabled: bool,
    ) -> Vec<GameEvent> {
        use crate::tier4::Tier4Event;

        let mut game_events = Vec::new();
        // Collect short descriptions for the recent_tier4_events ring buffer.
        let mut life_descriptions: Vec<String> = Vec::new();

        for event in events {
            match event {
                Tier4Event::Illness { npc_id } => {
                    if let Some(npc) = self.npcs.get_mut(npc_id) {
                        npc.is_ill = true;
                        npc.mood = "unwell".to_string();
                        let desc = format!("{} has fallen ill.", npc.name);
                        life_descriptions.push(desc.clone());
                        game_events.push(GameEvent::LifeEvent {
                            npc_id: *npc_id,
                            description: desc,
                            timestamp,
                        });
                        game_events.push(GameEvent::MoodChanged {
                            npc_id: *npc_id,
                            new_mood: "unwell".to_string(),
                            timestamp,
                        });
                    }
                }
                Tier4Event::Recovery { npc_id } => {
                    if let Some(npc) = self.npcs.get_mut(npc_id) {
                        npc.is_ill = false;
                        npc.mood = "content".to_string();
                        let desc = format!("{} has recovered from illness.", npc.name);
                        life_descriptions.push(desc.clone());
                        game_events.push(GameEvent::LifeEvent {
                            npc_id: *npc_id,
                            description: desc,
                            timestamp,
                        });
                        game_events.push(GameEvent::MoodChanged {
                            npc_id: *npc_id,
                            new_mood: "content".to_string(),
                            timestamp,
                        });
                    }
                }
                Tier4Event::Death { npc_id } => {
                    if banshee_enabled {
                        // Schedule the doom a game-day ahead so the banshee tick
                        // has a chance to herald it before the NPC is removed.
                        if let Some(npc) = self.npcs.get_mut(npc_id) {
                            let doom = timestamp
                                + chrono::Duration::hours(crate::banshee::DOOM_LEAD_TIME_HOURS);
                            npc.doom = Some(doom);
                            npc.banshee_heralded = false;
                            let desc = format!("{} is fated to die.", npc.name);
                            life_descriptions.push(desc.clone());
                            game_events.push(GameEvent::LifeEvent {
                                npc_id: *npc_id,
                                description: desc,
                                timestamp,
                            });
                        }
                    } else {
                        // Banshee disabled — immediate removal (pre-banshee behavior).
                        if let Some(npc) = self.npcs.get(npc_id) {
                            let desc = format!("{} has passed away.", npc.name);
                            life_descriptions.push(desc.clone());
                            game_events.push(GameEvent::LifeEvent {
                                npc_id: *npc_id,
                                description: desc,
                                timestamp,
                            });
                        }
                        // Scrub the dead id from introductions, name knowledge,
                        // and every surviving NPC's relationships map (#339).
                        self.remove_npc(*npc_id);
                    }
                }
                Tier4Event::Birth { parent_ids } => {
                    let parent_a_name = self
                        .npcs
                        .get(&parent_ids.0)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let parent_b_name = self
                        .npcs
                        .get(&parent_ids.1)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let desc =
                        format!("A child has been born to {parent_a_name} and {parent_b_name}.");
                    life_descriptions.push(desc.clone());
                    // For now, just publish the event — NPC creation is future work.
                    game_events.push(GameEvent::LifeEvent {
                        npc_id: parent_ids.0,
                        description: desc,
                        timestamp,
                    });
                }
                Tier4Event::SeasonalShift {
                    npc_id,
                    new_schedule_desc,
                } => {
                    if let Some(npc) = self.npcs.get(npc_id) {
                        let desc = format!("{}: {}", npc.name, new_schedule_desc);
                        life_descriptions.push(desc.clone());
                        game_events.push(GameEvent::LifeEvent {
                            npc_id: *npc_id,
                            description: desc,
                            timestamp,
                        });
                    }
                }
                Tier4Event::TradeCompleted { buyer, seller } => {
                    // Boost relationship between buyer and seller by +0.1
                    let buyer_name = self
                        .npcs
                        .get(buyer)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();
                    let seller_name = self
                        .npcs
                        .get(seller)
                        .map(|n| n.name.clone())
                        .unwrap_or_default();

                    if let Some(buyer_npc) = self.npcs.get_mut(buyer)
                        && let Some(rel) = buyer_npc.relationships.get_mut(seller)
                    {
                        rel.adjust_strength(0.1);
                    }
                    if let Some(seller_npc) = self.npcs.get_mut(seller)
                        && let Some(rel) = seller_npc.relationships.get_mut(buyer)
                    {
                        rel.adjust_strength(0.1);
                    }

                    let desc = format!("{buyer_name} completed a trade with {seller_name}.");
                    life_descriptions.push(desc.clone());
                    game_events.push(GameEvent::LifeEvent {
                        npc_id: *buyer,
                        description: desc,
                        timestamp,
                    });
                    game_events.push(GameEvent::RelationshipChanged {
                        npc_a: *buyer,
                        npc_b: *seller,
                        delta: 0.1,
                        timestamp,
                    });
                }
                Tier4Event::FestivalDetected { festival } => {
                    game_events.push(GameEvent::FestivalStarted {
                        name: festival.to_string(),
                        timestamp,
                    });
                }
                Tier4Event::FestivalBond {
                    npc_a,
                    npc_b,
                    festival: _,
                } => {
                    if let Some(npc) = self.npcs.get_mut(npc_a)
                        && let Some(rel) = npc.relationships.get_mut(npc_b)
                    {
                        rel.adjust_strength(0.05);
                    }
                    if let Some(npc) = self.npcs.get_mut(npc_b)
                        && let Some(rel) = npc.relationships.get_mut(npc_a)
                    {
                        rel.adjust_strength(0.05);
                    }
                    game_events.push(GameEvent::RelationshipChanged {
                        npc_a: *npc_a,
                        npc_b: *npc_b,
                        delta: 0.05,
                        timestamp,
                    });
                }
            }
        }

        // Push descriptions into the ring buffer (capacity 5).
        for desc in life_descriptions {
            if self.recent_tier4_events.len() >= 5 {
                self.recent_tier4_events.pop_front();
            }
            self.recent_tier4_events.push_back(desc);
        }

        game_events
    }

    /// Runs the banshee tick, heralding imminent deaths and finalising doomed NPCs.
    ///
    /// Call this alongside [`Self::tick_schedules`] in every backend's tick loop.
    /// It scans NPCs for a scheduled [`Npc::doom`]:
    ///
    /// - If the doom is already past, the NPC is removed and a "died"
    ///   [`crate::banshee::BansheeEvent`] is returned.
    /// - Otherwise, if `now` falls in the night window ahead of the doom and
    ///   the banshee has not yet been heralded, a "heard" event is returned and
    ///   the NPC's [`Npc::banshee_heralded`] flag is set so the same doom can't
    ///   fire a second wail.
    ///
    /// Produced lines are written to `world.text_log` and
    /// [`GameEvent::LifeEvent`] entries are published on `world.event_bus` for
    /// every finalised death, so downstream subscribers (persistence journal,
    /// debug panel) see deaths exactly once.
    ///
    /// The `player_loc` is used only to decide which of the two banshee
    /// voicings ("just beyond the thatch" vs. "out across the parish") to emit.
    pub fn tick_banshee(
        &mut self,
        clock: &GameClock,
        graph: &WorldGraph,
        world_text_log: &mut Vec<String>,
        event_bus: &parish_world::events::EventBus,
        player_loc: LocationId,
    ) -> crate::banshee::BansheeReport {
        use crate::banshee::{BansheeEvent, BansheeReport, herald_line, is_herald_window};

        let now = clock.now();
        let mut report = BansheeReport::default();

        // Collect ids first to avoid simultaneous iteration + mutation.
        let doomed_ids: Vec<NpcId> = self
            .npcs
            .iter()
            .filter_map(|(id, npc)| npc.doom.map(|d| (*id, d, npc.banshee_heralded)))
            .map(|(id, _doom, _h)| id)
            .collect();

        for id in doomed_ids {
            let (doom, already_heralded, name, home) = {
                let Some(npc) = self.npcs.get(&id) else {
                    continue;
                };
                let Some(d) = npc.doom else {
                    // Doom was cleared between collection and processing — skip.
                    continue;
                };
                (d, npc.banshee_heralded, npc.name.clone(), npc.home)
            };

            if now >= doom {
                // Doom has arrived — the NPC dies now. Use remove_npc
                // so introductions, name knowledge, and every other
                // NPC's relationships map all drop the dead id (#339).
                self.remove_npc(id);
                let desc = format!("{} has passed away.", name);
                world_text_log.push(format!(
                    "Word travels before the sun is fully up: {} did not see the morning. \
                     The banshee had the right of it.",
                    name
                ));
                event_bus.publish(GameEvent::LifeEvent {
                    npc_id: id,
                    description: desc,
                    timestamp: now,
                });
                if self.recent_tier4_events.len() >= 5 {
                    self.recent_tier4_events.pop_front();
                }
                self.recent_tier4_events
                    .push_back(format!("{} has passed away.", name));
                report.deaths.push(BansheeEvent::Died {
                    target: id,
                    target_name: name,
                });
                continue;
            }

            if already_heralded {
                continue;
            }

            if !is_herald_window(now, doom) {
                continue;
            }

            // Emit the wail. It rises from the NPC's home when known, else from
            // their current location.
            let home_loc = home.or_else(|| self.npcs.get(&id).map(|n| n.location));
            let home_name = home_loc.and_then(|l| graph.get(l).map(|d| d.name.clone()));
            let near_player = home_loc == Some(player_loc);

            let event = BansheeEvent::Heard {
                target: id,
                target_name: name,
                home: home_loc,
                home_name,
                near_player,
            };

            if let Some(line) = herald_line(&event) {
                world_text_log.push(line);
            }

            // Mark the herald flag on the NPC so we don't wail again for the
            // same doom. The death itself will still fire when `now >= doom`.
            if let Some(npc) = self.npcs.get_mut(&id) {
                npc.banshee_heralded = true;
            }
            report.wails.push(event);
        }

        report
    }

    /// Groups Tier 2 NPCs by their current location.
    ///
    /// Returns a map of location id to the NPC ids at that location.
    /// Only includes NPCs that are `Present` and assigned to Tier 2.
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
}

impl Default for NpcManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Computes BFS distances from a source location to all reachable locations.
fn bfs_distances(source: LocationId, graph: &WorldGraph) -> HashMap<LocationId, u32> {
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

/// Maps cognitive tiers to a numeric rank for comparison.
///
/// Lower rank = closer to the player = higher cognitive fidelity.
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
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::types::{ScheduleEntry, ScheduleVariant, SeasonalSchedule};
    use chrono::TimeZone;

    fn make_test_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            intelligence: crate::types::Intelligence::default(),
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

    fn make_scheduled_npc(id: u32, home: u32, work: u32) -> Npc {
        let mut npc = make_test_npc(id, home);
        npc.schedule = Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![
                    ScheduleEntry {
                        start_hour: 0,
                        end_hour: 7,
                        location: LocationId(home),
                        activity: "sleeping".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 8,
                        end_hour: 17,
                        location: LocationId(work),
                        activity: "working".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 18,
                        end_hour: 23,
                        location: LocationId(home),
                        activity: "evening rest".to_string(),
                        cuaird: false,
                    },
                ],
            }],
        });
        npc
    }

    /// Loads the parish graph for tests that need real topology.
    fn load_test_graph() -> Option<WorldGraph> {
        let path = Path::new("data/parish.json");
        if path.exists() {
            WorldGraph::load_from_file(path).ok()
        } else {
            None
        }
    }

    /// Creates a WorldState with the given graph and player location for tests.
    fn make_test_world(graph: WorldGraph, player_location: u32) -> WorldState {
        let mut world = WorldState::new();
        world.graph = graph;
        world.player_location = LocationId(player_location);
        world
    }

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
        assert!(!mgr.is_introduced(NpcId(2))); // unrelated NPC
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
        mgr.add_npc(make_test_npc(1, 2)); // at pub
        mgr.add_npc(make_test_npc(2, 2)); // at pub
        mgr.add_npc(make_test_npc(3, 3)); // at church

        let at_pub = mgr.npcs_at(LocationId(2));
        assert_eq!(at_pub.len(), 2);

        let at_church = mgr.npcs_at(LocationId(3));
        assert_eq!(at_church.len(), 1);

        let at_nowhere = mgr.npcs_at(LocationId(99));
        assert!(at_nowhere.is_empty());
    }

    #[test]
    fn test_in_transit_excluded_from_npcs_at() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.state = NpcState::InTransit {
            from: LocationId(2),
            to: LocationId(3),
            arrives_at: Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap(),
        };
        mgr.add_npc(npc);

        // Not at origin or destination
        assert!(mgr.npcs_at(LocationId(2)).is_empty());
        assert!(mgr.npcs_at(LocationId(3)).is_empty());
    }

    #[test]
    fn test_tier_assignment_with_parish_graph() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2)); // Pub (1 edge from crossroads)
        mgr.add_npc(make_test_npc(2, 1)); // Crossroads (player is here)
        mgr.add_npc(make_test_npc(3, 11)); // Fairy fort (far)

        // Player at crossroads (id 1)
        let world = make_test_world(graph, 1);
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(2)), Some(CogTier::Tier1)); // same location
        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier2)); // 1 edge
        // Fairy fort distance depends on graph topology
        let fairy_tier = mgr.tier_of(NpcId(3)).unwrap();
        assert!(
            fairy_tier == CogTier::Tier2
                || fairy_tier == CogTier::Tier3
                || fairy_tier == CogTier::Tier4,
            "fairy fort should be Tier2, Tier3, or Tier4 based on distance"
        );
    }

    #[test]
    fn test_tier1_and_tier2_lists() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 1)); // At crossroads with player
        mgr.add_npc(make_test_npc(2, 2)); // Pub, 1 edge away

        let world = make_test_world(graph, 1);
        mgr.assign_tiers(&world, &[]);

        let tier1 = mgr.tier1_npcs();
        assert!(tier1.contains(&NpcId(1)));

        let tier2 = mgr.tier2_npcs();
        assert!(tier2.contains(&NpcId(2)));
    }

    #[test]
    fn test_schedule_movement() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        // NPC lives at crossroads (1), works at pub (2)
        mgr.add_npc(make_scheduled_npc(1, 1, 2));

        // At 10am, NPC should want to be at work (pub, id 2)
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause(); // freeze time for determinism

        mgr.tick_schedules(&clock, &graph, parish_types::Weather::Clear);

        // NPC should now be in transit to pub
        let npc = mgr.get(NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::InTransit { to, .. } if to == LocationId(2)),
            "NPC should be in transit to pub"
        );
    }

    #[test]
    fn test_schedule_arrival() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_scheduled_npc(1, 1, 2));

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        // Start transit
        mgr.tick_schedules(&clock, &graph, parish_types::Weather::Clear);
        assert!(matches!(
            mgr.get(NpcId(1)).unwrap().state,
            NpcState::InTransit { .. }
        ));

        // Advance time past arrival
        clock.advance(30); // 30 minutes should be enough for any parish path
        mgr.tick_schedules(&clock, &graph, parish_types::Weather::Clear);

        let npc = mgr.get(NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::Present),
            "NPC should have arrived"
        );
        assert_eq!(npc.location, LocationId(2), "NPC should be at pub");
    }

    #[test]
    fn test_needs_tier2_tick() {
        let mgr = NpcManager::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();

        // First time should always need a tick
        assert!(mgr.needs_tier2_tick(now));
    }

    #[test]
    fn test_tier2_tick_interval() {
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();

        mgr.record_tier2_tick(t0);

        // 3 minutes later: not yet
        let t1 = t0 + Duration::minutes(3);
        assert!(!mgr.needs_tier2_tick(t1));

        // 5 minutes later: yes
        let t2 = t0 + Duration::minutes(5);
        assert!(mgr.needs_tier2_tick(t2));

        // 10 minutes later: yes
        let t3 = t0 + Duration::minutes(10);
        assert!(mgr.needs_tier2_tick(t3));
    }

    #[test]
    fn test_tier2_groups() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2)); // pub
        mgr.add_npc(make_test_npc(2, 2)); // pub
        mgr.add_npc(make_test_npc(3, 3)); // church

        // Player at crossroads — pub and church are nearby (Tier 2)
        let world = make_test_world(graph, 1);
        mgr.assign_tiers(&world, &[]);

        let groups = mgr.tier2_groups();
        assert_eq!(groups.get(&LocationId(2)).map(|v| v.len()), Some(2));
    }

    #[test]
    fn test_load_from_file() {
        let path = Path::new("data/npcs.json");
        if !path.exists() {
            return;
        }
        let mgr = NpcManager::load_from_file(path).unwrap();
        assert_eq!(mgr.npc_count(), 23);
    }

    #[test]
    fn test_npc_stays_put_when_at_desired_location() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        // NPC lives at crossroads (1), works at pub (2)
        // Start them already at pub
        let mut npc = make_scheduled_npc(1, 1, 2);
        npc.location = LocationId(2); // already at work
        mgr.add_npc(npc);

        // At 10am, NPC should want to be at pub — already there
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        mgr.tick_schedules(&clock, &graph, parish_types::Weather::Clear);

        // Should stay present, not start transit
        assert!(matches!(
            mgr.get(NpcId(1)).unwrap().state,
            NpcState::Present
        ));
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

        let found = mgr.find_by_name("padraig darcy", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
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

        let found = mgr.find_by_name("Padraig", LocationId(99));
        assert!(found.is_none());
    }

    #[test]
    fn test_find_by_name_no_match() {
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2));
        mgr.mark_introduced(NpcId(1));

        let found = mgr.find_by_name("Nobody", LocationId(2));
        assert!(found.is_none());
    }

    #[test]
    fn test_find_by_name_unintroduced_uses_brief_description() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(1, 2);
        npc.brief_description = "an older man behind the bar".to_string();
        mgr.add_npc(npc);
        // Not introduced — display name is brief_description

        let found = mgr.find_by_name("an older man behind the bar", LocationId(2));
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, NpcId(1));
    }

    #[test]
    fn test_known_roster_unions_home_and_work_matches() {
        // Regression test for the single-pass optimization in `known_roster`.
        // Ensures NPCs sharing either home or workplace are included, dedup'd.
        let mut mgr = NpcManager::new();

        // Subject NPC: home=10, work=20.
        let mut subject = make_test_npc(1, 10);
        subject.home = Some(LocationId(10));
        subject.workplace = Some(LocationId(20));
        mgr.add_npc(subject.clone());

        // NPC 2: shares home only (home=10, work=30).
        let mut home_mate = make_test_npc(2, 30);
        home_mate.home = Some(LocationId(10));
        home_mate.workplace = Some(LocationId(30));
        mgr.add_npc(home_mate);

        // NPC 3: shares workplace only (home=40, work=20).
        let mut work_mate = make_test_npc(3, 20);
        work_mate.home = Some(LocationId(40));
        work_mate.workplace = Some(LocationId(20));
        mgr.add_npc(work_mate);

        // NPC 4: currently located at subject's home (co-presence) but no home/work ties.
        let mut visitor = make_test_npc(4, 10);
        visitor.home = Some(LocationId(50));
        visitor.workplace = None;
        mgr.add_npc(visitor);

        // NPC 5: shares both home and work (should appear once, not twice).
        let mut both = make_test_npc(5, 10);
        both.home = Some(LocationId(10));
        both.workplace = Some(LocationId(20));
        mgr.add_npc(both);

        // NPC 6: unrelated — different home and work.
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
            "NPC located at subject's home should be in roster"
        );
        assert!(
            ids.contains(&NpcId(5)),
            "NPC sharing both home and work should be in roster"
        );
        assert!(
            !ids.contains(&NpcId(6)),
            "unrelated NPC must not be in roster"
        );
        assert!(
            !ids.contains(&NpcId(1)),
            "subject must not be in its own roster"
        );
        // Dedup: each id appears exactly once.
        assert_eq!(ids.len(), roster.len());
    }

    #[test]
    fn test_tier_promotion_inflates_npc() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 11)); // Far location (Tier 3)

        // Initial assignment — NPC at far location
        let world = make_test_world(graph.clone(), 1);
        mgr.assign_tiers(&world, &[]);
        let initial_tier = mgr.tier_of(NpcId(1)).unwrap();
        assert_ne!(initial_tier, CogTier::Tier1);

        // Move NPC to player's location and provide recent events
        mgr.get_mut(NpcId(1)).unwrap().location = LocationId(1);
        let events = vec![GameEvent::MoodChanged {
            npc_id: NpcId(1),
            new_mood: "excited".to_string(),
            timestamp: world.clock.now(),
        }];
        mgr.assign_tiers(&world, &events);

        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier1));
        // Check that a context recap memory was injected
        let npc = mgr.get(NpcId(1)).unwrap();
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

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 1)); // Same location as player (Tier 1)

        // Initial assignment
        let world = make_test_world(graph.clone(), 1);
        mgr.assign_tiers(&world, &[]);
        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier1));

        // Move NPC far away
        mgr.get_mut(NpcId(1)).unwrap().location = LocationId(11);
        mgr.assign_tiers(&world, &[]);

        // Check that deflated_summary was set
        let npc = mgr.get(NpcId(1)).unwrap();
        assert!(npc.deflated_summary.is_some());
        let summary = npc.deflated_summary.as_ref().unwrap();
        assert_eq!(summary.npc_id, NpcId(1));
        assert_eq!(summary.mood, "calm");
    }

    #[test]
    fn test_needs_tier2_tick_with_config_custom_interval() {
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        mgr.record_tier2_tick(t0);

        let config = CognitiveTierConfig {
            tier2_tick_interval_minutes: 10,
            ..CognitiveTierConfig::default()
        };

        // 5 minutes later: not enough (interval is 10)
        let t1 = t0 + Duration::minutes(5);
        assert!(!mgr.needs_tier2_tick_with_config(t1, &config));

        // 10 minutes later: yes
        let t2 = t0 + Duration::minutes(10);
        assert!(mgr.needs_tier2_tick_with_config(t2, &config));
    }

    #[test]
    fn test_needs_tier2_tick_with_config_first_tick() {
        let mgr = NpcManager::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();

        let config = CognitiveTierConfig {
            tier2_tick_interval_minutes: 10,
            ..CognitiveTierConfig::default()
        };

        // First time should always need a tick regardless of config
        assert!(mgr.needs_tier2_tick_with_config(now, &config));
    }

    #[test]
    fn test_npc_rain_override() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return, // skip if no test data
        };

        // NPC at home (Darcy's Pub, id=2, indoor), scheduled to work at Crossroads (id=1, outdoor)
        let mut npc = make_scheduled_npc(1, 2, 1);
        npc.home = Some(LocationId(2));
        npc.occupation = "Shopkeeper".to_string();

        let mut mgr = NpcManager::new();
        mgr.add_npc(npc);

        // At hour 10, schedule says go to Crossroads (work=1, outdoor)
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        // With HeavyRain, NPC should stay at home (indoor) instead of going outdoor
        mgr.tick_schedules(&clock, &graph, parish_types::Weather::HeavyRain);

        let npc = mgr.get(NpcId(1)).unwrap();
        // NPC should remain present (not start transit to outdoor location)
        assert!(
            matches!(npc.state, NpcState::Present),
            "NPC should stay put in heavy rain instead of going to outdoor location"
        );
        assert_eq!(
            npc.location,
            LocationId(2),
            "NPC should remain at indoor home"
        );
    }

    #[test]
    fn test_farmer_tolerates_light_rain() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        // Farmer NPC at home (Darcy's Pub, id=2, indoor), scheduled to work at Murphy's Farm (id=9, outdoor)
        let mut npc = make_scheduled_npc(1, 2, 9);
        npc.home = Some(LocationId(2));
        npc.occupation = "Farmer".to_string();

        let mut mgr = NpcManager::new();
        mgr.add_npc(npc);

        // At hour 10, schedule says go to Murphy's Farm (work=9, outdoor)
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        // With LightRain, farmer should still go to work (tolerate light rain)
        let _events = mgr.tick_schedules(&clock, &graph, parish_types::Weather::LightRain);

        let npc = mgr.get(NpcId(1)).unwrap();
        // Farmer should be in transit to the farm
        assert!(
            matches!(npc.state, NpcState::InTransit { .. }),
            "Farmer should tolerate light rain and head to outdoor work, got {:?}",
            npc.state
        );
    }

    // --- Tier 3 / Tier 4 assignment tests ---

    /// Builds a linear chain graph: 0 - 1 - 2 - ... - n for testing.
    fn make_chain_graph(n: u32) -> WorldGraph {
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
    fn test_tier_assignment_3_vs_4() {
        let graph = make_chain_graph(6);

        let mut mgr = NpcManager::new();
        // Place NPCs at various distances from player (at location 0)
        for i in 0..=6 {
            mgr.add_npc(make_test_npc(i + 10, i));
        }

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        mgr.assign_tiers(&world, &[]);

        // Distance 0 → Tier 1
        assert_eq!(mgr.tier_of(NpcId(10)), Some(CogTier::Tier1));
        // Distance 1 → Tier 2
        assert_eq!(mgr.tier_of(NpcId(11)), Some(CogTier::Tier2));
        // Distance 2 → Tier 2
        assert_eq!(mgr.tier_of(NpcId(12)), Some(CogTier::Tier2));
        // Distance 3 → Tier 3
        assert_eq!(mgr.tier_of(NpcId(13)), Some(CogTier::Tier3));
        // Distance 4 → Tier 3
        assert_eq!(mgr.tier_of(NpcId(14)), Some(CogTier::Tier3));
        // Distance 5 → Tier 3
        assert_eq!(mgr.tier_of(NpcId(15)), Some(CogTier::Tier3));
        // Distance 6 → Tier 4
        assert_eq!(mgr.tier_of(NpcId(16)), Some(CogTier::Tier4));
    }

    #[test]
    fn test_tier3_npcs() {
        let graph = make_chain_graph(5);

        let mut mgr = NpcManager::new();
        // NPC at distance 3 = Tier 3, NPC at distance 4 = Tier 3
        mgr.add_npc(make_test_npc(1, 3));
        mgr.add_npc(make_test_npc(2, 4));
        // NPC at distance 1 = Tier 2
        mgr.add_npc(make_test_npc(3, 1));

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        mgr.assign_tiers(&world, &[]);

        let tier3 = mgr.tier3_npcs();
        assert_eq!(tier3.len(), 2);
        assert!(tier3.contains(&NpcId(1)));
        assert!(tier3.contains(&NpcId(2)));
    }

    #[test]
    fn test_tier3_tick_interval() {
        let config = CognitiveTierConfig::default();
        let mgr = NpcManager::new();

        // Never ticked → needs tick
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier3_tick_with_config(now, &config));
    }

    #[test]
    fn test_tier3_tick_not_yet_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier3_tick(t0);

        // 12 hours later → not yet (need 24)
        let t1 = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(!mgr.needs_tier3_tick_with_config(t1, &config));
    }

    #[test]
    fn test_tier3_tick_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier3_tick(t0);

        // 24 hours later → due
        let t1 = Utc.with_ymd_and_hms(1820, 3, 21, 0, 0, 0).unwrap();
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

    /// Tier 4 tick interval: never-ticked manager always returns `needs_tier4_tick = true`.
    #[test]
    fn test_tier4_tick_never_ticked() {
        let mgr = NpcManager::new();
        let now = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick(now));
        assert!(mgr.last_tier4_game_time().is_none());
    }

    /// After recording a tick, the manager should NOT need another tick until the
    /// interval has elapsed.
    #[test]
    fn test_tier4_tick_not_yet_due() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 0, 0, 0).unwrap();
        mgr.record_tier4_tick(t0);

        // 30 days later → not yet due (interval = 90 days)
        let t1 = Utc.with_ymd_and_hms(1820, 4, 19, 0, 0, 0).unwrap();
        assert!(!mgr.needs_tier4_tick_with_config(t1, &config));
        assert_eq!(mgr.last_tier4_game_time(), Some(t0));
    }

    /// After the interval elapses, `needs_tier4_tick` returns true again.
    #[test]
    fn test_tier4_tick_due_after_interval() {
        let config = CognitiveTierConfig::default();
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 1, 1, 0, 0, 0).unwrap();
        mgr.record_tier4_tick(t0);

        // Exactly 90 days later → due
        let t1 = Utc.with_ymd_and_hms(1820, 4, 1, 0, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick_with_config(t1, &config));
    }

    /// Full wiring cycle: build tier4_refs, call tick_tier4, apply events,
    /// record tick. Asserts `last_tier4_game_time` is set.
    /// This is the same pattern used by parish-tauri and parish-server.
    #[test]
    fn test_tier4_dispatch_wiring_cycle() {
        use crate::tier4::tick_tier4;
        use parish_world::WorldState;

        let graph = make_chain_graph(6);
        let mut mgr = NpcManager::new();
        // NPC at distance 6 → Tier 4
        mgr.add_npc(make_test_npc(99, 6));

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(99)), Some(CogTier::Tier4));

        let now = Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();
        assert!(mgr.needs_tier4_tick(now));

        // Replicate the wiring pattern from the entry-point tick loops
        let tier4_ids: HashSet<NpcId> = mgr.tier4_npcs().into_iter().collect();
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
        assert!(!mgr.needs_tier4_tick(now)); // not due again immediately
    }

    /// Verifies the Tier 3 dispatch gating and state-management cycle used by
    /// all three entry points (parish-tauri, parish-server, parish-cli/headless).
    ///
    /// Checks:
    ///   1. A fresh manager signals `needs_tier3_tick`.
    ///   2. Setting `tier3_in_flight = true` does NOT clear `needs_tier3_tick`
    ///      (the entry point checks both; here we test each independently).
    ///   3. `record_tier3_tick` sets `last_tier3_game_time` and makes
    ///      `needs_tier3_tick` return false immediately after.
    ///   4. After clearing `tier3_in_flight`, the flag is false.
    #[test]
    fn test_tier3_dispatch_wiring_cycle() {
        use crate::ticks::tier3_snapshot_from_npc;
        use parish_world::WorldState;

        let graph = make_chain_graph(6);
        let mut mgr = NpcManager::new();
        // NPC at distance 4 → Tier 3 (between tier2_max and tier3_max)
        mgr.add_npc(make_test_npc(10, 4));

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(10)), Some(CogTier::Tier3));

        let now = Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

        // 1. Fresh manager → needs a tick
        assert!(mgr.needs_tier3_tick(now));

        // 2. Simulate the dispatch gating: in-flight flag blocks a second launch
        //    even though needs_tier3_tick is still true.
        assert!(!mgr.tier3_in_flight());
        let should_dispatch = mgr.needs_tier3_tick(now) && !mgr.tier3_in_flight();
        assert!(should_dispatch);

        mgr.set_tier3_in_flight(true);

        // While in-flight, a second pass must not dispatch again.
        let would_double_dispatch = mgr.needs_tier3_tick(now) && !mgr.tier3_in_flight();
        assert!(
            !would_double_dispatch,
            "in-flight guard must block double dispatch"
        );

        // 3. Build snapshots — same pattern as the entry-point loops.
        let tier3_ids = mgr.tier3_npcs();
        assert!(!tier3_ids.is_empty());
        let snapshots: Vec<_> = tier3_ids
            .iter()
            .filter_map(|id| mgr.get(*id))
            .map(|npc| tier3_snapshot_from_npc(npc, &world.graph))
            .collect();
        assert!(!snapshots.is_empty());

        // Simulate the async call completing: apply the tick + clear the flag.
        mgr.record_tier3_tick(now);
        mgr.set_tier3_in_flight(false);

        // 4. Post-tick: time recorded, flag clear, no immediate re-dispatch.
        assert_eq!(mgr.last_tier3_game_time(), Some(now));
        assert!(!mgr.tier3_in_flight());
        assert!(!mgr.needs_tier3_tick(now));
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

    /// Verifies the Tier 2 dispatch gating and state-management cycle used by
    /// all three entry points (parish-tauri, parish-server, parish-cli/headless).
    ///
    /// Checks:
    ///   1. A fresh manager signals `needs_tier2_tick`.
    ///   2. Setting `tier2_in_flight = true` does NOT clear `needs_tier2_tick`
    ///      (the entry point checks both; here we test each independently).
    ///   3. `record_tier2_tick` sets `last_tier2_game_time` and makes
    ///      `needs_tier2_tick` return false immediately after.
    ///   4. After clearing `tier2_in_flight`, the flag is false.
    #[test]
    fn test_tier2_dispatch_wiring_cycle() {
        use parish_world::WorldState;

        let graph = make_chain_graph(4);
        let mut mgr = NpcManager::new();
        // NPC at distance 2 → Tier 2 (within tier2_max distance)
        mgr.add_npc(make_test_npc(20, 2));

        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;
        mgr.assign_tiers(&world, &[]);

        assert_eq!(mgr.tier_of(NpcId(20)), Some(CogTier::Tier2));

        let now = Utc.with_ymd_and_hms(1820, 6, 1, 12, 0, 0).unwrap();

        // 1. Fresh manager → needs a tick
        assert!(mgr.needs_tier2_tick(now));

        // 2. Simulate the dispatch gating: in-flight flag blocks a second launch
        //    even though needs_tier2_tick is still true.
        assert!(!mgr.tier2_in_flight());
        let should_dispatch = mgr.needs_tier2_tick(now) && !mgr.tier2_in_flight();
        assert!(should_dispatch);

        mgr.set_tier2_in_flight(true);

        // While in-flight, a second pass must not dispatch again.
        let would_double_dispatch = mgr.needs_tier2_tick(now) && !mgr.tier2_in_flight();
        assert!(
            !would_double_dispatch,
            "in-flight guard must block double dispatch"
        );

        // 3. Build groups — same pattern as the entry-point loops.
        let groups_map = mgr.tier2_groups();
        assert!(!groups_map.is_empty());

        // Simulate the async call completing: apply the tick + clear the flag.
        mgr.record_tier2_tick(now);
        mgr.set_tier2_in_flight(false);

        // 4. Post-tick: time recorded, flag clear, no immediate re-dispatch.
        assert_eq!(mgr.last_tier2_game_time(), Some(now));
        assert!(!mgr.tier2_in_flight());
        assert!(!mgr.needs_tier2_tick(now));
    }

    // ── Banshee integration tests ────────────────────────────────────────────

    fn make_mourning_world() -> parish_world::WorldState {
        use chrono::TimeZone;
        let mut world = parish_world::WorldState::new();
        world.graph = make_chain_graph(4);
        world.player_location = LocationId(0);
        // Seed the clock at 22:00 — squarely inside the herald window.
        world.clock = parish_world::time::GameClock::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 22, 0, 0).unwrap(),
        );
        world
    }

    #[test]
    fn banshee_herald_fires_at_night_with_near_doom() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap()); // 8 hours ahead
        mgr.add_npc(npc);

        let mut world = make_mourning_world();

        let report = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );

        assert_eq!(report.wails.len(), 1, "one wail expected");
        assert_eq!(report.deaths.len(), 0, "no death yet");
        assert!(
            world
                .text_log
                .iter()
                .any(|l| l.contains("keening") || l.contains("banshee")),
            "wail line should appear in text log"
        );
        assert!(
            mgr.get(NpcId(42)).expect("still alive").banshee_heralded,
            "herald flag must be set"
        );
    }

    #[test]
    fn banshee_wail_is_emitted_only_once_per_doom() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        mgr.add_npc(npc);

        let mut world = make_mourning_world();

        let r1 = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );
        let r2 = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );
        assert_eq!(r1.wails.len(), 1);
        assert_eq!(r2.wails.len(), 0, "second tick must not re-wail");
    }

    #[test]
    fn banshee_finalises_death_once_doom_passes() {
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(42, 2);
        // Doom is 1 hour in the past — should be finalised immediately.
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 15, 21, 0, 0).unwrap());
        npc.banshee_heralded = true; // already heralded earlier
        mgr.add_npc(npc);

        let mut world = make_mourning_world();

        let report = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );
        assert_eq!(report.deaths.len(), 1);
        assert_eq!(report.wails.len(), 0);
        assert!(
            mgr.get(NpcId(42)).is_none(),
            "NPC must be removed once doom passes"
        );
        assert!(
            world
                .text_log
                .iter()
                .any(|l| l.contains("did not see the morning")),
            "epitaph line should appear in text log"
        );
    }

    #[test]
    fn banshee_does_not_fire_during_daytime() {
        use chrono::TimeZone;
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        mgr.add_npc(npc);

        // Clock at 14:00 — outside night window.
        let mut world = make_mourning_world();
        world.clock = parish_world::time::GameClock::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 14, 0, 0).unwrap(),
        );

        let report = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );
        assert!(
            report.is_empty(),
            "daytime should produce neither wail nor death"
        );
        assert!(world.text_log.is_empty());
    }

    #[test]
    fn tier4_death_now_schedules_doom_rather_than_removing() {
        use crate::tier4::Tier4Event;
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(42, 2));

        let now = Utc.with_ymd_and_hms(1820, 6, 15, 14, 0, 0).unwrap();
        let events = vec![Tier4Event::Death { npc_id: NpcId(42) }];
        let game_events = mgr.apply_tier4_events(&events, now, true);

        assert!(
            mgr.get(NpcId(42)).is_some(),
            "NPC should NOT be removed yet"
        );
        let doom = mgr.get(NpcId(42)).unwrap().doom.expect("doom must be set");
        assert!(doom > now, "doom must be in the future");
        assert_eq!(
            doom - now,
            chrono::Duration::hours(crate::banshee::DOOM_LEAD_TIME_HOURS)
        );
        assert!(!game_events.is_empty(), "should still emit a life event");
    }

    #[test]
    fn tier4_death_with_banshee_disabled_removes_npc_immediately() {
        use crate::tier4::Tier4Event;
        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(42, 2));

        let now = Utc.with_ymd_and_hms(1820, 6, 15, 14, 0, 0).unwrap();
        let events = vec![Tier4Event::Death { npc_id: NpcId(42) }];
        let game_events = mgr.apply_tier4_events(&events, now, false);

        assert!(
            mgr.get(NpcId(42)).is_none(),
            "NPC should be removed immediately when banshee is disabled"
        );
        assert!(!game_events.is_empty(), "should still emit a life event");
    }

    // ── #339 dead-NPC reference cleanup ─────────────────────────────────────

    #[test]
    fn remove_npc_scrubs_all_references() {
        use crate::types::Relationship;

        let mut mgr = NpcManager::new();
        // Three NPCs so we can verify scrubbing across the surviving roster.
        for id in [10, 20, 30] {
            mgr.add_npc(make_test_npc(id, 0));
        }
        // Tier assignments + introductions + name knowledge for the doomed NPC.
        mgr.tier_assignments.insert(NpcId(20), CogTier::Tier1);
        mgr.introduced_npcs.insert(NpcId(20));
        mgr.npcs_who_know_player_name.insert(NpcId(20));

        // Give the survivors relationships pointing at the doomed NPC.
        mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
            NpcId(20),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );
        mgr.npcs.get_mut(&NpcId(30)).unwrap().relationships.insert(
            NpcId(20),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );
        // Plus an unrelated relationship that must survive.
        mgr.npcs.get_mut(&NpcId(10)).unwrap().relationships.insert(
            NpcId(30),
            Relationship::new(crate::types::RelationshipKind::Neighbor, 0.0),
        );

        let removed = mgr.remove_npc(NpcId(20));
        assert!(removed.is_some(), "remove_npc should return the dead NPC");

        // npcs map: gone
        assert!(mgr.get(NpcId(20)).is_none());
        // tier_assignments: gone
        assert!(!mgr.tier_assignments.contains_key(&NpcId(20)));
        // introductions: gone
        assert!(!mgr.introduced_npcs.contains(&NpcId(20)));
        // name knowledge: gone
        assert!(!mgr.npcs_who_know_player_name.contains(&NpcId(20)));
        // Survivors lost their dead-NPC relationships but kept the unrelated ones.
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

    #[test]
    fn banshee_herald_near_player_uses_close_voicing() {
        use crate::banshee::BansheeEvent;
        let mut mgr = NpcManager::new();
        let mut npc = make_test_npc(42, 0); // NPC lives at player's location
        npc.home = Some(LocationId(0));
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        mgr.add_npc(npc);

        let mut world = make_mourning_world();

        let report = mgr.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        );
        assert_eq!(report.wails.len(), 1);
        if let BansheeEvent::Heard { near_player, .. } = &report.wails[0] {
            assert!(*near_player, "player shares location with the doomed NPC");
        } else {
            panic!("expected a Heard event");
        }
    }

    // ── BFS distance cache tests ──────────────────────────────────────────────

    /// Calling `assign_tiers` twice from the same player location must produce
    /// identical tier assignments regardless of whether the second call hits the
    /// cache.
    #[test]
    fn bfs_cache_same_distances_on_cache_hit() {
        let graph = make_chain_graph(4);
        let mut mgr = NpcManager::new();
        for i in 0..=4 {
            mgr.add_npc(make_test_npc(i + 10, i));
        }
        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        // First call — cold cache
        mgr.assign_tiers(&world, &[]);
        let tiers_first: Vec<_> = (0..=4u32)
            .map(|i| mgr.tier_of(NpcId(i + 10)).unwrap())
            .collect();

        // Second call — warm cache, same player location
        mgr.assign_tiers(&world, &[]);
        let tiers_second: Vec<_> = (0..=4u32)
            .map(|i| mgr.tier_of(NpcId(i + 10)).unwrap())
            .collect();

        assert_eq!(
            tiers_first, tiers_second,
            "cached BFS must produce the same tier assignments as the cold computation"
        );
    }

    /// After `invalidate_bfs_cache` the next `assign_tiers` call must
    /// recompute distances from scratch, but the result must still match
    /// the original (topology unchanged).
    #[test]
    fn bfs_cache_invalidation_preserves_correctness() {
        let graph = make_chain_graph(4);
        let mut mgr = NpcManager::new();
        for i in 0..=4 {
            mgr.add_npc(make_test_npc(i + 10, i));
        }
        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        // Warm the cache
        mgr.assign_tiers(&world, &[]);
        let tiers_before: Vec<_> = (0..=4u32)
            .map(|i| mgr.tier_of(NpcId(i + 10)).unwrap())
            .collect();

        // Invalidate then recompute with the same graph
        mgr.invalidate_bfs_cache();
        mgr.assign_tiers(&world, &[]);
        let tiers_after: Vec<_> = (0..=4u32)
            .map(|i| mgr.tier_of(NpcId(i + 10)).unwrap())
            .collect();

        assert_eq!(
            tiers_before, tiers_after,
            "post-invalidation recomputation must agree with the pre-invalidation result"
        );
    }

    /// Moving the player to a different location must cause a cache miss and
    /// produce distance-correct tier assignments for the new player position.
    #[test]
    fn bfs_cache_invalidated_on_player_move() {
        // Chain: 0 — 1 — 2 — 3 — 4 — 5 — 6
        // Default tier boundaries: Tier1 ≤ 0, Tier2 ≤ 2, Tier3 ≤ 5, Tier4 > 5.
        // Distance 6 exceeds tier3_max_distance (5), so the far-end NPC is Tier4.
        let graph = make_chain_graph(6);
        let mut mgr = NpcManager::new();
        // Only need NPCs at the two endpoints: loc 0 (NpcId 10) and loc 6 (NpcId 16).
        mgr.add_npc(make_test_npc(10, 0));
        mgr.add_npc(make_test_npc(16, 6));
        let mut world = WorldState::new();
        world.player_location = LocationId(0);
        world.graph = graph;

        mgr.assign_tiers(&world, &[]);
        // With player at 0: NPC at loc 0 is distance 0 → Tier 1;
        //                   NPC at loc 6 is distance 6 > tier3_max (5) → Tier 4.
        assert_eq!(mgr.tier_of(NpcId(10)), Some(CogTier::Tier1));
        assert_eq!(mgr.tier_of(NpcId(16)), Some(CogTier::Tier4));

        // Move player to the far end — cache must be invalidated automatically.
        world.player_location = LocationId(6);
        mgr.assign_tiers(&world, &[]);

        // Now NPC at loc 6 should be Tier 1, NPC at loc 0 should be Tier 4.
        assert_eq!(
            mgr.tier_of(NpcId(16)),
            Some(CogTier::Tier1),
            "NPC at player's new location must be promoted to Tier 1"
        );
        assert_eq!(
            mgr.tier_of(NpcId(10)),
            Some(CogTier::Tier4),
            "NPC 6 hops from player must be demoted to Tier 4"
        );
    }
}
