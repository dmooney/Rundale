//! Tier 4 CPU-only rules engine for far-away NPCs.
//!
//! Produces life events (illness, death, birth, trade, seasonal shifts)
//! using probabilistic rules — no LLM calls. Runs once per in-game
//! season (~30-45 real minutes at Normal speed).

use chrono::{DateTime, NaiveDate, Utc};
use rand::Rng;

use crate::types::RelationshipKind;
use crate::{Npc, NpcId};
use parish_types::LocationId;
use parish_world::time::{Festival, Season};

/// A life event produced by the Tier 4 rules engine.
#[derive(Debug, Clone)]
pub enum Tier4Event {
    /// A child is born to two NPCs.
    Birth {
        /// The parent NPC ids.
        parent_ids: (NpcId, NpcId),
    },
    /// An NPC has died (natural causes).
    Death {
        /// The deceased NPC's id.
        npc_id: NpcId,
    },
    /// A trade was completed between two NPCs.
    TradeCompleted {
        /// The buying NPC.
        buyer: NpcId,
        /// The selling NPC.
        seller: NpcId,
    },
    /// An NPC's schedule changed due to the season.
    SeasonalShift {
        /// The affected NPC.
        npc_id: NpcId,
        /// Description of the new schedule.
        new_schedule_desc: String,
    },
    /// An NPC fell ill.
    Illness {
        /// The ill NPC.
        npc_id: NpcId,
    },
    /// An NPC recovered from illness.
    Recovery {
        /// The recovered NPC.
        npc_id: NpcId,
    },
    /// A festival was detected during this tick.
    FestivalDetected {
        /// Which festival.
        festival: Festival,
    },
    /// Relationship boosted during a festival.
    FestivalBond {
        /// First NPC in the bond.
        npc_a: NpcId,
        /// Second NPC in the bond.
        npc_b: NpcId,
        /// The festival that brought them together.
        festival: Festival,
    },
}

/// Probability constants for Tier 4 life events.
mod probabilities {
    /// Chance of any NPC falling ill per season.
    pub const ILLNESS_RATE: f64 = 0.02;
    /// Chance of an ill NPC recovering per season.
    pub const RECOVERY_RATE: f64 = 0.80;
    /// Base death rate per season (0.5% per year / 4 seasons).
    pub const DEATH_RATE_BASE: f64 = 0.00125;
    /// Death rate per season for NPCs aged 60+.
    pub const DEATH_RATE_ELDERLY: f64 = 0.02;
    /// Death rate per season for NPCs aged 75+.
    pub const DEATH_RATE_VERY_OLD: f64 = 0.05;
    /// Birth rate per eligible married couple per season.
    pub const BIRTH_RATE: f64 = 0.05;
    /// Trade rate per merchant NPC per season.
    pub const TRADE_RATE: f64 = 0.10;
}

/// Returns the death probability for an NPC based on age.
fn death_rate_for_age(age: u8) -> f64 {
    if age > 75 {
        probabilities::DEATH_RATE_VERY_OLD
    } else if age > 60 {
        probabilities::DEATH_RATE_ELDERLY
    } else {
        probabilities::DEATH_RATE_BASE
    }
}

/// Returns true if the occupation indicates a merchant/trader.
fn is_merchant(occupation: &str) -> bool {
    let occ = occupation.to_lowercase();
    occ.contains("shop") || occ.contains("trade") || occ.contains("merchant")
}

/// Returns seasonal schedule override description for an occupation, if applicable.
pub fn seasonal_schedule_description(occupation: &str, season: Season) -> Option<String> {
    let occ = occupation.to_lowercase();
    if occ.contains("farm") {
        match season {
            Season::Summer => Some("Working longer hours: 5am to 9pm".to_string()),
            Season::Winter => Some("Shorter winter hours: 8am to 4pm".to_string()),
            _ => None,
        }
    } else if occ.contains("teach") || occ.contains("school") {
        match season {
            Season::Summer => Some("No school in summer — staying home".to_string()),
            _ => None,
        }
    } else if occ.contains("publican") || occ.contains("pub") || occ.contains("innkeeper") {
        match season {
            Season::Winter => {
                Some("Winter hours: opening at 11am, closing at midnight".to_string())
            }
            _ => None,
        }
    } else {
        None
    }
}

/// Checks if a festival falls on any date in the given range [from, to).
///
/// Returns the first festival found in the date range, if any.
pub fn check_festival_in_range(from: DateTime<Utc>, to: DateTime<Utc>) -> Option<Festival> {
    let start_date = from.date_naive();
    let end_date = to.date_naive();

    let mut date = start_date;
    while date <= end_date {
        if let Some(festival) = Festival::check(date) {
            return Some(festival);
        }
        match date.succ_opt() {
            Some(next) => date = next,
            None => break,
        }
    }
    None
}

/// Returns a human-readable description for a festival.
pub fn festival_description(festival: Festival) -> &'static str {
    match festival {
        Festival::Imbolc => {
            "The community gathers for Imbolc, marking the first stirrings of spring."
        }
        Festival::Bealtaine => {
            "Bonfires light the hills for Bealtaine — summer has arrived with celebration and merriment."
        }
        Festival::Lughnasa => "The harvest fair of Lughnasa brings trading, games, and feasting.",
        Festival::Samhain => {
            "A solemn mood falls over the parish for Samhain — the boundary between worlds grows thin."
        }
    }
}

/// Runs a Tier 4 tick: deterministic/random state transitions with no LLM.
///
/// Called once per in-game season (~30-45 real minutes).
/// Must run on `tokio::task::spawn_blocking` to avoid blocking the async runtime.
///
/// The `game_date` is the current game date used for festival detection.
/// The `prev_game_date` is the game date of the previous tier 4 tick (if any),
/// used to determine what date range to check for festivals.
pub fn tick_tier4(
    npcs: &mut [&mut Npc],
    season: Season,
    game_date: NaiveDate,
    rng: &mut impl Rng,
) -> Vec<Tier4Event> {
    let mut events = Vec::new();

    // 1. Check for festival on this date
    if let Some(festival) = Festival::check(game_date) {
        events.push(Tier4Event::FestivalDetected { festival });

        // Boost relationships between NPCs at the same location
        let location_groups = group_by_location(npcs);
        for npc_ids in location_groups.values() {
            for i in 0..npc_ids.len() {
                for j in (i + 1)..npc_ids.len() {
                    events.push(Tier4Event::FestivalBond {
                        npc_a: npc_ids[i],
                        npc_b: npc_ids[j],
                        festival,
                    });
                }
            }
        }
    }

    // 2. Process each NPC for life events
    // Collect ids of NPCs that die this tick to skip them in later processing
    let mut dead_ids = std::collections::HashSet::new();

    for npc in npcs.iter() {
        let id = npc.id;

        // Recovery (must check before illness to avoid same-tick illness+recovery)
        if npc.is_ill && rng.random_bool(probabilities::RECOVERY_RATE.min(1.0)) {
            events.push(Tier4Event::Recovery { npc_id: id });
            continue; // Skip illness check if recovering
        }

        // Death (age-scaled)
        let death_rate = death_rate_for_age(npc.age);
        if rng.random_bool(death_rate.min(1.0)) {
            events.push(Tier4Event::Death { npc_id: id });
            dead_ids.insert(id);
            continue;
        }

        // Illness (only if not already ill)
        if !npc.is_ill && rng.random_bool(probabilities::ILLNESS_RATE.min(1.0)) {
            events.push(Tier4Event::Illness { npc_id: id });
        }

        // Seasonal schedule shift
        if let Some(desc) = seasonal_schedule_description(&npc.occupation, season) {
            events.push(Tier4Event::SeasonalShift {
                npc_id: id,
                new_schedule_desc: desc,
            });
        }

        // Trade (merchants only)
        if is_merchant(&npc.occupation) && rng.random_bool(probabilities::TRADE_RATE.min(1.0)) {
            // Find another NPC at the same location to trade with
            if let Some(partner) = find_trade_partner(npcs, npc) {
                events.push(Tier4Event::TradeCompleted {
                    buyer: id,
                    seller: partner,
                });
            }
        }
    }

    // 3. Birth check — find eligible married couples
    let couples = find_eligible_couples(npcs);
    for (parent_a, parent_b) in &couples {
        if dead_ids.contains(parent_a) || dead_ids.contains(parent_b) {
            continue;
        }
        if rng.random_bool(probabilities::BIRTH_RATE.min(1.0)) {
            events.push(Tier4Event::Birth {
                parent_ids: (*parent_a, *parent_b),
            });
        }
    }

    events
}

/// Groups NPCs by their current location.
fn group_by_location(npcs: &[&mut Npc]) -> std::collections::HashMap<LocationId, Vec<NpcId>> {
    let mut groups: std::collections::HashMap<LocationId, Vec<NpcId>> =
        std::collections::HashMap::new();
    for npc in npcs {
        groups.entry(npc.location).or_default().push(npc.id);
    }
    groups
}

/// Finds a trade partner at the same location as the given NPC.
fn find_trade_partner(npcs: &[&mut Npc], merchant: &Npc) -> Option<NpcId> {
    npcs.iter()
        .find(|other| {
            other.id != merchant.id && other.location == merchant.location && !other.is_ill
        })
        .map(|other| other.id)
}

/// Finds eligible couples for birth events.
///
/// A couple is eligible if:
/// - They share a `Romantic` relationship
/// - Both are healthy (not ill)
/// - At least one is aged 18-45
fn find_eligible_couples(npcs: &[&mut Npc]) -> Vec<(NpcId, NpcId)> {
    let mut couples = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for npc in npcs {
        if npc.is_ill {
            continue;
        }
        for (partner_id, rel) in &npc.relationships {
            if rel.kind != RelationshipKind::Romantic {
                continue;
            }
            // Canonical pair ordering to avoid duplicates
            let pair = if npc.id.0 < partner_id.0 {
                (npc.id, *partner_id)
            } else {
                (*partner_id, npc.id)
            };
            if seen.contains(&pair) {
                continue;
            }
            seen.insert(pair);

            // Check partner health
            let partner_healthy = npcs
                .iter()
                .find(|n| n.id == *partner_id)
                .is_some_and(|p| !p.is_ill);
            if !partner_healthy {
                continue;
            }

            // At least one must be of childbearing age (18-45)
            let npc_eligible = (18..=45).contains(&npc.age);
            let partner_age = npcs
                .iter()
                .find(|n| n.id == *partner_id)
                .map(|p| p.age)
                .unwrap_or(0);
            let partner_eligible = (18..=45).contains(&partner_age);

            if npc_eligible || partner_eligible {
                couples.push(pair);
            }
        }
    }
    couples
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{LongTermMemory, ShortTermMemory};
    use crate::reactions::ReactionLog;
    use crate::types::{Intelligence, NpcState};
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;
    use std::collections::HashMap;

    fn make_npc(id: u32, age: u8, occupation: &str) -> Npc {
        Npc {
            id: NpcId(id),
            name: format!("NPC {}", id),
            brief_description: "a person".to_string(),
            age,
            occupation: occupation.to_string(),
            personality: "friendly".to_string(),
            intelligence: Intelligence::default(),
            location: LocationId(1),
            mood: "content".to_string(),
            home: Some(LocationId(1)),
            workplace: Some(LocationId(2)),
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

    #[test]
    fn test_tier4_deterministic_with_seed() {
        let mut npc1 = make_npc(1, 30, "Farmer");
        let mut npc2 = make_npc(2, 40, "Publican");
        let mut npcs: Vec<&mut Npc> = vec![&mut npc1, &mut npc2];

        let mut rng1 = ChaCha8Rng::seed_from_u64(42);
        let events1 = tick_tier4(
            &mut npcs,
            Season::Summer,
            NaiveDate::from_ymd_opt(1820, 7, 15).unwrap(),
            &mut rng1,
        );

        // Reset NPCs
        let mut npc1 = make_npc(1, 30, "Farmer");
        let mut npc2 = make_npc(2, 40, "Publican");
        let mut npcs: Vec<&mut Npc> = vec![&mut npc1, &mut npc2];

        let mut rng2 = ChaCha8Rng::seed_from_u64(42);
        let events2 = tick_tier4(
            &mut npcs,
            Season::Summer,
            NaiveDate::from_ymd_opt(1820, 7, 15).unwrap(),
            &mut rng2,
        );

        assert_eq!(events1.len(), events2.len());
    }

    #[test]
    fn test_tier4_illness_probability() {
        // Over many runs, illness rate should approximate 2%
        let mut illness_count = 0;
        let runs = 10_000;
        let mut rng = ChaCha8Rng::seed_from_u64(123);

        for _ in 0..runs {
            let mut npc = make_npc(1, 30, "Laborer");
            let mut npcs: Vec<&mut Npc> = vec![&mut npc];
            let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
            let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);
            if events
                .iter()
                .any(|e| matches!(e, Tier4Event::Illness { .. }))
            {
                illness_count += 1;
            }
        }

        let rate = illness_count as f64 / runs as f64;
        // Should be approximately 0.02 (2%), allow some statistical variance
        assert!(
            rate > 0.01 && rate < 0.04,
            "Illness rate was {rate}, expected ~0.02"
        );
    }

    #[test]
    fn test_tier4_death_age_scaling() {
        let mut young_deaths = 0;
        let mut old_deaths = 0;
        let runs = 10_000;
        let mut rng = ChaCha8Rng::seed_from_u64(456);
        let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();

        for _ in 0..runs {
            let mut young = make_npc(1, 25, "Laborer");
            let mut npcs: Vec<&mut Npc> = vec![&mut young];
            let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);
            if events.iter().any(|e| matches!(e, Tier4Event::Death { .. })) {
                young_deaths += 1;
            }
        }

        for _ in 0..runs {
            let mut old = make_npc(1, 80, "Retired");
            let mut npcs: Vec<&mut Npc> = vec![&mut old];
            let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);
            if events.iter().any(|e| matches!(e, Tier4Event::Death { .. })) {
                old_deaths += 1;
            }
        }

        // Elderly should die at significantly higher rate
        assert!(
            old_deaths > young_deaths * 5,
            "Old deaths ({old_deaths}) should be >> young deaths ({young_deaths})"
        );
    }

    #[test]
    fn test_tier4_no_birth_if_no_couples() {
        let mut npc1 = make_npc(1, 30, "Farmer");
        let mut npc2 = make_npc(2, 28, "Teacher");
        // No romantic relationships
        let mut npcs: Vec<&mut Npc> = vec![&mut npc1, &mut npc2];

        let mut rng = ChaCha8Rng::seed_from_u64(789);
        let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);

        assert!(
            !events.iter().any(|e| matches!(e, Tier4Event::Birth { .. })),
            "No births should occur without married couples"
        );
    }

    #[test]
    fn test_tier4_seasonal_shift_farmer() {
        let mut farmer = make_npc(1, 35, "Farmer");
        let mut npcs: Vec<&mut Npc> = vec![&mut farmer];

        let mut rng = ChaCha8Rng::seed_from_u64(100);
        let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);

        let has_shift = events.iter().any(|e| {
            matches!(
                e,
                Tier4Event::SeasonalShift { new_schedule_desc, .. }
                if new_schedule_desc.contains("5am")
            )
        });
        assert!(has_shift, "Farmer should get longer summer hours");
    }

    #[test]
    fn test_tier4_seasonal_shift_teacher() {
        let mut teacher = make_npc(1, 40, "Teacher");
        let mut npcs: Vec<&mut Npc> = vec![&mut teacher];

        let mut rng = ChaCha8Rng::seed_from_u64(100);
        let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        let events = tick_tier4(&mut npcs, Season::Summer, date, &mut rng);

        let has_shift = events.iter().any(|e| {
            matches!(
                e,
                Tier4Event::SeasonalShift { new_schedule_desc, .. }
                if new_schedule_desc.contains("No school")
            )
        });
        assert!(has_shift, "Teacher should have no school in summer");
    }

    #[test]
    fn test_festival_detection() {
        let imbolc = NaiveDate::from_ymd_opt(1820, 2, 1).unwrap();
        assert_eq!(Festival::check(imbolc), Some(Festival::Imbolc));
    }

    #[test]
    fn test_festival_between_dates() {
        use chrono::TimeZone;
        let from = Utc.with_ymd_and_hms(1820, 1, 28, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(1820, 2, 3, 0, 0, 0).unwrap();
        let festival = check_festival_in_range(from, to);
        assert_eq!(festival, Some(Festival::Imbolc));
    }

    #[test]
    fn test_festival_range_does_not_loop_at_naive_date_max() {
        // succ_opt() returns None at NaiveDate::MAX; the previous code used
        // .unwrap_or(date) which caused an infinite loop. Verify it terminates.
        use chrono::NaiveDate;
        let max = NaiveDate::MAX;
        let from = max
            .pred_opt()
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();
        let to = max.and_hms_opt(0, 0, 0).unwrap().and_utc();
        // Should return in finite time (no festival on these distant dates)
        let result = check_festival_in_range(from, to);
        assert!(result.is_none());
    }

    #[test]
    fn test_festival_context_injection() {
        let desc = festival_description(Festival::Bealtaine);
        assert!(desc.contains("Bealtaine"));
        assert!(desc.contains("Bonfires"));
    }

    #[test]
    fn test_tier4_festival_bond_events() {
        // Place two NPCs at the same location on a festival date
        let mut npc1 = make_npc(1, 30, "Farmer");
        let mut npc2 = make_npc(2, 25, "Laborer");
        // Both at LocationId(1)
        let mut npcs: Vec<&mut Npc> = vec![&mut npc1, &mut npc2];

        let mut rng = ChaCha8Rng::seed_from_u64(100);
        let imbolc = NaiveDate::from_ymd_opt(1820, 2, 1).unwrap();
        let events = tick_tier4(&mut npcs, Season::Winter, imbolc, &mut rng);

        let has_festival = events
            .iter()
            .any(|e| matches!(e, Tier4Event::FestivalDetected { .. }));
        assert!(has_festival, "Should detect Imbolc festival");

        let has_bond = events
            .iter()
            .any(|e| matches!(e, Tier4Event::FestivalBond { .. }));
        assert!(has_bond, "NPCs at same location should get festival bond");
    }

    #[test]
    fn test_tier4_runs_on_spawn_blocking() {
        // Verify the function can run within spawn_blocking
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let result = tokio::task::spawn_blocking(|| {
                let mut npc = make_npc(1, 30, "Farmer");
                let mut npcs: Vec<&mut Npc> = vec![&mut npc];
                let mut rng = ChaCha8Rng::seed_from_u64(42);
                let date = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
                tick_tier4(&mut npcs, Season::Summer, date, &mut rng)
            })
            .await;
            assert!(
                result.is_ok(),
                "tick_tier4 should run successfully on spawn_blocking"
            );
        });
    }

    #[test]
    fn test_seasonal_schedule_description() {
        // Farmer summer
        let desc = seasonal_schedule_description("Farmer", Season::Summer);
        assert!(desc.is_some());
        assert!(desc.unwrap().contains("5am"));

        // Farmer winter
        let desc = seasonal_schedule_description("Farmer", Season::Winter);
        assert!(desc.is_some());
        assert!(desc.unwrap().contains("8am"));

        // Teacher summer
        let desc = seasonal_schedule_description("Teacher", Season::Summer);
        assert!(desc.is_some());
        assert!(desc.unwrap().contains("No school"));

        // Publican winter
        let desc = seasonal_schedule_description("Publican", Season::Winter);
        assert!(desc.is_some());
        assert!(desc.unwrap().contains("11am"));

        // Laborer — no override in any season
        assert!(seasonal_schedule_description("Laborer", Season::Summer).is_none());
        assert!(seasonal_schedule_description("Laborer", Season::Winter).is_none());
    }
}
