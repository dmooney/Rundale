//! Hedgerow and bog foraging.
//!
//! The player can `/forage` at an outdoor location to hunt for wild food —
//! blackberries and sloes from the hedgerows in autumn, watercress from a
//! clean stream in spring, bog cotton and bilberries from the raised bog,
//! wild garlic in a shaded lane. What is found depends on where they are,
//! what season it is, what time of day it is, and what the weather is doing.
//!
//! A few habitat types (the fairy fort, the holy well) are taboo and refuse
//! the foraging attempt with a superstitious line of text — the player is
//! not going to risk the sídhe for a handful of hazelnuts.
//!
//! This module is deliberately self-contained: it takes plain values in and
//! returns a [`ForageOutcome`]. Wiring into the `/forage` command lives in
//! `parish-core/src/ipc/commands.rs`.
//!
//! See `docs/research/flora-fauna-landscape.md` for the background that
//! shaped the seasonal tables.

use parish_types::{Season, TimeOfDay, Weather};

use crate::graph::LocationData;

/// The kind of ground the player is on for foraging purposes.
///
/// Derived from the location's name, aliases, and indoor flag by
/// [`classify_habitat`]. Indoor locations always classify as
/// [`Habitat::Unforageable`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Habitat {
    /// Thorny hedgerows of whitethorn, blackthorn, hazel — the richest
    /// hedgerow fruit and nut habitat.
    Hedgerow,
    /// Raised bog with sphagnum, heather, bog cotton, and bilberry.
    Bog,
    /// Lakeshore or river edge — watercress, reeds, rushes.
    Lakeside,
    /// Open meadow, field, or drumlin pasture.
    Meadow,
    /// Liminal sacred ground (fairy fort, holy well). Foraging is taboo.
    FairyPlace,
    /// A farmyard or built area where foraging is neither private nor
    /// polite.
    Farmyard,
    /// Anywhere foraging makes no sense (indoors, on a lime-kiln floor).
    Unforageable,
}

impl Habitat {
    /// A short label used in the command response for the curious player.
    pub fn label(self) -> &'static str {
        match self {
            Habitat::Hedgerow => "hedgerow",
            Habitat::Bog => "bog",
            Habitat::Lakeside => "lakeshore",
            Habitat::Meadow => "meadow",
            Habitat::FairyPlace => "taboo ground",
            Habitat::Farmyard => "farmyard",
            Habitat::Unforageable => "indoors",
        }
    }
}

/// Classifies a location's habitat for foraging.
///
/// Uses the location name and aliases as hints. Indoor locations are always
/// [`Habitat::Unforageable`]. This is intentionally a simple lookup — a mod
/// that adds new locations should either name them descriptively or extend
/// this function.
pub fn classify_habitat(loc: &LocationData) -> Habitat {
    if loc.indoor {
        return Habitat::Unforageable;
    }

    let haystack = {
        let mut s = loc.name.to_lowercase();
        for alias in &loc.aliases {
            s.push(' ');
            s.push_str(&alias.to_lowercase());
        }
        s
    };

    let contains = |needle: &str| haystack.contains(needle);

    // Order matters: check taboo/specific habitats before generic ones.
    if contains("fairy") || contains("holy well") {
        return Habitat::FairyPlace;
    }
    if contains("bog") {
        return Habitat::Bog;
    }
    if contains("lough") || contains("shore") || contains("bay") || contains("river") {
        return Habitat::Lakeside;
    }
    if contains("farm") {
        return Habitat::Farmyard;
    }
    if contains("hurling green") || contains("green") || contains("meadow") {
        return Habitat::Meadow;
    }
    if contains("lime kiln") || contains("forge") || contains("mill") {
        // Outdoor industrial sites — hedgerows at the edge, but classify
        // conservatively as unforageable so the player moves on.
        return Habitat::Unforageable;
    }
    if contains("crossroads")
        || contains("road")
        || contains("village")
        || contains("hedge school")
        || contains("church")
        || contains("office")
        || contains("shop")
        || contains("cottage")
    {
        // Open country with the ubiquitous Irish hedgerow at every verge.
        return Habitat::Hedgerow;
    }

    Habitat::Hedgerow
}

/// The result of one foraging attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForageOutcome {
    /// Prose description to append to the text log.
    pub description: String,
    /// How many in-game minutes the attempt consumed.
    pub minutes_elapsed: u32,
    /// `true` when the player came home with something edible. Used so the
    /// caller can log a summary or, later, credit an inventory.
    pub found_something: bool,
}

impl ForageOutcome {
    fn empty(description: impl Into<String>, minutes: u32) -> Self {
        Self {
            description: description.into(),
            minutes_elapsed: minutes,
            found_something: false,
        }
    }

    fn full(description: impl Into<String>, minutes: u32) -> Self {
        Self {
            description: description.into(),
            minutes_elapsed: minutes,
            found_something: true,
        }
    }
}

/// Attempts a forage at the given habitat under the current conditions.
///
/// `roll` is a random value in `0.0..1.0` used to pick one of several
/// flavour strings for the outcome. The same roll drives yield variation —
/// a very low roll on a good day still returns the "rich haul" line.
pub fn forage(
    habitat: Habitat,
    season: Season,
    time_of_day: TimeOfDay,
    weather: Weather,
    roll: f64,
) -> ForageOutcome {
    // ── Hard gates: refuse outright ──────────────────────────────────────
    if habitat == Habitat::Unforageable {
        return ForageOutcome::empty(
            "There's nothing to forage here. You'd need the open country for that.",
            0,
        );
    }
    if habitat == Habitat::FairyPlace {
        return ForageOutcome::empty(
            "You think better of it. Not here. The old people say nothing grown on this ground is \
             yours to take, and you aren't about to be the one to prove them wrong.",
            1,
        );
    }
    if habitat == Habitat::Farmyard {
        return ForageOutcome::empty(
            "This is someone's yard — not a place for a stranger to be picking at the hedges.",
            1,
        );
    }
    if matches!(time_of_day, TimeOfDay::Midnight) {
        return ForageOutcome::empty(
            "It's pitch dark. You'd only be groping blind and tearing your hands on thorns.",
            2,
        );
    }
    if matches!(weather, Weather::Storm) {
        return ForageOutcome::empty(
            "The wind is savage and the rain is coming sideways. Not a chance of foraging in this.",
            2,
        );
    }
    if matches!(weather, Weather::HeavyRain) && habitat == Habitat::Bog {
        return ForageOutcome::empty(
            "The bog is running with water and the sphagnum is near floating. You'd be up to your \
             knees in moments. Not today.",
            3,
        );
    }

    // ── Poor-visibility attenuation ─────────────────────────────────────
    let dim = matches!(time_of_day, TimeOfDay::Night | TimeOfDay::Dusk)
        || matches!(weather, Weather::Fog);

    // ── Seasonal menu per habitat ────────────────────────────────────────
    let menu = seasonal_menu(habitat, season);
    if menu.is_empty() {
        let msg = match (habitat, season) {
            (Habitat::Hedgerow, Season::Winter) => {
                "The hedgerows are bare sticks and bramble. A few shrivelled rosehips cling on — \
                 not worth the scratches."
            }
            (Habitat::Bog, Season::Winter) => {
                "The bog is a grey waste. Nothing but wet moss and the wind. You turn back."
            }
            (Habitat::Lakeside, Season::Winter) => {
                "The shore is grey, the reeds are bent and brown, and what watercress there was \
                 has long since been cut back by the frosts."
            }
            (Habitat::Meadow, Season::Winter) => {
                "The meadow is flattened and soaked. There's nothing to find here in the cold."
            }
            _ => "You look about, but nothing worth the taking is in season here.",
        };
        return ForageOutcome::empty(msg, 10);
    }

    let idx = ((roll.clamp(0.0, 0.999_999) * menu.len() as f64) as usize).min(menu.len() - 1);
    let find = menu[idx];

    // ── Assemble the prose ──────────────────────────────────────────────
    let scene = scene_intro(habitat, season, time_of_day, weather);
    let verb = forage_verb(habitat);

    let (description, minutes) = if dim {
        (
            format!(
                "{} You {} for a while in the poor light and come away with {} — less than you \
                 might in broad day, but enough for the pot.",
                scene, verb, find
            ),
            25,
        )
    } else {
        (
            format!(
                "{} You {} carefully along the margins. After a while you have {} in your hand.",
                scene, verb, find
            ),
            20,
        )
    };

    ForageOutcome::full(description, minutes)
}

/// Picks a habitat-appropriate verb for the action.
fn forage_verb(habitat: Habitat) -> &'static str {
    match habitat {
        Habitat::Hedgerow => "work the hedgerow",
        Habitat::Bog => "pick your way across the bog",
        Habitat::Lakeside => "comb the shore",
        Habitat::Meadow => "cast about the field",
        _ => "look about",
    }
}

/// A sentence of scene-setting keyed to habitat, season, time-of-day, and
/// weather. Kept terse; the forage verb and find-string will follow.
fn scene_intro(
    habitat: Habitat,
    season: Season,
    time_of_day: TimeOfDay,
    weather: Weather,
) -> &'static str {
    let rain = matches!(
        weather,
        Weather::LightRain | Weather::HeavyRain | Weather::Fog
    );
    match (habitat, season) {
        (Habitat::Hedgerow, Season::Spring) if rain => {
            "The whitethorn is in bud and the lane is dripping."
        }
        (Habitat::Hedgerow, Season::Spring) => {
            "The whitethorn is hazing with new leaf and primroses star the bank."
        }
        (Habitat::Hedgerow, Season::Summer) if rain => {
            "The hedgerow is a dripping green tunnel, heavy with scent."
        }
        (Habitat::Hedgerow, Season::Summer) => {
            "The hedgerow is in full leaf and the lane hums with bees."
        }
        (Habitat::Hedgerow, Season::Autumn) if rain => {
            "The hedgerow is wet and blazing: russet leaves, black fruit, scarlet hip."
        }
        (Habitat::Hedgerow, Season::Autumn) => {
            "The hedgerow is heavy with fruit and the air is apple-sharp."
        }
        (Habitat::Hedgerow, Season::Winter) => {
            "The hedgerow is a lace of bare thorn against a white sky."
        }
        (Habitat::Bog, Season::Spring) => {
            "The bog is waking — cotton-grass heads just beginning to show above the moss."
        }
        (Habitat::Bog, Season::Summer) => {
            "The bog is alive with the drone of bees at the heather and the scent of sweetgale."
        }
        (Habitat::Bog, Season::Autumn) => {
            "The heather is going purple-brown and the bogland stretches out in every direction."
        }
        (Habitat::Bog, Season::Winter) => "The bog is a grey plain under a low sky.",
        (Habitat::Lakeside, Season::Spring) => {
            "The shore is loud with returning birds and the shallows run clear."
        }
        (Habitat::Lakeside, Season::Summer) => {
            "The water is glassy and the reeds are thick at the margin."
        }
        (Habitat::Lakeside, Season::Autumn) => {
            "The reeds are bleaching and the first geese are moving on the lough."
        }
        (Habitat::Lakeside, Season::Winter) => "The shore is cold and the water looks like slate.",
        (Habitat::Meadow, Season::Spring) => "The meadow is greening and the larks are up.",
        (Habitat::Meadow, Season::Summer) => "The meadow grass is tall enough to wade through.",
        (Habitat::Meadow, Season::Autumn) => "The meadow is gone to seed and stubble-pale.",
        (Habitat::Meadow, Season::Winter) => "The meadow is sodden and flat.",
        _ => {
            // Fallback plus night variants.
            if matches!(time_of_day, TimeOfDay::Night) {
                "The world is dim and close under the stars."
            } else {
                "You cast about the edges of the ground."
            }
        }
    }
}

/// Returns the list of possible "finds" for a given habitat and season.
///
/// Each string completes the sentence "…you have _____ in your hand." and
/// is chosen by the roll passed in to [`forage`]. Return value is empty
/// when nothing sensible is in season (winter hedgerows, etc.).
fn seasonal_menu(habitat: Habitat, season: Season) -> &'static [&'static str] {
    match (habitat, season) {
        (Habitat::Hedgerow, Season::Spring) => &[
            "a bunch of young nettle-tops for a spring soup",
            "a handful of wild garlic leaves, pungent and green",
            "a small bundle of hawthorn leaves — 'bread and cheese', the children call them",
            "a few sorrel leaves, sharp on the tongue",
        ],
        (Habitat::Hedgerow, Season::Summer) => &[
            "a few sprigs of elderflower, lacy and yellow-white",
            "a handful of hedge-woundwort and yarrow for a cut",
            "a small cap of wild strawberries, sun-warm",
            "a scattering of raspberries from a tangle in the ditch",
        ],
        (Habitat::Hedgerow, Season::Autumn) => &[
            "a cap of blackberries, your fingers stained purple",
            "a pocket of hazelnuts, still half in their green husks",
            "a handful of sloes from the blackthorn, dusted blue",
            "a few crab apples and a bunch of elderberries",
            "a heel of haws and a fistful of rosehips for the winter",
        ],
        (Habitat::Hedgerow, Season::Winter) => &[],

        (Habitat::Bog, Season::Spring) => &[
            "a small bundle of bog myrtle, the sweetgale bitter-fresh",
            "a few tender shoots of bog-bean from a dark pool",
        ],
        (Habitat::Bog, Season::Summer) => &[
            "a clutch of bilberries — fraochán, the old name — black-blue and sweet",
            "a handful of bog cotton, more for looking at than eating",
            "a strip of sphagnum, useful for a wound or a baby's bedding",
            "a sprig of heather in bloom",
        ],
        (Habitat::Bog, Season::Autumn) => &[
            "a last picking of bilberries, the plants reddening",
            "a sprig of dark-purple heather and a cut of turf-edge mushrooms",
            "a handful of cranberries from a mossy pool",
        ],
        (Habitat::Bog, Season::Winter) => &[],

        (Habitat::Lakeside, Season::Spring) => &[
            "a cold bunch of watercress from a clean runnel",
            "a handful of wild garlic from the damp bank",
            "a few young rushes — pith for a rushlight, stems for a chair",
        ],
        (Habitat::Lakeside, Season::Summer) => &[
            "a bunch of watercress and a few meadowsweet heads",
            "a cluster of wild mint from the water's edge",
            "a handful of reeds for thatching a small repair",
        ],
        (Habitat::Lakeside, Season::Autumn) => &[
            "a knot of watercress, yellowing now but still good",
            "a few hazelnuts from a tree leaning over the water",
            "a bundle of rushes bent by the wind",
        ],
        (Habitat::Lakeside, Season::Winter) => &[],

        (Habitat::Meadow, Season::Spring) => &[
            "a fistful of nettle-tops and young dandelion leaves",
            "a bunch of cowslips — don't tell the priest — and wood sorrel",
        ],
        (Habitat::Meadow, Season::Summer) => &[
            "a few field mushrooms from the dew",
            "a handful of yarrow and plantain for a poultice",
        ],
        (Habitat::Meadow, Season::Autumn) => &[
            "a cap of field mushrooms and a few late blackberries from the hedge",
            "a small stack of rushes for rushlights",
        ],
        (Habitat::Meadow, Season::Winter) => &[],

        (Habitat::FairyPlace, _) | (Habitat::Farmyard, _) | (Habitat::Unforageable, _) => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::LocationData;
    use parish_types::LocationId;

    fn loc(name: &str, indoor: bool, aliases: &[&str]) -> LocationData {
        LocationData {
            id: LocationId(0),
            name: name.to_string(),
            description_template: String::new(),
            indoor,
            public: true,
            connections: vec![],
            lat: 0.0,
            lon: 0.0,
            associated_npcs: vec![],
            mythological_significance: None,
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
            geo_kind: Default::default(),
            relative_to: None,
            geo_source: None,
        }
    }

    #[test]
    fn indoor_locations_are_unforageable() {
        let pub_ = loc("Darcy's Pub", true, &["pub"]);
        assert_eq!(classify_habitat(&pub_), Habitat::Unforageable);
    }

    #[test]
    fn fairy_fort_is_taboo() {
        let f = loc("The Fairy Fort", false, &["fort", "rath"]);
        assert_eq!(classify_habitat(&f), Habitat::FairyPlace);
    }

    #[test]
    fn holy_well_is_taboo() {
        let w = loc("The Holy Well", false, &["well"]);
        assert_eq!(classify_habitat(&w), Habitat::FairyPlace);
    }

    #[test]
    fn bog_road_is_bog() {
        let b = loc("The Bog Road", false, &["bog"]);
        assert_eq!(classify_habitat(&b), Habitat::Bog);
    }

    #[test]
    fn lough_shore_is_lakeside() {
        let s = loc("Lough Ree Shore", false, &["shore", "coast"]);
        assert_eq!(classify_habitat(&s), Habitat::Lakeside);
    }

    #[test]
    fn farm_is_farmyard() {
        let f = loc("Murphy's Farm", false, &["farm"]);
        assert_eq!(classify_habitat(&f), Habitat::Farmyard);
    }

    #[test]
    fn crossroads_is_hedgerow() {
        let c = loc("The Crossroads", false, &["crossroads"]);
        assert_eq!(classify_habitat(&c), Habitat::Hedgerow);
    }

    #[test]
    fn fairy_place_always_refused() {
        let out = forage(
            Habitat::FairyPlace,
            Season::Autumn,
            TimeOfDay::Afternoon,
            Weather::Clear,
            0.5,
        );
        assert!(!out.found_something);
        assert!(out.description.to_lowercase().contains("not here"));
    }

    #[test]
    fn storm_blocks_everywhere() {
        let out = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Midday,
            Weather::Storm,
            0.1,
        );
        assert!(!out.found_something);
        assert!(out.description.to_lowercase().contains("wind"));
    }

    #[test]
    fn midnight_blocks_everywhere() {
        let out = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Midnight,
            Weather::Clear,
            0.1,
        );
        assert!(!out.found_something);
        assert!(out.description.to_lowercase().contains("dark"));
    }

    #[test]
    fn heavy_rain_closes_the_bog() {
        let out = forage(
            Habitat::Bog,
            Season::Summer,
            TimeOfDay::Midday,
            Weather::HeavyRain,
            0.1,
        );
        assert!(!out.found_something);
        assert!(out.description.to_lowercase().contains("bog"));
    }

    #[test]
    fn winter_hedgerow_is_empty_but_open() {
        let out = forage(
            Habitat::Hedgerow,
            Season::Winter,
            TimeOfDay::Midday,
            Weather::Clear,
            0.5,
        );
        assert!(!out.found_something);
        assert!(out.minutes_elapsed >= 5);
    }

    #[test]
    fn autumn_hedgerow_yields_fruit() {
        let out = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Afternoon,
            Weather::Clear,
            0.0,
        );
        assert!(out.found_something);
        assert!(out.description.to_lowercase().contains("blackberries"));
        assert!(out.minutes_elapsed >= 15);
    }

    #[test]
    fn summer_bog_yields_bilberries() {
        let out = forage(
            Habitat::Bog,
            Season::Summer,
            TimeOfDay::Midday,
            Weather::PartlyCloudy,
            0.0,
        );
        assert!(out.found_something);
        assert!(out.description.to_lowercase().contains("bilberr"));
    }

    #[test]
    fn dusk_reduces_yield_but_does_not_block() {
        let out = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Dusk,
            Weather::Clear,
            0.0,
        );
        assert!(out.found_something);
        assert!(out.description.to_lowercase().contains("poor light"));
    }

    #[test]
    fn roll_extremes_are_safe() {
        let zero = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Afternoon,
            Weather::Clear,
            0.0,
        );
        let max = forage(
            Habitat::Hedgerow,
            Season::Autumn,
            TimeOfDay::Afternoon,
            Weather::Clear,
            0.999_999_9,
        );
        assert!(zero.found_something);
        assert!(max.found_something);
    }
}
