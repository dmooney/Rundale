//! NPC schedule resolution — advancing NPC positions through their daily routines.
//!
//! Extracted from `NpcManager` so schedule logic and its tests live in one place.
//! `NpcManager::tick_schedules` is a thin wrapper around [`tick_schedules`].

use std::collections::HashMap;

use chrono::{DateTime, Datelike, Duration, Timelike, Utc};

use crate::types::NpcState;
use crate::{Npc, NpcId};
use parish_types::events::{EventBus, GameEvent};
use parish_types::{LocationId, Weather};
use parish_world::graph::WorldGraph;
use parish_world::time::{DayType, GameClock, Season};

/// An event produced by a schedule tick.
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
            } => format!("{} heading to {} ({}min)", self.npc_name, to_name, minutes),
            ScheduleEventKind::Arrived { location_name, .. } => {
                format!("{} arrived at {}", self.npc_name, location_name)
            }
        }
    }
}

/// Returns a cuaird override location, or `None` if this NPC is not on a cuaird this hour.
fn resolve_cuaird_location(
    npc: &Npc,
    current_hour: u8,
    season: Season,
    day_type: DayType,
    npcs: &HashMap<NpcId, Npc>,
    now: DateTime<Utc>,
) -> Option<LocationId> {
    let entry = npc.schedule_entry(current_hour, season, day_type)?;
    if !entry.cuaird {
        return None;
    }
    let friends: Vec<LocationId> = npc
        .relationships
        .iter()
        .filter(|(_, rel)| rel.strength > 0.3)
        .filter_map(|(friend_id, _)| npcs.get(friend_id).and_then(|f| f.home))
        .collect();
    if friends.is_empty() {
        return None;
    }
    let day_of_year = now.ordinal() as usize;
    Some(friends[day_of_year % friends.len()])
}

/// Returns `true` when the NPC's desired destination is outdoor and weather
/// forces them to seek shelter (or stay put if none found).
fn needs_weather_shelter(
    desired: LocationId,
    npc: &Npc,
    weather: Weather,
    graph: &WorldGraph,
) -> bool {
    let rainy = matches!(
        weather,
        Weather::LightRain | Weather::HeavyRain | Weather::Storm
    );
    if !rainy {
        return false;
    }
    let is_farmer = npc.occupation.to_ascii_lowercase().contains("farm");
    let dest_outdoor = graph.get(desired).map(|d| !d.indoor).unwrap_or(false);
    if is_farmer && matches!(weather, Weather::LightRain) {
        return false;
    }
    dest_outdoor
}

/// Advances NPC schedules based on the current game time.
///
/// For each NPC that is `Present` and whose schedule says they should be
/// somewhere else, starts transit. For NPCs that are `InTransit` and whose
/// arrival time has passed, completes the move.
///
/// Publishes `GameEvent::NpcDeparted` when transit starts and
/// `GameEvent::NpcArrived` when transit completes — the two events that
/// describe a real physical move. Cognitive-tier transitions do **not**
/// publish these; they are reserved for actual location changes.
///
/// Returns a list of structured schedule events describing what happened.
pub fn tick_schedules(
    npcs: &mut HashMap<NpcId, Npc>,
    clock: &GameClock,
    graph: &WorldGraph,
    weather: Weather,
    event_bus: &EventBus,
) -> Vec<ScheduleEvent> {
    let now = clock.now();
    let current_hour = now.hour() as u8;
    let season = clock.season();
    let day_type = clock.day_type();
    let mut events = Vec::new();
    let npc_ids: Vec<NpcId> = npcs.keys().copied().collect();

    for id in npc_ids {
        let Some(npc) = npcs.get(&id) else {
            continue;
        };

        match &npc.state {
            NpcState::Present => {
                let Some(mut desired) = npc.desired_location(current_hour, season, day_type) else {
                    continue;
                };

                if let Some(cuaird_loc) =
                    resolve_cuaird_location(npc, current_hour, season, day_type, npcs, now)
                {
                    desired = cuaird_loc;
                }

                if needs_weather_shelter(desired, npc, weather, graph) {
                    match npc.home {
                        Some(home) if graph.get(home).map(|d| d.indoor).unwrap_or(false) => {
                            desired = home;
                        }
                        _ => continue,
                    }
                }

                if desired != npc.location
                    && let Some(path) = graph.shortest_path(npc.location, desired)
                {
                    let travel_minutes = graph.path_travel_time(&path, 1.25);
                    let arrives_at = now + Duration::minutes(travel_minutes as i64);
                    let from = npc.location;
                    let npc_name = npc.name.clone();
                    let dest_name = graph
                        .get(desired)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "?".to_string());
                    // Capture the authored activity for this trip only
                    // when the trip is heading to the *scheduled*
                    // location — cuaird/weather-shelter reroutes mean
                    // the authored activity no longer matches the
                    // destination and should be suppressed.
                    let scheduled_activity = npc
                        .schedule_entry(current_hour, season, day_type)
                        .filter(|entry| {
                            entry.location == desired && !entry.activity.trim().is_empty()
                        })
                        .map(|entry| entry.activity.clone());
                    events.push(ScheduleEvent {
                        npc_id: id,
                        npc_name,
                        kind: ScheduleEventKind::Departed {
                            from,
                            to: desired,
                            to_name: dest_name,
                            minutes: travel_minutes,
                        },
                    });
                    event_bus.publish(GameEvent::NpcDeparted {
                        npc_id: id,
                        location: from,
                        to: desired,
                        timestamp: now,
                    });
                    tracing::debug!(
                        npc = %npc.name,
                        from = from.0,
                        to = desired.0,
                        minutes = travel_minutes,
                        "NPC starting transit"
                    );
                    let Some(npc_mut) = npcs.get_mut(&id) else {
                        continue;
                    };
                    npc_mut.set_state(NpcState::InTransit {
                        from,
                        to: desired,
                        arrives_at,
                        activity: scheduled_activity,
                    });
                }
            }
            NpcState::InTransit {
                to,
                arrives_at,
                activity,
                ..
            } => {
                if now >= *arrives_at {
                    let destination = *to;
                    let activity = activity.clone();
                    let dest_name = graph
                        .get(destination)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "?".to_string());
                    events.push(ScheduleEvent {
                        npc_id: id,
                        npc_name: npc.name.clone(),
                        kind: ScheduleEventKind::Arrived {
                            location: destination,
                            location_name: dest_name,
                        },
                    });
                    event_bus.publish(GameEvent::NpcArrived {
                        npc_id: id,
                        location: destination,
                        timestamp: now,
                    });
                    if let Some(activity) = activity {
                        event_bus.publish(GameEvent::NpcActivity {
                            npc_id: id,
                            location: destination,
                            activity,
                            timestamp: now,
                        });
                    }
                    tracing::debug!(npc = %npc.name, location = destination.0, "NPC arrived");
                    let Some(npc_mut) = npcs.get_mut(&id) else {
                        continue;
                    };
                    npc_mut.set_location_and_state(destination, NpcState::Present);
                }
            }
        }
    }

    // Authored activity is derived from clock + schedule rather than stored as
    // a mutable field. Observe it after movement settles so same-location
    // schedule changes (including A→B→A) advance lineage without bumping on
    // ordinary ticks whose activity is unchanged.
    for npc in npcs
        .values_mut()
        .filter(|npc| matches!(npc.state, NpcState::Present))
    {
        npc.observe_authored_activity_at(now);
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{load_test_graph, make_scheduled_npc, make_test_npc};
    use chrono::TimeZone;
    use chrono::Utc;
    use parish_world::time::GameClock;

    #[test]
    fn test_schedule_movement() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut npcs = HashMap::new();
        // NPC lives at crossroads (1), works at pub (2).
        npcs.insert(NpcId(1), make_scheduled_npc(1, 1, 2));

        // At 10am, NPC should want to be at work (pub, id 2).
        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();
        let before_departure = npcs[&NpcId(1)].grounding_revision();

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &EventBus::new());

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::InTransit { to, .. } if to == LocationId(2)),
            "NPC should be in transit to pub"
        );
        assert_ne!(
            npc.grounding_revision(),
            before_departure,
            "Present → InTransit must invalidate asynchronous grounding"
        );
    }

    #[test]
    fn test_schedule_arrival() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_scheduled_npc(1, 1, 2));

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        // Start transit.
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &EventBus::new());
        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::InTransit { .. }
        ));
        let in_transit_revision = npcs[&NpcId(1)].grounding_revision();

        // Advance past arrival.
        clock.advance(30);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &EventBus::new());

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::Present),
            "NPC should have arrived"
        );
        assert_eq!(npc.location, LocationId(2), "NPC should be at pub");
        assert_ne!(
            npc.grounding_revision(),
            in_transit_revision,
            "InTransit → Present arrival must invalidate asynchronous grounding"
        );
    }

    #[test]
    fn test_npc_stays_put_when_at_desired_location() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut npcs = HashMap::new();
        let mut npc = make_scheduled_npc(1, 1, 2);
        npc.set_location(LocationId(2)); // Already at work.
        npcs.insert(NpcId(1), npc);

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &EventBus::new());

        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::Present
        ));
    }

    #[test]
    fn same_location_activity_aba_advances_grounding_revision_twice() {
        use crate::types::{ScheduleEntry, ScheduleVariant, SeasonalSchedule};

        let mut npc = make_test_npc(1, 1);
        npc.set_schedule(Some(SeasonalSchedule {
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
        let mut npcs = HashMap::from([(npc.id, npc)]);
        let graph = WorldGraph::new();
        let event_bus = EventBus::new();
        let mut clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap());
        clock.pause();

        assert!(tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus).is_empty());
        let revision_a = npcs[&NpcId(1)].grounding_revision();
        let fingerprint_a = npcs[&NpcId(1)]
            .observed_activity_fingerprint()
            .expect("initial production schedule tick must synchronize activity");

        // Repeating the same production tick is a no-op for grounding.
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        assert_eq!(npcs[&NpcId(1)].grounding_revision(), revision_a);

        clock.advance(60);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        let revision_b = npcs[&NpcId(1)].grounding_revision();
        assert!(revision_b > revision_a, "A→B must advance lineage");

        // Repeating B also remains a no-op.
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        assert_eq!(npcs[&NpcId(1)].grounding_revision(), revision_b);

        clock.advance(60);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        let npc = &npcs[&NpcId(1)];
        assert!(
            npc.grounding_revision() > revision_b,
            "B→A must advance lineage again"
        );
        assert_ne!(
            npc.observed_activity_fingerprint(),
            Some(fingerprint_a),
            "separate authored A intervals must have distinct fingerprints"
        );
        assert_eq!(npc.location, LocationId(1));
        assert!(matches!(npc.state, NpcState::Present));
    }

    #[test]
    fn authored_interval_fingerprint_is_stable_within_slot_but_changes_next_day() {
        use crate::types::{ScheduleEntry, ScheduleVariant, SeasonalSchedule};

        let mut npc = make_test_npc(1, 1);
        npc.set_schedule(Some(SeasonalSchedule {
            variants: vec![ScheduleVariant {
                season: None,
                day_type: None,
                entries: vec![ScheduleEntry {
                    start_hour: 10,
                    end_hour: 12,
                    location: LocationId(1),
                    activity: "mending nets".to_string(),
                    cuaird: false,
                }],
            }],
        }));
        let mut npcs = HashMap::from([(npc.id, npc)]);
        let graph = WorldGraph::new();
        let event_bus = EventBus::new();
        let mut clock = GameClock::new(Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap());
        clock.pause();

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        let first_fingerprint = npcs[&NpcId(1)]
            .observed_activity_fingerprint()
            .expect("production tick should observe the active interval");
        let first_revision = npcs[&NpcId(1)].grounding_revision();

        clock.advance(30);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        assert_eq!(
            npcs[&NpcId(1)].observed_activity_fingerprint(),
            Some(first_fingerprint),
            "ordinary inference latency inside one active interval must remain valid"
        );
        assert_eq!(npcs[&NpcId(1)].grounding_revision(), first_revision);

        clock.advance(23 * 60 + 30);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &event_bus);
        assert_ne!(
            npcs[&NpcId(1)].observed_activity_fingerprint(),
            Some(first_fingerprint),
            "the same daily slot on the next game date is a new interval instance"
        );
        assert!(
            npcs[&NpcId(1)].grounding_revision() > first_revision,
            "crossing to the next day's interval must advance lineage"
        );
    }

    #[test]
    fn test_npc_rain_override() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        // NPC at home (Darcy's Pub, id=2, indoor), scheduled to work at Crossroads (id=1, outdoor).
        let mut npc = make_scheduled_npc(1, 2, 1);
        npc.home = Some(LocationId(2));
        npc.occupation = "Shopkeeper".to_string();

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), npc);

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        tick_schedules(
            &mut npcs,
            &clock,
            &graph,
            Weather::HeavyRain,
            &EventBus::new(),
        );

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::Present),
            "NPC should stay put in heavy rain"
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

        // Farmer at home (pub, id=2), scheduled to work at Murphy's Farm (id=9, outdoor).
        let mut npc = make_scheduled_npc(1, 2, 9);
        npc.home = Some(LocationId(2));
        npc.occupation = "Farmer".to_string();

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), npc);

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        tick_schedules(
            &mut npcs,
            &clock,
            &graph,
            Weather::LightRain,
            &EventBus::new(),
        );

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::InTransit { .. }),
            "Farmer should tolerate light rain, got {:?}",
            npc.state
        );
    }

    #[test]
    fn test_schedule_event_debug_string() {
        let departed = ScheduleEvent {
            npc_id: NpcId(1),
            npc_name: "Brigid".to_string(),
            kind: ScheduleEventKind::Departed {
                from: LocationId(1),
                to: LocationId(2),
                to_name: "The Pub".to_string(),
                minutes: 5,
            },
        };
        assert!(departed.debug_string().contains("Brigid"));
        assert!(departed.debug_string().contains("The Pub"));
        assert!(departed.debug_string().contains("5min"));

        let arrived = ScheduleEvent {
            npc_id: NpcId(1),
            npc_name: "Brigid".to_string(),
            kind: ScheduleEventKind::Arrived {
                location: LocationId(2),
                location_name: "The Pub".to_string(),
            },
        };
        assert!(arrived.debug_string().contains("Brigid"));
        assert!(arrived.debug_string().contains("The Pub"));
    }

    // Ensure make_test_npc with no schedule never starts transit.
    #[test]
    fn test_no_schedule_stays_put() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_test_npc(1, 1));

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear, &EventBus::new());

        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::Present
        ));
    }
}
