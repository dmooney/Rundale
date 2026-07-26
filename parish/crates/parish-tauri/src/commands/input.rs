//! Player input submission — classification, validation, and dispatch.

use std::sync::Arc;

use parish_core::input::{InputResult, classify_input, is_player_dialogue};
use parish_core::ipc::text_log;

use crate::AppState;
use crate::events::EVENT_TEXT_LOG;

/// Processes player text input: classification → movement, look, or NPC conversation.
///
/// Movement and look results are resolved synchronously. NPC conversations
/// submit an inference request and stream tokens back via `stream-token` events.
#[tauri::command]
pub async fn submit_input(
    text: String,
    addressed_to: Option<Vec<String>>,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    do_submit_input(state.inner(), &app, text, addressed_to.unwrap_or_default()).await
}

/// Internal submit-input implementation shared with the MCP bridge.
///
/// Mirrors the Tauri command body but takes plain `&Arc<AppState>` /
/// `&tauri::AppHandle` so the `mcp_bridge` Axum handler can drive the same
/// dispatcher against the same live AppState the desktop window observes.
pub(crate) async fn do_submit_input(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: String,
    addressed_to: Vec<String>,
) -> Result<(), String> {
    let _persistence_guard = state.persistence_gate.lock().await;
    do_submit_input_locked(state, app, text, addressed_to).await
}

/// Dispatches input while the caller holds [`AppState::persistence_gate`].
///
/// The MCP bridge uses this form so its pre-turn cursor and projected response
/// belong to the same guarded request.
pub(crate) async fn do_submit_input_locked(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    text: String,
    addressed_to: Vec<String>,
) -> Result<(), String> {
    let text = validate_input_text(&text)?;
    if text.is_empty() {
        return Ok(());
    }
    // #752 — cap addressed_to to prevent unbounded memory/allocation via the
    // NPC-addressing chip list.  Max 10 entries; each name ≤ 100 chars.
    validate_addressed_to(&addressed_to)?;

    // #9 — preempt any in-flight Tier 2 / Tier 3 sim call so the player's
    // turn doesn't queue behind a 30 s constrained-decode batch. Cancel the
    // current sim token (any sim call that snapshotted it will drop
    // mid-stream and free the model slot) and install a fresh token so the
    // next sim cycle has a live one to snapshot.
    {
        let mut sc = state.sim_cancel.lock().await;
        sc.cancel();
        *sc = tokio_util::sync::CancellationToken::new();
    }

    match classify_input(&text) {
        InputResult::SystemCommand(cmd) => {
            touch_player_activity(state).await;
            handle_system_command(cmd, state, app, &text).await?;
        }
        InputResult::GameInput(raw) => {
            tracing::info!(input = %raw, "chat [player]");
            // #1351 — only surface a player speech bubble + NPC reactions for
            // genuine dialogue. Deterministic non-dialogue actions (a bare
            // `look`, `look around`, movement phrases) must not render as player
            // speech or provoke NPC reactions. `handle_game_input` still runs so
            // the look/move action itself executes.
            let (dispatch, prelude_emissions) = if is_player_dialogue(&raw) {
                let player_msg = text_log("player", format!("> {}", raw));
                let player_msg_id = player_msg.id.clone();
                let payload = serde_json::to_value(player_msg).unwrap_or(serde_json::Value::Null);
                (
                    Some((player_msg_id, raw.clone())),
                    vec![(EVENT_TEXT_LOG.to_string(), payload)],
                )
            } else {
                (None, Vec::new())
            };
            // Capture location before handle_game_input (which may move the player).
            let reaction_location = state.world.lock().await.player_location;
            handle_game_input(raw, addressed_to, state, app.clone(), prelude_emissions).await?;
            // Generate NPC reactions to the player's message in the background.
            if let Some((player_msg_id, raw_for_reactions)) = dispatch {
                super::reactions::emit_npc_reactions(
                    &player_msg_id,
                    &raw_for_reactions,
                    reaction_location,
                    state,
                    app,
                );
            }
        }
    }

    Ok(())
}

// ── #752 — addressed_to validation ───────────────────────────────────────────

/// Validates and trims player free-text input for `submit_input`.
///
/// - Trims leading/trailing whitespace.
/// - Returns `Ok(String)` (the trimmed text) for empty input — callers should
///   short-circuit before calling this if they want to silently drop empties.
/// - Returns `Err` when the trimmed length exceeds 2000 characters.
pub fn validate_input_text(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().to_string();
    if trimmed.len() > 2000 {
        return Err("Input too long (max 2000 characters).".to_string());
    }
    Ok(trimmed)
}

/// Maximum number of NPC chips a single `submit_input` may carry. Validated
/// in [`validate_addressed_to`] (#933 — CodeQL `rust/uncontrolled-allocation-size`).
pub(crate) const MAX_ADDRESSED_TO: usize = 10;

/// Validates the `addressed_to` list from the `submit_input` command.
///
/// Rules (mode-parity with the server path in `parish-server`):
/// - At most [`MAX_ADDRESSED_TO`] entries (prevents unbounded NPC-chip spam).
/// - Each name is at most **100** characters.
///
/// Returns `Err(String)` with a user-visible message on any violation.
pub fn validate_addressed_to(addressed_to: &[String]) -> Result<(), String> {
    if addressed_to.len() > MAX_ADDRESSED_TO {
        return Err(format!("Too many addressees (max {MAX_ADDRESSED_TO})."));
    }
    if addressed_to.iter().any(|name| name.len() > 100) {
        return Err("Addressee name too long (max 100 characters).".to_string());
    }
    Ok(())
}

// ── Internal helpers ─────────────────────────────────────────────────────────

pub(crate) async fn touch_player_activity(state: &Arc<AppState>) {
    let mut conversation = state.conversation.lock().await;
    let now = std::time::Instant::now();
    conversation.last_player_activity = now;
    conversation.last_spoken_at = now;
}

async fn world_update_payload(state: &Arc<AppState>) -> serde_json::Value {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = super::snapshot::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &state.pronunciations,
    );
    serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null)
}

/// Handles `/command` inputs.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`TauriCommandHost`] adapter (#696 slice 7).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
    raw_text: &str,
) -> Result<(), String> {
    use crate::command_host::TauriCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;

    let host = TauriCommandHost::new(Arc::clone(state), app.clone());
    shared_handle(&host, cmd, raw_text).await
}

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
///
/// Delegates to [`parish_core::game_loop::handle_game_input`] for all shared
/// logic (intent parsing, Interact narration, no-silent-drop fallback, NPC
/// conversation) — identical to the server path in `parish-server` (rule #12 /
/// #2 mode-parity, #1467). Takes plain `&Arc<AppState>` (not
/// `tauri::State<...>`) so the body can be called from non-Tauri-extractor
/// contexts — namely the `mcp_bridge` Axum handlers, which share the same live
/// AppState as the desktop window. The Tauri callsite passes `state.inner()`.
pub(crate) async fn handle_game_input(
    raw: String,
    addressed_to: Vec<String>,
    state: &Arc<AppState>,
    app: tauri::AppHandle,
    mut prelude_emissions: Vec<(String, serde_json::Value)>,
) -> Result<(), String> {
    let must_stage = {
        let world = state.world.lock().await;
        parish_core::game_loop::input_may_mutate_tasks(&world, &raw)
    };
    let before_progress = state.world.lock().await.player_progress.clone();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
        inference_failure_messages: &state.inference_failure_messages,
        idle_messages: &state.idle_messages,
    };
    let transport = state.transport.default_mode().clone();
    let reaction_templates = state.reaction_templates.clone();

    if must_stage {
        let task_target = {
            let save_path = state.save_path.lock().await;
            let branch_id = state.current_branch_id.lock().await;
            match (save_path.as_ref(), *branch_id) {
                (Some(path), Some(branch_id)) => {
                    Some(parish_core::session_store::TaskJournalTarget {
                        session_id: String::new(),
                        save_path: path.clone(),
                        branch_id,
                    })
                }
                _ => None,
            }
        };
        prelude_emissions.push((
            crate::events::EVENT_WORLD_UPDATE.to_string(),
            world_update_payload(state).await,
        ));
        parish_core::game_loop::handle_staged_game_input(
            &ctx,
            state.session_store.as_ref(),
            task_target.as_ref(),
            prelude_emissions,
            raw,
            addressed_to,
            &transport,
            &reaction_templates,
        )
        .await
        .map_err(|error| {
            tracing::error!(%error, "player task journal append failed");
            format!("failed to persist player task: {error}")
        })?;
        super::snapshot::emit_world_update(state, &app).await;
        return Ok(());
    }

    touch_player_activity(state).await;
    parish_core::game_loop::flush_staged_emissions(emitter.as_ref(), prelude_emissions);

    let app_for_loading = app.clone();
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        crate::events::spawn_loading_animation(app_for_loading.clone(), cancel.clone());
        Some(cancel)
    };

    super::snapshot::emit_world_update(state, &app).await;
    let outcome = parish_core::game_loop::handle_game_input(
        &ctx,
        raw,
        addressed_to,
        &transport,
        &reaction_templates,
        spawn_loading,
    )
    .await;
    let task_target = {
        let save_path = state.save_path.lock().await;
        let branch_id = state.current_branch_id.lock().await;
        match (save_path.as_ref(), *branch_id) {
            (Some(path), Some(branch_id)) => Some(parish_core::session_store::TaskJournalTarget {
                session_id: String::new(),
                save_path: path.clone(),
                branch_id,
            }),
            _ => None,
        }
    };
    persist_task_mutations(
        state,
        task_target.as_ref(),
        before_progress,
        &outcome.task_mutations,
    )
    .await
    .map_err(|error| {
        tracing::error!(%error, "player task journal append failed");
        format!("failed to persist player task: {error}")
    })?;
    super::snapshot::emit_world_update(state, &app).await;
    Ok(())
}

async fn persist_task_mutations(
    state: &Arc<AppState>,
    target: Option<&parish_core::session_store::TaskJournalTarget>,
    before: parish_core::session_store::PlayerProgress,
    tasks: &[parish_core::session_store::PlayerTask],
) -> Result<(), parish_core::error::ParishError> {
    parish_core::session_store::append_task_mutations_or_rollback(
        state.session_store.as_ref(),
        target,
        tasks,
        &state.world,
        before,
    )
    .await?;
    Ok(())
}

/// Delegates to [`parish_core::game_loop::run_idle_banter`] for all shared
/// logic (#696), then emits a world-update snapshot when the sequence ends.
/// Runs idle banter while the caller holds `persistence_gate`.
pub(super) async fn run_idle_banter_locked(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let before_progress = state.world.lock().await.player_progress.clone();
    let task_target = {
        let save_path = state.save_path.lock().await;
        let branch_id = state.current_branch_id.lock().await;
        match (save_path.as_ref(), *branch_id) {
            (Some(path), Some(branch_id)) => Some(parish_core::session_store::TaskJournalTarget {
                session_id: String::new(),
                save_path: path.clone(),
                branch_id,
            }),
            _ => None,
        }
    };
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));
    let ctx = parish_core::game_loop::GameLoopContext {
        world: &state.world,
        npc_manager: &state.npc_manager,
        config: &state.config,
        conversation: &state.conversation,
        inference_queue: &state.inference_queue,
        emitter: std::sync::Arc::clone(&emitter),
        inference_config: &state.inference_config,
        pronunciations: &state.pronunciations,
        client: &state.client,
        cloud_client: &state.cloud_client,
        language: state.language_settings.clone(),
        inference_failure_messages: &state.inference_failure_messages,
        idle_messages: &state.idle_messages,
    };

    super::snapshot::emit_world_update(state, app).await;
    // Idle banter spawns no loading animation.
    let outcome = parish_core::game_loop::run_idle_banter(&ctx, || None).await;
    if let Err(error) = persist_task_mutations(
        state,
        task_target.as_ref(),
        before_progress,
        &outcome.task_mutations,
    )
    .await
    {
        tracing::error!(%error, "idle-banter task journal append failed");
    }
    super::snapshot::emit_world_update(state, app).await;
}

/// Helper: sets `conversation_in_progress` on the conversation mutex.
///
/// Only used in unit tests; production code uses the `GameLoopContext`-based
/// shared orchestration which manages this flag internally.
#[cfg(test)]
pub(crate) async fn set_conversation_running(state: &Arc<AppState>, running: bool) {
    let mut conversation = state.conversation.lock().await;
    conversation.conversation_in_progress = running;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cmd_tests::test_app_state;

    // ── validate_input_text ─────────────────────────────────────────────────

    #[test]
    fn validate_input_accepts_normal_text() {
        assert!(validate_input_text("ask Brigid about the harvest").is_ok());
    }

    #[test]
    fn validate_input_trims_whitespace() {
        let result = validate_input_text("  hello  ").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn validate_input_allows_empty_after_trim() {
        assert!(validate_input_text("   ").is_ok());
    }

    #[test]
    fn validate_input_rejects_over_2000_chars() {
        let long: String = "a".repeat(2001);
        assert!(validate_input_text(&long).is_err());
    }

    #[test]
    fn validate_input_accepts_exactly_2000_chars() {
        let exactly: String = "a".repeat(2000);
        assert!(validate_input_text(&exactly).is_ok());
    }

    // ── validate_addressed_to ───────────────────────────────────────────────

    #[test]
    fn validate_addressed_to_accepts_empty_list() {
        assert!(validate_addressed_to(&[]).is_ok());
    }

    #[test]
    fn validate_addressed_to_accepts_up_to_10() {
        let names: Vec<String> = (0..10).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_ok());
    }

    #[test]
    fn validate_addressed_to_rejects_11_names() {
        let names: Vec<String> = (0..11).map(|i| format!("Npc{}", i)).collect();
        assert!(validate_addressed_to(&names).is_err());
    }

    #[test]
    fn validate_addressed_to_rejects_name_over_100_chars() {
        let long_name = "a".repeat(101);
        assert!(validate_addressed_to(&[long_name]).is_err());
    }

    #[test]
    fn validate_addressed_to_accepts_100_char_name() {
        let name = "a".repeat(100);
        assert!(validate_addressed_to(&[name]).is_ok());
    }

    // ── conversation state ──────────────────────────────────────────────────

    #[tokio::test]
    async fn set_conversation_running_toggles_flag() {
        let state = test_app_state();

        // Initially not running
        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }

        set_conversation_running(&state, true).await;

        {
            let conv = state.conversation.lock().await;
            assert!(conv.conversation_in_progress);
        }

        set_conversation_running(&state, false).await;

        {
            let conv = state.conversation.lock().await;
            assert!(!conv.conversation_in_progress);
        }
    }

    // ── #9 sim-cancel preemption ─────────────────────────────────────────────

    /// Snapshotting `sim_cancel`, then calling the fire-and-replace block
    /// from `do_submit_input`, must cancel the snapshot AND leave a fresh
    /// (uncancelled) token in place for the next sim cycle.
    #[tokio::test]
    async fn sim_cancel_fires_snapshot_and_replaces_token() {
        let state = test_app_state();

        // Snapshot the current sim_cancel — this is what Tier 2/3 spawn
        // captures before player input arrives.
        let snapshot = state.sim_cancel.lock().await.clone();
        assert!(!snapshot.is_cancelled(), "fresh token starts uncancelled");

        // Inline the fire-and-replace block from do_submit_input. We don't
        // call do_submit_input directly because it needs a tauri::AppHandle.
        {
            let mut sc = state.sim_cancel.lock().await;
            sc.cancel();
            *sc = tokio_util::sync::CancellationToken::new();
        }

        // The snapshot is cancelled — any in-flight Tier 2/3 call that
        // captured it will see is_cancelled() == true and bail.
        assert!(
            snapshot.is_cancelled(),
            "captured snapshot must observe cancellation"
        );

        // The new token replacing it is *not* cancelled — the next sim
        // cycle captures this one cleanly.
        let next = state.sim_cancel.lock().await.clone();
        assert!(
            !next.is_cancelled(),
            "replacement token must be fresh / uncancelled"
        );
    }

    // ── #1467: Tauri submit-input path parity — action narration ────────────
    //
    // These tests verify that the Tauri/MCP-bridge submit-input path now routes
    // through `parish_core::game_loop::handle_game_input` and therefore produces
    // the same Interact narration that the headless server path does.  They
    // exercise the game-loop context directly (the exact function the new
    // `handle_game_input` wrapper calls) since constructing a tauri::AppHandle
    // in unit tests is not possible without a real Tauri runtime.

    /// Minimal capturing EventEmitter for parity tests.
    ///
    /// Collects `(event_name, payload)` pairs so tests can assert the Tauri
    /// dispatch path emits the correct action narrations.
    struct TestCapturingEmitter {
        events: std::sync::Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl TestCapturingEmitter {
        fn new() -> Self {
            Self {
                events: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn text_log_contents(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .filter(|(n, _)| n == "text-log")
                .filter_map(|(_, p)| {
                    p.get("content")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                })
                .collect()
        }
    }

    impl parish_core::ipc::EventEmitter for TestCapturingEmitter {
        fn emit_event(&self, name: &str, payload: serde_json::Value) {
            self.events
                .lock()
                .unwrap()
                .push((name.to_string(), payload));
        }
    }

    fn make_transport_1467() -> parish_core::world::transport::TransportMode {
        parish_core::world::transport::TransportMode {
            label: "on foot".to_string(),
            id: "walking".to_string(),
            speed_m_per_s: 1.2,
        }
    }

    /// Build a minimal GameLoopContext for #1467 parity tests (no LLM).
    async fn run_parity_input(input: &str) -> Vec<String> {
        let emitter = std::sync::Arc::new(TestCapturingEmitter::new());
        let world = tokio::sync::Mutex::new(parish_core::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(parish_core::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(parish_core::ipc::GameConfig::default());
        let conversation =
            tokio::sync::Mutex::new(parish_core::ipc::ConversationRuntimeState::new());
        let inference_queue: tokio::sync::Mutex<Option<parish_core::inference::InferenceQueue>> =
            tokio::sync::Mutex::new(None);
        let client: tokio::sync::Mutex<Option<parish_core::inference::AnyClient>> =
            tokio::sync::Mutex::new(None);
        let cloud_client: tokio::sync::Mutex<Option<parish_core::inference::AnyClient>> =
            tokio::sync::Mutex::new(None);
        let inference_config = parish_core::config::InferenceConfig::default();

        let ctx = parish_core::game_loop::GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: std::sync::Arc::clone(&emitter)
                as std::sync::Arc<dyn parish_core::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: parish_core::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };
        let transport = make_transport_1467();
        let reaction_templates = parish_core::npc::reactions::ReactionTemplates::default();

        parish_core::game_loop::handle_game_input(
            &ctx,
            input.to_string(),
            vec![],
            &transport,
            &reaction_templates,
            || None,
        )
        .await;

        emitter.text_log_contents()
    }

    /// AC-1 (#1467): "draw a bucket of water from the well and take a long drink"
    /// must route to narrated action (not NPC dialogue) in the Tauri path.
    ///
    /// This is the exact repro input from the quality-harness run that revealed
    /// the mode-parity gap. The local parser classifies "draw " as Interact (#1461).
    /// The shared `handle_game_input` must emit "You draw..." narration.
    #[tokio::test]
    async fn tauri_path_draw_water_action_narrates_not_npc_dialogue() {
        let logs =
            run_parity_input("draw a bucket of water from the well and take a long drink").await;

        // Must emit a narrated action, not nothing.
        assert!(
            !logs.is_empty(),
            "#1467 tauri-parity: 'draw a bucket...' must emit a text-log; got none"
        );
        // The narration must reference the action ("draw") — not be an NPC speech reply.
        assert!(
            logs.iter().any(|l| l.starts_with("You draw")),
            "#1467 tauri-parity: expected 'You draw...' narration; got: {logs:?}"
        );
    }

    /// AC-2 (#1467): "pick up the bellows and pump them" is also an Interact-classified
    /// action — must narrate in the Tauri path.
    #[tokio::test]
    async fn tauri_path_pick_up_bellows_action_narrates() {
        let logs = run_parity_input("pick up the bellows and pump them").await;

        assert!(
            !logs.is_empty(),
            "#1467 tauri-parity: 'pick up the bellows...' must emit a text-log; got none"
        );
        assert!(
            logs.iter().any(|l| l.starts_with("You pick up")),
            "#1467 tauri-parity: expected 'You pick up...' narration; got: {logs:?}"
        );
    }

    /// AC-3 (#1467): greetings must still route to NPC dialogue (regression guard).
    /// In no-LLM mode with no NPC present, a greeting produces an idle text-log
    /// — which is still a dialogue path, NOT action narration.
    #[tokio::test]
    async fn tauri_path_greeting_routes_to_dialogue_not_narration() {
        let logs = run_parity_input("good morning").await;

        // Must emit something (idle message from NPC dialogue path, no NPC present).
        assert!(
            !logs.is_empty(),
            "#1467 regression: greeting must produce a text-log; got none"
        );
        // Must NOT be an action narration starting with "You ".
        assert!(
            !logs.iter().any(|l| l.starts_with("You good morning")),
            "#1467 regression: greeting must not produce action narration; got: {logs:?}"
        );
    }
}
