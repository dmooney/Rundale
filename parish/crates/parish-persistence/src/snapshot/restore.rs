//! Restore and capture helpers for [`GameSnapshot`].
//!
//! [`GameSnapshot::capture`] snapshots live game state into a serializable
//! value. [`GameSnapshot::restore`] rehydrates that value back into mutable
//! live state. The private helpers isolate the clock, world, and NPC
//! sub-restore steps.

use super::types::{ClockSnapshot, GameSnapshot, NpcSnapshot};

impl GameSnapshot {
    /// Captures a snapshot from live game state.
    pub fn capture(
        world: &parish_world::WorldState,
        npc_manager: &parish_npc::manager::NpcManager,
    ) -> Self {
        let clock = ClockSnapshot {
            game_time: world.clock.now(),
            speed_factor: world.clock.speed_factor(),
            paused: world.clock.is_paused(),
        };

        let npcs: Vec<NpcSnapshot> = npc_manager.all_npcs().map(NpcSnapshot::from_npc).collect();

        Self {
            player_location: world.player_location,
            weather: world.weather.to_string(),
            text_log: world.text_log.clone(),
            clock,
            npcs,
            last_tier2_game_time: npc_manager.last_tier2_game_time(),
            last_tier3_game_time: npc_manager.last_tier3_game_time(),
            last_tier4_game_time: npc_manager.last_tier4_game_time(),
            introduced_npcs: npc_manager.introduced_set(),
            visited_locations: world.visited_locations.clone(),
            visited_order: world.visited_order.clone(),
            edge_traversals: world.edge_traversals.clone(),
            gossip_network: world.gossip_network.clone(),
            conversation_log: world.conversation_log.clone(),
            player_name: world.player_name.clone(),
            player_progress: world.player_progress.clone(),
            npcs_who_know_player_name: npc_manager.player_name_known_set(),
            active_session: world.active_session.clone(),
        }
    }

    /// Rehydrates a [`GameClock`](parish_types::GameClock) from a
    /// [`ClockSnapshot`], matching preset speeds or falling back to a custom
    /// factor.
    fn restore_clock(snapshot: &ClockSnapshot) -> parish_types::GameClock {
        use parish_types::{GameClock, GameSpeed};
        let speed = GameSpeed::ALL
            .iter()
            .copied()
            .find(|s| (s.factor() - snapshot.speed_factor).abs() < 0.01);
        let mut clock = match speed {
            Some(s) => {
                let mut c = GameClock::new(snapshot.game_time);
                c.set_speed(s);
                c
            }
            None => GameClock::with_speed(snapshot.game_time, snapshot.speed_factor),
        };
        if snapshot.paused {
            clock.pause();
        }
        clock
    }

    /// Back-fills the legacy `locations` map from the graph and inserts a
    /// placeholder guard for the player's location.
    fn restore_world_locations(
        world: &mut parish_world::WorldState,
        player_location: parish_types::LocationId,
        visited_locations: std::collections::HashSet<parish_types::LocationId>,
        visited_order: Vec<parish_types::LocationId>,
        edge_traversals: std::collections::HashMap<
            (parish_types::LocationId, parish_types::LocationId),
            u32,
        >,
    ) {
        use parish_types::Location;
        for loc_id in world.graph.location_ids() {
            if let Some(data) = world.graph.get(loc_id) {
                world.locations.entry(loc_id).or_insert_with(|| Location {
                    id: loc_id,
                    name: data.name.clone(),
                    description: data.description_template.clone(),
                    indoor: data.indoor,
                    public: data.public,
                    lat: data.lat,
                    lon: data.lon,
                });
            }
        }
        world
            .locations
            .entry(player_location)
            .or_insert_with(|| Location {
                id: player_location,
                name: "Unknown location".to_string(),
                description: "The surroundings are hazy and unfamiliar.".to_string(),
                indoor: false,
                public: false,
                lat: 0.0,
                lon: 0.0,
            });
        world.visited_locations = visited_locations;
        // Restore first-visit order from the snapshot, retaining only ids
        // also present in the set. Legacy saves carry an empty
        // `visited_order` even when `visited_locations` is populated; in
        // that case backfill from the set (sorted by id for determinism)
        // so that a subsequent `mark_visited(new_id)` doesn't shrink the
        // renderer's output to only the freshly-visited locations
        // (#1130 / codex review).
        let mut restored: Vec<parish_types::LocationId> = visited_order
            .into_iter()
            .filter(|id| world.visited_locations.contains(id))
            .collect();
        if restored.is_empty() && !world.visited_locations.is_empty() {
            restored = world.visited_locations.iter().copied().collect();
            restored.sort_by_key(|id| id.0);
        } else {
            // Append any visited-set entries the order vector missed
            // (mismatched save). Sorted by id for determinism.
            let mut missing: Vec<parish_types::LocationId> = world
                .visited_locations
                .iter()
                .copied()
                .filter(|id| !restored.contains(id))
                .collect();
            missing.sort_by_key(|id| id.0);
            restored.extend(missing);
        }
        world.visited_order = restored;
        // The player's current location must always be marked visited.
        // `mark_visited` updates both fields, preserving the no-op
        // contract when already present.
        world.mark_visited(player_location);
        world.edge_traversals = edge_traversals;
    }

    /// Rebuilds the NPC manager from a snapshot.
    fn restore_npcs(
        npc_manager: &mut parish_npc::manager::NpcManager,
        npcs: Vec<NpcSnapshot>,
        last_tier2_game_time: Option<chrono::DateTime<chrono::Utc>>,
        last_tier3_game_time: Option<chrono::DateTime<chrono::Utc>>,
        last_tier4_game_time: Option<chrono::DateTime<chrono::Utc>>,
        introduced_npcs: std::collections::HashSet<parish_types::NpcId>,
        npcs_who_know_player_name: std::collections::HashSet<parish_types::NpcId>,
    ) {
        *npc_manager = parish_npc::manager::NpcManager::new();
        for npc_snap in npcs {
            npc_manager.add_npc(npc_snap.into_npc());
        }
        if let Some(t) = last_tier2_game_time {
            npc_manager.record_tier2_tick(t);
        }
        if let Some(t) = last_tier3_game_time {
            npc_manager.record_tier3_tick(t);
        }
        if let Some(t) = last_tier4_game_time {
            npc_manager.record_tier4_tick(t);
        }
        npc_manager.restore_introduced_set(introduced_npcs);
        npc_manager.restore_player_name_known(npcs_who_know_player_name);
    }

    /// Restores this snapshot into live game state.
    ///
    /// Replaces the dynamic fields of `world` and rebuilds the `npc_manager`
    /// from the snapshot. The world graph is left untouched (it's static data
    /// loaded from files), but the legacy `locations` map is back-filled from
    /// the graph so that [`parish_world::WorldState::current_location`] never
    /// panics for a player location that's present in the graph.
    pub fn restore(
        self,
        world: &mut parish_world::WorldState,
        npc_manager: &mut parish_npc::manager::NpcManager,
    ) {
        world.clock = Self::restore_clock(&self.clock);
        world.player_location = self.player_location;
        world.weather = self.weather.parse().unwrap_or(parish_types::Weather::Clear);
        world.text_log = self.text_log;

        Self::restore_world_locations(
            world,
            self.player_location,
            self.visited_locations,
            self.visited_order,
            self.edge_traversals,
        );
        Self::restore_npcs(
            npc_manager,
            self.npcs,
            self.last_tier2_game_time,
            self.last_tier3_game_time,
            self.last_tier4_game_time,
            self.introduced_npcs,
            self.npcs_who_know_player_name,
        );

        world.gossip_network = self.gossip_network;
        // #1838 compatibility repair: #1396 restored this durable set and then
        // cleared it at every runtime boundary. Saves written after that clear
        // may contain an empty set while their bounded canonical dialogue still
        // proves an explicit identity reveal. Merge only claims accepted by the
        // same strict detector as the live apply seam; never infer from contact
        // or canonical speaker metadata alone. The next ordinary save persists
        // any healed ids without a schema migration.
        npc_manager.heal_introductions_from_conversation(&self.conversation_log);
        world.conversation_log = self.conversation_log;
        world.player_name = self.player_name;
        world.player_progress = self.player_progress;
        world.active_session = self.active_session;

        // `restore_npcs` rebuilds the manager from scratch, which wipes
        // the in-memory `tier_assignments` map. Silently re-seed it
        // here so the next live `assign_tiers` call doesn't see every
        // NPC's `old_tier` default to `Tier4` and re-broadcast bogus
        // `NpcArrived` events for everyone who happens to be at Tier1
        // in the saved state. Tier is derivable from the world+NPC
        // state we just restored, so we don't need to persist it in
        // the snapshot.
        npc_manager.seed_tier_state(world);
    }
}
