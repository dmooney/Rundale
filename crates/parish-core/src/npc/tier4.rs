//! Tier 4 CPU-only rules engine.
//!
//! Deterministic rules applied once per game-season. No LLM calls.

use rand::Rng;

use crate::npc::{Npc, NpcId};
use crate::world::time::Season;

/// Game-minutes between Tier 4 ticks (~1 game season = 90 days).
pub const TIER4_TICK_GAME_MINUTES: i64 = 129_600; // 90 * 1440

/// An event produced by the Tier 4 rules engine.
#[derive(Debug, Clone)]
pub enum Tier4Event {
    /// NPC has fallen ill.
    Illness {
        /// The affected NPC.
        npc_id: NpcId,
    },
    /// NPC has recovered from illness.
    Recovery {
        /// The recovered NPC.
        npc_id: NpcId,
    },
    /// NPC mood shifts due to seasonal change.
    SeasonalShift {
        /// The affected NPC.
        npc_id: NpcId,
        /// New mood after the shift.
        new_mood: String,
    },
    /// A new relationship forms between two NPCs.
    RelationshipFormed {
        /// NPC who initiates the relationship.
        from: NpcId,
        /// NPC who is the target of the relationship.
        to: NpcId,
    },
    /// NPC has died (very rare, age-gated).
    Death {
        /// The deceased NPC.
        npc_id: NpcId,
    },
    /// NPC mood changes due to general circumstances.
    MoodShift {
        /// The affected NPC.
        npc_id: NpcId,
        /// New mood string.
        new_mood: String,
    },
}

/// Applies probabilistic rules to a set of NPCs for one game-season.
///
/// Rules:
/// - Illness: 2% per season (4% in winter)
/// - Recovery: 80% if currently ill (mood contains "ill" or "sick")
/// - Death: 0.125% per season for NPCs with age > 60
/// - Relationship formed: 5% between NPCs at the same location
/// - Mood shift: seasonal bias (brighter moods in summer, somber in winter)
pub fn tick_tier4(npcs: &[&Npc], season: Season) -> Vec<Tier4Event> {
    let mut rng = rand::thread_rng();
    tick_tier4_with_rng(npcs, season, &mut rng)
}

/// Internal implementation with injectable RNG for testing.
fn tick_tier4_with_rng<R: Rng>(npcs: &[&Npc], season: Season, rng: &mut R) -> Vec<Tier4Event> {
    let mut events = Vec::new();

    for npc in npcs {
        let mood_lower = npc.mood.to_lowercase();
        let is_ill = mood_lower.contains("ill") || mood_lower.contains("sick");

        // Recovery check (before illness, so an ill NPC can recover)
        if is_ill {
            if rng.r#gen::<f32>() < 0.80 {
                events.push(Tier4Event::Recovery { npc_id: npc.id });
            }
            continue; // ill NPCs skip other checks this tick
        }

        // Illness check
        let illness_chance = match season {
            Season::Winter => 0.04,
            _ => 0.02,
        };
        if rng.r#gen::<f32>() < illness_chance {
            events.push(Tier4Event::Illness { npc_id: npc.id });
            continue; // newly ill, skip further checks
        }

        // Death check (age > 60 only)
        if npc.age > 60 && rng.r#gen::<f32>() < 0.00125 {
            events.push(Tier4Event::Death { npc_id: npc.id });
            continue;
        }

        // Seasonal mood shift (10% chance)
        if rng.r#gen::<f32>() < 0.10 {
            let mood = seasonal_mood_bias(season).to_string();
            events.push(Tier4Event::SeasonalShift {
                npc_id: npc.id,
                new_mood: mood,
            });
        }
    }

    // Relationship formation: 5% between NPCs at same location
    for i in 0..npcs.len() {
        for j in (i + 1)..npcs.len() {
            if npcs[i].location == npcs[j].location && rng.r#gen::<f32>() < 0.05 {
                events.push(Tier4Event::RelationshipFormed {
                    from: npcs[i].id,
                    to: npcs[j].id,
                });
            }
        }
    }

    events
}

/// Applies Tier 4 events to NPC state.
///
/// - `Illness` / `SeasonalShift` / `MoodShift`: updates the NPC's mood.
/// - `Recovery`: sets mood to "recovering".
/// - `Death` / `RelationshipFormed`: logged but no NPC field mutation
///   (callers handle removal and relationship creation).
pub fn apply_tier4_events(npcs: &mut [Npc], events: &[Tier4Event]) {
    for event in events {
        match event {
            Tier4Event::Illness { npc_id } => {
                if let Some(npc) = npcs.iter_mut().find(|n| n.id == *npc_id) {
                    npc.mood = "ill".to_string();
                }
            }
            Tier4Event::Recovery { npc_id } => {
                if let Some(npc) = npcs.iter_mut().find(|n| n.id == *npc_id) {
                    npc.mood = "recovering".to_string();
                }
            }
            Tier4Event::SeasonalShift { npc_id, new_mood } => {
                if let Some(npc) = npcs.iter_mut().find(|n| n.id == *npc_id) {
                    npc.mood.clone_from(new_mood);
                }
            }
            Tier4Event::MoodShift { npc_id, new_mood } => {
                if let Some(npc) = npcs.iter_mut().find(|n| n.id == *npc_id) {
                    npc.mood.clone_from(new_mood);
                }
            }
            Tier4Event::Death { .. } | Tier4Event::RelationshipFormed { .. } => {
                // Callers handle these (NPC removal, relationship creation)
            }
        }
    }
}

/// Returns a typical mood for the given season.
///
/// Used as a seasonal bias for mood shifts:
/// - Spring: "hopeful"
/// - Summer: "cheerful"
/// - Autumn: "reflective"
/// - Winter: "somber"
pub fn seasonal_mood_bias(season: Season) -> &'static str {
    match season {
        Season::Spring => "hopeful",
        Season::Summer => "cheerful",
        Season::Autumn => "reflective",
        Season::Winter => "somber",
    }
}

/// Returns a typical activity for the given season.
///
/// Reflects rural Irish agricultural life in 1820:
/// - Spring: "planting crops"
/// - Summer: "tending fields"
/// - Autumn: "harvesting"
/// - Winter: "mending tools by the fire"
pub fn seasonal_activity(season: Season) -> &'static str {
    match season {
        Season::Spring => "planting crops",
        Season::Summer => "tending fields",
        Season::Autumn => "harvesting",
        Season::Winter => "mending tools by the fire",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::npc::memory::{LongTermMemory, ShortTermMemory};
    use crate::npc::types::NpcState;
    use crate::world::LocationId;
    use rand::rngs::mock::StepRng;
    use std::collections::HashMap;

    fn make_npc(id: u32, mood: &str, age: u8, location: u32) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a test NPC".to_string(),
            age,
            occupation: "Test".to_string(),
            personality: "Test".to_string(),
            location: LocationId(location),
            mood: mood.to_string(),
            home: None,
            workplace: None,
            schedule: None,
            relationships: HashMap::new(),
            memory: ShortTermMemory::new(),
            long_term_memory: LongTermMemory::new(),
            knowledge: Vec::new(),
            state: NpcState::Present,
        }
    }

    #[test]
    fn test_seasonal_mood_bias() {
        assert_eq!(seasonal_mood_bias(Season::Spring), "hopeful");
        assert_eq!(seasonal_mood_bias(Season::Summer), "cheerful");
        assert_eq!(seasonal_mood_bias(Season::Autumn), "reflective");
        assert_eq!(seasonal_mood_bias(Season::Winter), "somber");
    }

    #[test]
    fn test_seasonal_activity() {
        assert_eq!(seasonal_activity(Season::Spring), "planting crops");
        assert_eq!(seasonal_activity(Season::Summer), "tending fields");
        assert_eq!(seasonal_activity(Season::Autumn), "harvesting");
        assert_eq!(
            seasonal_activity(Season::Winter),
            "mending tools by the fire"
        );
    }

    #[test]
    fn test_tick_tier4_empty() {
        let npcs: Vec<&Npc> = vec![];
        let events = tick_tier4(&npcs, Season::Spring);
        assert!(events.is_empty());
    }

    #[test]
    fn test_recovery_for_ill_npc() {
        let npc = make_npc(1, "feeling ill", 30, 1);
        let npcs: Vec<&Npc> = vec![&npc];
        // StepRng(0, 1) generates 0.0 on first f32 call, which is < 0.80
        let mut rng = StepRng::new(0, 1);
        let events = tick_tier4_with_rng(&npcs, Season::Summer, &mut rng);

        let has_recovery = events
            .iter()
            .any(|e| matches!(e, Tier4Event::Recovery { npc_id } if *npc_id == NpcId(1)));
        assert!(has_recovery, "Ill NPC should recover with low RNG");
    }

    #[test]
    fn test_illness_with_low_rng() {
        let npc = make_npc(1, "calm", 30, 1);
        let npcs: Vec<&Npc> = vec![&npc];
        // StepRng(0, 1) generates 0.0 which is < 0.02 illness threshold
        let mut rng = StepRng::new(0, 1);
        let events = tick_tier4_with_rng(&npcs, Season::Summer, &mut rng);

        let has_illness = events
            .iter()
            .any(|e| matches!(e, Tier4Event::Illness { npc_id } if *npc_id == NpcId(1)));
        assert!(has_illness, "Healthy NPC should get ill with 0.0 RNG roll");
    }

    #[test]
    fn test_no_illness_with_high_rng() {
        let npc = make_npc(1, "calm", 30, 1);
        let npcs: Vec<&Npc> = vec![&npc];
        // StepRng with high initial value to generate high f32
        let mut rng = StepRng::new(u64::MAX / 2, 0);
        let events = tick_tier4_with_rng(&npcs, Season::Summer, &mut rng);

        let has_illness = events
            .iter()
            .any(|e| matches!(e, Tier4Event::Illness { .. }));
        assert!(!has_illness, "Should not get ill with high RNG");
    }

    #[test]
    fn test_apply_tier4_illness() {
        let mut npcs = vec![make_npc(1, "calm", 30, 1)];
        let events = vec![Tier4Event::Illness { npc_id: NpcId(1) }];
        apply_tier4_events(&mut npcs, &events);
        assert_eq!(npcs[0].mood, "ill");
    }

    #[test]
    fn test_apply_tier4_recovery() {
        let mut npcs = vec![make_npc(1, "ill", 30, 1)];
        let events = vec![Tier4Event::Recovery { npc_id: NpcId(1) }];
        apply_tier4_events(&mut npcs, &events);
        assert_eq!(npcs[0].mood, "recovering");
    }

    #[test]
    fn test_apply_tier4_seasonal_shift() {
        let mut npcs = vec![make_npc(1, "calm", 30, 1)];
        let events = vec![Tier4Event::SeasonalShift {
            npc_id: NpcId(1),
            new_mood: "cheerful".to_string(),
        }];
        apply_tier4_events(&mut npcs, &events);
        assert_eq!(npcs[0].mood, "cheerful");
    }

    #[test]
    fn test_apply_tier4_mood_shift() {
        let mut npcs = vec![make_npc(1, "calm", 30, 1)];
        let events = vec![Tier4Event::MoodShift {
            npc_id: NpcId(1),
            new_mood: "anxious".to_string(),
        }];
        apply_tier4_events(&mut npcs, &events);
        assert_eq!(npcs[0].mood, "anxious");
    }

    #[test]
    fn test_apply_tier4_unknown_npc() {
        let mut npcs = vec![make_npc(1, "calm", 30, 1)];
        let events = vec![Tier4Event::Illness { npc_id: NpcId(99) }];
        // Should not panic
        apply_tier4_events(&mut npcs, &events);
        assert_eq!(npcs[0].mood, "calm");
    }

    #[test]
    fn test_tier4_constants() {
        assert_eq!(TIER4_TICK_GAME_MINUTES, 129_600);
        assert_eq!(TIER4_TICK_GAME_MINUTES, 90 * 1440);
    }

    #[test]
    fn test_relationship_formed_same_location() {
        let npc1 = make_npc(1, "calm", 30, 5);
        let npc2 = make_npc(2, "calm", 30, 5); // same location
        let npcs: Vec<&Npc> = vec![&npc1, &npc2];
        // StepRng(0,1) generates 0.0 for all rolls
        let mut rng = StepRng::new(0, 1);
        let events = tick_tier4_with_rng(&npcs, Season::Summer, &mut rng);

        let has_rel = events.iter().any(|e| {
            matches!(e, Tier4Event::RelationshipFormed { from, to }
                if *from == NpcId(1) && *to == NpcId(2))
        });
        assert!(
            has_rel,
            "NPCs at same location should form relationship with low RNG"
        );
    }
}
