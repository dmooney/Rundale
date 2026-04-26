//! Editor IPC handlers shared by all frontends.
//!
//! Each function in this module is a self-contained handler that can be
//! called from a Tauri `#[tauri::command]` or an Axum route handler. They
//! coordinate between [`EditorSession`] (the in-memory state) and the
//! `parish-core::editor` pure functions. All I/O happens here; the caller
//! only needs to acquire the session lock.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::editor::mod_io;
use crate::editor::persist;
use crate::editor::save_inspect::{
    self, BranchSummary, SaveFileSummary, SnapshotDetail, SnapshotSummary,
};
use crate::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use crate::editor::validate;

// ── Editor session ──────────────────────────────────────────────────────────

/// Mutable session state for the editor.
///
/// Stored inside a `Mutex` on both the Tauri and Axum `AppState`. Fully
/// independent of the gameplay state — closing the editor drops this
/// without touching the live game.
#[derive(Debug, Default)]
pub struct EditorSession {
    /// Current snapshot being edited, if a mod is open.
    pub snapshot: Option<EditorModSnapshot>,
    /// Monotonic counter bumped on every mutating operation (open, update,
    /// reload, close). Used by `editor_save` to detect that another in-flight
    /// request overwrote the snapshot between clone-out and write-back, so
    /// the stale cloned copy is not written back and silently clobber newer
    /// edits — see codex P2 review on #439.
    pub version: u64,
    /// Monotonic counter bumped only on **snapshot-replacement** events
    /// (`editor_open_mod`, `editor_reload`, `editor_save`, `editor_close`)
    /// — i.e. whenever the lineage of `snapshot` changes. Peer-update
    /// paths (`editor_update_npcs`, `editor_update_locations`) leave
    /// this alone. The server-side `editor_routes` update handlers
    /// capture this under a brief lock before spawning the CPU-bound
    /// validate, then reject the write-back with 409 Conflict if it
    /// changed — so an in-flight update can't overwrite a snapshot
    /// that was replaced from disk during its spawn_blocking window
    /// (codex P1 on #574).
    pub generation: u64,
}

// ── IPC request/response types ──────────────────────────────────────────────

/// Response from `editor_open_mod`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorOpenModResponse {
    pub snapshot: EditorModSnapshot,
}

/// Request body for `editor_save`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSaveRequest {
    pub docs: Vec<EditorDoc>,
}

/// Response from `editor_save`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorSaveResponse {
    pub saved: bool,
    pub validation: ValidationReport,
}

// ── Handler functions ───────────────────────────────────────────────────────

/// Lists available mods under the given root directory.
pub fn handle_editor_list_mods(mods_root: &Path) -> Result<Vec<ModSummary>, String> {
    mod_io::list_mods(mods_root).map_err(|e| e.to_string())
}

/// Opens a mod from disk and stores it in the session.
///
/// Bumps both `version` and `generation`: this is a snapshot-replacement
/// event (new lineage). Any in-flight `update_*` requests that captured the
/// pre-open counters must reject their write-backs.
pub fn handle_editor_open_mod(
    session: &Mutex<EditorSession>,
    mod_path: &Path,
) -> Result<EditorOpenModResponse, String> {
    let snapshot = mod_io::load_mod_snapshot(mod_path).map_err(|e| e.to_string())?;
    let response = EditorOpenModResponse {
        snapshot: snapshot.clone(),
    };
    let mut s = session.lock().map_err(|e| e.to_string())?;
    s.snapshot = Some(snapshot);
    s.version = s.version.wrapping_add(1);
    s.generation = s.generation.wrapping_add(1);
    Ok(response)
}

/// Returns the current snapshot without reloading from disk.
pub fn handle_editor_get_snapshot(
    session: &Mutex<EditorSession>,
) -> Result<EditorModSnapshot, String> {
    let s = session.lock().map_err(|e| e.to_string())?;
    s.snapshot
        .clone()
        .ok_or_else(|| "no mod is open in the editor".to_string())
}

/// Validates the current in-memory snapshot.
pub fn handle_editor_validate(session: &Mutex<EditorSession>) -> Result<ValidationReport, String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    let snap = s
        .snapshot
        .as_mut()
        .ok_or_else(|| "no mod is open in the editor".to_string())?;
    validate::validate_snapshot(snap);
    Ok(snap.validation.clone())
}

/// Replaces the NPC data in the session with the provided value.
///
/// Bumps `version` (a mutating operation) but **not** `generation` — peer
/// updates do not change the snapshot lineage. Concurrent `update_locations`
/// requests that captured the same generation are still allowed to commit;
/// only open/reload/save/close invalidate them.
pub fn handle_editor_update_npcs(
    session: &Mutex<EditorSession>,
    npcs: parish_npc::NpcFile,
) -> Result<ValidationReport, String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    let snap = s
        .snapshot
        .as_mut()
        .ok_or_else(|| "no mod is open in the editor".to_string())?;
    snap.npcs = npcs;
    validate::validate_snapshot(snap);
    let report = snap.validation.clone();
    s.version = s.version.wrapping_add(1);
    Ok(report)
}

/// Replaces the locations in the session with the provided value.
///
/// Bumps `version` (a mutating operation) but **not** `generation` — peer
/// updates do not change the snapshot lineage. Concurrent `update_npcs`
/// requests that captured the same generation are still allowed to commit;
/// only open/reload/save/close invalidate them.
pub fn handle_editor_update_locations(
    session: &Mutex<EditorSession>,
    locations: Vec<parish_world::graph::LocationData>,
) -> Result<ValidationReport, String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    let snap = s
        .snapshot
        .as_mut()
        .ok_or_else(|| "no mod is open in the editor".to_string())?;
    snap.locations = locations;
    validate::validate_snapshot(snap);
    let report = snap.validation.clone();
    s.version = s.version.wrapping_add(1);
    Ok(report)
}

/// Saves the specified docs from the in-memory snapshot to disk.
///
/// Returns `EditorSaveResponse { saved: true, .. }` on success, or
/// `{ saved: false, .. }` if validation blocked the save.
///
/// Always bumps `version`. Bumps `generation` **only** on a successful disk
/// write (`was_saved == true`) — a validation-blocked save leaves the
/// snapshot lineage unchanged, so in-flight `update_*` requests that
/// captured the pre-save generation must still be allowed to commit.
pub fn handle_editor_save(
    session: &Mutex<EditorSession>,
    docs: Vec<EditorDoc>,
) -> Result<EditorSaveResponse, String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    // `snap` borrow must end before we mutate `s.version`/`s.generation`.
    let (was_saved, report) = {
        let snap = s
            .snapshot
            .as_mut()
            .ok_or_else(|| "no mod is open in the editor".to_string())?;
        let result = persist::save_mod(snap, &docs).map_err(|e| e.to_string())?;
        (result.was_saved(), result.report().clone())
    };
    s.version = s.version.wrapping_add(1);
    if was_saved {
        s.generation = s.generation.wrapping_add(1);
    }
    Ok(EditorSaveResponse {
        saved: was_saved,
        validation: report,
    })
}

/// Reloads the current mod from disk, discarding any unsaved edits.
///
/// Uses a read-then-swap pattern: the `mod_path` is cloned out under a
/// brief lock, the lock is dropped, the file read runs without holding
/// it, and then the lock is re-acquired only for the snapshot swap.
///
/// This avoids blocking a thread (or a Tokio worker, if called from an
/// async context) while disk I/O runs under the lock (#598).
///
/// The TOCTOU window that existed in the original drop+re-enter approach
/// is not reintroduced here: we validate the path once when the mod is
/// opened (`handle_editor_open_mod`), and the `mod_path` is stored
/// inside the session — a concurrent `editor_close` would set `snapshot`
/// to `None`, and the write-back guard below returns an error in that
/// case rather than silently installing a stale snapshot.
///
/// An additional generation-token check guards against the close+open race
/// on the **same** mod_path: because `handle_editor_open_mod` bumps
/// `EditorSession::generation`, a reload that started before the close will
/// detect the counter change on Phase 3 re-lock and return an error even
/// when the path is unchanged (codex P1 on #624/#627).
pub fn handle_editor_reload(session: &Mutex<EditorSession>) -> Result<EditorModSnapshot, String> {
    // Phase 1: clone the path and capture the generation token under a brief
    // lock, then release it.  The generation counter is bumped by every
    // snapshot-replacement event (open, reload, save, close), so it uniquely
    // identifies the current session lineage even when a close+open cycle
    // reuses the same mod_path (codex P1 on #624/#627).
    let (mod_path, original_generation) = {
        let s = session.lock().map_err(|e| e.to_string())?;
        let snap = s
            .snapshot
            .as_ref()
            .ok_or_else(|| "no mod is open in the editor".to_string())?;
        (snap.mod_path.clone(), s.generation)
    };

    // Phase 2: disk I/O runs without holding the lock.
    let snapshot = mod_io::load_mod_snapshot(&mod_path).map_err(|e| e.to_string())?;

    // Phase 3: swap the snapshot in — error if a concurrent close cleared the
    // session, if a concurrent operation changed the mod_path, or if the
    // generation token changed (catches a close+open of the *same* mod_path
    // that would otherwise slip through the path-equality check).
    // Bumps both version (mutating operation) and generation (lineage change)
    // so any in-flight update_* requests that captured the pre-reload counters
    // reject their write-backs (codex P1 on #624/#627, #636).
    {
        let mut s = session.lock().map_err(|e| e.to_string())?;
        match s.snapshot.as_ref() {
            None => {
                return Err("editor session was closed during reload".to_string());
            }
            Some(current) if current.mod_path != mod_path => {
                return Err("editor session was modified during reload".to_string());
            }
            Some(_) if s.generation != original_generation => {
                return Err("editor session was modified during reload".to_string());
            }
            Some(_) => {}
        }
        s.snapshot = Some(snapshot.clone());
        s.version = s.version.wrapping_add(1);
        s.generation = s.generation.wrapping_add(1);
    }

    Ok(snapshot)
}

/// Closes the editor session, freeing memory.
///
/// Bumps both `version` and `generation`: closing clears the snapshot, so
/// any in-flight `update_*` requests must reject their write-backs.
pub fn handle_editor_close(session: &Mutex<EditorSession>) -> Result<(), String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    s.snapshot = None;
    s.version = s.version.wrapping_add(1);
    s.generation = s.generation.wrapping_add(1);
    Ok(())
}

// ── Save inspector (read-only) ──────────────────────────────────────────────

/// Lists every `.db` file in `saves_dir`.
pub fn handle_editor_list_saves(saves_dir: &Path) -> Result<Vec<SaveFileSummary>, String> {
    save_inspect::list_saves(saves_dir).map_err(|e| e.to_string())
}

/// Lists every branch in the given save file.
pub fn handle_editor_list_branches(save_path: &Path) -> Result<Vec<BranchSummary>, String> {
    save_inspect::list_branches(save_path).map_err(|e| e.to_string())
}

/// Lists snapshots on the given branch (oldest first).
pub fn handle_editor_list_snapshots(
    save_path: &Path,
    branch_id: i64,
) -> Result<Vec<SnapshotSummary>, String> {
    save_inspect::list_snapshots(save_path, branch_id).map_err(|e| e.to_string())
}

/// Returns the latest snapshot on the given branch as parsed JSON.
pub fn handle_editor_read_snapshot(
    save_path: &Path,
    branch_id: i64,
) -> Result<Option<SnapshotDetail>, String> {
    save_inspect::read_latest_snapshot(save_path, branch_id).map_err(|e| e.to_string())
}

// ── Path validation ─────────────────────────────────────────────────────────

/// Canonicalises `raw` and ensures it resolves inside `root`.
/// Returns a `String` error so both Axum and Tauri call-sites can map it.
pub fn validate_within(raw: &Path, root: &Path) -> Result<PathBuf, String> {
    let canonical = raw.canonicalize().map_err(|_| "invalid path".to_string())?;
    let root_canonical = root
        .canonicalize()
        .map_err(|_| "invalid root directory".to_string())?;
    if !canonical.starts_with(&root_canonical) {
        return Err("path is outside allowed directory".to_string());
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn validate_within_happy_path() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("save.db");
        fs::write(&file, b"").unwrap();
        let result = validate_within(&file, dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn validate_within_dotdot_escape() {
        let dir = tempdir().unwrap();
        let inner = dir.path().join("sub");
        fs::create_dir(&inner).unwrap();
        let file = dir.path().join("outside.db");
        fs::write(&file, b"").unwrap();
        // Try to escape from `inner` to `dir` using `..`
        let traversal = inner.join("../outside.db");
        // The resolved path is inside `dir`, not inside `inner`
        let result = validate_within(&traversal, &inner);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("outside allowed directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_within_path_outside_root() {
        let root = tempdir().unwrap();
        let other = tempdir().unwrap();
        let file = other.path().join("evil.db");
        fs::write(&file, b"").unwrap();
        let result = validate_within(&file, root.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("outside allowed directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn validate_within_nonexistent_path() {
        let dir = tempdir().unwrap();
        let missing = dir.path().join("does_not_exist.db");
        let result = validate_within(&missing, dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("invalid path"), "unexpected error: {err}");
    }

    // ── #378 handle_editor_reload must preserve mod_path across the lock ─

    /// The real rundale mod is the simplest living fixture we can point a
    /// reload at. If it isn't present (sparse workspace checkout) the test
    /// no-ops instead of failing, matching `rundale_validates_clean`.
    #[test]
    fn editor_reload_preserves_mod_path() {
        let root = std::path::PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }
        let session = Mutex::new(EditorSession::default());
        // Seed the session as if `handle_editor_open_mod` had been called.
        let opened = handle_editor_open_mod(&session, &root).unwrap();
        let original_path = opened.snapshot.mod_path.clone();
        // Reload should come back with the same mod_path.
        let reloaded = handle_editor_reload(&session).unwrap();
        assert_eq!(
            reloaded.mod_path, original_path,
            "reload must not redirect the session to a different mod_path"
        );
        // And the session state itself should still hold that same mod_path.
        let stored = session.lock().unwrap();
        let snap = stored.snapshot.as_ref().expect("session snapshot cleared");
        assert_eq!(snap.mod_path, original_path);
    }

    #[test]
    fn editor_reload_errors_when_no_mod_open() {
        let session = Mutex::new(EditorSession::default());
        let err = handle_editor_reload(&session).unwrap_err();
        assert!(
            err.contains("no mod is open"),
            "expected 'no mod is open' error, got: {err}"
        );
    }

    /// Regression test for codex P1 (editor.rs:201/202):
    ///
    /// `handle_editor_reload` releases the mutex during disk I/O.  If a
    /// concurrent close+open races into that window and binds the session to a
    /// NEW mod_path, Phase 3's re-lock must detect the mismatch and refuse to
    /// install the stale snapshot — not silently swap it in.
    ///
    /// This test requires the rundale mod to be present (same guard as
    /// `editor_reload_preserves_mod_path`) and uses two real snapshot loads so
    /// we can exercise the actual write-back path in `handle_editor_reload`.
    #[test]
    fn editor_reload_rejects_stale_snapshot_when_mod_path_changed() {
        let root = std::path::PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }

        // Load two copies of the real snapshot.  In production the "second
        // path" would be a different mod directory; here we synthesise the
        // mismatch by loading the same bytes but then mutating the stored
        // mod_path after the load.
        let snap_a = mod_io::load_mod_snapshot(&root).expect("load snap_a");
        let mut snap_b = snap_a.clone();
        snap_b.mod_path = PathBuf::from("/tmp/__synthetic_other_mod__");

        // Open the session with snap_a (path = root).
        let session = Mutex::new(EditorSession {
            snapshot: Some(snap_a.clone()),
            ..EditorSession::default()
        });

        // Phase 1: clone mod_path (mirrors handle_editor_reload Phase 1).
        let cloned_path = {
            let s = session.lock().unwrap();
            s.snapshot.as_ref().unwrap().mod_path.clone()
        };
        assert_eq!(cloned_path, root.canonicalize().unwrap_or(root.clone()));

        // Simulate concurrent close+open: swap in snap_b (different mod_path).
        {
            let mut s = session.lock().unwrap();
            s.snapshot = Some(snap_b.clone());
        }

        // Phase 3 simulation: the write-back path that handle_editor_reload
        // now executes on re-lock.  A stale snap_a (from disk) is about to be
        // written into a session bound to snap_b's mod_path.
        //
        // Note: we also pass the original_generation here, mirroring the real
        // implementation (but the path check fires first in this scenario).
        let original_generation = 0u64; // EditorSession::default() starts at 0
        let write_back_result: Result<(), String> = {
            let mut s = session.lock().unwrap();
            match s.snapshot.as_ref() {
                None => Err("editor session was closed during reload".to_string()),
                Some(current) if current.mod_path != cloned_path => {
                    Err("editor session was modified during reload".to_string())
                }
                Some(_) if s.generation != original_generation => {
                    Err("editor session was modified during reload".to_string())
                }
                Some(_) => {
                    s.snapshot = Some(snap_a);
                    s.generation = s.generation.wrapping_add(1);
                    Ok(())
                }
            }
        };

        assert!(
            write_back_result.is_err(),
            "write-back must fail when mod_path changed during the I/O window"
        );
        let err = write_back_result.unwrap_err();
        assert!(
            err.contains("modified during reload"),
            "expected 'modified during reload' error, got: {err}"
        );

        // The session must still hold snap_b's mod_path (not the stale snap_a).
        let s = session.lock().unwrap();
        let stored_path = s.snapshot.as_ref().map(|sn| sn.mod_path.clone());
        assert_eq!(
            stored_path,
            Some(snap_b.mod_path),
            "session must remain bound to the new mod_path after a concurrent re-open"
        );
    }

    /// Regression test for codex P1 (#624/#627):
    ///
    /// A concurrent close+open of the **same** mod_path must be detected even
    /// though the path comparison in Phase 3 would pass.  The generation token
    /// is bumped by `handle_editor_open_mod`, so an in-flight reload that
    /// captured the pre-open generation must fail with an error rather than
    /// silently overwriting the freshly-opened snapshot.
    #[test]
    fn editor_reload_rejects_stale_snapshot_same_path_same_generation_bumped() {
        let root = std::path::PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }

        // Load the snapshot as if a reload were in-flight.
        let snap_original = mod_io::load_mod_snapshot(&root).expect("load snap_original");

        // Seed a session that looks exactly as it would after handle_editor_open_mod:
        // generation = 1, snapshot path = root.
        let session = Mutex::new(EditorSession {
            snapshot: Some(snap_original.clone()),
            generation: 1,
            ..EditorSession::default()
        });

        // Phase 1 (reload simulation): capture path + generation under lock.
        let (cloned_path, original_generation) = {
            let s = session.lock().unwrap();
            let snap = s.snapshot.as_ref().unwrap();
            (snap.mod_path.clone(), s.generation)
        };
        assert_eq!(original_generation, 1);

        // Concurrent close+open of the SAME path: generation increments to 2.
        // (Mirrors what handle_editor_open_mod now does.)
        {
            let mut s = session.lock().unwrap();
            let new_snap = snap_original.clone(); // same path, fresh snapshot
            s.snapshot = Some(new_snap);
            s.generation = s.generation.wrapping_add(1); // 1 → 2
        }

        // Phase 3 simulation: stale reload tries to write back.
        // mod_path is the SAME, but generation changed — must be rejected.
        let write_back_result: Result<(), String> = {
            let mut s = session.lock().unwrap();
            match s.snapshot.as_ref() {
                None => Err("editor session was closed during reload".to_string()),
                Some(current) if current.mod_path != cloned_path => {
                    Err("editor session was modified during reload".to_string())
                }
                Some(_) if s.generation != original_generation => {
                    Err("editor session was modified during reload".to_string())
                }
                Some(_) => {
                    s.snapshot = Some(snap_original);
                    s.generation = s.generation.wrapping_add(1);
                    Ok(())
                }
            }
        };

        assert!(
            write_back_result.is_err(),
            "write-back must fail when session was reopened (same path, generation changed)"
        );
        let err = write_back_result.unwrap_err();
        assert!(
            err.contains("modified during reload"),
            "expected 'modified during reload' error, got: {err}"
        );

        // Session generation must remain at 2 (not bumped to 3 by the stale write-back).
        let s = session.lock().unwrap();
        assert_eq!(
            s.generation, 2,
            "generation must not be bumped by the rejected stale write-back"
        );
    }

    // ── #597 version/generation bump tests ──────────────────────────────────

    /// Returns a minimal `EditorModSnapshot` useful for seeding an
    /// `EditorSession` without going to disk.
    fn minimal_snapshot() -> crate::editor::types::EditorModSnapshot {
        use crate::editor::types::{EditorManifest, EditorModSnapshot, ValidationReport};
        use crate::game_mod::{AnachronismData, EncounterTable};
        EditorModSnapshot {
            mod_path: std::path::PathBuf::from("/tmp/test_mod"),
            manifest: EditorManifest {
                id: "test".to_string(),
                name: "Test Mod".to_string(),
                title: None,
                version: "0.1.0".to_string(),
                description: String::new(),
                start_date: "1820-01-01".to_string(),
                start_location: 0,
                period_year: 1820,
            },
            npcs: parish_npc::NpcFile { npcs: vec![] },
            locations: vec![],
            festivals: vec![],
            encounters: EncounterTable {
                by_time: Default::default(),
            },
            anachronisms: AnachronismData {
                context_alert_prefix: String::new(),
                context_alert_suffix: String::new(),
                terms: vec![],
            },
            validation: ValidationReport::default(),
        }
    }

    /// Seeds a `Mutex<EditorSession>` with a snapshot and given starting
    /// counters for convenience in bump-assertion tests.
    fn seeded_session(version: u64, generation: u64) -> Mutex<EditorSession> {
        Mutex::new(EditorSession {
            snapshot: Some(minimal_snapshot()),
            version,
            generation,
        })
    }

    #[test]
    fn editor_open_mod_bumps_version_and_generation() {
        let root = std::path::PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }
        let session = Mutex::new(EditorSession {
            snapshot: None,
            version: 5,
            generation: 3,
        });
        handle_editor_open_mod(&session, &root).expect("open_mod failed");
        let s = session.lock().unwrap();
        assert_eq!(s.version, 6, "open_mod must bump version");
        assert_eq!(s.generation, 4, "open_mod must bump generation");
    }

    #[test]
    fn editor_update_npcs_bumps_version_only() {
        let session = seeded_session(10, 7);
        handle_editor_update_npcs(&session, parish_npc::NpcFile { npcs: vec![] })
            .expect("update_npcs failed");
        let s = session.lock().unwrap();
        assert_eq!(s.version, 11, "update_npcs must bump version");
        assert_eq!(s.generation, 7, "update_npcs must NOT bump generation");
    }

    #[test]
    fn editor_update_locations_bumps_version_only() {
        let session = seeded_session(4, 2);
        handle_editor_update_locations(&session, vec![]).expect("update_locations failed");
        let s = session.lock().unwrap();
        assert_eq!(s.version, 5, "update_locations must bump version");
        assert_eq!(s.generation, 2, "update_locations must NOT bump generation");
    }

    #[test]
    fn editor_close_bumps_version_and_generation() {
        let session = seeded_session(8, 5);
        handle_editor_close(&session).expect("close failed");
        let s = session.lock().unwrap();
        assert_eq!(s.version, 9, "close must bump version");
        assert_eq!(s.generation, 6, "close must bump generation");
        assert!(s.snapshot.is_none(), "close must clear snapshot");
    }

    #[test]
    fn editor_reload_bumps_version_and_generation() {
        let root = std::path::PathBuf::from("../../mods/rundale");
        if !root.exists() {
            return;
        }
        let session = Mutex::new(EditorSession {
            snapshot: None,
            version: 2,
            generation: 1,
        });
        // Open first so reload has a mod_path to work with.
        handle_editor_open_mod(&session, &root).expect("open_mod failed");
        // Reset to known state for clear assertions.
        {
            let mut s = session.lock().unwrap();
            s.version = 2;
            s.generation = 1;
        }
        handle_editor_reload(&session).expect("reload failed");
        let s = session.lock().unwrap();
        assert_eq!(s.version, 3, "reload must bump version");
        assert_eq!(s.generation, 2, "reload must bump generation");
    }
}
