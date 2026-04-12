//! Game time system.
//!
//! 40 real-world minutes = 1 in-game day (speed factor 36.0, "Normal").
//! Adjustable at runtime via [`GameSpeed`] presets (Slow/Normal/Fast/Fastest).
//! Tracks time of day, season, and calendar festivals.
//!
//! Festivals can be defined via the hardcoded [`Festival`] enum (legacy) or
//! loaded from a mod's [`FestivalDef`](crate::game_mod::FestivalDef) data.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;

/// Speed multiplier factors. Higher = faster game time.
///
/// Factor of 36.0 means 40 real minutes = 1 game day.
#[derive(Debug, Deserialize, Clone)]
pub struct SpeedConfig {
    /// 80 real minutes per game day.
    #[serde(default = "default_slow")]
    pub slow: f64,
    /// 40 real minutes per game day.
    #[serde(default = "default_normal")]
    pub normal: f64,
    /// 20 real minutes per game day.
    #[serde(default = "default_fast")]
    pub fast: f64,
    /// 10 real minutes per game day.
    #[serde(default = "default_fastest")]
    pub fastest: f64,
    /// ~100 real seconds per game day.
    #[serde(default = "default_ludicrous")]
    pub ludicrous: f64,
}

impl Default for SpeedConfig {
    fn default() -> Self {
        Self {
            slow: 18.0,
            normal: 36.0,
            fast: 72.0,
            fastest: 144.0,
            ludicrous: 864.0,
        }
    }
}

fn default_slow() -> f64 {
    18.0
}
fn default_normal() -> f64 {
    36.0
}
fn default_fast() -> f64 {
    72.0
}
fn default_fastest() -> f64 {
    144.0
}
fn default_ludicrous() -> f64 {
    864.0
}

/// Represents the time of day in the game world.
///
/// Used to drive color palette selection and NPC behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeOfDay {
    /// 5:00–6:59
    Dawn,
    /// 7:00–9:59
    Morning,
    /// 10:00–13:59
    Midday,
    /// 14:00–16:59
    Afternoon,
    /// 17:00–18:59
    Dusk,
    /// 19:00–22:59
    Night,
    /// 23:00–4:59
    Midnight,
}

impl fmt::Display for TimeOfDay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TimeOfDay::Dawn => write!(f, "Dawn"),
            TimeOfDay::Morning => write!(f, "Morning"),
            TimeOfDay::Midday => write!(f, "Midday"),
            TimeOfDay::Afternoon => write!(f, "Afternoon"),
            TimeOfDay::Dusk => write!(f, "Dusk"),
            TimeOfDay::Night => write!(f, "Night"),
            TimeOfDay::Midnight => write!(f, "Midnight"),
        }
    }
}

/// Represents the four seasons of the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Season {
    /// March–May
    Spring,
    /// June–August
    Summer,
    /// September–November
    Autumn,
    /// December–February
    Winter,
}

impl Season {
    /// Determines the season from a calendar date.
    ///
    /// Uses meteorological seasons (month-based):
    /// - Spring: March–May
    /// - Summer: June–August
    /// - Autumn: September–November
    /// - Winter: December–February
    pub fn from_date(date: NaiveDate) -> Self {
        match date.month() {
            3..=5 => Season::Spring,
            6..=8 => Season::Summer,
            9..=11 => Season::Autumn,
            _ => Season::Winter,
        }
    }
}

impl fmt::Display for Season {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Season::Spring => write!(f, "Spring"),
            Season::Summer => write!(f, "Summer"),
            Season::Autumn => write!(f, "Autumn"),
            Season::Winter => write!(f, "Winter"),
        }
    }
}

/// The type of day, affecting NPC schedules.
///
/// In 1820s rural Ireland, Sunday (Mass day) and market day (Saturday)
/// had distinctly different rhythms from ordinary weekdays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DayType {
    /// Monday through Friday — ordinary working days.
    Weekday,
    /// Sunday — Mass, socializing, no field work.
    Sunday,
    /// Saturday — market day in the nearest town.
    MarketDay,
}

impl DayType {
    /// Determines the day type from a calendar date.
    pub fn from_date(date: NaiveDate) -> Self {
        match date.weekday() {
            chrono::Weekday::Sun => DayType::Sunday,
            chrono::Weekday::Sat => DayType::MarketDay,
            _ => DayType::Weekday,
        }
    }
}

impl fmt::Display for DayType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DayType::Weekday => write!(f, "Weekday"),
            DayType::Sunday => write!(f, "Sunday"),
            DayType::MarketDay => write!(f, "Market Day"),
        }
    }
}

/// Traditional Irish seasonal festivals.
///
/// These mark the transitions between seasons in the Irish calendar
/// and serve as hooks for future mythological events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Festival {
    /// February 1 — Start of spring
    Imbolc,
    /// May 1 — Start of summer
    Bealtaine,
    /// August 1 — Start of autumn
    Lughnasa,
    /// November 1 — Start of winter
    Samhain,
}

impl Festival {
    /// Checks if the given date falls on a festival day.
    pub fn check(date: NaiveDate) -> Option<Festival> {
        match (date.month(), date.day()) {
            (2, 1) => Some(Festival::Imbolc),
            (5, 1) => Some(Festival::Bealtaine),
            (8, 1) => Some(Festival::Lughnasa),
            (11, 1) => Some(Festival::Samhain),
            _ => None,
        }
    }
}

impl fmt::Display for Festival {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Festival::Imbolc => write!(f, "Imbolc"),
            Festival::Bealtaine => write!(f, "Bealtaine"),
            Festival::Lughnasa => write!(f, "Lughnasa"),
            Festival::Samhain => write!(f, "Samhain"),
        }
    }
}

/// Named speed presets for the game clock, inspired by SimCity.
///
/// Each variant maps to a speed factor (game-time seconds per real-time second).
/// The default is `Normal` (36.0 = 40 real minutes per game day).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameSpeed {
    /// Slowest pace — 18.0 factor (80 real minutes per game day).
    Slow,
    /// Default pace — 36.0 factor (40 real minutes per game day).
    Normal,
    /// Fast pace — 72.0 factor (20 real minutes per game day).
    Fast,
    /// Fastest pace — 144.0 factor (10 real minutes per game day).
    Fastest,
    /// Ludicrous pace for testing — 864.0 factor (100 real seconds per game day).
    Ludicrous,
}

impl GameSpeed {
    /// All speed presets in order from slowest to fastest.
    pub const ALL: &[GameSpeed] = &[
        GameSpeed::Slow,
        GameSpeed::Normal,
        GameSpeed::Fast,
        GameSpeed::Fastest,
        GameSpeed::Ludicrous,
    ];

    /// Returns the speed factor for this preset using default config values.
    pub fn factor(self) -> f64 {
        self.factor_with_config(&SpeedConfig::default())
    }

    /// Returns the speed factor for this preset using the given config.
    pub fn factor_with_config(self, config: &SpeedConfig) -> f64 {
        match self {
            GameSpeed::Slow => config.slow,
            GameSpeed::Normal => config.normal,
            GameSpeed::Fast => config.fast,
            GameSpeed::Fastest => config.fastest,
            GameSpeed::Ludicrous => config.ludicrous,
        }
    }

    /// Parses a speed preset from a string (case-insensitive).
    pub fn from_name(s: &str) -> Option<GameSpeed> {
        match s.to_lowercase().as_str() {
            "slow" => Some(GameSpeed::Slow),
            "normal" => Some(GameSpeed::Normal),
            "fast" => Some(GameSpeed::Fast),
            "fastest" => Some(GameSpeed::Fastest),
            "ludicrous" => Some(GameSpeed::Ludicrous),
            _ => None,
        }
    }

    /// Returns a thematic message for when this speed is activated.
    pub fn activation_message(self) -> &'static str {
        match self {
            GameSpeed::Slow => "The parish slows to a gentle amble.",
            GameSpeed::Normal => "The parish settles into its natural stride.",
            GameSpeed::Fast => "The parish quickens its step.",
            GameSpeed::Fastest => "The parish fair flies — hold onto your hat!",
            GameSpeed::Ludicrous => "The world is a blur — days pass in the blink of an eye!",
        }
    }
}

impl fmt::Display for GameSpeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameSpeed::Slow => write!(f, "Slow"),
            GameSpeed::Normal => write!(f, "Normal"),
            GameSpeed::Fast => write!(f, "Fast"),
            GameSpeed::Fastest => write!(f, "Fastest"),
            GameSpeed::Ludicrous => write!(f, "Ludicrous"),
        }
    }
}

/// Maps real-world elapsed time to accelerated game time.
///
/// The default speed factor of 36.0 means 40 real-world minutes
/// equals 1 in-game day (24 hours). The clock can be paused,
/// resumed, manually advanced (e.g. during travel), and its speed
/// changed at runtime via [`GameSpeed`] presets.
pub struct GameClock {
    /// Wall-clock instant when the clock was created or last resumed.
    start_real: Instant,
    /// The game-world time corresponding to `start_real`.
    start_game: DateTime<Utc>,
    /// Whether the clock is paused by the player.
    paused: bool,
    /// Whether the clock is paused while waiting for an inference response.
    inference_paused: bool,
    /// Game-time seconds per real-time second (default 36.0).
    speed_factor: f64,
    /// Game time when the clock was frozen (valid when paused or inference_paused).
    paused_game_time: DateTime<Utc>,
}

impl GameClock {
    /// Creates a new game clock starting at the given game time.
    ///
    /// The default speed factor is 36.0 (40 real minutes = 1 game day).
    pub fn new(start_game: DateTime<Utc>) -> Self {
        Self {
            start_real: Instant::now(),
            start_game,
            paused: false,
            inference_paused: false,
            speed_factor: SpeedConfig::default().normal,
            paused_game_time: start_game,
        }
    }

    /// Creates a game clock with a custom speed factor.
    pub fn with_speed(start_game: DateTime<Utc>, speed_factor: f64) -> Self {
        Self {
            start_real: Instant::now(),
            start_game,
            paused: false,
            inference_paused: false,
            speed_factor,
            paused_game_time: start_game,
        }
    }

    /// Returns whether the clock is frozen (by player pause or inference pause).
    fn is_frozen(&self) -> bool {
        self.paused || self.inference_paused
    }

    /// Returns the current game time.
    ///
    /// When frozen (player-paused or inference-paused), returns the time at
    /// which the clock was frozen. When running, maps elapsed real time to
    /// game time using the speed factor.
    pub fn now(&self) -> DateTime<Utc> {
        if self.is_frozen() {
            return self.paused_game_time;
        }
        let elapsed_real = self.start_real.elapsed().as_secs_f64();
        let elapsed_game_secs = (elapsed_real * self.speed_factor) as i64;
        self.start_game + Duration::seconds(elapsed_game_secs)
    }

    /// Returns the current time of day.
    pub fn time_of_day(&self) -> TimeOfDay {
        time_of_day_from_hour(self.now().hour())
    }

    /// Returns the current season.
    pub fn season(&self) -> Season {
        Season::from_date(self.now().date_naive())
    }

    /// Returns the current day type (weekday, Sunday, or market day).
    pub fn day_type(&self) -> DayType {
        DayType::from_date(self.now().date_naive())
    }

    /// Checks if today is a festival day using the hardcoded [`Festival`] enum.
    ///
    /// Prefer [`check_festival_data`](GameClock::check_festival_data) for
    /// mod-driven festival definitions.
    pub fn check_festival(&self) -> Option<Festival> {
        Festival::check(self.now().date_naive())
    }

    /// Checks if today is a festival day using data-driven definitions.
    ///
    /// Returns a reference to a festival def if the current game date matches.
    /// The festival def type is generic to avoid depending on game_mod.
    pub fn check_festival_data<'a, F>(&self, festivals: &'a [F]) -> Option<&'a F>
    where
        F: HasFestivalDate,
    {
        let date = self.now().date_naive();
        let (month, day) = (date.month(), date.day());
        festivals
            .iter()
            .find(|f| f.month() == month && f.day() == day)
    }

    /// Advances the game clock by the given number of game minutes.
    ///
    /// Used during travel or other time-consuming actions.
    pub fn advance(&mut self, game_minutes: i64) {
        if self.is_frozen() {
            self.paused_game_time += Duration::minutes(game_minutes);
        } else {
            self.start_game += Duration::minutes(game_minutes);
        }
    }

    /// Pauses the game clock (player-initiated), freezing game time.
    pub fn pause(&mut self) {
        if !self.paused {
            if !self.is_frozen() {
                self.paused_game_time = self.now();
            }
            self.paused = true;
        }
    }

    /// Resumes the game clock (player-initiated).
    ///
    /// The clock only actually resumes if it is not also inference-paused.
    pub fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            if !self.is_frozen() {
                self.start_game = self.paused_game_time;
                self.start_real = Instant::now();
            }
        }
    }

    /// Returns whether the clock is player-paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pauses the game clock while waiting for an inference response.
    ///
    /// The clock freezes if it is not already frozen. Does not interfere
    /// with player-initiated pause/resume.
    pub fn inference_pause(&mut self) {
        if !self.inference_paused {
            if !self.is_frozen() {
                self.paused_game_time = self.now();
            }
            self.inference_paused = true;
        }
    }

    /// Resumes the game clock after an inference response completes.
    ///
    /// The clock only actually resumes if it is not also player-paused.
    pub fn inference_resume(&mut self) {
        if self.inference_paused {
            self.inference_paused = false;
            if !self.is_frozen() {
                self.start_game = self.paused_game_time;
                self.start_real = Instant::now();
            }
        }
    }

    /// Returns whether the clock is inference-paused.
    pub fn is_inference_paused(&self) -> bool {
        self.inference_paused
    }

    /// Returns the current speed factor.
    pub fn speed_factor(&self) -> f64 {
        self.speed_factor
    }

    /// Changes the speed factor at runtime, recalibrating the clock.
    ///
    /// Captures the current game time, resets the real-time anchor to now,
    /// and applies the new speed factor going forward. Works correctly
    /// whether the clock is paused or running.
    pub fn set_speed(&mut self, speed: GameSpeed) {
        if self.is_frozen() {
            self.speed_factor = speed.factor();
        } else {
            let current = self.now();
            self.start_game = current;
            self.start_real = Instant::now();
            self.speed_factor = speed.factor();
        }
    }

    /// Returns the named speed preset matching the current factor, if any.
    pub fn current_speed(&self) -> Option<GameSpeed> {
        const EPSILON: f64 = 0.01;
        GameSpeed::ALL
            .iter()
            .find(|s| (self.speed_factor - s.factor()).abs() < EPSILON)
            .copied()
    }

    /// Returns the game-time origin anchor (creation or last resume).
    ///
    /// Exposed for the debug panel so it can report real-vs-game drift.
    pub fn start_game(&self) -> DateTime<Utc> {
        self.start_game
    }

    /// Returns the frozen game time captured when the clock was paused.
    ///
    /// When the clock is running this is the last pause anchor; when paused
    /// (by player or inference) it matches `now()`.
    pub fn paused_game_time(&self) -> DateTime<Utc> {
        self.paused_game_time
    }

    /// Returns the real-world elapsed seconds since the last resume/create.
    ///
    /// Useful for the debug panel to compare real vs. accelerated time.
    pub fn real_elapsed_secs(&self) -> f64 {
        self.start_real.elapsed().as_secs_f64()
    }
}

/// Trait for types that have a festival month and day.
///
/// Allows `GameClock::check_festival_data` to work with any festival
/// definition type without depending on `game_mod`.
pub trait HasFestivalDate {
    /// Returns the festival month (1–12).
    fn month(&self) -> u32;
    /// Returns the festival day (1–31).
    fn day(&self) -> u32;
}

/// Maps an hour (0–23) to a `TimeOfDay` variant.
pub fn time_of_day_from_hour(hour: u32) -> TimeOfDay {
    match hour {
        5..=6 => TimeOfDay::Dawn,
        7..=9 => TimeOfDay::Morning,
        10..=13 => TimeOfDay::Midday,
        14..=16 => TimeOfDay::Afternoon,
        17..=18 => TimeOfDay::Dusk,
        19..=22 => TimeOfDay::Night,
        _ => TimeOfDay::Midnight, // 23, 0–4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn game_time(year: i32, month: u32, day: u32, hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, 0, 0).unwrap()
    }

    #[test]
    fn test_time_of_day_transitions() {
        assert_eq!(time_of_day_from_hour(0), TimeOfDay::Midnight);
        assert_eq!(time_of_day_from_hour(3), TimeOfDay::Midnight);
        assert_eq!(time_of_day_from_hour(4), TimeOfDay::Midnight);
        assert_eq!(time_of_day_from_hour(5), TimeOfDay::Dawn);
        assert_eq!(time_of_day_from_hour(6), TimeOfDay::Dawn);
        assert_eq!(time_of_day_from_hour(7), TimeOfDay::Morning);
        assert_eq!(time_of_day_from_hour(9), TimeOfDay::Morning);
        assert_eq!(time_of_day_from_hour(10), TimeOfDay::Midday);
        assert_eq!(time_of_day_from_hour(13), TimeOfDay::Midday);
        assert_eq!(time_of_day_from_hour(14), TimeOfDay::Afternoon);
        assert_eq!(time_of_day_from_hour(16), TimeOfDay::Afternoon);
        assert_eq!(time_of_day_from_hour(17), TimeOfDay::Dusk);
        assert_eq!(time_of_day_from_hour(18), TimeOfDay::Dusk);
        assert_eq!(time_of_day_from_hour(19), TimeOfDay::Night);
        assert_eq!(time_of_day_from_hour(22), TimeOfDay::Night);
        assert_eq!(time_of_day_from_hour(23), TimeOfDay::Midnight);
    }

    #[test]
    fn test_season_from_date() {
        let date = |m: u32, d: u32| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(Season::from_date(date(1, 15)), Season::Winter);
        assert_eq!(Season::from_date(date(2, 15)), Season::Winter);
        assert_eq!(Season::from_date(date(3, 1)), Season::Spring);
        assert_eq!(Season::from_date(date(5, 31)), Season::Spring);
        assert_eq!(Season::from_date(date(6, 1)), Season::Summer);
        assert_eq!(Season::from_date(date(8, 31)), Season::Summer);
        assert_eq!(Season::from_date(date(9, 1)), Season::Autumn);
        assert_eq!(Season::from_date(date(11, 30)), Season::Autumn);
        assert_eq!(Season::from_date(date(12, 1)), Season::Winter);
    }

    #[test]
    fn test_festival_detection() {
        let date = |m: u32, d: u32| NaiveDate::from_ymd_opt(2026, m, d).unwrap();
        assert_eq!(Festival::check(date(2, 1)), Some(Festival::Imbolc));
        assert_eq!(Festival::check(date(5, 1)), Some(Festival::Bealtaine));
        assert_eq!(Festival::check(date(8, 1)), Some(Festival::Lughnasa));
        assert_eq!(Festival::check(date(11, 1)), Some(Festival::Samhain));
        assert_eq!(Festival::check(date(3, 15)), None);
        assert_eq!(Festival::check(date(2, 2)), None);
    }

    #[test]
    fn test_game_clock_time_of_day() {
        let clock = GameClock::new(game_time(2026, 6, 15, 7));
        assert_eq!(clock.time_of_day(), TimeOfDay::Morning);

        let clock = GameClock::new(game_time(2026, 6, 15, 22));
        assert_eq!(clock.time_of_day(), TimeOfDay::Night);
    }

    #[test]
    fn test_game_clock_season() {
        let clock = GameClock::new(game_time(2026, 6, 15, 12));
        assert_eq!(clock.season(), Season::Summer);

        let clock = GameClock::new(game_time(2026, 1, 15, 12));
        assert_eq!(clock.season(), Season::Winter);
    }

    #[test]
    fn test_game_clock_advance() {
        let mut clock = GameClock::new(game_time(2026, 6, 15, 7));
        clock.advance(60); // advance 1 game hour
        let now = clock.now();
        assert_eq!(now.hour(), 8);
    }

    #[test]
    fn test_speed_config_defaults() {
        let cfg = SpeedConfig::default();
        assert!((cfg.slow - 18.0).abs() < f64::EPSILON);
        assert!((cfg.normal - 36.0).abs() < f64::EPSILON);
        assert!((cfg.fast - 72.0).abs() < f64::EPSILON);
        assert!((cfg.fastest - 144.0).abs() < f64::EPSILON);
        assert!((cfg.ludicrous - 864.0).abs() < f64::EPSILON);
    }
}
