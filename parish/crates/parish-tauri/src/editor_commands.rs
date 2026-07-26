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

/// Returns the canonical `mods/` directory.
///
/// Rule 9 (#1197): resolved from the active `GameMod` that was loaded once at
/// startup and stored on `AppState`, not by walking up from the cwd per call.
/// Falls back to the legacy cwd-walk only when no mod is loaded (bare install).
/// Mirrors `parish-server`'s `mods_root` for mode parity.
fn mods_root(state: &Arc<AppState>) -> PathBuf {
    mods_root_from_mod(state.game_mod.as_ref())
}

/// Pure resolution: the parent of the active mod's directory, else the legacy
/// cwd-walk fallback. Split out from [`mods_root`] so it can be unit-tested
/// without constructing a full `AppState`.
fn mods_root_from_mod(game_mod: Option<&parish_core::game_mod::GameMod>) -> PathBuf {
    if let Some(gm) = game_mod
        && let Some(parent) = gm.mod_dir.parent()
    {
        return parent.to_path_buf();
    }
    parish_core::game_mod::find_default_mod()
        .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| {
            let fallback = PathBuf::from("mods");
            tracing::warn!(
                path = %fallback.display(),
                "Could not find mods directory from game mod or workspace — falling back to \
                 relative path. The editor may list no mods on packaged builds."
            );
            fallback
        })
}

#[tauri::command]
pub async fn editor_list_mods(state: State<'_, Arc<AppState>>) -> Result<Vec<ModSummary>, String> {
    editor::handle_editor_list_mods(&mods_root(&state))
}

#[tauri::command]
pub async fn editor_open_mod(
    mod_path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<EditorModSnapshot, String> {
    let path = PathBuf::from(&mod_path);
    let root = mods_root(&state);
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
        && is_active_default_mod(&state, saved_mod_path.as_deref())
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

/// True when `path` is the active mod's directory. Rule 9 (#1197): compares
/// against the startup-resolved `state.game_mod.mod_dir`, not a per-call
/// cwd-walk.
fn is_active_default_mod(state: &Arc<AppState>, path: Option<&std::path::Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Some(ref active_mod) = state.game_mod else {
        return false;
    };
    let Ok(active_mod) = active_mod.mod_dir.canonicalize() else {
        return false;
    };
    path == active_mod
}

async fn reload_live_world_from_disk(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    let snapshot = reload_live_world_state_from_disk(state).await?;
    let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
    Ok(())
}

async fn reload_live_world_state_from_disk(
    state: &Arc<AppState>,
) -> Result<parish_core::ipc::WorldSnapshot, String> {
    // Rule 9 (#1197): reload from the startup-resolved mod, not a cwd-walk.
    let game_mod = state
        .game_mod
        .as_ref()
        .ok_or_else(|| "active game mod not found".to_string())?;

    // Serialize graph replacement with staged-turn clone/install.
    let _persistence_guard = state.persistence_gate.lock().await;
    let snapshot = {
        let mut world = state.world.lock().await;
        parish_core::editor::reload_world_graph_preserving_runtime(&mut world, game_mod)
            .map_err(|e| format!("failed to reload world graph: {e}"))?;
        let mut npc_manager = state.npc_manager.lock().await;
        // Graph was replaced wholesale — discard the cached BFS distances so
        // the next assign_tiers call recomputes from the new topology.
        npc_manager.invalidate_bfs_cache();
        get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations)
    };
    Ok(snapshot)
}

// ── Save inspector (read-only) ──────────────────────────────────────────────

#[tauri::command]
pub async fn editor_list_saves(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileSummary>, String> {
    editor::handle_editor_list_saves(&state.saves_dir)
}

#[tauri::command]
pub async fn editor_list_branches(
    save_path: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<BranchSummary>, String> {
    let raw = PathBuf::from(&save_path);
    let canonical = parish_core::ipc::editor::validate_within(&raw, &state.saves_dir)?;
    editor::handle_editor_list_branches(&canonical)
}

#[tauri::command]
pub async fn editor_list_snapshots(
    save_path: String,
    branch_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SnapshotSummary>, String> {
    let raw = PathBuf::from(&save_path);
    let canonical = parish_core::ipc::editor::validate_within(&raw, &state.saves_dir)?;
    editor::handle_editor_list_snapshots(&canonical, branch_id)
}

#[tauri::command]
pub async fn editor_read_snapshot(
    save_path: String,
    branch_id: i64,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<SnapshotDetail>, String> {
    let raw = PathBuf::from(&save_path);
    let canonical = parish_core::ipc::editor::validate_within(&raw, &state.saves_dir)?;
    editor::handle_editor_read_snapshot(&canonical, branch_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cmd_tests::test_app_state;
    use std::path::Path;

    fn load_rundale() -> parish_core::game_mod::GameMod {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        parish_core::game_mod::GameMod::load(&dir).expect("load rundale mod")
    }

    /// #1197 / TD-007: `mods_root` must derive from the startup-resolved mod,
    /// not a cwd-walk. Pointing the mod at a fabricated path that has no
    /// relation to the process cwd and getting that path's parent back proves
    /// the resolution ignores the cwd entirely.
    #[test]
    fn mods_root_derives_from_game_mod_not_cwd() {
        let mut gm = load_rundale();
        gm.mod_dir = PathBuf::from("/nonexistent/sandbox/mods/rundale");
        let root = mods_root_from_mod(Some(&gm));
        assert_eq!(
            root,
            PathBuf::from("/nonexistent/sandbox/mods"),
            "mods_root must be the active mod's parent dir, independent of cwd"
        );
    }

    /// With the real mod loaded, the root is the repo's `mods/` directory.
    #[test]
    fn mods_root_real_mod_points_at_mods_dir() {
        let gm = load_rundale();
        let root = mods_root_from_mod(Some(&gm));
        assert_eq!(root, gm.mod_dir.parent().unwrap());
        assert!(
            root.ends_with("mods"),
            "expected a path ending in mods/, got {}",
            root.display()
        );
    }

    /// No mod loaded → legacy fallback path is still reachable and does not
    /// panic (its value depends on the test cwd, so only liveness is asserted).
    #[test]
    fn mods_root_none_uses_fallback() {
        let _ = mods_root_from_mod(None);
    }

    #[tokio::test]
    async fn live_reload_waits_for_staged_turn_barrier() {
        let mut state = test_app_state();
        Arc::get_mut(&mut state).unwrap().game_mod = Some(load_rundale());
        let held = state.persistence_gate.lock().await;
        let state_for_reload = Arc::clone(&state);
        let reload =
            tokio::spawn(async move { reload_live_world_state_from_disk(&state_for_reload).await });
        tokio::task::yield_now().await;
        assert!(
            !reload.is_finished(),
            "live graph reload must wait while a staged turn owns persistence_gate"
        );
        drop(held);
        tokio::time::timeout(std::time::Duration::from_secs(2), reload)
            .await
            .expect("reload should finish once candidate installation is complete")
            .unwrap()
            .unwrap();
    }
}
