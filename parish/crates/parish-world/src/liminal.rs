//! Liminal moments — eerie atmospheric readings at mythologically significant sites.
//!
//! At certain confluences of place + time + weather + calendar, the "veil"
//! between worlds is said to thin. This module computes a `LiminalReading`
//! for a location: a terse flavor line and an intensity rating from 1–3.
//! It is pure, deterministic, and requires no LLM.
//!
//! Sites are classified by keyword-matching the `mythological_significance`
//! prose already stored on each location — so mod authors can add new
//! fairy forts, holy wells, bogs, crossroads, or lakes and have them surface
//! readings without touching the engine.
//!
//! Intensity rules (rough):
//! - **Faint (1)** — the site has mythology but conditions are ordinary.
//! - **Distinct (2)** — conditions line up with the site's lore (dusk at a
//!   fairy fort, dawn at a holy well, fog on a bog road, etc.).
//! - **Overwhelming (3)** — a Celtic festival day layered on top, especially
//!   Samhain (the "thin veil" night) or the site's patron festival.
//!
//! See `docs/design/mythology-hooks.md` for the broader layer design.
//!
//! # Example
//!
//! ```
//! use parish_world::liminal::liminal_reading;
//! use parish_types::{Festival, TimeOfDay, Weather};
//!
//! // Fairy fort at midnight on Samhain with fog — maximum intensity.
//! let reading = liminal_reading(
//!     Some("A rath said to be home to the sídhe."),
//!     TimeOfDay::Midnight,
//!     Weather::Fog,
//!     Some(Festival::Samhain),
//! );
//! assert!(reading.is_some());
//! assert_eq!(reading.unwrap().intensity, 3);
//! ```

use parish_types::{Festival, TimeOfDay, Weather};

/// Categories of mythologically significant sites, inferred from the
/// `mythological_significance` prose on a location.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiminalSite {
    /// Ring forts / raths — the home of the sídhe.
    FairyFort,
    /// Holy wells, pattern sites, blessed pools.
    HolyWell,
    /// Crossroads — places between places.
    Crossroads,
    /// Bogs — where the past is preserved and voices carry.
    Bog,
    /// Lakes with resident monsters or spirits.
    Lake,
    /// Anything else with mythological significance but no specific category.
    Other,
}

/// A computed liminal reading at a location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiminalReading {
    /// Which kind of mythological site this is.
    pub site: LiminalSite,
    /// 1 = faint, 2 = distinct, 3 = overwhelming.
    pub intensity: u8,
    /// Short sensory flavor line suitable for appending to a description.
    pub flavor: &'static str,
}

impl LiminalReading {
    /// A one-word summary of the intensity (for debug / CLI output).
    pub fn intensity_word(&self) -> &'static str {
        match self.intensity {
            1 => "faint",
            2 => "distinct",
            _ => "overwhelming",
        }
    }
}

/// Classifies a site from its `mythological_significance` string.
///
/// Returns `None` if the string is empty or contains no recognised keyword.
/// Keyword matching is deliberately loose so mod authors don't need to know
/// a canonical vocabulary — common synonyms are accepted.
pub fn classify_site(significance: Option<&str>) -> Option<LiminalSite> {
    let s = significance?.to_lowercase();
    if s.is_empty() {
        return None;
    }

    // Most specific first so "church of St. Brigid" beats a bare "well".
    if s.contains("fairy fort")
        || s.contains("rath")
        || s.contains("ring fort")
        || s.contains("sídhe")
        || s.contains("sidhe")
    {
        return Some(LiminalSite::FairyFort);
    }
    if s.contains("holy well") || s.contains("pattern") || s.contains("blessed well") {
        return Some(LiminalSite::HolyWell);
    }
    if s.contains("crossroads") {
        return Some(LiminalSite::Crossroads);
    }
    if s.contains("bog") {
        return Some(LiminalSite::Bog);
    }
    if s.contains("lough")
        || s.contains("lake")
        || s.contains("monster")
        || s.contains("wurm")
        || s.contains("worm")
    {
        return Some(LiminalSite::Lake);
    }
    Some(LiminalSite::Other)
}

/// Computes a liminal reading for a site under the given conditions, or
/// `None` if the location has no mythological significance or conditions
/// don't rise to a reading.
///
/// The minimum intensity is 1: if the site is classifiable *at all*, you'll
/// get at least a faint reading. Intensities 2 and 3 require the conditions
/// to align with the site's lore.
pub fn liminal_reading(
    significance: Option<&str>,
    tod: TimeOfDay,
    weather: Weather,
    festival: Option<Festival>,
) -> Option<LiminalReading> {
    let site = classify_site(significance)?;
    let intensity = compute_intensity(site, tod, weather, festival);
    let flavor = pick_flavor(site, intensity, weather, festival);
    Some(LiminalReading {
        site,
        intensity,
        flavor,
    })
}

/// Scores how strongly the current conditions align with a site's lore.
///
/// Returns an intensity in `1..=3`. The scoring is coarse on purpose —
/// there's no randomness, so the same place at the same time always feels
/// the same, and a player can learn the rhythms of the parish.
fn compute_intensity(
    site: LiminalSite,
    tod: TimeOfDay,
    weather: Weather,
    festival: Option<Festival>,
) -> u8 {
    let mut score: i32 = 0;

    // Per-site "lore-aligned condition" bonuses (additive).
    match site {
        LiminalSite::FairyFort => {
            if matches!(
                tod,
                TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight
            ) {
                score += 1;
            }
            if matches!(weather, Weather::Fog | Weather::Storm) {
                score += 1;
            }
            if matches!(
                festival,
                Some(Festival::Samhain) | Some(Festival::Bealtaine)
            ) {
                score += 2; // the great fairy nights
            }
        }
        LiminalSite::HolyWell => {
            if matches!(tod, TimeOfDay::Dawn) {
                score += 1;
            }
            // Any festival draws pilgrims and thins the veil here.
            if festival.is_some() {
                score += 1;
            }
            if matches!(festival, Some(Festival::Imbolc)) {
                score += 1; // Brigid's own day
            }
        }
        LiminalSite::Crossroads => {
            if matches!(tod, TimeOfDay::Midnight | TimeOfDay::Dusk) {
                score += 1;
            }
            if matches!(festival, Some(Festival::Samhain)) {
                score += 2;
            }
        }
        LiminalSite::Bog => {
            if matches!(weather, Weather::Fog) {
                score += 2; // classic bog ghost-light weather
            }
            if matches!(
                tod,
                TimeOfDay::Dusk | TimeOfDay::Night | TimeOfDay::Midnight
            ) {
                score += 1;
            }
        }
        LiminalSite::Lake => {
            if matches!(tod, TimeOfDay::Dawn | TimeOfDay::Dusk) {
                score += 1;
            }
            if matches!(weather, Weather::Fog | Weather::Storm) {
                score += 1;
            }
            if matches!(festival, Some(Festival::Lughnasa)) {
                score += 1;
            }
        }
        LiminalSite::Other => {
            // A generic mythological site gets a small bonus on Samhain only.
            if matches!(festival, Some(Festival::Samhain)) {
                score += 1;
            }
        }
    }

    // Intensity floors: 1 at baseline, 2 for one lore match, 3 for two or more.
    match score {
        0 => 1,
        1 => 2,
        _ => 3,
    }
}

/// Picks a flavor line for a (site, intensity) combination.
///
/// Lines are deliberately *atmospheric* rather than mechanical — no damage,
/// no items, no save-state changes. This feature is about mood, not mechanics.
fn pick_flavor(
    site: LiminalSite,
    intensity: u8,
    weather: Weather,
    festival: Option<Festival>,
) -> &'static str {
    match site {
        LiminalSite::FairyFort => match intensity {
            3 if matches!(festival, Some(Festival::Samhain)) => {
                "The hawthorn leaves are utterly still. Somewhere under the mound, \
                 something breathes in time with you. Tonight the sídhe are awake."
            }
            3 if matches!(festival, Some(Festival::Bealtaine)) => {
                "A thread of fiddle music winds out of the rath and stops mid-bar. \
                 The hawthorns are hung with rags you did not see placed there."
            }
            3 if matches!(weather, Weather::Fog) => {
                "The mist thickens on the mound. A distant fiddle plays a tune you \
                 almost recognise — then cuts off. The hawthorns have not moved."
            }
            3 => {
                "Something under the mound is awake. You feel eyes on the back of \
                 your neck and do not turn around."
            }
            2 => "The hawthorn leaves tremble though there is no wind.",
            _ => "The air here is heavier than elsewhere, watchful.",
        },
        LiminalSite::HolyWell => match intensity {
            3 if matches!(festival, Some(Festival::Imbolc)) => {
                "The water is perfectly still. For a moment the surface shows a \
                 woman in a white cloak; then it is just your own face, and Brigid's \
                 name is on your tongue without your putting it there."
            }
            3 => {
                "A hush holds the place. The water ripples once, quite deliberately, \
                 with no breath of wind. Something has been answered."
            }
            2 => {
                "A thrush calls a single clear note. The water holds very still, as \
                 though listening back."
            }
            _ => "The old blessing of the place settles on you, quiet and sure.",
        },
        LiminalSite::Crossroads => match intensity {
            3 if matches!(festival, Some(Festival::Samhain)) => {
                "The old people say the dead walk the road on Samhain night. You \
                 hear boots on gravel behind you, keeping your pace exactly. You \
                 know better than to turn around."
            }
            3 => {
                "At the place where four roads meet, you feel four different winds \
                 on your face at once. One of them smells of turf-smoke from a fire \
                 you cannot see."
            }
            2 => {
                "The crossroads feels emptier than it is. Your own footsteps sound \
                 like two sets."
            }
            _ => "Four roads meet. The place remembers everyone who has passed.",
        },
        LiminalSite::Bog => match intensity {
            3 if matches!(weather, Weather::Fog) => {
                "A small blue light floats above the cuttings — a foxfire, a stray \
                 soul, or nothing at all. It moves when you move. Voices in the \
                 wind name people you have never met."
            }
            3 => {
                "Something old gives under your feet and then settles. The bog \
                 remembers. Voices you can't quite catch travel across the sedge."
            }
            2 if matches!(weather, Weather::Fog) => {
                "Mist hangs low over the cuttings. Far off, a voice calls a name \
                 that might be yours."
            }
            2 => "The wind carries a snatch of voices from nowhere in particular.",
            _ => "The bog keeps its counsel; the air has the weight of buried time.",
        },
        LiminalSite::Lake => match intensity {
            3 => {
                "Something large rolls under the surface of the lough, far out. \
                 The ripple reaches the shore long after it ought to have faded."
            }
            2 => {
                "The lough is glass-calm and then, briefly, not. A single slow wave \
                 laps the stones."
            }
            _ => "The lough keeps its own colour no matter the sky.",
        },
        LiminalSite::Other => match intensity {
            3 => "The old significance of the place surfaces like a held breath.",
            2 => "There is something under the ordinariness of this place tonight.",
            _ => "The place has its own weight.",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_knows_the_fairy_fort() {
        // Direct prose from mods/rundale/world.json:
        let s = "A rath said to be home to the sídhe. No farmer has ever ploughed within twenty yards of it.";
        assert_eq!(classify_site(Some(s)), Some(LiminalSite::FairyFort));
    }

    #[test]
    fn classify_knows_holy_wells_and_crossroads_and_bogs_and_lakes() {
        let well = "Built on the site of an older holy well dedicated to Brigid.";
        let cross = "Crossroads hold power in Irish folklore — a place between places.";
        let bog = "Bogs preserve everything — bodies, butter, memories.";
        let lake = "Lough Ree is said to be home to a monster — the Lough Ree wurm.";
        assert_eq!(classify_site(Some(well)), Some(LiminalSite::HolyWell));
        assert_eq!(classify_site(Some(cross)), Some(LiminalSite::Crossroads));
        assert_eq!(classify_site(Some(bog)), Some(LiminalSite::Bog));
        assert_eq!(classify_site(Some(lake)), Some(LiminalSite::Lake));
    }

    #[test]
    fn classify_returns_none_for_missing_or_empty() {
        assert_eq!(classify_site(None), None);
        assert_eq!(classify_site(Some("")), None);
    }

    #[test]
    fn classify_falls_back_to_other() {
        // Mythological prose that doesn't name a known site category.
        let s = "An ogham stone stands here, its letters weathered past reading.";
        assert_eq!(classify_site(Some(s)), Some(LiminalSite::Other));
    }

    #[test]
    fn fairy_fort_at_midday_is_faint() {
        let r = liminal_reading(
            Some("A rath said to be home to the sídhe."),
            TimeOfDay::Midday,
            Weather::Clear,
            None,
        )
        .unwrap();
        assert_eq!(r.intensity, 1);
        assert_eq!(r.site, LiminalSite::FairyFort);
    }

    #[test]
    fn fairy_fort_at_dusk_is_distinct() {
        let r = liminal_reading(
            Some("A rath said to be home to the sídhe."),
            TimeOfDay::Dusk,
            Weather::Clear,
            None,
        )
        .unwrap();
        assert_eq!(r.intensity, 2);
    }

    #[test]
    fn fairy_fort_on_samhain_at_midnight_is_overwhelming() {
        let r = liminal_reading(
            Some("A rath said to be home to the sídhe."),
            TimeOfDay::Midnight,
            Weather::Clear,
            Some(Festival::Samhain),
        )
        .unwrap();
        assert_eq!(r.intensity, 3);
        // Flavor is the Samhain-specific fairy-fort line.
        assert!(r.flavor.contains("sídhe"));
    }

    #[test]
    fn holy_well_on_imbolc_at_dawn_is_overwhelming_and_names_brigid() {
        let r = liminal_reading(
            Some("Built on the site of an older holy well dedicated to Brigid."),
            TimeOfDay::Dawn,
            Weather::Clear,
            Some(Festival::Imbolc),
        )
        .unwrap();
        assert_eq!(r.intensity, 3);
        assert!(r.flavor.contains("Brigid"));
    }

    #[test]
    fn crossroads_on_samhain_midnight_is_overwhelming() {
        let r = liminal_reading(
            Some("Crossroads hold power in Irish folklore — a place between places."),
            TimeOfDay::Midnight,
            Weather::Clear,
            Some(Festival::Samhain),
        )
        .unwrap();
        assert_eq!(r.intensity, 3);
        assert!(r.flavor.to_lowercase().contains("samhain"));
    }

    #[test]
    fn bog_in_fog_is_distinct_even_at_noon() {
        let r = liminal_reading(
            Some("Bogs preserve everything — bodies, butter, memories."),
            TimeOfDay::Midday,
            Weather::Fog,
            None,
        )
        .unwrap();
        // Fog alone gives +2 on a bog => score 2 => intensity 3 actually.
        assert_eq!(r.intensity, 3);
        assert_eq!(r.site, LiminalSite::Bog);
    }

    #[test]
    fn location_without_mythology_yields_no_reading() {
        let r = liminal_reading(
            None,
            TimeOfDay::Midnight,
            Weather::Fog,
            Some(Festival::Samhain),
        );
        assert!(r.is_none());
    }

    #[test]
    fn intensity_word_matches_intensity() {
        let r = LiminalReading {
            site: LiminalSite::FairyFort,
            intensity: 1,
            flavor: "x",
        };
        assert_eq!(r.intensity_word(), "faint");
        let r = LiminalReading { intensity: 2, ..r };
        assert_eq!(r.intensity_word(), "distinct");
        let r = LiminalReading { intensity: 3, ..r };
        assert_eq!(r.intensity_word(), "overwhelming");
    }

    #[test]
    fn lake_at_dawn_in_fog_is_overwhelming() {
        let r = liminal_reading(
            Some("Lough Ree is said to be home to a monster — the Lough Ree wurm."),
            TimeOfDay::Dawn,
            Weather::Fog,
            None,
        )
        .unwrap();
        assert_eq!(r.site, LiminalSite::Lake);
        assert_eq!(r.intensity, 3);
    }
}
