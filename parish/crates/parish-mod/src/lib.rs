//! Game mod loader for the engine/game-data separation.
//!
//! A "mod" is a directory containing a `mod.toml` manifest plus data files
//! (world graph, NPCs, encounters, etc.). The engine loads a [`GameMod`] at
//! startup and uses it to access all game-specific content at runtime.
//!
//! This crate is the sole owner of the content-mod loader. `parish-core`
//! re-exports it as `parish_core::game_mod` so every historical consumer path
//! keeps compiling unchanged.

use std::path::{Path, PathBuf};

use parish_types::LanguageHint;
use parish_types::error::ParishError;
use parish_world::transport::TransportConfig;

mod assets;

pub(crate) mod discovery;

pub mod manifest;

#[cfg(test)]
mod tests;

pub mod types;

pub mod world;

// Re-export manifest types — all public
pub use manifest::*;

// Re-export runtime data types — all public
pub use types::*;

// Re-export world bridge
pub use world::world_state_from_mod;

// Re-export discovery items. `find_mods_root` is consumed by
// `parish_core::mod_source`, so it must be `pub` now that this loader lives in
// its own crate (it was `pub(crate)` while co-located with `parish-core`).
pub use discovery::find_mods_root;
pub use discovery::{
    DiscoveredMod, DiscoveredMods, discover_mods, discover_mods_in, find_default_mod,
};

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
    pub reactions: parish_npc::reactions::ReactionTemplates,
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
        assets::validate_optional_asset_ref(
            &mod_dir,
            "ui.branding.app_icon",
            ui.branding.app_icon.as_deref(),
        )?;
        assets::validate_optional_asset_ref(
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
            parish_npc::reactions::ReactionTemplates::default()
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
        rel.and_then(|path| assets::canonical_mod_asset_path(&self.mod_dir, path).ok())
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

// ---------------------------------------------------------------------------
// Provider catalog loading
// ---------------------------------------------------------------------------

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
