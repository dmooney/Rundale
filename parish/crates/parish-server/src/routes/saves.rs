//! Save-file and branch lifecycle endpoints.
//!
//! Covers:
//! - `GET /api/discover-save-files`
//! - `POST /api/save-game`
//! - `POST /api/load-branch`
//! - `POST /api/create-branch`
//! - `POST /api/new-save-file`
//! - `POST /api/new-game`
//! - `GET /api/save-state`
//! - Inner helpers: `do_save_game_inner`, `do_fork_branch_inner`,
//!   `do_list_branches_inner`, `do_branch_log_inner`, `do_new_game_inner`

use std::path::PathBuf;
use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::http::StatusCode;

use parish_core::event_bus::{EventBus as EventBusTrait, Topic};
use parish_core::ipc::text_log;
use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;

use crate::state::{AppState, SaveState};

use super::admin::validate_branch_name;

// ── Persistence helpers (called by both REST handlers and CommandEffect) ─────

/// Saves the current game state — delegates to the shared canonical impl (#696).
pub async fn do_save_game_inner(state: &Arc<AppState>) -> Result<String, String> {
    parish_core::game_loop::do_save_game(
        &state.world,
        &state.npc_manager,
        &state.save_identity.save_path,
        &state.save_identity.current_branch_id,
        &state.save_identity.current_branch_name,
        &state.saves_dir,
    )
    .await
}

/// Creates a new branch forked from a parent. Returns a human-readable message.
pub async fn do_fork_branch_inner(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    // #335 — validate at the inner call-site so the ForkBranch command path
    // (which bypasses the HTTP handler) is also protected.
    validate_branch_name(name)
        .map_err(|_| "Invalid branch name: must be 1–64 ASCII alphanumeric/underscore/hyphen/space characters.".to_string())?;

    let save_path_guard = state.save_identity.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let name_owned = name.to_string();
    let db_path_clone = db_path.clone();

    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        db.create_branch(&name_owned, Some(parent_branch_id))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let db_path_clone2 = db_path;
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let db = Database::open(&db_path_clone2).map_err(|e| e.to_string())?;
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    *state.save_identity.current_branch_id.lock().await = Some(new_id);
    *state.save_identity.current_branch_name.lock().await = Some(name.to_string());

    Ok(format!("Created new branch '{}'.", name))
}

/// Lists all branches in the current save file.
pub async fn do_list_branches_inner(state: &Arc<AppState>) -> Result<String, String> {
    let save_path_guard = state.save_identity.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let current_branch_id = *state.save_identity.current_branch_id.lock().await;

    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let branches = db.list_branches().map_err(|e| e.to_string())?;
        Ok(parish_core::game_loop::render_branches_text(
            &branches,
            current_branch_id,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Shows the save log for the current branch.
pub async fn do_branch_log_inner(state: &Arc<AppState>) -> Result<String, String> {
    let save_path_guard = state.save_identity.save_path.lock().await;
    let db_path = save_path_guard
        .as_ref()
        .ok_or_else(|| "No active save file. Use /save first.".to_string())?
        .clone();
    drop(save_path_guard);

    let branch_id = state
        .save_identity
        .current_branch_id
        .lock()
        .await
        .ok_or_else(|| "No active branch.".to_string())?;

    let branch_name = state.save_identity.current_branch_name.lock().await.clone();
    let name = branch_name.as_deref().unwrap_or("unknown").to_string();

    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let log = db.branch_log(branch_id).map_err(|e| e.to_string())?;
        Ok(parish_core::game_loop::render_branch_log_text(&name, &log))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Starts a new game (resets world and NPCs from data dir).
pub async fn do_new_game_inner(state: &Arc<AppState>) -> Result<(), String> {
    use crate::emitter::AppStateEmitter;
    use parish_core::game_loop::{NewGameParams, do_new_game};

    let emitter = AppStateEmitter::new(Arc::clone(state));
    let result = do_new_game(NewGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        conversation: &state.conversation,
        save_path: &state.save_identity.save_path,
        current_branch_id: &state.save_identity.current_branch_id,
        current_branch_name: &state.save_identity.current_branch_name,
        saves_dir: &state.saves_dir,
        game_mod: state.game_mod.as_ref(),
        data_dir: &state.data_dir,
        pronunciations: &state.pronunciations,
        default_transport: state.transport.default_mode(),
        emitter: &emitter,
        game_events: &state.game_events,
    })
    .await;
    if result.is_ok() {
        let retained_after_reset = state.game_events.lock().await.len();
        state
            .total_game_events
            .store(retained_after_reset, std::sync::atomic::Ordering::Relaxed);
    }
    result
}

// ── Persistence endpoints ────────────────────────────────────────────────────

/// `GET /api/discover-save-files` — returns all save files with branch metadata.
pub async fn discover_save_files(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<SaveFileInfo>>, (StatusCode, String)> {
    let graph = {
        let world = state.world.lock().await;
        world.graph.clone()
    };
    let saves_dir = state.saves_dir.clone();

    let saves = tokio::task::spawn_blocking(move || discover_saves(&saves_dir, &graph))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(saves))
}

/// `POST /api/save-game` — saves the current game state to the active save file.
pub async fn save_game(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<String>, (StatusCode, String)> {
    let msg = do_save_game_inner(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(msg))
}

/// Request body for `POST /api/load-branch`.
#[derive(serde::Deserialize)]
pub struct LoadBranchRequest {
    /// Path to the save file.
    #[serde(rename = "filePath")]
    pub file_path: String,
    /// Branch database id to load.
    #[serde(rename = "branchId")]
    pub branch_id: i64,
}

/// `POST /api/load-branch` — loads a branch from a save file.
pub async fn load_branch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<LoadBranchRequest>,
) -> Result<StatusCode, (StatusCode, String)> {
    let (path, branch_id) = validate_and_acquire_lock(&state, &body).await?;

    let path_clone = path.clone();
    let (snapshot, branch_name) =
        tokio::task::spawn_blocking(move || load_branch_snapshot(&path_clone, branch_id))
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    restore_snapshot_and_emit(&state, snapshot, &branch_name, branch_id, &path).await;

    Ok(StatusCode::OK)
}

/// Validates the save-file path, checks containment, and acquires an advisory
/// file lock when switching to a different save file.
pub async fn validate_and_acquire_lock(
    state: &Arc<AppState>,
    body: &LoadBranchRequest,
) -> Result<(PathBuf, i64), (StatusCode, String)> {
    use parish_core::persistence::SaveFileLock;

    let path = std::path::PathBuf::from(&body.file_path);
    let canonical = tokio::fs::canonicalize(&path).await.map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            "Invalid save file path".to_string(),
        )
    })?;
    let saves_canonical = tokio::fs::canonicalize(&state.saves_dir)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Saves directory error".to_string(),
            )
        })?;
    if !canonical.starts_with(&saves_canonical) {
        return Err((
            StatusCode::BAD_REQUEST,
            "Path is outside saves directory".to_string(),
        ));
    }
    let path = canonical;
    let branch_id = body.branch_id;

    let current_path = state.save_identity.save_path.lock().await.clone();
    let switching_files = current_path.as_ref() != Some(&path);
    if switching_files {
        let lock = SaveFileLock::try_acquire(&path).ok_or_else(|| {
            (
                StatusCode::CONFLICT,
                "This save file is in use by another instance.".to_string(),
            )
        })?;
        *state.save_lock.lock().await = Some(lock);
    }

    Ok((path, branch_id))
}

/// Opens the database file, loads the latest snapshot for the given branch,
/// and resolves the branch display name.
pub fn load_branch_snapshot(
    path: &std::path::Path,
    branch_id: i64,
) -> Result<(GameSnapshot, String), String> {
    use parish_core::persistence::Database;

    let db = Database::open(path).map_err(|e| e.to_string())?;
    let (_, snapshot) = db
        .load_latest_snapshot(branch_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No snapshots found on this branch.".to_string())?;
    let branches = db.list_branches().map_err(|e| e.to_string())?;
    let branch_name = branches
        .iter()
        .find(|b| b.id == branch_id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    Ok((snapshot, branch_name))
}

/// Restores the snapshot into the world/NPC manager, emits a world-update
/// event, updates session state, and logs the load.
pub async fn restore_snapshot_and_emit(
    state: &Arc<AppState>,
    snapshot: GameSnapshot,
    branch_name: &str,
    branch_id: i64,
    path: &std::path::Path,
) {
    {
        let grounding_enabled = {
            let cfg = state.config.lock().await;
            !cfg.flags.is_disabled("npc-dialogue-grounding")
        };
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        snapshot.restore(&mut world, &mut npc_manager);
        if grounding_enabled {
            npc_manager.clear_introduced_for_session();
        }
        npc_manager.assign_tiers(&world, &[]);

        let mut ws = parish_core::ipc::snapshot_from_world(&world);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        drop(npc_manager);
        drop(world);
        state
            .event_bus
            .emit_named(Topic::WorldUpdate, "world-update", &ws);
    }

    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    state.event_bus.emit_named(
        Topic::TextLog,
        "text-log",
        &text_log(
            "system",
            format!("Loaded {} (branch: {}).", filename, branch_name),
        ),
    );

    *state.save_identity.save_path.lock().await = Some(path.to_path_buf());
    *state.save_identity.current_branch_id.lock().await = Some(branch_id);
    *state.save_identity.current_branch_name.lock().await = Some(branch_name.to_string());
}

/// Request body for `POST /api/create-branch`.
#[derive(serde::Deserialize)]
pub struct CreateBranchRequest {
    /// Name for the new branch.
    pub name: String,
    /// Parent branch database id.
    #[serde(rename = "parentBranchId")]
    pub parent_branch_id: i64,
}

/// `POST /api/create-branch` — creates a new branch forked from a parent.
pub async fn create_branch(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<CreateBranchRequest>,
) -> Result<Json<String>, (StatusCode, String)> {
    // #335 — validate branch name before touching the database.
    validate_branch_name(&body.name).map_err(|s| (s, "Invalid branch name".to_string()))?;
    let msg = do_fork_branch_inner(&state, &body.name, body.parent_branch_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(msg))
}

/// `POST /api/new-save-file` — creates a new save file and saves current state.
pub async fn new_save_file(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    use parish_core::persistence::SaveFileLock;

    let saves_dir = state.saves_dir.clone();
    let path = new_save_path(&saves_dir);

    // Acquire lock on the new save file, releasing any previous lock.
    let lock = SaveFileLock::try_acquire(&path).ok_or_else(|| {
        (
            StatusCode::CONFLICT,
            "Could not lock the new save file.".to_string(),
        )
    })?;
    *state.save_lock.lock().await = Some(lock);

    let snapshot = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "Failed to create main branch".to_string())?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    *state.save_identity.save_path.lock().await = Some(path);
    *state.save_identity.current_branch_id.lock().await = Some(branch_id);
    *state.save_identity.current_branch_name.lock().await = Some("main".to_string());

    Ok(StatusCode::OK)
}

/// `POST /api/new-game` — reloads world/NPCs from data files and saves fresh state.
pub async fn new_game(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    do_new_game_inner(&state)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    state.event_bus.emit_named(
        Topic::TextLog,
        "text-log",
        &text_log("system", "A new chapter begins in the parish..."),
    );

    Ok(StatusCode::OK)
}

/// `GET /api/save-state` — returns the current save state for the StatusBar.
pub async fn get_save_state(Extension(state): Extension<Arc<AppState>>) -> Json<SaveState> {
    let filename = state
        .save_identity
        .save_path
        .lock()
        .await
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string());
    let branch_id = *state.save_identity.current_branch_id.lock().await;
    let branch_name = state.save_identity.current_branch_name.lock().await.clone();

    Json(SaveState {
        filename,
        branch_id,
        branch_name,
    })
}
