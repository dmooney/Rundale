//! Tier queries, in-flight tick state, and tick dispatch.
//!
//! Part of the `NpcManager` impl, split out of the former monolithic
//! `manager.rs` (#1200 TD-030). Public method paths are unchanged.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Utc};

use crate::NpcId;
use crate::types::{CogTier, NpcState};
use parish_config::CognitiveTierConfig;
use parish_types::{LocationId, Weather};
use parish_world::WorldState;
use parish_world::events::{EventBus, GameEvent};
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

use super::NpcManager;
use crate::schedule::ScheduleEvent;
use crate::tier_assign::TierTransition;

impl NpcManager {
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

    /// Groups co-located Tier 2 NPCs by location, returning only locations
    /// with two or more members.
    ///
    /// Tier 2 models *group* dynamics, so a location holding a single
    /// Tier 2 NPC is excluded from Tier 2 dispatch: running Tier 2 on a
    /// solo NPC only produces repetitive filler and wastes an LLM
    /// round-trip (#1025). The NPC keeps its distance-based `Tier2`
    /// assignment (it is not reassigned to Tier 3/4) and is picked up by
    /// Tier 2 again as soon as it shares a location with another Tier 2
    /// NPC, or by Tier 3/4 once the player moves and its distance grows.
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
        groups.retain(|_, ids| ids.len() >= 2);
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
