//! En-route encounter system.
//!
//! Generates random encounters during travel between locations.
//! Probability is ~20% per traversal, influenced by time of day.
//!
//! Encounter flavour text can come from hardcoded defaults (legacy) or from
//! a mod's `EncounterTable` data.

use parish_config::EncounterConfig;
use parish_types::TimeOfDay;

/// Checks whether an encounter occurs during travel using default config.
///
/// Base probability is 20%. Modified by time of day:
/// - Dawn/Morning: slightly higher (more people about)
/// - Night/Midnight: lower (fewer people out)
///
/// The `roll` parameter is a value in `0.0..1.0` for testability
/// (in production, pass `rand::random::<f64>()`).
pub fn check_encounter(time_of_day: TimeOfDay, roll: f64) -> Option<String> {
    if roll >= encounter_threshold(time_of_day, &EncounterConfig::default()) {
        return None;
    }
    Some(fallback_description(time_of_day).to_string())
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
        let desc = check_encounter(TimeOfDay::Dawn, 0.0).unwrap();
        assert!(!desc.is_empty());
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
            let desc = check_encounter(*time, 0.0).unwrap();
            assert!(!desc.is_empty(), "No description for {:?}", time);
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
