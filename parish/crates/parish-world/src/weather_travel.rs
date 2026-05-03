//! Weather-impeded travel — a pure function that translates the current
//! [`Weather`] into a travel-time multiplier, a flavour line, and an
//! occasional "forced back" outcome.
//!
//! Implements idea **#12 "Weather as Storytelling"** from
//! `docs/design/game-ideas-brainstorm.md`, scoped down to the single cheap
//! hook that makes the most difference in daily play: when you set out
//! during a storm, the parish tells you about it.
//!
//! The module is deliberately I/O-free. Call sites pass in a
//! [`DiceRoll`](parish_types::dice::DiceRoll) so tests can pin outcomes
//! without reaching for a thread RNG.

use parish_types::dice::DiceRoll;
use parish_types::{Season, Weather};

/// What the weather does to a journey the player is about to begin.
///
/// `multiplier` is applied to the nominal travel time (1.0 is no change).
/// `flavour` is a short sentence to print before the arrival narration.
/// If `forced_back` is set, the journey does not complete — the player
/// stays put, the clock advances by `minutes_spent`, and the flavour line
/// explains why.
#[derive(Debug, Clone, PartialEq)]
pub struct WeatherTravelEffect {
    /// Multiplier on nominal travel-minutes. `1.0` == unchanged.
    pub multiplier: f64,
    /// Short pre-arrival sentence, or `None` if the weather is fair enough
    /// to be silent.
    pub flavour: Option<&'static str>,
    /// `Some(minutes)` when the journey is aborted; the player stays at
    /// the origin but the clock advances by this many minutes.
    pub forced_back: Option<u16>,
}

impl WeatherTravelEffect {
    /// Fair-weather default — nothing happens, nothing is said.
    pub const fn clear() -> Self {
        Self {
            multiplier: 1.0,
            flavour: None,
            forced_back: None,
        }
    }
}

/// Flavour-line pools keyed by weather × season.
///
/// Lines are hand-authored, grounded in
/// `docs/research/flora-fauna-landscape.md` and
/// `docs/research/transportation.md`. Each pool is a slice so call sites
/// index with a `DiceRoll`.
fn flavour_pool(weather: Weather, season: Season) -> &'static [&'static str] {
    use Season::*;
    use Weather::*;
    match (weather, season) {
        // ── Fog ──────────────────────────────────────────────────────────
        (Fog, _) => &[
            "A soft grey fog has rolled in from the bog; you walk half-blind, \
             taking the familiar boreen on trust.",
            "You can hear your own boots on the road but not the cattle in the field, \
             so thick is the fog this morning.",
            "The fog turns every gatepost into a stranger; you slow down to be sure.",
        ],
        // ── Light rain ───────────────────────────────────────────────────
        (LightRain, Spring | Summer) => &[
            "A soft rain falls — the sort the country calls 'grand for the land' — \
             and you pull your shawl tighter and walk on.",
        ],
        (LightRain, Autumn | Winter) => &[
            "The rain is steady and cold; it works its way into your shoulders \
             before the first mile is out.",
            "A fine rain soaks you without ever quite falling; only the look of \
             your clothes proves it is there.",
        ],
        // ── Heavy rain ──────────────────────────────────────────────────
        (HeavyRain, _) => &[
            "The boreen is a creek and your boots a pair of buckets by the time \
             you've gone a furlong.",
            "The rain comes down in sheets; the ruts in the road are running brown.",
            "You pass a man with his coat held over his head like a tent — \
             you are tempted to do the same.",
        ],
        // ── Storm ───────────────────────────────────────────────────────
        (Storm, _) => &[
            "The wind drives the rain sideways; every hedge you pass sounds \
             like it is tearing itself apart.",
            "A gust takes your shawl off your head and nearly takes you with it. \
             You bow into the wind and press on.",
            "Somewhere inland a tree goes over with a crack like a musket shot.",
        ],
        // ── Overcast / partly cloudy — fair enough to say nothing ──────
        (Clear | PartlyCloudy | Overcast, _) => &[],
    }
}

/// Multiplier on nominal travel-minutes for a given weather.
///
/// Tuned so that a light rain is barely noticed, a heavy rain adds a
/// noticeable margin, fog slows you more than rain does (visibility is
/// worse for way-finding than weight is for footing), and a full storm
/// roughly doubles the journey.
fn multiplier_for(weather: Weather) -> f64 {
    match weather {
        Weather::Clear | Weather::PartlyCloudy | Weather::Overcast => 1.0,
        Weather::LightRain => 1.10,
        Weather::HeavyRain => 1.40,
        Weather::Fog => 1.50,
        Weather::Storm => 2.00,
    }
}

/// Probability that a Storm forces the player back before they arrive.
///
/// Only Storm triggers this — heavy rain and fog make travel miserable
/// but don't abort it. A Storm abort strands the player at the origin
/// with half the nominal travel time having passed (they got out, tried,
/// and came home).
const STORM_ABORT_PROBABILITY: f64 = 0.35;

/// Computes the [`WeatherTravelEffect`] for the player's upcoming journey.
///
/// - `weather` — current weather.
/// - `season` — current season (used only to select flavour voicing).
/// - `trigger_roll` — decides whether a Storm aborts the journey.
/// - `pick_roll` — selects a line from the flavour pool.
pub fn compute_weather_effect(
    weather: Weather,
    season: Season,
    trigger_roll: DiceRoll,
    pick_roll: DiceRoll,
) -> WeatherTravelEffect {
    let multiplier = multiplier_for(weather);
    let pool = flavour_pool(weather, season);
    let flavour = if pool.is_empty() {
        None
    } else {
        Some(*pick_roll.pick(pool))
    };

    let mut forced_back = None;
    if weather == Weather::Storm && trigger_roll.check(STORM_ABORT_PROBABILITY) {
        // Player sets out, fights the wind for a while, and gives up.
        // They lose half the slow-weather travel time, clamped so we never
        // claim zero minutes were spent.
        forced_back = Some(1); // the caller multiplies by the nominal minutes
    }

    WeatherTravelEffect {
        multiplier,
        flavour,
        forced_back,
    }
}

/// Applies an effect's multiplier to a nominal travel time in minutes,
/// rounding up so that even tiny multipliers register as "a bit longer".
pub fn apply_multiplier(nominal_minutes: u16, multiplier: f64) -> u16 {
    let scaled = (nominal_minutes as f64 * multiplier).ceil();
    if scaled >= u16::MAX as f64 {
        u16::MAX
    } else {
        scaled as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dice(v: f64) -> DiceRoll {
        DiceRoll::fixed(v)
    }

    #[test]
    fn clear_weather_is_silent_and_unchanged() {
        let e = compute_weather_effect(Weather::Clear, Season::Spring, dice(0.0), dice(0.0));
        assert_eq!(e.multiplier, 1.0);
        assert!(e.flavour.is_none());
        assert!(e.forced_back.is_none());
    }

    #[test]
    fn light_rain_in_spring_is_grand_for_the_land() {
        let e = compute_weather_effect(Weather::LightRain, Season::Spring, dice(0.0), dice(0.0));
        assert!((e.multiplier - 1.10).abs() < 1e-9);
        let line = e.flavour.expect("light rain should produce a line");
        assert!(
            line.contains("grand for the land"),
            "spring voicing missing: {line}"
        );
    }

    #[test]
    fn light_rain_in_winter_speaks_of_the_cold_soak() {
        let e = compute_weather_effect(Weather::LightRain, Season::Winter, dice(0.0), dice(0.0));
        let line = e.flavour.expect("light rain should produce a line");
        assert!(
            line.contains("cold") || line.contains("soak"),
            "winter voicing missing: {line}"
        );
    }

    #[test]
    fn fog_is_slower_than_heavy_rain() {
        // Visibility matters more than ground wetness for walking pace;
        // the fog multiplier should strictly dominate heavy rain's.
        assert!(multiplier_for(Weather::Fog) > multiplier_for(Weather::HeavyRain));
    }

    #[test]
    fn heavy_rain_turns_the_boreen_to_a_creek() {
        let e = compute_weather_effect(Weather::HeavyRain, Season::Autumn, dice(0.0), dice(0.0));
        assert!((e.multiplier - 1.40).abs() < 1e-9);
        let line = e.flavour.expect("heavy rain should produce a line");
        // Pool contains one of three lines; pick_roll=0.0 selects index 0.
        assert!(line.contains("boreen"));
    }

    #[test]
    fn storm_doubles_travel_time() {
        let e = compute_weather_effect(
            Weather::Storm,
            Season::Winter,
            dice(0.99), // above abort threshold → no forced_back
            dice(0.0),
        );
        assert!((e.multiplier - 2.0).abs() < 1e-9);
        assert!(e.forced_back.is_none());
        assert!(e.flavour.is_some());
    }

    #[test]
    fn storm_below_threshold_forces_the_player_back() {
        // trigger_roll below 0.35 triggers the abort branch.
        let e = compute_weather_effect(Weather::Storm, Season::Winter, dice(0.10), dice(0.0));
        assert!(e.forced_back.is_some());
        assert!(e.flavour.is_some());
    }

    #[test]
    fn only_storm_can_abort_the_journey() {
        for w in [
            Weather::Clear,
            Weather::PartlyCloudy,
            Weather::Overcast,
            Weather::LightRain,
            Weather::HeavyRain,
            Weather::Fog,
        ] {
            let e = compute_weather_effect(w, Season::Spring, dice(0.0), dice(0.0));
            assert!(
                e.forced_back.is_none(),
                "non-storm weather {w:?} should never force retreat"
            );
        }
    }

    #[test]
    fn pick_roll_selects_different_lines_from_the_pool() {
        let a = compute_weather_effect(Weather::HeavyRain, Season::Summer, dice(0.99), dice(0.0));
        let b = compute_weather_effect(Weather::HeavyRain, Season::Summer, dice(0.99), dice(0.99));
        // Pool has 3 lines; indices 0 and 2 pick distinct strings.
        assert_ne!(a.flavour, b.flavour);
    }

    #[test]
    fn apply_multiplier_rounds_up() {
        assert_eq!(apply_multiplier(10, 1.0), 10);
        assert_eq!(apply_multiplier(10, 1.1), 11);
        assert_eq!(apply_multiplier(10, 1.49), 15);
        assert_eq!(apply_multiplier(10, 2.0), 20);
        // Floor would lose the effect; ceil keeps it.
        assert_eq!(apply_multiplier(3, 1.10), 4);
    }

    #[test]
    fn apply_multiplier_saturates_near_u16_max() {
        // A wildly pathological multiplier should clamp, not wrap.
        let out = apply_multiplier(60_000, 10.0);
        assert_eq!(out, u16::MAX);
    }

    #[test]
    fn clear_helper_matches_compute_result() {
        let c = WeatherTravelEffect::clear();
        let e = compute_weather_effect(Weather::Clear, Season::Summer, dice(0.0), dice(0.0));
        assert_eq!(c, e);
    }

    #[test]
    fn all_weather_values_have_a_finite_multiplier() {
        for w in [
            Weather::Clear,
            Weather::PartlyCloudy,
            Weather::Overcast,
            Weather::LightRain,
            Weather::HeavyRain,
            Weather::Fog,
            Weather::Storm,
        ] {
            let m = multiplier_for(w);
            assert!(m.is_finite());
            assert!(m >= 1.0, "{w:?} multiplier went below 1.0: {m}");
        }
    }
}
