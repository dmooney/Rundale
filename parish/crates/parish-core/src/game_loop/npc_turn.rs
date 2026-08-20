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
//! - **`player_initiated`**: when `true`, failures carry player-safe recovery
//!   in the authoritative `stream-turn-end`. When `false` (autonomous follow-up
//!   / idle banter), failures are silently logged.
//! - **Loading animation**: controlled by the caller via `spawn_loading`; this
//!   module only cancels the returned token on completion or error.
//! - **Token streaming**: provider batches are internal candidates. Only the
//!   canonical accepted-or-replaced response is emitted as `"stream-token"`.
//!   A `"stream-turn-end"` event follows regardless of success. A single
//!   `"stream-end"` covering the entire chain is emitted by the caller.
//!
//! # Headless CLI
//!
//! `parish-engine`'s `App` uses bare (non-Mutex) fields, so it cannot construct
//! a [`GameLoopContext`].  Its inline implementations remain in `headless.rs`
//! until a follow-up slice wraps `App`'s fields in `Arc<Mutex<>>`.

use std::sync::atomic::Ordering;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::game_loop::{GameInputOutcome, GameLoopContext};
use crate::inference::{
    INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferencePriority, InferenceQueue,
    QueueRequest, await_inference_response,
};
use crate::ipc::{
    ConversationLine, DialogueCorrectedPayload, DialogueGenerationTelemetry,
    DialogueQualityPayload, IDLE_MESSAGES, REQUEST_ID, StreamEndPayload, StreamTokenPayload,
    StreamTurnEndPayload, capitalize_first, text_log, text_log_for_stream_turn, text_log_typed,
};
use crate::npc::NpcId;
use crate::npc::autonomous;
use crate::npc::parse_npc_stream_response_with_disposition;

/// Feature-flag name that gates the autonomous bystander chain in
/// [`handle_npc_conversation`]. Off by default — `FeatureFlags::is_enabled`
/// returns `false` for an unset flag, which matches the desired "opt in" shape.
pub const AUTONOMOUS_NPC_CHAIN_FLAG: &str = "autonomous-npc-chain";

/// Feature-flag name (default **on**) that serializes player-initiated NPC
/// conversation turns against an already-in-flight stream (#1379). When a turn
/// arrives while `conversation_in_progress` is `true`, the new turn is rejected
/// rather than spawning a second, interleaving NPC stream. This is a
/// kill-switch: it is enforced unless explicitly disabled
/// (`flags.is_disabled(SERIALIZE_TURN_STREAM_FLAG)`), so disabling it restores
/// the legacy interleaving behavior for debugging.
pub const SERIALIZE_TURN_STREAM_FLAG: &str = "serialize-turn-stream";

/// Feature-flag name (default **on**) that enables cross-NPC opener
/// de-duplication within a single multi-NPC turn (#1422). When multiple NPCs
/// reply in one turn, small models often open with the same stock phrase. This
/// deterministic, model-agnostic guard strips the duplicated opener from each
/// subsequent NPC's reply. Kill-switch: disable by setting this flag explicitly
/// (`flags.is_disabled(DIALOGUE_ANTI_REPETITION_FLAG)` → true).
pub const DIALOGUE_ANTI_REPETITION_FLAG: &str = "dialogue-anti-repetition";

/// Feature-flag name (default **on**) that enables the acquaintance-question /
/// identity-drift guard (#1504). When the player asks "do you know X?" and the
/// NPC responds with a pure self-identification ("I'm but Seamus Gallagher")
/// instead of answering whether they know the named person, this guard replaces
/// the response with the correct acquaintance answer. Kill-switch only.
pub const ACQUAINTANCE_INTENT_GUARD_FLAG: &str = "npc-acquaintance-intent-guard";

/// Feature-flag name (default **on**) that surfaces the NPC's `action` field
/// as a player-visible stage-direction line alongside the spoken dialogue (#1490).
///
/// When a Tier-1 JSON response carries a non-empty `action` field (e.g.
/// `"nods curtly"`, `"sighs"`) and this flag is enabled, a `text-log` event
/// with `subtype: "action"` is emitted immediately after the dialogue bubble,
/// formatted as `*{NPC name} {action}.*` so the frontend renders it as
/// italicised narration. The action text is ignored when the flag is disabled
/// or when the field is absent/empty. Kill-switch: disable via
/// `flags.is_disabled(NPC_ACTION_NARRATION_FLAG)`.
pub const NPC_ACTION_NARRATION_FLAG: &str = "npc-action-narration";

/// Feature-flag name (default **on**) that emits a `"dialogue-corrected"` event
/// after all post-generation guards run on an NPC turn, replacing the raw
/// streamed model output in the UI with the post-guard canonical text (#1552).
///
/// When a guard alters `parsed.dialogue` (e.g. the verbosity guard collapses a
/// repetition loop, the person-confirmation guard replaces a fabricated name),
/// the event tells the frontend to overwrite the accumulated raw stream tokens
/// with the corrected dialogue so the player sees the same text that is stored
/// in the conversation log and returned by `/api/transcript`.
///
/// Kill-switch: disable via `flags.is_disabled(POST_GUARD_UI_REPLACE_FLAG)`.
pub const POST_GUARD_UI_REPLACE_FLAG: &str = "post-guard-ui-replace";

/// Token cap for Tier 1 dialogue generation.
///
/// Sized so a 2-4 sentence reply plus the JSON envelope (`dialogue`, `action`,
/// `mood`, `language_hints`, `assigned_task`) fits without hitting the
/// provider default and truncating mid-sentence (#982, #1431). vllm-mlx and most
/// OpenAI-compat servers default to a value too low for the structured-output
/// schema once the dialogue runs more than a sentence or two.
///
/// Raised from 512 → 768 (#1431 item 3) after older prompt contracts let
/// metadata consume the budget before the envelope closed. The production
/// prompt no longer requests unused internal monologue. Budget breakdown:
///   - `action` + `mood` + JSON envelope overhead: ~35 tokens
///   - 2-3 sentence Hiberno-English dialogue (~70-110 tokens)
///   - Total observed minimum: ~170 tokens; 768 gives comfortable headroom.
pub const TIER1_DIALOGUE_MAX_TOKENS: u32 = 768;

/// Player-safe recovery used when no complete dialogue crosses the apply seam.
/// Rejected partial provider output is deliberately not interpolated.
pub const DIALOGUE_RETRY_MESSAGE: &str =
    "That reply could not be completed, so its partial response was not added. Please try again.";

/// Output of a single NPC turn.
#[derive(Debug)]
pub struct TurnOutcome {
    /// The spoken line, or `None` if the NPC produced no dialogue.
    pub line: Option<ConversationLine>,
    /// Pronunciation hints extracted from the NPC response.
    pub hints: Vec<crate::npc::LanguageHint>,
    /// Canonical task post-state when this NPC turn assigned a task.
    pub assigned_task: Option<parish_types::PlayerTask>,
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
    let (
        setup,
        person_guard_enabled,
        verbosity_guard_enabled,
        mood_sentence_cap_enabled,
        wrong_location_guard_enabled,
        routing_guard_enabled,
        wrong_speaker_guard_enabled,
        acquaintance_guard_enabled,
        action_narration_enabled,
        anti_repetition_enabled,
        false_denial_guard_enabled,
        invented_place_guard_enabled,
        dialogue_polish_guard_enabled,
        post_guard_ui_replace_enabled,
    ) = {
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
        let person_guard = !config
            .flags
            .is_disabled("dialogue-person-confirmation-guard");
        let verbosity_guard = !config.flags.is_disabled("dialogue-verbosity-guard");
        // Mood-aware sentence cap (#1491): default-on, kill-switch only.
        let mood_sentence_cap = !config.flags.is_disabled("npc-mood-aware-sentence-cap");
        // Wrong-location reference guard (#1477): default-on, kill-switch only.
        let wrong_location_guard = !config.flags.is_disabled("npc-wrong-location-guard");
        // Routing-after-denial guard (#1478): default-on, kill-switch only.
        let routing_guard = !config.flags.is_disabled("dialogue-person-routing-guard");
        // Wrong-speaker-identity guard (#1475): default-on, kill-switch only.
        let wrong_speaker_guard = !config.flags.is_disabled("npc-wrong-speaker-guard");
        // Acquaintance-question intent-drift guard (#1504): default-on, kill-switch only.
        let acquaintance_guard = !config.flags.is_disabled(ACQUAINTANCE_INTENT_GUARD_FLAG);
        // NPC action narration (#1490): default-on, kill-switch only.
        let action_narration = !config.flags.is_disabled(NPC_ACTION_NARRATION_FLAG);
        // Cross-NPC opener de-duplication (#1422, #1492): default-on kill-switch.
        let anti_rep = !config.flags.is_disabled(DIALOGUE_ANTI_REPETITION_FLAG);
        // False-denial guard (#1527, #1528): default-on, kill-switch only.
        let false_denial_guard = !config
            .flags
            .is_disabled(parish_npc::FALSE_DENIAL_GUARD_FLAG);
        // Invented-place confirmation guard (#1530): default-on, kill-switch only.
        let invented_place_guard = !config
            .flags
            .is_disabled(parish_npc::INVENTED_PLACE_GUARD_FLAG);
        // Dialogue polish guard (#1564): default-on, kill-switch only.
        let dialogue_polish_guard = !config
            .flags
            .is_disabled(parish_npc::DIALOGUE_POLISH_GUARD_FLAG);
        // Post-guard UI replace (#1552): emit `dialogue-corrected` event after
        // all guards run so the frontend shows post-guard text, not raw model
        // output. Default-on; kill-switch only.
        let ui_replace = !config.flags.is_disabled(POST_GUARD_UI_REPLACE_FLAG);
        let npc_cfg = crate::config::NpcConfig {
            dialogue_quality_continuity: !config.flags.is_disabled("dialogue-quality-continuity"),
            grounding_enabled: !config.flags.is_disabled("npc-dialogue-grounding"),
            person_confirmation_guard_enabled: person_guard,
            verbosity_guard_enabled: verbosity_guard,
            ..crate::config::NpcConfig::default()
        };
        let setup = crate::ipc::prepare_npc_conversation_turn(
            &world,
            &mut npc_manager,
            prompt_input,
            speaker_id,
            transcript,
            config.improv_enabled,
            &ctx.language,
            &npc_cfg,
        );
        (
            setup,
            person_guard,
            verbosity_guard,
            mood_sentence_cap,
            wrong_location_guard,
            routing_guard,
            wrong_speaker_guard,
            acquaintance_guard,
            action_narration,
            anti_rep,
            false_denial_guard,
            invented_place_guard,
            dialogue_polish_guard,
            ui_replace,
        )
    };
    let mut setup = setup?;
    {
        let mut conversation = ctx.conversation.lock().await;
        conversation.dialogue_referents.observe_player_input(
            prompt_input,
            &setup.grounding.known_person_names,
            &setup.grounding.known_location_names,
            setup.grounding.player_name.as_deref(),
        );
        setup.grounding.referent_context = conversation.dialogue_referents.clone();
        setup.grounding.prior_openers = conversation.seen_openers_this_location.clone();
    }

    let loading_cancel = spawn_loading();

    let (token_tx, token_rx) = mpsc::channel::<String>(crate::ipc::TOKEN_CHANNEL_CAPACITY);
    let display_label = capitalize_first(&setup.display_name);
    let req_id = REQUEST_ID.fetch_add(1, Ordering::SeqCst);
    let inference_profile = ctx
        .config
        .lock()
        .await
        .inference_profile(parish_config::InferenceSubrole::Dialogue);

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

    // The defaults preserve the measured Qwen2.5-14B-4bit workaround:
    // frequency_penalty=0.5 suppresses verbatim repetition loops. Keeping the
    // values in engine config lets each promoted model/backend profile carry
    // the exact sampling parameters that passed its evidence gate.
    let generation = ctx.inference_config.dialogue_generation.for_model(model);
    tracing::debug!(
        model,
        max_tokens = generation.max_tokens,
        temperature = generation.temperature,
        frequency_penalty = generation.frequency_penalty,
        json_mode = generation.json_mode,
        enable_thinking = generation.enable_thinking,
        reasoning_effort = ?generation.reasoning_effort,
        "submitting Tier-1 dialogue generation profile"
    );
    let send_result = queue
        .send(QueueRequest {
            id: req_id,
            model: model.to_string(),
            prompt: setup.context,
            system: Some(setup.system_prompt),
            token_tx: Some(token_tx),
            max_tokens: Some(generation.max_tokens),
            temperature: Some(generation.temperature),
            frequency_penalty: generation.frequency_penalty,
            enable_thinking: generation.enable_thinking,
            reasoning_effort: generation.reasoning_effort,
            priority: InferencePriority::Interactive,
            role: parish_config::InferenceCategory::Dialogue,
            subrole: parish_config::InferenceSubrole::Dialogue,
            profile: Some(inference_profile),
            json_mode: generation.json_mode,
            json_schema: None,
            cancel: None,
        })
        .await;

    let response_rx = match send_result {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("Failed to submit inference request: {}", e);
            ctx.emitter.emit_event(
                "stream-turn-end",
                serde_json::to_value(StreamTurnEndPayload::failed(
                    req_id,
                    Some(message_id.clone()),
                    player_initiated.then(|| DIALOGUE_RETRY_MESSAGE.to_string()),
                ))
                .unwrap_or(serde_json::Value::Null),
            );
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            return None;
        }
    };

    // Drain provider tokens for transport backpressure, but quarantine them.
    // Candidate text is untrusted until the completed response crosses the
    // canonical apply validator; no raw batch is player-renderable (#1834).
    let stream_handle = tokio::spawn(async move {
        let mut token_rx = token_rx;
        while token_rx.recv().await.is_some() {}
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
    if matches!(&outcome, InferenceAwaitOutcome::Response(_)) {
        let _ = stream_handle.await;
    } else {
        stream_handle.abort();
    }

    let response = match outcome {
        InferenceAwaitOutcome::Response(r) => r,
        InferenceAwaitOutcome::Closed => {
            tracing::warn!(
                req_id,
                "NPC inference response channel closed without a reply"
            );
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            ctx.emitter.emit_event(
                "stream-turn-end",
                serde_json::to_value(StreamTurnEndPayload::failed(
                    req_id,
                    Some(message_id.clone()),
                    player_initiated.then(|| DIALOGUE_RETRY_MESSAGE.to_string()),
                ))
                .unwrap_or(serde_json::Value::Null),
            );
            return None;
        }
        InferenceAwaitOutcome::TimedOut { secs } => {
            tracing::warn!(req_id, secs, "NPC inference response timed out");
            if let Some(cancel) = loading_cancel {
                cancel.cancel();
            }
            ctx.emitter.emit_event(
                "stream-turn-end",
                serde_json::to_value(StreamTurnEndPayload::failed(
                    req_id,
                    Some(message_id.clone()),
                    player_initiated.then(|| DIALOGUE_RETRY_MESSAGE.to_string()),
                ))
                .unwrap_or(serde_json::Value::Null),
            );
            return None;
        }
    };

    if response.error.is_some() {
        tracing::warn!("Inference error: {:?}", response.error);
        if let Some(cancel) = loading_cancel {
            cancel.cancel();
        }
        ctx.emitter.emit_event(
            "stream-turn-end",
            serde_json::to_value(StreamTurnEndPayload::failed(
                req_id,
                Some(message_id.clone()),
                player_initiated.then(|| DIALOGUE_RETRY_MESSAGE.to_string()),
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return None;
    }

    if let Some(cancel) = loading_cancel {
        cancel.cancel();
    }

    let (parsed, parse_disposition) = parse_npc_stream_response_with_disposition(&response.text);
    let candidate_dialogue = parsed.dialogue.clone();
    let mut guard_reasons = Vec::new();

    // Player-visible dialogue, set from the shared pipeline's `display_text`.
    let captured_display_text;
    let captured_hints;
    let assigned_task;
    let captured_action;
    let progression_flags = ctx.config.lock().await.flags.clone();
    {
        let mut world = ctx.world.lock().await;
        let game_time = world.clock.now();
        let mut npc_manager = ctx.npc_manager.lock().await;

        // Capture the speaker's location now, while the lock is held, so the
        // dialogue event/log routes by event-time location (#1035). Used as the
        // turn location for the whole shared pipeline below.
        let event_location = npc_manager
            .get(speaker_id)
            .map(|n| n.location())
            .unwrap_or(world.player_location);

        // Shared per-turn pipeline: name detection, Tier-1 apply, conversation
        // log, witness memories, and the `DialogueOccurred` publish (#1172 /
        // #1173). The live loop discards the returned debug-event strings but
        // keeps `display_text` — the guarded (#1228) and length-capped (#1224)
        // dialogue that must be shown to the player, identical to what was
        // stored in the event bus and conversation log.
        let outcome = crate::game_session::apply_npc_dialogue_turn_with_validation(
            &mut world,
            &mut npc_manager,
            speaker_id,
            &parsed,
            parse_disposition,
            &setup.grounding,
            crate::npc::DialogueValidationPolicy {
                person_confirmation: person_guard_enabled,
                person_routing: routing_guard_enabled,
                wrong_location: wrong_location_guard_enabled,
                false_denial: false_denial_guard_enabled,
                invented_place: invented_place_guard_enabled,
                polish: dialogue_polish_guard_enabled,
                verbosity: verbosity_guard_enabled,
                mood_sentence_cap: mood_sentence_cap_enabled,
                wrong_speaker: wrong_speaker_guard_enabled,
                acquaintance_intent: acquaintance_guard_enabled,
                anti_repetition: anti_repetition_enabled,
            },
            prompt_input,
            prompt_input,
            game_time,
            event_location,
            &display_label,
            &setup.npc_name,
            Some(req_id),
            &setup.known_person_names,
            &ctx.language,
            &progression_flags,
        );
        guard_reasons.extend(outcome.guard_reasons);
        if !outcome.accepted_candidate {
            drop(npc_manager);
            drop(world);
            ctx.emitter.emit_event(
                "stream-turn-end",
                serde_json::to_value(StreamTurnEndPayload::failed(
                    req_id,
                    Some(message_id.clone()),
                    player_initiated.then(|| DIALOGUE_RETRY_MESSAGE.to_string()),
                ))
                .unwrap_or(serde_json::Value::Null),
            );
            return None;
        }
        captured_display_text = outcome.display_text;
        captured_hints = outcome.language_hints;
        assigned_task = outcome.assigned_task;
        captured_action = outcome.action;
    }
    let guard_intervened = !guard_reasons.is_empty();

    // Record only the canonical opener. Candidate text is never admitted to
    // the cross-turn repetition state (#1834).
    let shown_opener = crate::npc::extract_normalized_opener(&captured_display_text);
    if !shown_opener.is_empty() {
        ctx.conversation.lock().await.record_opener(shown_opener);
    }
    tracing::info!(npc = %display_label, reply = %captured_display_text, "chat [npc]");
    for issue in crate::npc::quality::detect_all_text_issues(&captured_display_text) {
        tracing::warn!(
            site = "npc-reply",
            npc = %display_label,
            kind = issue.kind.as_str(),
            detail = %issue.detail,
            "quality issue in canonical NPC reply"
        );
    }

    // Publish only the canonical accepted-or-replaced line. A single batch
    // preserves the stream protocol and pacing while making transient display
    // of raw provider text impossible.
    ctx.emitter.emit_event(
        "stream-token",
        serde_json::to_value(StreamTokenPayload {
            token: captured_display_text.clone(),
            turn_id: req_id,
            source: display_label.clone(),
            message_id: Some(message_id.clone()),
        })
        .unwrap_or(serde_json::Value::Null),
    );
    ctx.emitter.emit_event(
        "stream-turn-end",
        serde_json::to_value(StreamTurnEndPayload::completed(
            req_id,
            Some(message_id.clone()),
            display_label.clone(),
            captured_display_text.clone(),
        ))
        .unwrap_or(serde_json::Value::Null),
    );
    ctx.emitter.emit_event(
        "dialogue-quality",
        serde_json::to_value(DialogueQualityPayload {
            turn_id: req_id,
            parse_disposition: parse_disposition.as_str().to_string(),
            contract_valid: parse_disposition == crate::npc::NpcResponseParseDisposition::FullJson
                && !candidate_dialogue.trim().is_empty(),
            guard_intervened,
            guard_reasons,
            model: model.to_string(),
            generation: DialogueGenerationTelemetry {
                max_tokens: generation.max_tokens,
                temperature: generation.temperature,
                frequency_penalty: generation.frequency_penalty,
                json_mode: generation.json_mode,
                enable_thinking: generation.enable_thinking,
                reasoning_effort: generation.reasoning_effort,
            },
        })
        .unwrap_or(serde_json::Value::Null),
    );

    // Compatibility correction for clients restoring an older in-flight turn.
    // New turns have already received this same canonical text as their only
    // stream batch, so the event is idempotent (#1552, #1834).
    if post_guard_ui_replace_enabled && guard_intervened {
        tracing::debug!(
            npc = %display_label,
            req_id,
            "guards altered dialogue — emitting dialogue-corrected (#1552)"
        );
        ctx.emitter.emit_event(
            "dialogue-corrected",
            serde_json::to_value(DialogueCorrectedPayload {
                turn_id: req_id,
                corrected_text: captured_display_text.clone(),
                message_id: Some(message_id.clone()),
            })
            .unwrap_or(serde_json::Value::Null),
        );
    }

    // NPC action narration (#1490): if the model supplied a non-empty `action`
    // field (e.g. "nods curtly", "sighs"), emit it as a player-visible
    // stage-direction alongside the dialogue. The `subtype: "action"` tag
    // matches the existing pattern used by arrival-reaction Gesture events
    // (see `stream_reaction_texts` in `game_session.rs` and movement.rs), so
    // the frontend renders it as italicised narration in the system style.
    //
    // Format: `*{NPC name} {action}.*` — the asterisks trigger the frontend's
    // `parseEmotes` path (italic span) and the trailing period normalises
    // sentences that omit it. Emitted only when the flag is on (default) and
    // the action field is non-empty after trimming.
    if action_narration_enabled {
        let action_text = captured_action.as_deref().unwrap_or("");
        if !action_text.is_empty() {
            // Normalise to a period if the action text doesn't already end with
            // sentence-ending punctuation, so the line reads as a complete clause.
            let punct = if action_text
                .chars()
                .last()
                .is_some_and(|c| matches!(c, '.' | '!' | '?'))
            {
                ""
            } else {
                "."
            };
            let narration = format!("*{display_label} {action_text}{punct}*");
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log_typed(&display_label, narration, "action"))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }

    // Note: the on-disk chat transcript is fed from the `GameEvent` bus
    // (see `chat_transcript::ChatTranscriptLog::process_event`), not from a
    // direct hook here — the `DialogueOccurred` event published above carries
    // `request_id` for the inference-log correlation.
    //
    // The ConversationLine shown to the player is the `display_text` returned by
    // `apply_npc_dialogue_turn`, so the anti-repetition guard (#1228) and the
    // display-length cap (#1224) are applied exactly once, in the shared path.
    let line = if captured_display_text.trim().is_empty() {
        None
    } else {
        Some(ConversationLine {
            speaker: display_label,
            text: captured_display_text,
        })
    };

    Some(TurnOutcome {
        line,
        hints: captured_hints,
        assigned_task,
    })
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
    assigned_tasks: &mut Vec<parish_types::PlayerTask>,
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
        assigned_tasks.extend(outcome.assigned_task);
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
) -> GameInputOutcome {
    let trimmed = raw.trim().to_string();

    // #1379 — serialize player turns against in-flight NPC streaming.
    //
    // `conversation_in_progress` is owned by the turn currently streaming. If a
    // second player turn arrives before the first chain emits its terminal
    // `stream-end`, spawning another NPC stream here would interleave the two
    // replies (the long-stream overlap of #1374, the duplicate bubble of
    // #1377). Reject the late turn instead so only one NPC stream is ever live.
    //
    // This is the cross-runtime enforcement point (rule #2/#12): every entry
    // point (Tauri, server, CLI) reaches NPC dialogue through this shared
    // function, so the guard holds for all of them — the frontend
    // `streamingActive` gate is now a UX convenience, not the sole safeguard.
    //
    // Kill-switch flag (default on): `flags.is_disabled(...)` restores the
    // legacy interleaving behavior when explicitly disabled (rule #6).
    //
    // When enabled we atomically claim the conversation here via
    // `try_begin_turn` (check-and-set under one lock acquisition), so two
    // concurrent turns can never both pass the guard. The claim is released by
    // `end_turn()` on every early-return path below that bails *before*
    // streaming starts, and by the normal terminal block once the chain ends.
    let serialize_turns = !ctx
        .config
        .lock()
        .await
        .flags
        .is_disabled(SERIALIZE_TURN_STREAM_FLAG);
    if serialize_turns && !ctx.conversation.lock().await.try_begin_turn() {
        tracing::debug!(
            "dropping player turn: an NPC conversation is already streaming (#1379 turn serialization)"
        );
        return GameInputOutcome::default();
    }

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

    // Releases the turn claim taken above when bailing out before any NPC
    // stream begins, so a non-dialogue / no-target outcome doesn't wedge the
    // conversation as permanently "in progress" (#1379).
    let release_claim = || async {
        if serialize_turns {
            ctx.conversation.lock().await.end_turn();
        }
    };

    if !npc_present && absent.is_empty() {
        release_claim().await;
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
        return GameInputOutcome::default();
    }

    if trimmed.is_empty() {
        release_claim().await;
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log(
                "system",
                "There are ears enough for ye here, but say something first.",
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return GameInputOutcome::default();
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
        release_claim().await;
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
        } else {
            // #1493: all named targets are absent. The player may have typed a
            // farewell ("Goodbye, Mary") to someone who has already departed.
            // Emit the player's own line so it appears in the log, then follow
            // with a graceful system message so the interaction is not silent.
            if !trimmed.is_empty() {
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log_typed("You", &trimmed, "dialogue"))
                        .unwrap_or(serde_json::Value::Null),
                );
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log("system", "They've already gone."))
                        .unwrap_or(serde_json::Value::Null),
                );
            }
        }
        return GameInputOutcome::default();
    }

    let Some(queue) = queue else {
        release_claim().await;
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log(
                "system",
                "There's someone here, but the LLM is not configured — set a provider with /provider.",
            ))
            .unwrap_or(serde_json::Value::Null),
        );
        return GameInputOutcome::default();
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
    let mut assigned_tasks = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;
    let mut dialogue_failure = None;

    // Phase 1: each addressed NPC takes one turn in the order named.
    // Cross-NPC opener de-duplication (#1422, #1492) is now applied inside
    // `run_npc_turn` (before `apply_npc_dialogue_turn` publishes the
    // `DialogueOccurred` event), so the event and conversation log carry the
    // already-deduped text. The session-level `seen_openers_this_location` set
    // in `ctx.conversation` accumulates across both turns within this call and
    // across successive calls (when the callers share the same `conversation`
    // Mutex — e.g. the real-loop test harness).
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
            dialogue_failure = Some(DIALOGUE_RETRY_MESSAGE.to_string());
            break;
        };

        combined_hints.extend(outcome.hints);
        assigned_tasks.extend(outcome.assigned_task);
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
        &mut assigned_tasks,
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
    GameInputOutcome {
        task_mutations: assigned_tasks,
        dialogue_failure,
    }
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
) -> GameInputOutcome {
    // Feature gate (default-off). Bail before any inference work, but still
    // bump the idle cooldown so inactivity ticks back off instead of
    // re-entering every second — otherwise the server/Tauri wrappers re-emit
    // world-update snapshots on every tick while the player sits idle.
    if !ctx.config.lock().await.flags.is_enabled("npc-idle-banter") {
        ctx.conversation.lock().await.last_spoken_at = std::time::Instant::now();
        return GameInputOutcome::default();
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
        return GameInputOutcome::default();
    };
    if speakers.is_empty() {
        return GameInputOutcome::default();
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
    let mut assigned_tasks = Vec::new();
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
        assigned_tasks.extend(outcome.assigned_task);
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
        &mut assigned_tasks,
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
    GameInputOutcome {
        task_mutations: assigned_tasks,
        dialogue_failure: None,
    }
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
        npc.set_location(player_loc);
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
        npc.set_location(player_loc);
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
        peig.set_location(player_loc);
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

    /// Regression for #1532: the named-absent feedback must win even when the
    /// player's current location has no co-located NPCs. The generic no-NPC
    /// idle branch used to run first, hiding the more specific target result.
    #[tokio::test]
    async fn addressed_absent_npc_emits_system_message_when_location_empty() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());

        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut priest = Npc::new_test_npc();
        priest.id = crate::npc::NpcId(10);
        priest.name = "Fr. Declan Tierney".to_string();
        priest.occupation = "Parish Priest".to_string();
        priest.set_location(crate::world::LocationId(player_loc.0 + 1));
        npc_mgr.add_npc(priest);

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
            "Is Father Declan here?".to_string(),
            vec!["Fr. Declan Tierney".to_string()],
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        assert!(
            events.iter().any(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("content")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s.contains("Fr. Declan Tierney is not here."))
                    && payload
                        .get("source")
                        .and_then(|v| v.as_str())
                        .is_some_and(|s| s == "system")
            }),
            "expected targeted absence message in an empty location; got events: {:#?}",
            events.iter().collect::<Vec<_>>(),
        );
        assert!(
            !events.iter().any(|(name, _)| name == "stream-token"),
            "expected no NPC stream when the only addressed target is absent"
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
        peig.set_location(player_loc);
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
        npc.set_location(player_loc);
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

    // ── #1379 turn-stream serialization guard ────────────────────────────────

    /// AC1/AC2: when a conversation is already streaming
    /// (`conversation_in_progress == true`), a newly arriving dialogue turn is
    /// rejected before any work — no events, and the in-flight claim is left
    /// untouched (still owned by the first turn).
    #[tokio::test]
    async fn rejects_turn_while_stream_in_flight() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(player_loc);
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        // Pre-mark a turn as already in flight (the first stream owns the claim).
        let mut conv = ConversationRuntimeState::new();
        conv.conversation_in_progress = true;
        let conversation = tokio::sync::Mutex::new(conv);
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

        assert!(
            emitter.event_names().is_empty(),
            "a turn arriving mid-stream must emit nothing (no second NPC stream, no stream-end); got {:?}",
            emitter.event_names()
        );
        assert!(
            ctx.conversation.lock().await.conversation_in_progress,
            "the first turn's in-flight claim must stay owned — the late turn must not clear it"
        );
    }

    /// AC3: with the kill-switch flag explicitly disabled, the guard is bypassed
    /// and the late turn proceeds (here it reaches the no-LLM branch, proving it
    /// was NOT short-circuited by the serialization guard).
    #[tokio::test]
    async fn disabled_flag_restores_interleaving() {
        use crate::npc::Npc;
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(player_loc);
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let mut cfg = GameConfig::default();
        cfg.flags.disable(super::SERIALIZE_TURN_STREAM_FLAG);
        let config = tokio::sync::Mutex::new(cfg);
        let mut conv = ConversationRuntimeState::new();
        conv.conversation_in_progress = true; // a stream is "in flight"
        let conversation = tokio::sync::Mutex::new(conv);
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
            "with the guard disabled the turn must proceed past serialization (reaches no-LLM branch)"
        );
    }

    /// AC1 release: a turn that bails out before streaming (here: no NPC
    /// present) must release the claim it took, so the conversation is not
    /// wedged permanently "in progress".
    #[tokio::test]
    async fn guard_claim_released_when_no_npc_present() {
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

        assert!(
            !ctx.conversation.lock().await.conversation_in_progress,
            "a no-target turn must release the serialization claim it took"
        );
    }

    /// AC1 atomicity: two concurrent turns racing on an idle conversation —
    /// only one may claim it via `try_begin_turn`.
    #[test]
    fn try_begin_turn_is_exclusive() {
        let mut conv = ConversationRuntimeState::new();
        assert!(conv.try_begin_turn(), "first claim wins");
        assert!(
            !conv.try_begin_turn(),
            "second claim while in flight must be refused"
        );
        conv.end_turn();
        assert!(conv.try_begin_turn(), "after release, a new turn may claim");
    }

    /// AC-4 (#1431 item 3): the Tier 1 token budget must be large enough to
    /// fit a complete 2-3 sentence dialogue reply plus the full JSON envelope
    /// (`dialogue`, `action`, `mood`, `internal_thought`, `language_hints`).
    /// At 512 the budget was consumed by metadata fields before `dialogue`
    /// finished, causing mid-sentence cutoffs.
    #[test]
    fn tier1_dialogue_max_tokens_is_adequate() {
        // 768 tokens: ~25 for internal_thought, ~35 for envelope overhead,
        // ~110 for 2-3 sentence dialogue at ~4 chars/token — leaves headroom.
        // Read through a local variable so clippy does not flag a const comparison.
        let budget: u32 = super::TIER1_DIALOGUE_MAX_TOKENS;
        assert_eq!(
            budget,
            crate::config::DialogueGenerationConfig::default().max_tokens,
            "the public compatibility constant and configurable default must not drift"
        );
        assert!(
            budget >= 768,
            "TIER1_DIALOGUE_MAX_TOKENS must be >= 768 to prevent mid-sentence \
             truncation when metadata fields precede dialogue in the JSON output \
             (fix #1431 item 3); current value: {budget}"
        );
    }

    /// #1552 — post-guard UI replace: when a post-generation guard alters the
    /// NPC dialogue, `run_npc_turn` must emit a `"dialogue-corrected"` event
    /// carrying the canonical text as a compatibility signal. The renderable
    /// stream itself must already contain only that canonical text (#1834).
    ///
    /// The test uses a fake inference worker that immediately responds with a
    /// 5-sentence dialogue (above the 4-sentence cap enforced by the verbosity
    /// guard), then asserts that:
    ///  1. A `"dialogue-corrected"` event is emitted.
    ///  2. Its `corrected_text` payload is shorter than the raw response.
    ///  3. No `"dialogue-corrected"` event is emitted when the kill-switch is
    ///     disabled (`post-guard-ui-replace` → `false`).
    #[tokio::test]
    async fn post_guard_ui_replace_emits_dialogue_corrected() {
        use crate::inference::{InferenceQueue, InferenceResponse};
        use crate::npc::Npc;

        // Raw dialogue with 5 sentences — above the 4-sentence cap applied by
        // `guard_verbosity_runons` / `cap_sentence_count`.
        let raw_five_sentences = "Good day to ye, friend. \
            The land hereabouts is fair and rich in cattle. \
            Many a family has tilled these fields for generations. \
            The river runs cold in winter and warm come the harvest. \
            Is it not a fine sight to behold the valley at dusk?";

        // JSON-wrapped so parse_npc_stream_response extracts it as `dialogue`.
        let raw_json = format!(
            r#"{{"dialogue": "{raw_five_sentences}", "action": "", "mood": "content", "internal_thought": "", "language_hints": []}}"#
        );

        // Build a fake inference worker that answers every request immediately.
        let (itx, mut irx) = tokio::sync::mpsc::channel::<crate::inference::InferenceRequest>(4);
        let (btx, _) = tokio::sync::mpsc::channel(1);
        let (xtx, _) = tokio::sync::mpsc::channel(1);
        let queue = InferenceQueue::new(itx, btx, xtx);
        let (profile_tx, mut profile_rx) = tokio::sync::mpsc::unbounded_channel();

        // Spawn a task that reads InferenceRequests and answers each with our
        // canned raw_json. The provider batch exercises quarantine and the
        // final response exercises the canonical renderable event.
        let raw_json_clone = raw_json.clone();
        tokio::spawn(async move {
            while let Some(req) = irx.recv().await {
                let _ = profile_tx.send((
                    req.max_tokens,
                    req.temperature,
                    req.frequency_penalty,
                    req.json_mode,
                ));
                // Send the untrusted provider batch; it must be drained without
                // becoming a player-renderable `stream-token`.
                if let Some(tx) = req.token_tx {
                    let _ = tx.send(raw_json_clone.clone()).await;
                }
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: raw_json_clone.clone(),
                    error: None,
                });
            }
        });

        // Build game context with one NPC at the player's location.
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(player_loc);
        let npc_id = npc.id;
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(Some(queue.clone()));
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

        // Run one NPC turn with the fake queue.
        let _outcome = super::run_npc_turn(
            &ctx,
            &queue,
            "test-model",
            npc_id,
            "Good day to you!",
            &[],
            true,
            || None,
        )
        .await;
        assert_eq!(
            profile_rx.recv().await,
            Some((Some(768), Some(0.7), Some(0.5), true)),
            "run_npc_turn must forward the configured generation profile"
        );

        // Scope the std `MutexGuard` in a block so it is structurally dropped
        // before the second `run_npc_turn().await` below — clippy's
        // `await_holding_lock` does not honour an explicit `drop()` here.
        let corrected_text = {
            let events = emitter.events.lock().unwrap();

            let rendered_stream: String = events
                .iter()
                .filter(|(name, _)| name == "stream-token")
                .filter_map(|(_, payload)| payload.get("token").and_then(|value| value.as_str()))
                .collect();
            assert!(
                !rendered_stream.contains(&raw_json),
                "raw provider JSON must remain quarantined from renderable stream events"
            );
            assert!(
                !rendered_stream.contains("Is it not a fine sight to behold the valley at dusk?"),
                "the sentence removed from the over-cap candidate must never appear transiently \
                 in the UI stream"
            );

            // 1. `dialogue-corrected` must be present.
            let corrected = events
                .iter()
                .find(|(name, _)| name == "dialogue-corrected")
                .map(|(_, payload)| payload.clone());
            assert!(
                corrected.is_some(),
                "expected a `dialogue-corrected` event when the verbosity guard \
                 shortens a 5-sentence reply to 4 (fix #1552); emitted events: {:#?}",
                events.iter().map(|(n, _)| n).collect::<Vec<_>>()
            );

            // 2. Extract the corrected text.
            let corrected_text = corrected
                .unwrap()
                .get("corrected_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            assert_eq!(
                rendered_stream, corrected_text,
                "the UI stream must contain only the canonical apply result"
            );
            corrected_text
        };

        assert!(
            corrected_text.len() < raw_five_sentences.len(),
            "corrected_text ({} chars) must be shorter than the raw 5-sentence \
             reply ({} chars); guard did not fire",
            corrected_text.len(),
            raw_five_sentences.len(),
        );

        // 3. Kill-switch: with `post-guard-ui-replace` disabled, no
        //    `dialogue-corrected` event must be emitted.
        let emitter2 = Arc::new(CapturingEmitter::new());
        let world_state2 = WorldState::new();
        let player_loc2 = world_state2.player_location;
        let mut npc_mgr2 = NpcManager::new();
        let mut npc2 = Npc::new_test_npc();
        npc2.set_location(player_loc2);
        let npc_id2 = npc2.id;
        npc_mgr2.add_npc(npc2);

        let world2 = tokio::sync::Mutex::new(world_state2);
        let npc_manager2 = tokio::sync::Mutex::new(npc_mgr2);
        let mut cfg2 = GameConfig::default();
        cfg2.flags.disable(super::POST_GUARD_UI_REPLACE_FLAG);
        let config2 = tokio::sync::Mutex::new(cfg2);
        let conversation2 = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue2 = tokio::sync::Mutex::new(Some(queue.clone()));
        let client2 = tokio::sync::Mutex::new(None);
        let cloud_client2 = tokio::sync::Mutex::new(None);
        let inference_config2 = crate::config::InferenceConfig::default();

        let ctx2 = make_test_ctx!(
            &world2,
            &npc_manager2,
            &config2,
            &conversation2,
            &inference_queue2,
            &client2,
            &cloud_client2,
            &inference_config2,
            Arc::clone(&emitter2) as Arc<dyn EventEmitter>
        );

        let _outcome2 = super::run_npc_turn(
            &ctx2,
            &queue,
            "test-model",
            npc_id2,
            "Good day to you!",
            &[],
            true,
            || None,
        )
        .await;

        let events2 = emitter2.events.lock().unwrap();
        assert!(
            !events2.iter().any(|(name, _)| name == "dialogue-corrected"),
            "with kill-switch disabled, no `dialogue-corrected` event must be emitted; \
             got: {:#?}",
            events2.iter().map(|(n, _)| n).collect::<Vec<_>>()
        );
    }

    /// #1779 regression: live mode must not apply the canonical semantic guards
    /// before calling the shared apply seam. Doing so made a sharp NPC's compact
    /// negative-register prefix appear twice, while headless mode showed it once.
    #[tokio::test]
    async fn canonical_semantic_guard_runs_once_and_corrects_stream_with_final_text() {
        use crate::inference::{InferenceQueue, InferenceResponse};
        use crate::npc::Npc;

        let raw_dialogue = "Good morning. Start by fetching water from the well for the sick \
                            woman in the next street.";
        let expected = "Plainly, then—Start by fetching water from the well for the sick woman \
                        in the next street.";
        let raw_json = format!(
            r#"{{"dialogue": "{raw_dialogue}", "action": "folds her arms", "mood": "content", "internal_thought": "", "language_hints": []}}"#
        );

        let (itx, mut irx) = tokio::sync::mpsc::channel::<crate::inference::InferenceRequest>(4);
        let (btx, _) = tokio::sync::mpsc::channel(1);
        let (xtx, _) = tokio::sync::mpsc::channel(1);
        let queue = InferenceQueue::new(itx, btx, xtx);
        let raw_json_clone = raw_json.clone();
        tokio::spawn(async move {
            while let Some(req) = irx.recv().await {
                if let Some(tx) = req.token_tx {
                    let _ = tx.send(raw_json_clone.clone()).await;
                }
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: raw_json_clone.clone(),
                    error: None,
                });
            }
        });

        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(player_loc);
        npc.mood = "sharp".to_string();
        let npc_id = npc.id;
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(Some(queue.clone()));
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

        let outcome = super::run_npc_turn(
            &ctx,
            &queue,
            "test-model",
            npc_id,
            "Could ye give me one specific job I can begin here now?",
            &[],
            true,
            || None,
        )
        .await
        .expect("canned inference turn should succeed");
        let displayed = outcome
            .line
            .expect("canonical dialogue should be player-visible")
            .text;

        assert_eq!(displayed, expected);
        assert_eq!(
            displayed.matches("Plainly, then—").count(),
            1,
            "the canonical mood guard must run exactly once"
        );

        let events = emitter.events.lock().unwrap();
        let stream_end_index = events
            .iter()
            .position(|(name, _)| name == "stream-turn-end")
            .expect("stream-turn-end should precede correction");
        let corrected_index = events
            .iter()
            .position(|(name, _)| name == "dialogue-corrected")
            .expect("canonical semantic change should correct the raw stream");
        let action_index = events
            .iter()
            .position(|(name, payload)| {
                name == "text-log"
                    && payload.get("subtype").and_then(serde_json::Value::as_str) == Some("action")
            })
            .expect("model action should follow the corrected dialogue event");
        assert!(
            stream_end_index < corrected_index && corrected_index < action_index,
            "event order must remain stream-turn-end → dialogue-corrected → action; got {:#?}",
            events.iter().map(|(name, _)| name).collect::<Vec<_>>()
        );

        let corrected_text = events[corrected_index]
            .1
            .get("corrected_text")
            .and_then(serde_json::Value::as_str)
            .expect("dialogue-corrected should carry canonical text");
        assert_eq!(corrected_text, displayed);
        let terminal: crate::ipc::StreamTurnEndPayload =
            serde_json::from_value(events[stream_end_index].1.clone())
                .expect("terminal payload should deserialize");
        assert_eq!(terminal.status, crate::ipc::StreamTurnStatus::Completed);
        assert_eq!(terminal.final_text.as_deref(), Some(displayed.as_str()));
        let placeholder_id = events
            .iter()
            .find(|(name, payload)| {
                name == "text-log"
                    && payload
                        .get("stream_turn_id")
                        .and_then(serde_json::Value::as_u64)
                        == Some(terminal.turn_id)
            })
            .and_then(|(_, payload)| payload.get("id"))
            .and_then(serde_json::Value::as_str)
            .expect("stream placeholder should carry a reaction target id");
        assert_eq!(terminal.message_id.as_deref(), Some(placeholder_id));
    }

    #[tokio::test]
    async fn failed_provider_turn_discards_partial_and_returns_retry_outcome() {
        use crate::inference::{InferenceQueue, InferenceResponse};
        use crate::npc::Npc;

        let (itx, mut irx) = tokio::sync::mpsc::channel::<crate::inference::InferenceRequest>(1);
        let (btx, _) = tokio::sync::mpsc::channel(1);
        let (xtx, _) = tokio::sync::mpsc::channel(1);
        let queue = InferenceQueue::new(itx, btx, xtx);
        tokio::spawn(async move {
            if let Some(req) = irx.recv().await {
                if let Some(tx) = req.token_tx {
                    let _ = tx
                        .send("forbidden length-terminated partial".to_string())
                        .await;
                }
                let _ = req.response_tx.send(InferenceResponse {
                    id: req.id,
                    text: "forbidden length-terminated partial".to_string(),
                    error: Some(
                        "stream ended without a complete response (finish_reason=length)"
                            .to_string(),
                    ),
                });
            }
        });

        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let mut npc_mgr = NpcManager::new();
        let mut npc = Npc::new_test_npc();
        npc.set_location(player_loc);
        npc_mgr.add_npc(npc);

        let world = tokio::sync::Mutex::new(world_state);
        let npc_manager = tokio::sync::Mutex::new(npc_mgr);
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(Some(queue));
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

        let outcome = super::handle_npc_conversation(
            &ctx,
            "Can ye tell me where to begin?".to_string(),
            Vec::new(),
            || None,
        )
        .await;

        assert_eq!(
            outcome.dialogue_failure.as_deref(),
            Some(super::DIALOGUE_RETRY_MESSAGE)
        );
        assert!(world.lock().await.conversation_log.is_empty());
        let events = emitter.events.lock().unwrap();
        assert!(
            !events.iter().any(|(name, _)| name == "stream-token"),
            "candidate tokens must remain quarantined on failure"
        );
        let (_, payload) = events
            .iter()
            .find(|(name, _)| name == "stream-turn-end")
            .expect("failed turn should emit an authoritative terminal event");
        let terminal: crate::ipc::StreamTurnEndPayload =
            serde_json::from_value(payload.clone()).expect("terminal payload should deserialize");
        assert_eq!(terminal.status, crate::ipc::StreamTurnStatus::Failed);
        assert!(terminal.final_text.is_none());
        assert_eq!(
            terminal.recovery_message.as_deref(),
            Some(super::DIALOGUE_RETRY_MESSAGE)
        );
    }
}
