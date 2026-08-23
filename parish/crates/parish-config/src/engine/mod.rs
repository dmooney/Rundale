//! Engine configuration structs for tunable parameters.
//!
//! Every struct derives `Deserialize` and has a `Default` implementation
//! that returns the original hardcoded values, ensuring backward compatibility
//! when no config file is present.
//!
//! These are ENGINE-LEVEL parameters (timeouts, game mechanics, palette tuning).
//! Game-specific CONTENT (prompts, loading phrases, encounter text) lives in
//! the mod system (`GameMod` / `mod.toml`).
//!
//! Structure (#1200 decomposition): the former single module is split by
//! config domain into submodules — [`session`], [`inference`] (timeouts +
//! rate limits), [`encounters`], [`npc`] (memory/cognition/relationships),
//! [`reactions`], [`palette`], [`world`] (+ persistence), and [`map`]. This
//! `mod.rs` owns the aggregate [`EngineConfig`] plus the `[engine]` loaders,
//! and re-exports every domain type flat so the public paths
//! (`engine::SessionConfig`, `engine::MapConfig`, …, and the `pub use
//! engine::*` re-export at the crate root) are unchanged.

mod encounters;
mod inference;
mod map;
mod npc;
mod palette;
mod reactions;
mod session;
mod world;

pub use encounters::EncounterConfig;
pub use inference::{
    CategoryRateLimit, DialogueGenerationConfig, InferenceConfig, RateLimitConfig, ReasoningEffort,
};
pub use map::{MapConfig, TileSourceConfig};
pub use npc::{CognitiveTierConfig, NpcConfig, RelationshipLabelConfig};
pub use palette::PaletteConfig;
pub use reactions::ReactionConfig;
pub use session::SessionConfig;
pub use world::{PersistenceConfig, WorldConfig};

use parish_types::SpeedConfig;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// Resolves the path to `parish.toml` by walking up from `start` looking for
/// an existing `parish.toml` file (up to 5 ancestor directories). If none is
/// found, returns `start.join("parish.toml")` so the caller still gets a path
/// (which will produce defaults when passed to [`load_engine_config`]).
///
/// Must be called once at startup with a deliberately resolved starting
/// directory — never from a request handler or per-call helper (see Rule 9).
pub fn resolve_config_path(start: &Path) -> PathBuf {
    let mut p = Some(start.to_path_buf());
    for _ in 0..5 {
        if let Some(ref dir) = p {
            let candidate = dir.join("parish.toml");
            if candidate.is_file() {
                return candidate;
            }
            p = dir.parent().map(|d| d.to_path_buf());
        }
    }
    start.join("parish.toml")
}

/// Loads the `[engine]` section from a `parish.toml` at the given path.
///
/// Returns [`EngineConfig::default`] if the file is missing, unreadable, or
/// doesn't contain a parseable `[engine]` table. After loading, calls
/// [`MapConfig::apply_defaults`] so partial `[engine.map.tile_sources.*]`
/// overrides don't wipe the baked-in registry.
///
/// Intended for Tauri/web-server boot; the CLI already has its own
/// `resolve_config` pipeline for provider/cloud config.
pub fn load_engine_config(path: &Path) -> EngineConfig {
    #[derive(Deserialize, Default)]
    struct Wrapper {
        #[serde(default)]
        engine: EngineConfig,
    }

    let text = match std::fs::read_to_string(path) {
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
#[derive(Debug, Default, Deserialize, Clone, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
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
    /// Color palette contrast configuration.
    #[serde(default)]
    pub palette: PaletteConfig,
    /// World graph tuning.
    #[serde(default)]
    pub world: WorldConfig,
    /// Persistence / save system tuning.
    #[serde(default)]
    pub persistence: PersistenceConfig,
    /// Session and pacing timeouts.
    #[serde(default)]
    pub session: SessionConfig,
    /// Map tile source registry and active default.
    #[serde(default)]
    pub map: MapConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::InferenceCategory;
    use std::collections::BTreeMap;

    #[test]
    fn test_engine_config_default() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.inference.timeout_secs, 300);
        assert_eq!(cfg.inference.streaming_timeout_secs, 300);
        assert_eq!(cfg.inference.log_capacity, 50);
        assert!((cfg.speeds.normal - 36.0).abs() < f64::EPSILON);
        assert!((cfg.encounters.dawn - 0.25).abs() < f64::EPSILON);
        assert_eq!(cfg.npc.memory_capacity, 20);
        assert!((cfg.palette.min_fg_bg_contrast - 80.0).abs() < f32::EPSILON);
        assert!((cfg.world.fuzzy_threshold - 0.82).abs() < f64::EPSILON);
        // PersistenceConfig is intentionally empty (TD-011)
        let _ = cfg.persistence;
    }

    #[test]
    fn test_engine_config_deserialize_empty() {
        let cfg: EngineConfig = toml::from_str("").unwrap();
        assert_eq!(cfg.inference.timeout_secs, 300);
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
        assert!((cfg.min_muted_bg_contrast - 45.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_world_config_defaults() {
        let cfg = WorldConfig::default();
        assert!((cfg.fuzzy_threshold - 0.82).abs() < f64::EPSILON);
    }

    #[test]
    fn test_persistence_config_default() {
        let cfg = PersistenceConfig::default();
        // Intentionally empty struct (TD-011); just verify it constructs.
        let _ = cfg;
    }

    #[test]
    fn test_inference_log_capacity_default() {
        let cfg = InferenceConfig::default();
        assert_eq!(cfg.log_capacity, 50);
    }

    #[test]
    fn test_dialogue_generation_defaults_preserve_existing_runtime_behavior() {
        let generation = InferenceConfig::default().dialogue_generation;
        assert_eq!(generation.max_tokens, 768);
        assert!((generation.temperature - 0.7).abs() < f32::EPSILON);
        assert_eq!(generation.frequency_penalty, Some(0.5));
        assert!(generation.json_mode);
        assert_eq!(generation.enable_thinking, None);
        assert_eq!(generation.reasoning_effort, None);
    }

    #[test]
    fn test_dialogue_generation_is_configurable() {
        let cfg: EngineConfig = toml::from_str(
            r#"
[inference.dialogue_generation]
max_tokens = 512
temperature = 0.35
frequency_penalty = 0.2
json_mode = false
enable_thinking = false
reasoning_effort = "max"
"#,
        )
        .unwrap();
        let generation = cfg.inference.dialogue_generation;
        assert_eq!(generation.max_tokens, 512);
        assert!((generation.temperature - 0.35).abs() < f32::EPSILON);
        assert_eq!(generation.frequency_penalty, Some(0.2));
        assert!(!generation.json_mode);
        assert_eq!(generation.enable_thinking, Some(false));
        assert_eq!(generation.reasoning_effort, Some(ReasoningEffort::Max));
    }

    #[test]
    fn promoted_gemini_37_profiles_preserve_route_and_use_measured_headroom() {
        let promoted = DialogueGenerationConfig::default().for_model("google/gemini-3.7-flash");
        assert_eq!(promoted.max_tokens, 4096);
        assert_eq!(promoted.temperature, 0.7);
        assert_eq!(promoted.frequency_penalty, Some(0.5));
        assert!(promoted.json_mode);
        assert_eq!(promoted.enable_thinking, Some(true));
        assert_eq!(promoted.reasoning_effort, Some(ReasoningEffort::Low));

        let explicit = DialogueGenerationConfig {
            reasoning_effort: Some(ReasoningEffort::High),
            max_tokens: 1024,
            ..DialogueGenerationConfig::default()
        }
        .for_model("google/gemini-3.7-flash");
        assert_eq!(explicit.max_tokens, 1024);
        assert_eq!(explicit.reasoning_effort, Some(ReasoningEffort::High));
        assert_eq!(explicit.enable_thinking, None);

        let prior_promoted =
            DialogueGenerationConfig::default().for_model("google/gemini-3.6-flash");
        assert_eq!(prior_promoted.max_tokens, 768);
        assert_eq!(prior_promoted.reasoning_effort, Some(ReasoningEffort::Low));

        let direct_google = DialogueGenerationConfig::default().for_model("gemini-3.7-flash");
        assert_eq!(direct_google.max_tokens, 4096);
        assert_eq!(direct_google.reasoning_effort, Some(ReasoningEffort::Low));
        assert_eq!(direct_google.enable_thinking, Some(true));
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
        assert_eq!(cfg.default_tile_source, "historic");
        assert!(cfg.tile_sources.contains_key("osm"));
        assert!(cfg.tile_sources.contains_key("historic"));
        let osm = &cfg.tile_sources["osm"];
        assert_eq!(osm.url, "https://tile.openstreetmap.org/{z}/{x}/{y}.png");
        assert!(
            osm.upstream_url.is_empty(),
            "OSM is fetched directly by the browser; no server-side proxying"
        );
        assert_eq!(osm.tile_size, 256);
        assert_eq!(osm.maxzoom, 19);
        assert!(!osm.tms);
        let historic = &cfg.tile_sources["historic"];
        assert!(!historic.tms, "NLS serves standard XYZ, not TMS");
        // Since issue #360, tiles are proxied through the local server so the
        // client never hits NLS S3 directly.  `url` is the same-origin proxy
        // path the browser hits; `upstream_url` is the absolute NLS S3 URL
        // the server-side cache fetches from on a miss (PR #955).
        assert!(
            historic.url.starts_with("/tiles/historic/"),
            "Historic 6\" tiles are proxied through the local server under the registered \
             tile source id (issue #360); got: {}",
            historic.url
        );
        assert!(
            historic
                .upstream_url
                .starts_with("https://mapseries-tilesets.s3.amazonaws.com/os/roscommon1/"),
            "Historic 6\" upstream_url must point at the NLS Roscommon 1st-edition \
             S3 path so the server-side cache can fetch tiles on a miss; got: {}",
            historic.upstream_url
        );
        assert_eq!(historic.maxzoom, 17, "NLS serves 6-inch up to z=17");
    }

    /// Pins the invariant that broke in PR #955: when a tile source's `url` is
    /// a same-origin proxy path (`/tiles/<seg>/{z}/{x}/{y}.png`), its first
    /// path segment must match the registering `tile_sources` key, because
    /// `parish-server`'s tile-proxy route validates that segment against the
    /// registered ids and 404s on mismatch.
    #[test]
    fn proxy_path_segment_matches_registered_source_id() {
        let cfg = MapConfig::default();
        for (id, src) in &cfg.tile_sources {
            let Some(rest) = src.url.strip_prefix("/tiles/") else {
                continue;
            };
            let seg = rest.split('/').next().unwrap_or("");
            assert_eq!(
                seg, id,
                "tile source {id:?} should use its own id as the proxy path segment, \
                 not {seg:?}; otherwise the tile-proxy route handler would 404 every \
                 request and the cache lookup would happen under the wrong key"
            );
        }
    }

    #[test]
    fn test_engine_config_includes_map_defaults() {
        let cfg = EngineConfig::default();
        assert_eq!(cfg.map.default_tile_source, "historic");
        assert_eq!(cfg.map.tile_sources.len(), 2);
    }

    #[test]
    fn test_map_config_apply_defaults_merges_missing_sources() {
        let mut cfg = MapConfig {
            default_tile_source: "osm".to_string(),
            tile_sources: BTreeMap::new(),
            bundled_tiles_dir: None,
        };
        cfg.tile_sources.insert(
            "custom".to_string(),
            TileSourceConfig {
                label: "Custom".to_string(),
                url: "https://example.com/{z}/{x}/{y}.png".to_string(),
                upstream_url: String::new(),
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
        assert!(cfg.tile_sources.contains_key("historic"));
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
    fn map_config_bundled_tiles_dir_defaults_to_none() {
        assert!(MapConfig::default().bundled_tiles_dir.is_none());
    }

    #[test]
    fn map_config_bundled_tiles_dir_deserializes_from_toml() {
        let toml = r#"bundled_tiles_dir = "/opt/parish/tiles""#;
        let cfg: MapConfig = toml::from_str(toml).unwrap();
        assert_eq!(
            cfg.bundled_tiles_dir,
            Some(std::path::PathBuf::from("/opt/parish/tiles"))
        );
    }

    #[test]
    fn map_config_apply_defaults_preserves_bundled_tiles_dir() {
        let mut cfg = MapConfig {
            bundled_tiles_dir: Some(std::path::PathBuf::from("/opt/tiles")),
            ..MapConfig::default()
        };
        cfg.apply_defaults();
        assert_eq!(
            cfg.bundled_tiles_dir,
            Some(std::path::PathBuf::from("/opt/tiles"))
        );
    }

    #[test]
    fn test_load_engine_config_missing_file() {
        let cfg = load_engine_config(Path::new("/nonexistent/parish.toml"));
        assert_eq!(cfg.map.default_tile_source, "historic");
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
default_tile_source = "historic"

[engine.map.tile_sources.osm]
url = "https://override/{z}/{x}/{y}.png"
"#,
        )
        .unwrap();
        let cfg = load_engine_config(&path);
        assert_eq!(cfg.map.default_tile_source, "historic");
        assert_eq!(
            cfg.map.tile_sources.len(),
            2,
            "apply_defaults folded historic back in"
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
        // BTreeMap iterates in sorted order, so "historic" < "osm".
        assert_eq!(pairs[0].0, "historic");
        assert_eq!(pairs[1].0, "osm");
    }

    #[test]
    fn test_map_config_deserialize_partial_toml() {
        // A partial override: user only overrides OSM URL, historic entry
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
            "apply_defaults folds in historic"
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

    #[test]
    fn test_inference_config_parses_force_model_redownload_from_toml() {
        let cfg: InferenceConfig = toml::from_str("force_model_redownload = true").unwrap();

        assert!(cfg.force_model_redownload);
        assert_eq!(cfg.model_download_timeout_secs, 3600);
    }

    #[test]
    fn test_session_config_deserialize_from_toml() {
        let toml_str = r#"
idle_banter_after_secs = 60
auto_pause_after_secs = 600
max_concurrent_sessions = 10
"#;
        let cfg: SessionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.idle_banter_after_secs, 60);
        assert_eq!(cfg.auto_pause_after_secs, 600);
        assert_eq!(cfg.max_concurrent_sessions, 10);
    }

    #[test]
    fn test_session_config_deserialize_partial() {
        let toml_str = "idle_banter_after_secs = 60";
        let cfg: SessionConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.idle_banter_after_secs, 60);
        assert_eq!(cfg.auto_pause_after_secs, 300);
        assert_eq!(cfg.max_concurrent_sessions, 50);
    }

    #[test]
    fn test_cognitive_tier_config_deserialize_from_toml() {
        let toml_str = r#"
tier1_max_distance = 1
tier2_max_distance = 3
tier3_max_distance = 10
tier2_tick_interval_minutes = 10
tier3_tick_interval_hours = 12
tier3_batch_size = 20
tier4_tick_interval_days = 30
"#;
        let cfg: CognitiveTierConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tier1_max_distance, 1);
        assert_eq!(cfg.tier2_max_distance, 3);
        assert_eq!(cfg.tier3_max_distance, 10);
        assert_eq!(cfg.tier2_tick_interval_minutes, 10);
        assert_eq!(cfg.tier3_tick_interval_hours, 12);
        assert_eq!(cfg.tier3_batch_size, 20);
        assert_eq!(cfg.tier4_tick_interval_days, 30);
    }

    #[test]
    fn test_cognitive_tier_config_deserialize_partial() {
        let toml_str = "tier1_max_distance = 1";
        let cfg: CognitiveTierConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.tier1_max_distance, 1);
        assert_eq!(cfg.tier2_max_distance, 2);
    }

    #[test]
    fn test_relationship_label_config_deserialize_from_toml() {
        let toml_str = r#"
very_close = 0.8
friendly = 0.4
acquainted = 0.1
cool = -0.2
strained = -0.6
"#;
        let cfg: RelationshipLabelConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.very_close - 0.8).abs() < f64::EPSILON);
        assert!((cfg.friendly - 0.4).abs() < f64::EPSILON);
        assert!((cfg.acquainted - 0.1).abs() < f64::EPSILON);
        assert!((cfg.cool - (-0.2)).abs() < f64::EPSILON);
        assert!((cfg.strained - (-0.6)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_reaction_config_deserialize_from_toml() {
        let toml_str = r#"
base_chance = 0.8
workplace_bonus = 0.2
indoor_bonus = 0.05
empathy_bonus = 0.1
negative_mood_penalty = 0.1
night_penalty = 0.05
llm_timeout_secs = 10
max_reactions = 5
"#;
        let cfg: ReactionConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.base_chance - 0.8).abs() < f64::EPSILON);
        assert!((cfg.workplace_bonus - 0.2).abs() < f64::EPSILON);
        assert!((cfg.indoor_bonus - 0.05).abs() < f64::EPSILON);
        assert!((cfg.empathy_bonus - 0.1).abs() < f64::EPSILON);
        assert!((cfg.negative_mood_penalty - 0.10).abs() < f64::EPSILON);
        assert!((cfg.night_penalty - 0.05).abs() < f64::EPSILON);
        assert_eq!(cfg.llm_timeout_secs, 10);
        assert_eq!(cfg.max_reactions, 5);
    }

    #[test]
    fn test_encounter_config_deserialize_from_toml() {
        let toml_str = r#"
dawn = 0.30
morning = 0.25
midday = 0.20
afternoon = 0.15
dusk = 0.10
night = 0.05
midnight = 0.02
"#;
        let cfg: EncounterConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.dawn - 0.30).abs() < f64::EPSILON);
        assert!((cfg.morning - 0.25).abs() < f64::EPSILON);
        assert!((cfg.midday - 0.20).abs() < f64::EPSILON);
        assert!((cfg.afternoon - 0.15).abs() < f64::EPSILON);
        assert!((cfg.dusk - 0.10).abs() < f64::EPSILON);
        assert!((cfg.night - 0.05).abs() < f64::EPSILON);
        assert!((cfg.midnight - 0.02).abs() < f64::EPSILON);
    }

    #[test]
    fn test_encounter_config_deserialize_partial() {
        let toml_str = "dawn = 0.10";
        let cfg: EncounterConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.dawn - 0.10).abs() < f64::EPSILON);
        assert!((cfg.morning - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn test_palette_config_deserialize_from_toml() {
        let toml_str = r#"
min_fg_bg_contrast = 90.0
min_muted_bg_contrast = 50.0
"#;
        let cfg: PaletteConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.min_fg_bg_contrast - 90.0).abs() < f32::EPSILON);
        assert!((cfg.min_muted_bg_contrast - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_palette_config_deserialize_partial() {
        let toml_str = "min_fg_bg_contrast = 70.0";
        let cfg: PaletteConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.min_fg_bg_contrast - 70.0).abs() < f32::EPSILON);
        assert!((cfg.min_muted_bg_contrast - 45.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_world_config_deserialize_from_toml() {
        let toml_str = "fuzzy_threshold = 0.90";
        let cfg: WorldConfig = toml::from_str(toml_str).unwrap();
        assert!((cfg.fuzzy_threshold - 0.90).abs() < f64::EPSILON);
    }

    #[test]
    fn test_persistence_config_deserialize_from_toml() {
        let cfg: PersistenceConfig = toml::from_str("").unwrap();
        // Intentionally empty struct (TD-011); just verify it parses.
        let _ = cfg;
    }

    #[test]
    fn test_inference_config_deserialize_from_toml() {
        let toml_str = r#"
timeout_secs = 45
streaming_timeout_secs = 600
reachability_timeout_secs = 15
model_download_timeout_secs = 7200
force_model_redownload = true
model_loading_timeout_secs = 600
log_capacity = 100
"#;
        let cfg: InferenceConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.timeout_secs, 45);
        assert_eq!(cfg.streaming_timeout_secs, 600);
        assert_eq!(cfg.reachability_timeout_secs, 15);
        assert_eq!(cfg.model_download_timeout_secs, 7200);
        assert!(cfg.force_model_redownload);
        assert_eq!(cfg.model_loading_timeout_secs, 600);
        assert_eq!(cfg.log_capacity, 100);
        assert!(cfg.rate_limits.default.is_none());
    }

    #[test]
    fn test_inference_config_deserialize_partial() {
        let toml_str = "timeout_secs = 60";
        let cfg: InferenceConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.timeout_secs, 60);
        assert_eq!(cfg.streaming_timeout_secs, 300);
    }

    #[test]
    fn test_map_config_deserialize_from_toml() {
        let toml_str = r#"
default_tile_source = "osm"

[tile_sources.osm]
label = "OpenStreetMap"
url = "https://tile.openstreetmap.org/{z}/{x}/{y}.png"
tile_size = 256
minzoom = 0
maxzoom = 19
attribution = "© OpenStreetMap contributors"
raster_saturation = -0.4
raster_opacity = 0.85
tms = false
"#;
        let cfg: MapConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.default_tile_source, "osm");
        assert_eq!(cfg.tile_sources.len(), 1);
        let osm = &cfg.tile_sources["osm"];
        assert_eq!(osm.label, "OpenStreetMap");
        assert_eq!(osm.url, "https://tile.openstreetmap.org/{z}/{x}/{y}.png");
        assert_eq!(osm.tile_size, 256);
        assert_eq!(osm.minzoom, 0);
        assert_eq!(osm.maxzoom, 19);
        assert_eq!(osm.attribution, "© OpenStreetMap contributors");
        assert!((osm.raster_saturation - (-0.4)).abs() < f32::EPSILON);
        assert!((osm.raster_opacity - 0.85).abs() < f32::EPSILON);
        assert!(!osm.tms);
    }

    // ── #1224 — dialogue display cap (AC-5) ──────────────────────────────────

    /// AC-5 (fix-1224-1225): `NpcConfig` must expose `dialogue_display_max_chars`
    /// with a sensible default, and it must survive round-trip TOML serialisation
    /// (old configs that omit the field must deserialise to the default).
    #[test]
    fn npc_config_has_dialogue_display_max_chars_with_sensible_default() {
        let cfg = NpcConfig::default();
        // Default should be > 0 (cap enabled) and > typical 2-4 sentence reply.
        assert!(
            cfg.dialogue_display_max_chars > 0,
            "cap must be enabled by default (non-zero)"
        );
        assert!(
            cfg.dialogue_display_max_chars >= 500,
            "cap must be generous enough not to clip normal 2-4 sentence replies"
        );
    }

    /// Old config without `dialogue_display_max_chars` must deserialise cleanly,
    /// falling back to the default value (serde `default` attribute).
    #[test]
    fn npc_config_dialogue_display_max_chars_defaults_when_absent() {
        let toml_str = r#"
[npc]
memory_capacity = 25
"#;
        let cfg: EngineConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.npc.memory_capacity, 25);
        assert_eq!(
            cfg.npc.dialogue_display_max_chars,
            NpcConfig::default().dialogue_display_max_chars,
            "missing field must deserialise to the default"
        );
    }

    // ── #1228 — dialogue repetition threshold (AC-5) ─────────────────────────

    /// AC-5 (fix-1228): `NpcConfig` exposes `dialogue_repetition_threshold` with
    /// a sensible default that enables the cross-turn guard by default.
    #[test]
    fn npc_config_has_dialogue_repetition_threshold_enabled_by_default() {
        let cfg = NpcConfig::default();
        assert!(
            cfg.dialogue_repetition_threshold > 0.0,
            "cross-turn guard must be enabled by default (threshold > 0)"
        );
        assert!(
            cfg.dialogue_repetition_threshold <= 1.0,
            "threshold is a Jaccard similarity in [0, 1]"
        );
        assert!(
            cfg.dialogue_repetition_threshold >= 0.8,
            "threshold must be strict enough to avoid flagging normal variation"
        );
    }

    /// Old config without `dialogue_repetition_threshold` must deserialise
    /// cleanly, falling back to the default (serde `default`).
    #[test]
    fn npc_config_dialogue_repetition_threshold_defaults_when_absent() {
        let toml_str = r#"
[npc]
memory_capacity = 25
"#;
        let cfg: EngineConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(
            cfg.npc.dialogue_repetition_threshold,
            NpcConfig::default().dialogue_repetition_threshold,
            "missing field must deserialise to the default"
        );
    }
}
