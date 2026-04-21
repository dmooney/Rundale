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

use chrono::{DateTime, Duration, Timelike, Utc};

use crate::NpcId;
use parish_types::LocationId;

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
}
