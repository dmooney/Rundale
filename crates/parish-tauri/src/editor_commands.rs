//! Tauri command wrappers for the Parish Designer editor.
//!
//! Each function is a thin `#[tauri::command]` that extracts the
//! `EditorSession` from `AppState`, calls the corresponding shared handler
//! in `parish_core::ipc::editor`, and returns the result as JSON. The
//! `parish-server` crate has identical Axum route handlers for mode parity.

use std::path::PathBuf;
use std::sync::Arc;

use tauri::State;

use parish_core::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use parish_core::ipc::editor::{self, EditorSaveResponse};

use crate::AppState;

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
    let path = PathBuf::from(mod_path);
    editor::handle_editor_open_mod(&state.editor, &path).map(|r| r.snapshot)
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
    state: State<'_, Arc<AppState>>,
) -> Result<EditorSaveResponse, String> {
    editor::handle_editor_save(&state.editor, docs)
}

#[tauri::command]
pub async fn editor_reload(state: State<'_, Arc<AppState>>) -> Result<EditorModSnapshot, String> {
    editor::handle_editor_reload(&state.editor)
}

#[tauri::command]
pub async fn editor_close(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    editor::handle_editor_close(&state.editor)
}
