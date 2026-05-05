//! [`TauriCommandHost`] — [`SystemCommandHost`] implementation for the Tauri desktop backend.
//!
//! Wraps `Arc<AppState>` and `tauri::AppHandle` and implements each
//! [`SystemCommandHost`] method by delegating to the existing helpers in
//! `commands.rs`.
//!
//! This replaces the ~150-line `handle_system_command` function that was
//! triplicated in `commands.rs` (#696 slice 7).

use std::sync::Arc;

use tauri::Emitter;

use parish_core::game_loop::system_command::{BoxFuture, SystemCommandHost};
use parish_core::input::Command;
use parish_core::ipc::{
    CommandResult, TextPresentation, compute_name_hints, handle_command, snapshot_from_world,
    text_log, text_log_typed,
};
use parish_core::persistence::Database;
use parish_core::persistence::picker::new_save_path;
use parish_core::persistence::snapshot::GameSnapshot;

use crate::AppState;
use crate::events::{
    EVENT_OPEN_DESIGNER, EVENT_SAVE_PICKER, EVENT_TEXT_LOG, EVENT_THEME_SWITCH, EVENT_TILES_SWITCH,
    EVENT_TOGGLE_MAP, EVENT_WORLD_UPDATE, spawn_loading_animation,
};

/// [`SystemCommandHost`] for the Tauri desktop backend.
pub struct TauriCommandHost {
    pub state: Arc<AppState>,
    pub app: tauri::AppHandle,
}

impl TauriCommandHost {
    pub fn new(state: Arc<AppState>, app: tauri::AppHandle) -> Self {
        Self { state, app }
    }
}

impl SystemCommandHost for TauriCommandHost {
    fn run_command(&self, cmd: Command) -> BoxFuture<'_, CommandResult> {
        Box::pin(async move {
            let mut world = self.state.world.lock().await;
            let mut npc_manager = self.state.npc_manager.lock().await;
            let mut config = self.state.config.lock().await;
            handle_command(cmd, &mut world, &mut npc_manager, &mut config)
        })
    }

    fn quit(&self) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            app.exit(0);
        })
    }

    fn rebuild_inference(&self) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        let app = self.app.clone();
        Box::pin(async move {
            crate::commands::rebuild_inference_inner(&state, &app).await;
        })
    }

    fn rebuild_cloud_client(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let config = self.state.config.lock().await;
            let base_url = config
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api")
                .to_string();
            let api_key = config.cloud_api_key.clone();
            let provider_enum = config
                .cloud_provider_name
                .as_deref()
                .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
                .unwrap_or(parish_core::config::Provider::OpenRouter);
            drop(config);
            let mut cloud_guard = self.state.cloud_client.lock().await;
            *cloud_guard = Some(parish_core::inference::build_client(
                &provider_enum,
                &base_url,
                api_key.as_deref(),
                &self.state.inference_config,
            ));
        })
    }

    fn toggle_map(&self) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(EVENT_TOGGLE_MAP, ());
        })
    }

    fn open_designer(&self) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(EVENT_OPEN_DESIGNER, ());
        })
    }

    fn save_game(&self) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match do_save_game_inner(&state).await {
                Ok(msg) => msg,
                Err(e) => format!("Save failed: {}", e),
            }
        })
    }

    fn fork_branch(&self, name: String) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let parent_id = state.current_branch_id.lock().await.unwrap_or(1);
            match crate::commands::do_create_branch(&state, &name, parent_id).await {
                Ok(msg) => msg,
                Err(e) => format!("Fork failed: {}", e),
            }
        })
    }

    fn load_branch(&self, _name: String) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(EVENT_SAVE_PICKER, ());
        })
    }

    fn list_branches(&self) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match crate::commands::do_list_branches_text(&state).await {
                Ok(text) => text,
                Err(e) => format!("Failed to list branches: {}", e),
            }
        })
    }

    fn show_log(&self) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match crate::commands::do_branch_log_text(&state).await {
                Ok(text) => text,
                Err(e) => format!("Failed to show log: {}", e),
            }
        })
    }

    fn show_spinner(&self, secs: u64) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let cancel = tokio_util::sync::CancellationToken::new();
            spawn_loading_animation(app, cancel.clone());
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                cancel.cancel();
            });
        })
    }

    fn new_game(&self) -> BoxFuture<'_, Result<(), String>> {
        let state = Arc::clone(&self.state);
        let app = self.app.clone();
        Box::pin(async move { crate::commands::do_new_game(&state, &app).await })
    }

    fn save_flags(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let flags = self.state.config.lock().await.flags.clone();
            let path = self.state.data_dir.join("parish-flags.json");
            tokio::task::spawn_blocking(move || {
                if let Err(e) = flags.save_to_file(&path) {
                    tracing::warn!("Failed to save feature flags: {}", e);
                }
            });
        })
    }

    fn apply_theme(&self, name: String, mode: String) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(
                EVENT_THEME_SWITCH,
                serde_json::json!({ "name": name, "mode": mode }),
            );
        })
    }

    fn apply_tiles(&self, id: String) -> BoxFuture<'_, ()> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(EVENT_TILES_SWITCH, serde_json::json!({ "id": id }));
        })
    }

    fn handle_debug(&self, _sub: Option<String>) -> BoxFuture<'_, String> {
        Box::pin(async move { "Debug commands are not available in the GUI.".to_string() })
    }

    fn emit_text_log(&self, msg: String, presentation: TextPresentation) {
        let payload = match presentation {
            TextPresentation::Tabular => text_log_typed("system", msg, "tabular"),
            TextPresentation::Prose => text_log("system", msg),
        };
        let _ = self.app.emit(EVENT_TEXT_LOG, payload);
    }

    fn emit_world_update(&self) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        let app = self.app.clone();
        Box::pin(async move {
            let world = state.world.lock().await;
            let transport = state.transport.default_mode();
            let npc_manager = state.npc_manager.lock().await;
            let mut snapshot = snapshot_from_world(&world, transport);
            snapshot.name_hints = compute_name_hints(&world, &npc_manager, &state.pronunciations);
            let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
        })
    }
}

// ── Persistence helpers ─────────────────────────────────────────────────────

/// Saves the current game state to the active save file. Returns a status message.
async fn do_save_game_inner(state: &Arc<AppState>) -> Result<String, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = GameSnapshot::capture(&world, &npc_manager);
    drop(npc_manager);
    drop(world);

    let mut save_path_guard = state.save_path.lock().await;
    let mut branch_id_guard = state.current_branch_id.lock().await;
    let mut branch_name_guard = state.current_branch_name.lock().await;

    let db_path = if let Some(ref path) = *save_path_guard {
        path.clone()
    } else {
        let path = new_save_path(&state.saves_dir);
        *save_path_guard = Some(path.clone());
        path
    };

    let existing_branch_id = *branch_id_guard;
    let (resolved_branch_id, resolved_branch_name) =
        tokio::task::spawn_blocking(move || -> Result<(i64, String), String> {
            let db = Database::open(&db_path).map_err(|e| e.to_string())?;
            let branch_id = if let Some(id) = existing_branch_id {
                id
            } else {
                let branch = db.find_branch("main").map_err(|e| e.to_string())?;
                branch.map(|b| b.id).unwrap_or(1)
            };
            db.save_snapshot(branch_id, &snapshot)
                .map_err(|e| e.to_string())?;
            Ok((branch_id, "main".to_string()))
        })
        .await
        .map_err(|e| e.to_string())??;

    if branch_id_guard.is_none() {
        *branch_id_guard = Some(resolved_branch_id);
        *branch_name_guard = Some(resolved_branch_name.clone());
    }

    let filename = save_path_guard
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let branch_name = branch_name_guard.as_deref().unwrap_or("main");
    Ok(format!(
        "Game saved to {} (branch: {}).",
        filename, branch_name
    ))
}
