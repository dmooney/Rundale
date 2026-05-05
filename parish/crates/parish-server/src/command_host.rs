//! [`AppStateCommandHost`] — [`SystemCommandHost`] implementation for the axum web server.
//!
//! Wraps `Arc<AppState>` and implements each [`SystemCommandHost`] method by
//! delegating to the existing persistence helpers and event-bus emission.
//!
//! This replaces the ~150-line `handle_system_command` function that was
//! triplicated in `routes.rs` (#696 slice 7).

use std::sync::Arc;

use parish_core::event_bus::{EventBus as EventBusTrait, Topic};
use parish_core::game_loop::system_command::{BoxFuture, SystemCommandHost};
use parish_core::input::Command;
use parish_core::ipc::{
    CommandResult, TextPresentation, compute_name_hints, handle_command, snapshot_from_world,
    text_log, text_log_typed,
};

use crate::routes::{do_save_game_inner, spawn_loading_animation};
use crate::state::AppState;

/// [`SystemCommandHost`] for the axum web-server backend.
///
/// Wraps `Arc<AppState>` and delegates each effect to the existing helper
/// functions in `routes.rs`.
pub struct AppStateCommandHost {
    pub state: Arc<AppState>,
}

impl AppStateCommandHost {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl SystemCommandHost for AppStateCommandHost {
    fn run_command(&self, cmd: Command) -> BoxFuture<'_, CommandResult> {
        Box::pin(async move {
            let mut world = self.state.world.lock().await;
            let mut npc_manager = self.state.npc_manager.lock().await;
            let mut config = self.state.config.lock().await;
            handle_command(cmd, &mut world, &mut npc_manager, &mut config)
        })
    }

    fn quit(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            // Web server cannot be quit from the game.
            self.state.event_bus.emit_named(
                Topic::TextLog,
                "text-log",
                &text_log(
                    "system",
                    "The web server cannot be quit from the game. Close your browser tab.",
                ),
            );
        })
    }

    fn rebuild_inference(&self) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            crate::routes::rebuild_inference_inner(&state).await;
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
        Box::pin(async move {
            self.state
                .event_bus
                .emit_named(Topic::UiControl, "toggle-full-map", &());
        })
    }

    fn open_designer(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state
                .event_bus
                .emit_named(Topic::UiControl, "open-designer", &());
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
            match crate::routes::do_fork_branch_inner(&state, &name, parent_id).await {
                Ok(msg) => msg,
                Err(e) => format!("Fork failed: {}", e),
            }
        })
    }

    fn load_branch(&self, _name: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            // Web server: open the save picker in the frontend.
            self.state
                .event_bus
                .emit_named(Topic::UiControl, "save-picker", &());
        })
    }

    fn list_branches(&self) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match crate::routes::do_list_branches_inner(&state).await {
                Ok(text) => text,
                Err(e) => format!("Failed to list branches: {}", e),
            }
        })
    }

    fn show_log(&self) -> BoxFuture<'_, String> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            match crate::routes::do_branch_log_inner(&state).await {
                Ok(text) => text,
                Err(e) => format!("Failed to show log: {}", e),
            }
        })
    }

    fn show_spinner(&self, secs: u64) -> BoxFuture<'_, ()> {
        let state = Arc::clone(&self.state);
        Box::pin(async move {
            let cancel = tokio_util::sync::CancellationToken::new();
            spawn_loading_animation(Arc::clone(&state), cancel.clone());
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
                cancel.cancel();
            });
        })
    }

    fn new_game(&self) -> BoxFuture<'_, Result<(), String>> {
        let state = Arc::clone(&self.state);
        Box::pin(async move { crate::routes::do_new_game_inner(&state).await })
    }

    fn save_flags(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let flags = self.state.config.lock().await.flags.clone();
            let path = self.state.flags_path.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = flags.save_to_file(&path) {
                    tracing::warn!("Failed to save feature flags: {}", e);
                }
            });
        })
    }

    fn apply_theme(&self, name: String, mode: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.event_bus.emit_named(
                Topic::UiControl,
                "theme-switch",
                &serde_json::json!({ "name": name, "mode": mode }),
            );
        })
    }

    fn apply_tiles(&self, id: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            self.state.event_bus.emit_named(
                Topic::UiControl,
                "tiles-switch",
                &serde_json::json!({ "id": id }),
            );
        })
    }

    fn handle_debug(&self, _sub: Option<String>) -> BoxFuture<'_, String> {
        Box::pin(async move { "Debug commands are not available in web mode.".to_string() })
    }

    fn emit_text_log(&self, msg: String, presentation: TextPresentation) {
        let payload = match presentation {
            TextPresentation::Tabular => text_log_typed("system", msg, "tabular"),
            TextPresentation::Prose => text_log("system", msg),
        };
        self.state
            .event_bus
            .emit_named(Topic::TextLog, "text-log", &payload);
    }

    fn emit_world_update(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let world = self.state.world.lock().await;
            let npc_manager = self.state.npc_manager.lock().await;
            let transport = self.state.transport.default_mode();
            let mut ws = snapshot_from_world(&world, transport);
            ws.name_hints = compute_name_hints(&world, &npc_manager, &self.state.pronunciations);
            self.state
                .event_bus
                .emit_named(Topic::WorldUpdate, "world-update", &ws);
        })
    }
}

// No local persistence helpers — delegate to `crate::routes::do_save_game_inner`
// (the canonical server implementation) to avoid duplication.
