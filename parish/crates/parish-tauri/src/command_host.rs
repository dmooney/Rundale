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

use parish_core::game_loop::system_command::{
    BoxFuture, SystemCommandHost, apply_inference_log_sub,
};
use parish_core::input::Command;
use parish_core::ipc::{CommandResult, TextPresentation, handle_command, text_log, text_log_typed};

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
                .unwrap_or_else(|| {
                    parish_core::config::Provider::from_id("openrouter").unwrap_or_default()
                });
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
            match crate::commands::do_save_game(&state).await {
                Ok(msg) => msg,
                Err(e) => format!("Save failed: {}", e),
            }
        })
    }

    fn fork_branch(&self, name: String) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        let app = self.app.clone();
        Box::pin(async move {
            let parent_id = state.current_branch_id.lock().await.unwrap_or(1);
            let emitter = crate::events::TauriEmitter::new(app);
            match crate::commands::do_create_branch(&state, &name, parent_id, Some(&emitter)).await
            {
                Ok(msg) => msg,
                Err(e) => format!("Fork failed: {}", e),
            }
        })
    }

    fn load_branch(&self, _name: String) -> BoxFuture<'_, Result<(), String>> {
        let app = self.app.clone();
        Box::pin(async move {
            let _ = app.emit(EVENT_SAVE_PICKER, ());
            Ok(())
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

    fn inference_log_toggle(
        &self,
        sub: parish_core::input::InferenceLogSub,
    ) -> BoxFuture<'_, String> {
        let file_log = self.state.inference_file_log.clone();
        Box::pin(async move { apply_inference_log_sub(&file_log, sub) })
    }

    fn reset_byok(&self) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        let app = self.app.clone();
        Box::pin(async move {
            // Wipe keychain + parish.toml + GameConfig.api_key.
            let ctx = parish_core::ipc::byok::ByokContext {
                config: &state.config,
                inference_config: &state.inference_config,
                inference_log: state.inference_log.clone(),
                inference_file_log: state.inference_file_log.clone(),
                slots: parish_core::game_loop::inference::InferenceSlots {
                    client: &state.client,
                    worker_handle: &state.worker_handle,
                    inference_queue: &state.inference_queue,
                },
                secrets: Arc::clone(&state.secret_store),
                user_config_dir: state.user_config_dir.as_path(),
            };
            let _ = parish_core::ipc::byok::handle_clear_provider_config(ctx).await;
            // Also wipe the .onboarded sentinel so the gate fires next launch.
            let marker = state
                .user_config_dir
                .join(parish_core::config::user_config::ONBOARDING_MARKER_FILENAME);
            let _ = std::fs::remove_file(&marker);
            // Flip the snapshot flag + tell the overlay to re-open.
            {
                let mut s = state.setup_status.lock().unwrap_or_else(|p| p.into_inner());
                *s = crate::SetupStatusSnapshot::default();
                s.record_needs_onboarding();
            }
            let _ = app.emit(
                crate::events::EVENT_SETUP_NEEDS_ONBOARDING,
                crate::events::SetupStatusPayload {
                    message: "Re-opening provider picker".to_string(),
                },
            );
        })
    }

    fn emit_command_echo(&self, raw_text: &str) {
        let payload = text_log_typed("player", raw_text, "command");
        let _ = self.app.emit(EVENT_TEXT_LOG, payload);
    }

    fn is_echo_commands_disabled(&self) -> bool {
        // Synchronous read — we're inside a non-async trait method.  Using
        // `try_lock` avoids blocking the async runtime.  If the lock is
        // contended (rare — only during a concurrent config write) we fall
        // back to `false` (echo fires) rather than silently suppressing it.
        self.state
            .config
            .try_lock()
            .map(|c| c.flags.is_disabled("echo-commands"))
            .unwrap_or(false)
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
            let npc_manager = state.npc_manager.lock().await;
            let snapshot = crate::commands::get_world_snapshot_inner(
                &world,
                Some(&npc_manager),
                &state.pronunciations,
            );
            let _ = app.emit(EVENT_WORLD_UPDATE, snapshot);
        })
    }
}
