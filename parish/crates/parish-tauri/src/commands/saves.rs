//! Save / load / branch / new-game persistence commands.

use std::sync::Arc;

use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;
use tauri::Emitter;

use crate::AppState;
use crate::events::{EVENT_TEXT_LOG, EVENT_WORLD_UPDATE, TextLogPayload};

/// Returns the list of save files with branch metadata.
#[tauri::command]
pub async fn discover_save_files(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<SaveFileInfo>, String> {
    let world = state.world.lock().await;
    let saves = discover_saves(&state.saves_dir, &world.graph);
    for s in &saves {
        tracing::info!(
            "Save file: {} — {} branches: {:?}",
            s.filename,
            s.branches.len(),
            s.branches.iter().map(|b| &b.name).collect::<Vec<_>>()
        );
    }
    Ok(saves)
}

/// Saves the current game state to the active save file and branch.
///
/// If no save file is active, creates a new one.
#[tauri::command]
pub async fn save_game(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    do_save_game(&state).await
}

/// Internal save implementation — delegates to the shared canonical impl (#696).
pub(crate) async fn do_save_game(state: &Arc<AppState>) -> Result<String, String> {
    parish_core::game_loop::do_save_game(
        &state.world,
        &state.npc_manager,
        &state.save_path,
        &state.current_branch_id,
        &state.current_branch_name,
        &state.saves_dir,
    )
    .await
}

/// Loads a branch from a save file, restoring world and NPC state.
#[tauri::command]
pub async fn load_branch(
    file_path: String,
    branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_load_branch(state.inner(), &app, file_path, branch_id).await
}

/// Internal load-branch implementation shared with the MCP bridge.
///
/// Takes plain `&Arc<AppState>` / `&tauri::AppHandle` so it can be called
/// from non-Tauri-extractor contexts (e.g. `mcp_bridge::load_branch_route`).
pub async fn do_load_branch(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    file_path: String,
    branch_id: i64,
) -> Result<(), String> {
    use parish_core::persistence::SaveFileLock;

    let path = std::path::PathBuf::from(&file_path);

    // If switching to a different save file, acquire a new lock first.
    let current_path = state.save_path.lock().await.clone();
    let switching_files = current_path.as_ref() != Some(&path);

    if switching_files {
        let lock = SaveFileLock::try_acquire(&path)
            .ok_or_else(|| "This save file is in use by another instance.".to_string())?;
        // Release old lock and store new one.
        *state.save_lock.lock().await = Some(lock);
    }

    let path_clone = path.clone();
    let (snapshot, branch_name) =
        tokio::task::spawn_blocking(move || -> Result<(GameSnapshot, String), String> {
            let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
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
        })
        .await
        .map_err(|e| e.to_string())??;

    // Restore state
    let grounding_enabled = {
        let cfg = state.config.lock().await;
        !cfg.flags.is_disabled("npc-dialogue-grounding")
    };
    let mut world = state.world.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    snapshot.restore(&mut world, &mut npc_manager);
    // Gate: clear the in-memory introduced set so NPCs must be re-introduced
    // this session (#1396, npc-dialogue-grounding flag, default-on).
    if grounding_enabled {
        npc_manager.clear_introduced_for_session();
    }
    npc_manager.assign_tiers(&world, &[]);

    // Update save tracking
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Emit updated state to frontend (compute name hints before dropping locks)
    let ws = super::snapshot::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &state.pronunciations,
    );
    drop(npc_manager);
    let _ = app.emit(EVENT_WORLD_UPDATE, ws);
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: format!("Loaded {} (branch: {}).", filename, branch_name),
            subtype: None,
        },
    );

    drop(world);

    // Update persistence tracking
    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some(branch_name);

    Ok(())
}

/// Creates a new branch forked from a specified parent branch.
#[tauri::command]
pub async fn create_branch(
    name: String,
    parent_branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    do_create_branch(&state, &name, parent_branch_id).await
}

/// Internal fork implementation shared by the command and /fork handler.
pub async fn do_create_branch(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
) -> Result<String, String> {
    // #1196 — validate before touching locks or the database.
    parish_core::input::validate_branch_name(name)
        .map_err(|e| format!("Invalid branch name: {e}"))?;

    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };

    tracing::info!(
        "Creating branch '{}' with parent {} in {:?}",
        name,
        parent_branch_id,
        db_path
    );

    // Capture snapshot before spawn_blocking so the tokio locks are not held across it.
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let name_owned = name.to_string();
    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        let new_id = db
            .create_branch(&name_owned, Some(parent_branch_id))
            .map_err(|e| {
                tracing::error!("create_branch failed: {}", e);
                e.to_string()
            })?;
        tracing::info!("Branch '{}' created with id {}", name_owned, new_id);
        db.save_snapshot(new_id, &snapshot)
            .map_err(|e| e.to_string())?;
        tracing::info!("Snapshot saved to branch '{}'", name_owned);
        Ok(new_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    // Switch to the new branch
    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.to_string());

    Ok(format!("Created new branch '{}'.", name))
}

/// Creates a new save file and saves the current state.
#[tauri::command]
pub async fn new_save_file(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    use parish_core::persistence::SaveFileLock;

    let path = new_save_path(&state.saves_dir);

    // Acquire lock on the new save file, releasing any previous lock.
    let lock = SaveFileLock::try_acquire(&path)
        .ok_or_else(|| "Could not lock the new save file.".to_string())?;
    *state.save_lock.lock().await = Some(lock);

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let path_clone = path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or("Failed to create main branch")?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())??;

    *state.save_path.lock().await = Some(path);
    *state.current_branch_id.lock().await = Some(branch_id);
    *state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Internal helper that reloads world/NPCs and creates a fresh save file.
///
/// Called both by the `new_game` Tauri command and the `CommandEffect::NewGame`
/// handler.  Delegates to the shared `parish_core::game_loop::do_new_game` (#696).
pub async fn do_new_game(state: &Arc<AppState>, app: &tauri::AppHandle) -> Result<(), String> {
    use parish_core::game_loop::{NewGameParams, do_new_game as core_do_new_game};

    // Rule 9 (#1197): use the mod resolved once at startup and stored on
    // AppState, not a per-call cwd-walk via find_default_mod().
    let emitter = crate::events::TauriEmitter::new(app.clone());
    let result = core_do_new_game(NewGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        conversation: &state.conversation,
        save_path: &state.save_path,
        current_branch_id: &state.current_branch_id,
        current_branch_name: &state.current_branch_name,
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

/// Starts a brand new game: reloads world and NPCs from data files,
/// creates a new save file, and saves the fresh initial state.
#[tauri::command]
pub async fn new_game(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_new_game(&state, &app).await?;
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: "A new chapter begins in the parish...".to_string(),
            subtype: None,
        },
    );
    Ok(())
}

/// Returns the current save state for display in the StatusBar.
#[tauri::command]
pub async fn get_save_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::SaveState, String> {
    let save_path = state.save_path.lock().await;
    let branch_id = state.current_branch_id.lock().await;
    let branch_name = state.current_branch_name.lock().await;

    Ok(crate::SaveState {
        filename: save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string()),
        branch_id: *branch_id,
        branch_name: branch_name.clone(),
    })
}

/// Formats branch list as text for the /branches command.
pub async fn do_list_branches_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let current_id = *state.current_branch_id.lock().await;

    let branches = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.list_branches().map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(parish_core::game_loop::render_branches_text(
        &branches, current_id,
    ))
}

/// Formats branch log as text for the /log command.
pub async fn do_branch_log_text(state: &Arc<AppState>) -> Result<String, String> {
    let db_path = {
        let guard = state.save_path.lock().await;
        guard
            .as_ref()
            .ok_or("No active save file. Use /save first.")?
            .clone()
    };
    let bid = state
        .current_branch_id
        .lock()
        .await
        .ok_or("No active branch.")?;

    let log = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
        db.branch_log(bid).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let branch_name = state.current_branch_name.lock().await;
    let name = branch_name.as_deref().unwrap_or("unknown");
    Ok(parish_core::game_loop::render_branch_log_text(name, &log))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cmd_tests::test_app_state;

    // ── AppState save state on fresh state ─────────────────────────────────

    #[tokio::test]
    async fn save_state_initial_is_empty() {
        let state = test_app_state();
        let save_path = state.save_path.lock().await;
        let branch_id = state.current_branch_id.lock().await;
        let branch_name = state.current_branch_name.lock().await;

        let filename = save_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string());

        assert!(filename.is_none(), "fresh state should have no save file");
        assert!(branch_id.is_none(), "fresh state should have no branch id");
        assert!(
            branch_name.is_none(),
            "fresh state should have no branch name"
        );
    }

    // ── #1196 — do_create_branch validation gate ────────────────────────────

    /// `do_create_branch` must reject names with disallowed characters before
    /// acquiring any locks or touching the database.
    #[tokio::test]
    async fn create_branch_rejects_invalid_name_before_db() {
        let state = test_app_state();
        let result = do_create_branch(&state, "bad/name!!", 1).await;
        let err = result.expect_err("expected validation error for 'bad/name!!'");
        assert!(
            err.contains("Invalid branch name"),
            "error should mention invalid branch name, got: {err}"
        );
    }

    /// Names with more than 64 characters must be rejected.
    #[tokio::test]
    async fn create_branch_rejects_too_long_name() {
        let state = test_app_state();
        let long_name = "a".repeat(65);
        let result = do_create_branch(&state, &long_name, 1).await;
        let err = result.expect_err("expected validation error for 65-char name");
        assert!(
            err.contains("Invalid branch name"),
            "error should mention invalid branch name, got: {err}"
        );
    }

    /// A valid name should pass validation; failure beyond that is acceptable
    /// (e.g. missing save file) — but the error must NOT be a validation error.
    #[tokio::test]
    async fn create_branch_accepts_valid_name() {
        let state = test_app_state();
        let result = do_create_branch(&state, "my branch 1", 1).await;
        // No save file in test state → expected to fail with "No active save file"
        // but NOT with "Invalid branch name".
        match result {
            Ok(_) => { /* valid name was accepted end-to-end */ }
            Err(e) => {
                assert!(
                    !e.contains("Invalid branch name"),
                    "valid name must not fail validation, got: {e}"
                );
            }
        }
    }
}
