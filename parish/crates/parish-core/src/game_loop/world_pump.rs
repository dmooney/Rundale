//! Single backend-agnostic "advance the world by a tick" pump (rule #12).
//!
//! Every Parish runtime advances the simulation on a minute-advance: tick the
//! weather engine, run NPC schedules, reassign cognitive tiers, run the banshee
//! tick, propagate gossip, and dispatch the tier-4 rules engine. Before #1159
//! that pump was copy-pasted into all four loops — the async background loops in
//! `parish-server` and `parish-tauri`, the headless REPL in `parish-engine`, and
//! the `GameTestHarness` (`advance_time` + the `Wait`/`Tick` arms) — so a string
//! or behaviour could silently drift between them (it is how #1156's "1 minutes"
//! reached the harness).
//!
//! [`advance_world`] is now the **only** place those atoms are advanced. Each
//! runtime keeps its own *scheduling* (the server budgets gossip and skips
//! tier-4; the harness backfills weather on bulk jumps) by passing the right
//! [`AdvanceOptions`]; the pump returns an [`AdvanceReport`] so each runtime can
//! render its own debug events / player-visible narration from the same data.
//!
//! All gameplay events (weather changes, NPC arrivals/departures, banshee
//! deaths, tier-4 life events) are published on the world's own
//! [`event_bus`](crate::world::WorldState::event_bus); the pump does not touch
//! the frontend [`EventEmitter`](crate::ipc::EventEmitter) — runtimes forward
//! bus events to their frontends separately, exactly as before.

use std::collections::{HashMap, HashSet};

use rand::Rng;

use crate::npc::banshee::BansheeReport;
use crate::npc::manager::{NpcManager, ScheduleEvent, TierTransition};
use crate::npc::types::CogTier;
use crate::npc::{Npc, NpcId};
use crate::world::{Weather, WorldState};

/// How the weather engine is advanced during a pump.
#[derive(Debug, Clone, Copy)]
pub enum WeatherMode {
    /// One transition check at the current clock time, publishing
    /// `WeatherChanged` on a transition (via
    /// [`WorldState::tick_weather`](crate::world::WorldState::tick_weather)).
    /// Used by the real-time loops and the per-turn synchronous pump.
    Single,
    /// Catch up one transition check per elapsed game-hour across `minutes`,
    /// updating weather *state* but deliberately **not** publishing a
    /// `WeatherChanged` per backfilled hour — a 226-day jump would otherwise
    /// flood the broadcast bus and evict events tests drain for. Used by
    /// `GameTestHarness::advance_time` for bulk time jumps.
    Backfill { minutes: i64 },
    /// Skip the weather engine entirely.
    Skip,
}

/// How gossip propagates among co-located Tier-2 NPCs during a pump.
#[derive(Debug, Clone, Copy)]
pub enum GossipMode {
    /// Do not propagate gossip this tick (headless REPL, per-turn pump).
    Skip,
    /// Propagate among every co-located Tier-2 group (Tauri, `advance_time`).
    All,
    /// Process at most `budget` groups starting from `cursor`, round-robin
    /// across ticks so a single tick never walks the whole parish (server,
    /// #466). The returned [`AdvanceReport::gossip_cursor`] carries the
    /// advanced cursor back to the caller.
    Budgeted { cursor: usize, budget: usize },
}

/// Per-runtime knobs for [`advance_world`]. Each call site reproduces its own
/// scheduling by selecting which atoms run and how — this is the
/// "per-runtime scheduling" the single core pump preserves.
#[derive(Debug, Clone, Copy)]
pub struct AdvanceOptions {
    /// How (or whether) to tick the weather engine.
    pub weather: WeatherMode,
    /// Run the banshee tick (callers gate this on the `banshee` feature flag).
    pub run_banshee: bool,
    /// How (or whether) to propagate gossip among co-located Tier-2 NPCs.
    pub gossip: GossipMode,
    /// Dispatch the tier-4 rules engine when `needs_tier4_tick` is due. The
    /// server intentionally leaves this `false` (its loop never ran tier-4).
    pub run_tier4: bool,
}

/// What a single [`advance_world`] pump produced. Each runtime renders its own
/// debug events / player narration from these fields and reads back the gossip
/// cursor for the next budgeted tick.
#[derive(Debug, Default)]
pub struct AdvanceReport {
    /// The new weather if a transition fired this tick, else `None`.
    pub weather_change: Option<Weather>,
    /// NPC arrivals/departures produced by the schedule tick (already
    /// published on the world bus; returned so the caller can narrate them).
    pub schedule_events: Vec<ScheduleEvent>,
    /// Tier promotions/demotions produced by reassignment.
    pub tier_transitions: Vec<TierTransition>,
    /// Wails + deaths produced by the banshee tick (empty if it did not run).
    pub banshee: BansheeReport,
    /// Total rumours propagated this tick.
    pub gossip_count: usize,
    /// Advanced gossip cursor for the next [`GossipMode::Budgeted`] tick.
    pub gossip_cursor: usize,
    /// Number of tier-4 rules-engine events generated this tick (0 if tier-4
    /// did not run or was not due).
    pub tier4_event_count: usize,
    /// The [`GameEvent`]s the tier-4 tick applied to NPC state and published on
    /// the world bus this tick (births, deaths, illnesses, …). Returned so a
    /// runtime can mirror them into its own debug panel; the bus already
    /// carries them for frontend forwarding.
    ///
    /// [`GameEvent`]: parish_types::events::GameEvent
    pub tier4_game_events: Vec<parish_types::events::GameEvent>,
}

/// Builds canonical Tier-2 inference groups from the current world and NPC
/// state.
///
/// This shared projection keeps Tauri, server, and headless simulation prompts
/// in mode parity. Each snapshot carries only an authored activity whose
/// schedule location still matches the NPC's actual location.
pub fn build_tier2_groups(
    world: &WorldState,
    npc: &NpcManager,
) -> Vec<crate::npc::ticks::Tier2Group> {
    use crate::npc::ticks::{Tier2Group, npc_snapshot_from_npc_at};

    let groups_map = npc.tier2_groups();
    if groups_map.is_empty() {
        return Vec::new();
    }

    let npc_names: HashMap<_, _> = npc
        .all_npcs()
        .map(|person| (person.id, person.name.clone()))
        .collect();
    let now = world.clock.now();
    let mut location_names: Vec<String> = world
        .graph
        .location_ids()
        .into_iter()
        .filter_map(|id| world.graph.get(id).map(|location| location.name.clone()))
        .collect();
    location_names.sort();

    let mut groups: Vec<Tier2Group> = groups_map
        .into_iter()
        .filter_map(|(location, npc_ids)| {
            let location_name = world
                .graph
                .get(location)
                .map(|data| data.name.clone())
                .unwrap_or_else(|| format!("Location {}", location.0));
            let snapshots: Vec<_> = npc_ids
                .iter()
                .filter_map(|id| npc.get(*id))
                .filter_map(|person| npc_snapshot_from_npc_at(person, &npc_names, now))
                .collect();
            let mut snapshots = snapshots;
            snapshots.sort_by_key(|snapshot| snapshot.id.0);
            if snapshots.len() < 2 {
                return None;
            }
            Some(Tier2Group {
                location,
                other_location_names: location_names
                    .iter()
                    .filter(|name| !name.eq_ignore_ascii_case(&location_name))
                    .cloned()
                    .collect(),
                location_name,
                npcs: snapshots,
            })
        })
        .collect();
    groups.sort_by_key(|group| group.location.0);
    groups
}

/// Advances the world one pump: weather → schedules → tiers → banshee → gossip
/// → tier-4, gated by `opts`. The canonical ordering matches the pre-#1159
/// real-time loops (weather first, then schedules, then tiers). RNG is consumed
/// only by weather, gossip, and tier-4, always in that relative order, so a
/// seeded harness stays deterministic.
pub fn advance_world(
    world: &mut WorldState,
    npc: &mut NpcManager,
    rng: &mut impl Rng,
    opts: AdvanceOptions,
) -> AdvanceReport {
    // 1. Weather (RNG).
    let weather_change = match opts.weather {
        WeatherMode::Single => world.tick_weather(rng),
        WeatherMode::Backfill { minutes } => backfill_weather(world, minutes, rng),
        WeatherMode::Skip => None,
    };

    // 2. NPC schedules → arrivals/departures published on the world bus.
    let schedule_events =
        npc.tick_schedules(&world.clock, &world.graph, world.weather, &world.event_bus);

    // 3. Tier reassignment (post-move, matching the real-time loops).
    let tier_transitions = npc.assign_tiers(world, &[]);

    // 4. Banshee — herald and finalise doomed NPCs.
    let banshee = if opts.run_banshee {
        npc.tick_banshee(
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        )
    } else {
        BansheeReport::default()
    };

    // 5. Gossip (RNG).
    let (gossip_count, gossip_cursor) = match opts.gossip {
        GossipMode::Skip => (0, 0),
        GossipMode::All => (propagate_gossip_all(world, npc, rng), 0),
        GossipMode::Budgeted { cursor, budget } => {
            if world.gossip_network.is_empty() {
                (0, cursor)
            } else {
                let groups = npc.tier2_groups();
                let mut total = 0usize;
                let network = &mut world.gossip_network;
                let next_cursor = budgeted_round_robin(&groups, cursor, budget, |npc_ids| {
                    total += crate::npc::ticks::propagate_gossip_at_location(npc_ids, network, rng);
                });
                (total, next_cursor)
            }
        }
    };

    // 6. Tier-4 rules engine (RNG), when due.
    let (tier4_event_count, tier4_game_events) = if opts.run_tier4 {
        dispatch_tier4(world, npc, opts.run_banshee, rng)
    } else {
        (0, Vec::new())
    };

    AdvanceReport {
        weather_change,
        schedule_events,
        tier_transitions,
        banshee,
        gossip_count,
        gossip_cursor,
        tier4_event_count,
        tier4_game_events,
    }
}

/// Bulk weather catch-up over `minutes`, one transition check per elapsed
/// game-hour. Updates `world.weather` but publishes no `WeatherChanged` events
/// (see [`WeatherMode::Backfill`]). Returns the last transition, if any.
fn backfill_weather(world: &mut WorldState, minutes: i64, rng: &mut impl Rng) -> Option<Weather> {
    let season = world.clock.season();
    let now = world.clock.now();
    let hours_elapsed = (minutes / 60).max(1) as u32;
    let mut last = None;
    for h in 0..hours_elapsed {
        let check_time =
            now - chrono::Duration::minutes((hours_elapsed.saturating_sub(h + 1) as i64) * 60);
        if let Some(new_weather) = world.weather_engine.tick(check_time, season, rng) {
            world.weather = new_weather;
            last = Some(new_weather);
        }
    }
    last
}

/// Propagates gossip among every co-located Tier-2 group, in sorted
/// `LocationId` order for deterministic RNG sequencing. Returns rumours spread.
fn propagate_gossip_all(world: &mut WorldState, npc: &NpcManager, rng: &mut impl Rng) -> usize {
    if world.gossip_network.is_empty() {
        return 0;
    }
    let groups = npc.tier2_groups();
    let mut sorted_keys: Vec<_> = groups.keys().copied().collect();
    sorted_keys.sort();
    let mut total = 0usize;
    for loc in sorted_keys {
        let npc_ids = &groups[&loc];
        if npc_ids.len() >= 2 {
            total += crate::npc::ticks::propagate_gossip_at_location(
                npc_ids,
                &mut world.gossip_network,
                rng,
            );
        }
    }
    total
}

/// Runs at most `budget` propagations across `groups`, starting from the group
/// at position `cursor` in `LocationId` order and wrapping around (#466).
/// Returns the new cursor to persist for the next tick, so the round-robin
/// makes forward progress through the group list over successive ticks rather
/// than re-hitting the same prefix every time.
///
/// Groups with fewer than 2 NPCs are skipped silently and do *not* consume
/// budget — they are no-ops for gossip and counting them would let a cluster of
/// sparse groups waste an entire tick's budget.
///
/// The propagation work itself is handed off via a `propagate` callback so the
/// helper stays free of the specific `GossipNetwork` / `Rng` types it would
/// otherwise name, keeping it unit-testable with a counting stub. This is the
/// single home of the budgeting math that `parish-server` used to own (#1159).
pub fn budgeted_round_robin<F>(
    groups: &std::collections::HashMap<crate::world::LocationId, Vec<NpcId>>,
    cursor: usize,
    budget: usize,
    mut propagate: F,
) -> usize
where
    F: FnMut(&[NpcId]),
{
    // Sort groups by LocationId so the cursor addresses a stable order across
    // ticks; `HashMap::iter` order would shift on every resize.
    let mut sorted_keys: Vec<crate::world::LocationId> = groups.keys().copied().collect();
    sorted_keys.sort();
    let n = sorted_keys.len();
    if n == 0 {
        return 0;
    }
    let start = cursor % n;
    let mut consumed = 0;
    for i in 0..n {
        if consumed >= budget {
            return (start + i) % n;
        }
        let idx = (start + i) % n;
        let loc = sorted_keys[idx];
        if let Some(npc_ids) = groups.get(&loc)
            && npc_ids.len() >= 2
        {
            propagate(npc_ids);
            consumed += 1;
        }
    }
    // Wrapped all the way around without hitting the budget — advance the cursor
    // by the number of groups we actually processed so we still rotate each tick.
    (start + consumed) % n
}

/// Applies a slice of Tier-2 events to NPC state and mints `GossipSpread`
/// events on the world event bus for any notable interactions.
///
/// This is the single shared implementation of the Tier-2 post-processing loop
/// that previously existed verbatim in `parish-engine/src/headless.rs` and
/// `parish-tauri/src/setup.rs` (TD-030). Every backend (CLI, Tauri, web) must
/// call this helper rather than copy the logic — rule #12.
///
/// Returns a list of debug strings describing what happened (mood/relationship
/// changes, dropped interactions).
///
/// # Arguments
///
/// * `events` — Tier-2 events produced by one or more `run_tier2_for_group`
///   calls.
/// * `npcs` — Mutable NPC map, updated in-place (mood, relationships,
///   memories).
/// * `game_time` — The current game clock at the moment the events are applied.
/// * `config` — NPC simulation config (tick intervals, thresholds). Pass
///   `&NpcConfig::default()` unless the runtime has a custom config.
/// * `world` — Mutable world state; the event bus and gossip network are
///   accessed through it. Taking `&mut WorldState` rather than the two fields
///   separately avoids a borrow-check conflict when the caller holds a
///   `MutexGuard<WorldState>` (the borrow splitter cannot project through a
///   `DerefMut` impl). Each backend (CLI, Tauri, web) passes its locked world
///   guard directly.
pub fn mint_tier2_gossip(
    events: &[crate::npc::types::Tier2Event],
    npcs: &mut std::collections::HashMap<NpcId, Npc>,
    game_time: chrono::DateTime<chrono::Utc>,
    config: &crate::config::NpcConfig,
    world: &mut WorldState,
) -> Vec<String> {
    let mut debug = Vec::new();
    for event in events {
        match crate::npc::ticks::apply_grounded_tier2_event_with_config(
            event,
            npcs,
            game_time,
            config,
            &world.event_bus,
            &mut world.gossip_network,
        ) {
            crate::npc::ticks::GroundedTier2ApplyOutcome::Applied(mut event_debug) => {
                debug.append(&mut event_debug);
            }
            crate::npc::ticks::GroundedTier2ApplyOutcome::Rejected(reason) => {
                debug.push(format!("Tier 2 event dropped as stale: {reason}"));
            }
        }
    }
    debug
}

/// Dispatches the tier-4 rules engine if enough game time has elapsed. Applies
/// the resulting events to NPC state, publishes the derived [`GameEvent`]s on
/// the world bus, and records the tick. Returns `(tier4_event_count,
/// applied_game_events)` — the count is the number of [`Tier4Event`]s rolled,
/// the events are what was published (for debug-panel mirroring).
///
/// [`GameEvent`]: parish_types::events::GameEvent
/// [`Tier4Event`]: crate::npc::tier4::Tier4Event
fn dispatch_tier4(
    world: &mut WorldState,
    npc: &mut NpcManager,
    banshee_on: bool,
    rng: &mut impl Rng,
) -> (usize, Vec<parish_types::events::GameEvent>) {
    let now = world.clock.now();
    if !npc.needs_tier4_tick(now) {
        return (0, Vec::new());
    }
    let tier4_ids: HashSet<NpcId> = npc.npcs_in_tier(CogTier::Tier4).into_iter().collect();
    let events = {
        let mut tier4_refs: Vec<&mut Npc> = npc
            .npcs_mut()
            .values_mut()
            .filter(|n| tier4_ids.contains(&n.id))
            .collect();
        // Sort by NpcId for deterministic RNG sequencing across ticks.
        tier4_refs.sort_by_key(|n| n.id);
        let season = world.clock.season();
        let game_date = now.date_naive();
        crate::npc::tier4::tick_tier4(&mut tier4_refs, season, game_date, rng)
    };
    let game_events = npc.apply_tier4_events(&events, now, banshee_on);
    for evt in &game_events {
        world.event_bus.publish(evt.clone());
    }
    npc.record_tier4_tick(now);
    (events.len(), game_events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::LocationId;

    fn make_group(n: u32) -> (LocationId, Vec<NpcId>) {
        // 2 NPCs so the group is gossip-eligible.
        (LocationId(n), vec![NpcId(n * 10), NpcId(n * 10 + 1)])
    }

    fn grounding_at(
        npcs: &HashMap<NpcId, Npc>,
        participants: &[NpcId],
        location: LocationId,
        game_time: chrono::DateTime<chrono::Utc>,
    ) -> Vec<crate::npc::types::Tier2ParticipantGrounding> {
        participants
            .iter()
            .map(|npc_id| {
                let npc = &npcs[npc_id];
                crate::npc::types::Tier2ParticipantGrounding {
                    npc_id: *npc_id,
                    location,
                    grounding_revision: npc.grounding_revision(),
                    activity_fingerprint: crate::npc::ticks::tier2_activity_fingerprint_from_npc_at(
                        npc, game_time,
                    ),
                }
            })
            .collect()
    }

    // ── #466 / #1159 gossip budget round-robin (moved from parish-server) ──────

    #[test]
    fn gossip_budget_empty_returns_zero_cursor() {
        let groups = std::collections::HashMap::new();
        let mut calls = 0;
        let new_cursor = budgeted_round_robin(&groups, 42, 20, |_| calls += 1);
        assert_eq!(new_cursor, 0);
        assert_eq!(calls, 0);
    }

    #[test]
    fn gossip_budget_caps_at_budget_and_returns_next_cursor() {
        // 50 eligible groups, budget 20 — expect 20 propagations and cursor=20.
        let mut groups = std::collections::HashMap::new();
        for i in 1..=50 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        let new_cursor = budgeted_round_robin(&groups, 0, 20, |_| calls += 1);
        assert_eq!(calls, 20);
        assert_eq!(new_cursor, 20, "cursor should advance by the budget");
    }

    #[test]
    fn gossip_budget_round_robins_across_ticks() {
        // 30 groups, budget 20. Tick 1 does 0..20, tick 2 should pick up at 20
        // and wrap through 29, 0..9 — ending at cursor 10 (20+20 mod 30).
        let mut groups = std::collections::HashMap::new();
        for i in 1..=30 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }

        let mut seen: Vec<LocationId> = Vec::new();
        let new_cursor = budgeted_round_robin(&groups, 0, 20, |npc_ids| {
            seen.push(LocationId(npc_ids[0].0 / 10));
        });
        assert_eq!(new_cursor, 20);
        assert_eq!(seen.len(), 20);

        let mut next_seen: Vec<LocationId> = Vec::new();
        let next_cursor = budgeted_round_robin(&groups, new_cursor, 20, |npc_ids| {
            next_seen.push(LocationId(npc_ids[0].0 / 10));
        });
        assert_eq!(next_cursor, 10, "wrap: (20+20) mod 30 = 10");
        assert_eq!(next_seen.len(), 20);
        assert_eq!(next_seen[0], LocationId(21));
        assert_eq!(next_seen[19], LocationId(10));
    }

    #[test]
    fn gossip_budget_skips_sparse_groups_without_consuming_budget() {
        let mut groups = std::collections::HashMap::new();
        for i in 1..=10u32 {
            let (loc, mut npcs) = make_group(i);
            if i.is_multiple_of(2) {
                npcs.truncate(1);
            }
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        let _ = budgeted_round_robin(&groups, 0, 3, |_| calls += 1);
        assert_eq!(calls, 3, "sparse groups must not consume budget");
    }

    #[test]
    fn gossip_budget_cursor_wraps_modulo_group_count() {
        let mut groups = std::collections::HashMap::new();
        for i in 1..=5 {
            let (loc, npcs) = make_group(i);
            groups.insert(loc, npcs);
        }
        let mut calls = 0;
        let new_cursor = budgeted_round_robin(&groups, 1_000_000, 2, |_| calls += 1);
        assert_eq!(calls, 2);
        assert_eq!(new_cursor, 2);
    }

    // ── advance_world wiring ───────────────────────────────────────────────────

    #[test]
    fn skip_options_consume_no_rng_and_publish_no_weather() {
        // With everything skipped, the pump only ticks schedules + tiers; no
        // weather transition is reported and the RNG is untouched, so a seeded
        // harness stays deterministic (this is the harness per-turn config).
        use crate::world::WorldState;
        use rand::rngs::StdRng;
        use rand::{Rng, SeedableRng};

        let mut world = WorldState::new();
        let mut npc = NpcManager::new();
        let mut rng_a = StdRng::seed_from_u64(7);
        let mut rng_b = StdRng::seed_from_u64(7);

        let report = advance_world(
            &mut world,
            &mut npc,
            &mut rng_a,
            AdvanceOptions {
                weather: WeatherMode::Skip,
                run_banshee: false,
                gossip: GossipMode::Skip,
                run_tier4: false,
            },
        );
        assert!(report.weather_change.is_none());
        assert_eq!(report.gossip_count, 0);
        assert_eq!(report.tier4_event_count, 0);
        // RNG untouched: a second generator at the same seed is still in lockstep.
        assert_eq!(rng_a.next_u64(), rng_b.next_u64());
    }

    // ── mint_tier2_gossip wiring (TD-030) ─────────────────────────────────────

    /// Verifies that `mint_tier2_gossip` mints a `GossipSpread` event on the
    /// world bus for a notable Tier-2 interaction (summary > 30 chars or
    /// |delta| > 0.3). This is the shared path that ALL three backends now call
    /// (CLI, Tauri, web/Axum). The test exercises the exact code path that
    /// previously did not exist on the web backend (TD-040).
    #[test]
    fn mint_tier2_gossip_publishes_gossip_spread() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{MoodChange, RelationshipChange, Tier2Event};
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::events::GameEvent;
        use parish_types::{LocationId, NpcId};
        use std::collections::HashMap;

        let mut world = WorldState::new();
        // Subscribe before publishing so we can drain the bus.
        let mut rx = world.event_bus.subscribe();

        // Build a minimal NPC map: two participants at the same location.
        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = NpcId(1);
        npc1.set_location(LocationId(1));
        npcs.insert(NpcId(1), npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = NpcId(2);
        npc2.set_location(LocationId(1));
        npcs.insert(NpcId(2), npc2);

        // A Tier-2 event with a non-trivial summary (>30 chars) — this
        // satisfies the `create_gossip_from_tier2_event` threshold.
        let summary = "Padraig and Brigid shared stories about the coming harvest".to_string();
        assert!(
            summary.len() > 30,
            "test summary must exceed the gossip threshold"
        );
        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 20, 0, 0).unwrap();
        let grounding = grounding_at(&npcs, &[NpcId(1), NpcId(2)], LocationId(1), game_time);
        let event = Tier2Event {
            location: LocationId(1),
            summary: summary.clone(),
            participants: vec![NpcId(1), NpcId(2)],
            mood_changes: vec![MoodChange {
                npc_id: NpcId(1),
                new_mood: "cheerful".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: NpcId(1),
                to: NpcId(2),
                delta: 0.1,
            }],
            grounding,
        };

        mint_tier2_gossip(
            &[event],
            &mut npcs,
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        // A GossipSpread event must have been published on the world bus.
        let mut found_gossip_spread = false;
        while let Ok(evt) = rx.try_recv() {
            if matches!(evt, GameEvent::GossipSpread { .. }) {
                found_gossip_spread = true;
            }
        }
        assert!(
            found_gossip_spread,
            "mint_tier2_gossip must publish a GossipSpread event on the world bus \
            for a notable Tier-2 interaction (summary > 30 chars)"
        );
    }

    /// #1785: an NPC may move while Tier-2 inference is in flight. The whole
    /// event must be rejected before the still-present participant receives a
    /// mood/relationship update, memory, interaction event, or gossip item.
    #[test]
    fn mint_tier2_gossip_rejects_move_between_snapshot_and_apply() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{
            MoodChange, Relationship, RelationshipChange, RelationshipKind, Tier2Event,
        };
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let participants = [NpcId(1), NpcId(2)];
        let mut npcs = HashMap::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = participants[0];
        npc1.set_location(LocationId(1));
        npc1.relationships.insert(
            participants[1],
            Relationship::new(RelationshipKind::Friend, 0.2),
        );
        npcs.insert(npc1.id, npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = participants[1];
        npc2.set_location(LocationId(1));
        npcs.insert(npc2.id, npc2);

        let event = Tier2Event {
            location: LocationId(1),
            summary: "At The Crossroads — both neighbours exchange important parish news."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[0],
                new_mood: "alarmed".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: participants[0],
                to: participants[1],
                delta: 0.5,
            }],
            grounding: grounding_at(&npcs, &participants, LocationId(1), game_time),
        };

        // The inference completes after participant 2 has physically moved.
        npcs.get_mut(&participants[1])
            .unwrap()
            .set_location(LocationId(2));
        let mut world = WorldState::new();
        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            &mut npcs,
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        let npc1 = &npcs[&participants[0]];
        assert_eq!(npc1.mood, "content");
        assert_eq!(
            npc1.relationships[&participants[1]].strength, 0.2,
            "relationship delta must not partially apply"
        );
        assert_eq!(npc1.memory.len(), 0, "stale prose must not enter memory");
        assert!(
            world.gossip_network.is_empty(),
            "stale prose must not seed gossip"
        );
        assert!(
            rx.try_recv().is_err(),
            "no world event may publish before stale-result rejection"
        );
        assert!(
            debug.iter().any(|line| line.contains("moved from 1 to 2")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// #1785: remaining at the same location is insufficient if the authored
    /// schedule activity changed while inference was in flight.
    #[test]
    fn mint_tier2_gossip_rejects_changed_activity_fingerprint() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{
            MoodChange, ScheduleEntry, ScheduleVariant, SeasonalSchedule, Tier2Event,
        };
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let snapshot_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let apply_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 11, 0, 0).unwrap();
        let participants = [NpcId(1), NpcId(2)];
        let mut npcs = HashMap::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = participants[0];
        npc1.set_location(LocationId(1));
        npc1.set_schedule(Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![
                    ScheduleEntry {
                        start_hour: 10,
                        end_hour: 10,
                        location: LocationId(1),
                        activity: "running errands and delivering repaired tools".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 11,
                        end_hour: 11,
                        location: LocationId(1),
                        activity: "waiting at the crossroads for another errand".to_string(),
                        cuaird: false,
                    },
                ],
            }],
        }));
        npcs.insert(npc1.id, npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = participants[1];
        npc2.set_location(LocationId(1));
        npcs.insert(npc2.id, npc2);

        let event = Tier2Event {
            location: LocationId(1),
            summary: "At The Crossroads — Colm is running errands; Tommy tells stories."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[1],
                new_mood: "amused".to_string(),
            }],
            relationship_changes: Vec::new(),
            grounding: grounding_at(&npcs, &participants, LocationId(1), snapshot_time),
        };

        let mut world = WorldState::new();
        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            &mut npcs,
            apply_time,
            &NpcConfig::default(),
            &mut world,
        );

        assert_eq!(npcs[&participants[1]].mood, "content");
        assert!(npcs[&participants[0]].memory.is_empty());
        assert!(npcs[&participants[1]].memory.is_empty());
        assert!(world.gossip_network.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(
            debug
                .iter()
                .any(|line| line.contains("activity fingerprint changed")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// #1785: callers cannot bypass grounding by constructing a raw event.
    /// Missing anchors fail before any event, memory, mechanical delta, or
    /// gossip side effect.
    #[test]
    fn mint_tier2_gossip_rejects_missing_anchors_without_partial_effects() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{
            MoodChange, Relationship, RelationshipChange, RelationshipKind, Tier2Event,
        };
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let participants = [NpcId(1), NpcId(2)];
        let mut npcs = HashMap::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = participants[0];
        npc1.relationships.insert(
            participants[1],
            Relationship::new(RelationshipKind::Friend, 0.2),
        );
        npcs.insert(npc1.id, npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = participants[1];
        npcs.insert(npc2.id, npc2);

        let event = Tier2Event {
            location: LocationId(1),
            summary: "Both neighbours exchange important parish news at the crossroads."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[0],
                new_mood: "alarmed".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: participants[0],
                to: participants[1],
                delta: 0.5,
            }],
            grounding: Vec::new(),
        };

        let mut world = WorldState::new();
        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            &mut npcs,
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        assert_eq!(npcs[&participants[0]].mood, "content");
        assert_eq!(
            npcs[&participants[0]].relationships[&participants[1]].strength,
            0.2
        );
        assert!(npcs.values().all(|npc| npc.memory.is_empty()));
        assert!(world.gossip_network.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(
            debug
                .iter()
                .any(|line| line.contains("participant/grounding count mismatch")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// #1785: restoring an identical snapshot creates a new live incarnation.
    /// A pre-restore inference result must therefore fail even though every
    /// serialized NPC value (location, schedule, mood) still matches.
    #[test]
    fn mint_tier2_gossip_rejects_event_after_identical_snapshot_restore() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::manager::NpcManager;
        use crate::npc::types::{
            MoodChange, Relationship, RelationshipChange, RelationshipKind, Tier2Event,
        };
        use crate::persistence::GameSnapshot;
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let participants = [NpcId(1), NpcId(2)];
        let mut manager = NpcManager::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = participants[0];
        npc1.relationships.insert(
            participants[1],
            Relationship::new(RelationshipKind::Friend, 0.2),
        );
        manager.add_npc(npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = participants[1];
        manager.add_npc(npc2);

        let before_revision = manager.get(participants[0]).unwrap().grounding_revision();
        let event = Tier2Event {
            location: LocationId(1),
            summary: "Both neighbours exchange important parish news at the crossroads."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[0],
                new_mood: "alarmed".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: participants[0],
                to: participants[1],
                delta: 0.5,
            }],
            grounding: grounding_at(manager.npcs(), &participants, LocationId(1), game_time),
        };

        let mut world = WorldState::new();
        GameSnapshot::capture(&world, &manager).restore(&mut world, &mut manager);
        assert_eq!(
            manager.get(participants[0]).unwrap().location(),
            LocationId(1)
        );
        assert_ne!(
            manager.get(participants[0]).unwrap().grounding_revision(),
            before_revision,
            "restore must create a fresh live grounding lineage"
        );

        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            manager.npcs_mut(),
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        let npc1 = manager.get(participants[0]).unwrap();
        assert_eq!(npc1.mood, "content");
        assert_eq!(npc1.relationships[&participants[1]].strength, 0.2);
        assert!(manager.all_npcs().all(|npc| npc.memory.is_empty()));
        assert!(world.gossip_network.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(
            debug
                .iter()
                .any(|line| line.contains("grounding revision changed")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// #1785: value fingerprints alone cannot detect an A→B→A location
    /// transition. The process-local revision makes the restored value stale.
    #[test]
    fn mint_tier2_gossip_rejects_location_aba_transition() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{MoodChange, Tier2Event};
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let participants = [NpcId(1), NpcId(2)];
        let mut npcs = HashMap::new();
        for id in participants {
            let mut npc = Npc::new_test_npc();
            npc.id = id;
            npcs.insert(id, npc);
        }
        let event = Tier2Event {
            location: LocationId(1),
            summary: "Both neighbours exchange important parish news at the crossroads."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[0],
                new_mood: "alarmed".to_string(),
            }],
            relationship_changes: Vec::new(),
            grounding: grounding_at(&npcs, &participants, LocationId(1), game_time),
        };

        let old_revision = npcs[&participants[1]].grounding_revision();
        npcs.get_mut(&participants[1])
            .unwrap()
            .set_location(LocationId(2));
        npcs.get_mut(&participants[1])
            .unwrap()
            .set_location(LocationId(1));
        assert_ne!(npcs[&participants[1]].grounding_revision(), old_revision);

        let mut world = WorldState::new();
        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            &mut npcs,
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        assert_eq!(npcs[&participants[0]].mood, "content");
        assert!(npcs.values().all(|npc| npc.memory.is_empty()));
        assert!(world.gossip_network.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(
            debug
                .iter()
                .any(|line| line.contains("grounding revision changed")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// #1785: a bulk wait may jump directly from authored interval A to a
    /// later, text-identical A without ever sampling the intervening B.
    /// Interval identity must still reject the old inference atomically.
    #[test]
    fn bulk_wait_rejects_stale_tier2_activity_interval_with_zero_side_effects() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::manager::NpcManager;
        use crate::npc::types::{
            MoodChange, Relationship, RelationshipChange, RelationshipKind, ScheduleEntry,
            ScheduleVariant, SeasonalSchedule, Tier2Event,
        };
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId, Weather};
        use parish_world::time::GameClock;
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        let participants = [NpcId(1), NpcId(2)];
        let mut manager = NpcManager::new();
        let mut npc1 = Npc::new_test_npc();
        npc1.id = participants[0];
        npc1.set_schedule(Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![
                    ScheduleEntry {
                        start_hour: 10,
                        end_hour: 10,
                        location: LocationId(1),
                        activity: "mending nets".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 11,
                        end_hour: 11,
                        location: LocationId(1),
                        activity: "hauling turf".to_string(),
                        cuaird: false,
                    },
                    ScheduleEntry {
                        start_hour: 12,
                        end_hour: 12,
                        location: LocationId(1),
                        activity: "mending nets".to_string(),
                        cuaird: false,
                    },
                ],
            }],
        }));
        npc1.relationships.insert(
            participants[1],
            Relationship::new(RelationshipKind::Friend, 0.2),
        );
        manager.add_npc(npc1);
        let mut npc2 = Npc::new_test_npc();
        npc2.id = participants[1];
        manager.add_npc(npc2);

        let mut world = WorldState::new();
        world.clock = GameClock::new(chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap());
        world.clock.pause();
        world.weather = Weather::Clear;
        let mut rng = StdRng::seed_from_u64(1785);
        let quiet_pump = AdvanceOptions {
            weather: WeatherMode::Skip,
            run_banshee: false,
            gossip: GossipMode::Skip,
            run_tier4: false,
        };

        // The same shared pump used after a live `/wait` synchronizes the
        // prompt-time authored interval.
        advance_world(&mut world, &mut manager, &mut rng, quiet_pump);
        let snapshot_time = world.clock.now();
        let original_revision = manager.get(participants[0]).unwrap().grounding_revision();
        let original_grounding =
            grounding_at(manager.npcs(), &participants, LocationId(1), snapshot_time);
        let event = Tier2Event {
            location: LocationId(1),
            summary: "Both neighbours exchange important parish news at the crossroads."
                .to_string(),
            participants: participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: participants[1],
                new_mood: "alarmed".to_string(),
            }],
            relationship_changes: vec![RelationshipChange {
                from: participants[0],
                to: participants[1],
                delta: 0.5,
            }],
            grounding: original_grounding,
        };

        // Canonical bulk-wait shape: move the clock once, then run one pump.
        // No tick observes the 11:00 B interval.
        world.clock.advance(120);
        advance_world(&mut world, &mut manager, &mut rng, quiet_pump);
        assert!(
            manager.get(participants[0]).unwrap().grounding_revision() > original_revision,
            "the later A interval must advance lineage even when B was skipped"
        );
        let apply_time = world.clock.now();
        assert_ne!(
            crate::npc::ticks::tier2_activity_fingerprint_from_npc_at(
                manager.get(participants[0]).unwrap(),
                apply_time,
            ),
            event.grounding[0].activity_fingerprint,
            "separate authored A intervals must have distinct fingerprints"
        );

        let mut rx = world.event_bus.subscribe();
        let debug = mint_tier2_gossip(
            &[event],
            manager.npcs_mut(),
            apply_time,
            &NpcConfig::default(),
            &mut world,
        );

        assert_eq!(manager.get(participants[1]).unwrap().mood, "content");
        assert_eq!(
            manager.get(participants[0]).unwrap().relationships[&participants[1]].strength,
            0.2,
            "stale relationship delta must not partially apply"
        );
        assert!(manager.all_npcs().all(|npc| npc.memory.is_empty()));
        assert!(world.gossip_network.is_empty());
        assert!(rx.try_recv().is_err());
        assert!(
            debug
                .iter()
                .any(|line| line.contains("grounding revision changed")),
            "debug output should explain the rejection: {debug:?}"
        );
    }

    /// A grounding change in one group must not invalidate a distinct group.
    /// Revisions are stored per NPC rather than compared to a global epoch.
    #[test]
    fn mint_tier2_gossip_keeps_unrelated_group_valid() {
        use crate::config::NpcConfig;
        use crate::npc::Npc;
        use crate::npc::types::{MoodChange, Tier2Event};
        use crate::world::WorldState;
        use chrono::TimeZone;
        use parish_types::{LocationId, NpcId};

        let game_time = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut npcs = HashMap::new();
        for (id, location) in [
            (NpcId(1), LocationId(1)),
            (NpcId(2), LocationId(1)),
            (NpcId(3), LocationId(2)),
            (NpcId(4), LocationId(2)),
        ] {
            let mut npc = Npc::new_test_npc();
            npc.id = id;
            npc.set_location(location);
            npcs.insert(id, npc);
        }

        let stale_participants = [NpcId(1), NpcId(2)];
        let valid_participants = [NpcId(3), NpcId(4)];
        let stale = Tier2Event {
            location: LocationId(1),
            summary: "The first pair exchange substantive parish news at the crossroads."
                .to_string(),
            participants: stale_participants.to_vec(),
            mood_changes: Vec::new(),
            relationship_changes: Vec::new(),
            grounding: grounding_at(&npcs, &stale_participants, LocationId(1), game_time),
        };
        let valid = Tier2Event {
            location: LocationId(2),
            summary: "The second pair exchange substantive parish news beside the mill."
                .to_string(),
            participants: valid_participants.to_vec(),
            mood_changes: vec![MoodChange {
                npc_id: valid_participants[0],
                new_mood: "cheerful".to_string(),
            }],
            relationship_changes: Vec::new(),
            grounding: grounding_at(&npcs, &valid_participants, LocationId(2), game_time),
        };
        npcs.get_mut(&stale_participants[0])
            .unwrap()
            .set_location(LocationId(99));
        npcs.get_mut(&stale_participants[0])
            .unwrap()
            .set_location(LocationId(1));

        let mut world = WorldState::new();
        let debug = mint_tier2_gossip(
            &[stale, valid],
            &mut npcs,
            game_time,
            &NpcConfig::default(),
            &mut world,
        );

        assert!(npcs[&stale_participants[0]].memory.is_empty());
        assert!(npcs[&stale_participants[1]].memory.is_empty());
        assert_eq!(npcs[&valid_participants[0]].mood, "cheerful");
        assert_eq!(npcs[&valid_participants[0]].memory.len(), 1);
        assert_eq!(npcs[&valid_participants[1]].memory.len(), 1);
        assert_eq!(world.gossip_network.len(), 1);
        assert_eq!(
            debug
                .iter()
                .filter(|line| line.contains("dropped as stale"))
                .count(),
            1
        );
    }
}
