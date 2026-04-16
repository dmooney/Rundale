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

use parish_core::editor::save_inspect::{
    BranchSummary, SaveFileSummary, SnapshotDetail, SnapshotSummary,
};
use parish_core::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use parish_core::ipc::editor::{self, EditorSaveResponse};

use crate::state::AppState;

/// Finds the `mods/` directory from the game mod or workspace root.
fn mods_root(state: &AppState) -> PathBuf {
    if let Some(ref gm) = state.game_mod
        && let Some(parent) = gm.mod_dir.parent()
    {
        return parent.to_path_buf();
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
    let root = mods_root(&state);
    let canonical =
        editor::validate_within(&path, &root).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    editor::handle_editor_open_mod(&state.editor, &canonical)
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

// ── Save inspector (read-only) ──────────────────────────────────────────────

/// `GET /api/editor-list-saves`
pub async fn editor_list_saves(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SaveFileSummary>>, (StatusCode, String)> {
    editor::handle_editor_list_saves(&state.saves_dir)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathBody {
    pub save_path: String,
}

/// `POST /api/editor-list-branches`
pub async fn editor_list_branches(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SavePathBody>,
) -> Result<Json<Vec<BranchSummary>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    editor::handle_editor_list_branches(&canonical)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathBranchBody {
    pub save_path: String,
    pub branch_id: i64,
}

/// `POST /api/editor-list-snapshots`
pub async fn editor_list_snapshots(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SavePathBranchBody>,
) -> Result<Json<Vec<SnapshotSummary>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    editor::handle_editor_list_snapshots(&canonical, body.branch_id)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `POST /api/editor-read-snapshot`
pub async fn editor_read_snapshot(
    State(state): State<Arc<AppState>>,
    Json(body): Json<SavePathBranchBody>,
) -> Result<Json<Option<SnapshotDetail>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    editor::handle_editor_read_snapshot(&canonical, body.branch_id)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::State;

    #[tokio::test]
    async fn editor_open_mod_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "../../etc/passwd".to_string(),
        };
        let result = editor_open_mod(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_list_branches_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBody {
            save_path: "../../etc/passwd".to_string(),
        };
        let result = editor_list_branches(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_list_snapshots_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBranchBody {
            save_path: "../../etc/passwd".to_string(),
            branch_id: 1,
        };
        let result = editor_list_snapshots(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_read_snapshot_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBranchBody {
            save_path: "../../etc/passwd".to_string(),
            branch_id: 1,
        };
        let result = editor_read_snapshot(State(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
