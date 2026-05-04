//! Dynamic weather state machine.
//!
//! Transitions weather over time based on season and randomness.
//! Adjacent-state transitions only — no jumping from Clear to Storm.
//! Minimum 2 game-hours between transitions to prevent rapid flipping.

use std::collections::VecDeque;

use chrono::{DateTime, Utc};
use rand::Rng;

use parish_types::{Season, Weather};

/// Maximum number of weather transitions to retain in history.
const HISTORY_CAPACITY: usize = 10;

/// Minimum duration in game-hours before a weather transition is allowed.
const DEFAULT_MIN_DURATION_HOURS: f64 = 2.0;

/// Maps each weather variant to an ordinal for adjacency calculations.
///
/// The main weather axis is:
///   Clear(0) ↔ PartlyCloudy(1) ↔ Overcast(2) ↔ LightRain(3) ↔ HeavyRain(4) ↔ Storm(5)
///
/// Fog is a special state reachable from PartlyCloudy or Overcast.
fn weather_ordinal(w: Weather) -> Option<u8> {
    match w {
        Weather::Clear => Some(0),
        Weather::PartlyCloudy => Some(1),
        Weather::Overcast => Some(2),
        Weather::LightRain => Some(3),
        Weather::HeavyRain => Some(4),
        Weather::Storm => Some(5),
        Weather::Fog => None, // off-axis
    }
}

/// Returns the weather variant for a given ordinal on the main axis.
fn weather_from_ordinal(ord: u8) -> Weather {
    match ord {
        0 => Weather::Clear,
        1 => Weather::PartlyCloudy,
        2 => Weather::Overcast,
        3 => Weather::LightRain,
        4 => Weather::HeavyRain,
        _ => Weather::Storm,
    }
}

/// Seasonal transition bias parameters.
struct SeasonalBias {
    /// Probability of moving toward clear (lower ordinal).
    clear_pull: f64,
    /// Probability of moving toward rain/storm (higher ordinal).
    rain_pull: f64,
    /// Probability of transitioning to/from fog.
    fog_chance: f64,
}

/// Returns the seasonal bias for weather transitions.
fn seasonal_bias(season: Season) -> SeasonalBias {
    match season {
        Season::Spring => SeasonalBias {
            clear_pull: 0.35,
            rain_pull: 0.30,
            fog_chance: 0.08,
        },
        Season::Summer => SeasonalBias {
            clear_pull: 0.50,
            rain_pull: 0.15,
            fog_chance: 0.05,
        },
        Season::Autumn => SeasonalBias {
            clear_pull: 0.20,
            rain_pull: 0.40,
            fog_chance: 0.12,
        },
        Season::Winter => SeasonalBias {
            clear_pull: 0.15,
            rain_pull: 0.45,
            fog_chance: 0.10,
        },
    }
}

/// Dynamic weather state machine that transitions over time.
///
/// Weather changes are evaluated once per game-hour after a minimum
/// duration has elapsed. Transitions follow adjacency rules — no
/// jumping from Clear directly to Storm.
pub struct WeatherEngine {
    /// Current weather state.
    current: Weather,
    /// Game time when the current state began.
    since: DateTime<Utc>,
    /// Minimum duration in game-hours before a transition is allowed.
    min_duration_hours: f64,
    /// Game-hour of the last transition check (to avoid multiple checks per hour).
    last_check_hour: Option<i64>,
    /// Ring buffer of recent weather transitions: (timestamp, new_weather).
    history: VecDeque<(DateTime<Utc>, Weather)>,
}

impl WeatherEngine {
    /// Creates a new engine starting in the given state.
    pub fn new(initial: Weather, start_time: DateTime<Utc>) -> Self {
        Self {
            current: initial,
            since: start_time,
            min_duration_hours: DEFAULT_MIN_DURATION_HOURS,
            last_check_hour: None,
            history: VecDeque::with_capacity(HISTORY_CAPACITY),
        }
    }

    /// Returns the current weather.
    pub fn current(&self) -> Weather {
        self.current
    }

    /// Returns the history of weather transitions (newest last).
    ///
    /// Each entry is `(timestamp, new_weather)` recorded when a transition fired.
    pub fn history(&self) -> &VecDeque<(DateTime<Utc>, Weather)> {
        &self.history
    }

    /// Returns how long the current weather has persisted (game-hours).
    pub fn duration_hours(&self, now: DateTime<Utc>) -> f64 {
        let elapsed = now.signed_duration_since(self.since);
        elapsed.num_minutes() as f64 / 60.0
    }

    /// Returns the game time when the current weather state began.
    pub fn since(&self) -> DateTime<Utc> {
        self.since
    }

    /// Returns the minimum duration (game-hours) before a transition is allowed.
    pub fn min_duration_hours(&self) -> f64 {
        self.min_duration_hours
    }

    /// Returns the game-hour of the last transition check, or `None` if unchecked.
    pub fn last_check_hour(&self) -> Option<i64> {
        self.last_check_hour
    }

    /// Forces the weather to a specific state immediately, resetting the
    /// minimum-duration timer so the engine won't try to transition away
    /// for another `min_duration_hours`. Intended for the `/weather <name>`
    /// command and deterministic play-test scripts.
    pub fn force(&mut self, weather: Weather, now: DateTime<Utc>) {
        self.current = weather;
        self.since = now;
        self.last_check_hour = Some(now.timestamp() / 3600);
        if self.history.len() >= HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back((now, weather));
    }

    /// Ticks the weather engine. Returns `Some(new_weather)` if a
    /// transition occurred, `None` if the weather is unchanged.
    ///
    /// Called every game tick. Only evaluates transitions after
    /// `min_duration_hours` have elapsed, and at most once per game-hour.
    ///
    /// Takes the current game time directly (rather than a `&GameClock`)
    /// to avoid borrow conflicts when the engine is part of `WorldState`.
    pub fn tick(
        &mut self,
        now: DateTime<Utc>,
        season: Season,
        rng: &mut impl Rng,
    ) -> Option<Weather> {
        let current_hour = now.timestamp() / 3600;

        // Only check once per game-hour
        if self.last_check_hour == Some(current_hour) {
            return None;
        }
        self.last_check_hour = Some(current_hour);

        // Enforce minimum duration
        if self.duration_hours(now) < self.min_duration_hours {
            return None;
        }

        let new_weather = self.compute_transition(season, rng)?;
        self.current = new_weather;
        self.since = now;
        self.last_check_hour = Some(current_hour);
        // Record transition in history ring buffer
        if self.history.len() >= HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back((now, new_weather));
        Some(new_weather)
    }

    /// Computes a possible weather transition based on season and RNG.
    ///
    /// Returns `None` if no transition occurs (weather stays the same).
    fn compute_transition(&self, season: Season, rng: &mut impl Rng) -> Option<Weather> {
        let bias = seasonal_bias(season);

        // Base probability of any transition occurring this hour: 40%
        let transition_roll: f64 = rng.random();
        if transition_roll > 0.40 {
            return None;
        }

        // Handle fog as a special case
        if self.current == Weather::Fog {
            // Fog clears to PartlyCloudy or Overcast
            return if rng.random::<f64>() < 0.5 {
                Some(Weather::PartlyCloudy)
            } else {
                Some(Weather::Overcast)
            };
        }

        // Check for fog transition (only from PartlyCloudy or Overcast)
        if matches!(self.current, Weather::PartlyCloudy | Weather::Overcast)
            && rng.random::<f64>() < bias.fog_chance
        {
            return Some(Weather::Fog);
        }

        // Main axis transition: move up or down by one step
        let ord = weather_ordinal(self.current)?;
        let direction_roll: f64 = rng.random();

        if direction_roll < bias.clear_pull {
            // Move toward clear (lower ordinal)
            if ord > 0 {
                Some(weather_from_ordinal(ord - 1))
            } else {
                None // Already at Clear
            }
        } else if direction_roll < bias.clear_pull + bias.rain_pull {
            // Move toward rain/storm (higher ordinal)
            if ord < 5 {
                Some(weather_from_ordinal(ord + 1))
            } else {
                None // Already at Storm
            }
        } else {
            // Stay put
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn time_at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(1820, 3, 20, hour, 0, 0).unwrap()
    }

    #[test]
    fn test_weather_engine_initial_state() {
        let start = time_at(8);
        let engine = WeatherEngine::new(Weather::Overcast, start);
        assert_eq!(engine.current(), Weather::Overcast);
        assert!((engine.duration_hours(start) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_weather_no_transition_before_min_duration() {
        let start = time_at(8);
        let mut engine = WeatherEngine::new(Weather::Clear, start);
        let mut rng = StdRng::seed_from_u64(42);

        // 1 hour later — less than min_duration of 2h
        let now = time_at(9);
        let result = engine.tick(now, Season::Spring, &mut rng);
        assert!(
            result.is_none(),
            "Should not transition before min duration"
        );
        assert_eq!(engine.current(), Weather::Clear);
    }

    #[test]
    fn test_weather_transitions_after_min_duration() {
        let start = time_at(8);

        // Try many seeds until we find one that produces a transition
        let mut transitioned = false;
        for seed in 0..100 {
            let mut engine = WeatherEngine::new(Weather::Overcast, start);
            let mut rng = StdRng::seed_from_u64(seed);

            // 3 hours later (past min_duration)
            let now = time_at(11);
            if let Some(new_weather) = engine.tick(now, Season::Winter, &mut rng) {
                // Verify it's an adjacent state
                assert!(
                    matches!(
                        new_weather,
                        Weather::PartlyCloudy | Weather::LightRain | Weather::Fog
                    ),
                    "Overcast should only transition to adjacent states, got {new_weather}"
                );
                transitioned = true;
                break;
            }
        }
        assert!(transitioned, "Should eventually transition with some seed");
    }

    #[test]
    fn test_weather_seasonal_bias() {
        // Drive the engine through tick() over simulated time so the test
        // exercises the same code path that production uses, rather than
        // calling compute_transition() directly and manually patching
        // internal state.
        //
        // tick() fires at most once per game-hour and only after the
        // min_duration (2h) has elapsed in the current state, so running
        // 600 simulated hours gives roughly 150–300 transition attempts —
        // enough to observe the seasonal bias signal clearly.
        let start = Utc.with_ymd_and_hms(1820, 6, 15, 8, 0, 0).unwrap();

        let count_rain_states = |season: Season, seed: u64| -> usize {
            let mut engine = WeatherEngine::new(Weather::Overcast, start);
            let mut rng = StdRng::seed_from_u64(seed);
            let mut rain_count = 0;

            for hour_offset in 1..=600u32 {
                let game_time = start + chrono::Duration::hours(hour_offset as i64);
                engine.tick(game_time, season, &mut rng);

                if matches!(
                    engine.current(),
                    Weather::LightRain | Weather::HeavyRain | Weather::Storm
                ) {
                    rain_count += 1;
                }
            }
            rain_count
        };

        let winter_rain = count_rain_states(Season::Winter, 12345);
        let summer_rain = count_rain_states(Season::Summer, 12345);

        assert!(
            winter_rain > summer_rain,
            "Winter should produce more rain states than summer: winter={winter_rain}, summer={summer_rain}"
        );
    }

    #[test]
    fn test_weather_no_skip_states() {
        let start = time_at(8);

        // Run many transitions from Clear and verify Storm is never reached directly
        for seed in 0..200 {
            let engine = WeatherEngine::new(Weather::Clear, start);
            let mut rng = StdRng::seed_from_u64(seed);

            if let Some(new) = engine.compute_transition(Season::Winter, &mut rng) {
                assert_ne!(
                    new,
                    Weather::Storm,
                    "Clear should never jump directly to Storm (seed={seed})"
                );
                assert_ne!(
                    new,
                    Weather::HeavyRain,
                    "Clear should never jump directly to HeavyRain (seed={seed})"
                );
                assert_ne!(
                    new,
                    Weather::LightRain,
                    "Clear should never jump directly to LightRain (seed={seed})"
                );
                // From Clear, only PartlyCloudy is reachable (one step up)
                assert!(
                    matches!(new, Weather::PartlyCloudy),
                    "Clear should only transition to PartlyCloudy, got {new} (seed={seed})"
                );
            }
        }
    }

    #[test]
    fn test_weather_changed_event_published() {
        use parish_types::events::{EventBus, GameEvent};

        let start = time_at(8);
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        // Find a seed that produces a transition from Overcast after 3h in Winter
        let mut transitioned = false;
        let now = time_at(11);
        for seed in 0..100 {
            let mut engine = WeatherEngine::new(Weather::Overcast, start);
            let mut rng = StdRng::seed_from_u64(seed);

            if let Some(new_weather) = engine.tick(now, Season::Winter, &mut rng) {
                // Simulate what the game loop would do
                let old_weather = Weather::Overcast;
                bus.publish(GameEvent::WeatherChanged {
                    new_weather: new_weather.to_string(),
                    timestamp: now,
                });

                let event = rx.try_recv().expect("should receive WeatherChanged event");
                match event {
                    GameEvent::WeatherChanged {
                        new_weather: w,
                        timestamp: _,
                    } => {
                        assert_ne!(w, old_weather.to_string());
                        transitioned = true;
                    }
                    _ => panic!("Expected WeatherChanged event"),
                }
                break;
            }
        }
        assert!(transitioned, "Should find a seed that triggers transition");
    }
}
