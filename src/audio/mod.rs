//! Audio playback system for ambient sounds.
//!
//! Manages rodio output streams and audio channels (Sinks) for layered
//! ambient sound playback. GUI-mode only — gated behind the `audio`
//! cargo feature flag.

pub mod ambient;
pub mod catalog;
pub mod propagation;

use std::io::BufReader;
use std::path::{Path, PathBuf};

use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use tracing::{debug, warn};

use crate::world::time::{Season, TimeOfDay};
use crate::world::{LocationId, LocationKind, Weather};

/// Snapshot of game state relevant to audio decisions.
///
/// Built from `WorldState` each frame, passed to `AmbientEngine::tick()`.
#[derive(Debug, Clone, PartialEq)]
pub struct GameState {
    /// The player's current location id.
    pub player_location: LocationId,
    /// The kind of location the player is at.
    pub location_kind: LocationKind,
    /// Current time of day.
    pub time_of_day: TimeOfDay,
    /// Current season.
    pub season: Season,
    /// Current weather.
    pub weather: Weather,
    /// Whether the player is indoors.
    pub indoor: bool,
}

/// Identifies one of the five audio mixing channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// Base ambient layer (wind, water, silence). Always playing.
    Ambient,
    /// Music layer (fiddle, hymns). Fades in/out.
    Music,
    /// Nature layer (birds, animals). Intermittent.
    Nature,
    /// Weather overlay (rain, wind, thunder). Fades with weather changes.
    WeatherOverlay,
    /// One-shot events (bell toll, rooster crow, door slam).
    Events,
}

/// Manages audio output via rodio.
///
/// Owns the `OutputStream` (which must stay alive for audio to play)
/// and provides five independent `Sink` channels for layered mixing.
pub struct AudioManager {
    /// The output stream — must be kept alive.
    _stream: OutputStream,
    /// Handle for creating new sinks (retained for future use).
    #[allow(dead_code)]
    stream_handle: OutputStreamHandle,
    /// The five audio channels.
    ambient: Sink,
    music: Sink,
    nature: Sink,
    weather: Sink,
    events: Sink,
    /// Base path for audio assets.
    assets_path: PathBuf,
}

impl AudioManager {
    /// Attempts to create a new AudioManager.
    ///
    /// Returns `None` if the audio output device is unavailable (e.g.,
    /// headless CI environments). Logs a warning on failure.
    pub fn new() -> Option<Self> {
        let (stream, stream_handle) = match OutputStream::try_default() {
            Ok(pair) => pair,
            Err(e) => {
                warn!("Audio: no output device available, audio disabled: {e}");
                return None;
            }
        };

        let make_sink = |handle: &OutputStreamHandle| -> Option<Sink> {
            match Sink::try_new(handle) {
                Ok(sink) => Some(sink),
                Err(e) => {
                    warn!("Audio: failed to create sink: {e}");
                    None
                }
            }
        };

        let ambient = make_sink(&stream_handle)?;
        let music = make_sink(&stream_handle)?;
        let nature = make_sink(&stream_handle)?;
        let weather = make_sink(&stream_handle)?;
        let events = make_sink(&stream_handle)?;

        Some(Self {
            _stream: stream,
            stream_handle,
            ambient,
            music,
            nature,
            weather,
            events,
            assets_path: PathBuf::from("assets/audio"),
        })
    }

    /// Returns a reference to the sink for the given channel.
    fn sink(&self, channel: Channel) -> &Sink {
        match channel {
            Channel::Ambient => &self.ambient,
            Channel::Music => &self.music,
            Channel::Nature => &self.nature,
            Channel::WeatherOverlay => &self.weather,
            Channel::Events => &self.events,
        }
    }

    /// Sets the volume for a channel (0.0 = silent, 1.0 = full).
    pub fn set_volume(&self, channel: Channel, volume: f32) {
        self.sink(channel).set_volume(volume.clamp(0.0, 1.0));
    }

    /// Returns the current volume of a channel.
    pub fn volume(&self, channel: Channel) -> f32 {
        self.sink(channel).volume()
    }

    /// Stops playback on a channel and clears its queue.
    pub fn stop(&self, channel: Channel) {
        self.sink(channel).stop();
    }

    /// Returns true if a channel has no audio queued or playing.
    pub fn is_empty(&self, channel: Channel) -> bool {
        self.sink(channel).empty()
    }

    /// Plays an audio file on the given channel.
    ///
    /// The `path` is relative to the `assets/audio/` directory.
    /// If the file doesn't exist, logs a debug message and returns false.
    /// If `looping` is true, the source repeats indefinitely.
    pub fn play(&self, channel: Channel, path: &str, looping: bool, volume: f32) -> bool {
        let full_path = self.assets_path.join(path);
        match Self::load_source(&full_path) {
            Some(source) => {
                let sink = self.sink(channel);
                sink.set_volume(volume.clamp(0.0, 1.0));
                if looping {
                    sink.append(source.repeat_infinite());
                } else {
                    sink.append(source);
                }
                true
            }
            None => false,
        }
    }

    /// Loads a decoder from a file path. Returns None if the file is
    /// missing or unreadable.
    fn load_source(path: &Path) -> Option<Decoder<BufReader<std::fs::File>>> {
        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => {
                debug!("Audio: asset not found: {}", path.display());
                return None;
            }
        };
        match Decoder::new(BufReader::new(file)) {
            Ok(source) => Some(source),
            Err(e) => {
                warn!("Audio: failed to decode {}: {e}", path.display());
                None
            }
        }
    }

    /// Pauses all channels.
    pub fn pause_all(&self) {
        self.ambient.pause();
        self.music.pause();
        self.nature.pause();
        self.weather.pause();
        self.events.pause();
    }

    /// Resumes all channels.
    pub fn resume_all(&self) {
        self.ambient.play();
        self.music.play();
        self.nature.play();
        self.weather.play();
        self.events.play();
    }

    /// Stops all channels.
    pub fn stop_all(&self) {
        self.ambient.stop();
        self.music.stop();
        self.nature.stop();
        self.weather.stop();
        self.events.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_state_equality() {
        let state1 = GameState {
            player_location: LocationId(1),
            location_kind: LocationKind::Crossroads,
            time_of_day: TimeOfDay::Dawn,
            season: Season::Spring,
            weather: Weather::Clear,
            indoor: false,
        };
        let state2 = state1.clone();
        assert_eq!(state1, state2);
    }

    #[test]
    fn test_game_state_inequality() {
        let state1 = GameState {
            player_location: LocationId(1),
            location_kind: LocationKind::Crossroads,
            time_of_day: TimeOfDay::Dawn,
            season: Season::Spring,
            weather: Weather::Clear,
            indoor: false,
        };
        let state2 = GameState {
            player_location: LocationId(2),
            ..state1.clone()
        };
        assert_ne!(state1, state2);
    }

    #[test]
    fn test_channel_variants() {
        let channels = [
            Channel::Ambient,
            Channel::Music,
            Channel::Nature,
            Channel::WeatherOverlay,
            Channel::Events,
        ];
        assert_eq!(channels.len(), 5);
        assert_ne!(Channel::Ambient, Channel::Music);
    }
}
