//! Game time system.
//!
//! 40 real-world minutes = 1 in-game day (speed factor 36.0, "Normal").
//! Adjustable at runtime via [`GameSpeed`] presets (Slow/Normal/Fast/Fastest).
//! Tracks time of day, season, and Irish calendar festivals
//! (Imbolc, Bealtaine, Lughnasa, Samhain).

use chrono::{DateTime, Datelike, Duration, NaiveDate, Timelike, Utc};
use std::fmt;
use std::time::Instant;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Returns the speed factor for this preset.
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
    /// Whether the clock is paused.
    paused: bool,
    /// Game-time seconds per real-time second (default 36.0).
    speed_factor: f64,
    /// Game time when the clock was paused (only valid when `paused` is true).
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
            speed_factor: 36.0,
            paused_game_time: start_game,
        }
    }

    /// Creates a game clock with a custom speed factor.
    pub fn with_speed(start_game: DateTime<Utc>, speed_factor: f64) -> Self {
        Self {
            start_real: Instant::now(),
            start_game,
            paused: false,
            speed_factor,
            paused_game_time: start_game,
        }
    }

    /// Returns the current game time.
    ///
    /// When paused, returns the time at which the clock was paused.
    /// When running, maps elapsed real time to game time using the speed factor.
    pub fn now(&self) -> DateTime<Utc> {
        if self.paused {
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

    /// Checks if today is a festival day.
    pub fn check_festival(&self) -> Option<Festival> {
        Festival::check(self.now().date_naive())
    }

    /// Advances the game clock by the given number of game minutes.
    ///
    /// Used during travel or other time-consuming actions.
    pub fn advance(&mut self, game_minutes: i64) {
        if self.paused {
            self.paused_game_time += Duration::minutes(game_minutes);
        } else {
            self.start_game += Duration::minutes(game_minutes);
        }
    }

    /// Pauses the game clock, freezing game time.
    pub fn pause(&mut self) {
        if !self.paused {
            self.paused_game_time = self.now();
            self.paused = true;
        }
    }

    /// Resumes the game clock from where it was paused.
    pub fn resume(&mut self) {
        if self.paused {
            self.start_game = self.paused_game_time;
            self.start_real = Instant::now();
            self.paused = false;
        }
    }

    /// Returns whether the clock is paused.
    pub fn is_paused(&self) -> bool {
        self.paused
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
        if self.paused {
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
        if (self.speed_factor - 18.0).abs() < EPSILON {
            Some(GameSpeed::Slow)
        } else if (self.speed_factor - 36.0).abs() < EPSILON {
            Some(GameSpeed::Normal)
        } else if (self.speed_factor - 72.0).abs() < EPSILON {
            Some(GameSpeed::Fast)
        } else if (self.speed_factor - 144.0).abs() < EPSILON {
            Some(GameSpeed::Fastest)
        } else if (self.speed_factor - 864.0).abs() < EPSILON {
            Some(GameSpeed::Ludicrous)
        } else {
            None
        }
    }
}

/// Maps an hour (0–23) to a `TimeOfDay` variant.
fn time_of_day_from_hour(hour: u32) -> TimeOfDay {
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
    fn test_game_clock_pause_resume() {
        let mut clock = GameClock::new(game_time(2026, 6, 15, 12));
        clock.pause();
        assert!(clock.is_paused());
        assert_eq!(clock.time_of_day(), TimeOfDay::Midday);

        // Advance while paused
        clock.advance(300); // 5 hours
        assert_eq!(clock.time_of_day(), TimeOfDay::Dusk);

        clock.resume();
        assert!(!clock.is_paused());
        // After resume, time should be around 17:00 (Dusk)
        assert_eq!(clock.time_of_day(), TimeOfDay::Dusk);
    }

    #[test]
    fn test_time_of_day_display() {
        assert_eq!(TimeOfDay::Dawn.to_string(), "Dawn");
        assert_eq!(TimeOfDay::Midnight.to_string(), "Midnight");
    }

    #[test]
    fn test_season_display() {
        assert_eq!(Season::Spring.to_string(), "Spring");
        assert_eq!(Season::Winter.to_string(), "Winter");
    }

    #[test]
    fn test_festival_display() {
        assert_eq!(Festival::Imbolc.to_string(), "Imbolc");
        assert_eq!(Festival::Samhain.to_string(), "Samhain");
    }

    #[test]
    fn test_game_clock_festival() {
        let clock = GameClock::new(game_time(2026, 5, 1, 12));
        assert_eq!(clock.check_festival(), Some(Festival::Bealtaine));

        let clock = GameClock::new(game_time(2026, 5, 2, 12));
        assert_eq!(clock.check_festival(), None);
    }

    #[test]
    fn test_game_speed_factor() {
        assert!((GameSpeed::Slow.factor() - 18.0).abs() < f64::EPSILON);
        assert!((GameSpeed::Normal.factor() - 36.0).abs() < f64::EPSILON);
        assert!((GameSpeed::Fast.factor() - 72.0).abs() < f64::EPSILON);
        assert!((GameSpeed::Fastest.factor() - 144.0).abs() < f64::EPSILON);
        assert!((GameSpeed::Ludicrous.factor() - 864.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_game_speed_from_name() {
        assert_eq!(GameSpeed::from_name("slow"), Some(GameSpeed::Slow));
        assert_eq!(GameSpeed::from_name("NORMAL"), Some(GameSpeed::Normal));
        assert_eq!(GameSpeed::from_name("Fast"), Some(GameSpeed::Fast));
        assert_eq!(GameSpeed::from_name("fastest"), Some(GameSpeed::Fastest));
        assert_eq!(
            GameSpeed::from_name("ludicrous"),
            Some(GameSpeed::Ludicrous)
        );
        assert_eq!(
            GameSpeed::from_name("LUDICROUS"),
            Some(GameSpeed::Ludicrous)
        );
        assert_eq!(GameSpeed::from_name("bogus"), None);
        assert_eq!(GameSpeed::from_name(""), None);
    }

    #[test]
    fn test_game_speed_display() {
        assert_eq!(GameSpeed::Slow.to_string(), "Slow");
        assert_eq!(GameSpeed::Normal.to_string(), "Normal");
        assert_eq!(GameSpeed::Fast.to_string(), "Fast");
        assert_eq!(GameSpeed::Fastest.to_string(), "Fastest");
        assert_eq!(GameSpeed::Ludicrous.to_string(), "Ludicrous");
    }

    #[test]
    fn test_default_speed_is_normal() {
        let clock = GameClock::new(game_time(2026, 6, 15, 12));
        assert!((clock.speed_factor() - 36.0).abs() < f64::EPSILON);
        assert_eq!(clock.current_speed(), Some(GameSpeed::Normal));
    }

    #[test]
    fn test_set_speed_while_running() {
        let mut clock = GameClock::new(game_time(2026, 6, 15, 12));
        let time_before = clock.now();
        clock.set_speed(GameSpeed::Fast);
        assert!((clock.speed_factor() - 72.0).abs() < f64::EPSILON);
        // Time should be continuous (not jump)
        let time_after = clock.now();
        let diff = (time_after - time_before).num_seconds().abs();
        assert!(
            diff < 2,
            "Time should be nearly continuous after speed change"
        );
    }

    #[test]
    fn test_set_speed_while_paused() {
        let mut clock = GameClock::new(game_time(2026, 6, 15, 12));
        clock.pause();
        let paused_time = clock.now();
        clock.set_speed(GameSpeed::Fastest);
        assert!((clock.speed_factor() - 144.0).abs() < f64::EPSILON);
        // Paused time should not change
        assert_eq!(clock.now(), paused_time);
    }

    #[test]
    fn test_current_speed() {
        let mut clock = GameClock::new(game_time(2026, 6, 15, 12));
        assert_eq!(clock.current_speed(), Some(GameSpeed::Normal));

        clock.set_speed(GameSpeed::Slow);
        assert_eq!(clock.current_speed(), Some(GameSpeed::Slow));

        clock.set_speed(GameSpeed::Fast);
        assert_eq!(clock.current_speed(), Some(GameSpeed::Fast));

        clock.set_speed(GameSpeed::Fastest);
        assert_eq!(clock.current_speed(), Some(GameSpeed::Fastest));

        // Custom speed returns None
        let clock = GameClock::with_speed(game_time(2026, 6, 15, 12), 50.0);
        assert_eq!(clock.current_speed(), None);
    }
}
