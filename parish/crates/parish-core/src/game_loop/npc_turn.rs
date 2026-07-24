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
    INFERENCE_RESPONSE_TIMEOUT_SECS, InferenceAwaitOutcome, InferencePriority, InferenceQueue,
    QueueRequest, await_inference_response,
};
use crate::ipc::{
    ConversationLine, DialogueCorrectedPayload, IDLE_MESSAGES, INFERENCE_FAILURE_MESSAGES,
    REQUEST_ID, StreamEndPayload, StreamTokenPayload, StreamTurnEndPayload, capitalize_first,
    text_log, text_log_for_stream_turn, text_log_typed,
};
use crate::npc::NpcId;
use crate::npc::autonomous;
use crate::npc::parse_npc_stream_response;

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
/// `mood`, `internal_thought`, `language_hints`) fits without hitting the
/// provider default and truncating mid-sentence (#982, #1431). vllm-mlx and most
/// OpenAI-compat servers default to a value too low for the structured-output
/// schema once the dialogue runs more than a sentence or two.
///
/// Raised from 512 → 768 (#1431 item 3): at 512 tokens the budget was
/// consumed by `internal_thought` / `action` / `mood` before `dialogue`
/// finished, producing mid-sentence cutoffs. Budget breakdown:
///   - `internal_thought` (~15-20 words): ~25 tokens
///   - `action` + `mood` + JSON envelope overhead: ~35 tokens
///   - 2-3 sentence Hiberno-English dialogue (~70-110 tokens)
///   - Total observed minimum: ~170 tokens; 768 gives comfortable headroom.
pub const TIER1_DIALOGUE_MAX_TOKENS: u32 = 768;

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
    let (
        setup,
        time_of_day,
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
        relationship_tone_hints,
        speaker_context,
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
        let relationship_tone_hints = npc_manager.relationship_tone_hints(speaker_id);
        let speaker_context =
            npc_manager
                .get(speaker_id)
                .map(|npc| crate::npc::DialogueSpeakerContext {
                    name: npc.name.clone(),
                    occupation: npc.occupation.clone(),
                    mood: npc.mood.clone(),
                });
        let time_of_day = world.clock.time_of_day();
        (
            setup,
            time_of_day,
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
            relationship_tone_hints,
            speaker_context,
        )
    };
    let setup = setup?;

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
        .send(QueueRequest {
            id: req_id,
            model: model.to_string(),
            prompt: setup.context,
            system: Some(setup.system_prompt),
            token_tx: Some(token_tx),
            max_tokens: Some(TIER1_DIALOGUE_MAX_TOKENS),
            temperature: Some(0.7),
            // TODO #10 / #23 / #34: frequency_penalty = 0.5 suppresses
            // Qwen2.5-14B-4bit verbatim repetition loops on vllm-mlx /
            // OpenAI / OpenRouter; Anthropic + Simulator ignore the field.
            frequency_penalty: Some(0.5),
            priority: InferencePriority::Interactive,
            json_mode: true,
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

    let mut parsed = parse_npc_stream_response(&response.text);

    // Snapshot the raw model dialogue before any guard runs.  After all guards
    // complete we compare against this snapshot to determine whether any guard
    // altered the text; if so (and the kill-switch is on) we emit
    // `"dialogue-corrected"` so the frontend can replace the accumulated raw
    // stream tokens with the post-guard canonical text (#1552).
    let pre_guard_dialogue = parsed.dialogue.clone();

    // Post-generation person-confirmation guard (#1459, #1466, #1470): detect
    // when the NPC's reply affirmatively confirms a fabricated person from the
    // player's input (or an earlier turn) who is not in the known-roster, and
    // replace with a stock decline.
    // Runs before the logging/quality-check block so the guarded text is what
    // gets logged and forwarded to the shared pipeline.
    if person_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed =
            speaker_id.0 as u64 ^ ctx.world.lock().await.clock.now().timestamp() as u64;
        // Extract prior player-speaker lines from the conversation transcript so
        // the pronoun follow-up guard (#1470 gap 2) can detect fabricated
        // referents established in earlier turns.
        let prior_player_inputs: Vec<&str> = transcript
            .iter()
            .filter(|line| line.speaker == "You")
            .map(|line| line.text.as_str())
            .collect();
        let guarded = crate::npc::guard_fabricated_person_confirmation_with_locations(
            &parsed.dialogue,
            prompt_input,
            &setup.known_person_names,
            &setup.known_location_names,
            &prior_player_inputs,
            setup.player_name.as_deref(),
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation routing-after-denial guard (#1478): when the NPC denied
    // knowing a fabricated person but also added a routing phrase ("ask at X",
    // "you might find them at…"), replace with a clean non-recognition decline.
    // Runs immediately after the primary person-confirmation guard.
    // Default-on; kill-switch via `dialogue-person-routing-guard` flag.
    if routing_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed =
            speaker_id.0 as u64 ^ ctx.world.lock().await.clock.now().timestamp() as u64;
        let guarded = crate::npc::guard_fabricated_person_routing(
            &parsed.dialogue,
            prompt_input,
            &setup.known_person_names,
            setup.player_name.as_deref(),
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation wrong-location reference guard (#1477): detect when an NPC
    // names a settlement other than the current location in "here in X" / "village
    // of X" collocations, and replace the wrong name with the correct one.
    // Default-on; kill-switch via `npc-wrong-location-guard` flag.
    if wrong_location_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let loc = setup.location_name.as_str();
        let guarded = crate::npc::guard_wrong_location_reference(&parsed.dialogue, Some(loc));
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation false-denial guards (#1527, #1528, #1563),
    // invented-place confirmation guard (#1530), and stock decline polish
    // (#1564): all require a seed derived from the world clock. Acquire the
    // async world lock ONCE here if any guard is active and the dialogue is
    // non-empty, then reuse the seed for these guards.
    let both_guards_seed: Option<u64> = if (false_denial_guard_enabled
        || invented_place_guard_enabled
        || dialogue_polish_guard_enabled)
        && !parsed.dialogue.trim().is_empty()
    {
        let ts = ctx.world.lock().await.clock.now().timestamp() as u64;
        Some(speaker_id.0 as u64 ^ ts)
    } else {
        None
    };

    // Post-generation false-denial guard (#1527, #1528): detect when an NPC
    // wrongly denies knowing a person who IS in the parish roster (known_person_names).
    // Runs after the routing guard so only confirmed-false denials are caught here.
    // Default-on; kill-switch via `dialogue-false-denial-guard` flag.
    if false_denial_guard_enabled && !parsed.dialogue.trim().is_empty() {
        // both_guards_seed is always Some here (guard enabled + dialogue non-empty).
        let guard_seed = both_guards_seed.unwrap_or(0);
        let guarded = crate::npc::guard_false_denial_of_roster_person_with_speaker(
            &parsed.dialogue,
            prompt_input,
            &setup.known_person_names,
            setup.player_name.as_deref(),
            guard_seed,
            speaker_context.as_ref(),
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation false-denial guard for real places (#1563): detect when
    // an NPC generically denies a real place from the world graph ("that place
    // does not exist", "no such person") and replace it with a neutral
    // grounded acknowledgement. Runs before invented-place confirmation.
    if false_denial_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed = both_guards_seed.unwrap_or(0);
        let guarded = crate::npc::guard_false_denial_of_known_place(
            &parsed.dialogue,
            prompt_input,
            &setup.known_location_names,
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation invented-place confirmation guard (#1530): detect when an
    // NPC affirms an invented place that is not in the world's location list.
    // Default-on; kill-switch via `dialogue-invented-place-guard` flag.
    if invented_place_guard_enabled && !parsed.dialogue.trim().is_empty() {
        // both_guards_seed is always Some here (guard enabled + dialogue non-empty).
        let guard_seed = both_guards_seed.unwrap_or(0);
        let guarded = crate::npc::guard_invented_place_confirmation(
            &parsed.dialogue,
            prompt_input,
            &setup.known_location_names,
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation dialogue polish guard (#1564): replace old stock
    // non-recognition templates and correct obvious morning greeting tics when
    // the world clock is not Morning. Runs after grounding guards so true
    // false-denial corrections win before generic polish.
    if dialogue_polish_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed = both_guards_seed.unwrap_or(0);
        let guarded = crate::npc::guard_stock_nonrecognition_decline_with_speaker(
            &parsed.dialogue,
            prompt_input,
            guard_seed,
            speaker_context.as_ref(),
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }

        let guarded = crate::npc::guard_time_of_day_phrase(&parsed.dialogue, time_of_day);
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }

        let guarded = crate::npc::guard_priest_tenure_drift(&parsed.dialogue, prompt_input);
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }

        let guarded = crate::npc::guard_presumed_prior_acquaintance(
            &parsed.dialogue,
            prompt_input,
            &setup.known_person_names,
            speaker_context.as_ref(),
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }

        let guarded =
            crate::npc::guard_repeated_speaker_name(&parsed.dialogue, speaker_context.as_ref());
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }

        let guarded = crate::npc::guard_rival_target_neutral_tone(
            &parsed.dialogue,
            prompt_input,
            &relationship_tone_hints,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Canonical semantic contracts from quality-harness run #1776–#1790.
    // These are independent of the older dialogue-polish kill switch and run
    // before the UI correction event, matching the unconditional shared apply
    // seam. Streamed text, stored dialogue, and projected events therefore
    // cannot diverge when dialogue polish is disabled.
    if !parsed.dialogue.trim().is_empty() {
        if let Some(speaker) = speaker_context.as_ref() {
            let guarded = crate::npc::guard_mood_register(&parsed.dialogue, &speaker.mood);
            if guarded != parsed.dialogue {
                parsed.dialogue = guarded;
            }
        }
        let guarded = crate::npc::guard_unfounded_first_contact_familiarity(
            &parsed.dialogue,
            setup.had_prior_exchange,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
        let guarded = crate::npc::guard_direct_evidence_evasion(&parsed.dialogue, prompt_input);
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
        let guarded = crate::npc::guard_work_recommendation(
            &parsed.dialogue,
            prompt_input,
            &setup.work_roster,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation verbosity / run-on guard (#1460, #1491): strip bare leaked
    // mood-adjective, trim mid-sentence truncation ellipsis to the last
    // complete sentence, and cap trailing question stacks to at most one.
    // When mood-aware sentence cap is enabled (#1491), uses a tighter 2-sentence
    // cap for busy/curt NPC moods.
    // Applied here (before the shared pipeline) so the guarded text is what
    // gets stored in the conversation log and event bus — same effect for
    // every runtime (Tauri, server, headless) via the shared npc_turn path.
    if verbosity_guard_enabled && !parsed.dialogue.trim().is_empty() {
        // Sentence style is governed by the authored mood at the start of the
        // turn, not by the model's self-reported JSON mood (#1779).
        let mood_str = speaker_context
            .as_ref()
            .map(|speaker| speaker.mood.as_str());
        let guarded = if mood_sentence_cap_enabled {
            crate::npc::guard_verbosity_runons_with_mood(&parsed.dialogue, mood_str)
        } else {
            crate::npc::guard_verbosity_runons(&parsed.dialogue)
        };
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation wrong-speaker-identity guard (#1475): detect when the
    // NPC's reply claims to be a different roster member ("I'm Brendan, the
    // Miller's Son" spoken by Nora Duffy) and replace with a recovery line.
    // Default-on; kill-switch via `npc-wrong-speaker-guard` flag.
    if wrong_speaker_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed =
            speaker_id.0 as u64 ^ ctx.world.lock().await.clock.now().timestamp() as u64;
        let guarded = crate::npc::guard_wrong_speaker_identity(
            &parsed.dialogue,
            &setup.npc_name,
            &setup.roster_names_occupations,
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Post-generation acquaintance-question intent-drift guard (#1504): detect
    // when the player asked "do you know X?" and the NPC responded only with a
    // self-identification ("I'm but Seamus Gallagher") instead of answering
    // whether they know the named person. Replaces with the correct acquaintance
    // answer (affirmation if known, non-recognition decline if not).
    // Default-on; kill-switch via `npc-acquaintance-intent-guard` flag.
    if acquaintance_guard_enabled && !parsed.dialogue.trim().is_empty() {
        let guard_seed =
            speaker_id.0 as u64 ^ ctx.world.lock().await.clock.now().timestamp() as u64;
        let guarded = crate::npc::guard_acquaintance_question_intent_drift(
            &parsed.dialogue,
            prompt_input,
            &setup.npc_name,
            &setup.known_person_names,
            guard_seed,
        );
        if guarded != parsed.dialogue {
            parsed.dialogue = guarded;
        }
    }

    // Cross-NPC opener de-duplication (#1422, #1492): strip duplicate stock
    // opener if the session has already seen a near-identical one from a
    // previous NPC at this location (across any number of prior turns).
    // Run BEFORE `apply_npc_dialogue_turn` so the `DialogueOccurred` event
    // and conversation log carry the deduped text, not the raw opener.
    if anti_repetition_enabled && !parsed.dialogue.trim().is_empty() {
        let mut conversation = ctx.conversation.lock().await;
        let deduped = crate::npc::dedupe_cross_npc_openers(
            &conversation.seen_openers_this_location,
            &parsed.dialogue,
        );
        if deduped != parsed.dialogue {
            tracing::debug!(
                npc = %display_label,
                "stripped duplicate cross-NPC opener in run_npc_turn (#1422/#1492)"
            );
        }
        // Record the opener actually shown to the player.
        let shown_opener = crate::npc::extract_normalized_opener(&deduped);
        if !shown_opener.is_empty() {
            conversation.record_opener(shown_opener);
        }
        parsed.dialogue = deduped;
    }

    // Post-guard UI replace (#1552): if any guard altered the raw model dialogue,
    // emit `"dialogue-corrected"` so the frontend can replace the accumulated raw
    // stream tokens with the canonical post-guard text.  Only fires when the
    // text actually changed (no-op for clean model output) and the kill-switch is
    // on (default).  The event is emitted AFTER `stream-turn-end` (already fired
    // above) so the stream pump has already seen all tokens; the UI handler must
    // flush any remaining buffered tokens and then overwrite with `corrected_text`.
    if post_guard_ui_replace_enabled && parsed.dialogue != pre_guard_dialogue {
        tracing::debug!(
            npc = %display_label,
            req_id,
            "guards altered dialogue — emitting dialogue-corrected (#1552)"
        );
        ctx.emitter.emit_event(
            "dialogue-corrected",
            serde_json::to_value(DialogueCorrectedPayload {
                turn_id: req_id,
                corrected_text: parsed.dialogue.clone(),
                message_id: Some(message_id.clone()),
            })
            .unwrap_or(serde_json::Value::Null),
        );
    }

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

    // Player-visible dialogue, set from the shared pipeline's `display_text`.
    let captured_display_text;
    let captured_hints;
    {
        let mut world = ctx.world.lock().await;
        let game_time = world.clock.now();
        let mut npc_manager = ctx.npc_manager.lock().await;

        // Capture the speaker's location now, while the lock is held, so the
        // dialogue event/log routes by event-time location (#1035). Used as the
        // turn location for the whole shared pipeline below.
        let event_location = npc_manager
            .get(speaker_id)
            .map(|n| n.location)
            .unwrap_or(world.player_location);

        // Shared per-turn pipeline: name detection, Tier-1 apply, conversation
        // log, witness memories, and the `DialogueOccurred` publish (#1172 /
        // #1173). The live loop discards the returned debug-event strings but
        // keeps `display_text` — the guarded (#1228) and length-capped (#1224)
        // dialogue that must be shown to the player, identical to what was
        // stored in the event bus and conversation log.
        let outcome = crate::game_session::apply_npc_dialogue_turn(
            &mut world,
            &mut npc_manager,
            speaker_id,
            &parsed,
            prompt_input,
            prompt_input,
            game_time,
            event_location,
            &display_label,
            &setup.npc_name,
            Some(req_id),
            &setup.known_person_names,
            &ctx.language,
        );
        captured_display_text = outcome.display_text;
        captured_hints = outcome.language_hints;
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
        let action_text = parsed
            .metadata
            .as_ref()
            .map(|m| m.action.trim())
            .unwrap_or("");
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
        return;
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
        return;
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
        return;
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
        priest.location = crate::world::LocationId(player_loc.0 + 1);
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
        npc.location = player_loc;
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
        npc.location = player_loc;
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
        assert!(
            budget >= 768,
            "TIER1_DIALOGUE_MAX_TOKENS must be >= 768 to prevent mid-sentence \
             truncation when metadata fields precede dialogue in the JSON output \
             (fix #1431 item 3); current value: {budget}"
        );
    }

    /// #1552 — post-guard UI replace: when a post-generation guard alters the
    /// NPC dialogue, `run_npc_turn` must emit a `"dialogue-corrected"` event
    /// carrying the post-guard canonical text so the frontend can replace the
    /// accumulated raw stream tokens.
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

        // Spawn a task that reads InferenceRequests and answers each with our
        // canned raw_json (streaming the full text as a single token batch,
        // then sending the final InferenceResponse).
        let raw_json_clone = raw_json.clone();
        tokio::spawn(async move {
            while let Some(req) = irx.recv().await {
                // Stream the whole payload as a single token batch so the
                // `stream-token` path is exercised (even though tests don't
                // pump the timer-based reveal).
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
        npc.location = player_loc;
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

        // Scope the std `MutexGuard` in a block so it is structurally dropped
        // before the second `run_npc_turn().await` below — clippy's
        // `await_holding_lock` does not honour an explicit `drop()` here.
        let corrected_text = {
            let events = emitter.events.lock().unwrap();

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
            corrected
                .unwrap()
                .get("corrected_text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
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
        npc2.location = player_loc2;
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
}
