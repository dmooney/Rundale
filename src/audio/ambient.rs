//! Ambient sound engine — orchestrates sound selection and scheduling.
//!
//! The `AmbientEngine` is ticked every GUI frame. It detects meaningful
//! game state changes (location, time of day, season, weather) and
//! rebuilds the active sound set when any change occurs. One-shot
//! events (rooster, bells, door slams) are triggered probabilistically
//! with cooldowns.

use std::time::{Duration, Instant};

use tracing::debug;

use crate::world::graph::WorldGraph;

use super::catalog::{LoopMode, SoundCatalog};
use super::propagation::{self, AudibleSound};
use super::{AudioManager, Channel, GameState};

/// Minimum time between one-shot events on any channel.
const EVENT_COOLDOWN: Duration = Duration::from_secs(30);

/// Tracks an actively playing sound.
#[derive(Debug, Clone)]
#[allow(dead_code)] // fields read during crossfade transitions (future)
struct ActiveSound {
    /// Path of the audio asset.
    path: &'static str,
    /// Channel this sound is playing on.
    channel: Channel,
    /// Current volume.
    volume: f32,
    /// Whether this sound loops.
    looping: bool,
}

/// The ambient sound engine.
///
/// Decides what sounds to play based on game state, manages transitions
/// when the player moves, and schedules one-shot events. Ticked every
/// GUI frame but only re-evaluates when state changes.
pub struct AmbientEngine {
    /// The sound catalog.
    catalog: SoundCatalog,
    /// Cached last-seen game state for change detection.
    last_state: Option<GameState>,
    /// Currently active looping sounds.
    active_loops: Vec<ActiveSound>,
    /// Last time a one-shot event was triggered.
    last_event_time: Instant,
    /// Whether audio is available (AudioManager was created successfully).
    audio_available: bool,
}

impl AmbientEngine {
    /// Creates a new AmbientEngine with the default sound catalog.
    pub fn new() -> Self {
        Self {
            catalog: SoundCatalog::new(),
            last_state: None,
            active_loops: Vec::new(),
            last_event_time: Instant::now(),
            audio_available: false,
        }
    }

    /// Creates an AmbientEngine with a custom catalog (for testing).
    pub fn with_catalog(catalog: SoundCatalog) -> Self {
        Self {
            catalog,
            last_state: None,
            active_loops: Vec::new(),
            last_event_time: Instant::now(),
            audio_available: false,
        }
    }

    /// Main tick — called every GUI frame.
    ///
    /// Only re-evaluates the full sound set when the game state changes
    /// meaningfully. Otherwise, it may trigger one-shot events.
    pub fn tick(&mut self, state: &GameState, graph: &WorldGraph, audio: &AudioManager) {
        self.audio_available = true;

        if self.state_changed(state) {
            self.rebuild_sound_set(state, graph, audio);
            self.last_state = Some(state.clone());
        }

        self.maybe_trigger_event(state, graph, audio);
    }

    /// Detects whether the game state has changed meaningfully.
    ///
    /// We only rebuild sounds when location, time of day, season, or
    /// weather changes — not every frame.
    pub fn state_changed(&self, state: &GameState) -> bool {
        match &self.last_state {
            None => true,
            Some(last) => {
                last.player_location != state.player_location
                    || last.time_of_day != state.time_of_day
                    || last.season != state.season
                    || last.weather != state.weather
                    || last.indoor != state.indoor
            }
        }
    }

    /// Rebuilds the active sound set for the current game state.
    ///
    /// Stops all current sounds, computes audible sounds via
    /// propagation, and starts new looping sounds on appropriate
    /// channels.
    fn rebuild_sound_set(&mut self, state: &GameState, graph: &WorldGraph, audio: &AudioManager) {
        // Stop existing loops
        audio.stop(Channel::Ambient);
        audio.stop(Channel::Music);
        audio.stop(Channel::Nature);
        audio.stop(Channel::WeatherOverlay);
        // Don't stop events — let them finish

        self.active_loops.clear();

        let audible = propagation::audible_sounds(
            state.player_location,
            graph,
            &self.catalog,
            state.time_of_day,
            state.season,
            state.weather,
            state.indoor,
        );

        // Aggregate volumes per channel (take loudest entry per channel for loops)
        let mut best_per_channel: std::collections::HashMap<Channel, Vec<&AudibleSound>> =
            std::collections::HashMap::new();

        for sound in &audible {
            if sound.entry.loop_mode == LoopMode::Loop {
                best_per_channel
                    .entry(sound.entry.channel)
                    .or_default()
                    .push(sound);
            }
        }

        // Play the loudest loop per channel
        for (channel, sounds) in &best_per_channel {
            if let Some(best) = sounds.iter().max_by(|a, b| {
                a.volume
                    .partial_cmp(&b.volume)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }) {
                let played = audio.play(*channel, best.entry.path, true, best.volume);
                if played {
                    debug!(
                        "Audio: playing {} on {:?} at volume {:.2}",
                        best.entry.path, channel, best.volume
                    );
                }
                self.active_loops.push(ActiveSound {
                    path: best.entry.path,
                    channel: *channel,
                    volume: best.volume,
                    looping: true,
                });
            }
        }
    }

    /// Probabilistically triggers one-shot events with cooldowns.
    fn maybe_trigger_event(&mut self, state: &GameState, graph: &WorldGraph, audio: &AudioManager) {
        if self.last_event_time.elapsed() < EVENT_COOLDOWN {
            return;
        }

        let audible = propagation::audible_sounds(
            state.player_location,
            graph,
            &self.catalog,
            state.time_of_day,
            state.season,
            state.weather,
            state.indoor,
        );

        // Collect one-shot candidates
        let oneshots: Vec<&AudibleSound> = audible
            .iter()
            .filter(|s| s.entry.loop_mode == LoopMode::OneShot)
            .collect();

        if oneshots.is_empty() {
            return;
        }

        // Simple probabilistic trigger: ~5% chance per tick (called at ~60fps,
        // so roughly once every ~0.3 seconds we check, meaning ~1 event per
        // ~6 seconds of eligible time after cooldown)
        let tick_probability = 0.05;
        let roll: f32 = simple_random();
        if roll > tick_probability {
            return;
        }

        // Pick a random one-shot weighted by volume
        let total_vol: f32 = oneshots.iter().map(|s| s.volume).sum();
        if total_vol <= 0.0 {
            return;
        }

        let mut pick = simple_random() * total_vol;
        for sound in &oneshots {
            pick -= sound.volume;
            if pick <= 0.0 {
                let played = audio.play(sound.entry.channel, sound.entry.path, false, sound.volume);
                if played {
                    debug!(
                        "Audio: one-shot {} on {:?} at volume {:.2}",
                        sound.entry.path, sound.entry.channel, sound.volume
                    );
                }
                self.last_event_time = Instant::now();
                return;
            }
        }
    }

    /// Returns a reference to the sound catalog.
    pub fn catalog(&self) -> &SoundCatalog {
        &self.catalog
    }

    /// Returns the number of currently active looping sounds.
    pub fn active_loop_count(&self) -> usize {
        self.active_loops.len()
    }

    /// Returns whether audio playback is available.
    pub fn is_audio_available(&self) -> bool {
        self.audio_available
    }
}

impl Default for AmbientEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple pseudo-random number generator using system time.
///
/// Returns a value in `[0.0, 1.0)`. Not cryptographically secure —
/// only used for one-shot event scheduling.
fn simple_random() -> f32 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    (nanos % 10000) as f32 / 10000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::graph::WorldGraph;
    use crate::world::time::{Season, TimeOfDay};
    use crate::world::{LocationId, LocationKind, Weather};

    fn test_graph() -> WorldGraph {
        let json = r#"{
            "locations": [
                {
                    "id": 1, "name": "The Crossroads",
                    "description_template": "test", "indoor": false, "public": true,
                    "location_kind": "crossroads",
                    "connections": [
                        {"target": 2, "traversal_minutes": 3, "path_description": "lane"}
                    ]
                },
                {
                    "id": 2, "name": "Darcy's Pub",
                    "description_template": "test", "indoor": true, "public": true,
                    "location_kind": "pub",
                    "connections": [
                        {"target": 1, "traversal_minutes": 3, "path_description": "lane"}
                    ]
                }
            ]
        }"#;
        WorldGraph::load_from_str(json).unwrap()
    }

    fn make_state(location: u32, kind: LocationKind, time: TimeOfDay) -> GameState {
        GameState {
            player_location: LocationId(location),
            location_kind: kind,
            time_of_day: time,
            season: Season::Summer,
            weather: Weather::Clear,
            indoor: false,
        }
    }

    #[test]
    fn test_state_change_detection_initial() {
        let engine = AmbientEngine::new();
        let state = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        assert!(engine.state_changed(&state));
    }

    #[test]
    fn test_state_change_detection_same() {
        let mut engine = AmbientEngine::new();
        let state = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        engine.last_state = Some(state.clone());
        assert!(!engine.state_changed(&state));
    }

    #[test]
    fn test_state_change_detection_location_change() {
        let mut engine = AmbientEngine::new();
        let state1 = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        engine.last_state = Some(state1);
        let state2 = make_state(2, LocationKind::Pub, TimeOfDay::Morning);
        assert!(engine.state_changed(&state2));
    }

    #[test]
    fn test_state_change_detection_time_change() {
        let mut engine = AmbientEngine::new();
        let state1 = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        engine.last_state = Some(state1);
        let state2 = make_state(1, LocationKind::Crossroads, TimeOfDay::Afternoon);
        assert!(engine.state_changed(&state2));
    }

    #[test]
    fn test_state_change_detection_weather_change() {
        let mut engine = AmbientEngine::new();
        let state1 = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        engine.last_state = Some(state1);
        let mut state2 = make_state(1, LocationKind::Crossroads, TimeOfDay::Morning);
        state2.weather = Weather::Rain;
        assert!(engine.state_changed(&state2));
    }

    #[test]
    fn test_engine_default() {
        let engine = AmbientEngine::default();
        assert!(!engine.is_audio_available());
        assert_eq!(engine.active_loop_count(), 0);
        assert!(!engine.catalog().is_empty());
    }

    #[test]
    fn test_simple_random_range() {
        for _ in 0..100 {
            let val = simple_random();
            assert!((0.0..1.0).contains(&val), "random value {val} out of range");
        }
    }
}
