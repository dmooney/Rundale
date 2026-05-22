//! Game mod loader for the engine/game-data separation.
//!
//! A "mod" is a directory containing a `mod.toml` manifest plus data files
//! (world graph, NPCs, encounters, etc.). The engine loads a [`GameMod`] at
//! startup and uses it to access all game-specific content at runtime.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::ParishError;
use crate::ipc::ThemePalette;
use crate::npc::LanguageHint;
use crate::world::transport::TransportConfig;
use parish_types::AnachronismEntry;

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
    /// Display title for the splash screen (e.g. "Rundale").
    /// Falls back to the engine default "Parish" if not set.
    #[serde(default)]
    pub title: Option<String>,
    /// Machine-friendly mod identifier (e.g. `rundale`).
    pub id: String,
    /// Optional override for the per-user data-directory name (saves +
    /// tile cache). When set, takes precedence over `name`; engine fallback
    /// when neither is meaningful is `"Parish"`. Set explicitly so a future
    /// rename of `name` doesn't silently relocate everyone's saves.
    #[serde(default)]
    pub save_root: Option<String>,
    /// Semantic version string.
    pub version: String,
    /// Short description of the mod.
    pub description: String,
    /// Mod kind. Defaults to [`ModKind::Setting`] when omitted, so older
    /// manifests without the field continue to load as primary mods.
    #[serde(default)]
    pub kind: ModKind,
    /// Hard dependencies (parsed but not yet enforced — reserved for the
    /// upcoming dependency resolver).
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Soft dependencies (parsed, not enforced).
    #[serde(default)]
    pub optional_dependencies: Vec<String>,
    /// Conflicting mod ids (parsed, not enforced).
    #[serde(default)]
    pub conflicts: Vec<String>,
}

/// Kinds of Parish mod, in the Factorio sense — declared via `kind = "..."`
/// in a manifest's `[mod]` table.
///
/// Implemented today: [`ModKind::Setting`] (the existing primary path),
/// [`ModKind::Asset`] (additive registries such as themes), and
/// [`ModKind::Providers`] (LLM provider catalog mods loaded into
/// `ProviderRegistry` at startup). The remaining variants are reserved so
/// manifests can be authored against the final schema today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModKind {
    /// Owns the world graph, NPC roster, prompts, and calendar baseline.
    /// Exactly one setting mod must be active.
    #[default]
    Setting,
    /// Pure additive registries (themes, sounds, fonts). No gameplay content.
    Asset,
    /// LLM provider catalog. Carries one or more `providers/<id>.toml` files
    /// that merge into `ProviderRegistry` at startup. Used to ship cloud
    /// providers (anthropic, openai, openrouter, ...) as runtime-loadable
    /// mods rather than compile-time-embedded data.
    Providers,
    /// Adds new gameplay entries (extra NPCs, locations, festivals).
    Content,
    /// Mutates entries already in the active setting.
    Override,
    /// Localized strings and pronunciations.
    Localization,
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
    /// BCP 47 tag for the language the player and NPCs primarily speak in dialogue.
    /// Defaults to "en" for backward compatibility with mods that omit it.
    #[serde(default = "default_player_language")]
    pub player_language: String,
    /// BCP 47 tag for the secondary language NPCs code-switch into. None means monolingual.
    #[serde(default)]
    pub native_language: Option<String>,
}

fn default_player_language() -> String {
    "en".to_string()
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
    /// NPC arrival reaction templates JSON file (optional; defaults to hardcoded bank).
    #[serde(default)]
    pub reactions: Option<String>,
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

/// Anachronism detection data loaded from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnachronismData {
    /// Prefix injected into the LLM context alert.
    pub context_alert_prefix: String,
    /// Suffix injected into the LLM context alert.
    pub context_alert_suffix: String,
    /// Known anachronistic terms.
    pub terms: Vec<AnachronismEntry>,
}

/// A festival or holy day definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
///
/// Uses [`BTreeMap`] so the JSON output is deterministic — the editor
/// relies on this for the empty-`git diff` round-trip invariant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterTable {
    /// Encounter flavour text keyed by time-of-day (e.g. "morning", "night").
    #[serde(flatten)]
    pub by_time: std::collections::BTreeMap<String, String>,
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

/// Theme palette configuration loaded from `ui.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ThemePaletteConfig {
    /// Main background colour.
    #[serde(default = "default_theme_bg")]
    pub bg: String,
    /// Primary text colour.
    #[serde(default = "default_theme_fg")]
    pub fg: String,
    /// Accent colour for highlights and status UI.
    #[serde(default = "default_theme_accent")]
    pub accent: String,
    /// Panel background colour.
    #[serde(default = "default_theme_panel_bg")]
    pub panel_bg: String,
    /// Input background colour.
    #[serde(default = "default_theme_input_bg")]
    pub input_bg: String,
    /// Border and separator colour.
    #[serde(default = "default_theme_border")]
    pub border: String,
    /// Secondary / muted text colour.
    #[serde(default = "default_theme_muted")]
    pub muted: String,
}

impl From<ThemePaletteConfig> for ThemePalette {
    fn from(config: ThemePaletteConfig) -> Self {
        ThemePalette {
            bg: config.bg,
            fg: config.fg,
            accent: config.accent,
            panel_bg: config.panel_bg,
            input_bg: config.input_bg,
            border: config.border,
            muted: config.muted,
        }
    }
}

impl Default for ThemePaletteConfig {
    fn default() -> Self {
        Self {
            bg: default_theme_bg(),
            fg: default_theme_fg(),
            accent: default_theme_accent(),
            panel_bg: default_theme_panel_bg(),
            input_bg: default_theme_input_bg(),
            border: default_theme_border(),
            muted: default_theme_muted(),
        }
    }
}

/// Theme section of the UI configuration.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ThemeConfig {
    /// Legacy accent override for older mods.
    #[serde(default)]
    pub default_accent: Option<String>,
    /// Fixed theme palette used by the frontend.
    #[serde(default)]
    pub palette: ThemePaletteConfig,
}

/// UI configuration loaded from `ui.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UiConfig {
    /// Mod branding assets, such as the app/window icon.
    #[serde(default)]
    pub branding: BrandingConfig,
    /// Sidebar panel settings.
    #[serde(default)]
    pub sidebar: SidebarConfig,
    /// Theme settings.
    #[serde(default)]
    pub theme: ThemeConfig,
}

/// Branding assets loaded from `ui.toml`.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrandingConfig {
    /// Primary app/window icon relative to the mod directory.
    #[serde(default)]
    pub app_icon: Option<String>,
    /// Small browser favicon relative to the mod directory.
    #[serde(default)]
    pub favicon: Option<String>,
}

fn default_hints_label() -> String {
    "Language Hints".to_string()
}

fn default_theme_bg() -> String {
    "#fafad8".to_string()
}

fn default_theme_fg() -> String {
    "#31240f".to_string()
}

fn default_theme_accent() -> String {
    "#b08531".to_string()
}

fn default_theme_panel_bg() -> String {
    "#f5f5d3".to_string()
}

fn default_theme_input_bg() -> String {
    "#f0f0ce".to_string()
}

fn default_theme_border() -> String {
    "#cec293".to_string()
}

fn default_theme_muted() -> String {
    "#76663b".to_string()
}

/// Returns the built-in fixed theme palette used when a mod does not provide one.
pub fn default_theme_palette() -> ThemePalette {
    ThemePaletteConfig::default().into()
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            hints_label: default_hints_label(),
        }
    }
}

impl ThemeConfig {
    /// Returns the fully resolved theme palette, applying any legacy overrides.
    pub fn resolved_palette(&self) -> ThemePalette {
        let mut palette = ThemePalette::from(self.palette.clone());
        if let Some(ref accent) = self.default_accent {
            palette.accent = accent.clone();
        }
        palette
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
        // Pre-compute lowercased match strings and word once, rather than
        // re-allocating them for every name in the outer loop.
        // Reduces allocations from O(names × matches) to O(matches + 1).
        let matches_lower: Vec<String> = self.matches.iter().map(|m| m.to_lowercase()).collect();
        let word_lower = self.word.to_lowercase();

        for name in names {
            let name_lower = name.to_lowercase();
            // Check the match strings first
            for m in &matches_lower {
                if name_lower.contains(m.as_str()) {
                    return true;
                }
            }
            // Fall back to matching the word itself
            if name_lower.contains(word_lower.as_str()) {
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
    /// NPC arrival reaction templates (loaded from JSON or hardcoded defaults).
    pub reactions: crate::npc::reactions::ReactionTemplates,
}

impl ModMeta {
    /// Name used for the per-user data folder (saves + tile cache).
    ///
    /// Resolution: explicit `save_root` field on `mod.toml` first (when it
    /// has non-whitespace content), then `name`. Engine-only runs with no
    /// mod loaded should use [`parish_persistence::paths::DEFAULT_APP_NAME`]
    /// instead of calling this.
    ///
    /// This does **not** sanitise path separators; callers that turn the
    /// result into a directory name must use [`app_name_from_mod`] (or apply
    /// equivalent basename + traversal guarding) so a malicious `save_root`
    /// can't write outside the user-data root.
    pub fn app_name(&self) -> &str {
        self.save_root
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.name)
    }
}

/// Shared resolver for the per-user data folder name used by saves + tile cache.
///
/// Centralises the `Option<GameMod>` → `app_name` mapping so the server,
/// Tauri, and CLI entry points never drift (rule #12). Returns the first of:
///
/// 1. Sanitised `save_root` from the active mod's `mod.toml` (when set and
///    valid after sanitisation).
/// 2. Sanitised `name` from the active mod's `mod.toml`.
/// 3. The engine fallback [`parish_persistence::paths::DEFAULT_APP_NAME`]
///    — used only when no mod is loaded or every candidate sanitises away.
///
/// Sanitisation: trims whitespace, strips any path separators by taking the
/// basename only, and rejects `.` / `..` / empty. A mod that sets
/// `save_root = "../../etc"` therefore can't redirect save I/O outside the
/// per-user root — and an invalid `save_root` falls through to the mod's
/// `name` rather than collapsing unrelated mods into a shared `Parish` dir.
pub fn app_name_from_mod(game_mod: &Option<GameMod>) -> String {
    if let Some(gm) = game_mod.as_ref() {
        let meta = &gm.manifest.meta;
        if let Some(s) = meta.save_root.as_deref().and_then(sanitize_app_name) {
            return s;
        }
        if let Some(s) = sanitize_app_name(&meta.name) {
            return s;
        }
    }
    parish_persistence::paths::DEFAULT_APP_NAME.to_string()
}

/// Returns a safe folder-name form of `raw`, or `None` if `raw` cannot be
/// used as a directory name without breaking per-user isolation.
fn sanitize_app_name(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let basename = std::path::Path::new(trimmed).file_name()?.to_str()?;
    if basename.is_empty() || basename == "." || basename == ".." {
        return None;
    }
    Some(basename.to_string())
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
        // Guards against directory traversal: a malicious mod.toml could
        // specify "../../etc/passwd" — refuse anything that resolves outside
        // mod_dir (#741).
        let read_text = |rel: &str| -> Result<String, ParishError> {
            let p = mod_dir.join(rel);
            let canonical = p.canonicalize().map_err(|e| {
                ParishError::Config(format!("failed to resolve {}: {e}", p.display()))
            })?;
            if !canonical.starts_with(&mod_dir) {
                return Err(ParishError::Config(format!(
                    "manifest path {} escapes mod directory",
                    rel
                )));
            }
            std::fs::read_to_string(&canonical).map_err(|e| {
                ParishError::Config(format!("failed to read {}: {e}", canonical.display()))
            })
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
        validate_optional_asset_ref(
            &mod_dir,
            "ui.branding.app_icon",
            ui.branding.app_icon.as_deref(),
        )?;
        validate_optional_asset_ref(
            &mod_dir,
            "ui.branding.favicon",
            ui.branding.favicon.as_deref(),
        )?;

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

        // -- reactions (optional) -----------------------------------------------
        let reactions = if let Some(ref reactions_file) = manifest.files.reactions {
            let reactions_json = read_json(reactions_file)?;
            serde_json::from_str(&reactions_json).map_err(|e| {
                ParishError::Config(format!("failed to parse {reactions_file}: {e}"))
            })?
        } else {
            crate::npc::reactions::ReactionTemplates::default()
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
            reactions,
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

    /// Absolute path to the mod's app/window icon, if one is configured.
    pub fn app_icon_path(&self) -> Option<PathBuf> {
        self.resolve_asset_path(self.ui.branding.app_icon.as_deref())
    }

    /// Absolute path to the browser favicon, falling back to the app icon.
    pub fn favicon_path(&self) -> Option<PathBuf> {
        self.resolve_asset_path(
            self.ui
                .branding
                .favicon
                .as_deref()
                .or(self.ui.branding.app_icon.as_deref()),
        )
    }

    fn resolve_asset_path(&self, rel: Option<&str>) -> Option<PathBuf> {
        rel.and_then(|path| canonical_mod_asset_path(&self.mod_dir, path).ok())
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

    /// BCP 47 tag for the primary dialogue language.
    pub fn player_language(&self) -> &str {
        &self.manifest.setting.player_language
    }

    /// BCP 47 tag for the secondary code-switch language, if any.
    pub fn native_language(&self) -> Option<&str> {
        self.manifest.setting.native_language.as_deref()
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
/// Creates a [`crate::world::WorldState`] from a loaded [`GameMod`].
///
/// Bridges [`GameMod`] (which lives in `parish-core`) and
/// [`crate::world::WorldState::from_mod_params`] (which lives in `parish-world`
/// and cannot depend on `parish-core`).
pub fn world_state_from_mod(
    game_mod: &GameMod,
) -> Result<crate::world::WorldState, parish_types::ParishError> {
    crate::world::WorldState::from_mod_params(
        &game_mod.world_path(),
        parish_types::LocationId(game_mod.start_location()),
        game_mod.start_date(),
    )
}

fn validate_optional_asset_ref(
    mod_dir: &Path,
    field: &str,
    rel: Option<&str>,
) -> Result<(), ParishError> {
    match rel {
        Some(path) => canonical_mod_asset_path(mod_dir, path)
            .map(|_| ())
            .map_err(|e| ParishError::Config(format!("{field}: {e}"))),
        None => Ok(()),
    }
}

fn canonical_mod_asset_path(mod_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(format!("asset path {rel} must be relative"));
    }
    if !rel_path.starts_with("assets") {
        return Err(format!("asset path {rel} must live under assets/"));
    }
    if rel_path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("asset path {rel} contains invalid path components"));
    }

    let candidate = mod_dir.join(rel_path);
    let canonical = candidate
        .canonicalize()
        .map_err(|e| format!("failed to resolve {}: {e}", candidate.display()))?;
    if !canonical.starts_with(mod_dir) {
        return Err(format!("asset path {rel} escapes mod directory"));
    }
    Ok(canonical)
}

/// All mods discovered under a `mods/` root.
///
/// `setting` is the unique [`ModKind::Setting`] mod (rundale today). Every
/// other mod is recorded in `auxiliary` in lexicographic-by-directory-name
/// order so registry merging is deterministic across machines and tests.
#[derive(Debug, Clone)]
pub struct DiscoveredMods {
    pub setting: PathBuf,
    pub auxiliary: Vec<DiscoveredMod>,
}

#[derive(Debug, Clone)]
pub struct DiscoveredMod {
    pub path: PathBuf,
    pub kind: ModKind,
    pub id: String,
}

/// Lightweight read of just `[mod]` from a `mod.toml`, used during discovery
/// to classify a directory before deciding how to load it.
#[derive(Debug, Clone, Deserialize)]
struct ModMetaOnly {
    #[serde(rename = "mod")]
    meta: ModMeta,
}

/// Walk up from the current working directory looking for a `mods/`
/// directory; once found, enumerate every `mods/*/mod.toml` and classify
/// each by its declared `kind`.
///
/// Errors at startup if zero or more-than-one setting mods are present.
pub fn discover_mods() -> Result<DiscoveredMods, ParishError> {
    let mods_root = find_mods_root()
        .ok_or_else(|| ParishError::Config("No `mods/` directory found".to_string()))?;
    discover_mods_in(&mods_root)
}

/// Variant of [`discover_mods`] that scans an explicit `mods/` root. Used by
/// tests; production callers want [`discover_mods`].
pub fn discover_mods_in(mods_root: &Path) -> Result<DiscoveredMods, ParishError> {
    let mut entries: Vec<_> = std::fs::read_dir(mods_root)
        .map_err(|e| ParishError::Config(format!("read_dir({}): {e}", mods_root.display())))?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("mod.toml").is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut setting: Option<PathBuf> = None;
    let mut setting_id: Option<String> = None;
    let mut auxiliary: Vec<DiscoveredMod> = Vec::new();

    for entry in entries {
        let dir = entry.path();
        let manifest_path = dir.join("mod.toml");
        let raw = std::fs::read_to_string(&manifest_path)
            .map_err(|e| ParishError::Config(format!("read {}: {e}", manifest_path.display())))?;
        let parsed: ModMetaOnly = toml::from_str(&raw)
            .map_err(|e| ParishError::Config(format!("parse {}: {e}", manifest_path.display())))?;
        let meta = parsed.meta;
        match meta.kind {
            ModKind::Setting => {
                if let Some(prev) = &setting_id {
                    return Err(ParishError::Config(format!(
                        "Multiple setting mods active: '{prev}' and '{}'. Only one mod may declare kind = \"setting\".",
                        meta.id
                    )));
                }
                setting = Some(dir.clone());
                setting_id = Some(meta.id);
            }
            other => auxiliary.push(DiscoveredMod {
                path: dir,
                kind: other,
                id: meta.id,
            }),
        }
    }

    let setting = setting.ok_or_else(|| {
        ParishError::Config(
            "No setting mod found (expected exactly one mod with kind = \"setting\").".to_string(),
        )
    })?;
    Ok(DiscoveredMods { setting, auxiliary })
}

/// Iterate every `ModKind::Providers` mod in `discovered.auxiliary`, load
/// its `providers/*.toml` files, and merge them into
/// `parish_config::registry()`. Idempotent — guarded by a process-wide
/// `OnceLock` so repeated calls (e.g. from both the Tauri-sync path and a
/// later server reload) do not double-register.
///
/// Returns `Ok(count)` of providers registered on the first call, or
/// `Ok(0)` on subsequent calls. Failure to load any single provider mod
/// is fatal — startup should refuse to continue rather than silently lose
/// providers and confuse the user later.
pub fn register_provider_mods_once(discovered: &DiscoveredMods) -> Result<usize, ParishError> {
    use std::sync::OnceLock;
    static GUARD: OnceLock<()> = OnceLock::new();
    if GUARD.get().is_some() {
        return Ok(0);
    }

    let mut all: Vec<parish_config::ProviderMod> = Vec::new();
    for aux in &discovered.auxiliary {
        if aux.kind == ModKind::Providers {
            let providers = load_providers_from_mod(&aux.path)?;
            tracing::info!(
                mod_id = %aux.id,
                count = providers.len(),
                "registered provider mod"
            );
            all.extend(providers);
        }
    }
    let count = all.len();
    parish_config::registry().register_mod_providers(all)?;
    let _ = GUARD.set(());
    Ok(count)
}

/// Load every `<mod_dir>/providers/*.toml` and parse each into a
/// [`parish_config::ProviderMod`]. Intended for mods declared with
/// `kind = "providers"`.
///
/// Sorts entries lexicographically by filename so the registration order
/// is deterministic across machines + filesystems.
///
/// Path safety: each provider TOML must live directly under
/// `<mod_dir>/providers/` (no traversal, no symlinks escaping the mod
/// directory). The check mirrors [`canonical_mod_asset_path`] without
/// requiring the `assets/` prefix, since provider catalogs are their own
/// directory layer.
pub fn load_providers_from_mod(
    mod_dir: &Path,
) -> Result<Vec<parish_config::ProviderMod>, ParishError> {
    let providers_dir = mod_dir.join("providers");
    if !providers_dir.is_dir() {
        return Ok(Vec::new());
    }

    let canonical_mod_dir = mod_dir
        .canonicalize()
        .map_err(|e| ParishError::Config(format!("canonicalize({}): {e}", mod_dir.display())))?;

    let mut entries: Vec<_> = std::fs::read_dir(&providers_dir)
        .map_err(|e| ParishError::Config(format!("read_dir({}): {e}", providers_dir.display())))?
        .filter_map(Result::ok)
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut out: Vec<parish_config::ProviderMod> = Vec::with_capacity(entries.len());
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for entry in entries {
        let path = entry.path();
        let canonical = path
            .canonicalize()
            .map_err(|e| ParishError::Config(format!("canonicalize({}): {e}", path.display())))?;
        if !canonical.starts_with(&canonical_mod_dir) {
            return Err(ParishError::Config(format!(
                "provider TOML {} escapes mod directory {}",
                path.display(),
                mod_dir.display()
            )));
        }

        let raw = std::fs::read_to_string(&canonical)
            .map_err(|e| ParishError::Config(format!("read {}: {e}", canonical.display())))?;
        let provider: parish_config::ProviderMod = toml::from_str(&raw)
            .map_err(|e| ParishError::Config(format!("parse {}: {e}", canonical.display())))?;
        if !seen_ids.insert(provider.id.clone()) {
            return Err(ParishError::Config(format!(
                "mod at {} declares provider id '{}' more than once",
                mod_dir.display(),
                provider.id
            )));
        }
        out.push(provider);
    }

    Ok(out)
}

/// Resolves the `mods/` directory.
///
/// Resolution order:
/// 1. `PARISH_MODS_DIR` environment variable — explicit operator override.
/// 2. Walks up from the current working directory searching for a `mods/`
///    directory.
///
/// # Rule 9 warning
///
/// AGENTS.md rule 9 forbids resolving runtime paths from the cwd. The
/// cwd-walk below is a **development fallback only**. Production and packaged
/// builds must set `PARISH_MODS_DIR` so this walk is never reached.
///
/// See [`LocalDiskModSource::with_root`] for the explicit-path API.
pub(crate) fn find_mods_root() -> Option<PathBuf> {
    if let Some(explicit) = std::env::var_os("PARISH_MODS_DIR") {
        let p = PathBuf::from(explicit);
        if p.is_dir() {
            return Some(p);
        }
        // Misconfigured override — fall through to the cwd-walk so dev
        // environments aren't broken by a stale env var, but log it.
        tracing::warn!(
            path = %p.display(),
            "PARISH_MODS_DIR is set but does not point to a directory; falling back to cwd-walk"
        );
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("mods");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Walk up from the current working directory looking for the active
/// setting mod (rundale today). Backwards-compatible shim — prefer
/// [`discover_mods`] when you need the full mod list.
///
/// Returns the mod directory path (not the `mod.toml` path) if found.
pub fn find_default_mod() -> Option<PathBuf> {
    discover_mods().ok().map(|d| d.setting)
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

[theme.palette]
accent = "#aabbcc"
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
    fn test_mod_meta_app_name_falls_back_to_name() {
        let meta = ModMeta {
            name: "Rundale".to_string(),
            title: None,
            id: "rundale".to_string(),
            save_root: None,
            version: "0.1".to_string(),
            description: String::new(),
            kind: ModKind::default(),
            dependencies: vec![],
            optional_dependencies: vec![],
            conflicts: vec![],
        };
        assert_eq!(meta.app_name(), "Rundale");
    }

    #[test]
    fn test_mod_meta_app_name_treats_blank_save_root_as_unset() {
        let meta = ModMeta {
            name: "Rundale".to_string(),
            title: None,
            id: "rundale".to_string(),
            save_root: Some("   ".to_string()),
            version: "0.1".to_string(),
            description: String::new(),
            kind: ModKind::default(),
            dependencies: vec![],
            optional_dependencies: vec![],
            conflicts: vec![],
        };
        // Whitespace-only save_root must fall back to `name`, not collapse
        // saves into the bare user-data root.
        assert_eq!(meta.app_name(), "Rundale");
    }

    #[test]
    fn test_sanitize_app_name_strips_traversal_and_separators() {
        // Basename extraction handles "../etc" and absolute paths.
        assert_eq!(sanitize_app_name("Rundale"), Some("Rundale".to_string()));
        assert_eq!(
            sanitize_app_name("  Rundale  "),
            Some("Rundale".to_string())
        );
        assert_eq!(sanitize_app_name("../etc"), Some("etc".to_string()));
        assert_eq!(
            sanitize_app_name("/abs/Rundale"),
            Some("Rundale".to_string())
        );
        // Pure traversal / dot / empty all reject.
        assert_eq!(sanitize_app_name(".."), None);
        assert_eq!(sanitize_app_name("."), None);
        assert_eq!(sanitize_app_name(""), None);
        assert_eq!(sanitize_app_name("   "), None);
        // Trailing separator: file_name on "foo/" returns Some("foo").
        assert_eq!(sanitize_app_name("Rundale/"), Some("Rundale".to_string()));
    }

    #[test]
    fn test_app_name_from_mod_engine_fallback_when_none() {
        let resolved = app_name_from_mod(&None);
        assert_eq!(resolved, parish_persistence::paths::DEFAULT_APP_NAME);
    }

    #[test]
    fn test_app_name_from_mod_falls_through_to_name_when_save_root_invalid() {
        // Build a real GameMod via the test fixture, then mutate save_root
        // to an invalid value to verify the resolver falls back to `name`
        // rather than to the engine default.
        let tmp = create_test_mod();
        let mut gm = GameMod::load(tmp.path()).unwrap();
        gm.manifest.meta.save_root = Some("..".to_string());
        let resolved = app_name_from_mod(&Some(gm));
        // Invalid save_root rejected, falls back to sanitised `name`
        // (the fixture's `name = "Test Mod"` → basename "Test Mod").
        assert_eq!(resolved, "Test Mod");
    }

    #[test]
    fn test_mod_meta_app_name_uses_save_root_when_set() {
        let meta = ModMeta {
            name: "Rundale".to_string(),
            title: None,
            id: "rundale".to_string(),
            save_root: Some("Rundale-Beta".to_string()),
            version: "0.1".to_string(),
            description: String::new(),
            kind: ModKind::default(),
            dependencies: vec![],
            optional_dependencies: vec![],
            conflicts: vec![],
        };
        assert_eq!(meta.app_name(), "Rundale-Beta");
    }

    #[test]
    fn test_load_mod_from_directory() {
        let tmp = create_test_mod();
        let gm = GameMod::load(tmp.path()).expect("should load test mod");
        assert_eq!(gm.manifest.meta.id, "test-mod");
        assert_eq!(gm.manifest.meta.name, "Test Mod");
        // Schema-additive: test fixture omits `save_root`, so it must
        // round-trip as None and `app_name()` falls back to `name`.
        assert!(gm.manifest.meta.save_root.is_none());
        assert_eq!(gm.manifest.meta.app_name(), "Test Mod");
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
    fn test_mod_icon_paths_resolve_under_assets() {
        let tmp = create_test_mod();
        let icons = tmp.path().join("assets/icons/app");
        fs::create_dir_all(&icons).unwrap();
        fs::write(icons.join("icon-512.png"), b"icon").unwrap();
        fs::write(icons.join("favicon-32.png"), b"favicon").unwrap();
        fs::write(
            tmp.path().join("ui.toml"),
            r##"
[branding]
app_icon = "assets/icons/app/icon-512.png"
favicon = "assets/icons/app/favicon-32.png"

[sidebar]
hints_label = "Focail"

[theme.palette]
accent = "#aabbcc"
"##,
        )
        .unwrap();

        let gm = GameMod::load(tmp.path()).unwrap();
        assert!(gm.app_icon_path().unwrap().ends_with("icon-512.png"));
        assert!(gm.favicon_path().unwrap().ends_with("favicon-32.png"));
    }

    #[test]
    fn test_mod_icon_paths_must_stay_under_assets() {
        let tmp = create_test_mod();
        fs::write(
            tmp.path().join("ui.toml"),
            r##"
[branding]
app_icon = "../icon.png"
"##,
        )
        .unwrap();

        let err = GameMod::load(tmp.path()).expect_err("escaping icon path should fail");
        let msg = format!("{err:?}");
        assert!(msg.contains("ui.branding.app_icon"));
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

    /// #741 — a malicious mod.toml with a `..` path must be rejected, not
    /// allowed to read files outside the mod directory.
    #[test]
    fn test_load_rejects_directory_traversal_in_manifest() {
        let outer = TempDir::new().unwrap();
        // Sensitive file outside the mod directory.
        fs::write(outer.path().join("secret.txt"), "TOP SECRET").unwrap();

        // Build a real mod inside outer/, then rewrite mod.toml so the
        // `world` path traverses out to secret.txt.
        let mod_dir = outer.path().join("mod");
        fs::create_dir_all(&mod_dir).unwrap();

        // Re-use the canonical fixture's contents but point a manifest path at a traversal.
        let inner_tmp = create_test_mod();
        for entry in fs::read_dir(inner_tmp.path()).unwrap() {
            let entry = entry.unwrap();
            let dest = mod_dir.join(entry.file_name());
            if entry.path().is_dir() {
                fs::create_dir_all(&dest).unwrap();
                for sub in fs::read_dir(entry.path()).unwrap() {
                    let sub = sub.unwrap();
                    fs::copy(sub.path(), dest.join(sub.file_name())).unwrap();
                }
            } else {
                fs::copy(entry.path(), &dest).unwrap();
            }
        }

        // Overwrite mod.toml with a malicious anachronisms path. (anachronisms
        // is read via read_text during load, unlike world/npcs which are
        // resolved lazily by world_path()/npcs_path().)
        let manifest = fs::read_to_string(mod_dir.join("mod.toml")).unwrap();
        let evil = manifest.replace(
            "anachronisms = \"anachronisms.json\"",
            "anachronisms = \"../secret.txt\"",
        );
        fs::write(mod_dir.join("mod.toml"), evil).unwrap();

        let result = GameMod::load(&mod_dir);
        assert!(result.is_err(), "expected traversal to be rejected");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("escapes mod directory"),
            "expected escape error, got: {err}"
        );
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
        assert_eq!(ui.theme.palette.bg, "#fafad8");
        assert_eq!(ui.theme.palette.accent, "#b08531");
    }

    #[test]
    fn test_ui_config_custom() {
        let toml_str = r##"
[sidebar]
hints_label = "Custom"

[theme.palette]
accent = "#ff0000"
bg = "#010203"
"##;
        let ui: UiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(ui.sidebar.hints_label, "Custom");
        assert_eq!(ui.theme.palette.bg, "#010203");
        assert_eq!(ui.theme.palette.accent, "#ff0000");
        assert_eq!(ui.theme.palette.fg, "#31240f");
    }

    #[test]
    fn test_ui_config_legacy_default_accent() {
        let toml_str = r##"
[theme]
default_accent = "#112233"
"##;
        let ui: UiConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(ui.theme.resolved_palette().accent, "#112233");
        assert_eq!(ui.theme.resolved_palette().bg, "#fafad8");
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
                    {"word": "Kilteevan", "pronunciation": "kill-TEE-van", "meaning": "Cill Taobháin — Teevan's Church", "matches": ["Kilteevan"]}
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
            // The rundale mod should have pronunciation data
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

    fn write_manifest(dir: &Path, id: &str, kind: Option<&str>) {
        fs::create_dir_all(dir).unwrap();
        let kind_line = kind
            .map(|k| format!("kind = \"{k}\"\n"))
            .unwrap_or_default();
        let body = format!(
            "[mod]\nname = \"{id}\"\nid = \"{id}\"\nversion = \"0.0.0\"\ndescription = \"x\"\n{kind_line}"
        );
        fs::write(dir.join("mod.toml"), body).unwrap();
    }

    #[test]
    fn discover_mods_finds_setting_and_auxiliary_in_lex_order() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_manifest(&mods.join("rundale"), "rundale", Some("setting"));
        write_manifest(&mods.join("solarized"), "solarized", Some("asset"));
        write_manifest(&mods.join("aurora"), "aurora", Some("asset"));

        let discovered = discover_mods_in(&mods).expect("discovery succeeds");
        assert!(discovered.setting.ends_with("rundale"));
        let aux_ids: Vec<_> = discovered.auxiliary.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(aux_ids, vec!["aurora", "solarized"]);
        assert_eq!(discovered.auxiliary[0].kind, ModKind::Asset);
    }

    #[test]
    fn discover_mods_treats_missing_kind_as_setting() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_manifest(&mods.join("rundale"), "rundale", None);
        let discovered = discover_mods_in(&mods).expect("discovery succeeds");
        assert!(discovered.setting.ends_with("rundale"));
        assert!(discovered.auxiliary.is_empty());
    }

    #[test]
    fn discover_mods_rejects_two_settings() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_manifest(&mods.join("rundale"), "rundale", Some("setting"));
        write_manifest(&mods.join("hokkaido"), "hokkaido", Some("setting"));
        let err = discover_mods_in(&mods).expect_err("two settings is a hard error");
        let msg = format!("{err:?}");
        assert!(msg.contains("Multiple setting mods"));
    }

    #[test]
    fn discover_mods_requires_a_setting() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_manifest(&mods.join("solarized"), "solarized", Some("asset"));
        let err = discover_mods_in(&mods).expect_err("no setting mod is fatal");
        let msg = format!("{err:?}");
        assert!(msg.contains("No setting mod"));
    }

    #[test]
    fn discover_mods_classifies_providers_kind() {
        let tmp = TempDir::new().unwrap();
        let mods = tmp.path().join("mods");
        write_manifest(&mods.join("rundale"), "rundale", Some("setting"));
        write_manifest(&mods.join("anthropic"), "anthropic", Some("providers"));
        let discovered = discover_mods_in(&mods).expect("discovery succeeds");
        assert_eq!(discovered.auxiliary.len(), 1);
        assert_eq!(discovered.auxiliary[0].kind, ModKind::Providers);
        assert_eq!(discovered.auxiliary[0].id, "anthropic");
    }

    fn write_provider_toml(dir: &Path, id: &str) {
        let body = format!(
            r#"
id = "{id}"
display_name = "{id} Provider"
kind = "openai-compat"
default_base_url = "https://api.{id}.example/v1"
requires_api_key = true
requires_model = true
api_key_env_var = "{KEY}"
featured = false
"#,
            id = id,
            KEY = id.to_uppercase().replace('-', "_") + "_API_KEY",
        );
        fs::write(dir.join(format!("{id}.toml")), body).unwrap();
    }

    #[test]
    fn load_providers_from_mod_parses_multiple_tomls_in_lex_order() {
        let tmp = TempDir::new().unwrap();
        let mod_dir = tmp.path().join("multi-providers");
        fs::create_dir_all(mod_dir.join("providers")).unwrap();
        // Write three files in an unsorted order. The loader must return them sorted.
        write_provider_toml(&mod_dir.join("providers"), "zeta");
        write_provider_toml(&mod_dir.join("providers"), "alpha");
        write_provider_toml(&mod_dir.join("providers"), "mu");

        let providers = load_providers_from_mod(&mod_dir).expect("load succeeds");
        let ids: Vec<_> = providers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "mu", "zeta"]);
    }

    #[test]
    fn load_providers_from_mod_empty_when_directory_missing() {
        let tmp = TempDir::new().unwrap();
        let mod_dir = tmp.path().join("no-providers");
        fs::create_dir_all(&mod_dir).unwrap();
        let providers = load_providers_from_mod(&mod_dir).expect("load succeeds");
        assert!(providers.is_empty());
    }

    #[test]
    fn load_providers_from_mod_rejects_symlink_traversal() {
        // Skip on platforms without symlinks (Windows in CI). Unix-only.
        #[cfg(unix)]
        {
            let tmp = TempDir::new().unwrap();
            let outside = tmp.path().join("outside");
            fs::create_dir_all(&outside).unwrap();
            write_provider_toml(&outside, "evil");

            let mod_dir = tmp.path().join("mod-with-symlink");
            fs::create_dir_all(mod_dir.join("providers")).unwrap();
            // Symlink the evil TOML into the mod's providers/ dir.
            std::os::unix::fs::symlink(
                outside.join("evil.toml"),
                mod_dir.join("providers/evil.toml"),
            )
            .unwrap();

            let err = load_providers_from_mod(&mod_dir).expect_err("traversal must be rejected");
            let msg = format!("{err:?}");
            assert!(
                msg.contains("escapes mod directory"),
                "expected traversal error, got: {msg}"
            );
        }
    }

    #[test]
    fn load_providers_from_mod_rejects_duplicate_ids_within_one_mod() {
        let tmp = TempDir::new().unwrap();
        let mod_dir = tmp.path().join("dup-providers");
        fs::create_dir_all(mod_dir.join("providers")).unwrap();
        // Write two files containing the *same* id.
        let body = r#"
id = "duplicate"
display_name = "Duplicate"
kind = "openai-compat"
default_base_url = "https://api.example/v1"
requires_api_key = false
requires_model = true
featured = false
"#;
        fs::write(mod_dir.join("providers/a.toml"), body).unwrap();
        fs::write(mod_dir.join("providers/b.toml"), body).unwrap();
        let err =
            load_providers_from_mod(&mod_dir).expect_err("intra-mod duplicate must be rejected");
        let msg = format!("{err:?}");
        assert!(msg.contains("more than once"), "got: {msg}");
    }

    // ── SettingConfig language field deserialization tests ─────────────────────

    #[test]
    fn setting_config_with_both_languages() {
        let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "en-IE"
native_language = "ga-IE"
"#;
        let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
        assert_eq!(cfg.player_language, "en-IE");
        assert_eq!(cfg.native_language.as_deref(), Some("ga-IE"));
    }

    #[test]
    fn setting_config_defaults_player_language_to_en_when_omitted() {
        let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
"#;
        let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
        assert_eq!(
            cfg.player_language, "en",
            "player_language should default to \"en\" for backward-compat mods"
        );
        assert!(
            cfg.native_language.is_none(),
            "native_language should default to None"
        );
    }

    #[test]
    fn setting_config_with_only_player_language() {
        let toml_src = r#"
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "fr-FR"
"#;
        let cfg: SettingConfig = toml::from_str(toml_src).expect("should deserialize");
        assert_eq!(cfg.player_language, "fr-FR");
        assert!(
            cfg.native_language.is_none(),
            "native_language should be None when omitted"
        );
    }

    #[test]
    fn game_mod_accessors_expose_language_settings() {
        let tmp = create_test_mod();
        // Rewrite mod.toml to include language fields
        fs::write(
            tmp.path().join("mod.toml"),
            r#"
[mod]
name = "Lang Test Mod"
id = "lang-test"
version = "0.1.0"
description = "Language settings test."

[setting]
start_date = "1820-03-20T08:00:00Z"
start_location = 15
period_year = 1820
player_language = "en-IE"
native_language = "ga-IE"

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
        let gm = GameMod::load(tmp.path()).expect("should load mod with language settings");
        assert_eq!(gm.player_language(), "en-IE");
        assert_eq!(gm.native_language(), Some("ga-IE"));
    }
}
