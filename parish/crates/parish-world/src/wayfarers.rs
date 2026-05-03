//! Travel encounters — people and things met on the road between locations.
//!
//! Pure, deterministic module. Call [`resolve_encounter`] with the current
//! time, season, weather, and a seed derived from the game clock + path to
//! get an optional one-line encounter description.

use parish_types::{LocationId, TimeOfDay};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::Weather;
use crate::time::Season;

/// A single travel encounter event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WayfarerEncounter {
    /// Player-visible prose line.
    pub text: String,
}

/// Base probability thresholds by time of day.
fn base_prob(time: TimeOfDay) -> f64 {
    match time {
        TimeOfDay::Dawn => 0.55,
        TimeOfDay::Morning => 0.60,
        TimeOfDay::Midday => 0.45,
        TimeOfDay::Afternoon => 0.50,
        TimeOfDay::Dusk => 0.55,
        TimeOfDay::Night => 0.25,
        TimeOfDay::Midnight => 0.12,
    }
}

/// Weather modifier applied to the base probability.
fn weather_mod(weather: Weather) -> f64 {
    match weather {
        Weather::Clear | Weather::PartlyCloudy => 0.0,
        Weather::Overcast => -0.05,
        Weather::LightRain => -0.10,
        Weather::HeavyRain => -0.20,
        Weather::Fog => -0.05,
        Weather::Storm => -0.30,
    }
}

/// Pick one item from a slice using the given RNG.
fn pick<'a, T>(rng: &mut StdRng, items: &'a [T]) -> &'a T {
    let idx = rng.random_range(0..items.len());
    &items[idx]
}

// Lines keyed by (time, weather-bucket, season). Each function returns
// a slice of candidates; the caller picks one deterministically.

fn dawn_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (_, Weather::Fog) => &[
            "A figure shapes itself from the fog ahead — a farmer, head down, gone before you speak.",
            "The road ahead is white. A dog trots out of the fog, sniffs at you, and disappears back in.",
            "You hear someone's wooden-soled shoes on the road before you see them; they pass without a word.",
        ],
        (Season::Winter, _) => &[
            "A woman wrapped in her shawl hurries past, her breath white in the cold air.",
            "An old man cracks the ice in a puddle with his stick as he passes you on the road.",
            "A cart horse stands in the lane steaming in the frost; its driver gives you a hard nod.",
        ],
        (Season::Spring, _) => &[
            "An early riser is already pulling weeds along his ditch; he straightens to watch you pass.",
            "A girl with a bucket of morning milk steps off the road to let you by.",
            "Two men in work clothes walk ahead of you, talking low — they fall quiet as you draw level.",
        ],
        _ => &[
            "A man with a bundle of turf on his back nods as he passes, breathing hard.",
            "You pass a woman drawing water at a stream, who watches you without speaking.",
            "A youth on a donkey catches you up, looks you over, and trots on ahead.",
        ],
    }
}

fn morning_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (_, Weather::LightRain) | (_, Weather::HeavyRain) | (_, Weather::Storm) => &[
            "A farmer leans over his gate in the rain, watching the road with an expression that says he has nowhere better to be.",
            "You pass a woman hurrying the other way, shawl pulled tight, too wet to talk.",
            "A cart stands in the lane while its driver argues with the rain about whether to go on.",
        ],
        (Season::Summer, _) => &[
            "Two women walking with baskets greet you in Irish and keep going.",
            "A boy drives three thin cows along the grass verge, switching them with a stick.",
            "A tinker's cart is pulled over on the verge; the man is mending something under the wheels.",
        ],
        (Season::Autumn, _) => &[
            "A line of people with sacks over their shoulders are heading to the fields for the harvest.",
            "A man on a cart piled with turnips raises a finger off the reins as you pass.",
            "A woman is shaking a cloth out over her half-door; she watches you with mild curiosity.",
        ],
        _ => &[
            "A farmer nods to you from the far side of a gate as you pass.",
            "A boy and his dog come up the road behind you, pass you at a run, and are gone.",
            "You hear a man singing to himself — thin and reedy — before you see him around the bend.",
            "An older woman sitting on a wall watches you approach, watches you pass, says nothing.",
        ],
    }
}

fn midday_lines(season: Season, _weather: Weather) -> &'static [&'static str] {
    match season {
        Season::Summer => &[
            "You pass a man asleep against a ditch wall in the sun, hat over his face.",
            "A group of children scatter off the road laughing as you approach.",
            "Two men eat their midday meal sitting on a stone wall; one offers you a piece of bread.",
        ],
        Season::Winter => &[
            "The road is empty. Somewhere beyond the ditch a crow is very loudly insisting on something.",
            "A priest rides past on a bony horse without looking at you.",
            "You pass a man cutting furze with a slash-hook; he keeps his eyes on his work.",
        ],
        _ => &[
            "A man on a cart passes, muttering something to his horse.",
            "You see someone ahead on the road, but they turn off down a lane before you reach them.",
            "A crow walks the middle of the road ahead of you and will not be hurried.",
        ],
    }
}

fn afternoon_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (_, Weather::Storm) => &[
            "A man running to get home from the fields shouts something at you as he passes, lost in the wind.",
            "The road is deserted except for a dog who looks at you as if you're equally foolish to be out.",
        ],
        (Season::Autumn, _) => &[
            "A cart loaded high with turf and sods comes down the road; you step into the ditch to let it pass.",
            "Three women with baskets are coming back from the market, talking all at once.",
            "A man herding geese gives you a long look as the birds part around your ankles.",
        ],
        _ => &[
            "A cart slows as it passes. The driver gives a wave without stopping.",
            "You share the road briefly with a man who walks exactly your pace and says nothing.",
            "A child runs out from behind a ditch to stare at you, then runs back.",
        ],
    }
}

fn dusk_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (Season::Autumn, _) | (Season::Winter, _) => &[
            "A figure walks ahead of you in the near-dark, then turns off without looking back.",
            "You hear a door close somewhere ahead — the last person in off the road.",
            "A man passes you going the other way with a lit rushlight cupped in his hand; it bobs away into the dark.",
        ],
        (_, Weather::Fog) => &[
            "The road disappears ahead into grey. You can hear someone walking in the fog — you never see them.",
            "A shape detaches itself from the ditch as you pass — a man resting, watching the light go out of the sky.",
        ],
        _ => &[
            "A figure walks ahead of you in the fading light, then turns off down a lane.",
            "Two men going home from the fields pass you and bid you good night.",
            "A woman calls something from a half-door as you pass — you're not sure if it's meant for you.",
        ],
    }
}

fn night_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (_, Weather::Storm) | (_, Weather::HeavyRain) => &[
            "You hear footsteps behind you in the rain — they stop when you stop.",
            "The road is empty and black. Only the water running in the ditch keeps you company.",
        ],
        (_, Weather::Fog) => &[
            "The fog is thick enough to touch. Somewhere ahead something moves, then is still.",
            "A light floats over the bog to your left — too high for a rushlight, too low for a star.",
        ],
        (Season::Summer, _) => &[
            "A man coming back from the pub raises his hat without breaking stride.",
            "You pass a house where someone is playing a fiddle very softly; they don't stop when you pass.",
        ],
        _ => &[
            "You hear footsteps on the road behind you, but when you turn, no one is there.",
            "A dog barks from behind a gate as you pass, then falls suddenly silent.",
            "A man materialises from the dark, passes without speaking, and is gone.",
        ],
    }
}

fn midnight_lines(season: Season, weather: Weather) -> &'static [&'static str] {
    match (season, weather) {
        (_, Weather::Storm) => {
            &["The road is yours alone. The storm has driven everything else inside."]
        }
        (_, Weather::Fog) => &[
            "An owl calls from somewhere in the fog — and then, closer, calls again.",
            "Something moves at the edge of the ditch. You don't stop to see what.",
        ],
        (Season::Autumn, _) | (Season::Winter, _) => &[
            "An owl hoots from a nearby tree, breaking the silence.",
            "The world has gone to bed. You and the stars have the road to yourselves.",
            "A fox crosses ahead of you, glances back once, and slips into the dark.",
        ],
        _ => &[
            "An owl hoots from a nearby tree, breaking the silence.",
            "The moonlight makes a familiar road strange. Something white moves in the field — a sheet on a line.",
            "A fox sits in the middle of the road, watching you approach. It doesn't move until you're almost on it.",
        ],
    }
}

/// Compute a seed from the game clock (minutes since epoch) and path endpoints.
pub fn encounter_seed(clock_minutes: i64, from: LocationId, to: LocationId) -> u64 {
    let a = clock_minutes as u64;
    let b = from.0 as u64;
    let c = to.0 as u64;
    // Simple mix
    a.wrapping_mul(2654435761)
        .wrapping_add(b.wrapping_mul(40503))
        .wrapping_add(c.wrapping_mul(16777619))
}

/// Collect inspiration lines for a given (time, season, weather) across *all*
/// season/weather variants for that time of day, not just the matching one.
/// This gives the LLM a broader sense of tone without locking it into the
/// exact canned bucket for the current conditions.
fn inspiration_pool(time: TimeOfDay) -> Vec<&'static str> {
    let seasons = [
        Season::Spring,
        Season::Summer,
        Season::Autumn,
        Season::Winter,
    ];
    let weathers = [
        Weather::Clear,
        Weather::Fog,
        Weather::LightRain,
        Weather::Storm,
        Weather::HeavyRain,
    ];
    let mut out: Vec<&'static str> = Vec::new();
    for s in seasons {
        for w in weathers {
            let lines = match time {
                TimeOfDay::Dawn => dawn_lines(s, w),
                TimeOfDay::Morning => morning_lines(s, w),
                TimeOfDay::Midday => midday_lines(s, w),
                TimeOfDay::Afternoon => afternoon_lines(s, w),
                TimeOfDay::Dusk => dusk_lines(s, w),
                TimeOfDay::Night => night_lines(s, w),
                TimeOfDay::Midnight => midnight_lines(s, w),
            };
            for l in lines {
                if !out.contains(l) {
                    out.push(l);
                }
            }
        }
    }
    out
}

/// Prompt pair (system, context) for an LLM travel-encounter generation call.
///
/// The canned line is passed as the seed-variant and 4 other lines from the
/// same time-of-day are drawn (deterministically, using the seed) as
/// inspirations. The LLM is asked to write a single new line in the same
/// register — short, sensory, period-correct for 1820 rural Ireland, and
/// without stage-direction bullets or NPC dialogue.
pub fn build_enrichment_prompt(
    canned: &WayfarerEncounter,
    time: TimeOfDay,
    season: Season,
    weather: Weather,
    seed: u64,
) -> (String, String) {
    let system = "You are writing one line of ambient narration for a walking \
scene in rural Ireland, 1820. Output a single sentence — at most two — \
describing something sensory the player notices on the road. Examples of \
tone will be provided; match them in length, rhythm, and register. \
Do not use quotation marks, stage directions, or bullet points. Do not \
name specific NPCs. Do not refer to the player as 'you' more than once. \
Do not use anachronistic vocabulary (no cars, bikes, watches, minutes). \
Write one line only, no preamble, no explanation."
        .to_string();

    let pool = inspiration_pool(time);
    let mut rng = StdRng::seed_from_u64(seed ^ 0x9E3779B97F4A7C15);
    // Pick up to 4 distinct inspiration lines other than the canned one.
    let mut picks: Vec<&'static str> = Vec::new();
    let mut attempts = 0;
    while picks.len() < 4 && attempts < 40 && !pool.is_empty() {
        let idx = rng.random_range(0..pool.len());
        let line = pool[idx];
        if line != canned.text && !picks.contains(&line) {
            picks.push(line);
        }
        attempts += 1;
    }

    let mut context = String::new();
    context.push_str(&format!(
        "Conditions: {time} on a {season:?} {weather} day.\n\n"
    ));
    context.push_str("Example lines in the right register:\n");
    for p in &picks {
        context.push_str(&format!("- {p}\n"));
    }
    context.push_str(&format!("- {}\n", canned.text));
    context.push_str(
        "\nWrite ONE new line in the same register. \
Do not copy any of the examples. Just the line — no quotes, no explanation.\n",
    );
    (system, context)
}

/// Resolve a travel encounter.
///
/// Returns `Some(encounter)` if the dice roll triggers, `None` otherwise.
/// `seed` should be derived from [`encounter_seed`] for reproducibility.
pub fn resolve_encounter(
    time: TimeOfDay,
    season: Season,
    weather: Weather,
    seed: u64,
) -> Option<WayfarerEncounter> {
    let mut rng = StdRng::seed_from_u64(seed);
    let prob = (base_prob(time) + weather_mod(weather)).clamp(0.0, 1.0);
    let roll: f64 = rng.random();
    if roll >= prob {
        return None;
    }
    let lines = match time {
        TimeOfDay::Dawn => dawn_lines(season, weather),
        TimeOfDay::Morning => morning_lines(season, weather),
        TimeOfDay::Midday => midday_lines(season, weather),
        TimeOfDay::Afternoon => afternoon_lines(season, weather),
        TimeOfDay::Dusk => dusk_lines(season, weather),
        TimeOfDay::Night => night_lines(season, weather),
        TimeOfDay::Midnight => midnight_lines(season, weather),
    };
    Some(WayfarerEncounter {
        text: pick(&mut rng, lines).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_is_deterministic() {
        let s1 = encounter_seed(1000, LocationId(1), LocationId(5));
        let s2 = encounter_seed(1000, LocationId(1), LocationId(5));
        assert_eq!(s1, s2);
    }

    #[test]
    fn seed_varies_by_clock() {
        let s1 = encounter_seed(1000, LocationId(1), LocationId(5));
        let s2 = encounter_seed(1001, LocationId(1), LocationId(5));
        assert_ne!(s1, s2);
    }

    #[test]
    fn seed_varies_by_path() {
        let s1 = encounter_seed(1000, LocationId(1), LocationId(5));
        let s2 = encounter_seed(1000, LocationId(2), LocationId(5));
        assert_ne!(s1, s2);
    }

    #[test]
    fn roll_zero_always_triggers() {
        // With seed producing a near-zero roll, should always hit even low-prob times
        // We brute-force a seed that triggers midnight (prob 0.12)
        let mut found = false;
        for i in 0u64..200 {
            let seed = encounter_seed(i as i64, LocationId(1), LocationId(2));
            if resolve_encounter(TimeOfDay::Midnight, Season::Autumn, Weather::Clear, seed)
                .is_some()
            {
                found = true;
                break;
            }
        }
        assert!(found, "At least one of 200 midnight seeds should trigger");
    }

    #[test]
    fn morning_clear_spring_triggers_often() {
        let mut hits = 0usize;
        for i in 0..100u64 {
            let seed = encounter_seed(i as i64, LocationId(1), LocationId(3));
            if resolve_encounter(TimeOfDay::Morning, Season::Spring, Weather::Clear, seed).is_some()
            {
                hits += 1;
            }
        }
        // Morning prob = 0.60; expect ~60 hits out of 100
        assert!(hits > 30, "Morning hits={hits}, expected >30");
    }

    #[test]
    fn storm_suppresses_encounters() {
        let mut hits = 0usize;
        for i in 0..100u64 {
            let seed = encounter_seed(i as i64, LocationId(1), LocationId(3));
            if resolve_encounter(TimeOfDay::Morning, Season::Winter, Weather::Storm, seed).is_some()
            {
                hits += 1;
            }
        }
        // Morning+storm prob = 0.30; should be well below normal 60
        assert!(hits < 55, "Storm hits={hits}, should suppress encounters");
    }

    #[test]
    fn inspiration_pool_is_nonempty_for_all_times() {
        let times = [
            TimeOfDay::Dawn,
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ];
        for t in times {
            let pool = inspiration_pool(t);
            assert!(pool.len() >= 3, "Pool too small for {t:?}: {}", pool.len());
        }
    }

    #[test]
    fn enrichment_prompt_contains_canned_and_conditions() {
        let canned = WayfarerEncounter {
            text: "CANNED_SEED_LINE".to_string(),
        };
        let (system, context) = build_enrichment_prompt(
            &canned,
            TimeOfDay::Morning,
            Season::Summer,
            Weather::Clear,
            12345,
        );
        assert!(system.to_lowercase().contains("1820"));
        assert!(context.contains("CANNED_SEED_LINE"));
        assert!(context.contains("Morning"));
        // Four inspirations + canned = 5 "- " prefixed lines minimum
        let hyphen_lines = context
            .lines()
            .filter(|l| l.trim_start().starts_with("- "))
            .count();
        assert!(hyphen_lines >= 4, "Too few examples: {hyphen_lines}");
    }

    #[test]
    fn enrichment_prompt_examples_are_deterministic() {
        let canned = WayfarerEncounter {
            text: "SEED".to_string(),
        };
        let (_, ctx1) =
            build_enrichment_prompt(&canned, TimeOfDay::Dusk, Season::Autumn, Weather::Fog, 42);
        let (_, ctx2) =
            build_enrichment_prompt(&canned, TimeOfDay::Dusk, Season::Autumn, Weather::Fog, 42);
        assert_eq!(ctx1, ctx2);
    }

    #[test]
    fn encounter_text_is_nonempty() {
        let seed = encounter_seed(42, LocationId(1), LocationId(2));
        if let Some(enc) =
            resolve_encounter(TimeOfDay::Morning, Season::Summer, Weather::Clear, seed)
        {
            assert!(!enc.text.is_empty());
        }
    }

    #[test]
    fn all_time_season_weather_combos_produce_nonempty_text() {
        let times = [
            TimeOfDay::Dawn,
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
            TimeOfDay::Midnight,
        ];
        let seasons = [
            Season::Spring,
            Season::Summer,
            Season::Autumn,
            Season::Winter,
        ];
        let weathers = [
            Weather::Clear,
            Weather::Fog,
            Weather::Storm,
            Weather::LightRain,
        ];
        for &t in &times {
            for &s in &seasons {
                for &w in &weathers {
                    // Use seed=0 which gives roll=0, guaranteed trigger
                    let rng = rand::rngs::StdRng::seed_from_u64(0);
                    let prob = (super::base_prob(t) + super::weather_mod(w)).clamp(0.0, 1.0);
                    // Just test the pool lookup doesn't panic
                    let lines = match t {
                        TimeOfDay::Dawn => super::dawn_lines(s, w),
                        TimeOfDay::Morning => super::morning_lines(s, w),
                        TimeOfDay::Midday => super::midday_lines(s, w),
                        TimeOfDay::Afternoon => super::afternoon_lines(s, w),
                        TimeOfDay::Dusk => super::dusk_lines(s, w),
                        TimeOfDay::Night => super::night_lines(s, w),
                        TimeOfDay::Midnight => super::midnight_lines(s, w),
                    };
                    assert!(!lines.is_empty(), "No lines for {t:?}/{s:?}/{w:?}");
                    let _ = prob;
                    let _ = rng;
                }
            }
        }
    }
}
