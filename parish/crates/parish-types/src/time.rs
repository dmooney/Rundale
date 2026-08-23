//! Game time system.
//!
//! 40 real-world minutes = 1 in-game day (speed factor 36.0, "Normal").
//! Adjustable at runtime via [`GameSpeed`] presets (Slow/Normal/Fast/Fastest).
//! Tracks time of day, season, and calendar festivals.
//!
//! Festivals are defined via the hardcoded [`Festival`] enum.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;

/// Speed multiplier factors. Higher = faster game time.
///
/// Factor of 36.0 means 40 real minutes = 1 game day.
#[derive(Debug, Deserialize, Serialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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

/// Convert a chrono weekday to its English name.
///
/// Time-formatting utility shared by the IPC handlers (`parish-core`) and the
/// diagnostics snapshot builders (`parish-diagnostics`). Lives in the lowest
/// leaf crate so neither consumer reaches across crate boundaries for it.
pub fn weekday_name(wd: chrono::Weekday) -> &'static str {
    match wd {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeOfDay {
    /// 5:00–6:59
    Dawn,
    /// 7:00–11:59
    Morning,
    /// 12:00–13:59
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
        match self {
            GameSpeed::Slow => 18.0,
            GameSpeed::Normal => 36.0,
            GameSpeed::Fast => 72.0,
            GameSpeed::Fastest => 144.0,
            GameSpeed::Ludicrous => 864.0,
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
#[derive(Clone)]
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
    pub fn check_festival(&self) -> Option<Festival> {
        Festival::check(self.now().date_naive())
    }

    /// Advances the game clock by the given number of game minutes.
    ///
    /// Used during travel or other time-consuming actions. Emits a
    /// `tracing::debug` line so the per-turn clock advance is visible
    /// in the demo log (TODO #32: the audit caught a "15 minutes on
    /// foot" travel that appeared to consume ~5 game-hours of clock
    /// time; the only way to localise the discrepancy is to see
    /// every advance call with its delta).
    pub fn advance(&mut self, game_minutes: i64) {
        if self.is_frozen() {
            self.paused_game_time += Duration::minutes(game_minutes);
        } else {
            self.start_game += Duration::minutes(game_minutes);
        }
        tracing::debug!(
            game_minutes_advanced = game_minutes,
            frozen = self.is_frozen(),
            now = %self.now().format("%H:%M"),
            "clock advance"
        );
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

/// Maps an hour (0–23) to a `TimeOfDay` variant.
pub fn time_of_day_from_hour(hour: u32) -> TimeOfDay {
    match hour {
        5..=6 => TimeOfDay::Dawn,
        7..=11 => TimeOfDay::Morning,
        12..=13 => TimeOfDay::Midday,
        14..=16 => TimeOfDay::Afternoon,
        17..=18 => TimeOfDay::Dusk,
        19..=22 => TimeOfDay::Night,
        _ => TimeOfDay::Midnight, // 23, 0–4
    }
}

/// Returns the correctly pluralized word for a count of minutes:
/// `"minute"` for exactly 1, `"minutes"` otherwise.
///
/// Generic over the integer type so callers can pass `u16` (travel legs),
/// `u32` (`/wait` minutes), etc. without casts. Centralizes the
/// singular/plural branch shared by travel-arrival and `/wait` narration
/// so a 1-minute leg reads "1 minute on foot" rather than "1 minutes on
/// foot" (#1156).
pub fn minute_word<T>(minutes: T) -> &'static str
where
    T: PartialEq + From<u8>,
{
    if minutes == T::from(1u8) {
        "minute"
    } else {
        "minutes"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_minute_word_singular_plural() {
        // #1156: a 1-minute leg must read "1 minute", not "1 minutes".
        assert_eq!(minute_word(0u16), "minutes");
        assert_eq!(minute_word(1u16), "minute");
        assert_eq!(minute_word(2u16), "minutes");
        assert_eq!(minute_word(11u16), "minutes");
        // Generic over the integer type used by `/wait` (u32).
        assert_eq!(minute_word(1u32), "minute");
        assert_eq!(minute_word(15u32), "minutes");
    }

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
        assert_eq!(time_of_day_from_hour(10), TimeOfDay::Morning);
        assert_eq!(time_of_day_from_hour(11), TimeOfDay::Morning);
        assert_eq!(time_of_day_from_hour(12), TimeOfDay::Midday);
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
    fn festival_hardcoded_calendar_contract() {
        let cases = [
            ((1820, 2, 1), Some(Festival::Imbolc)),
            ((1820, 5, 1), Some(Festival::Bealtaine)),
            ((1820, 8, 1), Some(Festival::Lughnasa)),
            ((1820, 11, 1), Some(Festival::Samhain)),
            ((1820, 1, 31), None),
            ((1820, 2, 2), None),
            ((1820, 4, 30), None),
            ((1820, 5, 2), None),
            ((1820, 7, 31), None),
            ((1820, 8, 2), None),
            ((1820, 10, 31), None),
            ((1820, 11, 2), None),
        ];

        for ((year, month, day), expected) in cases {
            let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
            assert_eq!(Festival::check(date), expected, "{date}");
        }
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

    #[test]
    fn speed_config_deserializes_partial_config_with_defaults() {
        let cfg: SpeedConfig =
            serde_json::from_str(r#"{"normal": 42.5, "ludicrous": 1000.0}"#).unwrap();

        assert!((cfg.slow - 18.0).abs() < f64::EPSILON);
        assert!((cfg.normal - 42.5).abs() < f64::EPSILON);
        assert!((cfg.fast - 72.0).abs() < f64::EPSILON);
        assert!((cfg.fastest - 144.0).abs() < f64::EPSILON);
        assert!((cfg.ludicrous - 1000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn time_enum_serde_spellings_are_stable() {
        let time_spellings = [
            (TimeOfDay::Dawn, r#""Dawn""#),
            (TimeOfDay::Morning, r#""Morning""#),
            (TimeOfDay::Midday, r#""Midday""#),
            (TimeOfDay::Afternoon, r#""Afternoon""#),
            (TimeOfDay::Dusk, r#""Dusk""#),
            (TimeOfDay::Night, r#""Night""#),
            (TimeOfDay::Midnight, r#""Midnight""#),
        ];
        for (variant, spelling) in time_spellings {
            assert_eq!(serde_json::to_string(&variant).unwrap(), spelling);
            assert_eq!(
                serde_json::from_str::<TimeOfDay>(spelling).unwrap(),
                variant
            );
        }

        let season_spellings = [
            (Season::Spring, r#""spring""#),
            (Season::Summer, r#""summer""#),
            (Season::Autumn, r#""autumn""#),
            (Season::Winter, r#""winter""#),
        ];
        for (variant, spelling) in season_spellings {
            assert_eq!(serde_json::to_string(&variant).unwrap(), spelling);
            assert_eq!(serde_json::from_str::<Season>(spelling).unwrap(), variant);
        }

        let day_type_spellings = [
            (DayType::Weekday, r#""weekday""#),
            (DayType::Sunday, r#""sunday""#),
            (DayType::MarketDay, r#""market_day""#),
        ];
        for (variant, spelling) in day_type_spellings {
            assert_eq!(serde_json::to_string(&variant).unwrap(), spelling);
            assert_eq!(serde_json::from_str::<DayType>(spelling).unwrap(), variant);
        }

        let festival_spellings = [
            (Festival::Imbolc, r#""Imbolc""#),
            (Festival::Bealtaine, r#""Bealtaine""#),
            (Festival::Lughnasa, r#""Lughnasa""#),
            (Festival::Samhain, r#""Samhain""#),
        ];
        for (variant, spelling) in festival_spellings {
            assert_eq!(serde_json::to_string(&variant).unwrap(), spelling);
            assert_eq!(serde_json::from_str::<Festival>(spelling).unwrap(), variant);
        }

        let speed_spellings = [
            (GameSpeed::Slow, r#""Slow""#),
            (GameSpeed::Normal, r#""Normal""#),
            (GameSpeed::Fast, r#""Fast""#),
            (GameSpeed::Fastest, r#""Fastest""#),
            (GameSpeed::Ludicrous, r#""Ludicrous""#),
        ];
        for (variant, spelling) in speed_spellings {
            assert_eq!(serde_json::to_string(&variant).unwrap(), spelling);
            assert_eq!(
                serde_json::from_str::<GameSpeed>(spelling).unwrap(),
                variant
            );
        }
    }

    // --- DayType::from_date ---

    #[test]
    fn test_day_type_weekdays() {
        // 1820-03-20 is a Monday (game start date)
        let date = NaiveDate::from_ymd_opt(1820, 3, 20).unwrap();
        assert_eq!(DayType::from_date(date), DayType::Weekday);
        // Tuesday through Friday
        for d in 21..=24 {
            let date = NaiveDate::from_ymd_opt(1820, 3, d).unwrap();
            assert_eq!(DayType::from_date(date), DayType::Weekday, "day {d}");
        }
    }

    #[test]
    fn test_day_type_saturday_is_market_day() {
        // 1820-03-25 is a Saturday
        let date = NaiveDate::from_ymd_opt(1820, 3, 25).unwrap();
        assert_eq!(DayType::from_date(date), DayType::MarketDay);
    }

    #[test]
    fn test_day_type_sunday() {
        // 1820-03-26 is a Sunday
        let date = NaiveDate::from_ymd_opt(1820, 3, 26).unwrap();
        assert_eq!(DayType::from_date(date), DayType::Sunday);
    }

    #[test]
    fn test_day_type_display() {
        assert_eq!(DayType::Weekday.to_string(), "Weekday");
        assert_eq!(DayType::Sunday.to_string(), "Sunday");
        assert_eq!(DayType::MarketDay.to_string(), "Market Day");
    }

    // ── GameClock pause/resume ─────────────────────────────────────────────

    #[test]
    fn test_pause_resume() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        assert!(!clock.is_paused());
        clock.pause();
        assert!(clock.is_paused());
        let frozen = clock.now();
        clock.resume();
        assert!(!clock.is_paused());
        // After resume, now() should be >= frozen time (not earlier)
        assert!(clock.now() >= frozen);
    }

    #[test]
    fn test_pause_is_idempotent() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.pause();
        let t1 = clock.now();
        clock.pause(); // second pause is a no-op
        assert!(clock.is_paused());
        assert_eq!(clock.now(), t1);
    }

    #[test]
    fn test_resume_noop_when_not_paused() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.resume(); // no-op since not paused
        assert!(!clock.is_paused());
    }

    #[test]
    fn test_pause_advance_while_paused() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.pause();
        clock.advance(60);
        assert_eq!(clock.now().hour(), 11);
    }

    // ── GameClock inference_pause/resume ───────────────────────────────────

    #[test]
    fn test_inference_pause_resume() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        assert!(!clock.is_inference_paused());
        clock.inference_pause();
        assert!(clock.is_inference_paused());
        let frozen = clock.now();
        clock.inference_resume();
        assert!(!clock.is_inference_paused());
        assert!(clock.now() >= frozen);
    }

    #[test]
    fn test_inference_pause_is_idempotent() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.inference_pause();
        let t1 = clock.now();
        clock.inference_pause(); // second call is no-op
        assert_eq!(clock.now(), t1);
    }

    #[test]
    fn test_inference_resume_noop_when_not_paused() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.inference_resume(); // no-op
        assert!(!clock.is_inference_paused());
    }

    #[test]
    fn test_inference_and_player_pause_independent() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.pause();
        clock.inference_pause();
        assert!(clock.is_paused());
        assert!(clock.is_inference_paused());
        // resume player pause but clock stays frozen due to inference pause
        clock.resume();
        assert!(clock.is_inference_paused());
        assert!(!clock.is_paused());
        let frozen = clock.now();
        // now resume inference
        clock.inference_resume();
        assert!(!clock.is_inference_paused());
        assert!(clock.now() >= frozen);
    }

    // ── GameClock set_speed / current_speed / speed_factor ────────────────

    #[test]
    fn test_set_speed_changes_factor() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        assert!((clock.speed_factor() - 36.0).abs() < f64::EPSILON);
        clock.set_speed(GameSpeed::Fast);
        assert!((clock.speed_factor() - 72.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_current_speed_detection() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        assert_eq!(clock.current_speed(), Some(GameSpeed::Normal));
        clock.set_speed(GameSpeed::Ludicrous);
        assert_eq!(clock.current_speed(), Some(GameSpeed::Ludicrous));
    }

    #[test]
    fn test_speed_factor_getter() {
        let clock = GameClock::new(game_time(1820, 3, 20, 10));
        assert!((clock.speed_factor() - 36.0).abs() < f64::EPSILON);
    }

    // ── GameClock accessors ────────────────────────────────────────────────

    #[test]
    fn test_start_game_getter() {
        let t = game_time(1820, 3, 20, 10);
        let clock = GameClock::new(t);
        assert_eq!(clock.start_game(), t);
    }

    #[test]
    fn test_paused_game_time_initialized() {
        let t = game_time(1820, 3, 20, 10);
        let clock = GameClock::new(t);
        assert_eq!(clock.paused_game_time(), t);
    }

    #[test]
    fn test_real_elapsed_secs_positive() {
        let clock = GameClock::new(game_time(1820, 3, 20, 10));
        let secs = clock.real_elapsed_secs();
        assert!(secs >= 0.0, "elapsed time must be non-negative");
    }

    // ── GameSpeed::from_name / activation_message ──────────────────────────

    #[test]
    fn test_game_speed_from_name() {
        assert_eq!(GameSpeed::from_name("slow"), Some(GameSpeed::Slow));
        assert_eq!(GameSpeed::from_name("normal"), Some(GameSpeed::Normal));
        assert_eq!(GameSpeed::from_name("fast"), Some(GameSpeed::Fast));
        assert_eq!(GameSpeed::from_name("fastest"), Some(GameSpeed::Fastest));
        assert_eq!(
            GameSpeed::from_name("ludicrous"),
            Some(GameSpeed::Ludicrous)
        );
        assert_eq!(GameSpeed::from_name("SLOW"), Some(GameSpeed::Slow));
        assert_eq!(GameSpeed::from_name("unknown"), None);
        assert_eq!(GameSpeed::from_name(""), None);
    }

    #[test]
    fn test_game_speed_activation_message() {
        assert!(GameSpeed::Slow.activation_message().contains("amble"));
        assert!(GameSpeed::Normal.activation_message().contains("stride"));
        assert!(GameSpeed::Fast.activation_message().contains("quickens"));
        assert!(GameSpeed::Fastest.activation_message().contains("hat"));
        assert!(GameSpeed::Ludicrous.activation_message().contains("blur"));
    }

    // ── GameClock with_speed ──────────────────────────────────────────────

    #[test]
    fn test_game_clock_with_speed() {
        let clock = GameClock::with_speed(game_time(1820, 3, 20, 10), 72.0);
        assert!((clock.speed_factor() - 72.0).abs() < f64::EPSILON);
    }

    // ── Display round-trip tests ───────────────────────────────────────────

    #[test]
    fn test_festival_display() {
        assert_eq!(Festival::Imbolc.to_string(), "Imbolc");
        assert_eq!(Festival::Bealtaine.to_string(), "Bealtaine");
        assert_eq!(Festival::Lughnasa.to_string(), "Lughnasa");
        assert_eq!(Festival::Samhain.to_string(), "Samhain");
    }

    #[test]
    fn test_season_display() {
        assert_eq!(Season::Spring.to_string(), "Spring");
        assert_eq!(Season::Summer.to_string(), "Summer");
        assert_eq!(Season::Autumn.to_string(), "Autumn");
        assert_eq!(Season::Winter.to_string(), "Winter");
    }

    #[test]
    fn test_time_of_day_display() {
        assert_eq!(TimeOfDay::Dawn.to_string(), "Dawn");
        assert_eq!(TimeOfDay::Morning.to_string(), "Morning");
        assert_eq!(TimeOfDay::Midday.to_string(), "Midday");
        assert_eq!(TimeOfDay::Afternoon.to_string(), "Afternoon");
        assert_eq!(TimeOfDay::Dusk.to_string(), "Dusk");
        assert_eq!(TimeOfDay::Night.to_string(), "Night");
        assert_eq!(TimeOfDay::Midnight.to_string(), "Midnight");
    }

    #[test]
    fn test_game_speed_display() {
        assert_eq!(GameSpeed::Slow.to_string(), "Slow");
        assert_eq!(GameSpeed::Normal.to_string(), "Normal");
        assert_eq!(GameSpeed::Fast.to_string(), "Fast");
        assert_eq!(GameSpeed::Fastest.to_string(), "Fastest");
        assert_eq!(GameSpeed::Ludicrous.to_string(), "Ludicrous");
    }

    // ── GameClock set_speed while frozen ───────────────────────────────────

    #[test]
    fn test_set_speed_while_frozen() {
        let mut clock = GameClock::new(game_time(1820, 3, 20, 10));
        clock.pause();
        let frozen = clock.now();
        clock.set_speed(GameSpeed::Fast);
        assert!(clock.is_paused());
        assert_eq!(clock.now(), frozen);
        assert!((clock.speed_factor() - 72.0).abs() < f64::EPSILON);
    }

    // ── weekday_name (shared with parish-diagnostics + ipc handlers) ────────

    #[test]
    fn test_weekday_name() {
        use chrono::Weekday;
        assert_eq!(weekday_name(Weekday::Mon), "Monday");
        assert_eq!(weekday_name(Weekday::Tue), "Tuesday");
        assert_eq!(weekday_name(Weekday::Wed), "Wednesday");
        assert_eq!(weekday_name(Weekday::Thu), "Thursday");
        assert_eq!(weekday_name(Weekday::Fri), "Friday");
        assert_eq!(weekday_name(Weekday::Sat), "Saturday");
        assert_eq!(weekday_name(Weekday::Sun), "Sunday");
    }
}
