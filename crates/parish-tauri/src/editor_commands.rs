//! Tauri command wrappers for the Parish Designer editor.
//!
//! Each function is a thin `#[tauri::command]` that extracts the
//! `EditorSession` from `AppState`, calls the corresponding shared handler
//! in `parish_core::ipc::editor`, and returns the result as JSON. The
//! `parish-server` crate has identical Axum route handlers for mode parity.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::{Emitter, State};

use parish_core::editor::save_inspect::{
    BranchSummary, SaveFileSummary, SnapshotDetail, SnapshotSummary,
};
use parish_core::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use parish_core::ipc::editor::{self, EditorSaveResponse};

use crate::AppState;
use crate::commands::get_world_snapshot_inner;
use crate::events::EVENT_WORLD_UPDATE;

/// Finds the `mods/` directory by walking up from the working directory.
fn mods_root() -> PathBuf {
    // Reuse find_default_mod's parent: if mods/rundale exists, its grandparent is the workspace.
    parish_core::game_mod::find_default_mod()
        .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("mods"))
}

#[tauri::command]
pub async fn editor_list_mods(_state: State<'_, Arc<AppState>>) -> Result<Vec<ModSummary>, String> {
    editor::handle_editor_list_mods(&mods_root())
}

#[tauri::command]
pub async fn editor_open_mod(
    mod_path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EditorModSnapshot, String> {
    let path = PathBuf::from(&mod_path);
    let root = mods_root();
    let canonical = parish_core::ipc::editor::validate_within(&path, &root)?;
    editor::handle_editor_open_mod(&state.editor, &canonical).map(|r| r.snapshot)
}

#[tauri::command]
pub async fn editor_get_snapshot(
    state: State<'_, Arc<AppState>>,
) -> Result<EditorModSnapshot, String> {
    editor::handle_editor_get_snapshot(&state.editor)
}

#[tauri::command]
pub async fn editor_validate(state: State<'_, Arc<AppState>>) -> Result<ValidationReport, String> {
    editor::handle_editor_validate(&state.editor)
}

#[tauri::command]
pub async fn editor_update_npcs(
    npcs: serde_json::Value,
    state: State<'_, Arc<AppState>>,
) -> Result<ValidationReport, String> {
    let npcs = serde_json::from_value(npcs).map_err(|e| format!("invalid NPC data: {e}"))?;
    editor::handle_editor_update_npcs(&state.editor, npcs)
}

#[tauri::command]
pub async fn editor_update_locations(
    locations: serde_json::Value,
    state: State<'_, Arc<AppState>>,
) -> Result<ValidationReport, String> {
    let locations =
        serde_json::from_value(locations).map_err(|e| format!("invalid location data: {e}"))?;
    editor::handle_editor_update_locations(&state.editor, locations)
}

#[tauri::command]
pub async fn editor_save(
    docs: Vec<EditorDoc>,
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<EditorSaveResponse, String> {
    let saved_mod_path = {
        let session = state.editor.lock().map_err(|e| e.to_string())?;
        session.snapshot.as_ref().map(|snap| snap.mod_path.clone())
    };
    let result = editor::handle_editor_save(&state.editor, docs.clone())?;
    if result.saved
        && docs.contains(&EditorDoc::World)
        && is_active_default_mod(saved_mod_path.as_deref())
    {
        reload_live_world_from_disk(&state, &app).await?;
    }
    Ok(result)
}

#[tauri::command]
pub async fn editor_reload(state: State<'_, Arc<AppState>>) -> Result<EditorModSnapshot, String> {
    editor::handle_editor_reload(&state.editor)
}

#[tauri::command]
pub async fn editor_close(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    editor::handle_editor_close(&state.editor)
}

fn is_active_default_mod(path: Option<&std::path::Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Some(active_mod) = parish_core::game_mod::find_default_mod() else {
        return false;
    };
    let Ok(active_mod) = active_mod.canonicalize() else {
        return false;
    };
    path == active_mod
}

async fn reload_live_world_from_disk(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let mod_dir = parish_core::game_mod::find_default_mod()
        .ok_or_else(|| "active game mod not found".to_string())?;
    let game_mod = parish_core::game_mod::GameMod::load(&mod_dir)
        .map_err(|e| format!("failed to reload game mod: {e}"))?;

    let snapshot = {
        let mut world = state.world.lock().await;
        parish_core::editor::reload_world_graph_preserving_runtime(&mut world, &game_mod)
            .map_err(|e| format!("failed to reload world graph: {e}"))?;
        let npc_manager = state.npc_manager.lock().await;
        get_world_snapshot_inner(
            &world,
            state.transport.default_mode(),
            Some(&npc_manager),
            &state.pronunciations,
        )
    };

    let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    Ok(())
}

// ── Save inspector (read-only) ──────────────────────────────────────────────

fn saves_dir() -> PathBuf {
    parish_core::persistence::picker::ensure_saves_dir()
}

#[tauri::command]
pub async fn editor_list_saves(
    _state: State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileSummary>, String> {
    editor::handle_editor_list_saves(&saves_dir())
}

#[tauri::command]
pub async fn editor_list_branches(
    save_path: String,
    _state: State<'_, Arc<AppState>>,
) -> Result<Vec<BranchSummary>, String> {
    let raw = PathBuf::from(&save_path);
    let root = saves_dir();
    let canonical = parish_core::ipc::editor::validate_within(&raw, &root)?;
    editor::handle_editor_list_branches(&canonical)
}

#[tauri::command]
pub async fn editor_list_snapshots(
    save_path: String,
    branch_id: i64,
    _state: State<'_, Arc<AppState>>,
) -> Result<Vec<SnapshotSummary>, String> {
    let raw = PathBuf::from(&save_path);
    let root = saves_dir();
    let canonical = parish_core::ipc::editor::validate_within(&raw, &root)?;
    editor::handle_editor_list_snapshots(&canonical, branch_id)
}

#[tauri::command]
pub async fn editor_read_snapshot(
    save_path: String,
    branch_id: i64,
    _state: State<'_, Arc<AppState>>,
) -> Result<Option<SnapshotDetail>, String> {
    let raw = PathBuf::from(&save_path);
    let root = saves_dir();
    let canonical = parish_core::ipc::editor::validate_within(&raw, &root)?;
    editor::handle_editor_read_snapshot(&canonical, branch_id)
}
