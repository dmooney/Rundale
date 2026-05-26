//! Mod discovery — scanning `mods/` for mod directories, classifying by kind,
//! and resolving the active base mod.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::ParishError;
use super::manifest::{ModKind, ModMeta};

/// All mods discovered under a `mods/` root.
///
/// `base` is the unique [`ModKind::Base`] mod (e.g. rundale, testbed). Every
/// other mod is recorded in `auxiliary` in lexicographic-by-directory-name
/// order so registry merging is deterministic across machines and tests.
#[derive(Debug, Clone)]
pub struct DiscoveredMods {
    pub base: PathBuf,
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

/// Selects among multiple base mods when `mods/mod-list.toml` is present.
///
/// Analogous to Factorio's `mod-list.json` — lets multiple base mods coexist
/// in the `mods/` directory, with one declared active at a time. When absent,
/// the engine reverts to single-base-mod enforcement.
#[derive(serde::Deserialize, Default)]
struct ModList {
    #[serde(default)]
    active_setting: Option<String>,
}

/// Walk up from the current working directory looking for a `mods/`
/// directory; once found, enumerate every `mods/*/mod.toml` and classify
/// each by its declared `kind`.
///
/// When `mods/mod-list.toml` specifies `active_base`, multiple
/// `kind = "base"` mods may coexist and the named one is selected.
/// Without that file, exactly one base mod is required.
pub fn discover_mods() -> Result<DiscoveredMods, ParishError> {
    let mods_root = find_mods_root()
        .ok_or_else(|| ParishError::Config("No `mods/` directory found".to_string()))?;
    discover_mods_in(&mods_root)
}

/// Variant of [`discover_mods`] that scans an explicit `mods/` root. Used by
/// tests; production callers want [`discover_mods`].
pub fn discover_mods_in(mods_root: &Path) -> Result<DiscoveredMods, ParishError> {
    let mod_list: ModList = std::fs::read_to_string(mods_root.join("mod-list.toml"))
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();

    let mut entries: Vec<_> = std::fs::read_dir(mods_root)
        .map_err(|e| ParishError::Config(format!("read_dir({}): {e}", mods_root.display())))?
        .filter_map(Result::ok)
        .filter(|e| e.path().join("mod.toml").is_file())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    let mut base: Option<PathBuf> = None;
    let mut base_id: Option<String> = None;
    // Candidates collected when mod-list.toml declares active_base.
    let mut base_candidates: Vec<(PathBuf, String)> = Vec::new();
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
            ModKind::Base => {
                if mod_list.active_setting.is_some() {
                    base_candidates.push((dir, meta.id));
                } else if let Some(prev) = &base_id {
                    return Err(ParishError::Config(format!(
                        "Multiple base mods found: '{prev}' and '{}'. \
                        Add mods/mod-list.toml with active_setting = \"<id>\" to select one.",
                        meta.id
                    )));
                } else {
                    base = Some(dir.clone());
                    base_id = Some(meta.id);
                }
            }
            other => auxiliary.push(DiscoveredMod {
                path: dir,
                kind: other,
                id: meta.id,
            }),
        }
    }

    if let Some(ref target) = mod_list.active_setting {
        let chosen = base_candidates
            .into_iter()
            .find(|(_, id)| id == target)
            .ok_or_else(|| {
                ParishError::Config(format!(
                    "mod-list.toml specifies active_setting = \"{target}\" \
                    but no base mod with that id was found in {}.",
                    mods_root.display()
                ))
            })?;
        base = Some(chosen.0);
    }

    let base = base.ok_or_else(|| {
        ParishError::Config(
            "No base mod found (expected exactly one mod with kind = \"base\").".to_string(),
        )
    })?;
    Ok(DiscoveredMods { base, auxiliary })
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
    discover_mods().ok().map(|d| d.base)
}
