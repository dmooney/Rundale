//! NPC daily schedule system.
//!
//! Each NPC has a weekday and weekend schedule that determines
//! where they should be at any given time. Seasonal overrides
//! allow for different behavior during different times of year.

use serde::{Deserialize, Serialize};

use crate::world::LocationId;
use crate::world::time::{GameClock, Season};

/// A single entry in an NPC's daily schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// Hour this activity starts (0-23).
    pub start_hour: u8,
    /// Hour this activity ends (0-23, exclusive).
    pub end_hour: u8,
    /// Where the NPC should be during this time.
    pub location: LocationId,
    /// What the NPC is doing.
    pub activity: String,
}

/// An NPC's full daily schedule with weekday, weekend, and seasonal variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailySchedule {
    /// Schedule entries for weekdays (Monday-Friday).
    pub weekday: Vec<ScheduleEntry>,
    /// Schedule entries for weekends (Saturday-Sunday).
    pub weekend: Vec<ScheduleEntry>,
    /// Seasonal override schedules (replaces weekday/weekend when active).
    #[serde(default)]
    pub overrides: std::collections::HashMap<SeasonKey, Vec<ScheduleEntry>>,
}

/// Season key for schedule overrides, wrapping the Season enum for HashMap use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeasonKey {
    /// Spring override.
    Spring,
    /// Summer override.
    Summer,
    /// Autumn override.
    Autumn,
    /// Winter override.
    Winter,
}

impl From<Season> for SeasonKey {
    fn from(s: Season) -> Self {
        match s {
            Season::Spring => SeasonKey::Spring,
            Season::Summer => SeasonKey::Summer,
            Season::Autumn => SeasonKey::Autumn,
            Season::Winter => SeasonKey::Winter,
        }
    }
}

impl DailySchedule {
    /// Returns the location and activity for the given clock time.
    ///
    /// Checks seasonal overrides first, then weekday/weekend schedule.
    /// Returns `None` if no schedule entry covers the current hour.
    pub fn current_entry(&self, clock: &GameClock) -> Option<&ScheduleEntry> {
        let now = clock.now();
        let hour = chrono::Timelike::hour(&now) as u8;
        let is_weekend = matches!(
            chrono::Datelike::weekday(&now),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        );
        let season: SeasonKey = clock.season().into();

        // Check seasonal overrides first
        if let Some(override_entries) = self.overrides.get(&season)
            && let Some(entry) = find_entry(override_entries, hour)
        {
            return Some(entry);
        }

        // Fall back to weekday/weekend
        let entries = if is_weekend {
            &self.weekend
        } else {
            &self.weekday
        };
        find_entry(entries, hour)
    }

    /// Returns the desired location for the given clock time.
    pub fn desired_location(&self, clock: &GameClock) -> Option<LocationId> {
        self.current_entry(clock).map(|e| e.location)
    }
}

/// Finds the schedule entry that covers the given hour.
fn find_entry(entries: &[ScheduleEntry], hour: u8) -> Option<&ScheduleEntry> {
    entries.iter().find(|e| {
        if e.start_hour <= e.end_hour {
            hour >= e.start_hour && hour < e.end_hour
        } else {
            // Wraps midnight (e.g., 22-6)
            hour >= e.start_hour || hour < e.end_hour
        }
    })
}

/// The current state of an NPC's movement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NpcState {
    /// NPC is at a location.
    Present(LocationId),
    /// NPC is traveling between locations.
    InTransit {
        /// Where they came from.
        from: LocationId,
        /// Where they're going.
        to: LocationId,
        /// When they'll arrive (game time).
        arrives_at: chrono::DateTime<chrono::Utc>,
    },
}

impl NpcState {
    /// Returns the location the NPC is at, or None if in transit.
    pub fn location(&self) -> Option<LocationId> {
        match self {
            NpcState::Present(loc) => Some(*loc),
            NpcState::InTransit { .. } => None,
        }
    }

    /// Returns true if the NPC is at the given location.
    pub fn is_at(&self, location: LocationId) -> bool {
        matches!(self, NpcState::Present(loc) if *loc == location)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn make_schedule() -> DailySchedule {
        DailySchedule {
            weekday: vec![
                ScheduleEntry {
                    start_hour: 6,
                    end_hour: 9,
                    location: LocationId(1),
                    activity: "Morning routine at home".to_string(),
                },
                ScheduleEntry {
                    start_hour: 9,
                    end_hour: 17,
                    location: LocationId(2),
                    activity: "Working at the pub".to_string(),
                },
                ScheduleEntry {
                    start_hour: 17,
                    end_hour: 23,
                    location: LocationId(2),
                    activity: "Evening at the pub".to_string(),
                },
            ],
            weekend: vec![
                ScheduleEntry {
                    start_hour: 8,
                    end_hour: 12,
                    location: LocationId(1),
                    activity: "Lie-in and breakfast".to_string(),
                },
                ScheduleEntry {
                    start_hour: 12,
                    end_hour: 23,
                    location: LocationId(2),
                    activity: "Weekend at the pub".to_string(),
                },
            ],
            overrides: std::collections::HashMap::new(),
        }
    }

    fn clock_at(year: i32, month: u32, day: u32, hour: u32) -> GameClock {
        let time = Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap();
        let mut clock = GameClock::new(time);
        clock.pause(); // Freeze time for predictable tests
        clock
    }

    #[test]
    fn test_schedule_weekday_morning() {
        let schedule = make_schedule();
        // 2026-03-20 is a Friday
        let clock = clock_at(2026, 3, 20, 7);
        let entry = schedule.current_entry(&clock).unwrap();
        assert_eq!(entry.location, LocationId(1));
        assert!(entry.activity.contains("Morning"));
    }

    #[test]
    fn test_schedule_weekday_work() {
        let schedule = make_schedule();
        let clock = clock_at(2026, 3, 20, 12);
        let entry = schedule.current_entry(&clock).unwrap();
        assert_eq!(entry.location, LocationId(2));
        assert!(entry.activity.contains("Working"));
    }

    #[test]
    fn test_schedule_weekend() {
        let schedule = make_schedule();
        // 2026-03-21 is a Saturday
        let clock = clock_at(2026, 3, 21, 10);
        let entry = schedule.current_entry(&clock).unwrap();
        assert_eq!(entry.location, LocationId(1));
        assert!(entry.activity.contains("Lie-in"));
    }

    #[test]
    fn test_schedule_no_entry() {
        let schedule = make_schedule();
        // 3 AM - no entry covers this
        let clock = clock_at(2026, 3, 20, 3);
        assert!(schedule.current_entry(&clock).is_none());
    }

    #[test]
    fn test_schedule_desired_location() {
        let schedule = make_schedule();
        let clock = clock_at(2026, 3, 20, 12);
        assert_eq!(schedule.desired_location(&clock), Some(LocationId(2)));
    }

    #[test]
    fn test_schedule_seasonal_override() {
        let mut schedule = make_schedule();
        schedule.overrides.insert(
            SeasonKey::Spring,
            vec![ScheduleEntry {
                start_hour: 6,
                end_hour: 23,
                location: LocationId(5),
                activity: "Spring farming".to_string(),
            }],
        );
        // March is Spring
        let clock = clock_at(2026, 3, 20, 12);
        let entry = schedule.current_entry(&clock).unwrap();
        assert_eq!(entry.location, LocationId(5));
        assert!(entry.activity.contains("Spring"));
    }

    #[test]
    fn test_npc_state_present() {
        let state = NpcState::Present(LocationId(1));
        assert_eq!(state.location(), Some(LocationId(1)));
        assert!(state.is_at(LocationId(1)));
        assert!(!state.is_at(LocationId(2)));
    }

    #[test]
    fn test_npc_state_in_transit() {
        let state = NpcState::InTransit {
            from: LocationId(1),
            to: LocationId(2),
            arrives_at: Utc::now(),
        };
        assert_eq!(state.location(), None);
        assert!(!state.is_at(LocationId(1)));
    }

    #[test]
    fn test_schedule_entry_wrapping_midnight() {
        let schedule = DailySchedule {
            weekday: vec![ScheduleEntry {
                start_hour: 22,
                end_hour: 6,
                location: LocationId(1),
                activity: "Sleeping".to_string(),
            }],
            weekend: vec![],
            overrides: std::collections::HashMap::new(),
        };
        // Friday at 23:00 should match
        let clock = clock_at(2026, 3, 20, 23);
        let entry = schedule.current_entry(&clock).unwrap();
        assert!(entry.activity.contains("Sleeping"));

        // Friday at 3:00 should also match
        let clock = clock_at(2026, 3, 20, 3);
        let entry = schedule.current_entry(&clock).unwrap();
        assert!(entry.activity.contains("Sleeping"));
    }

    #[test]
    fn test_schedule_serialize_deserialize() {
        let schedule = make_schedule();
        let json = serde_json::to_string(&schedule).unwrap();
        let deser: DailySchedule = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.weekday.len(), 3);
        assert_eq!(deser.weekend.len(), 2);
    }

    #[test]
    fn test_season_key_from_season() {
        assert_eq!(SeasonKey::from(Season::Spring), SeasonKey::Spring);
        assert_eq!(SeasonKey::from(Season::Winter), SeasonKey::Winter);
    }
}
