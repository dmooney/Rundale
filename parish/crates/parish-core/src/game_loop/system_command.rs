//! Shared system-command dispatcher — extracted from all three backends (#696 slice 7).
//!
//! [`handle_system_command`] runs the shared [`crate::ipc::handle_command`] logic
//! and dispatches each [`crate::ipc::CommandEffect`] through the backend-specific
//! [`SystemCommandHost`] trait.
//!
//! # Design
//!
//! Each backend (axum server, Tauri desktop, headless CLI) provides a
//! [`SystemCommandHost`] implementation that encapsulates its runtime-specific
//! state.  The trait uses `BoxFuture` return types (the same pattern as
//! [`crate::session_store::SessionStore`]) so it is dyn-compatible and can be
//! passed as `&dyn SystemCommandHost` without the `async-trait` crate.
//!
//! # Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::pin::Pin;

use crate::input::Command;
use crate::ipc::{CommandEffect, TextPresentation};

/// A heap-allocated, Send async future — used as the return type for all
/// [`SystemCommandHost`] async methods so the trait is dyn-compatible.
pub type BoxFuture<'a, T> = Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

/// Backend-specific dispatcher for [`CommandEffect`] side effects.
///
/// Each Parish runtime provides one implementation:
///
/// - `parish-server` → `AppStateCommandHost` (axum web server)
/// - `parish-tauri` → `TauriCommandHost` (Tauri desktop)
/// - `parish-cli` → `CliCommandHost` (headless CLI)
///
/// # Implementing this trait
///
/// Run [`crate::ipc::handle_command`] inside [`run_command`], acquire locks
/// as appropriate for your runtime, release them, then return the result.
/// The individual effect-handler methods (`save_game`, `quit`, etc.) are
/// called by [`handle_system_command`] for each returned effect; they may
/// lock runtime-specific state independently.
pub trait SystemCommandHost: Send + Sync {
    /// Run [`handle_command`] with the appropriate locks held and return the result.
    ///
    /// Implementations must:
    /// 1. Acquire `world`, `npc_manager`, and `config` locks.
    /// 2. Call [`handle_command`].
    /// 3. Release the locks.
    /// 4. Return the [`CommandResult`] (effects + response).
    fn run_command(&self, cmd: Command) -> BoxFuture<'_, crate::ipc::CommandResult>;

    // ── Effect handlers ───────────────────────────────────────────────────────

    /// Handle [`CommandEffect::Quit`] — exit the process/app.
    fn quit(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::RebuildInference`] — rebuild the local inference pipeline.
    fn rebuild_inference(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::RebuildCloudClient`] — rebuild the cloud/dialogue client.
    fn rebuild_cloud_client(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::ToggleMap`] — emit toggle event or print text map.
    fn toggle_map(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::OpenDesigner`] — open the Parish Designer.
    fn open_designer(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::SaveGame`] — save current state; returns status message.
    fn save_game(&self) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::ForkBranch`] — fork a new branch; returns status message.
    fn fork_branch(&self, name: String) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::LoadBranch`] — load a named branch.
    fn load_branch(&self, name: String) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::ListBranches`] — list branches; returns formatted text.
    fn list_branches(&self) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::ShowLog`] — show snapshot log; returns formatted text.
    fn show_log(&self) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::ShowSpinner`] — run loading animation for `secs` seconds.
    fn show_spinner(&self, secs: u64) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::NewGame`] — reset world/NPCs and create a fresh save.
    ///
    /// Returns `Ok(())` on success, `Err(message)` on failure.
    fn new_game(&self) -> BoxFuture<'_, Result<(), String>>;

    /// Handle [`CommandEffect::SaveFlags`] — persist feature flags to disk.
    fn save_flags(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::ApplyTheme`] — apply a UI theme.
    fn apply_theme(&self, name: String, mode: String) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::ApplyTiles`] — switch the full-map tile source.
    fn apply_tiles(&self, id: String) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::Debug`] — run a debug sub-command or return a message.
    ///
    /// GUI backends return `"Debug commands are not available."` (or similar).
    /// The CLI backend runs [`crate::debug::handle_debug`] and returns the lines.
    fn handle_debug(&self, sub: Option<String>) -> BoxFuture<'_, String>;

    /// Emit a text-log message with the given presentation hint.
    ///
    /// This is synchronous (no await) because all three backends emit text-log
    /// events via their `EventEmitter`, which is sync.
    fn emit_text_log(&self, msg: String, presentation: TextPresentation);

    /// Emit an updated world snapshot.
    fn emit_world_update(&self) -> BoxFuture<'_, ()>;
}

/// Shared system-command dispatcher for all three backends.
///
/// Acquires locks, runs the shared [`handle_command`] processor, then dispatches
/// each returned [`CommandEffect`] to the backend-specific `host`.  Finally,
/// emits the command's text response and an updated world snapshot.
///
/// This replaces the ~150-line `handle_system_command` that was triplicated in
/// `parish-server`, `parish-tauri`, and `parish-cli` (with only the effect
/// dispatch body differing).  Each backend now provides a ~20-line
/// [`SystemCommandHost`] implementation delegating to this function.
pub async fn handle_system_command(host: &dyn SystemCommandHost, cmd: Command) {
    let result = host.run_command(cmd).await;

    for effect in result.effects.clone() {
        match &effect {
            CommandEffect::Quit => {
                host.quit().await;
                return;
            }
            CommandEffect::RebuildInference => {
                host.rebuild_inference().await;
            }
            CommandEffect::RebuildCloudClient => {
                host.rebuild_cloud_client().await;
            }
            CommandEffect::ToggleMap => {
                host.toggle_map().await;
                // No text log for map toggle — return early (match GUI behaviour).
                return;
            }
            CommandEffect::OpenDesigner => {
                host.open_designer().await;
                // No text log — navigation handled by frontend.
                return;
            }
            CommandEffect::SaveGame => {
                let msg = host.save_game().await;
                host.emit_text_log(msg, TextPresentation::Prose);
            }
            CommandEffect::ForkBranch(name) => {
                let msg = host.fork_branch(name.clone()).await;
                host.emit_text_log(msg, TextPresentation::Prose);
            }
            CommandEffect::LoadBranch(name) => {
                host.load_branch(name.clone()).await;
            }
            CommandEffect::ListBranches => {
                let msg = host.list_branches().await;
                host.emit_text_log(msg, TextPresentation::Tabular);
            }
            CommandEffect::ShowLog => {
                let msg = host.show_log().await;
                host.emit_text_log(msg, TextPresentation::Tabular);
            }
            CommandEffect::ShowSpinner(secs) => {
                host.show_spinner(*secs).await;
                host.emit_text_log(
                    format!("Showing spinner for {} seconds...", secs),
                    TextPresentation::Prose,
                );
            }
            CommandEffect::NewGame => match host.new_game().await {
                Ok(()) => {
                    host.emit_text_log(
                        "A new chapter begins in the parish...".to_string(),
                        TextPresentation::Prose,
                    );
                }
                Err(e) => {
                    host.emit_text_log(format!("New game failed: {}", e), TextPresentation::Prose);
                }
            },
            CommandEffect::SaveFlags => {
                host.save_flags().await;
            }
            CommandEffect::ApplyTheme(name, mode) => {
                host.apply_theme(name.clone(), mode.clone()).await;
            }
            CommandEffect::ApplyTiles(id) => {
                host.apply_tiles(id.clone()).await;
            }
            CommandEffect::Debug(sub) => {
                let msg = host.handle_debug(sub.clone()).await;
                if !msg.is_empty() {
                    host.emit_text_log(msg, TextPresentation::Prose);
                }
            }
        }
    }

    // Emit the command's text response (if any).
    if !result.response.is_empty() {
        host.emit_text_log(result.response, result.presentation);
    }

    // Emit updated world snapshot.
    host.emit_world_update().await;
}
