//! Deterministic listening vignettes grounded in authored location lore.
//!
//! This module gives every place an ordinary soundscape and lets locations
//! with `mythological_significance` carry a restrained sensory echo. Concrete
//! echo families require structural signals from the location; unfamiliar
//! places get neutral phrasing. Lore is returned exactly as the mod authored it.

use chrono::{Datelike, NaiveDate};
use parish_types::time::{Season, TimeOfDay};

use crate::graph::LocationData;
use crate::{LocationId, Weather};

/// The three pieces of a player-facing listening response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListeningVignette {
    /// Ordinary sounds shaped by shelter, weather, time, and season.
    pub ambient: String,
    /// A cautious sensory impression at a place with authored folklore.
    pub echo: Option<String>,
    /// The exact `mythological_significance` text authored by the mod.
    pub lore: Option<String>,
}

/// Builds a stable seed for a listening moment.
///
/// The broad time-of-day bucket is deliberate: repeated listens in the same
/// scene reproduce exactly, while another date, place, or phase of the day can
/// reveal a different sensory phrasing.
pub fn listening_seed(date: NaiveDate, location_id: LocationId, time: TimeOfDay) -> u64 {
    let date_part = (date.year() as i64 as u64)
        .wrapping_mul(512)
        .wrapping_add(date.ordinal() as u64);
    let scene_key = date_part
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((location_id.0 as u64).wrapping_mul(2_654_435_761));

    // SplitMix64's finalizer avalanches the date/location key. Add the compact
    // time bucket afterwards so adjacent phases rotate through the phrasing
    // table instead of aliasing when the table happens to have three entries.
    let mut mixed = scene_key;
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (mixed ^ (mixed >> 31)).wrapping_add(time_index(time))
}

/// Listens to one location without inference, randomness, or state mutation.
pub fn listen_at_location(
    location: &LocationData,
    time: TimeOfDay,
    season: Season,
    weather: Weather,
    seed: u64,
) -> ListeningVignette {
    let ambient = ambient_sound(location.indoor, time, season, weather).to_string();
    let lore = location
        .mythological_significance
        .as_deref()
        .filter(|text| !text.trim().is_empty());
    let echo = lore.map(|authored| {
        let kind = classify_echo(location, authored);
        let variants = echo_variants(kind);
        variants[(seed % variants.len() as u64) as usize].to_string()
    });

    ListeningVignette {
        ambient,
        echo,
        lore: lore.map(str::to_string),
    }
}

fn time_index(time: TimeOfDay) -> u64 {
    match time {
        TimeOfDay::Dawn => 0,
        TimeOfDay::Morning => 1,
        TimeOfDay::Midday => 2,
        TimeOfDay::Afternoon => 3,
        TimeOfDay::Dusk => 4,
        TimeOfDay::Night => 5,
        TimeOfDay::Midnight => 6,
    }
}

fn ambient_sound(indoor: bool, time: TimeOfDay, season: Season, weather: Weather) -> &'static str {
    if indoor {
        return match weather {
            Weather::Storm => {
                "Wind shoulders the walls; a loose fastening taps somewhere above the rafters."
            }
            Weather::HeavyRain => {
                "Rain drums on the roof until the smaller sounds of the room gather beneath it."
            }
            Weather::LightRain => {
                "Soft rain works at the roof, with the occasional settling creak from the room."
            }
            Weather::Fog => {
                "The damp has muffled the world outside; close at hand, wood and cloth softly shift."
            }
            Weather::Clear | Weather::PartlyCloudy | Weather::Overcast => match time {
                TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight => {
                    "Beyond the walls the parish has gone quiet; the room keeps its own small creaks."
                }
                _ => "Work and footfall sound briefly distinct, then settle back into the room.",
            },
        };
    }

    match weather {
        Weather::Storm => {
            "The storm fills every quarter at once — wind in the grass, branches, and stone."
        }
        Weather::HeavyRain => {
            "Heavy rain closes the distance, beating earth and leaves into one steady noise."
        }
        Weather::LightRain => {
            "A fine rain ticks through grass and hedge; farther sounds come softened."
        }
        Weather::Fog => {
            "Fog draws the parish close. A bird calls once, with no distance to place it by."
        }
        Weather::Clear | Weather::PartlyCloudy | Weather::Overcast => match time {
            TimeOfDay::Dawn => match season {
                Season::Winter => {
                    "A rook stirs in the cold dawn; frost-stiff grass answers under a passing foot."
                }
                _ => "The first birds test the dawn, one call at a time above the wet grass.",
            },
            TimeOfDay::Morning | TimeOfDay::Midday | TimeOfDay::Afternoon => match season {
                Season::Spring => {
                    "Larks work above the fields, with lambs and distant labour underneath."
                }
                Season::Summer => {
                    "Insects hum in the verge while field sounds carry cleanly on the mild air."
                }
                Season::Autumn => {
                    "Dry stalks fret against one another; a cart sounds somewhere beyond the fields."
                }
                Season::Winter => {
                    "The bare hedges give back every crow call and far-off knock of work."
                }
            },
            TimeOfDay::Dusk => {
                "Day-work falls away by degrees; a gate knocks once and the hedges take the sound."
            }
            TimeOfDay::Night | TimeOfDay::Midnight => {
                "Once your breathing settles, water, grass, and an unseen night bird take separate voices."
            }
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EchoKind {
    Crossroads,
    Well,
    Lough,
    FairyFort,
    Bog,
    OldChurch,
    Other,
}

fn classify_echo(location: &LocationData, lore: &str) -> EchoKind {
    let name = location.name.to_lowercase();
    let description = location.description_template.to_lowercase();
    let lore = lore.to_lowercase();

    // Only strong structural signals may select a concrete echo family. Broad
    // lore words such as "monster" or "spring" are deliberately insufficient:
    // another mod's cave monster or spring festival must not invent a lough or
    // well at that location.
    if contains_word(&name, "crossroads") || contains_word(&description, "crossroads") {
        EchoKind::Crossroads
    } else if contains_phrase(&name, "holy well")
        || contains_phrase(&description, "holy well")
        || contains_phrase(&description, "stone-lined well")
    {
        EchoKind::Well
    } else if contains_word(&name, "lough") || contains_word(&description, "lake") {
        EchoKind::Lough
    } else if contains_phrase(&name, "fairy fort")
        || contains_phrase(&description, "ring fort")
        || contains_word(&lore, "rath")
    {
        EchoKind::FairyFort
    } else if contains_word(&name, "bog") || contains_word(&description, "bog") {
        EchoKind::Bog
    } else if contains_word(&name, "church")
        || contains_word(&description, "church")
        || contains_phrase(&lore, "old church")
        || contains_phrase(&lore, "church ruins")
    {
        EchoKind::OldChurch
    } else {
        EchoKind::Other
    }
}

fn contains_word(text: &str, needle: &str) -> bool {
    contains_phrase(text, needle)
}

fn contains_phrase(text: &str, phrase: &str) -> bool {
    let words = text
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();
    let phrase_words = phrase
        .split(|character: char| !character.is_alphabetic())
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>();

    !phrase_words.is_empty()
        && words
            .windows(phrase_words.len())
            .any(|window| window == phrase_words.as_slice())
}

fn echo_variants(kind: EchoKind) -> &'static [&'static str] {
    match kind {
        EchoKind::Crossroads => &[
            "For a breath, each road seems to carry a different wind.",
            "A sound travels up one road and appears to leave by another, too faint to name.",
            "The four ways fall quiet together, as though the meeting of them were listening too.",
        ],
        EchoKind::Well => &[
            "The air above the well stones feels cooler than the ground around them.",
            "A small sound rises from within the well and is gone before you can place it.",
            "The quiet gathered around the well seems, for a moment, deeper than the well itself.",
        ],
        EchoKind::Lough => &[
            "Far from shore, something turns the surface once; it may be a fish, or only the current.",
            "A hollow knock carries over the water with no boat near enough to own it.",
            "The near water falls still while one deep sound travels in from the lough.",
        ],
        EchoKind::FairyFort => &[
            "The grass within the old ring lies still while the verge beyond it moves.",
            "A thin chiming reaches you and is gone before you can decide whether it was harness or birdcall.",
            "Your next footstep sounds too loud beside the old earthwork, so you do not take it.",
        ],
        EchoKind::Bog => &[
            "The wind draws a long vowel across the heather; distance makes a voice of it.",
            "A wet settling sound passes under the turf, slow as a sleeper turning.",
            "For a moment the bog returns a footfall that was not your last one.",
        ],
        EchoKind::OldChurch => &[
            "Wind moves through stone and grass with a patient, papery whisper.",
            "A small bird stirs near the church ground; the place seems briefly occupied again.",
            "Stone catches one narrow current of air and gives it a low, breath-like note.",
        ],
        EchoKind::Other => &[
            "One small sound stands apart from the rest, then is ordinary again.",
            "The place seems to hold its breath for no longer than you do.",
            "You hear nothing impossible, though one sound resists being named.",
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::GeoKind;

    fn location(name: &str, lore: Option<&str>, indoor: bool) -> LocationData {
        LocationData {
            id: LocationId(11),
            name: name.to_string(),
            description_template: String::new(),
            landmarks: vec![],
            indoor,
            public: true,
            connections: vec![],
            lat: 53.0,
            lon: -8.0,
            associated_npcs: vec![],
            mythological_significance: lore.map(str::to_string),
            aliases: vec![],
            geo_kind: GeoKind::Fictional,
            relative_to: None,
            geo_source: None,
        }
    }

    #[test]
    fn same_state_yields_the_same_vignette() {
        let place = location(
            "The Crossroads",
            Some("Crossroads hold power in Irish folklore."),
            false,
        );
        let a = listen_at_location(&place, TimeOfDay::Dusk, Season::Autumn, Weather::Fog, 2);
        let b = listen_at_location(&place, TimeOfDay::Dusk, Season::Autumn, Weather::Fog, 2);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_select_different_echo_wording() {
        let place = location(
            "The Crossroads",
            Some("Crossroads hold power in Irish folklore."),
            false,
        );
        let a = listen_at_location(&place, TimeOfDay::Dusk, Season::Autumn, Weather::Clear, 0);
        let b = listen_at_location(&place, TimeOfDay::Dusk, Season::Autumn, Weather::Clear, 1);
        assert_ne!(a.echo, b.echo);
        assert_eq!(a.lore, b.lore);
    }

    #[test]
    fn authored_lore_is_preserved_exactly() {
        let lore = "  A rath said to be home to the sídhe. No farmer ploughs it.  ";
        let place = location("The Fairy Fort", Some(lore), false);
        let vignette =
            listen_at_location(&place, TimeOfDay::Night, Season::Winter, Weather::Clear, 7);
        assert_eq!(vignette.lore.as_deref(), Some(lore));
        assert!(vignette.echo.is_some());
    }

    #[test]
    fn ordinary_location_invents_no_lore_or_echo() {
        let place = location("The Letter Office", None, true);
        let vignette = listen_at_location(
            &place,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Clear,
            0,
        );
        assert!(vignette.lore.is_none());
        assert!(vignette.echo.is_none());
        assert!(!vignette.ambient.is_empty());
    }

    #[test]
    fn dramatic_weather_changes_the_ordinary_soundscape() {
        let place = location("The Bog Road", None, false);
        let clear = listen_at_location(
            &place,
            TimeOfDay::Afternoon,
            Season::Summer,
            Weather::Clear,
            0,
        );
        let storm = listen_at_location(
            &place,
            TimeOfDay::Afternoon,
            Season::Summer,
            Weather::Storm,
            0,
        );
        assert_ne!(clear.ambient, storm.ambient);
        assert!(storm.ambient.contains("storm"));
    }

    #[test]
    fn strong_location_signals_select_expected_echo_families() {
        let places = [
            (
                "The Crossroads",
                "Crossroads hold power — a place between places.",
                EchoKind::Crossroads,
            ),
            (
                "St. Brigid's Church",
                "An older holy well dedicated to Brigid.",
                EchoKind::OldChurch,
            ),
            (
                "Lough Ree Shore",
                "The lough is said to hold a wurm or monster.",
                EchoKind::Lough,
            ),
            (
                "The Fairy Fort",
                "A rath said to be home to the sídhe.",
                EchoKind::FairyFort,
            ),
            (
                "The Bog Road",
                "Bogs preserve memories and voices in the wind.",
                EchoKind::Bog,
            ),
            (
                "Kilteevan Village",
                "Taobhán kept a cell by the old church ruins.",
                EchoKind::OldChurch,
            ),
            (
                "The Holy Well",
                "The holy well is a spring left offerings for cures.",
                EchoKind::Well,
            ),
        ];
        for (name, lore, expected) in places {
            let place = location(name, Some(lore), false);
            assert_eq!(
                classify_echo(&place, lore),
                expected,
                "wrong family for {name}"
            );
        }
    }

    #[test]
    fn broad_lore_words_do_not_invent_location_features() {
        let place = location(
            "The Limestone Cave",
            Some("A monster sleeps here until spring, or so the story goes."),
            false,
        );
        assert_eq!(
            classify_echo(&place, place.mythological_significance.as_deref().unwrap()),
            EchoKind::Other
        );

        let meadow = location(
            "The Meadow",
            Some("It is said to be rather quiet after dusk."),
            false,
        );
        assert_eq!(
            classify_echo(
                &meadow,
                meadow.mythological_significance.as_deref().unwrap()
            ),
            EchoKind::Other,
            "the letters 'rath' inside 'rather' must not imply a ring fort"
        );

        let fairy_shop = location(
            "The Fairy Fortune Shop",
            Some("Nothing more than a shopkeeper's tale."),
            false,
        );
        assert_eq!(
            classify_echo(
                &fairy_shop,
                fairy_shop.mythological_significance.as_deref().unwrap()
            ),
            EchoKind::Other,
            "'fairy fort' spanning the start of 'fortune' must not imply an earthwork"
        );

        let mut fortnight = location(
            "The Long Meadow",
            Some("A seasonal custom is remembered here."),
            false,
        );
        fortnight.description_template =
            "Every spring fortnight, families gather beside the road.".to_string();
        assert_eq!(
            classify_echo(
                &fortnight,
                fortnight.mythological_significance.as_deref().unwrap()
            ),
            EchoKind::Other,
            "'ring fort' spanning two unrelated words must not imply an earthwork"
        );

        let household = location(
            "The Cottage",
            Some("This was once a household church custom."),
            false,
        );
        assert_eq!(
            classify_echo(
                &household,
                household.mythological_significance.as_deref().unwrap()
            ),
            EchoKind::Other,
            "'old church' spanning 'household' must not imply church ground"
        );
    }

    #[test]
    fn time_phase_rotates_echo_wording() {
        let place = location(
            "The Crossroads",
            Some("The old people avoid this meeting of roads after dusk."),
            false,
        );
        let day = NaiveDate::from_ymd_opt(1820, 3, 20).unwrap();
        let morning_seed = listening_seed(day, place.id, TimeOfDay::Morning);
        let midday_seed = listening_seed(day, place.id, TimeOfDay::Midday);
        let morning = listen_at_location(
            &place,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Clear,
            morning_seed,
        );
        let midday = listen_at_location(
            &place,
            TimeOfDay::Midday,
            Season::Spring,
            Weather::Clear,
            midday_seed,
        );
        assert_ne!(morning.echo, midday.echo);
    }

    #[test]
    fn ordinary_ambience_varies_by_time_season_and_shelter() {
        let outside = location("The Green", None, false);
        let indoors = location("The Letter Office", None, true);

        let dawn = listen_at_location(&outside, TimeOfDay::Dawn, Season::Spring, Weather::Clear, 0);
        let night = listen_at_location(
            &outside,
            TimeOfDay::Night,
            Season::Spring,
            Weather::Clear,
            0,
        );
        assert_ne!(dawn.ambient, night.ambient);

        let spring = listen_at_location(
            &outside,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Clear,
            0,
        );
        let winter = listen_at_location(
            &outside,
            TimeOfDay::Morning,
            Season::Winter,
            Weather::Clear,
            0,
        );
        assert_ne!(spring.ambient, winter.ambient);

        let sheltered = listen_at_location(
            &indoors,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Storm,
            0,
        );
        let exposed = listen_at_location(
            &outside,
            TimeOfDay::Morning,
            Season::Spring,
            Weather::Storm,
            0,
        );
        assert_ne!(sheltered.ambient, exposed.ambient);
    }

    #[test]
    fn listening_seed_changes_across_place_date_and_time() {
        let day = NaiveDate::from_ymd_opt(1820, 3, 20).unwrap();
        let next_day = NaiveDate::from_ymd_opt(1820, 3, 21).unwrap();
        let seed = listening_seed(day, LocationId(1), TimeOfDay::Morning);
        assert_ne!(
            seed,
            listening_seed(next_day, LocationId(1), TimeOfDay::Morning)
        );
        assert_ne!(seed, listening_seed(day, LocationId(2), TimeOfDay::Morning));
        assert_ne!(seed, listening_seed(day, LocationId(1), TimeOfDay::Night));
    }
}
