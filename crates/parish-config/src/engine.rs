//! Engine configuration structs for tunable parameters.
//!
//! Every struct derives `Deserialize` and has a `Default` implementation
//! that returns the original hardcoded values, ensuring backward compatibility
//! when no config file is present.
//!
//! These are ENGINE-LEVEL parameters (timeouts, game mechanics, palette tuning).
//! Game-specific CONTENT (prompts, loading phrases, encounter text) lives in
//! the mod system (`GameMod` / `mod.toml`).

use crate::provider::InferenceCategory;
use parish_types::SpeedConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Loads the `[engine]` section from a `parish.toml` at the given path.
///
/// Returns [`EngineConfig::default`] if the file is missing, unreadable, or
/// doesn't contain a parseable `[engine]` table. After loading, calls
/// [`MapConfig::apply_defaults`] so partial `[engine.map.tile_sources.*]`
/// overrides don't wipe the baked-in registry.
///
/// Intended for Tauri/web-server boot; the CLI already has its own
/// `resolve_config` pipeline for provider/cloud config.
pub fn load_engine_config(path: Option<&Path>) -> EngineConfig {
    #[derive(Deserialize, Default)]
    struct Wrapper {
        #[serde(default)]
        engine: EngineConfig,
    }

    let default_path = Path::new("parish.toml");
    let resolved = path.unwrap_or(default_path);
    let text = match std::fs::read_to_string(resolved) {
        Ok(s) => s,
        Err(_) => return EngineConfig::default(),
    };
    let mut engine = toml::from_str::<Wrapper>(&text)
        .map(|w| w.engine)
        .unwrap_or_default();
    engine.map.apply_defaults();
    engine
}

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
    /// Map tile source registry and active default.
    #[serde(default)]
    pub map: MapConfig,
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
    /// Per-category outbound request rate limits.
    ///
    /// Defaults to no limit. Useful when targeting paid providers
    /// (OpenRouter, Anthropic, etc.) to avoid burning through quota
    /// or hitting `429 Too Many Requests`.
    #[serde(default)]
    pub rate_limits: RateLimitConfig,
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
            rate_limits: RateLimitConfig::default(),
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
// Rate limiting
// ---------------------------------------------------------------------------

/// Per-category rate limit configuration for outbound LLM requests.
///
/// All fields are optional. A `None` value disables rate limiting for
/// that category. Categories without an explicit override fall back to
/// the [`RateLimitConfig::default`] field, which applies to the base
/// provider client. Configuration example (`parish.toml`):
///
/// ```toml
/// [engine.inference.rate_limits.default]
/// per_minute = 60
/// burst = 10
///
/// [engine.inference.rate_limits.dialogue]
/// per_minute = 20
/// burst = 4
/// ```
#[derive(Debug, Default, Deserialize, Clone, Copy)]
pub struct RateLimitConfig {
    /// Default rate limit applied to the base provider client.
    /// Categories without an explicit override share this limiter.
    #[serde(default)]
    pub default: Option<CategoryRateLimit>,
    /// Override for the player-facing NPC dialogue category.
    #[serde(default)]
    pub dialogue: Option<CategoryRateLimit>,
    /// Override for the background NPC simulation category.
    #[serde(default)]
    pub simulation: Option<CategoryRateLimit>,
    /// Override for the player intent parsing category.
    #[serde(default)]
    pub intent: Option<CategoryRateLimit>,
    /// Override for the NPC arrival reaction category.
    #[serde(default)]
    pub reaction: Option<CategoryRateLimit>,
}

impl RateLimitConfig {
    /// Returns the configured rate limit for a category override, if any.
    ///
    /// This does NOT fall back to [`Self::default`] — the base limit is
    /// only applied to the base client itself, not to per-category
    /// override clients. (Override clients target a different provider
    /// endpoint and should have their own quota.)
    pub fn for_category(&self, cat: InferenceCategory) -> Option<CategoryRateLimit> {
        match cat {
            InferenceCategory::Dialogue => self.dialogue,
            InferenceCategory::Simulation => self.simulation,
            InferenceCategory::Intent => self.intent,
            InferenceCategory::Reaction => self.reaction,
        }
    }
}

/// A single rate-limit quota: sustained rate plus burst capacity.
///
/// Implements a token-bucket / GCRA model: up to `burst` requests may
/// be issued back-to-back, after which new requests are admitted at
/// `per_minute / 60` per second until the bucket refills.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct CategoryRateLimit {
    /// Sustained rate: maximum number of requests admitted per minute.
    /// Must be greater than zero — a value of zero disables the limiter.
    pub per_minute: u32,
    /// Maximum burst size (token-bucket capacity). Defaults to 1.
    #[serde(default = "default_burst")]
    pub burst: u32,
}

fn default_burst() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// Game Speed — SpeedConfig is defined in parish-types::time
// ---------------------------------------------------------------------------

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
    /// Number of recent player reactions included in dialogue context.
    #[serde(default = "default_reaction_context_count")]
    pub reaction_context_count: usize,
    /// NPC arrival reaction tuning.
    #[serde(default)]
    pub reactions: ReactionConfig,
    /// Whether to use two-pass dialogue generation (pre-pass validates
    /// which people the NPC intends to reference before generating dialogue).
    #[serde(default)]
    pub two_pass_dialogue: bool,
}

impl Default for NpcConfig {
    fn default() -> Self {
        Self {
            memory_capacity: 20,
            separator_holdback: 24,
            memory_context_count: 5,
            memory_truncation_dialogue: 250,
            memory_truncation_event_log: 150,
            event_summary_truncation: 100,
            event_summary_debug_truncation: 50,
            cognitive_tiers: CognitiveTierConfig::default(),
            relationship_labels: RelationshipLabelConfig::default(),
            reaction_context_count: 5,
            reactions: ReactionConfig::default(),
            two_pass_dialogue: false,
        }
    }
}

fn default_reaction_context_count() -> usize {
    5
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
    250
}
fn default_memory_truncation_event_log() -> usize {
    150
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
    /// Maximum distance for Tier 3 (distant but still LLM-simulated).
    #[serde(default = "default_tier3_max_distance")]
    pub tier3_max_distance: u32,
    /// Tier 2 simulation tick interval in game-minutes.
    #[serde(default = "default_tier2_tick_interval_minutes")]
    pub tier2_tick_interval_minutes: i64,
    /// Tier 3 simulation tick interval in game-hours (1 game-day = 24).
    #[serde(default = "default_tier3_tick_interval_hours")]
    pub tier3_tick_interval_hours: i64,
    /// Maximum NPCs per Tier 3 batch LLM call.
    #[serde(default = "default_tier3_batch_size")]
    pub tier3_batch_size: usize,
    /// Tier 4 rules-engine tick interval in game-days (1 season ≈ 90 days).
    #[serde(default = "default_tier4_tick_interval_days")]
    pub tier4_tick_interval_days: i64,
}

impl Default for CognitiveTierConfig {
    fn default() -> Self {
        Self {
            tier1_max_distance: 0,
            tier2_max_distance: 2,
            tier3_max_distance: 5,
            tier2_tick_interval_minutes: 5,
            tier3_tick_interval_hours: 24,
            tier3_batch_size: 10,
            tier4_tick_interval_days: 90,
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
fn default_tier3_max_distance() -> u32 {
    5
}
fn default_tier3_tick_interval_hours() -> i64 {
    24
}
fn default_tier3_batch_size() -> usize {
    10
}
fn default_tier4_tick_interval_days() -> i64 {
    90
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
// Reactions
// ---------------------------------------------------------------------------

/// Tuning for NPC arrival reactions (greetings, nods, introductions).
#[derive(Debug, Deserialize, Clone)]
pub struct ReactionConfig {
    /// Base probability that an NPC reacts when the player arrives.
    #[serde(default = "default_reaction_base_chance")]
    pub base_chance: f64,
    /// Bonus when NPC is at their workplace.
    #[serde(default = "default_reaction_workplace_bonus")]
    pub workplace_bonus: f64,
    /// Bonus when location is indoors.
    #[serde(default = "default_reaction_indoor_bonus")]
    pub indoor_bonus: f64,
    /// Bonus when NPC has high emotional intelligence (≥4).
    #[serde(default = "default_reaction_empathy_bonus")]
    pub empathy_bonus: f64,
    /// Penalty when NPC has a negative mood.
    #[serde(default = "default_reaction_negative_mood_penalty")]
    pub negative_mood_penalty: f64,
    /// Penalty at night or midnight.
    #[serde(default = "default_reaction_night_penalty")]
    pub night_penalty: f64,
    /// LLM timeout for reaction greeting calls (seconds).
    #[serde(default = "default_reaction_llm_timeout_secs")]
    pub llm_timeout_secs: u64,
}

impl Default for ReactionConfig {
    fn default() -> Self {
        Self {
            base_chance: 0.55,
            workplace_bonus: 0.35,
            indoor_bonus: 0.10,
            empathy_bonus: 0.05,
            negative_mood_penalty: 0.20,
            night_penalty: 0.15,
            llm_timeout_secs: 5,
        }
    }
}

fn default_reaction_base_chance() -> f64 {
    0.55
}
fn default_reaction_workplace_bonus() -> f64 {
    0.35
}
fn default_reaction_indoor_bonus() -> f64 {
    0.10
}
fn default_reaction_empathy_bonus() -> f64 {
    0.05
}
fn default_reaction_negative_mood_penalty() -> f64 {
    0.20
}
fn default_reaction_night_penalty() -> f64 {
    0.15
}
fn default_reaction_llm_timeout_secs() -> u64 {
    5
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
    /// Partly cloudy weather.
    #[serde(default = "default_partly_cloudy_tint")]
    pub partly_cloudy: [f32; 6],
    /// Overcast weather.
    #[serde(default = "default_overcast_tint")]
    pub overcast: [f32; 6],
    /// Light rain weather.
    #[serde(default = "default_light_rain_tint")]
    pub light_rain: [f32; 6],
    /// Heavy rain weather.
    #[serde(default = "default_heavy_rain_tint")]
    pub heavy_rain: [f32; 6],
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
            partly_cloudy: [0.97, 0.97, 0.98, 0.08, 0.96, 0.0],
            overcast: [0.95, 0.95, 0.97, 0.15, 0.92, 0.0],
            light_rain: [0.90, 0.92, 0.96, 0.15, 0.88, 0.0],
            heavy_rain: [0.85, 0.87, 0.93, 0.25, 0.80, 0.0],
            fog: [0.97, 0.97, 0.98, 0.35, 0.95, 0.15],
            storm: [0.80, 0.82, 0.85, 0.30, 0.75, 0.0],
        }
    }
}

fn default_clear_tint() -> [f32; 6] {
    [1.0, 1.0, 1.0, 0.0, 1.0, 0.0]
}
fn default_partly_cloudy_tint() -> [f32; 6] {
    [0.97, 0.97, 0.98, 0.08, 0.96, 0.0]
}
fn default_overcast_tint() -> [f32; 6] {
    [0.95, 0.95, 0.97, 0.15, 0.92, 0.0]
}
fn default_light_rain_tint() -> [f32; 6] {
    [0.90, 0.92, 0.96, 0.15, 0.88, 0.0]
}
fn default_heavy_rain_tint() -> [f32; 6] {
    [0.85, 0.87, 0.93, 0.25, 0.80, 0.0]
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
// Map
// ---------------------------------------------------------------------------

/// Map tile source registry and active default.
///
/// A registry of named raster-tile sources (XYZ templates) that the frontend
/// can switch between at runtime via the `/tiles` slash command. Users can
/// override the baked-in defaults by adding `[engine.map.tile_sources.<id>]`
/// blocks in `parish.toml`.
///
/// **Partial overrides:** serde's BTreeMap deserialisation replaces the whole
/// map rather than merging. Call [`MapConfig::apply_defaults`] after parsing
/// to fold the baked defaults (OSM, Ireland Historic 6") back into a user-supplied
/// registry that only overrode a subset.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MapConfig {
    /// Id of the source used on first boot (pre-localStorage). Must match one
    /// of the keys in `tile_sources`.
    #[serde(default = "default_tile_source_id")]
    pub default_tile_source: String,
    /// Registry of available raster tile sources, keyed by id.
    #[serde(default = "default_tile_sources")]
    pub tile_sources: BTreeMap<String, TileSourceConfig>,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            default_tile_source: default_tile_source_id(),
            tile_sources: default_tile_sources(),
        }
    }
}

impl MapConfig {
    /// Fold baked-in defaults into the registry for any id the user didn't
    /// override. Call this after deserialising `parish.toml` so a partial
    /// `[engine.map.tile_sources.osm]` block doesn't wipe the historic entry.
    pub fn apply_defaults(&mut self) {
        for (id, source) in default_tile_sources() {
            self.tile_sources.entry(id).or_insert(source);
        }
    }
}

fn default_tile_source_id() -> String {
    "osm".to_string()
}

fn default_tile_sources() -> BTreeMap<String, TileSourceConfig> {
    let mut m = BTreeMap::new();
    m.insert(
        "osm".to_string(),
        TileSourceConfig {
            label: "OpenStreetMap".to_string(),
            url: "https://tile.openstreetmap.org/{z}/{x}/{y}.png".to_string(),
            tile_size: 256,
            minzoom: 0,
            maxzoom: 19,
            attribution: "© OpenStreetMap contributors".to_string(),
            raster_saturation: -0.4,
            raster_opacity: 0.85,
            tms: false,
        },
    );
    m.insert(
        "historic-6inch".to_string(),
        TileSourceConfig {
            label: "Ireland Historic 6\" (via NLS)".to_string(),
            // Ordnance Survey of Ireland First Edition 6-inch (1829–1842),
            // reprojected and hosted by the National Library of Scotland.
            // Free, public, CORS-open (S3). See:
            //   https://maps.nls.uk/geo/explore/
            // For alternative (gated) access via Tailte Éireann MapGenie,
            // see https://tailte.ie/services/mapgenie/.
            url: "https://mapseries-tilesets.s3.amazonaws.com/ireland_6inch/{z}/{x}/{y}.jpg"
                .to_string(),
            tile_size: 256,
            minzoom: 0,
            maxzoom: 15,
            attribution: "Historic 6\" OS Ireland (1829–1842), via National Library of Scotland"
                .to_string(),
            raster_saturation: 0.0,
            raster_opacity: 1.0,
            tms: false,
        },
    );
    m
}

/// A single raster tile source — URL template plus display metadata.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TileSourceConfig {
    /// Human-readable label displayed in `/tiles` listings.
    #[serde(default)]
    pub label: String,
    /// XYZ URL template (e.g. `https://…/{z}/{x}/{y}.png`). Empty string
    /// means the source is registered but not yet configured; the frontend
    /// falls back to a flat background.
    #[serde(default)]
    pub url: String,
    /// Tile edge length in pixels. 256 for classic OSM-style sources.
    #[serde(default = "default_tile_size")]
    pub tile_size: u32,
    /// Minimum zoom level the source serves tiles for.
    #[serde(default)]
    pub minzoom: u32,
    /// Maximum zoom level the source serves tiles for.
    #[serde(default = "default_tile_maxzoom")]
    pub maxzoom: u32,
    /// Attribution text shown in the MapLibre attribution control.
    #[serde(default)]
    pub attribution: String,
    /// MapLibre `raster-saturation` paint (-1.0 to 1.0). Negative values
    /// desaturate; 0.0 leaves colours untouched.
    #[serde(default = "default_raster_saturation")]
    pub raster_saturation: f32,
    /// MapLibre `raster-opacity` paint (0.0 to 1.0).
    #[serde(default = "default_raster_opacity")]
    pub raster_opacity: f32,
    /// When true, the frontend sets `scheme: 'tms'` on the MapLibre source,
    /// flipping the y-axis for ArcGIS-style tile services.
    #[serde(default)]
    pub tms: bool,
}

fn default_tile_size() -> u32 {
    256
}
fn default_tile_maxzoom() -> u32 {
    19
}
fn default_raster_saturation() -> f32 {
    -0.4
}
fn default_raster_opacity() -> f32 {
    0.85
}

impl MapConfig {
    /// Returns tile-source entries as `(id, label)` pairs, alphabetical by id.
    /// Used by backends to populate [`parish_core::ipc::GameConfig::tile_sources`]
    /// so the `/tiles` command handler can list and validate without needing
    /// a reference to the whole engine config.
    pub fn id_label_pairs(&self) -> Vec<(String, String)> {
        self.tile_sources
            .iter()
            .map(|(id, src)| (id.clone(), src.label.clone()))
            .collect()
    }
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

    #[test]
    fn test_rate_limit_config_default_is_unset() {
        let cfg = RateLimitConfig::default();
        assert!(cfg.default.is_none());
        assert!(cfg.dialogue.is_none());
        assert!(cfg.simulation.is_none());
        assert!(cfg.intent.is_none());
        assert!(cfg.reaction.is_none());
    }

    #[test]
    fn test_inference_config_default_has_no_rate_limits() {
        let cfg = InferenceConfig::default();
        assert!(cfg.rate_limits.default.is_none());
        assert!(cfg.rate_limits.dialogue.is_none());
    }

    #[test]
    fn test_rate_limit_config_for_category_returns_override() {
        let cfg = RateLimitConfig {
            dialogue: Some(CategoryRateLimit {
                per_minute: 20,
                burst: 4,
            }),
            simulation: Some(CategoryRateLimit {
                per_minute: 60,
                burst: 10,
            }),
            ..RateLimitConfig::default()
        };
        let dial = cfg.for_category(InferenceCategory::Dialogue).unwrap();
        assert_eq!(dial.per_minute, 20);
        assert_eq!(dial.burst, 4);
        let sim = cfg.for_category(InferenceCategory::Simulation).unwrap();
        assert_eq!(sim.per_minute, 60);
        assert!(cfg.for_category(InferenceCategory::Intent).is_none());
        assert!(cfg.for_category(InferenceCategory::Reaction).is_none());
    }

    #[test]
    fn test_rate_limit_config_for_category_does_not_inherit_default() {
        // The `default` field is for the base client, not per-category fallback.
        let cfg = RateLimitConfig {
            default: Some(CategoryRateLimit {
                per_minute: 100,
                burst: 5,
            }),
            ..RateLimitConfig::default()
        };
        assert!(cfg.for_category(InferenceCategory::Dialogue).is_none());
    }

    #[test]
    fn test_category_rate_limit_burst_defaults_to_one() {
        let toml = "per_minute = 30";
        let cfg: CategoryRateLimit = toml::from_str(toml).unwrap();
        assert_eq!(cfg.per_minute, 30);
        assert_eq!(cfg.burst, 1);
    }

    #[test]
    fn test_map_config_default_has_both_sources() {
        let cfg = MapConfig::default();
        assert_eq!(cfg.default_tile_source, "osm");
        assert!(cfg.tile_sources.contains_key("osm"));
        assert!(cfg.tile_sources.contains_key("historic-6inch"));
        let osm = &cfg.tile_sources["osm"];
        assert_eq!(osm.url, "https://tile.openstreetmap.org/{z}/{x}/{y}.png");
        assert_eq!(osm.tile_size, 256);
        assert_eq!(osm.maxzoom, 19);
        assert!(!osm.tms);
        let historic = &cfg.tile_sources["historic-6inch"];
        assert!(!historic.tms, "NLS serves standard XYZ, not TMS");
        assert!(
            historic.url.starts_with("https://"),
            "Historic 6\" ships with a live NLS URL"
        );
        assert!(historic.url.contains("ireland_6inch"));
    }

    #[test]
    fn test_engine_config_includes_map_defaults() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.map.default_tile_source, "osm");
        assert_eq!(cfg.map.tile_sources.len(), 2);
    }

    #[test]
    fn test_map_config_apply_defaults_merges_missing_sources() {
        let mut cfg = MapConfig {
            default_tile_source: "osm".to_string(),
            tile_sources: BTreeMap::new(),
        };
        cfg.tile_sources.insert(
            "custom".to_string(),
            TileSourceConfig {
                label: "Custom".to_string(),
                url: "https://example.com/{z}/{x}/{y}.png".to_string(),
                tile_size: 256,
                minzoom: 0,
                maxzoom: 18,
                attribution: "custom".to_string(),
                raster_saturation: 0.0,
                raster_opacity: 1.0,
                tms: false,
            },
        );
        cfg.apply_defaults();
        assert!(cfg.tile_sources.contains_key("custom"));
        assert!(cfg.tile_sources.contains_key("osm"));
        assert!(cfg.tile_sources.contains_key("historic-6inch"));
    }

    #[test]
    fn test_map_config_apply_defaults_preserves_user_overrides() {
        let mut cfg = MapConfig::default();
        cfg.tile_sources.get_mut("osm").unwrap().url =
            "https://example.com/custom-osm/{z}/{x}/{y}.png".to_string();
        cfg.apply_defaults();
        assert_eq!(
            cfg.tile_sources["osm"].url,
            "https://example.com/custom-osm/{z}/{x}/{y}.png"
        );
    }

    #[test]
    fn test_load_engine_config_missing_file() {
        let cfg = load_engine_config(Some(Path::new("/nonexistent/parish.toml")));
        assert_eq!(cfg.map.default_tile_source, "osm");
        assert_eq!(cfg.map.tile_sources.len(), 2);
    }

    #[test]
    fn test_load_engine_config_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("parish.toml");
        std::fs::write(
            &path,
            r#"
[engine.map]
default_tile_source = "historic-6inch"

[engine.map.tile_sources.osm]
url = "https://override/{z}/{x}/{y}.png"
"#,
        )
        .unwrap();
        let cfg = load_engine_config(Some(&path));
        assert_eq!(cfg.map.default_tile_source, "historic-6inch");
        assert_eq!(
            cfg.map.tile_sources.len(),
            2,
            "apply_defaults folded historic-6inch back in"
        );
        assert_eq!(
            cfg.map.tile_sources["osm"].url,
            "https://override/{z}/{x}/{y}.png"
        );
    }

    #[test]
    fn test_map_config_id_label_pairs_is_sorted() {
        let cfg = MapConfig::default();
        let pairs = cfg.id_label_pairs();
        assert_eq!(pairs.len(), 2);
        // BTreeMap iterates in sorted order, so "historic-6inch" < "osm".
        assert_eq!(pairs[0].0, "historic-6inch");
        assert_eq!(pairs[1].0, "osm");
    }

    #[test]
    fn test_map_config_deserialize_partial_toml() {
        // A partial override: user only overrides OSM URL, historic-6inch entry
        // would be wiped without apply_defaults.
        let toml_str = r#"
[tile_sources.osm]
url = "https://custom/{z}/{x}/{y}.png"
attribution = "custom attribution"
"#;
        let mut cfg: MapConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tile_sources.len(), 1, "serde replace semantics");
        cfg.apply_defaults();
        assert_eq!(
            cfg.tile_sources.len(),
            2,
            "apply_defaults folds in historic-6inch"
        );
        assert_eq!(
            cfg.tile_sources["osm"].url,
            "https://custom/{z}/{x}/{y}.png"
        );
        assert_eq!(cfg.tile_sources["osm"].attribution, "custom attribution");
    }

    #[test]
    fn test_inference_config_parses_rate_limits_from_toml() {
        let toml_text = r#"
            [rate_limits.default]
            per_minute = 60
            burst = 10

            [rate_limits.dialogue]
            per_minute = 20
            burst = 4

            [rate_limits.simulation]
            per_minute = 30
        "#;
        let cfg: InferenceConfig = toml::from_str(toml_text).unwrap();
        let default = cfg.rate_limits.default.unwrap();
        assert_eq!(default.per_minute, 60);
        assert_eq!(default.burst, 10);
        let dial = cfg.rate_limits.dialogue.unwrap();
        assert_eq!(dial.per_minute, 20);
        assert_eq!(dial.burst, 4);
        let sim = cfg.rate_limits.simulation.unwrap();
        assert_eq!(sim.per_minute, 30);
        assert_eq!(sim.burst, 1);
        // Unspecified categories remain None
        assert!(cfg.rate_limits.intent.is_none());
        assert!(cfg.rate_limits.reaction.is_none());
    }
}
