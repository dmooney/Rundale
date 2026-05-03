//! Pure helpers backing [`crate::manager::NpcManager::tick_schedules`].
//!
//! `tick_schedules` does two things on every clock advance:
//!
//! 1. Decide where each `Present` NPC *wants* to be (the "resolution" phase).
//! 2. Mutate the NPC: start transit, complete transit, publish events.
//!
//! This module owns the resolution phase. It is deliberately side-effect-free
//! so the seasonal-schedule lookup, cuaird rotation, weather-shelter override,
//! and farmer "tolerates light rain" rule can all be unit-tested without
//! constructing an `NpcManager`, a `WorldGraph`, or a tick loop. See GitHub
//! issue #697.

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Utc};
use parish_types::{DayType, LocationId, NpcId, Season, Weather};
use parish_world::graph::WorldGraph;

use crate::Npc;

/// Per-NPC list of friend home locations, used when an NPC's active schedule
/// entry is flagged `cuaird` (rotating evening visit).
#[derive(Debug, Clone, Default)]
pub struct CuairdTargets(pub HashMap<NpcId, Vec<LocationId>>);

impl CuairdTargets {
    /// Returns the friend list for `id`, or an empty slice when the NPC has no
    /// matching friends or is not tracked.
    pub fn get(&self, id: NpcId) -> &[LocationId] {
        self.0.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Builds [`CuairdTargets`] from the manager's NPC map.
///
/// For each NPC, gathers the home locations of every friend with relationship
/// strength > 0.3. Used by [`resolve_desired_location`] to rotate cuaird visits
/// by day-of-year, so a strong friend's hearth is visited each evening rather
/// than the same one every night.
pub fn collect_cuaird_targets(npcs: &HashMap<NpcId, Npc>) -> CuairdTargets {
    let map: HashMap<NpcId, Vec<LocationId>> = npcs
        .iter()
        .map(|(id, npc)| {
            let friends: Vec<LocationId> = npc
                .relationships
                .iter()
                .filter(|(_, r)| r.strength > 0.3)
                .filter_map(|(friend_id, _)| npcs.get(friend_id).and_then(|f| f.home))
                .collect();
            (*id, friends)
        })
        .collect();
    CuairdTargets(map)
}

/// Inputs needed to resolve a single NPC's desired location for one tick.
///
/// Bundling these into a struct keeps [`resolve_desired_location`] cheap to
/// call and easy to mock in tests.
#[derive(Debug, Clone, Copy)]
pub struct TickContext<'a> {
    /// Wall-clock time in the game world (used for cuaird rotation and the
    /// caller's transit arrival math).
    pub now: DateTime<Utc>,
    /// Hour of day (0..=23), already extracted from `now`.
    pub current_hour: u8,
    /// Current season — selects a seasonal schedule variant.
    pub season: Season,
    /// Whether today is a weekday, market day, etc. — selects a schedule
    /// variant.
    pub day_type: DayType,
    /// Current weather; drives the shelter override.
    pub weather: Weather,
    /// World graph; consulted for the `indoor` flag of the prospective
    /// destination and home.
    pub graph: &'a WorldGraph,
}

impl<'a> TickContext<'a> {
    /// Convenience builder. Extracts `current_hour` from `now`.
    pub fn new(
        now: DateTime<Utc>,
        season: Season,
        day_type: DayType,
        weather: Weather,
        graph: &'a WorldGraph,
    ) -> Self {
        use chrono::Timelike;
        Self {
            now,
            current_hour: now.hour() as u8,
            season,
            day_type,
            weather,
            graph,
        }
    }
}

/// Outcome of resolving an NPC's desired location for the current tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleResolution {
    /// The NPC has no business going anywhere this tick (no schedule, no
    /// active entry, weather forces a no-op, or already at the desired
    /// location). The caller leaves the NPC's state untouched.
    Stay,
    /// The NPC should head to `target`. The caller is still responsible for
    /// resolving a path and possibly short-circuiting if the target equals
    /// the NPC's current location.
    MoveTo {
        /// Where the NPC wants to be after this tick resolves.
        target: LocationId,
    },
}

/// Returns whether `weather` is rainy enough to drive an NPC indoors, given
/// the NPC's occupation and whether the prospective destination is outdoors.
///
/// Farmers tolerate light rain so they can still work the fields; everyone
/// else (and farmers in heavy rain or storms) seeks shelter when the
/// destination is outdoors.
pub fn needs_shelter(weather: Weather, npc: &Npc, dest_is_outdoor: bool) -> bool {
    let dominated_by_rain = matches!(
        weather,
        Weather::LightRain | Weather::HeavyRain | Weather::Storm
    );
    if !dominated_by_rain {
        return false;
    }
    let is_farmer = npc.occupation.eq_ignore_ascii_case("farmer");
    if is_farmer {
        // Farmers tolerate light rain; they only seek shelter in heavier weather.
        !matches!(weather, Weather::LightRain) && dest_is_outdoor
    } else {
        dest_is_outdoor
    }
}

/// Computes where `npc` should be heading for this tick.
///
/// The decision is composed of three stacked rules, applied in this order:
///
/// 1. **Schedule lookup.** No active entry → [`ScheduleResolution::Stay`].
/// 2. **Cuaird rotation.** If the active entry is a cuaird visit, replace
///    the destination with one of the NPC's friend's homes, picked by
///    day-of-year so the round-robin is deterministic.
/// 3. **Weather shelter.** If [`needs_shelter`] fires for the resolved
///    destination, route the NPC home if home is indoor, else hold them in
///    place.
///
/// This function never mutates state. The caller is responsible for the rest
/// of `tick_schedules`: pathfinding, transit start, and event emission.
pub fn resolve_desired_location(
    npc: &Npc,
    cuaird_targets: &CuairdTargets,
    ctx: &TickContext<'_>,
) -> ScheduleResolution {
    let Some(mut desired) = npc.desired_location(ctx.current_hour, ctx.season, ctx.day_type) else {
        return ScheduleResolution::Stay;
    };

    // Cuaird override: rotate visiting location by day-of-year so the NPC
    // doesn't visit the same friend every evening.
    if let Some(entry) = npc.schedule_entry(ctx.current_hour, ctx.season, ctx.day_type)
        && entry.cuaird
    {
        let friends = cuaird_targets.get(npc.id);
        if !friends.is_empty() {
            let day_of_year = ctx.now.ordinal() as usize;
            desired = friends[day_of_year % friends.len()];
        }
    }

    // Weather shelter override.
    let dest_is_outdoor = ctx.graph.get(desired).map(|d| !d.indoor).unwrap_or(false);
    if needs_shelter(ctx.weather, npc, dest_is_outdoor) {
        // Try to fall back to home if it's indoor; otherwise the NPC stays put.
        if let Some(home) = npc.home {
            let home_is_indoor = ctx.graph.get(home).map(|d| d.indoor).unwrap_or(false);
            if home_is_indoor {
                desired = home;
            } else {
                return ScheduleResolution::Stay;
            }
        } else {
            return ScheduleResolution::Stay;
        }
    }

    ScheduleResolution::MoveTo { target: desired }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;

    use super::*;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::reactions::ReactionLog;
    use crate::types::{
        Intelligence, NpcState, Relationship, RelationshipKind, ScheduleEntry, ScheduleVariant,
        SeasonalSchedule,
    };

    fn make_npc(id: u32, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age: 30,
            occupation: "Test".to_string(),
            personality: "Test".to_string(),
            intelligence: Intelligence::default(),
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
            reaction_log: ReactionLog::default(),
            last_activity: None,
            is_ill: false,
            doom: None,
            banshee_heralded: false,
        }
    }

    /// Schedule: home (id=home) at night/morning, work (id=work) at hour 8..=17,
    /// home again in the evening — like a typical NPC.
    fn make_scheduled_npc(id: u32, home: u32, work: u32) -> Npc {
        let mut npc = make_npc(id, home);
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

    /// Two-location world. `id_a` is indoor or outdoor as `a_indoor` says,
    /// likewise `id_b`. Connected by a single edge.
    fn pair_graph(id_a: u32, a_indoor: bool, id_b: u32, b_indoor: bool) -> WorldGraph {
        let json = serde_json::json!({
            "locations": [
                {
                    "id": id_a,
                    "name": format!("Loc {}", id_a),
                    "description_template": "Test",
                    "indoor": a_indoor,
                    "public": true,
                    "connections": [
                        {"target": id_b, "path_description": "a path"}
                    ]
                },
                {
                    "id": id_b,
                    "name": format!("Loc {}", id_b),
                    "description_template": "Test",
                    "indoor": b_indoor,
                    "public": true,
                    "connections": [
                        {"target": id_a, "path_description": "a path"}
                    ]
                }
            ]
        })
        .to_string();
        WorldGraph::load_from_str(&json).unwrap()
    }

    #[test]
    fn stay_when_no_schedule() {
        let npc = make_npc(1, 5);
        let graph = pair_graph(5, false, 6, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::Clear,
            &graph,
        );
        let targets = CuairdTargets::default();
        assert_eq!(
            resolve_desired_location(&npc, &targets, &ctx),
            ScheduleResolution::Stay,
            "an NPC with no schedule should never move"
        );
    }

    #[test]
    fn move_to_workplace_during_work_hours() {
        // Outdoor home (id=10), outdoor work (id=20), 10am, clear weather.
        let npc = make_scheduled_npc(1, 10, 20);
        let graph = pair_graph(10, false, 20, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::Clear,
            &graph,
        );
        let targets = CuairdTargets::default();
        match resolve_desired_location(&npc, &targets, &ctx) {
            ScheduleResolution::MoveTo { target } => assert_eq!(target, LocationId(20)),
            other => panic!("expected MoveTo(work), got {:?}", other),
        }
    }

    #[test]
    fn cuaird_rotates_by_day_of_year() {
        // NPC with a cuaird-flagged evening entry visiting friends.
        let mut npc = make_npc(1, 10);
        npc.home = Some(LocationId(10));
        npc.schedule = Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![ScheduleEntry {
                    start_hour: 0,
                    end_hour: 23,
                    location: LocationId(10),
                    activity: "visiting".to_string(),
                    cuaird: true,
                }],
            }],
        });
        // Two strong-relationship friends with home locations 100 and 200.
        npc.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.9));
        npc.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Friend, 0.9));

        let mut friend_a = make_npc(2, 100);
        friend_a.home = Some(LocationId(100));
        let mut friend_b = make_npc(3, 200);
        friend_b.home = Some(LocationId(200));

        let mut npcs: HashMap<NpcId, Npc> = HashMap::new();
        npcs.insert(npc.id, npc.clone());
        npcs.insert(friend_a.id, friend_a);
        npcs.insert(friend_b.id, friend_b);
        let targets = collect_cuaird_targets(&npcs);

        // Build a connected graph so the orphan-location validator passes.
        // Weather is Clear in this test so the indoor flags don't matter,
        // but we make every node indoor for safety.
        let json = serde_json::json!({
            "locations": [
                {"id": 10, "name": "Home", "description_template": "x", "indoor": true, "public": true, "connections": [
                    {"target": 100, "path_description": "a path"},
                    {"target": 200, "path_description": "a path"}
                ]},
                {"id": 100, "name": "A's Home", "description_template": "x", "indoor": true, "public": true, "connections": [
                    {"target": 10, "path_description": "a path"}
                ]},
                {"id": 200, "name": "B's Home", "description_template": "x", "indoor": true, "public": true, "connections": [
                    {"target": 10, "path_description": "a path"}
                ]}
            ]
        }).to_string();
        let graph = WorldGraph::load_from_str(&json).unwrap();

        // Day 1 of year — picks friends[1 % 2] = friends[1].
        let day_one = Utc.with_ymd_and_hms(1820, 1, 1, 19, 0, 0).unwrap();
        let ctx_one = TickContext::new(
            day_one,
            Season::Winter,
            DayType::Weekday,
            Weather::Clear,
            &graph,
        );
        let r1 = resolve_desired_location(&npc, &targets, &ctx_one);

        // Day 2 of year — picks friends[2 % 2] = friends[0]; must differ from r1.
        let day_two = Utc.with_ymd_and_hms(1820, 1, 2, 19, 0, 0).unwrap();
        let ctx_two = TickContext::new(
            day_two,
            Season::Winter,
            DayType::Weekday,
            Weather::Clear,
            &graph,
        );
        let r2 = resolve_desired_location(&npc, &targets, &ctx_two);

        assert!(matches!(r1, ScheduleResolution::MoveTo { .. }));
        assert!(matches!(r2, ScheduleResolution::MoveTo { .. }));
        assert_ne!(r1, r2, "cuaird must rotate by day-of-year");
    }

    #[test]
    fn heavy_rain_diverts_to_indoor_home() {
        // Indoor home (id=10), outdoor work (id=20). Heavy rain at 10am.
        let mut npc = make_scheduled_npc(1, 10, 20);
        npc.occupation = "Shopkeeper".to_string();
        let graph = pair_graph(10, true, 20, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::HeavyRain,
            &graph,
        );
        let targets = CuairdTargets::default();
        match resolve_desired_location(&npc, &targets, &ctx) {
            ScheduleResolution::MoveTo { target } => {
                assert_eq!(target, LocationId(10), "should redirect to indoor home")
            }
            other => panic!("expected MoveTo(home), got {:?}", other),
        }
    }

    #[test]
    fn heavy_rain_with_outdoor_home_holds_in_place() {
        // Both home and work are outdoor. NPC has nowhere indoor → Stay.
        let mut npc = make_scheduled_npc(1, 10, 20);
        npc.occupation = "Shopkeeper".to_string();
        let graph = pair_graph(10, false, 20, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::HeavyRain,
            &graph,
        );
        let targets = CuairdTargets::default();
        assert_eq!(
            resolve_desired_location(&npc, &targets, &ctx),
            ScheduleResolution::Stay
        );
    }

    #[test]
    fn farmer_tolerates_light_rain_and_proceeds_to_outdoor_work() {
        let mut npc = make_scheduled_npc(1, 10, 20);
        npc.occupation = "Farmer".to_string();
        let graph = pair_graph(10, true, 20, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::LightRain,
            &graph,
        );
        let targets = CuairdTargets::default();
        match resolve_desired_location(&npc, &targets, &ctx) {
            ScheduleResolution::MoveTo { target } => assert_eq!(target, LocationId(20)),
            other => panic!(
                "farmer should still go to outdoor work in light rain, got {:?}",
                other
            ),
        }
    }

    #[test]
    fn farmer_seeks_shelter_in_heavy_rain() {
        let mut npc = make_scheduled_npc(1, 10, 20);
        npc.occupation = "Farmer".to_string();
        let graph = pair_graph(10, true, 20, false);
        let ctx = TickContext::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 10, 0, 0).unwrap(),
            Season::Summer,
            DayType::Weekday,
            Weather::HeavyRain,
            &graph,
        );
        let targets = CuairdTargets::default();
        match resolve_desired_location(&npc, &targets, &ctx) {
            ScheduleResolution::MoveTo { target } => assert_eq!(target, LocationId(10)),
            other => panic!("farmer should shelter in heavy rain, got {:?}", other),
        }
    }

    #[test]
    fn needs_shelter_only_in_rain() {
        let npc = make_npc(1, 0);
        // Clear / overcast / fog: never shelter.
        for w in [Weather::Clear, Weather::Overcast, Weather::Fog] {
            assert!(!needs_shelter(w, &npc, true));
            assert!(!needs_shelter(w, &npc, false));
        }
        // Rainy + outdoor destination: shelter.
        for w in [Weather::LightRain, Weather::HeavyRain, Weather::Storm] {
            assert!(needs_shelter(w, &npc, true));
            // Indoor destination: never shelter.
            assert!(!needs_shelter(w, &npc, false));
        }
    }

    #[test]
    fn collect_cuaird_targets_excludes_weak_relationships() {
        let mut a = make_npc(1, 0);
        // Strong friend → included.
        a.relationships
            .insert(NpcId(2), Relationship::new(RelationshipKind::Friend, 0.9));
        // Weak acquaintance → excluded (strength <= 0.3).
        a.relationships
            .insert(NpcId(3), Relationship::new(RelationshipKind::Neighbor, 0.2));

        let mut b = make_npc(2, 100);
        b.home = Some(LocationId(100));
        let mut c = make_npc(3, 200);
        c.home = Some(LocationId(200));

        let mut npcs = HashMap::new();
        npcs.insert(a.id, a.clone());
        npcs.insert(b.id, b);
        npcs.insert(c.id, c);

        let targets = collect_cuaird_targets(&npcs);
        let friends = targets.get(NpcId(1));
        assert_eq!(friends, &[LocationId(100)]);
    }
}
