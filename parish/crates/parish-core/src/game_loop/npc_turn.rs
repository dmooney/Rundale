//! Shared NPC-turn orchestration — extracted from all backends (#696).
//!
//! Functions here operate identically on every runtime (axum web server,
//! Tauri desktop).  Backend-specific behaviour is injected through:
//!
//! - [`GameLoopContext::emitter`] — every emit call goes through this trait.
//! - A `spawn_loading` callback — callers provide a closure that starts a
//!   loading animation and returns an optional [`CancellationToken`].
//!   Pass `|| None` to disable the animation (autonomous follow-up turns, or
//!   headless mode which does not have a spinner UI).
//!
//! # Behavioural notes
//!
//! - **`player_initiated`**: when `true`, error messages are surfaced to the
//!   player via `text-log`. When `false` (autonomous follow-up / idle banter),
//!   errors are silently logged. This unifies the server behaviour (which had
//!   the flag) with the Tauri runtime (which previously always surfaced errors).
//! - **Loading animation**: controlled by the caller via `spawn_loading`; this
//!   module only cancels the returned token on completion or error.
//! - **Token streaming**: each incoming batch is emitted as `"stream-token"`.
//!   A `"stream-turn-end"` event follows regardless of success. A single
//!   `"stream-end"` covering the entire chain is emitted by the caller.
//!
//! # Headless CLI
//!
//! `parish-cli`'s `App` uses bare (non-Mutex) fields, so it cannot construct
//! a [`GameLoopContext`].  Its inline implementations remain in `headless.rs`
//! until a follow-up slice wraps `App`'s fields in `Arc<Mutex<>>`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::game_loop::GameLoopContext;
use crate::inference::{
    INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferenceQueue,
    await_inference_response,
};
use crate::ipc::{
    ConversationLine, IDLE_MESSAGES, INFERENCE_FAILURE_MESSAGES, REQUEST_ID, StreamEndPayload,
    StreamTokenPayload, StreamTurnEndPayload, capitalize_first, text_log, text_log_for_stream_turn,
};
use crate::npc::NpcId;
use crate::npc::autonomous;
use crate::npc::parse_npc_stream_response;
use crate::npc::ticks::apply_tier1_response_with_config;

/// Output of a single NPC turn.
#[derive(Debug)]
pub struct TurnOutcome {
    /// The spoken line, or `None` if the NPC produced no dialogue.
    pub line: Option<ConversationLine>,
    /// Irish-word pronunciation hints extracted from the NPC response.
    pub hints: Vec<crate::npc::IrishWordHint>,
}

/// Runs a single NPC inference turn and emits all events via `ctx.emitter`.
///
/// Returns `Some(TurnOutcome)` on success, `None` on any failure (channel
/// closed, timeout, inference error).
///
/// # Parameters
///
/// - `ctx`: shared game-loop context (world, NPC manager, config, emitter, …).
/// - `queue`: inference request queue (obtained by the caller before calling).
/// - `model`: model name string for this inference call.
/// - `speaker_id`: which NPC speaks this turn.
/// - `prompt_input`: the triggering player text or autonomous prompt.
/// - `transcript`: recent conversation history for context.
/// - `player_initiated`: `true` when the player typed the input; `false` for
///   autonomous bystander or idle-banter turns.
/// - `spawn_loading`: closure that starts a loading animation and returns an
///   optional [`CancellationToken`].  Pass `|| None` to skip.
#[allow(clippy::too_many_arguments)]
pub async fn run_npc_turn(
    ctx: &GameLoopContext<'_>,
    queue: &InferenceQueue,
    model: &str,
    speaker_id: NpcId,
    prompt_input: &str,
    transcript: &[ConversationLine],
    player_initiated: bool,
    spawn_loading: impl FnOnce() -> Option<CancellationToken>,
) -> Option<TurnOutcome> {
    let setup = {
        let mut world = ctx.world.lock().await;
        let mut npc_manager = ctx.npc_manager.lock().await;
        let config = ctx.config.lock().await;

        // Detect player self-introduction before building the NPC prompt.
        crate::ipc::detect_and_record_player_name(
            &mut world,
            &mut npc_manager,
            prompt_input,
            speaker_id,
        );
        crate::ipc::prepare_npc_conversation_turn(
            &world,
            &mut npc_manager,
            prompt_input,
            speaker_id,
            transcript,
            config.improv_enabled,
        )
    }?;

    let loading_cancel = spawn_loading();

    let (token_tx, token_rx) = mpsc::channel::<String>(crate::ipc::TOKEN_CHANNEL_CAPACITY);
    let display_label = capitalize_first(&setup.display_name);
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    ctx.emitter.emit_event(
        "text-log",
        serde_json::to_value(text_log_for_stream_turn(
            display_label.clone(),
            String::new(),
            req_id,
        ))
        .unwrap_or(serde_json::Value::Null),
    );

    let send_result = queue
        .send(
            req_id,
            model.to_string(),
            setup.context,
            Some(setup.system_prompt),
            Some(token_tx),
            None,
            Some(0.7),
            crate::inference::InferencePriority::Interactive,
            true,
        )
        .await;

    let response_rx = match send_result {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
            ctx.emitter.emit_event(
                "stream-turn-end",
                serde_json::to_value(StreamTurnEndPayload { turn_id: req_id })
                    .unwrap_or(serde_json::Value::Null),
            );
            if player_initiated {
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log(
                        "system",
                        "The parish storyteller has wandered off. Try again.",
                    ))
                    .unwrap_or(serde_json::Value::Null),
                );
            }
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            return None;
        }
    };

    // Stream tokens in a background task while awaiting the final response.
    let emitter_clone = Arc::clone(&ctx.emitter);
    let source = display_label.clone();
    let stream_handle = tokio::spawn(async move {
        crate::ipc::stream_npc_tokens(token_rx, |batch| {
            emitter_clone.emit_event(
                "stream-token",
                serde_json::to_value(StreamTokenPayload {
                    token: batch.to_string(),
                    turn_id: req_id,
                    source: source.clone(),
                })
                .unwrap_or(serde_json::Value::Null),
            );
        })
        .await
    });

    let timeout_secs = {
        let config = ctx.config.lock().await;
        if config.flags.is_disabled("inference-response-timeout") {
            None
        } else {
            Some(INFERENCE_RESPONSE_TIMEOUT_SECS)
        }
    };
    let outcome = await_inference_response(
        response_rx,
        timeout_secs.map(std::time::Duration::from_secs),
    )
    .await;
    let _ = stream_handle.await;

    ctx.emitter.emit_event(
        "stream-turn-end",
        serde_json::to_value(StreamTurnEndPayload { turn_id: req_id })
            .unwrap_or(serde_json::Value::Null),
    );

    let response = match outcome {
        InferenceAwaitOutcome::Response(r) => r,
        InferenceAwaitOutcome::Closed => {
            tracing::warn!(
                req_id,
                "NPC inference response channel closed without a reply"
            );
            if player_initiated {
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log(
                        "system",
                        "The storyteller has wandered off mid-tale.",
                    ))
                    .unwrap_or(serde_json::Value::Null),
                );
            }
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            return None;
        }
        InferenceAwaitOutcome::TimedOut { secs } => {
            tracing::warn!(req_id, secs, "NPC inference response timed out");
            if player_initiated {
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log(
                        "system",
                        "The storyteller is lost in thought. Try again.",
                    ))
                    .unwrap_or(serde_json::Value::Null),
                );
            }
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            return None;
        }
    };

    if response.error.is_some() {
        tracing::warn!("Inference error: {:?}", response.error);
        if player_initiated {
            let idx = response.id as usize % INFERENCE_FAILURE_MESSAGES.len();
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", INFERENCE_FAILURE_MESSAGES[idx]))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        if let Some(cancel) = loading_cancel {
            cancel.cancel();
        }
        return None;
    }

    if let Some(cancel) = loading_cancel {
        cancel.cancel();
    }

    let parsed = parse_npc_stream_response(&response.text);
    let hints = parsed
        .metadata
        .as_ref()
        .map(|meta| meta.language_hints.clone())
        .unwrap_or_default();

    {
        let world = ctx.world.lock().await;
        let game_time = world.clock.now();
        let mut npc_manager = ctx.npc_manager.lock().await;
        let player_name = if npc_manager.knows_player_name(speaker_id) {
            world.player_name.clone()
        } else {
            None
        };
        if let Some(npc) = npc_manager.get_mut(speaker_id) {
            let _ = apply_tier1_response_with_config(
                npc,
                &parsed,
                prompt_input,
                game_time,
                &Default::default(),
                player_name.as_deref(),
            );
        }
    }

    let line = if parsed.dialogue.trim().is_empty() {
        None
    } else {
        Some(ConversationLine {
            speaker: display_label,
            text: parsed.dialogue,
        })
    };

    Some(TurnOutcome { line, hints })
}

/// Routes input to one or more NPCs at the player's location, or shows an idle
/// message when no NPCs are present.
///
/// Emits `"stream-end"` with combined language hints after the full
/// conversation chain (all addressed NPCs + autonomous follow-up turns).
///
/// # Parameters
///
/// - `ctx`: shared game-loop context.
/// - `raw`: raw player input string.
/// - `target_names`: display names of explicitly addressed NPCs (from chip
///   selection or `@mention` parsing). Empty → fall back to first NPC.
/// - `spawn_loading`: closure that starts a loading animation; called once per
///   player-initiated NPC turn.
pub async fn handle_npc_conversation(
    ctx: &GameLoopContext<'_>,
    raw: String,
    target_names: Vec<String>,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) {
    let trimmed = raw.trim().to_string();

    let (npc_present, player_location, queue, model, max_follow_up_turns, targets) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let queue = ctx.inference_queue.lock().await;
        let config = ctx.config.lock().await;
        let npc_present = !npc_manager.npcs_at(world.player_location).is_empty();
        let targets = crate::ipc::resolve_npc_targets(&world, &npc_manager, &target_names);
        (
            npc_present,
            world.player_location,
            queue.clone(),
            config.model_name.clone(),
            config.max_follow_up_turns,
            targets,
        )
    };

    if !npc_present {
        let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % IDLE_MESSAGES.len();
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log("system", IDLE_MESSAGES[idx]))
                .unwrap_or(serde_json::Value::Null),
        );
        return;
    }

    if trimmed.is_empty() {
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log(
                "system",
                "There are ears enough for ye here, but say something first.",
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return;
    }

    let Some(queue) = queue else {
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log(
                "system",
                "There's someone here, but the LLM is not configured — set a provider with /provider.",
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return;
    };

    if targets.is_empty() {
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log(
                "system",
                "No one here answers to that name just now.",
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return;
    }

    let mut transcript = {
        let mut conversation = ctx.conversation.lock().await;
        conversation.sync_location(player_location);
        conversation.push_line(ConversationLine {
            speaker: "You".to_string(),
            text: trimmed.clone(),
        });
        conversation.transcript.iter().cloned().collect::<Vec<_>>()
    };

    {
        let mut conversation = ctx.conversation.lock().await;
        conversation.conversation_in_progress = true;
    }
    {
        let mut world = ctx.world.lock().await;
        world.clock.inference_pause();
    }

    let mut combined_hints: Vec<crate::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // Phase 1: each addressed NPC takes one turn in the order named.
    for speaker_id in &targets {
        let Some(outcome) = run_npc_turn(
            ctx,
            &queue,
            &model,
            *speaker_id,
            trimmed.as_str(),
            &transcript,
            true,
            &spawn_loading,
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = ctx.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(*speaker_id);
        last_speaker = Some(*speaker_id);
    }

    // Phase 2: autonomous chain via bystander-aware heuristic.
    let chain_cap = max_follow_up_turns.min(autonomous::MAX_CHAIN_TURNS);
    for _ in 0..chain_cap {
        let next_speaker_id = {
            let world = ctx.world.lock().await;
            let npc_manager = ctx.npc_manager.lock().await;
            let candidates: Vec<&crate::npc::Npc> = npc_manager.npcs_at(world.player_location);
            autonomous::pick_next_speaker(&candidates, last_speaker, &spoken_this_chain, &targets)
                .map(|npc| npc.id)
        };

        let Some(speaker_id) = next_speaker_id else {
            break;
        };

        let Some(outcome) = run_npc_turn(
            ctx,
            &queue,
            &model,
            speaker_id,
            "listens while the nearby conversation continues",
            &transcript,
            false,
            || None, // no loading animation for autonomous follow-up turns
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = ctx.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(speaker_id);
        last_speaker = Some(speaker_id);
    }

    {
        let mut world = ctx.world.lock().await;
        world.clock.inference_resume();
    }
    {
        let mut conversation = ctx.conversation.lock().await;
        conversation.conversation_in_progress = false;
    }

    // Single stream-end after the entire chain so the input field stays
    // disabled through every NPC's response (#222).
    ctx.emitter.emit_event(
        "stream-end",
        serde_json::to_value(StreamEndPayload {
            hints: combined_hints,
        })
        .unwrap_or(serde_json::Value::Null),
    );
}

/// Generates spontaneous NPC banter when the player has been idle long enough.
///
/// Picks up to two NPCs sorted by ID and drives a short autonomous exchange
/// (one initial remark + up to `max_follow_up_turns` additional lines, capped
/// at [`autonomous::MAX_CHAIN_TURNS`]).
///
/// Emits `"stream-end"` after the full sequence completes.  Updates
/// `conversation.last_spoken_at` regardless of inference success, creating a
/// cooldown that prevents spam when inference is down.
pub async fn run_idle_banter(
    ctx: &GameLoopContext<'_>,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) {
    let (queue, model, player_location, max_follow_up_turns, speakers) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let queue = ctx.inference_queue.lock().await;
        let config = ctx.config.lock().await;

        let mut speakers = npc_manager.npcs_at_ids(world.player_location);
        speakers.sort_by_key(|id| id.0);
        speakers.truncate(2);

        (
            queue.clone(),
            config.model_name.clone(),
            world.player_location,
            config.max_follow_up_turns.min(2),
            speakers,
        )
    };

    let Some(queue) = queue else {
        return;
    };
    if speakers.is_empty() {
        return;
    }

    let mut transcript = {
        let mut conversation = ctx.conversation.lock().await;
        conversation.sync_location(player_location);
        conversation.transcript.iter().cloned().collect::<Vec<_>>()
    };

    {
        let mut conversation = ctx.conversation.lock().await;
        conversation.conversation_in_progress = true;
    }
    {
        let mut world = ctx.world.lock().await;
        world.clock.inference_pause();
    }

    let mut combined_hints: Vec<crate::npc::IrishWordHint> = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;

    // First spontaneous remark: deterministic ordering so a quiet location
    // with calm NPCs still produces a line.
    if let Some(first_speaker) = speakers.first().copied()
        && let Some(outcome) = run_npc_turn(
            ctx,
            &queue,
            &model,
            first_speaker,
            "breaks the silence with a natural nearby remark",
            &transcript,
            false,
            &spawn_loading,
        )
        .await
    {
        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = ctx.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(first_speaker);
        last_speaker = Some(first_speaker);
    }

    let chain_cap = max_follow_up_turns.min(autonomous::MAX_CHAIN_TURNS);
    for _ in 0..chain_cap {
        let next_speaker_id = {
            let world = ctx.world.lock().await;
            let npc_manager = ctx.npc_manager.lock().await;
            let candidates: Vec<&crate::npc::Npc> = npc_manager.npcs_at(world.player_location);
            autonomous::pick_next_speaker(&candidates, last_speaker, &spoken_this_chain, &[])
                .map(|npc| npc.id)
        };

        let Some(speaker_id) = next_speaker_id else {
            break;
        };

        let Some(outcome) = run_npc_turn(
            ctx,
            &queue,
            &model,
            speaker_id,
            "answers the nearby remark and keeps the local chatter going",
            &transcript,
            false,
            || None,
        )
        .await
        else {
            break;
        };

        combined_hints.extend(outcome.hints);
        if let Some(line) = outcome.line {
            transcript.push(line.clone());
            let mut conversation = ctx.conversation.lock().await;
            conversation.push_line(line);
            conversation.last_spoken_at = std::time::Instant::now();
        }
        spoken_this_chain.push(speaker_id);
        last_speaker = Some(speaker_id);
    }

    {
        let mut world = ctx.world.lock().await;
        world.clock.inference_resume();
    }
    // Update last_spoken_at regardless of success — creates a cooldown so a
    // failed banter attempt does not spam failure messages on every 1s tick.
    {
        let mut conversation = ctx.conversation.lock().await;
        conversation.last_spoken_at = std::time::Instant::now();
        conversation.conversation_in_progress = false;
    }

    ctx.emitter.emit_event(
        "stream-end",
        serde_json::to_value(StreamEndPayload {
            hints: combined_hints,
        })
        .unwrap_or(serde_json::Value::Null),
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub mod tests {
    use std::sync::{Arc, Mutex};

    use crate::ipc::{ConversationRuntimeState, EventEmitter, GameConfig};
    use crate::npc::manager::NpcManager;
    use crate::world::WorldState;

    /// Records all emitted events for assertion.
    pub struct CapturingEmitter {
        pub events: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl Default for CapturingEmitter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl CapturingEmitter {
        pub fn new() -> Self {
            Self {
                events: Arc::new(Mutex::new(Vec::new())),
            }
        }

        /// Returns the list of event names emitted so far.
        pub fn event_names(&self) -> Vec<String> {
            self.events
                .lock()
                .unwrap()
                .iter()
                .map(|(n, _)| n.clone())
                .collect()
        }
    }

    impl EventEmitter for CapturingEmitter {
        fn emit_event(&self, name: &str, payload: serde_json::Value) {
            self.events
                .lock()
                .unwrap()
                .push((name.to_string(), payload));
        }
    }

    macro_rules! make_test_ctx {
        ($world:expr, $npc_manager:expr, $config:expr, $conversation:expr,
         $inference_queue:expr, $client:expr, $cloud_client:expr,
         $inference_config:expr, $emitter:expr) => {
            crate::game_loop::GameLoopContext {
                world: $world,
                npc_manager: $npc_manager,
                config: $config,
                conversation: $conversation,
                inference_queue: $inference_queue,
                emitter: $emitter,
                inference_config: $inference_config,
                pronunciations: &[],
                client: $client,
                cloud_client: $cloud_client,
            }
        };
    }

    #[tokio::test]
    async fn idle_message_when_no_npc_present() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = make_test_ctx!(
            &world,
            &npc_manager,
            &config,
            &conversation,
            &inference_queue,
            &client,
            &cloud_client,
            &inference_config,
            Arc::clone(&emitter) as Arc<dyn EventEmitter>
        );

        super::handle_npc_conversation(&ctx, "hello".to_string(), vec![], || None).await;

        let names = emitter.event_names();
        assert!(
            names.iter().any(|n| n == "text-log"),
            "expected text-log for idle message when no NPC present; got {names:?}"
        );
    }

    #[tokio::test]
    async fn empty_input_message() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.location = player_loc;
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = make_test_ctx!(
            &world,
            &npc_manager,
            &config,
            &conversation,
            &inference_queue,
            &client,
            &cloud_client,
            &inference_config,
            Arc::clone(&emitter) as Arc<dyn EventEmitter>
        );

        super::handle_npc_conversation(&ctx, "   ".to_string(), vec![], || None).await;

        let events = emitter.events.lock().unwrap();
        assert!(
            events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("say something first"))
            }),
            "expected 'say something first' for empty input"
        );
    }

    #[tokio::test]
    async fn no_llm_message() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.location = player_loc;
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None); // No LLM
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = make_test_ctx!(
            &world,
            &npc_manager,
            &config,
            &conversation,
            &inference_queue,
            &client,
            &cloud_client,
            &inference_config,
            Arc::clone(&emitter) as Arc<dyn EventEmitter>
        );

        super::handle_npc_conversation(&ctx, "hello".to_string(), vec![], || None).await;

        let events = emitter.events.lock().unwrap();
        assert!(
            events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("LLM is not configured"))
            }),
            "expected LLM-not-configured message when no queue"
        );
    }

    /// Cross-mode equivalence test (#734): two independent CapturingEmitter
    /// instances receiving the same input must produce identical event-name
    /// sequences, proving the shared orchestration is deterministic.
    #[tokio::test]
    async fn cross_mode_equivalence_no_npc() {
        async fn run() -> Vec<String> {
            let emitter = Arc::new(CapturingEmitter::new());
            let world = tokio::sync::Mutex::new(WorldState::new());
            let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
            let config = tokio::sync::Mutex::new(GameConfig::default());
            let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
            let inference_queue = tokio::sync::Mutex::new(None);
            let client = tokio::sync::Mutex::new(None);
            let cloud_client = tokio::sync::Mutex::new(None);
            let inference_config = crate::config::InferenceConfig::default();

            let ctx = crate::game_loop::GameLoopContext {
                world: &world,
                npc_manager: &npc_manager,
                config: &config,
                conversation: &conversation,
                inference_queue: &inference_queue,
                emitter: Arc::clone(&emitter) as Arc<dyn EventEmitter>,
                inference_config: &inference_config,
                pronunciations: &[],
                client: &client,
                cloud_client: &cloud_client,
            };

            super::handle_npc_conversation(&ctx, "hello".to_string(), vec![], || None).await;
            emitter.event_names()
        }

        let names_a = run().await;
        let names_b = run().await;

        assert_eq!(
            names_a, names_b,
            "cross-mode: event sequences must match across two independent invocations"
        );
    }
}
