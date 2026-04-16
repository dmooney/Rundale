//! Granular mod loading for the editor.
//!
//! Mirrors [`crate::game_mod::GameMod::load`] but loads each file
//! independently so a broken `festivals.json` does not hide a working
//! `npcs.json` from the designer. Every parse error becomes a
//! [`ValidationIssue`] rather than a hard stop.

use std::fs;
use std::path::Path;

use parish_npc::NpcFile;
use parish_types::ParishError;
use parish_world::graph::LocationData;

use super::types::{
    EditorDoc, EditorManifest, EditorModSnapshot, ModSummary, ValidationCategory, ValidationIssue,
    ValidationReport, ValidationSeverity,
};
use super::validate;
use crate::game_mod::{AnachronismData, EncounterTable, FestivalDef, ModManifest};

/// Scans `mods_root` for subdirectories containing a `mod.toml` manifest.
///
/// Returns a [`ModSummary`] per valid mod. Mods that fail to parse are
/// silently skipped — use [`load_mod_snapshot`] to inspect them properly.
pub fn list_mods(mods_root: &Path) -> Result<Vec<ModSummary>, ParishError> {
    let mut summaries = Vec::new();

    let entries = fs::read_dir(mods_root).map_err(|e| {
        ParishError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "failed to read mods directory {}: {}",
                mods_root.display(),
                e
            ),
        ))
    })?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("mod.toml");
        if !manifest_path.exists() {
            continue;
        }
        let Ok(text) = fs::read_to_string(&manifest_path) else {
            continue;
        };
        let Ok(manifest) = toml::from_str::<ModManifest>(&text) else {
            continue;
        };
        summaries.push(ModSummary {
            id: manifest.meta.id.clone(),
            name: manifest.meta.name.clone(),
            title: manifest.meta.title.clone(),
            version: manifest.meta.version.clone(),
            description: manifest.meta.description.clone(),
            path: path.clone(),
        });
    }

    summaries.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(summaries)
}

/// Loads a mod snapshot from `mod_dir` file-by-file.
///
/// The manifest must parse (that's a hard prerequisite for knowing where the
/// other files are). Everything else is best-effort: parse failures become
/// [`ValidationIssue`] entries in the returned snapshot's `validation`
/// report, with empty defaults substituted for the broken data so the
/// editor can still display the rest of the mod.
pub fn load_mod_snapshot(mod_dir: &Path) -> Result<EditorModSnapshot, ParishError> {
    let mod_dir = mod_dir.canonicalize().map_err(|e| {
        ParishError::Config(format!(
            "mod directory not found: {} ({})",
            mod_dir.display(),
            e
        ))
    })?;

    let manifest_path = mod_dir.join("mod.toml");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|e| {
        ParishError::Config(format!("failed to read {}: {}", manifest_path.display(), e))
    })?;
    let manifest: ModManifest = toml::from_str(&manifest_text)
        .map_err(|e| ParishError::Config(format!("failed to parse mod.toml: {}", e)))?;

    let mut report = ValidationReport::default();

    let npcs = load_json_or_default::<NpcFile>(
        &mod_dir.join(&manifest.files.npcs),
        EditorDoc::Npcs,
        &mut report,
        || NpcFile { npcs: Vec::new() },
    );

    let locations = load_json_or_default::<WorldFile>(
        &mod_dir.join(&manifest.files.world),
        EditorDoc::World,
        &mut report,
        || WorldFile {
            locations: Vec::new(),
        },
    )
    .locations;

    let festivals = load_json_or_default::<Vec<FestivalDef>>(
        &mod_dir.join(&manifest.files.festivals),
        EditorDoc::Festivals,
        &mut report,
        Vec::new,
    );

    let encounters = load_json_or_default::<EncounterTable>(
        &mod_dir.join(&manifest.files.encounters),
        EditorDoc::Encounters,
        &mut report,
        || EncounterTable {
            by_time: Default::default(),
        },
    );

    let anachronisms = load_json_or_default::<AnachronismData>(
        &mod_dir.join(&manifest.files.anachronisms),
        EditorDoc::Anachronisms,
        &mut report,
        || AnachronismData {
            context_alert_prefix: String::new(),
            context_alert_suffix: String::new(),
            terms: Vec::new(),
        },
    );

    let mut snapshot = EditorModSnapshot {
        mod_path: mod_dir,
        manifest: EditorManifest::from(&manifest),
        npcs,
        locations,
        festivals,
        encounters,
        anachronisms,
        validation: report,
    };

    // Cross-reference validation (orphans, relationship targets, etc.) runs
    // on top of whatever parsed successfully. Its issues accumulate onto the
    // existing report.
    validate::validate_snapshot(&mut snapshot);

    Ok(snapshot)
}

/// Helper: read a path and deserialize it into `T`. On any failure, push a
/// parse error onto `report` and return the fallback value.
fn load_json_or_default<T>(
    path: &Path,
    doc: EditorDoc,
    report: &mut ValidationReport,
    fallback: impl FnOnce() -> T,
) -> T
where
    T: serde::de::DeserializeOwned,
{
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            report.push(ValidationIssue {
                category: ValidationCategory::Parse,
                severity: ValidationSeverity::Error,
                doc,
                field_path: String::new(),
                message: format!("failed to read {}: {}", path.display(), e),
                context: None,
            });
            return fallback();
        }
    };
    match serde_json::from_str::<T>(&text) {
        Ok(v) => v,
        Err(e) => {
            report.push(ValidationIssue {
                category: ValidationCategory::Parse,
                severity: ValidationSeverity::Error,
                doc,
                field_path: String::new(),
                message: format!("failed to parse {}: {}", path.display(), e),
                context: None,
            });
            fallback()
        }
    }
}

/// Local JSON wrapper for `world.json` (which uses a top-level
/// `{"locations": [...]}` envelope).
#[derive(serde::Serialize, serde::Deserialize)]
struct WorldFile {
    locations: Vec<LocationData>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn list_mods_includes_rundale() {
        let root = PathBuf::from("../../mods");
        if !root.exists() {
            return;
        }
        let mods = list_mods(&root).unwrap();
        assert!(
            mods.iter().any(|m| m.id == "rundale"),
            "expected to find rundale mod in {:?}",
            mods.iter().map(|m| &m.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn load_mod_snapshot_rundale() {
        let root = PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }
        let snapshot = load_mod_snapshot(&root).unwrap();
        assert_eq!(snapshot.manifest.id, "rundale");
        assert!(!snapshot.npcs.npcs.is_empty(), "rundale should have NPCs");
        assert!(
            !snapshot.locations.is_empty(),
            "rundale should have locations"
        );
        assert!(
            !snapshot.festivals.is_empty(),
            "rundale should have festivals"
        );
    }
}
