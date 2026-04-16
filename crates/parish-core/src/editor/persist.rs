//! Atomic persistence for editor-edited mod files.
//!
//! Every write goes through [`write_json_deterministic`] (deterministic JSON
//! plus atomic temp-and-rename) and is gated by [`validate_snapshot`] so
//! invalid data never reaches disk. Each write function takes a snapshot
//! plus the set of docs the caller wants to persist, validates the whole
//! snapshot, and either writes the requested subset or returns the error
//! report untouched.

use std::path::{Path, PathBuf};

use parish_npc::NpcFile;
use parish_types::ParishError;
use parish_world::graph::LocationData;

use super::format::write_json_deterministic;
use super::types::{EditorDoc, EditorModSnapshot, ValidationReport};
use super::validate::validate_snapshot;
use crate::game_mod::{AnachronismData, EncounterTable, FestivalDef, ModManifest};

/// Result of a save attempt.
///
/// The editor front-end uses this to decide whether to display a success
/// toast or a validation panel.
#[derive(Debug, Clone)]
pub enum SaveResult {
    /// Every requested doc was validated and written successfully.
    ///
    /// The contained report may still contain *warnings*; they do not
    /// block saving.
    Saved(ValidationReport),
    /// Validation found blocking errors; nothing was written.
    Blocked(ValidationReport),
}

impl SaveResult {
    /// Returns the inner report regardless of outcome.
    pub fn report(&self) -> &ValidationReport {
        match self {
            SaveResult::Saved(r) | SaveResult::Blocked(r) => r,
        }
    }

    /// Returns `true` if the save committed to disk.
    pub fn was_saved(&self) -> bool {
        matches!(self, SaveResult::Saved(_))
    }
}

/// Writes the requested subset of docs in `snapshot` back to disk.
///
/// Validation runs against the whole snapshot before any writes happen.
/// If validation finds any **error**-severity issues, nothing is written
/// and [`SaveResult::Blocked`] is returned with the full report. Warnings
/// do not block saving.
///
/// Writes are atomic per-file (temp + rename). If one file fails to write
/// after an earlier one succeeded, the earlier writes are **not** rolled
/// back — they are already committed to disk. This matches typical
/// filesystem editor semantics and avoids the complexity of a cross-file
/// transaction. Since validation runs first and the write step can only
/// fail on I/O errors, this trade-off is acceptable in practice.
pub fn save_mod(
    snapshot: &mut EditorModSnapshot,
    docs: &[EditorDoc],
) -> Result<SaveResult, ParishError> {
    validate_snapshot(snapshot);
    if snapshot.validation.has_errors() {
        return Ok(SaveResult::Blocked(snapshot.validation.clone()));
    }

    // Re-read the on-disk manifest so we can resolve file paths. We do not
    // edit the manifest itself in Phase 1 — the `EditorManifest` on the
    // snapshot is read-only metadata.
    let manifest = load_manifest(&snapshot.mod_path)?;

    for doc in docs {
        match doc {
            EditorDoc::Npcs => save_npcs(
                &snapshot.mod_path.join(&manifest.files.npcs),
                &snapshot.npcs,
            )?,
            EditorDoc::World => save_world(
                &snapshot.mod_path.join(&manifest.files.world),
                &snapshot.locations,
            )?,
            EditorDoc::Festivals => save_festivals(
                &snapshot.mod_path.join(&manifest.files.festivals),
                &snapshot.festivals,
            )?,
            EditorDoc::Encounters => save_encounters(
                &snapshot.mod_path.join(&manifest.files.encounters),
                &snapshot.encounters,
            )?,
            EditorDoc::Anachronisms => save_anachronisms(
                &snapshot.mod_path.join(&manifest.files.anachronisms),
                &snapshot.anachronisms,
            )?,
            EditorDoc::Manifest => {
                // Manifest editing is out of Phase 1 scope; silently skip
                // so callers can pass a "save all" set without errors.
            }
        }
    }

    Ok(SaveResult::Saved(snapshot.validation.clone()))
}

/// Writes the NPC file to disk deterministically.
pub fn save_npcs(path: &Path, npcs: &NpcFile) -> Result<(), ParishError> {
    write_json_deterministic(path, npcs)
}

/// Writes the world graph to disk deterministically.
///
/// Wraps the locations in the `{"locations": [...]}` envelope the game
/// loader expects.
pub fn save_world(path: &Path, locations: &[LocationData]) -> Result<(), ParishError> {
    #[derive(serde::Serialize)]
    struct WorldFile<'a> {
        locations: &'a [LocationData],
    }
    let file = WorldFile { locations };
    write_json_deterministic(path, &file)
}

/// Writes the festivals list to disk deterministically.
pub fn save_festivals(path: &Path, festivals: &[FestivalDef]) -> Result<(), ParishError> {
    write_json_deterministic(path, &festivals)
}

/// Writes the encounters table to disk deterministically.
pub fn save_encounters(path: &Path, encounters: &EncounterTable) -> Result<(), ParishError> {
    write_json_deterministic(path, encounters)
}

/// Writes the anachronism data to disk deterministically.
pub fn save_anachronisms(path: &Path, data: &AnachronismData) -> Result<(), ParishError> {
    write_json_deterministic(path, data)
}

fn load_manifest(mod_dir: &Path) -> Result<ModManifest, ParishError> {
    let path: PathBuf = mod_dir.join("mod.toml");
    let text = std::fs::read_to_string(&path)
        .map_err(|e| ParishError::Config(format!("failed to read {}: {}", path.display(), e)))?;
    toml::from_str(&text)
        .map_err(|e| ParishError::Config(format!("failed to parse mod.toml: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::super::mod_io::load_mod_snapshot;
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Copies the real `mods/rundale/` tree into a temporary directory so
    /// tests can write through `save_mod` without touching source files.
    fn rundale_tempdir() -> Option<(TempDir, PathBuf)> {
        let src = PathBuf::from("../../mods/rundale");
        if !src.exists() {
            return None;
        }
        let dir = TempDir::new().unwrap();
        let dst = dir.path().join("rundale");
        copy_dir_recursive(&src, &dst).unwrap();
        Some((dir, dst))
    }

    fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            let ty = entry.file_type()?;
            let dst_path = dst.join(entry.file_name());
            if ty.is_dir() {
                copy_dir_recursive(&entry.path(), &dst_path)?;
            } else {
                fs::copy(entry.path(), &dst_path)?;
            }
        }
        Ok(())
    }

    #[test]
    fn save_mod_round_trip_npcs_and_world_in_tempdir() {
        // Load rundale, save NPCs and world back to the tempdir copy,
        // re-load, assert the snapshot is structurally identical.
        let Some((_guard, root)) = rundale_tempdir() else {
            return;
        };
        let mut snapshot = load_mod_snapshot(&root).unwrap();
        let original_npc_count = snapshot.npcs.npcs.len();
        let original_loc_count = snapshot.locations.len();

        let result = save_mod(&mut snapshot, &[EditorDoc::Npcs, EditorDoc::World]).unwrap();
        assert!(result.was_saved(), "save_mod should succeed for clean data");

        let reloaded = load_mod_snapshot(&root).unwrap();
        assert_eq!(reloaded.npcs.npcs.len(), original_npc_count);
        assert_eq!(reloaded.locations.len(), original_loc_count);
        assert!(reloaded.validation.errors.is_empty());
    }

    #[test]
    fn save_mod_is_idempotent() {
        // Two consecutive saves produce byte-identical files.
        let Some((_guard, root)) = rundale_tempdir() else {
            return;
        };
        let mut snapshot = load_mod_snapshot(&root).unwrap();
        save_mod(&mut snapshot, &[EditorDoc::Npcs, EditorDoc::World]).unwrap();
        let npcs_first = fs::read(root.join("npcs.json")).unwrap();
        let world_first = fs::read(root.join("world.json")).unwrap();

        save_mod(&mut snapshot, &[EditorDoc::Npcs, EditorDoc::World]).unwrap();
        let npcs_second = fs::read(root.join("npcs.json")).unwrap();
        let world_second = fs::read(root.join("world.json")).unwrap();

        assert_eq!(
            npcs_first, npcs_second,
            "two saves must produce identical npcs.json"
        );
        assert_eq!(
            world_first, world_second,
            "two saves must produce identical world.json"
        );
    }

    /// **The critical acceptance test for the editor.**
    ///
    /// Copies the real `mods/rundale/` into a tempdir, loads it through the
    /// editor, saves every editable doc back out, and asserts that the
    /// resulting file bytes are **identical** to the source files.
    ///
    /// An empty diff here is equivalent to the manual "`git diff` must be
    /// empty after saving" acceptance check in `docs/design/designer-editor.md`.
    /// Any drift means the editor is silently corrupting source files and
    /// the feature should not ship.
    #[test]
    fn save_mod_byte_identical_to_source() {
        let Some((_guard, root)) = rundale_tempdir() else {
            return;
        };
        let mut snapshot = load_mod_snapshot(&root).unwrap();
        save_mod(
            &mut snapshot,
            &[
                EditorDoc::Npcs,
                EditorDoc::World,
                EditorDoc::Festivals,
                EditorDoc::Encounters,
                EditorDoc::Anachronisms,
            ],
        )
        .unwrap();

        let src = PathBuf::from("../../mods/rundale");
        for file in [
            "npcs.json",
            "world.json",
            "festivals.json",
            "encounters.json",
            "anachronisms.json",
        ] {
            let before = fs::read(src.join(file)).unwrap();
            let after = fs::read(root.join(file)).unwrap();
            assert_eq!(
                before, after,
                "{file} must be byte-identical after editor round-trip (drift = silent corruption)"
            );
        }
    }

    #[test]
    fn save_mod_blocks_on_validation_errors() {
        // Break a relationship target and verify save_mod refuses.
        let Some((_guard, root)) = rundale_tempdir() else {
            return;
        };
        let mut snapshot = load_mod_snapshot(&root).unwrap();
        assert!(!snapshot.npcs.npcs.is_empty());
        assert!(
            !snapshot.npcs.npcs[0].relationships.is_empty(),
            "fixture must have at least one relationship"
        );
        // Point the first relationship at a nonexistent NPC.
        snapshot.npcs.npcs[0].relationships[0].target_id = 99_999;

        let result = save_mod(&mut snapshot, &[EditorDoc::Npcs]).unwrap();
        assert!(!result.was_saved(), "save_mod must refuse invalid data");
        let report = result.report();
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.field_path.contains("target_id")),
            "report should flag the broken target_id, got: {:?}",
            report.errors
        );
    }
}
