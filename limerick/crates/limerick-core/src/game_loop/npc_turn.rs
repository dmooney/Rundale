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
//! `limerick-engine`'s `App` uses bare (non-Mutex) fields, so it cannot construct
//! a [`GameLoopContext`].  Its inline implementations remain in `headless.rs`
//! until a follow-up slice wraps `App`'s fields in `Arc<Mutex<>>`.

use std::sync::atomic::Ordering;

use tokio_util::sync::CancellationToken;

use crate::endpoint_input::Addressing;
use crate::game_loop::recipients::{
    AddressRequest, Answers, PartnerChange, Resolution, resolve_recipients,
};
use crate::game_loop::{AddresseeClarification, GameInputOutcome, GameLoopContext};
use crate::ipc::{
    ConversationLine, DialogueGenerationTelemetry, DialogueQualityPayload, IDLE_MESSAGES,
    REQUEST_ID, StreamEndPayload, StreamTokenPayload, StreamTurnEndPayload, capitalize_first,
    text_log, text_log_for_stream_turn, text_log_typed,
};
use crate::npc::NpcId;
use crate::npc::autonomous;
use crate::npc::parse_npc_stream_response_with_disposition;
use crate::turn_inference::{InferenceCall, InferenceOutcome, ResponseShape, RouteStatus};

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

/// Feature-flag name (default **on**) for addressee clarification: the game
/// asks "Who are you speaking to?" about unaddressed speech with no partner,
/// and "Which Connolly do you mean?" about a reference that fits several
/// people present (design §3.3). Disabled, it never asks, and such a line
/// reaches everyone present instead. Kill-switch: disable via
/// `flags.is_disabled(ADDRESSEE_CLARIFICATION_FLAG)`.
pub const ADDRESSEE_CLARIFICATION_FLAG: &str = "addressee-clarification";

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
    pub assigned_task: Option<limerick_types::PlayerTask>,
}

/// Runs a single NPC inference turn and emits all events via `ctx.emitter`.
///
/// Returns `Some(TurnOutcome)` on success, `None` on any failure (channel
/// closed, timeout, inference error).
///
/// # Parameters
///
/// - `ctx`: shared game-loop context (world, NPC manager, config, emitter, …).
///   The Tier-1 request goes through [`GameLoopContext::inference`].
/// - `speaker_id`: which NPC speaks this turn.
/// - `prompt_input`: the triggering player text or autonomous prompt.
/// - `transcript`: recent conversation history for context.
/// - `player_initiated`: `true` when the player typed the input; `false` for
///   autonomous bystander or idle-banter turns.
/// - `spawn_loading`: closure that starts a loading animation and returns an
///   optional [`CancellationToken`].  Pass `|| None` to skip.
pub async fn run_npc_turn(
    ctx: &GameLoopContext<'_>,
    speaker_id: NpcId,
    prompt_input: &str,
    transcript: &[ConversationLine],
    player_initiated: bool,
    spawn_loading: impl FnOnce() -> Option<CancellationToken>,
) -> Option<TurnOutcome> {
    run_npc_turn_addressed(
        ctx,
        speaker_id,
        prompt_input,
        transcript,
        player_initiated,
        Addressing::SpokenTo,
        spawn_loading,
    )
    .await
}

/// [`run_npc_turn`] for a speaker who came to answer as `addressing` says
/// (spoken to, part of everyone present, or joining as someone mentioned;
/// design §3.1 and §6), which the dialogue Endpoint input carries.
#[allow(clippy::too_many_arguments)]
pub async fn run_npc_turn_addressed(
    ctx: &GameLoopContext<'_>,
    speaker_id: NpcId,
    prompt_input: &str,
    transcript: &[ConversationLine],
    player_initiated: bool,
    addressing: Addressing,
    spawn_loading: impl FnOnce() -> Option<CancellationToken>,
) -> Option<TurnOutcome> {
    let (setup, endpoint, action_narration_enabled) = {
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
        // NPC action narration (#1490): default-on, kill-switch only.
        let action_narration = !config.flags.is_disabled(NPC_ACTION_NARRATION_FLAG);
        let npc_cfg = crate::config::NpcConfig {
            dialogue_quality_continuity: !config.flags.is_disabled("dialogue-quality-continuity"),
            grounding_enabled: !config.flags.is_disabled("npc-dialogue-grounding"),
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
        // Built from the same locked state as the prompt, so both describe
        // one world snapshot.
        let endpoint = crate::endpoint_input::dialogue_endpoint(
            ctx.endpoints,
            &world,
            &npc_manager,
            speaker_id,
            prompt_input,
            addressing,
        );
        (setup, endpoint, action_narration)
    };
    let setup = setup?;

    let loading_cancel = spawn_loading();

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

    // Provider tokens are drained and quarantined by the host: candidate text
    // is untrusted until the completed response crosses the canonical apply
    // validator; no raw batch is player-renderable (#1834).
    let outcome = ctx
        .inference()
        .complete(InferenceCall {
            subrole: limerick_config::InferenceSubrole::Dialogue,
            system: Some(setup.system_prompt),
            prompt: setup.context,
            response: ResponseShape::NpcDialogue,
            correlation_id: Some(req_id),
            endpoint,
        })
        .await;

    let (response_text, report) = match outcome {
        InferenceOutcome::Completed { text, report } => (text, report),
        InferenceOutcome::Failed { .. } => {
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

    if let Some(cancel) = loading_cancel {
        cancel.cancel();
    }

    let (parsed, parse_disposition) = parse_npc_stream_response_with_disposition(&response_text);
    let candidate_dialogue = parsed.dialogue.clone();

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
        // keeps `display_text`: the reply exactly as the model wrote it, which
        // is what was stored in the event bus and conversation log.
        let outcome = crate::game_session::apply_npc_dialogue_turn_with_disposition(
            &mut world,
            &mut npc_manager,
            speaker_id,
            &parsed,
            parse_disposition,
            prompt_input,
            prompt_input,
            game_time,
            event_location,
            &display_label,
            &setup.npc_name,
            Some(req_id),
            &ctx.language,
            &progression_flags,
        );
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
            model: report.model.clone(),
            generation: report
                .generation
                .as_ref()
                .map(|generation| DialogueGenerationTelemetry {
                    max_tokens: generation.max_tokens,
                    temperature: generation.temperature,
                    frequency_penalty: generation.frequency_penalty,
                    json_mode: generation.json_mode,
                    enable_thinking: generation.enable_thinking,
                    reasoning_effort: generation.reasoning_effort,
                })
                .unwrap_or_default(),
        })
        .unwrap_or(serde_json::Value::Null),
    );

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
    // `apply_npc_dialogue_turn`: the committed reply, unchanged.
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
    chain_cap: usize,
    transcript: &mut Vec<ConversationLine>,
    combined_hints: &mut Vec<crate::npc::LanguageHint>,
    assigned_tasks: &mut Vec<limerick_types::PlayerTask>,
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

        let Some(outcome) =
            run_npc_turn(ctx, speaker_id, prompt, transcript, false, &spawn_loading).await
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

/// Routes input to the people at the player's location who should hear it,
/// or shows an idle message when nobody is present.
///
/// Emits `"stream-end"` with combined language hints after the full
/// conversation chain (all addressed NPCs + autonomous follow-up turns).
///
/// # Parameters
///
/// - `ctx`: shared game-loop context.
/// - `raw`: raw player input string.
/// - `target_names`: display names of explicitly addressed NPCs (from chip
///   selection or `@mention` parsing). Empty → the unaddressed rules of
///   design §3.1: the one person present, the partner, or a question.
/// - `spawn_loading`: closure that starts a loading animation; called once per
///   player-initiated NPC turn.
pub async fn handle_npc_conversation(
    ctx: &GameLoopContext<'_>,
    raw: String,
    target_names: Vec<String>,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) -> GameInputOutcome {
    let request = AddressRequest {
        explicit: target_names,
        decision: None,
    };
    handle_npc_conversation_settled(ctx, raw, request, Answers::default(), spawn_loading).await
}

/// [`handle_npc_conversation`] for a line whose recipients the recipient
/// policy settles (design §3.1): `request` holds the explicit selection or
/// the System 1 answer, and `answers` the player's answers to questions this
/// request already asked.
///
/// When the policy has to ask (unaddressed speech with no partner, or a
/// reference that fits several people present), the turn emits the question
/// as a system line and returns it in [`GameInputOutcome::clarification`]
/// without running any dialogue. No line is answered with a system line
/// about someone who is not here; only a person chosen earlier who has since
/// left gets "{label} is no longer here."
pub async fn handle_npc_conversation_settled(
    ctx: &GameLoopContext<'_>,
    raw: String,
    request: AddressRequest,
    answers: Answers<'_>,
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

    let (player_location, max_follow_up_turns, autonomous_chain_enabled, resolution) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let config = ctx.config.lock().await;
        let clarify = !config.flags.is_disabled(ADDRESSEE_CLARIFICATION_FLAG);
        (
            world.player_location,
            config.max_follow_up_turns,
            config.flags.is_enabled(AUTONOMOUS_NPC_CHAIN_FLAG),
            resolve_recipients(&world, &npc_manager, &request, answers, clarify),
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

    let speakers = match resolution {
        // Nothing is sent until the player answers.
        Resolution::Ask(prompt) => {
            release_claim().await;
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", prompt.as_line()))
                    .unwrap_or(serde_json::Value::Null),
            );
            return GameInputOutcome {
                clarification: Some(AddresseeClarification {
                    prompt,
                    intent: None,
                    decision: None,
                }),
                ..GameInputOutcome::default()
            };
        }
        Resolution::Nobody if answers.everyone || !answers.chosen.is_empty() => {
            release_claim().await;
            return GameInputOutcome {
                unsent: Some(None),
                ..GameInputOutcome::default()
            };
        }
        Resolution::Nobody => {
            release_claim().await;
            let msg = if ctx.idle_messages.is_empty() {
                let idx = REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % IDLE_MESSAGES.len();
                IDLE_MESSAGES[idx].to_string()
            } else {
                let idx =
                    REQUEST_ID.fetch_add(1, Ordering::SeqCst) as usize % ctx.idle_messages.len();
                ctx.idle_messages[idx].clone()
            };
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", &msg)).unwrap_or(serde_json::Value::Null),
            );
            return GameInputOutcome::default();
        }
        Resolution::Gone(label) => {
            release_claim().await;
            return GameInputOutcome {
                unsent: Some(Some(format!("{label} is no longer here."))),
                ..GameInputOutcome::default()
            };
        }
        Resolution::Speak { speakers, partner } => {
            // Set on the attempt's state, so it commits only with the turn.
            let mut npc_manager = ctx.npc_manager.lock().await;
            match partner {
                PartnerChange::Keep => {}
                PartnerChange::Set(id) => npc_manager.set_partner(Some(id)),
                PartnerChange::End => npc_manager.set_partner(None),
            }
            speakers
        }
    };

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

    if ctx
        .inference()
        .route(limerick_config::InferenceSubrole::Dialogue)
        .await
        == RouteStatus::Unavailable
    {
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

    let mut combined_hints: Vec<crate::npc::LanguageHint> = Vec::new();
    let mut assigned_tasks = Vec::new();
    let mut spoken_this_chain: Vec<NpcId> = Vec::new();
    let mut last_speaker: Option<NpcId> = None;
    let mut dialogue_failure = None;
    let targets: Vec<NpcId> = speakers.iter().map(|(id, _)| *id).collect();

    // Phase 1: each recipient takes one turn in roster order, then each
    // person mentioned who joins.
    for (speaker_id, addressing) in &speakers {
        let Some(outcome) = run_npc_turn_addressed(
            ctx,
            *speaker_id,
            trimmed.as_str(),
            &transcript,
            true,
            *addressing,
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
    // recipients and joining people of Phase 1 reply.
    let chain_cap = if autonomous_chain_enabled {
        max_follow_up_turns.min(autonomous::MAX_CHAIN_TURNS)
    } else {
        0
    };
    run_autonomous_chain(
        ctx,
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
        clarification: None,
        unsent: None,
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

    let (player_location, max_follow_up_turns, speakers) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let config = ctx.config.lock().await;

        let mut speakers = npc_manager.npcs_at_ids(world.player_location);
        speakers.sort_by_key(|id| id.0);
        speakers.truncate(2);

        (
            world.player_location,
            config.max_follow_up_turns.min(2),
            speakers,
        )
    };

    if ctx
        .inference()
        .route(limerick_config::InferenceSubrole::Dialogue)
        .await
        == RouteStatus::Unavailable
    {
        return GameInputOutcome::default();
    }
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
        ..GameInputOutcome::default()
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
                endpoints: &crate::game_mod::NO_ENDPOINTS,
                client: $client,
                cloud_client: $cloud_client,
                language: crate::npc::LanguageSettings::english_only(),
                inference_failure_messages: &[],
                idle_messages: &[],
                inference_override: None,
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

    /// Design §3.1 (owner, 2026-10-17; supersedes #985): a line addressed to
    /// someone not present reaches the people who are. With Peig alone, a
    /// chip for Aoife Brennan goes to Peig, and no system line names Aoife.
    #[tokio::test]
    async fn a_chip_for_someone_absent_reaches_the_one_person_present() {
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
            endpoints: &crate::game_mod::NO_ENDPOINTS,
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        };

        super::handle_npc_conversation(
            &ctx,
            "talk to Aoife Brennan about the school".to_string(),
            vec!["Aoife Brennan".to_string()],
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        let lines: Vec<&str> = events
            .iter()
            .filter(|(name, _)| name == "text-log")
            .filter_map(|(_, payload)| payload.get("content").and_then(|v| v.as_str()))
            .collect();
        assert!(
            !lines.iter().any(|line| line.contains("not here")),
            "no line answers a name with a system line: {lines:?}"
        );
        // The line went on to Peig, the one person present; this test has
        // no model, so the turn stops at the missing provider.
        assert!(
            lines
                .iter()
                .any(|line| line.contains("LLM is not configured")),
            "{lines:?}"
        );
    }

    /// Design §3.1 (owner, 2026-10-17; supersedes #1532): with nobody
    /// present, naming someone gets the idle message only.
    #[tokio::test]
    async fn naming_someone_absent_with_nobody_present_gets_the_idle_message() {
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
            endpoints: &crate::game_mod::NO_ENDPOINTS,
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
        };

        super::handle_npc_conversation(
            &ctx,
            "Is Father Declan here?".to_string(),
            vec!["Fr. Declan Tierney".to_string()],
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        let lines: Vec<&str> = events
            .iter()
            .filter(|(name, _)| name == "text-log")
            .filter_map(|(_, payload)| payload.get("content").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(crate::ipc::IDLE_MESSAGES.contains(&lines[0]), "{lines:?}");
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
            endpoints: &crate::game_mod::NO_ENDPOINTS,
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
            inference_override: None,
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
                endpoints: &crate::game_mod::NO_ENDPOINTS,
                client: &client,
                cloud_client: &cloud_client,
                language: crate::npc::LanguageSettings::english_only(),
                inference_failure_messages: &[],
                idle_messages: &[],
                inference_override: None,
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
        let queue = crate::inference::InferenceQueue::new(itx, btx, xtx);

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
