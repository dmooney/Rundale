//! Sound catalog — what sounds exist and when they play.
//!
//! Maps `(LocationKind, TimeOfDay, Season, Weather)` combinations to
//! audio asset paths. The catalog is built once at startup from a
//! static table derived from the design document.

use crate::world::time::{Season, TimeOfDay};
use crate::world::{LocationKind, Weather};

use super::Channel;

/// How far a sound carries through the world graph.
#[derive(Debug, Clone, PartialEq)]
pub enum Propagation {
    /// Only audible at the source location.
    Local,
    /// Audible at source + direct neighbors (1 edge).
    Near,
    /// Audible up to ~15 minutes traversal away.
    Medium,
    /// Audible up to `max_minutes` total traversal time from source.
    /// Church bells use `Far(60)` to cover the entire parish.
    Far(u16),
}

impl Propagation {
    /// Returns the maximum distance in minutes this propagation covers.
    pub fn max_distance(&self) -> u16 {
        match self {
            Propagation::Local => 0,
            Propagation::Near => 5, // typical edge weight
            Propagation::Medium => 15,
            Propagation::Far(max) => *max,
        }
    }

    /// Returns true if a sound with this propagation is audible at the
    /// given distance in minutes.
    pub fn reaches(&self, distance_minutes: u16) -> bool {
        match self {
            Propagation::Local => distance_minutes == 0,
            Propagation::Near => distance_minutes <= 10,
            Propagation::Medium => distance_minutes <= 15,
            Propagation::Far(max) => distance_minutes <= *max,
        }
    }
}

/// Time-of-day filter for a sound entry.
#[derive(Debug, Clone, PartialEq)]
pub enum TimeFilter {
    /// Plays at any time.
    Any,
    /// Plays only during these times.
    Only(Vec<TimeOfDay>),
    /// Plays at all times except these.
    Except(Vec<TimeOfDay>),
}

impl TimeFilter {
    /// Returns true if the filter matches the given time of day.
    pub fn matches(&self, time: TimeOfDay) -> bool {
        match self {
            TimeFilter::Any => true,
            TimeFilter::Only(times) => times.contains(&time),
            TimeFilter::Except(times) => !times.contains(&time),
        }
    }
}

/// Seasonal filter for a sound entry.
#[derive(Debug, Clone, PartialEq)]
pub enum SeasonFilter {
    /// Plays in any season.
    Any,
    /// Plays only during these seasons.
    Only(Vec<Season>),
}

impl SeasonFilter {
    /// Returns true if the filter matches the given season.
    pub fn matches(&self, season: Season) -> bool {
        match self {
            SeasonFilter::Any => true,
            SeasonFilter::Only(seasons) => seasons.contains(&season),
        }
    }
}

/// Weather filter for a sound entry.
#[derive(Debug, Clone, PartialEq)]
pub enum WeatherFilter {
    /// Plays in any weather.
    Any,
    /// Plays only during these weather conditions.
    Only(Vec<Weather>),
    /// Plays in all weather except these.
    Except(Vec<Weather>),
}

impl WeatherFilter {
    /// Returns true if the filter matches the given weather.
    pub fn matches(&self, weather: Weather) -> bool {
        match self {
            WeatherFilter::Any => true,
            WeatherFilter::Only(conditions) => conditions.contains(&weather),
            WeatherFilter::Except(conditions) => !conditions.contains(&weather),
        }
    }
}

/// Whether a sound loops or plays once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopMode {
    /// Plays continuously, crossfading when restarting.
    Loop,
    /// Plays once; the engine may select it again later.
    OneShot,
}

/// A single sound asset with its playback conditions.
#[derive(Debug, Clone)]
pub struct SoundEntry {
    /// Path to the audio file relative to `assets/audio/`.
    pub path: &'static str,
    /// What location type produces this sound.
    pub source_kind: LocationKind,
    /// How far the sound carries through the graph.
    pub propagation: Propagation,
    /// When this sound can play (time of day).
    pub time_filter: TimeFilter,
    /// Seasonal constraint.
    pub season_filter: SeasonFilter,
    /// Weather constraint.
    pub weather_filter: WeatherFilter,
    /// Whether this is a looping ambient layer or a one-shot event.
    pub loop_mode: LoopMode,
    /// Relative volume (0.0–1.0) before distance attenuation.
    pub base_volume: f32,
    /// Which audio channel this sound plays on.
    pub channel: Channel,
    /// Whether this is a global weather overlay (not tied to a location).
    pub is_weather_overlay: bool,
}

/// The complete catalog of ambient sounds.
///
/// Built once at startup from a hardcoded table. All sound entries are
/// tagged with their location kind, time/season/weather filters, and
/// propagation range.
pub struct SoundCatalog {
    entries: Vec<SoundEntry>,
}

impl SoundCatalog {
    /// Builds the complete sound catalog from the design specification.
    pub fn new() -> Self {
        Self {
            entries: build_catalog(),
        }
    }

    /// Returns all sound entries.
    pub fn entries(&self) -> &[SoundEntry] {
        &self.entries
    }

    /// Returns all entries for a given location kind.
    pub fn entries_for_kind(&self, kind: LocationKind) -> Vec<&SoundEntry> {
        self.entries
            .iter()
            .filter(|e| !e.is_weather_overlay && e.source_kind == kind)
            .collect()
    }

    /// Returns all weather overlay entries.
    pub fn weather_entries(&self) -> Vec<&SoundEntry> {
        self.entries
            .iter()
            .filter(|e| e.is_weather_overlay)
            .collect()
    }

    /// Returns entries matching the full filter set for a specific location.
    pub fn matching_entries(
        &self,
        kind: LocationKind,
        time: TimeOfDay,
        season: Season,
        weather: Weather,
    ) -> Vec<&SoundEntry> {
        self.entries
            .iter()
            .filter(|e| {
                !e.is_weather_overlay
                    && e.source_kind == kind
                    && e.time_filter.matches(time)
                    && e.season_filter.matches(season)
                    && e.weather_filter.matches(weather)
            })
            .collect()
    }

    /// Returns weather overlays matching the current weather.
    pub fn matching_weather_entries(&self, weather: Weather) -> Vec<&SoundEntry> {
        self.entries
            .iter()
            .filter(|e| e.is_weather_overlay && e.weather_filter.matches(weather))
            .collect()
    }

    /// Returns the total number of entries in the catalog.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the catalog is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for SoundCatalog {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds the full sound catalog from the design document tables.
#[allow(clippy::vec_init_then_push)] // catalog is built incrementally by section
fn build_catalog() -> Vec<SoundEntry> {
    let mut entries = Vec::new();

    // === Pub sounds ===
    entries.push(SoundEntry {
        path: "pub/fiddle_reel.ogg",
        source_kind: LocationKind::Pub,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Night, TimeOfDay::Dusk]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.8,
        channel: Channel::Music,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "pub/sean_nos.ogg",
        source_kind: LocationKind::Pub,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Night]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.6,
        channel: Channel::Music,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "pub/crowd_murmur.ogg",
        source_kind: LocationKind::Pub,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.4,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "pub/glasses_clink.ogg",
        source_kind: LocationKind::Pub,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
            TimeOfDay::Night,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.3,
        channel: Channel::Events,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "pub/hearth_crackling.ogg",
        source_kind: LocationKind::Pub,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Only(vec![Season::Autumn, Season::Winter]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.25,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Church sounds ===
    entries.push(SoundEntry {
        path: "church/bell_angelus.ogg",
        source_kind: LocationKind::Church,
        propagation: Propagation::Far(60),
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dawn, TimeOfDay::Midday, TimeOfDay::Dusk]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 1.0,
        channel: Channel::Events,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "church/bell_sunday.ogg",
        source_kind: LocationKind::Church,
        propagation: Propagation::Far(60),
        time_filter: TimeFilter::Only(vec![TimeOfDay::Morning]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 1.0,
        channel: Channel::Events,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "church/hymns.ogg",
        source_kind: LocationKind::Church,
        propagation: Propagation::Medium,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Morning]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.5,
        channel: Channel::Music,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "church/stone_echo.ogg",
        source_kind: LocationKind::Church,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.15,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Farm sounds ===
    entries.push(SoundEntry {
        path: "farm/rooster.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dawn]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.7,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "farm/cattle_lowing.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.5,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "farm/sheep_bleating.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.4,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "farm/dog_barking.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.5,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "farm/donkey_braying.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Medium,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.6,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "farm/hens_clucking.ogg",
        source_kind: LocationKind::Farm,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.25,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });

    // === Waterside sounds ===
    entries.push(SoundEntry {
        path: "water/water_lapping.ogg",
        source_kind: LocationKind::Waterside,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.5,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "water/wind_in_reeds.ogg",
        source_kind: LocationKind::Waterside,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.35,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "water/waterfowl.ogg",
        source_kind: LocationKind::Waterside,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dawn, TimeOfDay::Dusk]),
        season_filter: SeasonFilter::Only(vec![Season::Spring, Season::Summer]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.4,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });

    // === Bog sounds ===
    entries.push(SoundEntry {
        path: "bog/wind_over_bog.ogg",
        source_kind: LocationKind::Bog,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.45,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "bog/curlew_call.ogg",
        source_kind: LocationKind::Bog,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dawn, TimeOfDay::Dusk]),
        season_filter: SeasonFilter::Only(vec![Season::Spring, Season::Summer]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.5,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "bog/bog_silence.ogg",
        source_kind: LocationKind::Bog,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Night, TimeOfDay::Midnight]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.1,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Crossroads sounds ===
    entries.push(SoundEntry {
        path: "crossroads/dance_music.ogg",
        source_kind: LocationKind::Crossroads,
        propagation: Propagation::Near,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dusk, TimeOfDay::Night]),
        season_filter: SeasonFilter::Only(vec![Season::Summer]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.7,
        channel: Channel::Music,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "crossroads/wind.ogg",
        source_kind: LocationKind::Crossroads,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.3,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Village sounds ===
    entries.push(SoundEntry {
        path: "village/rooster.ogg",
        source_kind: LocationKind::Village,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dawn]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.5,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "village/children_playing.ogg",
        source_kind: LocationKind::Village,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Morning, TimeOfDay::Afternoon]),
        season_filter: SeasonFilter::Only(vec![Season::Spring, Season::Summer, Season::Autumn]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.4,
        channel: Channel::Nature,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "village/door_footsteps.ogg",
        source_kind: LocationKind::Village,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![
            TimeOfDay::Morning,
            TimeOfDay::Midday,
            TimeOfDay::Afternoon,
            TimeOfDay::Dusk,
        ]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::OneShot,
        base_volume: 0.3,
        channel: Channel::Events,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "village/evening_settling.ogg",
        source_kind: LocationKind::Village,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Dusk]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.25,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Fairy Fort sounds ===
    entries.push(SoundEntry {
        path: "fairy_fort/hawthorn_wind.ogg",
        source_kind: LocationKind::FairyFort,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.35,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "fairy_fort/uncanny_silence.ogg",
        source_kind: LocationKind::FairyFort,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Night, TimeOfDay::Midnight]),
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.1,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });
    entries.push(SoundEntry {
        path: "fairy_fort/samhain_atmosphere.ogg",
        source_kind: LocationKind::FairyFort,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Only(vec![TimeOfDay::Night]),
        season_filter: SeasonFilter::Only(vec![Season::Autumn]),
        weather_filter: WeatherFilter::Any,
        loop_mode: LoopMode::Loop,
        base_volume: 0.3,
        channel: Channel::Ambient,
        is_weather_overlay: false,
    });

    // === Weather overlay sounds (global, not location-specific) ===
    entries.push(SoundEntry {
        path: "weather/light_rain.ogg",
        source_kind: LocationKind::Road, // not used for weather overlays
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Only(vec![Weather::Rain]),
        loop_mode: LoopMode::Loop,
        base_volume: 0.5,
        channel: Channel::WeatherOverlay,
        is_weather_overlay: true,
    });
    entries.push(SoundEntry {
        path: "weather/heavy_rain.ogg",
        source_kind: LocationKind::Road,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Only(vec![Weather::Storm]),
        loop_mode: LoopMode::Loop,
        base_volume: 0.7,
        channel: Channel::WeatherOverlay,
        is_weather_overlay: true,
    });
    entries.push(SoundEntry {
        path: "weather/wind.ogg",
        source_kind: LocationKind::Road,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Only(vec![Weather::Rain, Weather::Storm]),
        loop_mode: LoopMode::Loop,
        base_volume: 0.4,
        channel: Channel::WeatherOverlay,
        is_weather_overlay: true,
    });
    entries.push(SoundEntry {
        path: "weather/thunder.ogg",
        source_kind: LocationKind::Road,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Only(vec![Weather::Storm]),
        loop_mode: LoopMode::OneShot,
        base_volume: 0.8,
        channel: Channel::Events,
        is_weather_overlay: true,
    });
    entries.push(SoundEntry {
        path: "weather/fog_silence.ogg",
        source_kind: LocationKind::Road,
        propagation: Propagation::Local,
        time_filter: TimeFilter::Any,
        season_filter: SeasonFilter::Any,
        weather_filter: WeatherFilter::Only(vec![Weather::Fog]),
        loop_mode: LoopMode::Loop,
        base_volume: 0.15,
        channel: Channel::WeatherOverlay,
        is_weather_overlay: true,
    });

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_propagation_max_distance() {
        assert_eq!(Propagation::Local.max_distance(), 0);
        assert_eq!(Propagation::Near.max_distance(), 5);
        assert_eq!(Propagation::Medium.max_distance(), 15);
        assert_eq!(Propagation::Far(60).max_distance(), 60);
    }

    #[test]
    fn test_propagation_reaches() {
        assert!(Propagation::Local.reaches(0));
        assert!(!Propagation::Local.reaches(1));

        assert!(Propagation::Near.reaches(0));
        assert!(Propagation::Near.reaches(5));
        assert!(Propagation::Near.reaches(10));
        assert!(!Propagation::Near.reaches(11));

        assert!(Propagation::Medium.reaches(15));
        assert!(!Propagation::Medium.reaches(16));

        assert!(Propagation::Far(60).reaches(60));
        assert!(!Propagation::Far(60).reaches(61));
    }

    #[test]
    fn test_time_filter_any() {
        let filter = TimeFilter::Any;
        assert!(filter.matches(TimeOfDay::Dawn));
        assert!(filter.matches(TimeOfDay::Midnight));
    }

    #[test]
    fn test_time_filter_only() {
        let filter = TimeFilter::Only(vec![TimeOfDay::Dawn, TimeOfDay::Dusk]);
        assert!(filter.matches(TimeOfDay::Dawn));
        assert!(filter.matches(TimeOfDay::Dusk));
        assert!(!filter.matches(TimeOfDay::Midday));
    }

    #[test]
    fn test_time_filter_except() {
        let filter = TimeFilter::Except(vec![TimeOfDay::Midnight]);
        assert!(filter.matches(TimeOfDay::Dawn));
        assert!(!filter.matches(TimeOfDay::Midnight));
    }

    #[test]
    fn test_season_filter_any() {
        let filter = SeasonFilter::Any;
        assert!(filter.matches(Season::Spring));
        assert!(filter.matches(Season::Winter));
    }

    #[test]
    fn test_season_filter_only() {
        let filter = SeasonFilter::Only(vec![Season::Summer]);
        assert!(filter.matches(Season::Summer));
        assert!(!filter.matches(Season::Winter));
    }

    #[test]
    fn test_weather_filter_any() {
        let filter = WeatherFilter::Any;
        assert!(filter.matches(Weather::Clear));
        assert!(filter.matches(Weather::Storm));
    }

    #[test]
    fn test_weather_filter_only() {
        let filter = WeatherFilter::Only(vec![Weather::Rain, Weather::Storm]);
        assert!(filter.matches(Weather::Rain));
        assert!(filter.matches(Weather::Storm));
        assert!(!filter.matches(Weather::Clear));
    }

    #[test]
    fn test_weather_filter_except() {
        let filter = WeatherFilter::Except(vec![Weather::Storm]);
        assert!(filter.matches(Weather::Clear));
        assert!(!filter.matches(Weather::Storm));
    }

    #[test]
    fn test_catalog_not_empty() {
        let catalog = SoundCatalog::new();
        assert!(!catalog.is_empty());
        assert!(catalog.len() > 30);
    }

    #[test]
    fn test_catalog_pub_entries() {
        let catalog = SoundCatalog::new();
        let pub_entries = catalog.entries_for_kind(LocationKind::Pub);
        assert!(pub_entries.len() >= 5);
        assert!(pub_entries.iter().any(|e| e.path.contains("fiddle")));
    }

    #[test]
    fn test_catalog_church_bells() {
        let catalog = SoundCatalog::new();
        let church = catalog.entries_for_kind(LocationKind::Church);
        let bells: Vec<_> = church.iter().filter(|e| e.path.contains("bell")).collect();
        assert!(bells.len() >= 2);
        for bell in &bells {
            assert!(matches!(bell.propagation, Propagation::Far(60)));
        }
    }

    #[test]
    fn test_catalog_weather_entries() {
        let catalog = SoundCatalog::new();
        let weather = catalog.weather_entries();
        assert!(weather.len() >= 5);
        assert!(weather.iter().all(|e| e.is_weather_overlay));
    }

    #[test]
    fn test_catalog_matching_entries() {
        let catalog = SoundCatalog::new();
        let matches = catalog.matching_entries(
            LocationKind::Pub,
            TimeOfDay::Night,
            Season::Winter,
            Weather::Clear,
        );
        assert!(!matches.is_empty());
        // Fiddle reel plays at night
        assert!(matches.iter().any(|e| e.path.contains("fiddle")));
        // Hearth plays in winter
        assert!(matches.iter().any(|e| e.path.contains("hearth")));
    }

    #[test]
    fn test_catalog_matching_weather() {
        let catalog = SoundCatalog::new();
        let storm = catalog.matching_weather_entries(Weather::Storm);
        assert!(storm.iter().any(|e| e.path.contains("heavy_rain")));
        assert!(storm.iter().any(|e| e.path.contains("thunder")));

        let clear = catalog.matching_weather_entries(Weather::Clear);
        assert!(clear.is_empty());
    }

    #[test]
    fn test_catalog_no_duplicate_paths() {
        let catalog = SoundCatalog::new();
        let mut paths: Vec<&str> = catalog.entries().iter().map(|e| e.path).collect();
        paths.sort();
        let len_before = paths.len();
        paths.dedup();
        assert_eq!(paths.len(), len_before, "catalog has duplicate paths");
    }

    #[test]
    fn test_all_entries_have_valid_volume() {
        let catalog = SoundCatalog::new();
        for entry in catalog.entries() {
            assert!(
                (0.0..=1.0).contains(&entry.base_volume),
                "entry {} has invalid volume {}",
                entry.path,
                entry.base_volume
            );
        }
    }

    #[test]
    fn test_fairy_fort_has_samhain() {
        let catalog = SoundCatalog::new();
        let matches = catalog.matching_entries(
            LocationKind::FairyFort,
            TimeOfDay::Night,
            Season::Autumn,
            Weather::Clear,
        );
        assert!(matches.iter().any(|e| e.path.contains("samhain")));
    }

    #[test]
    fn test_crossroads_dance_summer_only() {
        let catalog = SoundCatalog::new();
        let summer = catalog.matching_entries(
            LocationKind::Crossroads,
            TimeOfDay::Dusk,
            Season::Summer,
            Weather::Clear,
        );
        assert!(summer.iter().any(|e| e.path.contains("dance")));

        let winter = catalog.matching_entries(
            LocationKind::Crossroads,
            TimeOfDay::Dusk,
            Season::Winter,
            Weather::Clear,
        );
        assert!(!winter.iter().any(|e| e.path.contains("dance")));
    }
}
