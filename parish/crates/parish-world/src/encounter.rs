//! En-route encounter system.
//!
//! Generates random encounters during travel between locations.
//! Probability is ~20% per traversal, influenced by time of day.
//!
//! Encounter flavour text can come from hardcoded defaults (legacy) or from
//! a mod's [`EncounterTable`](crate::game_mod::EncounterTable) data.

use std::collections::HashMap;

use parish_config::EncounterConfig;
use parish_types::{NpcId, TimeOfDay};

/// Encounter text table keyed by time-of-day label.
///
/// Loaded from a mod's `encounters.json` file. Used by
/// [`check_encounter_with_table`] to provide mod-specific encounter text.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EncounterTable {
    /// Encounter flavour text keyed by time-of-day (e.g. "morning", "night").
    #[serde(flatten)]
    pub by_time: HashMap<String, String>,
}

/// An encounter event that occurs during travel.
#[derive(Debug, Clone)]
pub struct EncounterEvent {
    /// The NPC involved, if any.
    pub npc_id: Option<NpcId>,
    /// A prose description of the encounter.
    pub description: String,
}

/// Checks whether an encounter occurs during travel using default config.
///
/// Base probability is 20%. Modified by time of day:
/// - Dawn/Morning: slightly higher (more people about)
/// - Night/Midnight: lower (fewer people out)
///
/// The `roll` parameter is a value in `0.0..1.0` for testability
/// (in production, pass `rand::random::<f64>()`).
pub fn check_encounter(time_of_day: TimeOfDay, roll: f64) -> Option<EncounterEvent> {
    check_encounter_with_config(time_of_day, roll, &EncounterConfig::default())
}

/// Returns the encounter probability threshold for the given time of day.
fn encounter_threshold(time_of_day: TimeOfDay, config: &EncounterConfig) -> f64 {
    match time_of_day {
        TimeOfDay::Dawn => config.dawn,
        TimeOfDay::Morning => config.morning,
        TimeOfDay::Midday => config.midday,
        TimeOfDay::Afternoon => config.afternoon,
        TimeOfDay::Dusk => config.dusk,
        TimeOfDay::Night => config.night,
        TimeOfDay::Midnight => config.midnight,
    }
}

/// Checks whether an encounter occurs during travel using the given config.
///
/// The config provides per-time-of-day probability thresholds. A random `roll`
/// in `0.0..1.0` below the threshold triggers an encounter.
pub fn check_encounter_with_config(
    time_of_day: TimeOfDay,
    roll: f64,
    config: &EncounterConfig,
) -> Option<EncounterEvent> {
    if roll >= encounter_threshold(time_of_day, config) {
        return None;
    }

    Some(EncounterEvent {
        npc_id: None,
        description: fallback_description(time_of_day).to_string(),
    })
}

/// Checks whether an encounter occurs during travel, using a mod-provided
/// [`EncounterTable`] for flavour text instead of hardcoded strings.
///
/// Falls back to a generic description if the table has no entry for the
/// current time of day.
pub fn check_encounter_with_table(
    time_of_day: TimeOfDay,
    roll: f64,
    table: &EncounterTable,
) -> Option<EncounterEvent> {
    if roll >= encounter_threshold(time_of_day, &EncounterConfig::default()) {
        return None;
    }

    let key = format!("{}", time_of_day).to_lowercase();
    let description = table
        .by_time
        .get(&key)
        .cloned()
        .unwrap_or_else(|| fallback_description(time_of_day).to_string());

    Some(EncounterEvent {
        npc_id: None,
        description,
    })
}

/// Returns the period-appropriate fallback description for the given time of day.
///
/// All strings here must pass the anachronism check — no references to technology
/// post-dating the 1820s Irish setting (no bicycles, motorcars, telephones, etc.).
/// The companion test `test_fallback_descriptions_no_anachronisms` enforces this.
fn fallback_description(time_of_day: TimeOfDay) -> &'static str {
    match time_of_day {
        TimeOfDay::Dawn => {
            "A lone figure trudges along the road in the early morning grey, bundle on their back."
        }
        TimeOfDay::Morning => "A farmer nods to you from the far side of a gate as you pass.",
        TimeOfDay::Midday => "You spot someone on the road ahead, driving a cart at a lazy pace.",
        TimeOfDay::Afternoon => "A cart slows as it passes you. The driver gives a wave.",
        TimeOfDay::Dusk => {
            "A figure walks ahead of you in the fading light, then turns off down a lane."
        }
        TimeOfDay::Night => {
            "You hear footsteps on the road behind you, but when you turn, no one is there."
        }
        TimeOfDay::Midnight => "An owl hoots from a nearby tree, breaking the silence.",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encounter_below_threshold_triggers() {
        // Morning threshold is 0.25, roll of 0.1 should trigger
        let result = check_encounter(TimeOfDay::Morning, 0.1);
        assert!(result.is_some());
    }

    #[test]
    fn test_encounter_above_threshold_none() {
        // Morning threshold is 0.25, roll of 0.5 should not trigger
        let result = check_encounter(TimeOfDay::Morning, 0.5);
        assert!(result.is_none());
    }

    #[test]
    fn test_encounter_midnight_low_chance() {
        // Midnight threshold is 0.05
        let result = check_encounter(TimeOfDay::Midnight, 0.03);
        assert!(result.is_some());

        let result = check_encounter(TimeOfDay::Midnight, 0.1);
        assert!(result.is_none());
    }

    #[test]
    fn test_encounter_has_description() {
        let event = check_encounter(TimeOfDay::Dawn, 0.0).unwrap();
        assert!(!event.description.is_empty());
        assert!(event.npc_id.is_none()); // Phase 2: no specific NPC yet
    }

    #[test]
    fn test_encounter_probability_distribution() {
        // Run 1000 trials at morning (threshold 0.25)
        // With uniform random rolls from 0..1, ~25% should trigger
        let mut hits = 0;
        for i in 0..1000 {
            let roll = i as f64 / 1000.0;
            if check_encounter(TimeOfDay::Morning, roll).is_some() {
                hits += 1;
            }
        }
        // Should be 250 (exactly 25% with uniform spacing)
        assert_eq!(hits, 250);
    }

    #[test]
    fn test_encounter_all_times_of_day() {
        // All times should produce an encounter with roll 0.0
        let times = [
            TimeOfDay::Dawn,
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ];
        for time in &times {
            let event = check_encounter(*time, 0.0).unwrap();
            assert!(
                !event.description.is_empty(),
                "No description for {:?}",
                time
            );
        }
    }

    #[test]
    fn test_encounter_with_table_uses_mod_text() {
        use std::collections::HashMap;
        let mut by_time = HashMap::new();
        by_time.insert("morning".to_string(), "A shepherd passes.".to_string());
        let table = EncounterTable { by_time };

        let event = check_encounter_with_table(TimeOfDay::Morning, 0.1, &table).unwrap();
        assert_eq!(event.description, "A shepherd passes.");
        assert!(event.npc_id.is_none());
    }

    #[test]
    fn test_encounter_with_table_fallback() {
        use std::collections::HashMap;
        let table = EncounterTable {
            by_time: HashMap::new(),
        };

        // No entry for "dawn", should use fallback
        let event = check_encounter_with_table(TimeOfDay::Dawn, 0.1, &table).unwrap();
        assert!(!event.description.is_empty());
    }

    #[test]
    fn test_encounter_with_table_respects_threshold() {
        use std::collections::HashMap;
        let table = EncounterTable {
            by_time: HashMap::new(),
        };
        // Roll above threshold should return None
        assert!(check_encounter_with_table(TimeOfDay::Morning, 0.5, &table).is_none());
    }

    #[test]
    fn test_encounter_with_config_custom_thresholds() {
        let config = EncounterConfig {
            dawn: 0.50,
            morning: 0.50,
            midday: 0.50,
            afternoon: 0.50,
            dusk: 0.50,
            night: 0.50,
            midnight: 0.50,
        };
        // Roll of 0.4 is below 0.50 — should trigger for all times
        assert!(check_encounter_with_config(TimeOfDay::Midnight, 0.4, &config).is_some());
        assert!(check_encounter_with_config(TimeOfDay::Night, 0.4, &config).is_some());
        // Roll of 0.6 is above 0.50 — should not trigger
        assert!(check_encounter_with_config(TimeOfDay::Dawn, 0.6, &config).is_none());
    }

    #[test]
    fn test_encounter_with_config_zero_thresholds() {
        let config = EncounterConfig {
            dawn: 0.0,
            morning: 0.0,
            midday: 0.0,
            afternoon: 0.0,
            dusk: 0.0,
            night: 0.0,
            midnight: 0.0,
        };
        // No encounters should ever trigger with zero thresholds
        assert!(check_encounter_with_config(TimeOfDay::Morning, 0.0, &config).is_none());
    }

    #[test]
    fn test_encounter_with_config_delegates_from_default() {
        // check_encounter should produce the same result as check_encounter_with_config
        // with default config
        let config = EncounterConfig::default();
        for roll in [0.0, 0.05, 0.10, 0.15, 0.20, 0.25, 0.50, 0.99] {
            let times = [
                TimeOfDay::Dawn,
                TimeOfDay::Morning,
                TimeOfDay::Midday,
                TimeOfDay::Afternoon,
                TimeOfDay::Dusk,
                TimeOfDay::Night,
                TimeOfDay::Midnight,
            ];
            for time in &times {
                let a = check_encounter(*time, roll).is_some();
                let b = check_encounter_with_config(*time, roll, &config).is_some();
                assert_eq!(a, b, "Mismatch for {:?} at roll {}", time, roll);
            }
        }
    }

    #[test]
    fn test_encounter_at_exact_threshold() {
        // At exactly the threshold, should NOT trigger (>= check)
        let result = check_encounter(TimeOfDay::Midday, 0.20);
        assert!(result.is_none());
    }

    #[test]
    fn test_encounter_just_below_threshold() {
        let result = check_encounter(TimeOfDay::Midday, 0.19);
        assert!(result.is_some());
    }

    /// Every fallback description must be free of anachronistic terms.
    ///
    /// Extend `FORBIDDEN_WORDS` when a new term is added to `mods/rundale/anachronisms.json`
    /// that could plausibly appear in encounter prose.  Adding a new time-of-day arm to
    /// `fallback_description` without updating the word list will NOT cause this test to fail
    /// silently — the helper covers every `TimeOfDay` variant exhaustively.
    #[test]
    fn test_fallback_descriptions_no_anachronisms() {
        /// Terms that post-date the 1820s Irish setting and must never appear in engine
        /// encounter text.  Uses whole-word matching so "cart" does not false-positive
        /// on "car".
        const FORBIDDEN_WORDS: &[&str] = &[
            "car",
            "bicycle",
            "cycling",
            "bike",
            "automobile",
            "engine",
            "motor",
            "phone",
            "radio",
            "tractor",
            "train",
            "railway",
            "railroad",
            "locomotive",
            "electric",
            "electricity",
            "television",
            "computer",
            "internet",
            "smartphone",
        ];

        let times = [
            TimeOfDay::Dawn,
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ];

        for time in &times {
            let description = fallback_description(*time);
            // Lowercase first and bind it so the reference lives long enough.
            let lowered = description.to_lowercase();
            let word_set: std::collections::HashSet<&str> = lowered
                .split(|c: char| !c.is_alphanumeric())
                .filter(|s| !s.is_empty())
                .collect();

            for &forbidden in FORBIDDEN_WORDS {
                assert!(
                    !word_set.contains(forbidden),
                    "Anachronism '{}' found in {:?} encounter description: {:?}",
                    forbidden,
                    time,
                    description,
                );
            }
        }
    }
}
