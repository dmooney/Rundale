//! [`CliCommandHost`] — [`SystemCommandHost`] implementation for the headless CLI.
//!
//! Wraps `Arc<tokio::sync::Mutex<App>>` so the CLI can use the shared
//! [`parish_core::game_loop::handle_system_command`] dispatcher without
//! migrating the entire `App` struct to per-field `Arc<Mutex<T>>` wrappers.
//!
//! # Usage pattern
//!
//! The CLI's `handle_headless_command` function temporarily moves `App` into
//! an `Arc<Mutex<App>>`, calls the shared dispatcher, then moves it back out:
//!
//! ```rust,ignore
//! let app_val = std::mem::replace(app, App::new());
//! let app_arc = Arc::new(tokio::sync::Mutex::new(app_val));
//! let host = CliCommandHost::new(Arc::clone(&app_arc));
//! let should_quit = parish_core::game_loop::handle_system_command(&host, cmd).await;
//! *app = Arc::try_unwrap(app_arc).expect("no clone").into_inner();
//! ```
//!
//! # Mode-parity
//!
//! The single-threaded CLI incurs trivial lock overhead (no contention), but
//! wrapping in `Arc<Mutex<T>>` satisfies the `Send + Sync` requirement of
//! `SystemCommandHost` and gives the CLI identical orchestration logic to the
//! server and Tauri runtimes (#696 slice 7).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use parish_core::game_loop::system_command::{BoxFuture, SystemCommandHost};
use parish_core::input::Command;
use parish_core::ipc::{CommandResult, TextPresentation, handle_command};
use parish_core::persistence::snapshot::GameSnapshot;

use crate::app::App;

/// [`SystemCommandHost`] for the headless CLI backend.
pub struct CliCommandHost {
    /// Shared mutable app state.
    pub app: Arc<tokio::sync::Mutex<App>>,
    /// Set to `true` when [`CommandEffect::Quit`] is processed.
    pub quit_requested: Arc<AtomicBool>,
    /// Set to `true` when [`CommandEffect::RebuildInference`] is processed.
    pub rebuild_inference: Arc<AtomicBool>,
}

impl CliCommandHost {
    /// Creates a new host wrapping the given app.
    ///
    /// Query [`quit_requested`] and [`rebuild_inference`] after
    /// [`parish_core::game_loop::handle_system_command`] returns to
    /// propagate these signals back to the REPL loop.
    pub fn new(app: Arc<tokio::sync::Mutex<App>>) -> Self {
        Self {
            app,
            quit_requested: Arc::new(AtomicBool::new(false)),
            rebuild_inference: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns `true` if `Quit` was processed during the last command.
    pub fn did_quit(&self) -> bool {
        self.quit_requested.load(Ordering::SeqCst)
    }

    /// Returns `true` if `RebuildInference` was processed during the last command.
    pub fn did_rebuild_inference(&self) -> bool {
        self.rebuild_inference.load(Ordering::SeqCst)
    }
}

impl SystemCommandHost for CliCommandHost {
    fn run_command(&self, cmd: Command) -> BoxFuture<'_, CommandResult> {
        Box::pin(async move {
            let mut app = self.app.lock().await;
            let mut config = app.snapshot_config();
            let app_ref: &mut App = &mut app;
            let result = handle_command(
                cmd,
                &mut app_ref.world,
                &mut app_ref.npc_manager,
                &mut config,
            );
            app.apply_config(&config);
            result
        })
    }

    fn quit(&self) -> BoxFuture<'_, ()> {
        self.quit_requested.store(true, Ordering::SeqCst);
        Box::pin(async move {
            // Autosave before quitting.
            let mut app = self.app.lock().await;
            if let Some(ref db) = app.db {
                let snapshot = GameSnapshot::capture(&app.world, &app.npc_manager);
                match db.save_snapshot(app.active_branch_id, &snapshot).await {
                    Ok(snap_id) => {
                        app.latest_snapshot_id = snap_id;
                        println!("Saved and farewell.");
                    }
                    Err(e) => eprintln!("Warning: Failed to save on quit: {}", e),
                }
            }
            app.should_quit = true;
        })
    }

    fn rebuild_inference(&self) -> BoxFuture<'_, ()> {
        self.rebuild_inference.store(true, Ordering::SeqCst);
        Box::pin(async move {
            let mut app = self.app.lock().await;
            if app.provider_name != "simulator" {
                if !(app.base_url.starts_with("http://") || app.base_url.starts_with("https://")) {
                    println!(
                        "[Warning: '{}' doesn't look like a valid URL — NPC conversations may fail.]",
                        app.base_url
                    );
                }
                let provider = parish_core::config::Provider::from_str_loose(&app.provider_name)
                    .unwrap_or_default();
                app.client = Some(parish_core::inference::build_client(
                    &provider,
                    &app.base_url,
                    app.api_key.as_deref(),
                    &app.inference_config,
                ));
            }
        })
    }

    fn rebuild_cloud_client(&self) -> BoxFuture<'_, ()> {
        self.rebuild_inference.store(true, Ordering::SeqCst);
        Box::pin(async move {
            let mut app = self.app.lock().await;
            let base_url = app
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api")
                .to_string();
            let provider = app
                .cloud_provider_name
                .as_deref()
                .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
                .unwrap_or(parish_core::config::Provider::OpenRouter);
            app.cloud_client = Some(parish_core::inference::build_client(
                &provider,
                &base_url,
                app.cloud_api_key.as_deref(),
                &app.inference_config,
            ));
        })
    }

    fn toggle_map(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let app = self.app.lock().await;
            println!("=== Parish Map ===");
            let player_loc = app.world.player_location;
            for node_id in app.world.graph.location_ids() {
                if let Some(data) = app.world.graph.get(node_id) {
                    let marker = if node_id == player_loc { " * " } else { "   " };
                    println!("{}{}", marker, data.name);
                }
            }
            println!();
            println!("Connections:");
            for node_id in app.world.graph.location_ids() {
                if let Some(data) = app.world.graph.get(node_id) {
                    for (neighbor_id, _) in app.world.graph.neighbors(node_id) {
                        if node_id.0 < neighbor_id.0 {
                            let neighbor_name = app
                                .world
                                .graph
                                .get(neighbor_id)
                                .map(|d| d.name.as_str())
                                .unwrap_or("???");
                            println!("  {} — {}", data.name, neighbor_name);
                        }
                    }
                }
            }
        })
    }

    fn open_designer(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            println!("The Parish Designer is only available in the GUI.");
        })
    }

    fn save_game(&self) -> BoxFuture<'_, String> {
        Box::pin(async move {
            let mut app = self.app.lock().await;
            if let Some(ref db) = app.db {
                let snapshot = GameSnapshot::capture(&app.world, &app.npc_manager);
                match db.save_snapshot(app.active_branch_id, &snapshot).await {
                    Ok(snap_id) => {
                        let _ = db
                            .clear_journal(app.active_branch_id, app.latest_snapshot_id)
                            .await;
                        app.latest_snapshot_id = snap_id;
                        app.last_autosave = Some(std::time::Instant::now());
                        "Game saved.".to_string()
                    }
                    Err(e) => format!("Failed to save: {}", e),
                }
            } else {
                "Persistence not available.".to_string()
            }
        })
    }

    fn fork_branch(&self, name: String) -> BoxFuture<'_, String> {
        Box::pin(async move {
            let mut app = self.app.lock().await;
            if let Some(ref db) = app.db {
                let snapshot = GameSnapshot::capture(&app.world, &app.npc_manager);
                let _ = db.save_snapshot(app.active_branch_id, &snapshot).await;
                match db.create_branch(&name, Some(app.active_branch_id)).await {
                    Ok(new_branch_id) => match db.save_snapshot(new_branch_id, &snapshot).await {
                        Ok(snap_id) => {
                            app.active_branch_id = new_branch_id;
                            app.latest_snapshot_id = snap_id;
                            app.last_autosave = Some(std::time::Instant::now());
                            format!("Forked to branch '{}'.", name)
                        }
                        Err(e) => format!("Failed to save fork snapshot: {}", e),
                    },
                    Err(e) => format!("Failed to create branch '{}': {}", name, e),
                }
            } else {
                "Persistence not available.".to_string()
            }
        })
    }

    fn load_branch(&self, name: String) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let mut app = self.app.lock().await;
            if let Err(e) = crate::headless::handle_headless_load(&mut app, &name).await {
                eprintln!("{e}");
            }
        })
    }

    fn list_branches(&self) -> BoxFuture<'_, String> {
        Box::pin(async move {
            let app = self.app.lock().await;
            if let Some(ref db) = app.db {
                match db.list_branches().await {
                    Ok(branches) => {
                        let mut lines = vec!["Save branches:".to_string()];
                        for b in &branches {
                            let marker = if b.id == app.active_branch_id {
                                " *"
                            } else {
                                ""
                            };
                            lines.push(format!(
                                "  {}{} (created {})",
                                b.name,
                                marker,
                                crate::persistence::format_timestamp(&b.created_at)
                            ));
                        }
                        lines.join("\n")
                    }
                    Err(e) => format!("Failed to list branches: {}", e),
                }
            } else {
                "Persistence not available.".to_string()
            }
        })
    }

    fn show_log(&self) -> BoxFuture<'_, String> {
        Box::pin(async move {
            let app = self.app.lock().await;
            if let Some(ref db) = app.db {
                match db.branch_log(app.active_branch_id).await {
                    Ok(snapshots) => {
                        if snapshots.is_empty() {
                            "No snapshots on this branch yet.".to_string()
                        } else {
                            let mut lines =
                                vec!["Snapshot history (most recent first):".to_string()];
                            for s in &snapshots {
                                lines.push(format!(
                                    "  #{} — game: {} | saved: {}",
                                    s.id,
                                    s.game_time,
                                    crate::persistence::format_timestamp(&s.real_time)
                                ));
                            }
                            lines.join("\n")
                        }
                    }
                    Err(e) => format!("Failed to get branch log: {}", e),
                }
            } else {
                "Persistence not available.".to_string()
            }
        })
    }

    fn show_spinner(&self, secs: u64) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            use parish_core::loading::LoadingAnimation;
            use std::io::Write;

            println!("Showing spinner for {} seconds...", secs);
            let mut anim = LoadingAnimation::new();
            let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
            while std::time::Instant::now() < end {
                anim.tick();
                let (r, g, b) = anim.current_color_rgb();
                print!(
                    "\r  \x1b[38;2;{};{};{}m{} {}\x1b[0m\x1b[K",
                    r,
                    g,
                    b,
                    anim.spinner_char(),
                    anim.phrase()
                );
                std::io::stdout().flush().ok();
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            println!("\r\x1b[K");
        })
    }

    fn new_game(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let mut app = self.app.lock().await;
            // Delegate to the existing new-game helper which handles
            // world/NPC reload, branch creation, and location arrival.
            crate::headless::handle_headless_new_game(&mut app).await;
            Ok(())
        })
    }

    fn save_flags(&self) -> BoxFuture<'_, ()> {
        Box::pin(async move {
            let app = self.app.lock().await;
            if let Some(ref p) = app.flags_path
                && let Err(e) = app.flags.save_to_file(p)
            {
                eprintln!("Warning: failed to save feature flags: {}", e);
            }
        })
    }

    fn apply_theme(&self, _name: String, _mode: String) -> BoxFuture<'_, ()> {
        // No visual theme in headless mode; response text is printed by the shared dispatcher.
        Box::pin(async move {})
    }

    fn apply_tiles(&self, _id: String) -> BoxFuture<'_, ()> {
        // No map in headless mode; response text is printed by the shared dispatcher.
        Box::pin(async move {})
    }

    fn handle_debug(&self, sub: Option<String>) -> BoxFuture<'_, String> {
        Box::pin(async move {
            let app = self.app.lock().await;
            let lines = crate::debug::handle_debug(sub.as_deref(), &app);
            lines.join("\n")
        })
    }

    fn emit_text_log(&self, msg: String, _presentation: TextPresentation) {
        // CLI: just print to stdout.
        if !msg.is_empty() {
            println!("{}", msg);
        }
    }

    fn emit_world_update(&self) -> BoxFuture<'_, ()> {
        // CLI has no world-update event; the REPL reads fresh state at each turn.
        Box::pin(async move {})
    }
}
