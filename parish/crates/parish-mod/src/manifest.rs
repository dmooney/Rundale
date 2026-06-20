//! Manifest types parsed from `mod.toml`.

use serde::{Deserialize, Serialize};

/// Highest `mod.toml` schema version this engine understands.
///
/// Bump when the manifest schema changes incompatibly; loaders reject
/// manifests declaring a higher version so an old engine fails fast with a
/// clear message instead of misreading a future-format mod (#1366).
pub const SUPPORTED_MOD_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    1
}

/// Top-level manifest parsed from `mod.toml`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModManifest {
    /// Manifest schema version. Defaults to 1 when absent so every
    /// pre-versioning mod keeps loading unchanged.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
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
    /// Mod kind. Defaults to [`ModKind::Base`] when omitted, so a manifest
    /// without `kind = "..."` loads as the primary base mod.
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
/// Implemented today: [`ModKind::Base`] (the primary game world mod),
/// [`ModKind::Asset`] (additive registries such as themes), and
/// [`ModKind::Providers`] (LLM provider catalog mods loaded into
/// `ProviderRegistry` at startup). The remaining variants are reserved so
/// manifests can be authored against the final schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModKind {
    /// Owns the world graph, NPC roster, prompts, palette, and calendar
    /// baseline. Exactly one base mod must be active for the engine to
    /// have something to render.
    #[default]
    Base,
    /// Pure additive registries (themes, sounds, fonts). No gameplay content.
    Asset,
    /// LLM provider catalog. Carries one or more `providers/<id>.toml` files
    /// that merge into `ProviderRegistry` at startup. Used to ship cloud
    /// providers (anthropic, openai, openrouter, ...) as runtime-loadable
    /// mods rather than compile-time-embedded data.
    Providers,
    /// Adds new gameplay entries (extra NPCs, locations, festivals).
    Content,
    /// Mutates entries already in the active base mod.
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
    /// Scene-diorama index JSON file (optional; mods without scenes remain text-first).
    #[serde(default)]
    pub scenes: Option<String>,
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

impl ModManifest {
    /// Parses a `mod.toml` body and validates its `schema_version`.
    ///
    /// All manifest loaders go through this instead of a bare
    /// `toml::from_str` so a mod authored against a future schema is
    /// rejected with an actionable message rather than half-parsed.
    /// Callers add file-path context to the returned message.
    pub fn from_toml_str(text: &str) -> Result<Self, String> {
        let manifest: ModManifest = toml::from_str(text).map_err(|e| e.to_string())?;
        if manifest.schema_version > SUPPORTED_MOD_SCHEMA_VERSION {
            return Err(format!(
                "mod declares schema_version = {} but this engine supports up to {}; \
                 upgrade the engine or use an older release of the mod",
                manifest.schema_version, SUPPORTED_MOD_SCHEMA_VERSION
            ));
        }
        Ok(manifest)
    }
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
    /// result into a directory name must use [`super::app_name_from_mod`] (or apply
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
