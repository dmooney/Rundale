//! Banshee heralds — keening cries that foreshadow an NPC's death.
//!
//! A staple of Irish folklore: the *bean sídhe* wails on the night before
//! a death in the household. In Parish, this bridges the Tier 4 rules
//! engine (which rolls random `Death` events) and the player experience
//! (which would otherwise see NPCs blink out with no warning).
//!
//! When Tier 4 rolls a `Death`, [`crate::manager::NpcManager`] schedules the
//! doom a game-day ahead instead of removing the NPC immediately. The
//! banshee tick — [`NpcManager::tick_banshee`](crate::manager::NpcManager::tick_banshee)
//! — then emits two kinds of report:
//!
//! 1. **A wail** once, during the night preceding the doom, written to the
//!    world text log so the player hears it wherever they are.
//! 2. **The death itself**, when the doom timestamp passes, removing the
//!    NPC and logging a short epitaph.
//!
//! The whole system is gated behind the default-on `banshee` feature flag
//! — disabling it reverts to the older behaviour of instant removal.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Duration, Timelike, Utc};

use crate::{Npc, NpcId};
use parish_types::LocationId;
use parish_world::events::{EventBus, GameEvent};
use parish_world::graph::WorldGraph;
use parish_world::time::GameClock;

/// How long before the doom timestamp the banshee becomes eligible to cry.
///
/// Set wide enough that a Tier 4 `Death` rolled at ~2pm will still fall
/// inside the window once night comes, but narrow enough that the cry
/// feels connected to the coming dawn rather than a random night weeks
/// out.
pub const DOOM_HERALD_WINDOW_HOURS: i64 = 12;

/// How far ahead of "now" a fresh doom is scheduled when Tier 4 rolls a death.
///
/// Far enough to guarantee a night falls between the roll and the doom,
/// so the banshee has something to foreshadow.
pub const DOOM_LEAD_TIME_HOURS: i64 = 18;

/// A single outcome produced by a banshee tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BansheeEvent {
    /// The banshee was heard heralding a coming death.
    Heard {
        /// Which NPC is fated.
        target: NpcId,
        /// Display name of the fated NPC.
        target_name: String,
        /// NPC's home location (where the cry is said to rise from), if known.
        home: Option<LocationId>,
        /// Human-readable name of the home location, if any.
        home_name: Option<String>,
        /// Whether the player is at the same location as the home.
        near_player: bool,
    },
    /// The NPC's doom arrived — they have passed away.
    Died {
        /// Which NPC died.
        target: NpcId,
        /// Display name.
        target_name: String,
    },
}

/// Accumulated outcome of one banshee tick.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BansheeReport {
    /// Wails heralded this tick.
    pub wails: Vec<BansheeEvent>,
    /// Deaths finalised this tick.
    pub deaths: Vec<BansheeEvent>,
}

impl BansheeReport {
    /// Returns `true` if nothing happened this tick.
    pub fn is_empty(&self) -> bool {
        self.wails.is_empty() && self.deaths.is_empty()
    }
}

/// Returns `true` if `now` falls in the nighttime herald window before `doom`.
///
/// The banshee only cries between dusk and dawn — roughly hours 20..=23 and
/// 0..=5 game-time — and only if the doom is less than [`DOOM_HERALD_WINDOW_HOURS`]
/// ahead. A doom scheduled for tomorrow afternoon lights up tonight's window;
/// a doom already in the past falls through to the death path instead.
pub fn is_herald_window(now: DateTime<Utc>, doom: DateTime<Utc>) -> bool {
    if doom <= now {
        return false;
    }
    if doom - now > Duration::hours(DOOM_HERALD_WINDOW_HOURS) {
        return false;
    }
    let hour = now.hour();
    // Dusk/night/early-morning — when the old stories say the veil is thin.
    (20..=23).contains(&hour) || (0..=5).contains(&hour)
}

/// Default descriptive line for a wail, built from a [`BansheeEvent::Heard`].
///
/// Two voicings — one if the player is at the fated NPC's home, one if they
/// hear it on the wind from elsewhere. The line is deliberately spare so it
/// reads as folklore rather than a system notification.
pub fn herald_line(event: &BansheeEvent) -> Option<String> {
    let BansheeEvent::Heard {
        home_name,
        near_player,
        ..
    } = event
    else {
        return None;
    };

    let line = if *near_player {
        "A thin, high wailing climbs from just beyond the thatch \u{2014} \
         a sound like wind drawn through reeds, but shaped like grief. \
         The old ones would say it is the banshee, crying a name the night already knows."
            .to_string()
    } else if let Some(home) = home_name {
        format!(
            "Out across the parish, a keening rises \u{2014} thin and impossibly high. \
             It drifts in from the direction of {}. \
             Someone beside you mutters, quietly: \u{201c}Someone's for the morning.\u{201d}",
            home
        )
    } else {
        "Out across the parish, a keening rises \u{2014} thin and impossibly high. \
         Someone beside you mutters, quietly: \u{201c}Someone's for the morning.\u{201d}"
            .to_string()
    };
    Some(line)
}

/// Default descriptive line for a death finalisation.
pub fn epitaph_line(event: &BansheeEvent) -> Option<String> {
    let BansheeEvent::Died { target_name, .. } = event else {
        return None;
    };
    Some(format!(
        "Word travels before the sun is fully up: {} did not see the morning. \
         The banshee had the right of it.",
        target_name
    ))
}

/// Ring-buffer capacity for recent Tier 4 life-event descriptions.
///
/// Defined in `tier4` (the canonical owner of life events) and re-exported
/// here for use in this module.
pub(crate) use crate::tier4::RING_BUFFER_CAPACITY;

/// Runs one banshee tick: heralds imminent deaths and finalises doomed NPCs.
///
/// Scans `npcs` for any NPC with a [`crate::Npc::doom`] timestamp set:
///
/// - If `now >= doom`: removes the NPC, writes an epitaph to `world_text_log`,
///   publishes a [`GameEvent::LifeEvent`], and pushes a description into
///   `recent_events_ring`.
/// - Otherwise, if `now` falls in the night herald window before `doom` and
///   the NPC has not yet been heralded: emits the banshee wail, writes it to
///   `world_text_log`, and sets [`crate::Npc::banshee_heralded`].
///
/// `player_loc` is used only to pick the near/far wail voicing.
pub fn tick(
    npcs: &mut HashMap<NpcId, Npc>,
    recent_events_ring: &mut VecDeque<String>,
    clock: &GameClock,
    graph: &WorldGraph,
    world_text_log: &mut Vec<String>,
    event_bus: &EventBus,
    player_loc: LocationId,
) -> BansheeReport {
    let now = clock.now();
    let mut report = BansheeReport::default();

    // Collect ids first to avoid simultaneous iteration + mutation.
    let doomed_ids: Vec<NpcId> = npcs
        .iter()
        .filter_map(|(id, npc)| npc.doom.map(|_| *id))
        .collect();

    for id in doomed_ids {
        // Extract all fields in one lookup so we avoid a second get() for the herald path.
        let (doom, already_heralded, name, home, current_location) = {
            let Some(npc) = npcs.get(&id) else { continue };
            let Some(d) = npc.doom else { continue };
            (
                d,
                npc.banshee_heralded,
                npc.name.clone(),
                npc.home,
                npc.location,
            )
        };

        if now >= doom {
            npcs.remove(&id);
            let desc = format!("{name} has passed away.");
            world_text_log.push(format!(
                "Word travels before the sun is fully up: {name} did not see the morning. \
                 The banshee had the right of it."
            ));
            event_bus.publish(GameEvent::LifeEvent {
                npc_id: id,
                description: desc,
                timestamp: now,
            });
            if recent_events_ring.len() >= RING_BUFFER_CAPACITY {
                recent_events_ring.pop_front();
            }
            recent_events_ring.push_back(format!("{name} has passed away."));
            report.deaths.push(BansheeEvent::Died {
                target: id,
                target_name: name,
            });
            continue;
        }

        if already_heralded || !is_herald_window(now, doom) {
            continue;
        }

        // Wail rises from the NPC's home if set, otherwise from their current location.
        let wail_loc = home.unwrap_or(current_location);
        let home_name = graph.get(wail_loc).map(|d| d.name.clone());
        let near_player = wail_loc == player_loc;

        let event = BansheeEvent::Heard {
            target: id,
            target_name: name,
            home,
            home_name,
            near_player,
        };
        if let Some(line) = herald_line(&event) {
            world_text_log.push(line);
        }
        if let Some(npc) = npcs.get_mut(&id) {
            npc.banshee_heralded = true;
        }
        report.wails.push(event);
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t(h: u32, m: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 6, 15, h, m, 0).unwrap()
    }

    #[test]
    fn herald_window_open_at_midnight_before_afternoon_doom() {
        // Doom tomorrow at 14:00, check at 00:00 tonight (14 hours ahead — out of range).
        // Check at 02:00 instead (12 hours ahead — inside window).
        let now = t(2, 0);
        let doom = now + Duration::hours(12);
        assert!(is_herald_window(now, doom));
    }

    #[test]
    fn herald_window_closed_during_daytime() {
        // Midday check — night hours only.
        let now = t(13, 0);
        let doom = now + Duration::hours(6);
        assert!(!is_herald_window(now, doom));
    }

    #[test]
    fn herald_window_closed_when_doom_too_far_out() {
        // Night time, but doom is 30 hours away.
        let now = t(23, 0);
        let doom = now + Duration::hours(30);
        assert!(!is_herald_window(now, doom));
    }

    #[test]
    fn herald_window_closed_after_doom_passes() {
        let now = t(23, 0);
        let doom = now - Duration::hours(1);
        assert!(!is_herald_window(now, doom));
    }

    #[test]
    fn herald_window_open_at_21_to_21() {
        let now = t(21, 30);
        let doom = now + Duration::hours(8);
        assert!(is_herald_window(now, doom));
    }

    #[test]
    fn herald_line_near_player_uses_close_voicing() {
        let evt = BansheeEvent::Heard {
            target: NpcId(7),
            target_name: "Brigid".to_string(),
            home: Some(LocationId(3)),
            home_name: Some("the shepherd's cottage".to_string()),
            near_player: true,
        };
        let line = herald_line(&evt).unwrap();
        assert!(line.contains("just beyond the thatch"));
        assert!(!line.contains("shepherd's cottage"));
    }

    #[test]
    fn herald_line_far_names_home() {
        let evt = BansheeEvent::Heard {
            target: NpcId(7),
            target_name: "Brigid".to_string(),
            home: Some(LocationId(3)),
            home_name: Some("the shepherd's cottage".to_string()),
            near_player: false,
        };
        let line = herald_line(&evt).unwrap();
        assert!(line.contains("shepherd's cottage"));
        assert!(line.contains("Someone's for the morning"));
    }

    #[test]
    fn epitaph_line_names_the_dead() {
        let evt = BansheeEvent::Died {
            target: NpcId(9),
            target_name: "Seamus Flynn".to_string(),
        };
        let line = epitaph_line(&evt).unwrap();
        assert!(line.contains("Seamus Flynn"));
        assert!(line.contains("banshee"));
    }

    #[test]
    fn is_empty_is_true_on_default() {
        assert!(BansheeReport::default().is_empty());
    }

    // ── Integration tests (tick free fn) ─────────────────────────────────────

    use crate::test_helpers::{make_mourning_world, make_test_npc};
    use parish_types::NpcId;
    use std::collections::{HashMap, VecDeque};

    fn run_tick(
        npcs: &mut HashMap<NpcId, crate::Npc>,
        world: &mut parish_world::WorldState,
    ) -> BansheeReport {
        let mut ring = VecDeque::new();
        tick(
            npcs,
            &mut ring,
            &world.clock,
            &world.graph,
            &mut world.text_log,
            &world.event_bus,
            world.player_location,
        )
    }

    #[test]
    fn banshee_herald_fires_at_night_with_near_doom() {
        let mut npcs = HashMap::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        npcs.insert(NpcId(42), npc);

        let mut world = make_mourning_world();
        let report = run_tick(&mut npcs, &mut world);

        assert_eq!(report.wails.len(), 1, "one wail expected");
        assert_eq!(report.deaths.len(), 0, "no death yet");
        assert!(
            world
                .text_log
                .iter()
                .any(|l| l.contains("keening") || l.contains("banshee")),
            "wail line should appear in text log"
        );
        assert!(npcs[&NpcId(42)].banshee_heralded, "herald flag must be set");
    }

    #[test]
    fn banshee_wail_is_emitted_only_once_per_doom() {
        let mut npcs = HashMap::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        npcs.insert(NpcId(42), npc);

        let mut world = make_mourning_world();
        let r1 = run_tick(&mut npcs, &mut world);
        let r2 = run_tick(&mut npcs, &mut world);

        assert_eq!(r1.wails.len(), 1);
        assert_eq!(r2.wails.len(), 0, "second tick must not re-wail");
    }

    #[test]
    fn banshee_finalises_death_once_doom_passes() {
        let mut npcs = HashMap::new();
        let mut npc = make_test_npc(42, 2);
        // Doom 1 hour in the past.
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 15, 21, 0, 0).unwrap());
        npc.banshee_heralded = true;
        npcs.insert(NpcId(42), npc);

        let mut world = make_mourning_world();
        let report = run_tick(&mut npcs, &mut world);

        assert_eq!(report.deaths.len(), 1);
        assert_eq!(report.wails.len(), 0);
        assert!(!npcs.contains_key(&NpcId(42)), "NPC must be removed");
        assert!(
            world
                .text_log
                .iter()
                .any(|l| l.contains("did not see the morning")),
            "epitaph line should appear"
        );
    }

    #[test]
    fn banshee_does_not_fire_during_daytime() {
        let mut npcs = HashMap::new();
        let mut npc = make_test_npc(42, 2);
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        npcs.insert(NpcId(42), npc);

        let mut world = make_mourning_world();
        // Override clock to 14:00 — outside night window.
        world.clock = parish_world::time::GameClock::new(
            Utc.with_ymd_and_hms(1820, 6, 15, 14, 0, 0).unwrap(),
        );

        let report = run_tick(&mut npcs, &mut world);
        assert!(
            report.is_empty(),
            "daytime should produce neither wail nor death"
        );
        assert!(world.text_log.is_empty());
    }

    #[test]
    fn banshee_herald_near_player_uses_close_voicing() {
        let mut npcs = HashMap::new();
        let mut npc = make_test_npc(42, 0); // NPC lives at player's location.
        npc.home = Some(LocationId(0));
        npc.doom = Some(Utc.with_ymd_and_hms(1820, 6, 16, 6, 0, 0).unwrap());
        npcs.insert(NpcId(42), npc);

        let mut world = make_mourning_world();
        let report = run_tick(&mut npcs, &mut world);

        assert_eq!(report.wails.len(), 1);
        if let BansheeEvent::Heard { near_player, .. } = &report.wails[0] {
            assert!(*near_player, "player shares location with the doomed NPC");
        } else {
            panic!("expected a Heard event");
        }
    }
}
