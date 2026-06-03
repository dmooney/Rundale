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
//! `parish-engine`'s `App` uses bare (non-Mutex) fields, so it cannot construct
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

/// Feature-flag name that gates the autonomous bystander chain in
/// [`handle_npc_conversation`]. Off by default — `FeatureFlags::is_enabled`
/// returns `false` for an unset flag, which matches the desired "opt in" shape.
pub const AUTONOMOUS_NPC_CHAIN_FLAG: &str = "autonomous-npc-chain";

/// Token cap for Tier 1 dialogue generation.
///
/// Sized so a 2-4 sentence reply plus the JSON envelope (`dialogue`, `action`,
/// `mood`, `internal_thought`, `language_hints`) fits without hitting the
/// provider default and truncating mid-sentence (#982). vllm-mlx and most
/// OpenAI-compat servers default to a value too low for the structured-output
/// schema once the dialogue runs more than a sentence or two.
pub const TIER1_DIALOGUE_MAX_TOKENS: u32 = 512;

/// Output of a single NPC turn.
#[derive(Debug)]
pub struct TurnOutcome {
    /// The spoken line, or `None` if the NPC produced no dialogue.
    pub line: Option<ConversationLine>,
    /// Pronunciation hints extracted from the NPC response.
    pub hints: Vec<crate::npc::LanguageHint>,
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
            &ctx.language,
        )
    }?;

    let loading_cancel = spawn_loading();

    let (token_tx, token_rx) = mpsc::channel::<String>(crate::ipc::TOKEN_CHANNEL_CAPACITY);
    let display_label = capitalize_first(&setup.display_name);
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);

    // Build the placeholder first so we can capture its message id: a stream
    // that resumes after a WebSocket reconnect carries this id on every
    // `stream-token` so the client can rebind to the reactable `textLog`
    // entry even after `StreamManager.reset()` discarded its only copy (#1164).
    let placeholder = text_log_for_stream_turn(display_label.clone(), String::new(), req_id);
    let message_id = placeholder.id.clone();
    ctx.emitter.emit_event(
        "text-log",
        serde_json::to_value(placeholder).unwrap_or(serde_json::Value::Null),
    );

    // TODO #10 / #23 / #34: Qwen2.5-14B-4bit degenerates into verbatim
    // repetition loops ("'Tis a place of steady X, but not without its Y"
    // x12, trailing-question chains, "'Tis not just X, but Y" stutters)
    // without a sampling penalty. `frequency_penalty = 0.5` breaks the
    // loop on vllm-mlx / OpenAI / OpenRouter; Anthropic + Simulator
    // ignore the field. Only Tier 1 dialogue sets this; Tier 2/3/intent
    // /reaction stay at `None` so behaviour there is unchanged.
    let send_result = queue
        .send_with_penalty(
            req_id,
            model.to_string(),
            setup.context,
            Some(setup.system_prompt),
            Some(token_tx),
            Some(TIER1_DIALOGUE_MAX_TOKENS),
            Some(0.7),
            Some(0.5),
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
    let stream_message_id = message_id.clone();
    let stream_handle = tokio::spawn(async move {
        crate::ipc::stream_npc_tokens(token_rx, |batch| {
            emitter_clone.emit_event(
                "stream-token",
                serde_json::to_value(StreamTokenPayload {
                    token: batch.to_string(),
                    turn_id: req_id,
                    source: source.clone(),
                    message_id: Some(stream_message_id.clone()),
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
            let msg = if ctx.inference_failure_messages.is_empty() {
                let idx = response.id as usize % INFERENCE_FAILURE_MESSAGES.len();
                INFERENCE_FAILURE_MESSAGES[idx].to_string()
            } else {
                let idx = response.id as usize % ctx.inference_failure_messages.len();
                ctx.inference_failure_messages[idx].clone()
            };
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", &msg)).unwrap_or(serde_json::Value::Null),
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

    if !parsed.dialogue.trim().is_empty() {
        tracing::info!(
            npc = %display_label,
            reply = %parsed.dialogue,
            "chat [npc]"
        );
        for issue in crate::npc::quality::detect_all_text_issues(&parsed.dialogue) {
            tracing::warn!(
                site = "npc-reply",
                npc = %display_label,
                kind = issue.kind.as_str(),
                detail = %issue.detail,
                "quality issue in NPC reply"
            );
        }
    }

    {
        let mut world = ctx.world.lock().await;
        let game_time = world.clock.now();
        let mut npc_manager = ctx.npc_manager.lock().await;

        // Capture the speaker's event-time location and canonical name while
        // the locks are held, so the dialogue routes by event-time location
        // (#1035) and records under the right speaker name.
        let event_location = npc_manager
            .get(speaker_id)
            .map(|n| n.location)
            .unwrap_or(world.player_location);
        let actual_name = npc_manager
            .get(speaker_id)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| display_label.clone());

        // Shared per-turn chokepoint: name detection, tier-1 memory, the
        // conversation-log exchange, witness memories, and the verbatim
        // DialogueOccurred publish — identical across every entry point so the
        // paths can never drift again (#1173). The publish is internally
        // skipped only when both the player line and the reply are empty.
        let _ = crate::game_session::apply_npc_turn(
            &mut world,
            &mut npc_manager,
            crate::game_session::NpcTurnInput {
                npc_id: speaker_id,
                parsed: &parsed,
                player_input: prompt_input,
                player_said: prompt_input,
                game_time,
                location: event_location,
                display_name: &display_label,
                actual_name: &actual_name,
                request_id: Some(req_id),
                config: &Default::default(),
            },
        );
    }

    // Note: the on-disk chat transcript is fed from the `GameEvent` bus
    // (see `chat_transcript::ChatTranscriptLog::process_event`), not from a
    // direct hook here — the `DialogueOccurred` event published above carries
    // `request_id` for the inference-log correlation.
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

/// Runs an autonomous NPC conversation chain for up to `chain_cap` turns.
///
/// Picks the next speaker via [`autonomous::pick_next_speaker`], drives each
/// turn through [`run_npc_turn`], and updates the shared conversation state.
///
/// # Arguments
///
/// Argument count is high because this helper replaces two near-identical
/// inline loops that each referenced the same set of local variables.
/// Grouping into a struct would add indirection without improving clarity.
#[allow(clippy::too_many_arguments)]
async fn run_autonomous_chain(
    ctx: &GameLoopContext<'_>,
    queue: &InferenceQueue,
    model: &str,
    chain_cap: usize,
    transcript: &mut Vec<ConversationLine>,
    combined_hints: &mut Vec<crate::npc::LanguageHint>,
    spoken_this_chain: &mut Vec<NpcId>,
    last_speaker: &mut Option<NpcId>,
    targets: &[NpcId],
    prompt: &str,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) {
    for _ in 0..chain_cap {
        let next_speaker_id = {
            let world = ctx.world.lock().await;
            let npc_manager = ctx.npc_manager.lock().await;
            let candidates: Vec<&crate::npc::Npc> = npc_manager.npcs_at(world.player_location);
            autonomous::pick_next_speaker(&candidates, *last_speaker, spoken_this_chain, targets)
                .map(|npc| npc.id)
        };

        let Some(speaker_id) = next_speaker_id else {
            break;
        };

        let Some(outcome) = run_npc_turn(
            ctx,
            queue,
            model,
            speaker_id,
            prompt,
            transcript,
            false,
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
        spoken_this_chain.push(speaker_id);
        *last_speaker = Some(speaker_id);
    }
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

    let (
        npc_present,
        player_location,
        queue,
        model,
        max_follow_up_turns,
        autonomous_chain_enabled,
        targets,
        absent,
    ) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let queue = ctx.inference_queue.lock().await;
        let config = ctx.config.lock().await;
        let npc_present = !npc_manager.npcs_at(world.player_location).is_empty();
        // When the player explicitly addresses someone (chip selection, @mention,
        // or "talk to X"), use the absent-aware resolver so we can tell the
        // player "{name} is not here." instead of letting a different
        // co-located NPC speak for them (#985). For ambient input with no
        // named target, fall back to the first co-located NPC as before.
        let (targets, absent) = if target_names.is_empty() {
            (
                crate::ipc::resolve_npc_targets(&world, &npc_manager, &target_names),
                Vec::new(),
            )
        } else {
            let resolved =
                crate::ipc::resolve_addressed_targets(&world, &npc_manager, &target_names);
            (resolved.resolved, resolved.absent)
        };
        (
            npc_present,
            world.player_location,
            queue.clone(),
            config.model_name.clone(),
            config.max_follow_up_turns,
            config.flags.is_enabled(AUTONOMOUS_NPC_CHAIN_FLAG),
            targets,
            absent,
        )
    };

    if !npc_present {
        let msg = if ctx.idle_messages.is_empty() {
            let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % IDLE_MESSAGES.len();
            IDLE_MESSAGES[idx].to_string()
        } else {
            let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % ctx.idle_messages.len();
            ctx.idle_messages[idx].clone()
        };
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log("system", &msg)).unwrap_or(serde_json::Value::Null),
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

    // If the player named one or more absent NPCs, tell them so by name —
    // never let an unrelated co-located NPC speak in the absent NPC's place
    // (#985). Emit one "{name} is not here." line per absent target. This
    // must fire before the LLM-not-configured short-circuit so the player
    // gets useful feedback even when no inference provider is set.
    //
    // Also publish a `GameEvent::AddressedAbsentNpc` so character + location
    // log writers capture the missed introduction in the persisted
    // markdown — the UI text-log emission alone is ephemeral (#1135 / F9).
    if !absent.is_empty() {
        let world = ctx.world.lock().await;
        let now = world.clock.now();
        for name in &absent {
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", format!("{name} is not here.")))
                    .unwrap_or(serde_json::Value::Null),
            );
            world
                .event_bus
                .publish(parish_types::GameEvent::AddressedAbsentNpc {
                    name: name.clone(),
                    location: player_location,
                    timestamp: now,
                });
        }
    }

    if targets.is_empty() {
        // Either the input had no named targets and the location is empty
        // (handled above by `npc_present`), or every named target was absent
        // (already reported via the loop above). Nothing more to say.
        if absent.is_empty() {
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log(
                    "system",
                    "No one here answers to that name just now.",
                ))
                .unwrap_or(serde_json::Value::Null),
            );
        }
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

    let mut combined_hints: Vec<crate::npc::LanguageHint> = Vec::new();
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
    // Gated by the `autonomous-npc-chain` feature flag (off by default) so
    // operators can opt into bystander follow-ups. With the flag off, only the
    // explicitly addressed NPC(s) in Phase 1 reply.
    let chain_cap = if autonomous_chain_enabled {
        max_follow_up_turns.min(autonomous::MAX_CHAIN_TURNS)
    } else {
        0
    };
    run_autonomous_chain(
        ctx,
        &queue,
        &model,
        chain_cap,
        &mut transcript,
        &mut combined_hints,
        &mut spoken_this_chain,
        &mut last_speaker,
        &targets,
        "listens while the nearby conversation continues",
        || None,
    )
    .await;

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
///
/// Gated by the default-off `npc-idle-banter` feature flag — operators must
/// opt in (`/flag enable npc-idle-banter`) to allow spontaneous chatter.
/// Player-initiated NPC reactions (`npc-llm-reactions`) are unaffected.
pub async fn run_idle_banter(
    ctx: &GameLoopContext<'_>,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) {
    // Feature gate (default-off). Bail before any inference work, but still
    // bump the idle cooldown so inactivity ticks back off instead of
    // re-entering every second — otherwise the server/Tauri wrappers re-emit
    // world-update snapshots on every tick while the player sits idle.
    if !ctx.config.lock().await.flags.is_enabled("npc-idle-banter") {
        ctx.conversation.lock().await.last_spoken_at = std::time::Instant::now();
        return;
    }

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

    let mut combined_hints: Vec<crate::npc::LanguageHint> = Vec::new();
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
    run_autonomous_chain(
        ctx,
        &queue,
        &model,
        chain_cap,
        &mut transcript,
        &mut combined_hints,
        &mut spoken_this_chain,
        &mut last_speaker,
        &[],
        "answers the nearby remark and keeps the local chatter going",
        || None,
    )
    .await;

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
                language: crate::npc::LanguageSettings::english_only(),
                inference_failure_messages: &[],
                idle_messages: &[],
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

    /// Regression test for #985: when the player explicitly addresses an NPC
    /// who is not at the player's location, a system "{name} is not here."
    /// message must be emitted and no NPC inference may be triggered —
    /// crucially, the shared dispatcher must NOT fall back to letting a
    /// different co-located NPC speak as if they were the addressee.
    #[tokio::test]
    async fn addressed_absent_npc_emits_system_message_and_no_npc_reply() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());

        // Construct a world with one NPC (Peig) at the player's location.
        // The player will address "Aoife Brennan" who is not present.
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut peig = Npc::new_test_npc();
        peig.id = crate::npc::NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.location = player_loc;
        npc_mgr.add_npc(peig);
        npc_mgr.mark_introduced(crate::npc::NpcId(1));

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
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
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        super::handle_npc_conversation(
            &ctx,
            "talk to Aoife Brennan about the school".to_string(),
            vec!["Aoife Brennan".to_string()],
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();

        // The "Aoife Brennan is not here." system message must be emitted.
        assert!(
            events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("Aoife Brennan is not here."))
                    && payload
                        .get("source")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s == "system")
            }),
            "expected `Aoife Brennan is not here.` system message; got events: {:#?}",
            events.iter().collect::<Vec<_>>(),
        );

        // No NPC turn was started: the shared dispatcher must NOT emit a
        // `stream-token` or open a `stream-turn-end` for a co-located NPC.
        assert!(
            !events.iter().any(|(name, _)| name == "stream-token"),
            "expected no stream-token events when addressed NPC is absent"
        );
        assert!(
            !events.iter().any(|(name, _)| name == "stream-turn-end"),
            "expected no stream-turn-end events when addressed NPC is absent"
        );

        // The generic "No one here answers to that name just now." message
        // must NOT be used here — we want the more specific "{name} is not
        // here." form whenever the player named a target.
        assert!(
            !events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("No one here answers to that name"))
            }),
            "should emit the targeted absence message, not the generic fallback"
        );
    }

    /// Companion test for #985: when the player explicitly addresses a
    /// co-located NPC, the dispatcher proceeds normally toward that NPC's
    /// turn. We can't run real inference here (no queue configured), so the
    /// assertion is structural: the absent-NPC system message must NOT fire
    /// when the target is present.
    #[tokio::test]
    async fn addressed_present_npc_does_not_emit_absent_message() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());

        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut peig = Npc::new_test_npc();
        peig.id = crate::npc::NpcId(1);
        peig.name = "Peig Hannigan".to_string();
        peig.location = player_loc;
        npc_mgr.add_npc(peig);
        npc_mgr.mark_introduced(crate::npc::NpcId(1));

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
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
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        super::handle_npc_conversation(
            &ctx,
            "talk to Peig Hannigan about the road".to_string(),
            vec!["Peig Hannigan".to_string()],
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        assert!(
            !events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("is not here."))
            }),
            "should NOT emit absent-NPC message when the target IS co-located"
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
                language: crate::npc::LanguageSettings::english_only(),
                inference_failure_messages: &[],
                idle_messages: &[],
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

    /// The `npc-idle-banter` flag is default-off — `run_idle_banter` must
    /// return immediately with no events emitted and no conversation-state
    /// mutation unless the flag has been explicitly enabled. Player-initiated
    /// dialogue paths are unaffected (covered by the other tests in this
    /// module).
    ///
    /// The context is given a *live* (but immediately-closed) inference queue
    /// and a co-located NPC so the guard is actually exercised: if the flag
    /// check were removed, the function would proceed past it and emit events
    /// for the NPC. A `None` queue would mask that by short-circuiting later
    /// regardless of the flag.
    #[tokio::test]
    async fn idle_banter_skipped_by_default() {
        use crate::npc::Npc;

        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.location = player_loc;
        npc_mgr.add_npc(npc);

        // Closed channels: any inference send fails fast so the test never
        // blocks on a response, while keeping the queue `Some` so the flag
        // guard is the only thing standing between entry and event emission.
        let (itx, _) = tokio::sync::mpsc::channel(1);
        let (btx, _) = tokio::sync::mpsc::channel(1);
        let (xtx, _) = tokio::sync::mpsc::channel(1);
        let queue = super::InferenceQueue::new(itx, btx, xtx);

        // GameConfig::default() leaves npc-idle-banter unset → off.
        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(Some(queue));
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let last_spoken_before = conversation.lock().await.last_spoken_at;

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

        super::run_idle_banter(&ctx, || None).await;

        assert!(
            emitter.event_names().is_empty(),
            "expected no events when npc-idle-banter is unset (default-off); got {:?}",
            emitter.event_names()
        );
        let conversation = ctx.conversation.lock().await;
        assert!(
            !conversation.conversation_in_progress,
            "conversation_in_progress must remain false when flag is unset"
        );
        assert!(
            conversation.last_spoken_at > last_spoken_before,
            "disabled path must still bump last_spoken_at so inactivity ticks back off"
        );
    }
}
