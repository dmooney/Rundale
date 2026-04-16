//! Editor IPC handlers shared by all frontends.
//!
//! Each function in this module is a self-contained handler that can be
//! called from a Tauri `#[tauri::command]` or an Axum route handler. They
//! coordinate between [`EditorSession`] (the in-memory state) and the
//! `parish-core::editor` pure functions. All I/O happens here; the caller
//! only needs to acquire the session lock.

use std::path::Path;
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
    Ok(snap.validation.clone())
}

/// Replaces the locations in the session with the provided value.
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
    Ok(snap.validation.clone())
}

/// Saves the specified docs from the in-memory snapshot to disk.
///
/// Returns `EditorSaveResponse { saved: true, .. }` on success, or
/// `{ saved: false, .. }` if validation blocked the save.
pub fn handle_editor_save(
    session: &Mutex<EditorSession>,
    docs: Vec<EditorDoc>,
) -> Result<EditorSaveResponse, String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    let snap = s
        .snapshot
        .as_mut()
        .ok_or_else(|| "no mod is open in the editor".to_string())?;

    let result = persist::save_mod(snap, &docs).map_err(|e| e.to_string())?;
    Ok(EditorSaveResponse {
        saved: result.was_saved(),
        validation: result.report().clone(),
    })
}

/// Reloads the current mod from disk, discarding any unsaved edits.
pub fn handle_editor_reload(session: &Mutex<EditorSession>) -> Result<EditorModSnapshot, String> {
    let s = session.lock().map_err(|e| e.to_string())?;
    let mod_path = s
        .snapshot
        .as_ref()
        .map(|snap| snap.mod_path.clone())
        .ok_or_else(|| "no mod is open in the editor".to_string())?;
    drop(s); // release lock before I/O
    handle_editor_open_mod(session, &mod_path).map(|r| r.snapshot)
}

/// Closes the editor session, freeing memory.
pub fn handle_editor_close(session: &Mutex<EditorSession>) -> Result<(), String> {
    let mut s = session.lock().map_err(|e| e.to_string())?;
    s.snapshot = None;
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
