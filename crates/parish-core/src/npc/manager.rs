//! Central NPC coordinator.
//!
//! Manages all NPCs in the world, assigns cognitive tiers based on
//! proximity to the player, advances NPC schedules, and provides
//! queries for NPCs at specific locations.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use chrono::{DateTime, Duration, Timelike, Utc};

use crate::config::CognitiveTierConfig;
use crate::error::ParishError;
use crate::npc::data::load_npcs_from_file;
use crate::npc::types::{CogTier, NpcState};
use crate::npc::{Npc, NpcId};
use crate::world::LocationId;
use crate::world::graph::WorldGraph;
use crate::world::time::GameClock;

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
    /// Set of NPC ids that have introduced themselves to the player.
    introduced_npcs: HashSet<NpcId>,
}

impl NpcManager {
    /// Creates an empty NpcManager.
    pub fn new() -> Self {
        Self {
            npcs: HashMap::new(),
            tier_assignments: HashMap::new(),
            last_tier2_game_time: None,
            introduced_npcs: HashSet::new(),
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

        // Exact match on display name
        if let Some(npc) = npcs
            .iter()
            .find(|n| self.display_name(n).to_lowercase() == lower)
        {
            return Some(npc);
        }

        // First-name prefix match
        npcs.iter()
            .find(|n| {
                let display = self.display_name(n).to_lowercase();
                display
                    .split_whitespace()
                    .next()
                    .is_some_and(|first| first == lower)
            })
            .copied()
    }

    /// Returns an iterator over all NPCs.
    pub fn all_npcs(&self) -> impl Iterator<Item = &Npc> {
        self.npcs.values()
    }

    /// Returns the number of NPCs managed.
    pub fn npc_count(&self) -> usize {
        self.npcs.len()
    }

    /// Assigns cognitive tiers to all NPCs based on BFS distance from the player,
    /// using the given cognitive tier config for distance thresholds.
    pub fn assign_tiers_with_config(
        &mut self,
        player_location: LocationId,
        graph: &WorldGraph,
        config: &CognitiveTierConfig,
    ) {
        // BFS from player location to compute distances
        let distances = bfs_distances(player_location, graph);

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

            let tier = match distance {
                Some(d) if d <= config.tier1_max_distance => CogTier::Tier1,
                Some(d) if d <= config.tier2_max_distance => CogTier::Tier2,
                _ => CogTier::Tier3,
            };

            self.tier_assignments.insert(npc.id, tier);
        }

        tracing::debug!(
            player_location = player_location.0,
            tier1 = self.tier1_npcs().len(),
            tier2 = self.tier2_npcs().len(),
            "Tier assignment complete"
        );
    }

    /// Assigns cognitive tiers to all NPCs based on BFS distance from the player.
    ///
    /// - Distance 0 (same location): Tier 1
    /// - Distance 1-2: Tier 2
    /// - Distance 3+: Tier 3
    pub fn assign_tiers(&mut self, player_location: LocationId, graph: &WorldGraph) {
        self.assign_tiers_with_config(player_location, graph, &CognitiveTierConfig::default());
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
    pub fn tick_schedules(&mut self, clock: &GameClock, graph: &WorldGraph) -> Vec<ScheduleEvent> {
        let now = clock.now();
        let current_hour = now.hour() as u8;
        let mut events = Vec::new();

        let npc_ids: Vec<NpcId> = self.npcs.keys().copied().collect();

        for id in npc_ids {
            let npc = self.npcs.get(&id).unwrap();

            match &npc.state {
                NpcState::Present => {
                    if let Some(desired) = npc.desired_location(current_hour)
                        && desired != npc.location
                        && let Some(path) = graph.shortest_path(npc.location, desired)
                    {
                        let travel_minutes = graph.path_travel_time(&path);
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
                        let npc = self.npcs.get_mut(&id).unwrap();
                        npc.state = NpcState::InTransit {
                            from,
                            to: desired,
                            arrives_at,
                        };
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
                        let npc = self.npcs.get_mut(&id).unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::memory::ShortTermMemory;
    use crate::npc::types::{DailySchedule, ScheduleEntry};
    use chrono::TimeZone;

    fn make_test_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test personality".to_string(),
            intelligence: crate::npc::types::Intelligence::default(),
            location: LocationId(location),
            mood: "calm".to_string(),
            home: Some(LocationId(location)),
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
        }
    }

    fn make_scheduled_npc(id: u32, home: u32, work: u32) -> Npc {
        let mut npc = make_test_npc(id, home);
        npc.schedule = Some(DailySchedule {
            entries: vec![
                ScheduleEntry {
                    start_hour: 0,
                    end_hour: 7,
                    location: LocationId(home),
                    activity: "sleeping".to_string(),
                },
                ScheduleEntry {
                    start_hour: 8,
                    end_hour: 17,
                    location: LocationId(work),
                    activity: "working".to_string(),
                },
                ScheduleEntry {
                    start_hour: 18,
                    end_hour: 23,
                    location: LocationId(home),
                    activity: "evening rest".to_string(),
                },
            ],
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
        mgr.assign_tiers(LocationId(1), &graph);

        assert_eq!(mgr.tier_of(NpcId(2)), Some(CogTier::Tier1)); // same location
        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier2)); // 1 edge
        // Fairy fort distance depends on graph topology
        let fairy_tier = mgr.tier_of(NpcId(3)).unwrap();
        assert!(
            fairy_tier == CogTier::Tier2 || fairy_tier == CogTier::Tier3,
            "fairy fort should be Tier2 or Tier3 based on distance"
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

        mgr.assign_tiers(LocationId(1), &graph);

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

        mgr.tick_schedules(&clock, &graph);

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
        mgr.tick_schedules(&clock, &graph);
        assert!(matches!(
            mgr.get(NpcId(1)).unwrap().state,
            NpcState::InTransit { .. }
        ));

        // Advance time past arrival
        clock.advance(30); // 30 minutes should be enough for any parish path
        mgr.tick_schedules(&clock, &graph);

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
        mgr.assign_tiers(LocationId(1), &graph);

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
        assert_eq!(mgr.npc_count(), 8);
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

        mgr.tick_schedules(&clock, &graph);

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
    fn test_assign_tiers_with_config_custom_distances() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2)); // Pub (1 edge from crossroads)
        mgr.add_npc(make_test_npc(2, 1)); // Crossroads (player is here)

        // With tier1_max_distance = 1, NPC at pub (distance 1) should be Tier 1
        let config = CognitiveTierConfig {
            tier1_max_distance: 1,
            tier2_max_distance: 3,
            tier2_tick_interval_minutes: 5,
        };
        mgr.assign_tiers_with_config(LocationId(1), &graph, &config);

        assert_eq!(mgr.tier_of(NpcId(2)), Some(CogTier::Tier1)); // distance 0
        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier1)); // distance 1, within tier1_max
    }

    #[test]
    fn test_assign_tiers_with_config_narrow_tiers() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut mgr = NpcManager::new();
        mgr.add_npc(make_test_npc(1, 2)); // Pub (1 edge from crossroads)
        mgr.add_npc(make_test_npc(2, 1)); // Crossroads (player is here)

        // With tier2_max_distance = 0, only same location is Tier 1, everything else Tier 3
        let config = CognitiveTierConfig {
            tier1_max_distance: 0,
            tier2_max_distance: 0,
            tier2_tick_interval_minutes: 5,
        };
        mgr.assign_tiers_with_config(LocationId(1), &graph, &config);

        assert_eq!(mgr.tier_of(NpcId(2)), Some(CogTier::Tier1)); // distance 0
        assert_eq!(mgr.tier_of(NpcId(1)), Some(CogTier::Tier3)); // distance 1, beyond tier2_max
    }

    #[test]
    fn test_needs_tier2_tick_with_config_custom_interval() {
        let mut mgr = NpcManager::new();
        let t0 = Utc.with_ymd_and_hms(1820, 3, 20, 12, 0, 0).unwrap();
        mgr.record_tier2_tick(t0);

        let config = CognitiveTierConfig {
            tier1_max_distance: 0,
            tier2_max_distance: 2,
            tier2_tick_interval_minutes: 10,
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
            tier1_max_distance: 0,
            tier2_max_distance: 2,
            tier2_tick_interval_minutes: 10,
        };

        // First time should always need a tick regardless of config
        assert!(mgr.needs_tier2_tick_with_config(now, &config));
    }
}
