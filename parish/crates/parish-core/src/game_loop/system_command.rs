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

use crate::input::{Command, InferenceLogSub};
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
/// - `parish-engine` → `CliCommandHost` (headless CLI)
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

    /// Handle [`CommandEffect::ToggleMap`] — emit toggle event or print text map.
    fn toggle_map(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::OpenDesigner`] — open the Parish Designer.
    fn open_designer(&self) -> BoxFuture<'_, ()>;

    /// Handle [`CommandEffect::SaveGame`] — save current state; returns status message.
    fn save_game(&self) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::ForkBranch`] — fork a new branch; returns status message.
    fn fork_branch(&self, name: String) -> BoxFuture<'_, String>;

    /// Handle [`CommandEffect::LoadBranch`] — load a named branch.
    fn load_branch(&self, name: String) -> BoxFuture<'_, Result<(), String>>;

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

    /// Handle [`CommandEffect::ResetByok`] — wipe BYOK config and re-open the
    /// provider picker. GUI backends emit `EVENT_SETUP_NEEDS_ONBOARDING`;
    /// headless backends should print a short message.
    fn reset_byok(&self) -> BoxFuture<'_, ()> {
        Box::pin(async {})
    }

    /// Handle [`CommandEffect::InferenceLog`] — toggle or report on the
    /// on-disk inference log. Each runtime owns the `InferenceFileLog` /
    /// `ChatTranscriptLog` handles and should flip their shared enable flag
    /// here, then return a human-readable status reply for the player.
    ///
    /// Default implementation is a stub for hosts that haven't been
    /// updated yet; once each entry point overrides it, the default can
    /// be removed.
    fn inference_log_toggle(&self, _sub: InferenceLogSub) -> BoxFuture<'_, String> {
        Box::pin(async { "Inference log control is not available in this runtime.".to_string() })
    }

    /// Emit a text-log message with the given presentation hint.
    ///
    /// This is synchronous (no await) because all three backends emit text-log
    /// events via their `EventEmitter`, which is sync.
    fn emit_text_log(&self, msg: String, presentation: TextPresentation);

    /// Emit an updated world snapshot.
    fn emit_world_update(&self) -> BoxFuture<'_, ()>;

    /// Emit the player's slash command as a distinct command-echo entry in the
    /// transcript (`source:"player"`, `subtype:"command"`).
    ///
    /// Called by [`handle_system_command`] before processing effects when
    /// `is_echo_commands_disabled` returns `false`.  GUI backends (Tauri,
    /// server) override this to emit the payload; the headless CLI keeps the
    /// default no-op because the REPL already prints the raw input.
    fn emit_command_echo(&self, _raw_text: &str) {}

    /// Returns `true` only when the `echo-commands` flag has been explicitly
    /// disabled via `/flag disable echo-commands`.
    ///
    /// Unknown flag = `false` (not disabled) = echo fires.  Default
    /// implementation returns `false` so every host echoes by default.
    fn is_echo_commands_disabled(&self) -> bool {
        false
    }
}

/// Translates a parser-level `InferenceLogSub` to the inference-crate's
/// runtime-level toggle and runs it against the supplied writer handle.
/// Returns the player-visible reply.
///
/// Used by every runtime's `SystemCommandHost::inference_log_toggle` so the
/// wording stays consistent across CLI / server / Tauri.
pub fn apply_inference_log_sub(
    log: &crate::inference::file_log::InferenceFileLog,
    sub: InferenceLogSub,
) -> String {
    use crate::inference::file_log::{LogToggleAction, apply_toggle};
    let action = match sub {
        InferenceLogSub::On => LogToggleAction::On,
        InferenceLogSub::Off => LogToggleAction::Off,
        InferenceLogSub::Status => LogToggleAction::Status,
        InferenceLogSub::Path => LogToggleAction::Path,
    };
    apply_toggle(log, action)
}

/// Shared system-command dispatcher for all three backends.
///
/// Acquires locks, runs the shared [`handle_command`] processor, then dispatches
/// each returned [`CommandEffect`] to the backend-specific `host`.  Finally,
/// emits the command's text response and an updated world snapshot.
///
/// `raw_text` is the original player-typed string (e.g. `"/pause"`).  When the
/// `echo-commands` feature flag is not disabled, it is echoed into the
/// transcript via [`SystemCommandHost::emit_command_echo`] before effects run,
/// so the player can see what was typed alongside the narration it produced.
///
/// This replaces the ~150-line `handle_system_command` that was triplicated in
/// `parish-server`, `parish-tauri`, and `parish-engine` (with only the effect
/// dispatch body differing).  Each backend now provides a ~20-line
/// [`SystemCommandHost`] implementation delegating to this function.
pub async fn handle_system_command(
    host: &dyn SystemCommandHost,
    cmd: Command,
    raw_text: &str,
) -> Result<(), String> {
    // Echo the typed command into the transcript (feature-flagged, default-ON).
    if !host.is_echo_commands_disabled() {
        host.emit_command_echo(raw_text);
    }

    let result = host.run_command(cmd).await;

    for effect in result.effects.clone() {
        match &effect {
            CommandEffect::Quit => {
                host.quit().await;
                return Ok(());
            }
            CommandEffect::RebuildInference => {
                host.rebuild_inference().await;
            }
            // The v2 router has one atomic client set. The legacy effect is
            // retained as an input compatibility detail, but it can no longer
            // rebuild an independently configured "cloud" transport.
            CommandEffect::ToggleMap => {
                host.toggle_map().await;
                // No text log for map toggle — return early (match GUI behaviour).
                return Ok(());
            }
            CommandEffect::OpenDesigner => {
                host.open_designer().await;
                // No text log — navigation handled by frontend.
                return Ok(());
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
                host.load_branch(name.clone()).await?;
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
            CommandEffect::ResetByok => {
                host.reset_byok().await;
            }
            CommandEffect::InferenceLog(sub) => {
                let msg = host.inference_log_toggle(sub.clone()).await;
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ipc::commands::CommandResult;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mock host that records which methods were called.
    struct MockHost {
        quit_called: AtomicBool,
        save_called: AtomicBool,
        world_update_called: AtomicBool,
        text_log: std::sync::Mutex<Vec<(String, TextPresentation)>>,
        calls: std::sync::Mutex<Vec<String>>,
        scripted_result: std::sync::Mutex<Option<CommandResult>>,
        /// Captured raw_text values passed to `emit_command_echo`.
        echo_calls: std::sync::Mutex<Vec<String>>,
        /// When `true`, `is_echo_commands_disabled` returns `true`.
        echo_disabled: bool,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                quit_called: AtomicBool::new(false),
                save_called: AtomicBool::new(false),
                world_update_called: AtomicBool::new(false),
                text_log: std::sync::Mutex::new(Vec::new()),
                calls: std::sync::Mutex::new(Vec::new()),
                scripted_result: std::sync::Mutex::new(None),
                echo_calls: std::sync::Mutex::new(Vec::new()),
                echo_disabled: false,
            }
        }

        fn with_result(result: CommandResult) -> Self {
            let host = Self::new();
            *host.scripted_result.lock().unwrap() = Some(result);
            host
        }

        fn record(&self, call: impl Into<String>) {
            self.calls.lock().unwrap().push(call.into());
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }

        fn assert_quit_called(&self) {
            assert!(self.quit_called.load(Ordering::SeqCst));
        }

        fn assert_save_called(&self) {
            assert!(self.save_called.load(Ordering::SeqCst));
        }

        fn assert_world_update_called(&self) {
            assert!(self.world_update_called.load(Ordering::SeqCst));
        }

        fn assert_text_emitted(&self, expected: &str) {
            let log = self.text_log.lock().unwrap();
            assert!(log.iter().any(|(msg, _)| msg == expected));
        }

        fn echo_calls(&self) -> Vec<String> {
            self.echo_calls.lock().unwrap().clone()
        }
    }

    impl SystemCommandHost for MockHost {
        fn run_command(&self, cmd: Command) -> BoxFuture<'_, CommandResult> {
            if let Some(result) = self.scripted_result.lock().unwrap().clone() {
                return Box::pin(async move { result });
            }
            let effect = match &cmd {
                Command::Save => CommandEffect::SaveGame,
                Command::Quit => CommandEffect::Quit,
                _ => {
                    return Box::pin(async {
                        CommandResult {
                            response: String::new(),
                            effects: vec![],
                            presentation: TextPresentation::Prose,
                        }
                    });
                }
            };
            let result = CommandResult {
                response: "done".to_string(),
                effects: vec![effect],
                presentation: TextPresentation::Prose,
            };
            Box::pin(async { result })
        }

        fn quit(&self) -> BoxFuture<'_, ()> {
            self.record("quit");
            self.quit_called.store(true, Ordering::SeqCst);
            Box::pin(async {})
        }

        fn save_game(&self) -> BoxFuture<'_, String> {
            self.record("save_game");
            self.save_called.store(true, Ordering::SeqCst);
            Box::pin(async { "Game saved.".to_string() })
        }

        fn emit_text_log(&self, msg: String, presentation: TextPresentation) {
            self.record(format!("text:{msg}:{presentation:?}"));
            self.text_log.lock().unwrap().push((msg, presentation));
        }

        fn emit_world_update(&self) -> BoxFuture<'_, ()> {
            self.record("world_update");
            self.world_update_called.store(true, Ordering::SeqCst);
            Box::pin(async {})
        }

        fn rebuild_inference(&self) -> BoxFuture<'_, ()> {
            self.record("rebuild_inference");
            Box::pin(async {})
        }
        fn toggle_map(&self) -> BoxFuture<'_, ()> {
            self.record("toggle_map");
            Box::pin(async {})
        }
        fn open_designer(&self) -> BoxFuture<'_, ()> {
            self.record("open_designer");
            Box::pin(async {})
        }
        fn fork_branch(&self, name: String) -> BoxFuture<'_, String> {
            self.record(format!("fork_branch:{name}"));
            Box::pin(async { "Forked branch.".to_string() })
        }
        fn load_branch(&self, name: String) -> BoxFuture<'_, Result<(), String>> {
            self.record(format!("load_branch:{name}"));
            Box::pin(async { Ok(()) })
        }
        fn list_branches(&self) -> BoxFuture<'_, String> {
            self.record("list_branches");
            Box::pin(async { "branch list".to_string() })
        }
        fn show_log(&self) -> BoxFuture<'_, String> {
            self.record("show_log");
            Box::pin(async { "history".to_string() })
        }
        fn show_spinner(&self, secs: u64) -> BoxFuture<'_, ()> {
            self.record(format!("show_spinner:{secs}"));
            Box::pin(async {})
        }
        fn new_game(&self) -> BoxFuture<'_, Result<(), String>> {
            self.record("new_game");
            Box::pin(async { Ok(()) })
        }
        fn save_flags(&self) -> BoxFuture<'_, ()> {
            self.record("save_flags");
            Box::pin(async {})
        }
        fn apply_theme(&self, name: String, mode: String) -> BoxFuture<'_, ()> {
            self.record(format!("apply_theme:{name}:{mode}"));
            Box::pin(async {})
        }
        fn apply_tiles(&self, id: String) -> BoxFuture<'_, ()> {
            self.record(format!("apply_tiles:{id}"));
            Box::pin(async {})
        }
        fn handle_debug(&self, sub: Option<String>) -> BoxFuture<'_, String> {
            self.record(format!("debug:{sub:?}"));
            Box::pin(async { "debug output".to_string() })
        }

        fn reset_byok(&self) -> BoxFuture<'_, ()> {
            self.record("reset_byok");
            Box::pin(async {})
        }

        fn emit_command_echo(&self, raw_text: &str) {
            self.record(format!("echo:{raw_text}"));
            self.echo_calls.lock().unwrap().push(raw_text.to_string());
        }

        fn is_echo_commands_disabled(&self) -> bool {
            self.echo_disabled
        }
    }

    #[tokio::test]
    async fn dispatches_save_effect_and_emits_world_update() {
        let host = MockHost::new();
        handle_system_command(&host, Command::Save, "/save")
            .await
            .unwrap();
        host.assert_save_called();
        host.assert_text_emitted("Game saved.");
        host.assert_world_update_called();
    }

    #[tokio::test]
    async fn quit_effect_returns_early() {
        let host = MockHost::new();
        handle_system_command(&host, Command::Quit, "/quit")
            .await
            .unwrap();
        host.assert_quit_called();
        // world update should NOT be called after quit (early return)
        assert!(!host.world_update_called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn dispatches_effects_in_order_before_response_and_world_update() {
        let host = MockHost::with_result(CommandResult {
            response: "all done".to_string(),
            effects: vec![
                CommandEffect::RebuildInference,
                CommandEffect::SaveFlags,
                CommandEffect::ApplyTheme("solarized".to_string(), "dark".to_string()),
                CommandEffect::ApplyTiles("historic".to_string()),
                CommandEffect::Debug(Some("schedule".to_string())),
                CommandEffect::ResetByok,
            ],
            presentation: TextPresentation::Tabular,
        });

        handle_system_command(&host, Command::Help, "/help")
            .await
            .unwrap();

        assert_eq!(
            host.calls(),
            vec![
                "echo:/help",
                "rebuild_inference",
                "save_flags",
                "apply_theme:solarized:dark",
                "apply_tiles:historic",
                "debug:Some(\"schedule\")",
                "text:debug output:Prose",
                "reset_byok",
                "text:all done:Tabular",
                "world_update",
            ]
        );
    }

    #[tokio::test]
    async fn toggle_map_effect_returns_before_text_response_or_world_update() {
        let host = MockHost::with_result(CommandResult {
            response: "should not be emitted".to_string(),
            effects: vec![CommandEffect::ToggleMap],
            presentation: TextPresentation::Prose,
        });

        handle_system_command(&host, Command::Help, "/map")
            .await
            .unwrap();

        assert_eq!(host.calls(), vec!["echo:/map", "toggle_map"]);
    }

    /// AC-5: `handle_system_command` emits a command echo for `Command::Pause`
    /// with the original typed text before dispatching any effects.
    #[tokio::test]
    async fn system_command_emits_player_command_echo() {
        let host = MockHost::new();
        handle_system_command(&host, Command::Pause, "/pause")
            .await
            .unwrap();
        let echoes = host.echo_calls();
        assert_eq!(echoes, vec!["/pause"]);
    }

    /// When the host reports `is_echo_commands_disabled() == true`, no echo is
    /// emitted (feature flag kill-switch).
    #[tokio::test]
    async fn system_command_echo_suppressed_when_flag_disabled() {
        let mut host = MockHost::new();
        host.echo_disabled = true;
        handle_system_command(&host, Command::Pause, "/pause")
            .await
            .unwrap();
        assert!(
            host.echo_calls().is_empty(),
            "echo must not fire when echo-commands flag is disabled"
        );
    }
}
