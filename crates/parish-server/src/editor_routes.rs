//! Axum route handlers for the Parish Designer editor.
//!
//! Each route mirrors a Tauri editor command in `parish-tauri/src/editor_commands.rs`
//! by calling the shared handlers in `parish_core::ipc::editor`. Mode parity is
//! maintained: the same editor works identically in Tauri and web modes.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use parish_core::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use parish_core::ipc::editor::{self, EditorSaveResponse};

use crate::state::AppState;

/// Finds the `mods/` directory from the game mod or workspace root.
fn mods_root(state: &AppState) -> PathBuf {
    if let Some(ref gm) = state.game_mod {
        if let Some(parent) = gm.mod_dir.parent() {
            return parent.to_path_buf();
        }
    }
    parish_core::game_mod::find_default_mod()
        .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("mods"))
}

/// `GET /api/editor-list-mods`
pub async fn editor_list_mods(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ModSummary>>, (StatusCode, String)> {
    editor::handle_editor_list_mods(&mods_root(&state))
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// `POST /api/editor-open-mod` with JSON body `{ "modPath": "..." }`
pub async fn editor_open_mod(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EditorOpenModBody>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    let path = PathBuf::from(&body.mod_path);
    editor::handle_editor_open_mod(&state.editor, &path)
        .map(|r| Json(r.snapshot))
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpenModBody {
    pub mod_path: String,
}

/// `GET /api/editor-get-snapshot`
pub async fn editor_get_snapshot(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    editor::handle_editor_get_snapshot(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `GET /api/editor-validate`
pub async fn editor_validate(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    editor::handle_editor_validate(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `POST /api/editor-update-npcs` with JSON body `{ "npcs": ... }`
pub async fn editor_update_npcs(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EditorUpdateNpcsBody>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    let npcs = serde_json::from_value(body.npcs)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid NPC data: {e}")))?;
    editor::handle_editor_update_npcs(&state.editor, npcs)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(serde::Deserialize)]
pub struct EditorUpdateNpcsBody {
    pub npcs: serde_json::Value,
}

/// `POST /api/editor-update-locations` with JSON body `{ "locations": [...] }`
pub async fn editor_update_locations(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EditorUpdateLocationsBody>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    let locations = serde_json::from_value(body.locations).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("invalid location data: {e}"),
        )
    })?;
    editor::handle_editor_update_locations(&state.editor, locations)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(serde::Deserialize)]
pub struct EditorUpdateLocationsBody {
    pub locations: serde_json::Value,
}

/// `POST /api/editor-save` with JSON body `{ "docs": ["npcs", "world", ...] }`
pub async fn editor_save(
    State(state): State<Arc<AppState>>,
    Json(body): Json<EditorSaveBody>,
) -> Result<Json<EditorSaveResponse>, (StatusCode, String)> {
    editor::handle_editor_save(&state.editor, body.docs)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(serde::Deserialize)]
pub struct EditorSaveBody {
    pub docs: Vec<EditorDoc>,
}

/// `POST /api/editor-reload`
pub async fn editor_reload(
    State(state): State<Arc<AppState>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    editor::handle_editor_reload(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `POST /api/editor-close`
pub async fn editor_close(
    State(state): State<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    editor::handle_editor_close(&state.editor)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}
