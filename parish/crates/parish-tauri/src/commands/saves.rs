//! Save / load / branch / new-game persistence commands.

use std::sync::Arc;

use parish_core::persistence::Database;
use parish_core::persistence::picker::{SaveFileInfo, discover_saves, new_save_path};
use parish_core::persistence::snapshot::GameSnapshot;
use tauri::Emitter;

use crate::AppState;
use crate::events::{EVENT_TEXT_LOG, TextLogPayload};

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
    let _persistence_guard = state.persistence_gate.lock().await;
    do_save_game(&state).await
}

/// Internal save implementation — delegates to the shared canonical impl (#696).
pub(crate) async fn do_save_game(state: &Arc<AppState>) -> Result<String, String> {
    parish_core::game_loop::do_save_game(parish_core::game_loop::SaveGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        save_path: &state.save_path,
        current_branch_id: &state.current_branch_id,
        current_branch_name: &state.current_branch_name,
        save_lock: &state.save_lock,
        saves_dir: &state.saves_dir,
        session_store: state.session_store.as_ref(),
        session_id: "",
    })
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
    let _persistence_guard = state.persistence_gate.lock().await;
    do_load_branch(state.inner(), &app, file_path, branch_id).await
}

struct PreparedBranchLoad {
    path: std::path::PathBuf,
    branch_name: String,
    recovery: parish_core::session_store::RecoveryBundle,
    candidate_lock: Option<parish_core::persistence::SaveFileLock>,
}

async fn prepare_branch_load(
    state: &Arc<AppState>,
    file_path: String,
    branch_id: i64,
) -> Result<PreparedBranchLoad, String> {
    use parish_core::persistence::SaveFileLock;

    let path = tokio::fs::canonicalize(std::path::PathBuf::from(&file_path))
        .await
        .map_err(|error| format!("Invalid save file path: {error}"))?;
    let saves_dir = tokio::fs::canonicalize(&state.saves_dir)
        .await
        .map_err(|error| format!("Invalid saves directory: {error}"))?;
    if !path.starts_with(&saves_dir) || path.extension().is_none_or(|ext| ext != "db") {
        return Err(format!(
            "Save file {} is outside the configured saves directory",
            path.display()
        ));
    }

    // Keep a candidate lock local until recovery and store binding succeed.
    // The old active lock remains installed throughout validation.
    let current_path = state.save_path.lock().await.clone();
    let switching_files = current_path.as_ref() != Some(&path);
    let candidate_lock = if switching_files {
        Some(
            SaveFileLock::try_acquire(&path)
                .ok_or_else(|| "This save file is in use by another instance.".to_string())?,
        )
    } else {
        None
    };

    let path_clone = path.clone();
    let branch_name = tokio::task::spawn_blocking(move || -> Result<String, String> {
        let db = Database::open(&path_clone).map_err(|e| e.to_string())?;
        db.list_branches()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|branch| branch.id == branch_id)
            .map(|branch| branch.name)
            .ok_or_else(|| format!("Branch id {branch_id} does not exist"))
    })
    .await
    .map_err(|e| e.to_string())??;
    let recovery = parish_core::session_store::load_recovery_bundle(
        state.session_store.as_ref(),
        "",
        &path,
        branch_id,
    )
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "No snapshots found on this branch.".to_string())?;

    Ok(PreparedBranchLoad {
        path,
        branch_name,
        recovery,
        candidate_lock,
    })
}

async fn restore_loaded_branch_state(
    state: &Arc<AppState>,
    recovery: parish_core::session_store::RecoveryBundle,
) -> crate::WorldSnapshot {
    let mut world = state.world.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    world.event_bus.advance_context_epoch();
    recovery.restore(&mut world, &mut npc_manager);
    npc_manager.assign_tiers(&world, &[]);
    let ws = super::snapshot::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &state.pronunciations,
    );
    drop(npc_manager);
    drop(world);

    // Runtime-only context is branch-local even when both branches happen to
    // restore at the same location.
    *state.conversation.lock().await = parish_core::ipc::ConversationRuntimeState::new();
    state.game_events.lock().await.clear();
    ws
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
    let PreparedBranchLoad {
        path,
        branch_name,
        recovery,
        candidate_lock,
    } = prepare_branch_load(state, file_path, branch_id).await?;
    let prepared_binding = state
        .session_store
        .prepare_active_save("", &path)
        .map_err(|error| error.to_string())?;
    parish_core::persistence::write_active_save_identity(
        &state.saves_dir,
        &path,
        branch_id,
        &branch_name,
    )
    .map_err(|error| error.to_string())?;

    // Marker is the durable commit record.
    prepared_binding.commit();

    // Update save tracking
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Restore canonical state and clear every branch-local runtime cache
    // before notifying frontend/MCP consumers.
    let ws = restore_loaded_branch_state(state, recovery).await;
    let emitter = crate::events::TauriEmitter::new(app.clone());
    parish_core::ipc::emit_game_context_reset_then_world_update(
        &emitter,
        serde_json::to_value(&ws).unwrap_or(serde_json::Value::Null),
    );
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

    // Update persistence tracking as one identity critical section.
    let mut save_path = state.save_path.lock().await;
    let mut current_branch_id = state.current_branch_id.lock().await;
    let mut current_branch_name = state.current_branch_name.lock().await;
    *save_path = Some(path.clone());
    *current_branch_id = Some(branch_id);
    *current_branch_name = Some(branch_name.clone());
    drop(current_branch_name);
    drop(current_branch_id);
    drop(save_path);
    if let Some(lock) = candidate_lock {
        *state.save_lock.lock().await = Some(lock);
    }

    Ok(())
}

/// Creates a new branch forked from a specified parent branch.
#[tauri::command]
pub async fn create_branch(
    name: String,
    parent_branch_id: i64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    let _persistence_guard = state.persistence_gate.lock().await;
    let emitter = crate::events::TauriEmitter::new(app);
    do_create_branch(&state, &name, parent_branch_id, Some(&emitter)).await
}

/// Internal fork implementation shared by the command and /fork handler.
pub async fn do_create_branch(
    state: &Arc<AppState>,
    name: &str,
    parent_branch_id: i64,
    emitter: Option<&dyn parish_core::ipc::EventEmitter>,
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
    let db_path_for_write = db_path.clone();
    let new_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&db_path_for_write).map_err(|e| e.to_string())?;
        let (new_id, _) = db
            .create_branch_with_snapshot(&name_owned, Some(parent_branch_id), &snapshot)
            .map_err(|e| {
                tracing::error!("transactional branch creation failed: {}", e);
                e.to_string()
            })?;
        tracing::info!("Branch '{}' created with id {}", name_owned, new_id);
        Ok(new_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    let prepared_binding = state
        .session_store
        .prepare_active_save("", &db_path)
        .map_err(|error| error.to_string())?;
    if let Err(marker_error) = parish_core::persistence::write_active_save_identity(
        &state.saves_dir,
        &db_path,
        new_id,
        name,
    ) {
        let rollback_path = db_path.clone();
        let rollback = tokio::task::spawn_blocking(move || {
            Database::open(&rollback_path)
                .and_then(|db| db.delete_branch(new_id))
                .map_err(|error| error.to_string())
        })
        .await
        .map_err(|error| error.to_string())?;
        return match rollback {
            Ok(()) => Err(marker_error.to_string()),
            Err(rollback_error) => Err(format!(
                "{marker_error}; additionally failed to roll back branch: {rollback_error}"
            )),
        };
    }

    // The marker is the durable commit record. Everything after this point is
    // an infallible in-memory publication of the already-committed identity.
    prepared_binding.commit();
    *state.current_branch_id.lock().await = Some(new_id);
    *state.current_branch_name.lock().await = Some(name.to_string());
    let ws = {
        let world = state.world.lock().await;
        world.event_bus.advance_context_epoch();
        let npc_manager = state.npc_manager.lock().await;
        super::snapshot::get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations)
    };
    *state.conversation.lock().await = parish_core::ipc::ConversationRuntimeState::new();
    state.game_events.lock().await.clear();
    if let Some(emitter) = emitter {
        parish_core::ipc::emit_game_context_reset_then_world_update(
            emitter,
            serde_json::to_value(&ws).unwrap_or(serde_json::Value::Null),
        );
    }

    Ok(format!("Created new branch '{}'.", name))
}

/// Creates a new save file and saves the current state.
#[tauri::command]
pub async fn new_save_file(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _persistence_guard = state.persistence_gate.lock().await;
    use parish_core::persistence::SaveFileLock;

    let path = new_save_path(&state.saves_dir);

    // Keep the candidate lock local so a failed write/bind preserves the old
    // active-file lock and identity.
    let candidate_lock = SaveFileLock::try_acquire(&path)
        .ok_or_else(|| "Could not lock the new save file.".to_string())?;

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

    let prepared_binding = state
        .session_store
        .prepare_active_save("", &path)
        .map_err(|error| error.to_string())?;
    if let Err(marker_error) = parish_core::persistence::write_active_save_identity(
        &state.saves_dir,
        &path,
        branch_id,
        "main",
    ) {
        drop(candidate_lock);
        let cleanup_path = path.clone();
        let cleanup = tokio::task::spawn_blocking(move || std::fs::remove_file(cleanup_path))
            .await
            .map_err(|error| error.to_string())?;
        return match cleanup {
            Ok(()) => Err(marker_error.to_string()),
            Err(cleanup_error) => Err(format!(
                "{marker_error}; additionally failed to remove candidate save: {cleanup_error}"
            )),
        };
    }
    prepared_binding.commit();
    let mut save_path = state.save_path.lock().await;
    let mut current_branch_id = state.current_branch_id.lock().await;
    let mut current_branch_name = state.current_branch_name.lock().await;
    *save_path = Some(path.clone());
    *current_branch_id = Some(branch_id);
    *current_branch_name = Some("main".to_string());
    drop(current_branch_name);
    drop(current_branch_id);
    drop(save_path);
    *state.save_lock.lock().await = Some(candidate_lock);

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
    core_do_new_game(NewGameParams {
        world: &state.world,
        npc_manager: &state.npc_manager,
        conversation: &state.conversation,
        save_path: &state.save_path,
        current_branch_id: &state.current_branch_id,
        current_branch_name: &state.current_branch_name,
        save_lock: &state.save_lock,
        saves_dir: &state.saves_dir,
        session_store: state.session_store.as_ref(),
        session_id: "",
        game_mod: state.game_mod.as_ref(),
        data_dir: &state.data_dir,
        pronunciations: &state.pronunciations,
        default_transport: state.transport.default_mode(),
        emitter: &emitter,
        game_events: &state.game_events,
    })
    .await
}

/// Starts a brand new game: reloads world and NPCs from data files,
/// creates a new save file, and saves the fresh initial state.
#[tauri::command]
pub async fn new_game(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let _persistence_guard = state.persistence_gate.lock().await;
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

    async fn seed_stale_branch_runtime(state: &Arc<AppState>) {
        let location = state.world.lock().await.player_location;
        let mut conversation = state.conversation.lock().await;
        conversation.location = Some(location);
        conversation.record_player_input("old branch input");
        conversation
            .seen_openers_this_location
            .push("old opener".to_string());
        conversation
            .transcript
            .push_back(parish_core::ipc::ConversationLine {
                speaker: "Old NPC".to_string(),
                text: "Old branch transcript".to_string(),
            });
        drop(conversation);
        state.game_events.lock().await.push_back(
            parish_core::world::events::GameEvent::MoodChanged {
                npc_id: parish_core::npc::NpcId(7),
                new_mood: "stale".to_string(),
                location,
                timestamp: chrono::Utc::now(),
            },
        );
        state
            .total_game_events
            .store(41, std::sync::atomic::Ordering::Relaxed);
    }

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
        let result = do_create_branch(&state, "bad/name!!", 1, None).await;
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
        let result = do_create_branch(&state, &long_name, 1, None).await;
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
        let result = do_create_branch(&state, "my branch 1", 1, None).await;
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

    #[tokio::test]
    async fn same_location_branch_restore_resets_runtime_context_and_preserves_lifetime_cursor() {
        let state = test_app_state();
        seed_stale_branch_runtime(&state).await;
        let introduced_id = {
            let mut npc_manager = state.npc_manager.lock().await;
            npc_manager.add_npc(parish_core::npc::Npc::new_test_npc());
            let id = npc_manager
                .all_npcs()
                .next()
                .expect("Tauri fixture has an NPC")
                .id;
            npc_manager.mark_introduced(id);
            id
        };
        let snapshot = {
            let world = state.world.lock().await;
            let npc_manager = state.npc_manager.lock().await;
            parish_core::persistence::GameSnapshot::capture(&world, &npc_manager)
        };
        let recovery = parish_core::session_store::RecoveryBundle {
            snapshot_id: 1,
            snapshot,
            journal: Vec::new(),
        };

        restore_loaded_branch_state(&state, recovery).await;

        let conversation = state.conversation.lock().await;
        assert!(conversation.location.is_none());
        assert!(conversation.transcript.is_empty());
        assert!(conversation.last_player_input.is_none());
        assert!(conversation.seen_openers_this_location.is_empty());
        drop(conversation);
        assert!(state.game_events.lock().await.is_empty());
        assert!(
            state.npc_manager.lock().await.is_introduced(introduced_id),
            "Tauri branch restore must preserve durable identity knowledge"
        );
        assert_eq!(
            state
                .total_game_events
                .load(std::sync::atomic::Ordering::Relaxed),
            41,
            "context replacement clears retained events without rewinding the lifetime cursor"
        );
    }

    #[tokio::test]
    async fn failed_branch_prepare_preserves_runtime_context_and_event_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let candidate_path = temp.path().join("parish_001.db");
        let db = Database::open(&candidate_path).unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        drop(db);

        let mut state = test_app_state();
        let state_parts = Arc::get_mut(&mut state).expect("fresh state must be uniquely owned");
        state_parts.saves_dir = temp.path().to_path_buf();
        state_parts.session_store = Arc::new(parish_core::session_store::DbSessionStore::new(
            temp.path().to_path_buf(),
        ));
        seed_stale_branch_runtime(&state).await;

        let result = prepare_branch_load(
            &state,
            candidate_path.to_string_lossy().to_string(),
            branch.id,
        )
        .await;

        assert!(result.is_err(), "empty candidate branch must fail recovery");
        let conversation = state.conversation.lock().await;
        assert_eq!(
            conversation.last_player_input.as_deref(),
            Some("old branch input")
        );
        assert_eq!(conversation.transcript.len(), 1);
        assert_eq!(conversation.seen_openers_this_location, ["old opener"]);
        drop(conversation);
        assert_eq!(state.game_events.lock().await.len(), 1);
        assert_eq!(
            state
                .total_game_events
                .load(std::sync::atomic::Ordering::Relaxed),
            41
        );
    }
}
