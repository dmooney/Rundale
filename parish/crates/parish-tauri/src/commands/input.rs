//! Player input submission — classification, validation, and dispatch.

use std::sync::Arc;

use parish_core::input::{InputResult, classify_input, parse_intent};
use parish_core::ipc::text_log;
use tauri::Emitter;

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
    let text = validate_input_text(&text)?;
    if text.is_empty() {
        return Ok(());
    }
    // #752 — cap addressed_to to prevent unbounded memory/allocation via the
    // NPC-addressing chip list.  Max 10 entries; each name ≤ 100 chars.
    validate_addressed_to(&addressed_to)?;

    touch_player_activity(state).await;

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
            handle_system_command(cmd, state, app).await;
        }
        InputResult::GameInput(raw) => {
            tracing::info!(input = %raw, "chat [player]");
            // Emit the player's own text as a dialogue bubble only for actual dialogue
            let player_msg = text_log("player", format!("> {}", raw));
            let player_msg_id = player_msg.id.clone();
            let _ = app.emit(EVENT_TEXT_LOG, player_msg);
            let raw_for_reactions = raw.clone();
            // Capture location before handle_game_input (which may move the player).
            let reaction_location = state.world.lock().await.player_location;
            handle_game_input(raw, addressed_to, state, app.clone()).await;
            // Generate NPC reactions to the player's message in the background.
            super::reactions::emit_npc_reactions(
                &player_msg_id,
                &raw_for_reactions,
                reaction_location,
                state,
                app,
            );
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
/// in [`validate_addressed_to`] and reused by `handle_game_input` to bound
/// `Vec::with_capacity` calls (#933 — CodeQL `rust/uncontrolled-allocation-size`).
pub(crate) const MAX_ADDRESSED_TO: usize = 10;

/// Upper bound for the merged `addressed_to + mentions` target list. Sized
/// generously above the realistic combined total — `addressed_to` is capped
/// at [`MAX_ADDRESSED_TO`] and `mentions.names.len()` is bounded by NPCs in
/// the world — so the allocation is guaranteed-small regardless of input.
pub(crate) const MAX_TARGETS: usize = 64;

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

/// Handles `/command` inputs.
///
/// Delegates to [`parish_core::game_loop::handle_system_command`] via the
/// [`TauriCommandHost`] adapter (#696 slice 7).
async fn handle_system_command(
    cmd: parish_core::input::Command,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use crate::command_host::TauriCommandHost;
    use parish_core::game_loop::handle_system_command as shared_handle;

    let host = TauriCommandHost::new(Arc::clone(state), app.clone());
    shared_handle(&host, cmd).await;
}

/// Handles free-form game input: parses intent (with LLM fallback) then dispatches.
///
/// Takes plain `&Arc<AppState>` (not `tauri::State<...>`) so the body can be
/// called from non-Tauri-extractor contexts — namely the `mcp_bridge` Axum
/// handlers, which share the same live AppState as the desktop window. The
/// Tauri callsite passes `state.inner()`.
pub(crate) async fn handle_game_input(
    raw: String,
    addressed_to: Vec<String>,
    state: &Arc<AppState>,
    app: tauri::AppHandle,
) {
    use parish_core::config::InferenceCategory;

    // Resolve the intent client and model (Intent category override, or base).
    let (client, model) = {
        let config = state.config.lock().await;
        let base_client = state.client.lock().await;
        config.resolve_category_client(InferenceCategory::Intent, base_client.as_ref())
    };

    // Parse intent: tries local keywords first, then LLM for ambiguous input.
    let intent = if let Some(client) = &client {
        // Capture generation before releasing the lock so we can detect TOCTOU
        // races on re-acquire (issue #283).
        let gen_before = {
            let mut world = state.world.lock().await;
            world.clock.inference_pause();
            world.tick_generation
        };
        let result = parse_intent(client, &raw, &model).await;
        {
            let mut world = state.world.lock().await;
            world.clock.inference_resume();
            let gen_after = world.tick_generation;
            if gen_after != gen_before {
                tracing::warn!(
                    gen_before,
                    gen_after,
                    "World advanced during intent parse (TOCTOU #283) — \
                     {} tick(s) elapsed; proceeding with parsed intent",
                    gen_after.wrapping_sub(gen_before),
                );
                let _ = app.emit(
                    crate::events::EVENT_TEXT_LOG,
                    text_log(
                        "system",
                        "The world shifted while your words were in the air.",
                    ),
                );
            }
        }
        result.ok()
    } else {
        // No client configured — use local keyword parsing only.
        parish_core::input::parse_intent_local(&raw)
    };

    let is_move = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Move))
        .unwrap_or(false);
    let is_look = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Look))
        .unwrap_or(false);
    let is_talk = intent
        .as_ref()
        .map(|i| matches!(i.intent, parish_core::input::IntentKind::Talk))
        .unwrap_or(false);
    let move_target = intent
        .as_ref()
        .filter(|_i| is_move)
        .and_then(|i| i.target.clone());
    let talk_target = intent
        .as_ref()
        .filter(|_i| is_talk)
        .and_then(|i| i.target.clone());

    if is_move {
        if let Some(target) = move_target {
            super::movement::handle_movement(&target, state, &app).await;
        } else {
            let _ = app.emit(
                EVENT_TEXT_LOG,
                crate::events::TextLogPayload {
                    id: String::new(),
                    stream_turn_id: None,
                    source: "system".into(),
                    content: "And where would ye be off to?".to_string(),
                    subtype: None,
                },
            );
        }
        return;
    }

    if is_look {
        super::movement::handle_look(state, &app).await;
        return;
    }

    let mentions = {
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        parish_core::ipc::extract_npc_mentions(&raw, &world, &npc_manager)
    };

    // Chip selections (real names from the frontend) come first, then names
    // detected in the player's text, then the LLM's single talk target when it
    // supplied one. Deduping happens in `resolve_npc_targets` via
    // `find_by_name`, which matches both real and display names.
    // Pre-allocate at the fixed upper bound so the allocation argument is a
    // constant — independent of any user-controlled input — and CodeQL's
    // data-flow analyzer can see that.
    let mut targets: Vec<String> = Vec::with_capacity(MAX_TARGETS);
    for name in addressed_to {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }
    for name in mentions.names {
        if !targets.iter().any(|t| t == &name) {
            targets.push(name);
        }
    }
    if is_talk
        && let Some(target) = talk_target
        && !targets.iter().any(|t| t == &target)
    {
        targets.push(target);
    }

    handle_npc_conversation(mentions.remaining, targets, state, app).await;
}

/// Routes input to one or more NPCs at the player's location, or shows an idle message.
///
/// Delegates to [`parish_core::game_loop::handle_npc_conversation`] for all
/// shared logic (#696), then emits a world-update snapshot when inference
/// finishes.
async fn handle_npc_conversation(
    raw: String,
    target_names: Vec<String>,
    state: &Arc<AppState>,
    app: tauri::AppHandle,
) {
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

    let app_for_loading = app.clone();
    let spawn_loading = move || {
        let cancel = tokio_util::sync::CancellationToken::new();
        crate::events::spawn_loading_animation(app_for_loading.clone(), cancel.clone());
        Some(cancel)
    };

    super::snapshot::emit_world_update(state, &app).await;
    parish_core::game_loop::handle_npc_conversation(&ctx, raw, target_names, spawn_loading).await;
    super::snapshot::emit_world_update(state, &app).await;
}

/// Delegates to [`parish_core::game_loop::run_idle_banter`] for all shared
/// logic (#696), then emits a world-update snapshot when the sequence ends.
pub(super) async fn run_idle_banter(state: &Arc<AppState>, app: &tauri::AppHandle) {
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
    parish_core::game_loop::run_idle_banter(&ctx, || None).await;
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
}
