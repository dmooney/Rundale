//! Game mod loader for the engine/game-data separation.
//!
//! A "mod" is a directory containing a `mod.toml` manifest plus data files
//! (world graph, NPCs, encounters, etc.). The engine loads a [`GameMod`] at
//! startup and uses it to access all game-specific content at runtime.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ParishError;
use crate::npc::LanguageHint;
use crate::world::transport::TransportConfig;

// ---------------------------------------------------------------------------
// Manifest types (parsed from mod.toml)
// ---------------------------------------------------------------------------

/// Top-level manifest parsed from `mod.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModManifest {
    /// Mod identity metadata.
    #[serde(rename = "mod")]
    pub meta: ModMeta,
    /// Historical-setting parameters.
    pub setting: SettingConfig,
    /// Relative paths to data files inside the mod directory.
    pub files: FileRefs,
    /// Relative paths to prompt template text files.
    pub prompts: PromptRefs,
}

/// Identity metadata for a mod.
#[derive(Debug, Clone, Deserialize)]
pub struct ModMeta {
    /// Human-readable mod name.
    pub name: String,
    /// Display title for the splash screen (e.g. "Parish: Kilteevan 1820").
    /// Falls back to the engine default "Parish" if not set.
    #[serde(default)]
    pub title: Option<String>,
    /// Machine-friendly mod identifier (e.g. `kilteevan-1820`).
    pub id: String,
    /// Semantic version string.
    pub version: String,
    /// Short description of the mod.
    pub description: String,
}

/// Historical-setting parameters.
#[derive(Debug, Clone, Deserialize)]
pub struct SettingConfig {
    /// ISO 8601 start date/time for the game clock.
    pub start_date: String,
    /// Location id where the player begins.
    pub start_location: u32,
    /// Year used as cutoff for anachronism detection.
    pub period_year: u16,
}

/// Relative paths to structured data files inside the mod directory.
#[derive(Debug, Clone, Deserialize)]
pub struct FileRefs {
    /// World graph JSON file.
    pub world: String,
    /// NPC definitions JSON file.
    pub npcs: String,
    /// Anachronism terms JSON file.
    pub anachronisms: String,
    /// Festival definitions JSON file.
    pub festivals: String,
    /// Encounter table JSON file.
    pub encounters: String,
    /// Loading-screen configuration TOML file.
    pub loading: String,
    /// UI configuration TOML file.
    pub ui: String,
    /// Pronunciation hints JSON file (optional for backward compatibility).
    #[serde(default)]
    pub pronunciations: Option<String>,
    /// Transport modes TOML file (optional; defaults to walking only).
    #[serde(default)]
    pub transport: Option<String>,
}

/// Relative paths to prompt template text files.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptRefs {
    /// Tier-1 (reflexive) system prompt.
    pub tier1_system: String,
    /// Tier-1 (reflexive) context prompt.
    pub tier1_context: String,
    /// Tier-2 (deliberative) system prompt.
    pub tier2_system: String,
}

// ---------------------------------------------------------------------------
// Runtime data types (loaded from JSON / TOML files referenced by manifest)
// ---------------------------------------------------------------------------

/// Prompt templates loaded from text files.
#[derive(Debug, Clone)]
pub struct PromptTemplates {
    /// Tier-1 system prompt text.
    pub tier1_system: String,
    /// Tier-1 context prompt text.
    pub tier1_context: String,
    /// Tier-2 system prompt text.
    pub tier2_system: String,
}

/// A single anachronism term entry.
#[derive(Debug, Clone, Deserialize)]
pub struct AnachronismEntry {
    /// The anachronistic term or phrase.
    pub term: String,
    /// Category of anachronism (e.g. "technology", "slang").
    #[serde(default)]
    pub category: Option<String>,
    /// Earliest year this concept existed.
    #[serde(default)]
    pub origin_year: Option<u32>,
    /// Brief note explaining why the term is anachronistic.
    #[serde(default, alias = "reason")]
    pub note: String,
}

/// Anachronism detection data loaded from JSON.
#[derive(Debug, Clone, Deserialize)]
pub struct AnachronismData {
    /// Prefix injected into the LLM context alert.
    pub context_alert_prefix: String,
    /// Suffix injected into the LLM context alert.
    pub context_alert_suffix: String,
    /// Known anachronistic terms.
    pub terms: Vec<AnachronismEntry>,
}

/// A festival or holy day definition.
#[derive(Debug, Clone, Deserialize)]
pub struct FestivalDef {
    /// Festival name.
    pub name: String,
    /// Month (1–12).
    pub month: u32,
    /// Day of month (1–31).
    pub day: u32,
    /// Short description of the festival.
    pub description: String,
}

/// Encounter text table keyed by time-of-day label.
#[derive(Debug, Clone, Deserialize)]
pub struct EncounterTable {
    /// Encounter flavour text keyed by time-of-day (e.g. "morning", "night").
    #[serde(flatten)]
    pub by_time: HashMap<String, String>,
}

/// Loading-screen configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct LoadingConfig {
    /// Unicode frames for the spinner animation.
    pub spinner_frames: Vec<String>,
    /// RGB colours cycled through during the spinner animation.
    pub spinner_colors: Vec<[u8; 3]>,
    /// Random phrases shown while loading.
    pub phrases: Vec<String>,
}

/// Sidebar section of the UI configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct SidebarConfig {
    /// Label for the language-hints panel.
    #[serde(default = "default_hints_label")]
    pub hints_label: String,
}

/// Theme section of the UI configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemeConfig {
    /// Default accent colour (CSS hex string).
    #[serde(default = "default_accent")]
    pub default_accent: String,
}

/// UI configuration loaded from `ui.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiConfig {
    /// Sidebar panel settings.
    #[serde(default)]
    pub sidebar: SidebarConfig,
    /// Theme settings.
    #[serde(default)]
    pub theme: ThemeConfig,
}

fn default_hints_label() -> String {
    "Language Hints".to_string()
}

fn default_accent() -> String {
    "#c4a35a".to_string()
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            hints_label: default_hints_label(),
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            default_accent: default_accent(),
        }
    }
}

/// A single pronunciation entry from the mod's `pronunciations.json`.
///
/// Extends [`LanguageHint`] with a list of match strings used to associate
/// the pronunciation with NPC or location names (case-insensitive).
#[derive(Debug, Clone, Deserialize)]
pub struct PronunciationEntry {
    /// The word displayed in the sidebar (may include fada/diacritics).
    pub word: String,
    /// Phonetic pronunciation guide.
    pub pronunciation: String,
    /// English meaning or gloss.
    #[serde(default)]
    pub meaning: Option<String>,
    /// Strings to match against NPC/location names (case-insensitive substring).
    #[serde(default)]
    pub matches: Vec<String>,
}

impl PronunciationEntry {
    /// Convert to a [`LanguageHint`] for frontend display.
    pub fn to_hint(&self) -> LanguageHint {
        LanguageHint {
            word: self.word.clone(),
            pronunciation: self.pronunciation.clone(),
            meaning: self.meaning.clone(),
        }
    }

    /// Check whether this entry matches any of the given names.
    pub fn matches_any(&self, names: &[&str]) -> bool {
        for name in names {
            let name_lower = name.to_lowercase();
            // Check the match strings first
            for m in &self.matches {
                if name_lower.contains(&m.to_lowercase()) {
                    return true;
                }
            }
            // Fall back to matching the word itself
            if name_lower.contains(&self.word.to_lowercase()) {
                return true;
            }
        }
        false
    }
}

/// Pronunciation data loaded from the mod's `pronunciations.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct PronunciationData {
    /// Name pronunciation entries.
    pub names: Vec<PronunciationEntry>,
}

// ---------------------------------------------------------------------------
// GameMod
// ---------------------------------------------------------------------------

/// A loaded game mod containing all game-specific content.
///
/// Created via [`GameMod::load`] by pointing at a mod directory that contains
/// a `mod.toml` manifest. The engine holds one `GameMod` and queries it for
/// world paths, prompts, encounters, festivals, etc.
#[derive(Debug, Clone)]
pub struct GameMod {
    /// Parsed manifest from `mod.toml`.
    pub manifest: ModManifest,
    /// Absolute path to the mod directory.
    pub mod_dir: PathBuf,
    /// Prompt template strings loaded from text files.
    pub prompts: PromptTemplates,
    /// Anachronism detection data.
    pub anachronisms: AnachronismData,
    /// Festival definitions.
    pub festivals: Vec<FestivalDef>,
    /// Encounter text table.
    pub encounters: EncounterTable,
    /// Loading-screen configuration.
    pub loading: LoadingConfig,
    /// UI configuration.
    pub ui: UiConfig,
    /// Name pronunciation entries loaded from `pronunciations.json`.
    pub pronunciations: Vec<PronunciationEntry>,
    /// Transport modes configuration.
    pub transport: TransportConfig,
}

impl GameMod {
    /// Load a game mod from the given directory.
    ///
    /// Reads `mod.toml`, then loads every file referenced by the manifest.
    /// Returns a descriptive [`ParishError::Config`] if any file is missing or
    /// malformed.
    pub fn load(mod_dir: &Path) -> Result<Self, ParishError> {
        let mod_dir = mod_dir
            .canonicalize()
            .map_err(|e| ParishError::Config(format!("mod directory not found: {e}")))?;

        // -- manifest -------------------------------------------------------
        let manifest_path = mod_dir.join("mod.toml");
        let manifest_text = std::fs::read_to_string(&manifest_path).map_err(|e| {
            ParishError::Config(format!("failed to read {}: {e}", manifest_path.display()))
        })?;
        let manifest: ModManifest = toml::from_str(&manifest_text).map_err(|e| {
            ParishError::Config(format!("failed to parse {}: {e}", manifest_path.display()))
        })?;

        // -- helper to read a text file relative to mod_dir -----------------
        let read_text = |rel: &str| -> Result<String, ParishError> {
            let p = mod_dir.join(rel);
            std::fs::read_to_string(&p)
                .map_err(|e| ParishError::Config(format!("failed to read {}: {e}", p.display())))
        };

        // -- helper to read + deserialize JSON ------------------------------
        let read_json = |rel: &str| -> Result<String, ParishError> { read_text(rel) };

        // -- helper to read + deserialize TOML ------------------------------
        let read_toml_text = |rel: &str| -> Result<String, ParishError> { read_text(rel) };

        // -- prompts --------------------------------------------------------
        let prompts = PromptTemplates {
            tier1_system: read_text(&manifest.prompts.tier1_system)?,
            tier1_context: read_text(&manifest.prompts.tier1_context)?,
            tier2_system: read_text(&manifest.prompts.tier2_system)?,
        };

        // -- JSON data files ------------------------------------------------
        let anachronisms_json = read_json(&manifest.files.anachronisms)?;
        let anachronisms: AnachronismData =
            serde_json::from_str(&anachronisms_json).map_err(|e| {
                ParishError::Config(format!(
                    "failed to parse {}: {e}",
                    manifest.files.anachronisms
                ))
            })?;

        let festivals_json = read_json(&manifest.files.festivals)?;
        let festivals: Vec<FestivalDef> = serde_json::from_str(&festivals_json).map_err(|e| {
            ParishError::Config(format!("failed to parse {}: {e}", manifest.files.festivals))
        })?;

        let encounters_json = read_json(&manifest.files.encounters)?;
        let encounters: EncounterTable = serde_json::from_str(&encounters_json).map_err(|e| {
            ParishError::Config(format!(
                "failed to parse {}: {e}",
                manifest.files.encounters
            ))
        })?;

        // -- TOML data files ------------------------------------------------
        let loading_text = read_toml_text(&manifest.files.loading)?;
        let loading: LoadingConfig = toml::from_str(&loading_text).map_err(|e| {
            ParishError::Config(format!("failed to parse {}: {e}", manifest.files.loading))
        })?;

        let ui_text = read_toml_text(&manifest.files.ui)?;
        let ui: UiConfig = toml::from_str(&ui_text).map_err(|e| {
            ParishError::Config(format!("failed to parse {}: {e}", manifest.files.ui))
        })?;

        // -- optional pronunciation data ------------------------------------
        let pronunciations = if let Some(ref pron_path) = manifest.files.pronunciations {
            let pron_json = read_text(pron_path)?;
            let data: PronunciationData = serde_json::from_str(&pron_json)
                .map_err(|e| ParishError::Config(format!("failed to parse {}: {e}", pron_path)))?;
            data.names
        } else {
            vec![]
        };

        // -- transport (optional) ---------------------------------------------
        let transport = if let Some(ref transport_file) = manifest.files.transport {
            let transport_text = read_toml_text(transport_file)?;
            toml::from_str(&transport_text).map_err(|e| {
                ParishError::Config(format!("failed to parse {transport_file}: {e}"))
            })?
        } else {
            TransportConfig::default()
        };

        Ok(Self {
            manifest,
            mod_dir,
            prompts,
            anachronisms,
            festivals,
            encounters,
            loading,
            ui,
            pronunciations,
            transport,
        })
    }

    /// Absolute path to the world graph JSON file.
    pub fn world_path(&self) -> PathBuf {
        self.mod_dir.join(&self.manifest.files.world)
    }

    /// Absolute path to the NPC definitions JSON file.
    pub fn npcs_path(&self) -> PathBuf {
        self.mod_dir.join(&self.manifest.files.npcs)
    }

    /// ISO 8601 start date string from the manifest.
    pub fn start_date(&self) -> &str {
        &self.manifest.setting.start_date
    }

    /// Starting location id from the manifest.
    pub fn start_location(&self) -> u32 {
        self.manifest.setting.start_location
    }

    /// Period year used for anachronism detection.
    pub fn period_year(&self) -> u16 {
        self.manifest.setting.period_year
    }

    /// Look up encounter flavour text for a given time of day.
    pub fn encounter_text(&self, time_of_day: &str) -> Option<&str> {
        self.encounters.by_time.get(time_of_day).map(|s| s.as_str())
    }

    /// Returns pronunciation hints for names matching the given context strings.
    ///
    /// Typically called with the current location name and NPC names at
    /// the player's location. Returns a deduplicated list of [`LanguageHint`]
    /// values suitable for sidebar display.
    pub fn name_hints_for(&self, names: &[&str]) -> Vec<LanguageHint> {
        self.pronunciations
            .iter()
            .filter(|entry| entry.matches_any(names))
            .map(|entry| entry.to_hint())
            .collect()
    }

    /// Check whether a festival falls on the given month and day.
    pub fn check_festival(&self, month: u32, day: u32) -> Option<&FestivalDef> {
        self.festivals
            .iter()
            .find(|f| f.month == month && f.day == day)
    }
}

/// Interpolates `{placeholder}` patterns in a template string.
///
/// Replaces each `{key}` with the corresponding value from the provided
/// key-value pairs. Unknown placeholders are left as-is.
///
/// # Examples
///
/// ```ignore
/// let result = interpolate_template(
///     "Hello, {name}! You are {age} years old.",
///     &[("name", "Séamas"), ("age", "42")],
/// );
/// assert_eq!(result, "Hello, Séamas! You are 42 years old.");
/// ```
pub fn interpolate_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        let placeholder = format!("{{{}}}", key);
        result = result.replace(&placeholder, value);
    }
    result
}

/// Walk up from the current working directory looking for
/// `mods/kilteevan-1820/mod.toml`.
///
/// Returns the mod directory path (not the `mod.toml` path) if found.
pub fn find_default_mod() -> Option<PathBuf> {
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("mods/kilteevan-1820/mod.toml");
        if candidate.is_file() {
            return Some(dir.join("mods/kilteevan-1820"));
        }
        if !dir.pop() {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Build a minimal mod directory inside a tempdir and return it.
    fn create_test_mod() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // prompts/
        fs::create_dir_all(root.join("prompts")).unwrap();
        fs::write(root.join("prompts/tier1_system.txt"), "You are tier1.").unwrap();
        fs::write(root.join("prompts/tier1_context.txt"), "Context here.").unwrap();
        fs::write(root.join("prompts/tier2_system.txt"), "You are tier2.").unwrap();

        // world.json (content not parsed by GameMod, just path referenced)
        fs::write(root.join("world.json"), "{}").unwrap();

        // npcs.json
        fs::write(root.join("npcs.json"), "[]").unwrap();

        // anachronisms.json
        fs::write(
            root.join("anachronisms.json"),
            r#"{
                "context_alert_prefix": "NOTE:",
                "context_alert_suffix": "END",
                "terms": [
                    {"term": "internet", "reason": "not invented until the 20th century"}
                ]
            }"#,
        )
        .unwrap();

        // festivals.json
        fs::write(
            root.join("festivals.json"),
            r#"[
                {"name": "St Patrick's Day", "month": 3, "day": 17, "description": "Patron saint feast."},
                {"name": "May Day", "month": 5, "day": 1, "description": "Start of summer."}
            ]"#,
        )
        .unwrap();

        // encounters.json
        fs::write(
            root.join("encounters.json"),
            r#"{"morning": "A farmer waves.", "night": "An owl hoots."}"#,
        )
        .unwrap();

        // loading.toml
        fs::write(
            root.join("loading.toml"),
            r#"
spinner_frames = ["|", "/", "-", "\\"]
spinner_colors = [[200, 180, 100], [100, 200, 100]]
phrases = ["Loading...", "Please wait..."]
"#,
        )
        .unwrap();

        // ui.toml
        fs::write(
            root.join("ui.toml"),
            r##"
[sidebar]
hints_label = "Focail"

[theme]
default_accent = "#aabbcc"
"##,
        )
        .unwrap();

        // mod.toml
        fs::write(
            root.join("mod.toml"),
            r#"
[mod]
name = "Test Mod"
id = "test-mod"
version = "0.1.0"
description = "A test mod."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820

[files]
world = "world.json"
npcs = "npcs.json"
anachronisms = "anachronisms.json"
festivals = "festivals.json"
encounters = "encounters.json"
loading = "loading.toml"
ui = "ui.toml"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
        )
        .unwrap();

        tmp
    }

    #[test]
    fn test_load_mod_from_directory() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).expect("should load test mod");
        assert_eq!(gm.manifest.meta.id, "test-mod");
        assert_eq!(gm.manifest.meta.name, "Test Mod");
        assert_eq!(gm.prompts.tier1_system, "You are tier1.");
        assert_eq!(gm.anachronisms.terms.len(), 1);
        assert_eq!(gm.festivals.len(), 2);
        assert_eq!(gm.loading.spinner_frames.len(), 4);
        // No pronunciations file referenced → empty vec
        assert!(gm.pronunciations.is_empty());
        // No transport.toml in test mod — should default to walking
        assert_eq!(gm.transport.default, "walking");
        assert_eq!(gm.transport.modes.len(), 1);
        assert_eq!(gm.transport.default_mode().id, "walking");
    }

    #[test]
    fn test_mod_world_path() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).unwrap();
        assert!(gm.world_path().ends_with("world.json"));
        assert!(gm.world_path().is_absolute());
    }

    #[test]
    fn test_mod_npcs_path() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).unwrap();
        assert!(gm.npcs_path().ends_with("npcs.json"));
        assert!(gm.npcs_path().is_absolute());
    }

    #[test]
    fn test_encounter_text_lookup() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).unwrap();
        assert_eq!(gm.encounter_text("morning"), Some("A farmer waves."));
        assert_eq!(gm.encounter_text("night"), Some("An owl hoots."));
        assert_eq!(gm.encounter_text("afternoon"), None);
    }

    #[test]
    fn test_check_festival() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).unwrap();
        let fest = gm
            .check_festival(3, 17)
            .expect("should find St Patrick's Day");
        assert_eq!(fest.name, "St Patrick's Day");
        assert!(gm.check_festival(12, 25).is_none());
    }

    #[test]
    fn test_load_nonexistent_dir() {
        let result = GameMod::load(Path::new("/tmp/nonexistent_parish_mod_dir_12345"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("mod directory not found"), "got: {err}");
    }

    #[test]
    fn test_festival_def_deserialize() {
        let json = r#"{"name":"Lughnasa","month":8,"day":1,"description":"Harvest festival."}"#;
        let f: FestivalDef = serde_json::from_str(json).unwrap();
        assert_eq!(f.name, "Lughnasa");
        assert_eq!(f.month, 8);
        assert_eq!(f.day, 1);
    }

    #[test]
    fn test_loading_config_deserialize() {
        let toml_str = r#"
spinner_frames = ["a", "b"]
spinner_colors = [[255, 0, 0]]
phrases = ["Loading"]
"#;
        let lc: LoadingConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(lc.spinner_frames, vec!["a", "b"]);
        assert_eq!(lc.spinner_colors, vec![[255, 0, 0]]);
        assert_eq!(lc.phrases, vec!["Loading"]);
    }

    #[test]
    fn test_ui_config_defaults() {
        let toml_str = "";
        let ui: UiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(ui.sidebar.hints_label, "Language Hints");
        assert_eq!(ui.theme.default_accent, "#c4a35a");
    }

    #[test]
    fn test_ui_config_custom() {
        let toml_str = r##"
[sidebar]
hints_label = "Custom"

[theme]
default_accent = "#ff0000"
"##;
        let ui: UiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(ui.sidebar.hints_label, "Custom");
        assert_eq!(ui.theme.default_accent, "#ff0000");
    }

    #[test]
    fn test_anachronism_entry_deserialize() {
        // JSON with the current format (note, category, origin_year)
        let json = r#"{"term":"telephone","category":"technology","origin_year":1876,"note":"invented by Bell in 1876"}"#;
        let e: AnachronismEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.term, "telephone");
        assert_eq!(e.note, "invented by Bell in 1876");
        assert_eq!(e.category.as_deref(), Some("technology"));
        assert_eq!(e.origin_year, Some(1876));
    }

    #[test]
    fn test_anachronism_entry_deserialize_legacy_reason() {
        // Backward compatible: accepts "reason" alias for "note"
        let json = r#"{"term":"telephone","reason":"invented 1876"}"#;
        let e: AnachronismEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.term, "telephone");
        assert_eq!(e.note, "invented 1876");
    }

    #[test]
    fn test_interpolate_template() {
        let result = interpolate_template(
            "Hello, {name}! You are a {occupation}.",
            &[("name", "Séamas"), ("occupation", "publican")],
        );
        assert_eq!(result, "Hello, Séamas! You are a publican.");
    }

    #[test]
    fn test_interpolate_template_missing_key() {
        let result = interpolate_template("Hello, {name}! Age: {age}", &[("name", "Aoife")]);
        assert_eq!(result, "Hello, Aoife! Age: {age}");
    }

    #[test]
    fn test_interpolate_template_no_placeholders() {
        let result = interpolate_template("No placeholders here.", &[("key", "value")]);
        assert_eq!(result, "No placeholders here.");
    }

    #[test]
    fn test_interpolate_template_empty() {
        let result = interpolate_template("", &[("key", "value")]);
        assert_eq!(result, "");
    }

    // -- Pronunciation tests --------------------------------------------------

    /// Build a test mod that includes a pronunciations.json file.
    fn create_test_mod_with_pronunciations() -> TempDir {
        let tmp = create_test_mod();
        let root = tmp.path();

        fs::write(
            root.join("pronunciations.json"),
            r#"{
                "names": [
                    {"word": "Niamh", "pronunciation": "NEEV", "meaning": "brightness", "matches": ["Niamh"]},
                    {"word": "Siobhán", "pronunciation": "shiv-AWN", "meaning": "Irish form of Joan", "matches": ["Siobhan"]},
                    {"word": "Kilteevan", "pronunciation": "kill-TEE-van", "meaning": "church of St. Tíobán", "matches": ["Kilteevan"]}
                ]
            }"#,
        )
        .unwrap();

        // Rewrite mod.toml to include pronunciations
        fs::write(
            root.join("mod.toml"),
            r#"
[mod]
name = "Test Mod"
id = "test-mod"
version = "0.1.0"
description = "A test mod."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820

[files]
world = "world.json"
npcs = "npcs.json"
anachronisms = "anachronisms.json"
festivals = "festivals.json"
encounters = "encounters.json"
loading = "loading.toml"
ui = "ui.toml"
pronunciations = "pronunciations.json"

[prompts]
tier1_system = "prompts/tier1_system.txt"
tier1_context = "prompts/tier1_context.txt"
tier2_system = "prompts/tier2_system.txt"
"#,
        )
        .unwrap();

        tmp
    }

    #[test]
    fn test_load_mod_with_pronunciations() {
        let tmp = create_test_mod_with_pronunciations();
        let gm = GameMod::load(tmp.path()).expect("should load mod with pronunciations");
        assert_eq!(gm.pronunciations.len(), 3);
        assert_eq!(gm.pronunciations[0].word, "Niamh");
        assert_eq!(gm.pronunciations[0].pronunciation, "NEEV");
    }

    #[test]
    fn test_name_hints_for_matching() {
        let tmp = create_test_mod_with_pronunciations();
        let gm = GameMod::load(tmp.path()).unwrap();

        // Match NPC name containing "Niamh"
        let hints = gm.name_hints_for(&["Niamh Darcy"]);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].word, "Niamh");
        assert_eq!(hints[0].pronunciation, "NEEV");

        // Match location name
        let hints = gm.name_hints_for(&["Kilteevan Village"]);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].word, "Kilteevan");

        // Multiple matches
        let hints = gm.name_hints_for(&["Niamh Darcy", "Kilteevan Village"]);
        assert_eq!(hints.len(), 2);

        // No match
        let hints = gm.name_hints_for(&["Tommy O'Brien"]);
        assert!(hints.is_empty());
    }

    #[test]
    fn test_name_hints_case_insensitive() {
        let tmp = create_test_mod_with_pronunciations();
        let gm = GameMod::load(tmp.path()).unwrap();

        let hints = gm.name_hints_for(&["niamh darcy"]);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].word, "Niamh");
    }

    #[test]
    fn test_pronunciation_entry_deserialize() {
        let json =
            r#"{"word":"Aoife","pronunciation":"EE-fa","meaning":"beauty","matches":["Aoife"]}"#;
        let e: PronunciationEntry = serde_json::from_str(json).unwrap();
        assert_eq!(e.word, "Aoife");
        assert_eq!(e.pronunciation, "EE-fa");
        assert_eq!(e.meaning, Some("beauty".to_string()));
        assert_eq!(e.matches, vec!["Aoife"]);
    }

    #[test]
    fn test_pronunciation_entry_matches_via_word_fallback() {
        let json = r#"{"word":"Aoife","pronunciation":"EE-fa"}"#;
        let e: PronunciationEntry = serde_json::from_str(json).unwrap();
        // No explicit matches → falls back to matching the word itself
        assert!(e.matches_any(&["Aoife Brennan"]));
        assert!(!e.matches_any(&["Tommy O'Brien"]));
    }

    // -- Integration test against the real mod directory (skipped in CI) ----

    #[test]
    fn test_load_real_default_mod() {
        if let Some(mod_dir) = find_default_mod() {
            let gm = GameMod::load(&mod_dir).expect("should load default mod");
            assert!(!gm.manifest.meta.name.is_empty());
            assert!(gm.world_path().is_absolute());
            assert!(gm.npcs_path().is_absolute());
            // The kilteevan mod should have pronunciation data
            assert!(
                !gm.pronunciations.is_empty(),
                "default mod should have pronunciation entries"
            );
        }
    }

    #[test]
    fn test_real_mod_npc_name_hints() {
        if let Some(mod_dir) = find_default_mod() {
            let gm = GameMod::load(&mod_dir).expect("should load default mod");

            // Each NPC with an Irish name should produce a hint
            let hints = gm.name_hints_for(&["Padraig Darcy"]);
            assert_eq!(hints.len(), 1, "Padraig should match");
            assert_eq!(hints[0].word, "Pádraig");

            let hints = gm.name_hints_for(&["Siobhan Murphy"]);
            assert_eq!(hints.len(), 1, "Siobhan should match");
            assert_eq!(hints[0].word, "Siobhán");

            let hints = gm.name_hints_for(&["Niamh Darcy"]);
            assert_eq!(hints.len(), 1, "Niamh should match");

            let hints = gm.name_hints_for(&["Aoife Brennan"]);
            assert_eq!(hints.len(), 1, "Aoife should match");

            let hints = gm.name_hints_for(&["Roisin Connolly"]);
            assert_eq!(hints.len(), 1, "Roisin should match");

            // Location + NPC combined
            let hints = gm.name_hints_for(&["Kilteevan Village", "Padraig Darcy", "Niamh Darcy"]);
            assert_eq!(hints.len(), 3, "should match location + both NPCs");
        }
    }
}
