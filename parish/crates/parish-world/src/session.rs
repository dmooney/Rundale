//! Traditional music session at the pub.
//!
//! A session (*seisiún*) is a loose gathering of musicians — a fiddler, a
//! piper, a flute, sometimes a singer — taking turns at tunes in the warm
//! corner of the pub. In 1820s rural Ireland this is the weekly heartbeat
//! of a village: tune heard last week returns, the *sean-nós* verse drifts
//! over the talk, and a stranger is asked for a song to see what they know.
//!
//! This module generates a short evocative vignette of a session from the
//! current game clock, weather, and season. The output is a
//! [`SessionVignette`] with a few composed lines; the handler in
//! `parish-core/src/ipc/commands.rs` stitches them into a single response.
//!
//! Design goals, in order:
//!
//! 1. **Deterministic** — same date + same location seed → same session,
//!    so saves reproduce exactly and tests can assert on output text.
//! 2. **Offline** — a pure function, no LLM or network. Works in the
//!    simulator and in headless script mode.
//! 3. **Evocative but vague** — short, rhythm-balanced lines that read
//!    as a memory of the session rather than a transcript. The LLM can
//!    be layered on later without changing the API.
//!
//! The set of tunes, musicians and ambient lines is deliberately kept to
//! historically plausible Irish material from before 1820 (Carolan,
//! *sean-nós* songs, named dance forms) so that the vignette doesn't fight
//! the setting.

use parish_types::Weather;
use parish_types::time::{Season, TimeOfDay};

/// A single vignette describing a moment from a session at the pub.
///
/// Four short composed lines: one musician, one tune, one ambient touch
/// (the room, the weather, a corner of the bar), and optionally a
/// one-line verse when the musician is a singer rather than a player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionVignette {
    /// The musician or singer leading this moment.
    pub musician: String,
    /// A description of the tune or song being played.
    pub tune: String,
    /// One line of room / bar / weather atmosphere.
    pub ambient: String,
    /// Present only when `musician` is a singer — a single line of verse.
    pub verse: Option<String>,
}

/// Returns `true` when the given time-of-day is a plausible session hour.
///
/// Sessions happen after the day's work: dusk through midnight. A midday
/// enquiry returns a hint instead of a vignette — tunes at noon would read
/// as staged rather than overheard.
pub fn is_session_hour(tod: TimeOfDay) -> bool {
    matches!(
        tod,
        TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight
    )
}

/// Builds a vignette from a deterministic seed plus current weather and
/// season.
///
/// `seed` is expected to be derived from the game date and the location id
/// so that two calls on the same night at the same pub return the same
/// session. The handler folds those fields in — see
/// [`session_seed`].
pub fn vignette_from_seed(seed: u64, weather: Weather, season: Season) -> SessionVignette {
    let musician_idx = (seed % MUSICIANS.len() as u64) as usize;
    let musician = MUSICIANS[musician_idx];

    let tune_idx = ((seed / 7) % TUNES.len() as u64) as usize;
    let tune = TUNES[tune_idx];

    // Ambient line mixes weather + season so the vignette reads as
    // *this night, this weather*. Weather wins when it's dramatic
    // (storm, fog, heavy rain); otherwise season phrasing carries.
    let ambient = match weather {
        Weather::Storm => "Outside, the wind worries the door; inside, the turf fire keeps time.",
        Weather::HeavyRain => "Rain hammers the thatch overhead; the fiddle rises against it.",
        Weather::Fog => "A soft fog presses the small window; the candles bead with damp.",
        Weather::LightRain => match season {
            Season::Winter => "A thin rain on the shutters; the bar smells of wet wool and turf.",
            _ => "A soft rain speckles the window; the talk drops as the bow comes up.",
        },
        Weather::Overcast | Weather::PartlyCloudy | Weather::Clear => match season {
            Season::Spring => {
                "Through the open door, the parish is still light; lambs bleat somewhere far off."
            }
            Season::Summer => {
                "The door stands half-open to a long summer dusk; swallows cut the sky above the yard."
            }
            Season::Autumn => {
                "Smoke from the hearth curls out the open door into the autumn half-dark."
            }
            Season::Winter => {
                "The shutters are closed against a hard cold; pints go down slow, the room leans in."
            }
        },
    };

    let tune_line = format!("strikes up {tune}.");

    // Some "musicians" are in fact singers; those get an additional line
    // of verse so the vignette breathes. We check a stable marker rather
    // than a separate table so adding a new singer means editing one list.
    let verse = if musician.starts_with("A woman's voice")
        || musician.starts_with("An old man's voice")
        || musician.starts_with("A boy's clear voice")
    {
        let verse_idx = ((seed / 53) % VERSES.len() as u64) as usize;
        Some(VERSES[verse_idx].to_string())
    } else {
        None
    };

    SessionVignette {
        musician: musician.to_string(),
        tune: tune_line,
        ambient: ambient.to_string(),
        verse,
    }
}

/// Folds a game date and location id into a single `u64` seed.
///
/// Two calls on the same date at the same location return the same seed.
/// Different locations on the same night produce different seeds, which
/// matters if a future patch lets sessions happen anywhere a fiddle might
/// turn up (e.g. a wake-house or a threshing).
pub fn session_seed(date: chrono::NaiveDate, location_id: u32) -> u64 {
    // Encode the date as y*512 + ordinal so adjacent days produce very
    // different tune/musician pairs rather than the obvious off-by-one.
    let year = date.format("%Y").to_string().parse::<u64>().unwrap_or(1820);
    let ordinal = date.format("%j").to_string().parse::<u64>().unwrap_or(1);
    let date_part = year.wrapping_mul(512).wrapping_add(ordinal);
    // Spread location into upper bits so the tune-index divisor (`/7`)
    // still sees variety across pubs on the same night.
    date_part
        .wrapping_mul(0x9E37_79B9_7F4A_7C15) // Weyl / Fibonacci mixer
        .wrapping_add((location_id as u64).wrapping_mul(2_654_435_761))
}

// ── Tables ─────────────────────────────────────────────────────────────
//
// Kept short and rhythm-balanced. Adding a line anywhere is safe; the
// seed mixer guarantees no index ever reads past the end of the table.
// All content is plausible for 1820 rural Connacht: Carolan harp tunes
// (d. 1738), *sean-nós* songs in circulation by then, named dance forms
// (reel, jig, slip jig, hornpipe, slow air, lament, planxty).

/// Musicians and singers who might be carrying the session tonight.
const MUSICIANS: &[&str] = &[
    "An old fiddler sets his bow to the strings and",
    "A young piper settles the bag under his elbow and",
    "A blind flute-player feels for the stops and",
    "A bodhrán under a hard-callused hand picks up the beat as the fiddler",
    "A travelling harper — a rare sight this far west — tunes briefly and",
    "A fiddler with a red beard leans into his instrument and",
    "A woman's voice lifts clear from the corner by the fire; she",
    "An old man's voice, cracked and slow, lifts from the settle; he",
    "A boy's clear voice rises over the crowd's talk; he",
    "A whistle player — pennywhistle, tin — warms the metal in his hand and",
];

/// Tunes and songs in the session's repertoire.
///
/// The phrasing completes the sentence `"{musician} strikes up {tune}."`
/// so each entry reads naturally with or without a dance-form label.
const TUNES: &[&str] = &[
    "a reel they call \"The Blackbird\", old as any of them",
    "a slow air on the harp — Carolan's \"Sheebeg and Sheemore\"",
    "a jig the boys call \"The Humours of Glen\"",
    "a slip jig, light on the third beat",
    "a lament — no name, only the shape of one",
    "\"Planxty Irwin\", for the big house that is no more",
    "a hornpipe, quick and stubborn",
    "a march — a United Irishmen's tune, or the shape of one",
    "\"Róisín Dubh\", slow and close to the bone",
    "\"Carolan's Concerto\", for the hands that remember it",
    "a reel with no name — a fisherman brought it back from Mayo last year",
    "\"An Bonnán Buí\", for the yellow bittern and the drink that took him",
    "a ballad the travellers left behind, half in English, half not",
    "a double jig — the bar claps in against the bodhrán",
    "a set of three reels run together without pause",
];

/// Single-line verses used when the \"musician\" is in fact a singer.
///
/// Kept deliberately short and vague so they sit inside the vignette
/// rather than overwhelm it. Each line is a paraphrase or direct line
/// from a traditional Irish song in circulation before 1820.
const VERSES: &[&str] = &[
    "\"Is fada liom oíche fhírfhliuch…\" — the night is long and over-wet, she sings, and his bed is cold without me.",
    "\"A Róisín, ná bí brónach…\" — Róisín, do not be sorrowful; the friars are on the sea.",
    "\"The summer is gone and the leaves they are falling\" — sung low, as if the room is not meant to hear.",
    "\"My love is like the bonny broom\" — a Scots tune carried by a sailor, and the pub will take it tonight.",
    "\"I will go down to yonder town\" — a wandering song, each verse a different parish.",
    "\"Táim 'mo shuí ó éirigh na gealaí\" — I have sat awake since the moon rose, and the cock will not crow for me.",
    "\"An cuimhin leat an oíche úd?\" — do you remember that night, she asks, and the fire hisses an answer.",
    "\"A Dhia na ngrást\" — God of graces, he sings, and the bar leans a little closer.",
];

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn is_session_hour_covers_evening_window() {
        assert!(is_session_hour(TimeOfDay::Dusk));
        assert!(is_session_hour(TimeOfDay::Night));
        assert!(is_session_hour(TimeOfDay::Midnight));
    }

    #[test]
    fn is_session_hour_rejects_daytime() {
        assert!(!is_session_hour(TimeOfDay::Dawn));
        assert!(!is_session_hour(TimeOfDay::Morning));
        assert!(!is_session_hour(TimeOfDay::Midday));
        assert!(!is_session_hour(TimeOfDay::Afternoon));
    }

    #[test]
    fn same_seed_yields_same_vignette() {
        let a = vignette_from_seed(12345, Weather::Clear, Season::Summer);
        let b = vignette_from_seed(12345, Weather::Clear, Season::Summer);
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_usually_differ() {
        // Two non-adjacent seeds should very likely pick different
        // (musician, tune) pairs; assert on at least one axis changing.
        let a = vignette_from_seed(1, Weather::Clear, Season::Summer);
        let b = vignette_from_seed(999_999, Weather::Clear, Season::Summer);
        assert!(a.musician != b.musician || a.tune != b.tune);
    }

    #[test]
    fn session_seed_is_stable_across_calls() {
        let d = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        assert_eq!(session_seed(d, 2), session_seed(d, 2));
    }

    #[test]
    fn session_seed_varies_by_date() {
        let d1 = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        let d2 = NaiveDate::from_ymd_opt(1820, 7, 16).unwrap();
        assert_ne!(session_seed(d1, 2), session_seed(d2, 2));
    }

    #[test]
    fn session_seed_varies_by_location() {
        let d = NaiveDate::from_ymd_opt(1820, 7, 15).unwrap();
        assert_ne!(session_seed(d, 2), session_seed(d, 3));
    }

    #[test]
    fn singer_seeds_produce_a_verse() {
        // Walk a window of seeds and assert that at least one lands on a
        // singer (three of ten musicians). With a uniform mixer, missing
        // a singer in 30 tries would be astronomically unlikely — if this
        // ever fires, the seed distribution has broken, not the table.
        let mut saw_verse = false;
        for s in 0..30u64 {
            let v = vignette_from_seed(s, Weather::Clear, Season::Summer);
            if v.verse.is_some() {
                saw_verse = true;
                break;
            }
        }
        assert!(saw_verse, "no singer appeared in 30 seeds");
    }

    #[test]
    fn instrumentalist_seeds_produce_no_verse() {
        // Seed 0 lands on the first musician (an old fiddler). An
        // instrumentalist must never carry a verse — that would read as
        // a singer playing an instrument.
        let v = vignette_from_seed(0, Weather::Clear, Season::Summer);
        assert!(!v.musician.starts_with("A woman's voice"));
        assert!(v.verse.is_none());
    }

    #[test]
    fn storm_overrides_season_in_ambient() {
        let v_clear = vignette_from_seed(42, Weather::Clear, Season::Summer);
        let v_storm = vignette_from_seed(42, Weather::Storm, Season::Summer);
        // Same seed, only weather differs → same musician + tune, but
        // ambient must shift to the storm line.
        assert_eq!(v_clear.musician, v_storm.musician);
        assert_eq!(v_clear.tune, v_storm.tune);
        assert_ne!(v_clear.ambient, v_storm.ambient);
        assert!(v_storm.ambient.contains("wind"));
    }

    #[test]
    fn tune_line_is_a_complete_sentence() {
        // Musician + tune must compose into a readable sentence; the
        // tune_line starts with `strikes up` and ends with a full stop.
        let v = vignette_from_seed(7, Weather::Clear, Season::Summer);
        assert!(v.tune.starts_with("strikes up "));
        assert!(v.tune.ends_with('.'));
    }

    #[test]
    fn every_table_entry_is_non_empty() {
        // Guard against an editor accidentally leaving a trailing comma
        // that splits a line — every table entry must be a real line.
        for m in MUSICIANS {
            assert!(!m.is_empty(), "empty musician entry");
        }
        for t in TUNES {
            assert!(!t.is_empty(), "empty tune entry");
        }
        for v in VERSES {
            assert!(!v.is_empty(), "empty verse entry");
        }
    }
}
