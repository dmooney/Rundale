//! NPC schedule resolution — advancing NPC positions through their daily routines.
//!
//! Extracted from `NpcManager` so schedule logic and its tests live in one place.
//! `NpcManager::tick_schedules` is a thin wrapper around [`tick_schedules`].

use std::collections::HashMap;

use chrono::{Datelike, Duration, Timelike};

use crate::types::NpcState;
use crate::{Npc, NpcId};
use parish_types::{LocationId, Weather};
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

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

/// Advances NPC schedules based on the current game time.
///
/// For each NPC that is `Present` and whose schedule says they should be
/// somewhere else, starts transit. For NPCs that are `InTransit` and whose
/// arrival time has passed, completes the move.
///
/// Returns a list of structured schedule events describing what happened.
pub fn tick_schedules(
    npcs: &mut HashMap<NpcId, Npc>,
    clock: &GameClock,
    graph: &WorldGraph,
    weather: Weather,
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

                // Cuaird override: only compute friend locations when this NPC actually has a
                // cuaird slot active — avoids an O(relationships) scan for every NPC every tick.
                if let Some(entry) = npc.schedule_entry(current_hour, season, day_type)
                    && entry.cuaird
                {
                    let r: &HashMap<NpcId, Npc> = npcs;
                    let friends: Vec<LocationId> = npc
                        .relationships
                        .iter()
                        .filter(|(_, rel)| rel.strength > 0.3)
                        .filter_map(|(friend_id, _)| r.get(friend_id).and_then(|f| f.home))
                        .collect();
                    if !friends.is_empty() {
                        let day_of_year = now.ordinal() as usize;
                        desired = friends[day_of_year % friends.len()];
                    }
                }

                // Weather shelter override: seek indoor locations in bad weather.
                let rainy = matches!(
                    weather,
                    Weather::LightRain | Weather::HeavyRain | Weather::Storm
                );
                if rainy {
                    // Consistent with tier4.rs which uses contains("farm") for occupation checks.
                    let is_farmer = npc.occupation.to_ascii_lowercase().contains("farm");
                    let dest_outdoor = graph.get(desired).map(|d| !d.indoor).unwrap_or(false);
                    let needs_shelter = if is_farmer {
                        // Farmers tolerate light rain.
                        !matches!(weather, Weather::LightRain) && dest_outdoor
                    } else {
                        dest_outdoor
                    };
                    if needs_shelter {
                        match npc.home {
                            Some(home) if graph.get(home).map(|d| d.indoor).unwrap_or(false) => {
                                desired = home;
                            }
                            _ => continue, // No indoor refuge — stay put.
                        }
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
                    npc_mut.state = NpcState::InTransit {
                        from,
                        to: desired,
                        arrives_at,
                    };
                }
            }
            NpcState::InTransit { to, arrives_at, .. } => {
                if now >= *arrives_at {
                    let destination = *to;
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
                    tracing::debug!(npc = %npc.name, location = destination.0, "NPC arrived");
                    let Some(npc_mut) = npcs.get_mut(&id) else {
                        continue;
                    };
                    npc_mut.location = destination;
                    npc_mut.state = NpcState::Present;
                }
            }
        }
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

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear);

        let npc = npcs.get(&NpcId(1)).unwrap();
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

        let mut npcs = HashMap::new();
        npcs.insert(NpcId(1), make_scheduled_npc(1, 1, 2));

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        // Start transit.
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear);
        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::InTransit { .. }
        ));

        // Advance past arrival.
        clock.advance(30);
        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear);

        let npc = npcs.get(&NpcId(1)).unwrap();
        assert!(
            matches!(npc.state, NpcState::Present),
            "NPC should have arrived"
        );
        assert_eq!(npc.location, LocationId(2), "NPC should be at pub");
    }

    #[test]
    fn test_npc_stays_put_when_at_desired_location() {
        let graph = match load_test_graph() {
            Some(g) => g,
            None => return,
        };

        let mut npcs = HashMap::new();
        let mut npc = make_scheduled_npc(1, 1, 2);
        npc.location = LocationId(2); // Already at work.
        npcs.insert(NpcId(1), npc);

        let start = Utc.with_ymd_and_hms(1820, 3, 20, 10, 0, 0).unwrap();
        let mut clock = GameClock::new(start);
        clock.pause();

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear);

        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::Present
        ));
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

        tick_schedules(&mut npcs, &clock, &graph, Weather::HeavyRain);

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

        tick_schedules(&mut npcs, &clock, &graph, Weather::LightRain);

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

        tick_schedules(&mut npcs, &clock, &graph, Weather::Clear);

        assert!(matches!(
            npcs.get(&NpcId(1)).unwrap().state,
            NpcState::Present
        ));
    }
}
