//! En-route encounter system.
//!
//! Generates random encounters during travel between locations.
//! Probability is ~20% per traversal, influenced by time of day.

use super::time::TimeOfDay;
use crate::npc::NpcId;

/// An encounter event that occurs during travel.
#[derive(Debug, Clone)]
pub struct EncounterEvent {
    /// The NPC involved, if any.
    pub npc_id: Option<NpcId>,
    /// A prose description of the encounter.
    pub description: String,
}

/// Checks whether an encounter occurs during travel.
///
/// Base probability is 20%. Modified by time of day:
/// - Dawn/Morning: slightly higher (more people about)
/// - Night/Midnight: lower (fewer people out)
///
/// The `roll` parameter is a value in `0.0..1.0` for testability
/// (in production, pass `rand::random::<f64>()`).
pub fn check_encounter(time_of_day: TimeOfDay, roll: f64) -> Option<EncounterEvent> {
    let threshold = match time_of_day {
        TimeOfDay::Dawn | TimeOfDay::Morning => 0.25,
        TimeOfDay::Midday | TimeOfDay::Afternoon => 0.20,
        TimeOfDay::Dusk => 0.15,
        TimeOfDay::Night => 0.10,
        TimeOfDay::Midnight => 0.05,
    };

    if roll >= threshold {
        return None;
    }

    // Generate flavor text based on time of day
    let description = match time_of_day {
        TimeOfDay::Dawn => "You pass an early riser walking their dog along the road.",
        TimeOfDay::Morning => "A farmer nods to you from the far side of a gate as you pass.",
        TimeOfDay::Midday => "You spot someone cycling past on the road ahead.",
        TimeOfDay::Afternoon => "A car slows as it passes you. The driver gives a wave.",
        TimeOfDay::Dusk => {
            "A figure walks ahead of you in the fading light, then turns off down a lane."
        }
        TimeOfDay::Night => {
            "You hear footsteps on the road behind you, but when you turn, no one is there."
        }
        TimeOfDay::Midnight => "An owl hoots from a nearby tree, breaking the silence.",
    };

    Some(EncounterEvent {
        npc_id: None,
        description: description.to_string(),
    })
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
}
