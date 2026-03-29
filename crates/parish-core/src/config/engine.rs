//! Engine configuration structs for tunable parameters.
//!
//! Every struct derives `Deserialize` and has a `Default` implementation
//! that returns the original hardcoded values, ensuring backward compatibility
//! when no config file is present.
//!
//! These are ENGINE-LEVEL parameters (timeouts, game mechanics, palette tuning).
//! Game-specific CONTENT (prompts, loading phrases, encounter text) lives in
//! the mod system (`GameMod` / `mod.toml`).

use serde::Deserialize;

/// Root engine configuration parsed from `[engine]` section of `parish.toml`.
#[derive(Debug, Default, Deserialize, Clone)]
pub struct EngineConfig {
    /// LLM inference timeouts.
    #[serde(default)]
    pub inference: InferenceConfig,
    /// Game speed presets.
    #[serde(default)]
    pub speeds: SpeedConfig,
    /// Encounter probability by time of day.
    #[serde(default)]
    pub encounters: EncounterConfig,
    /// NPC memory, cognition, and relationship tuning.
    #[serde(default)]
    pub npc: NpcConfig,
    /// Color palette tints and contrast.
    #[serde(default)]
    pub palette: PaletteConfig,
    /// World graph tuning.
    #[serde(default)]
    pub world: WorldConfig,
    /// Persistence / save system tuning.
    #[serde(default)]
    pub persistence: PersistenceConfig,
}

// ---------------------------------------------------------------------------
// Inference
// ---------------------------------------------------------------------------

/// LLM inference timeouts.
#[derive(Debug, Deserialize, Clone)]
pub struct InferenceConfig {
    /// Non-streaming request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
    /// Streaming request timeout in seconds.
    #[serde(default = "default_streaming_timeout_secs")]
    pub streaming_timeout_secs: u64,
    /// Ollama reachability check timeout in seconds.
    #[serde(default = "default_reachability_timeout_secs")]
    pub reachability_timeout_secs: u64,
    /// Model download timeout in seconds.
    #[serde(default = "default_model_download_timeout_secs")]
    pub model_download_timeout_secs: u64,
    /// Model loading/warmup timeout in seconds.
    #[serde(default = "default_model_loading_timeout_secs")]
    pub model_loading_timeout_secs: u64,
    /// Maximum entries in the debug inference log ring buffer.
    #[serde(default = "default_log_capacity")]
    pub log_capacity: usize,
}

impl Default for InferenceConfig {
    fn default() -> Self {
        Self {
            timeout_secs: 30,
            streaming_timeout_secs: 300,
            reachability_timeout_secs: 10,
            model_download_timeout_secs: 3600,
            model_loading_timeout_secs: 300,
            log_capacity: 50,
        }
    }
}

fn default_timeout_secs() -> u64 {
    30
}
fn default_streaming_timeout_secs() -> u64 {
    300
}
fn default_reachability_timeout_secs() -> u64 {
    10
}
fn default_model_download_timeout_secs() -> u64 {
    3600
}
fn default_model_loading_timeout_secs() -> u64 {
    300
}
fn default_log_capacity() -> usize {
    50
}

// ---------------------------------------------------------------------------
// Game Speed
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Encounters
// ---------------------------------------------------------------------------

/// Encounter probability thresholds by time of day.
///
/// A random roll in `0.0..1.0` below the threshold triggers an encounter.
#[derive(Debug, Deserialize, Clone)]
pub struct EncounterConfig {
    /// Encounter probability at dawn.
    #[serde(default = "default_encounter_dawn")]
    pub dawn: f64,
    /// Encounter probability in the morning.
    #[serde(default = "default_encounter_morning")]
    pub morning: f64,
    /// Encounter probability at midday.
    #[serde(default = "default_encounter_midday")]
    pub midday: f64,
    /// Encounter probability in the afternoon.
    #[serde(default = "default_encounter_afternoon")]
    pub afternoon: f64,
    /// Encounter probability at dusk.
    #[serde(default = "default_encounter_dusk")]
    pub dusk: f64,
    /// Encounter probability at night.
    #[serde(default = "default_encounter_night")]
    pub night: f64,
    /// Encounter probability at midnight.
    #[serde(default = "default_encounter_midnight")]
    pub midnight: f64,
}

impl Default for EncounterConfig {
    fn default() -> Self {
        Self {
            dawn: 0.25,
            morning: 0.25,
            midday: 0.20,
            afternoon: 0.20,
            dusk: 0.15,
            night: 0.10,
            midnight: 0.05,
        }
    }
}

fn default_encounter_dawn() -> f64 {
    0.25
}
fn default_encounter_morning() -> f64 {
    0.25
}
fn default_encounter_midday() -> f64 {
    0.20
}
fn default_encounter_afternoon() -> f64 {
    0.20
}
fn default_encounter_dusk() -> f64 {
    0.15
}
fn default_encounter_night() -> f64 {
    0.10
}
fn default_encounter_midnight() -> f64 {
    0.05
}

// ---------------------------------------------------------------------------
// NPC
// ---------------------------------------------------------------------------

/// NPC memory, cognition, and relationship tuning.
#[derive(Debug, Deserialize, Clone)]
pub struct NpcConfig {
    /// Maximum number of entries in NPC short-term memory.
    #[serde(default = "default_memory_capacity")]
    pub memory_capacity: usize,
    /// Buffer size for detecting `---` separator in streamed NPC responses.
    #[serde(default = "default_separator_holdback")]
    pub separator_holdback: usize,
    /// Number of recent memories included in dialogue context.
    #[serde(default = "default_memory_context_count")]
    pub memory_context_count: usize,
    /// Max characters for dialogue memory entries.
    #[serde(default = "default_memory_truncation_dialogue")]
    pub memory_truncation_dialogue: usize,
    /// Max characters for event log memory entries.
    #[serde(default = "default_memory_truncation_event_log")]
    pub memory_truncation_event_log: usize,
    /// Max characters for event summary in simulation.
    #[serde(default = "default_event_summary_truncation")]
    pub event_summary_truncation: usize,
    /// Max characters for event summary in debug display.
    #[serde(default = "default_event_summary_debug_truncation")]
    pub event_summary_debug_truncation: usize,
    /// Cognitive tier distance thresholds.
    #[serde(default)]
    pub cognitive_tiers: CognitiveTierConfig,
    /// Relationship strength label thresholds.
    #[serde(default)]
    pub relationship_labels: RelationshipLabelConfig,
}

impl Default for NpcConfig {
    fn default() -> Self {
        Self {
            memory_capacity: 20,
            separator_holdback: 24,
            memory_context_count: 5,
            memory_truncation_dialogue: 80,
            memory_truncation_event_log: 60,
            event_summary_truncation: 100,
            event_summary_debug_truncation: 50,
            cognitive_tiers: CognitiveTierConfig::default(),
            relationship_labels: RelationshipLabelConfig::default(),
        }
    }
}

fn default_memory_capacity() -> usize {
    20
}
fn default_separator_holdback() -> usize {
    24
}
fn default_memory_context_count() -> usize {
    5
}
fn default_memory_truncation_dialogue() -> usize {
    80
}
fn default_memory_truncation_event_log() -> usize {
    60
}
fn default_event_summary_truncation() -> usize {
    100
}
fn default_event_summary_debug_truncation() -> usize {
    50
}

/// Cognitive tier assignment based on distance from player.
#[derive(Debug, Deserialize, Clone)]
pub struct CognitiveTierConfig {
    /// Maximum distance for Tier 1 (same location).
    #[serde(default = "default_tier1_max_distance")]
    pub tier1_max_distance: u32,
    /// Maximum distance for Tier 2 (nearby).
    #[serde(default = "default_tier2_max_distance")]
    pub tier2_max_distance: u32,
    /// Tier 2 simulation tick interval in game-minutes.
    #[serde(default = "default_tier2_tick_interval_minutes")]
    pub tier2_tick_interval_minutes: i64,
}

impl Default for CognitiveTierConfig {
    fn default() -> Self {
        Self {
            tier1_max_distance: 0,
            tier2_max_distance: 2,
            tier2_tick_interval_minutes: 5,
        }
    }
}

fn default_tier1_max_distance() -> u32 {
    0
}
fn default_tier2_max_distance() -> u32 {
    2
}
fn default_tier2_tick_interval_minutes() -> i64 {
    5
}

/// Relationship strength thresholds for descriptive labels.
#[derive(Debug, Deserialize, Clone)]
pub struct RelationshipLabelConfig {
    /// Threshold for "very close".
    #[serde(default = "default_very_close")]
    pub very_close: f64,
    /// Threshold for "friendly".
    #[serde(default = "default_friendly")]
    pub friendly: f64,
    /// Threshold for "acquainted".
    #[serde(default = "default_acquainted")]
    pub acquainted: f64,
    /// Threshold for "cool".
    #[serde(default = "default_cool")]
    pub cool: f64,
    /// Threshold for "strained".
    #[serde(default = "default_strained")]
    pub strained: f64,
}

impl Default for RelationshipLabelConfig {
    fn default() -> Self {
        Self {
            very_close: 0.7,
            friendly: 0.3,
            acquainted: 0.0,
            cool: -0.3,
            strained: -0.7,
        }
    }
}

fn default_very_close() -> f64 {
    0.7
}
fn default_friendly() -> f64 {
    0.3
}
fn default_acquainted() -> f64 {
    0.0
}
fn default_cool() -> f64 {
    -0.3
}
fn default_strained() -> f64 {
    -0.7
}

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

/// Color palette tinting and contrast configuration.
#[derive(Debug, Deserialize, Clone)]
pub struct PaletteConfig {
    /// Minimum luminance contrast between foreground and background.
    #[serde(default = "default_min_fg_bg_contrast")]
    pub min_fg_bg_contrast: f32,
    /// Minimum luminance contrast between muted text and background.
    #[serde(default = "default_min_muted_bg_contrast")]
    pub min_muted_bg_contrast: f32,
    /// Season color tint multipliers.
    #[serde(default)]
    pub season_tints: SeasonTintConfig,
    /// Weather color tint multipliers.
    #[serde(default)]
    pub weather_tints: WeatherTintConfig,
}

impl Default for PaletteConfig {
    fn default() -> Self {
        Self {
            min_fg_bg_contrast: 80.0,
            min_muted_bg_contrast: 45.0,
            season_tints: SeasonTintConfig::default(),
            weather_tints: WeatherTintConfig::default(),
        }
    }
}

fn default_min_fg_bg_contrast() -> f32 {
    80.0
}
fn default_min_muted_bg_contrast() -> f32 {
    45.0
}

/// Season tint multipliers: `[r_mult, g_mult, b_mult, desaturation]`.
#[derive(Debug, Deserialize, Clone)]
pub struct SeasonTintConfig {
    /// Spring tint.
    #[serde(default = "default_spring_tint")]
    pub spring: [f32; 4],
    /// Summer tint.
    #[serde(default = "default_summer_tint")]
    pub summer: [f32; 4],
    /// Autumn tint.
    #[serde(default = "default_autumn_tint")]
    pub autumn: [f32; 4],
    /// Winter tint.
    #[serde(default = "default_winter_tint")]
    pub winter: [f32; 4],
}

impl Default for SeasonTintConfig {
    fn default() -> Self {
        Self {
            spring: [0.98, 1.02, 0.98, 0.0],
            summer: [1.03, 1.01, 0.97, 0.0],
            autumn: [1.06, 1.00, 0.92, 0.0],
            winter: [0.94, 0.96, 1.04, 0.08],
        }
    }
}

fn default_spring_tint() -> [f32; 4] {
    [0.98, 1.02, 0.98, 0.0]
}
fn default_summer_tint() -> [f32; 4] {
    [1.03, 1.01, 0.97, 0.0]
}
fn default_autumn_tint() -> [f32; 4] {
    [1.06, 1.00, 0.92, 0.0]
}
fn default_winter_tint() -> [f32; 4] {
    [0.94, 0.96, 1.04, 0.08]
}

/// Weather tint multipliers: `[r_mult, g_mult, b_mult, desaturation, brightness, contrast_reduction]`.
#[derive(Debug, Deserialize, Clone)]
pub struct WeatherTintConfig {
    /// Clear weather (identity).
    #[serde(default = "default_clear_tint")]
    pub clear: [f32; 6],
    /// Overcast weather.
    #[serde(default = "default_overcast_tint")]
    pub overcast: [f32; 6],
    /// Rain weather.
    #[serde(default = "default_rain_tint")]
    pub rain: [f32; 6],
    /// Fog weather.
    #[serde(default = "default_fog_tint")]
    pub fog: [f32; 6],
    /// Storm weather.
    #[serde(default = "default_storm_tint")]
    pub storm: [f32; 6],
}

impl Default for WeatherTintConfig {
    fn default() -> Self {
        Self {
            clear: [1.0, 1.0, 1.0, 0.0, 1.0, 0.0],
            overcast: [0.95, 0.95, 0.97, 0.15, 0.92, 0.0],
            rain: [0.88, 0.90, 0.95, 0.20, 0.85, 0.0],
            fog: [0.97, 0.97, 0.98, 0.35, 0.95, 0.15],
            storm: [0.80, 0.82, 0.85, 0.30, 0.75, 0.0],
        }
    }
}

fn default_clear_tint() -> [f32; 6] {
    [1.0, 1.0, 1.0, 0.0, 1.0, 0.0]
}
fn default_overcast_tint() -> [f32; 6] {
    [0.95, 0.95, 0.97, 0.15, 0.92, 0.0]
}
fn default_rain_tint() -> [f32; 6] {
    [0.88, 0.90, 0.95, 0.20, 0.85, 0.0]
}
fn default_fog_tint() -> [f32; 6] {
    [0.97, 0.97, 0.98, 0.35, 0.95, 0.15]
}
fn default_storm_tint() -> [f32; 6] {
    [0.80, 0.82, 0.85, 0.30, 0.75, 0.0]
}

// ---------------------------------------------------------------------------
// World
// ---------------------------------------------------------------------------

/// World graph tuning parameters.
#[derive(Debug, Deserialize, Clone)]
pub struct WorldConfig {
    /// Minimum Jaro-Winkler similarity (0.0–1.0) for fuzzy location name matching.
    ///
    /// Higher values reduce false positives but miss more typos.
    #[serde(default = "default_fuzzy_threshold")]
    pub fuzzy_threshold: f64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            fuzzy_threshold: 0.82,
        }
    }
}

fn default_fuzzy_threshold() -> f64 {
    0.82
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

/// Persistence / save system tuning parameters.
#[derive(Debug, Deserialize, Clone)]
pub struct PersistenceConfig {
    /// Maximum journal entries per branch before automatic compaction.
    ///
    /// Reserved for future use — compaction is not yet implemented.
    #[serde(default = "default_journal_compaction_threshold")]
    pub journal_compaction_threshold: usize,
}

impl Default for PersistenceConfig {
    fn default() -> Self {
        Self {
            journal_compaction_threshold: 1000,
        }
    }
}

fn default_journal_compaction_threshold() -> usize {
    1000
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engine_config_default() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.inference.timeout_secs, 30);
        assert_eq!(cfg.inference.streaming_timeout_secs, 300);
        assert_eq!(cfg.inference.log_capacity, 50);
        assert!((cfg.speeds.normal - 36.0).abs() < f64::EPSILON);
        assert!((cfg.encounters.dawn - 0.25).abs() < f64::EPSILON);
        assert_eq!(cfg.npc.memory_capacity, 20);
        assert!((cfg.palette.min_fg_bg_contrast - 80.0).abs() < f32::EPSILON);
        assert!((cfg.world.fuzzy_threshold - 0.82).abs() < f64::EPSILON);
        assert_eq!(cfg.persistence.journal_compaction_threshold, 1000);
    }

    #[test]
    fn test_engine_config_deserialize_empty() {
        let cfg: EngineConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.inference.timeout_secs, 30);
        assert_eq!(cfg.npc.memory_capacity, 20);
    }

    #[test]
    fn test_engine_config_deserialize_partial() {
        let toml_str = r#"
[inference]
timeout_secs = 60

[npc]
memory_capacity = 30
"#;
        let cfg: EngineConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.inference.timeout_secs, 60);
        assert_eq!(cfg.inference.streaming_timeout_secs, 300); // default
        assert_eq!(cfg.npc.memory_capacity, 30);
        assert_eq!(cfg.npc.separator_holdback, 24); // default
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
    fn test_encounter_config_defaults() {
        let cfg = EncounterConfig::default();
        assert!((cfg.dawn - 0.25).abs() < f64::EPSILON);
        assert!((cfg.midnight - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_npc_config_defaults() {
        let cfg = NpcConfig::default();
        assert_eq!(cfg.memory_capacity, 20);
        assert_eq!(cfg.separator_holdback, 24);
        assert_eq!(cfg.memory_context_count, 5);
        assert_eq!(cfg.cognitive_tiers.tier1_max_distance, 0);
        assert_eq!(cfg.cognitive_tiers.tier2_max_distance, 2);
        assert!((cfg.relationship_labels.very_close - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn test_palette_config_defaults() {
        let cfg = PaletteConfig::default();
        assert!((cfg.min_fg_bg_contrast - 80.0).abs() < f32::EPSILON);
        assert_eq!(cfg.season_tints.spring, [0.98, 1.02, 0.98, 0.0]);
        assert_eq!(cfg.weather_tints.clear, [1.0, 1.0, 1.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn test_world_config_defaults() {
        let cfg = WorldConfig::default();
        assert!((cfg.fuzzy_threshold - 0.82).abs() < f64::EPSILON);
    }

    #[test]
    fn test_persistence_config_defaults() {
        let cfg = PersistenceConfig::default();
        assert_eq!(cfg.journal_compaction_threshold, 1000);
    }

    #[test]
    fn test_inference_log_capacity_default() {
        let cfg = InferenceConfig::default();
        assert_eq!(cfg.log_capacity, 50);
    }
}
