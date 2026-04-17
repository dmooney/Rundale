//! Axum route handlers for the Parish Designer editor.
//!
//! Each route mirrors a Tauri editor command in `parish-tauri/src/editor_commands.rs`
//! by calling the shared handlers in `parish_core::ipc::editor`. Mode parity is
//! maintained: the same editor works identically in Tauri and web modes.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Extension;
use axum::Json;
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
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<ModSummary>>, (StatusCode, String)> {
    editor::handle_editor_list_mods(&mods_root(&state))
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

/// `POST /api/editor-open-mod` with JSON body `{ "modPath": "..." }`
pub async fn editor_open_mod(
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    editor::handle_editor_get_snapshot(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `GET /api/editor-validate`
pub async fn editor_validate(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    editor::handle_editor_validate(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `POST /api/editor-update-npcs` with JSON body `{ "npcs": ... }`
pub async fn editor_update_npcs(
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<EditorSaveBody>,
) -> Result<Json<EditorSaveResponse>, (StatusCode, String)> {
    let docs = body.docs;
    let saved_mod_path = {
        let session = state
            .editor
            .lock()
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        session.snapshot.as_ref().map(|snap| snap.mod_path.clone())
    };
    let result = editor::handle_editor_save(&state.editor, docs.clone())
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    if result.saved
        && docs.contains(&EditorDoc::World)
        && is_active_game_mod(&state, saved_mod_path.as_deref())
    {
        reload_live_world_from_disk(&state)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    Ok(Json(result))
}

#[derive(serde::Deserialize)]
pub struct EditorSaveBody {
    pub docs: Vec<EditorDoc>,
}

fn is_active_game_mod(state: &AppState, path: Option<&std::path::Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Some(active_mod) = state
        .game_mod
        .as_ref()
        .map(|gm| gm.mod_dir.clone())
        .or_else(parish_core::game_mod::find_default_mod)
    else {
        return false;
    };
    let Ok(active_mod) = active_mod.canonicalize() else {
        return false;
    };
    path == active_mod
}

async fn reload_live_world_from_disk(state: &Arc<AppState>) -> Result<(), String> {
    let game_mod = state
        .game_mod
        .clone()
        .or_else(|| {
            parish_core::game_mod::find_default_mod()
                .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok())
        })
        .ok_or_else(|| "active game mod not found".to_string())?;

    let snapshot = {
        let mut world = state.world.lock().await;
        parish_core::editor::reload_world_graph_preserving_runtime(&mut world, &game_mod)
            .map_err(|e| format!("failed to reload world graph: {e}"))?;
        let npc_manager = state.npc_manager.lock().await;
        let transport = state.transport.default_mode();
        let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        ws
    };

    state.event_bus.emit("world-update", &snapshot);
    Ok(())
}

/// `POST /api/editor-reload`
pub async fn editor_reload(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    editor::handle_editor_reload(&state.editor)
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

/// `POST /api/editor-close`
pub async fn editor_close(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    editor::handle_editor_close(&state.editor)
        .map(|_| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

// ── Save inspector (read-only) ──────────────────────────────────────────────

/// `GET /api/editor-list-saves`
pub async fn editor_list_saves(
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
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
    Extension(state): Extension<Arc<AppState>>,
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
    use axum::Extension;

    #[tokio::test]
    async fn editor_open_mod_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "../../etc/passwd".to_string(),
        };
        let result = editor_open_mod(Extension(state), Json(body)).await;
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
        let result = editor_list_branches(Extension(state), Json(body)).await;
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
        let result = editor_list_snapshots(Extension(state), Json(body)).await;
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
        let result = editor_read_snapshot(Extension(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
