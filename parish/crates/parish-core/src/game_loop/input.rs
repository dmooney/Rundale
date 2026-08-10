//! Shared input dispatch extracted from all backends (#696 slice 4).
//!
//! [`handle_game_input`] is the entry point for all free-form player text.
//! It parses the intent (optionally via LLM), then routes to movement,
//! look, or NPC conversation — all through the shared [`GameLoopContext`].
//!
//! [`handle_look`] renders the current location description and emits a
//! `"text-log"` event.
//!
//! # Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use tokio_util::sync::CancellationToken;

use crate::config::InferenceCategory;
use crate::game_loop::{
    GameInputOutcome, GameLoopContext, handle_movement, handle_npc_conversation,
};
use crate::input::{is_physical_action_shaped, is_player_dialogue, parse_intent_local};
use crate::ipc::{extract_npc_mentions, render_look_text, text_log, text_log_typed};
use crate::npc::reactions::ReactionTemplates;
use crate::world::transport::TransportMode;

// ── Look ──────────────────────────────────────────────────────────────────────

/// Renders the current location description and emits a `"text-log"` event.
pub async fn handle_look(ctx: &GameLoopContext<'_>, transport: &TransportMode) {
    let world = ctx.world.lock().await;
    let npc_manager = ctx.npc_manager.lock().await;
    let text = render_look_text(
        &world,
        &npc_manager,
        transport.speed_m_per_s,
        &transport.label,
        false,
    );
    ctx.emitter.emit_event(
        "text-log",
        serde_json::to_value(text_log("system", text)).unwrap_or(serde_json::Value::Null),
    );
}

// ── Examine ───────────────────────────────────────────────────────────────────

/// Handles an `Examine` intent.
///
/// When `target` is `None` (bare "examine room") this falls through to
/// [`handle_look`] — the player just wants a room description.
///
/// When `target` is `Some(name)` this emits a target-specific detail message
/// instead of the generic room blurb, so the player receives an acknowledgement
/// about the named subject rather than a silent reprint of the location
/// description (#1424).  The world model does not yet carry per-object examine
/// prose, so for now the response is a brief "nothing more noteworthy" message
/// that at minimum references the target name and is **distinct** from the room
/// blurb.  Future iterations can replace this with world-model-driven detail.
///
/// Gated by the `examine-intent` feature flag (default-ON via `is_disabled`).
/// When the flag is explicitly disabled the call falls through to `handle_look`,
/// preserving the pre-fix behaviour.
pub async fn handle_examine(
    ctx: &GameLoopContext<'_>,
    target: Option<String>,
    transport: &TransportMode,
) {
    // Flag gate: if examine-intent is explicitly disabled, fall through to look.
    let flag_enabled = {
        let config = ctx.config.lock().await;
        !config.flags.is_disabled("examine-intent")
    };

    match (flag_enabled, target) {
        (true, Some(name)) => {
            // Emit a target-specific acknowledgement that is never the room blurb.
            let msg = format!(
                "You look more closely at {name}. There is nothing more noteworthy about it than what you have already observed."
            );
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", msg)).unwrap_or(serde_json::Value::Null),
            );
        }
        // Bare examine (no target) or flag disabled → fall through to room look.
        _ => {
            handle_look(ctx, transport).await;
        }
    }
}

/// Result of `try_handle_move`.
enum MoveDispatch {
    /// Movement intent was fully handled (either travel succeeded or a system
    /// "where to?" hint was emitted at an empty location). Caller should
    /// return.
    Handled,
    /// Move intent had no resolvable target but at least one NPC is present.
    /// The caller should fall through to NPC conversation routing so the
    /// co-located NPC can respond (TODO #40/#56).
    FallThroughToNpc,
}

/// Dispatches a parsed `Move` intent. Returns `FallThroughToNpc` only when the
/// LLM (or local parser) classified the input as movement but supplied no
/// target AND there is at least one co-located NPC who could respond — in
/// that case the caller routes the input to `handle_npc_conversation` instead
/// of emitting a silent system one-liner (TODO #40/#56).
async fn try_handle_move(
    ctx: &GameLoopContext<'_>,
    move_target: Option<String>,
    transport: &TransportMode,
    reaction_templates: &ReactionTemplates,
) -> MoveDispatch {
    if let Some(target) = move_target {
        handle_movement(ctx, &target, transport, reaction_templates).await;
        return MoveDispatch::Handled;
    }
    let npc_present = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        !npc_manager.npcs_at(world.player_location).is_empty()
    };
    if npc_present {
        return MoveDispatch::FallThroughToNpc;
    }
    ctx.emitter.emit_event(
        "text-log",
        serde_json::to_value(text_log("system", "And where would ye be off to?"))
            .unwrap_or(serde_json::Value::Null),
    );
    MoveDispatch::Handled
}

// ── Interact ──────────────────────────────────────────────────────────────────

/// Handles an `Interact` intent by emitting a narrated action `text-log`.
///
/// Gated by the `interact-narration` flag (default-ON via `is_disabled`).
/// When the flag is explicitly disabled the caller falls through to
/// `handle_npc_conversation`, preserving pre-fix behaviour (#1449).
///
/// Extracted so tests can drive the narration branch directly without
/// requiring an LLM-classified `Interact` intent.
pub(crate) async fn handle_interact(ctx: &GameLoopContext<'_>, raw: &str) -> GameInputOutcome {
    let flags = {
        let config = ctx.config.lock().await;
        config.flags.clone()
    };
    let outcome = {
        let mut world = ctx.world.lock().await;
        crate::game_session::apply_player_action(&mut world, raw, &flags)
    };
    let Some(outcome) = outcome else {
        return GameInputOutcome::default();
    };
    ctx.emitter.emit_event(
        "text-log",
        serde_json::to_value(text_log("action", outcome.narration))
            .unwrap_or(serde_json::Value::Null),
    );
    GameInputOutcome::from_task(outcome.progressed_task)
}

// ── Game input dispatch ───────────────────────────────────────────────────────

/// Handles free-form player input: parses intent (with LLM fallback) then
/// dispatches to movement, look, or NPC conversation.
///
/// # Parameters
///
/// - `ctx`: shared game-loop context.
/// - `raw`: the original player text.
/// - `addressed_to`: display names of explicitly addressed NPCs (from chip
///   selection).  These are prepended to the target list when routing to NPC
///   conversation.
/// - `transport`: the active transport mode (used by movement and look).
/// - `reaction_templates`: NPC arrival reaction templates (passed to movement).
/// - `spawn_loading`: closure that starts a loading animation; passed through
///   to [`handle_npc_conversation`].
#[allow(clippy::too_many_arguments)]
pub async fn handle_game_input(
    ctx: &GameLoopContext<'_>,
    raw: String,
    addressed_to: Vec<String>,
    transport: &TransportMode,
    reaction_templates: &ReactionTemplates,
    spawn_loading: impl Fn() -> Option<CancellationToken>,
) -> GameInputOutcome {
    // Record the raw player input before any parsing so a bug report filed
    // mid-turn carries the exact action that triggered the failure (#1331).
    ctx.conversation.lock().await.record_player_input(&raw);

    if !is_player_dialogue(&raw) {
        let echo_enabled = {
            let config = ctx.config.lock().await;
            !config.flags.is_disabled("echo-commands")
        };
        if echo_enabled {
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log_typed("player", raw.as_str(), "command"))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
    }

    // Resolve the intent client and model (Intent category override, or base).
    let (client, model) = {
        let config = ctx.config.lock().await;
        let base_client = ctx.client.lock().await;
        config.resolve_category_client(InferenceCategory::Intent, base_client.as_ref())
    };

    // Parse intent: tries local keywords first, then LLM for ambiguous input.
    let intent = if let Some(client) = &client {
        // Capture generation before releasing the lock so we can detect TOCTOU
        // races on re-acquire (#283).
        let gen_before = {
            let mut world = ctx.world.lock().await;
            world.clock.inference_pause();
            world.tick_generation
        };
        let profile = ctx
            .config
            .lock()
            .await
            .inference_profile(parish_config::InferenceSubrole::Intent);
        let audit_sink = ctx
            .inference_queue
            .lock()
            .await
            .as_ref()
            .and_then(crate::inference::InferenceQueue::audit_sink);
        let result = crate::input::parse_intent_with_profile_and_audit(
            client, &raw, &model, profile, audit_sink,
        )
        .await;
        {
            let mut world = ctx.world.lock().await;
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
                ctx.emitter.emit_event(
                    "text-log",
                    serde_json::to_value(text_log(
                        "system",
                        "The world shifted while your words were in the air.",
                    ))
                    .unwrap_or(serde_json::Value::Null),
                );
            }
        }
        result.ok()
    } else {
        // No client configured — use local keyword parsing only.
        parse_intent_local(&raw)
    };

    let is_move = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Move))
        .unwrap_or(false);
    let is_look = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Look))
        .unwrap_or(false);
    let is_examine = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Examine))
        .unwrap_or(false);
    let is_talk = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Talk))
        .unwrap_or(false);
    let is_interact = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Interact))
        .unwrap_or(false);
    let move_target = intent
        .as_ref()
        .filter(|_i| is_move)
        .and_then(|i| i.target.clone());
    let examine_target = intent
        .as_ref()
        .filter(|_i| is_examine)
        .and_then(|i| i.target.clone());
    let talk_target = intent
        .as_ref()
        .filter(|_i| is_talk)
        .and_then(|i| i.target.clone());

    // #1450: when `addressed_to` is non-empty the player is explicitly directing
    // speech at a named NPC — movement classification must NOT win. Skip the move
    // branch so the input routes to `handle_npc_conversation` instead.
    if is_move && addressed_to.is_empty() {
        match try_handle_move(ctx, move_target, transport, reaction_templates).await {
            MoveDispatch::Handled => return GameInputOutcome::default(),
            // TODO #40/#56: Move-no-target at a populated location falls through
            // to the NPC conversation path below so the co-located NPC has a
            // chance to reply instead of the player getting a silent system
            // one-liner. Empty-location case is still handled inline above.
            MoveDispatch::FallThroughToNpc => {}
        }
    }

    if is_look {
        handle_look(ctx, transport).await;
        return GameInputOutcome::default();
    }

    if is_examine {
        handle_examine(ctx, examine_target, transport).await;
        return GameInputOutcome::default();
    }

    // #1449: physical player actions classified as `Interact` get a narrated
    // acknowledgement rather than routing to NPC conversation.  Gated by the
    // `interact-narration` flag (default-ON, kill-switch pattern: gate fires
    // unless the flag has been explicitly disabled).
    // Additionally, mirror the #1450 pattern: if the player explicitly addresses
    // an NPC while performing an action, route to NPC conversation so the NPC
    // can witness/react rather than the generic narration handler intercepting.
    if is_interact && addressed_to.is_empty() {
        let flag_enabled = {
            let config = ctx.config.lock().await;
            !config.flags.is_disabled("interact-narration")
        };
        if flag_enabled {
            return handle_interact(ctx, &raw).await;
        }
        // Flag disabled: fall through to NPC conversation (legacy behaviour).
    }

    // #1461: no-silent-drop fallback.
    //
    // When the intent is `Unknown` (the LLM returned an unrecognised
    // classification or the call failed) AND the input is shaped like an
    // imperative physical action (not first-person, not a greeting, not a
    // question), narrate the action rather than letting it vanish silently
    // into `handle_npc_conversation`.  This covers verbs that are neither in
    // the local-parser `interact_prefixes` list nor correctly classified by
    // the LLM — e.g. "draw a bucket of water" when the intent model returns
    // Unknown due to load or quantisation drift.
    //
    // Gated by `interact-narration` (same flag) and only fires when
    // `addressed_to` is empty, mirroring the primary `is_interact` guard
    // above.  When the flag is disabled or the player is addressing an NPC
    // the input falls through to NPC conversation (legacy behaviour).
    // `unwrap_or(true)` — when `intent` is `None` the LLM call failed
    // entirely; that is semantically equivalent to `Unknown` and the
    // no-silent-drop fallback must fire (if the input is action-shaped).
    let is_unknown = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Unknown))
        .unwrap_or(true);
    if is_unknown && addressed_to.is_empty() && is_physical_action_shaped(&raw) {
        let flag_enabled = {
            let config = ctx.config.lock().await;
            !config.flags.is_disabled("interact-narration")
        };
        if flag_enabled {
            return handle_interact(ctx, &raw).await;
        }
    }

    // Resolve ordered NPC recipients from visible local names.
    // Also validate the LLM's talk_target — only accept it when it resolves
    // to an actual co-located NPC (by name or role vocative). Non-NPC nouns
    // such as "a boat" or the player's own name must not be pushed into the
    // target list: they will generate a spurious "X is not here." message
    // (#1220, #1227).
    let (mentions, explicit_recipient_names, validated_talk_target) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let mentions = extract_npc_mentions(&raw, &world, &npc_manager);
        let explicit_recipient_names = explicit_talk_recipient_clause(&raw)
            .map(|clause| extract_npc_mentions(clause, &world, &npc_manager).names);
        let validated = if is_talk {
            talk_target.filter(|t| {
                npc_manager
                    .find_by_name(t, world.player_location)
                    .or_else(|| npc_manager.find_by_role_at(t, world.player_location))
                    .is_some()
            })
        } else {
            None
        };
        (mentions, explicit_recipient_names, validated)
    };

    // Explicit recipients are authoritative. A chip-selected addressee, or the
    // recipient clause in `talk to X about Y`, must not be polluted by other
    // parish names mentioned in the message body. Otherwise asking Seamus
    // "Where is Padraig?" addresses both men and emits a false
    // "Padraig Darcy is not here." line. Free-form dialogue without an
    // explicit recipient still routes to every naturally mentioned local NPC.
    let mut targets: Vec<String> =
        Vec::with_capacity(addressed_to.len() + mentions.names.len() + 1);
    if !addressed_to.is_empty() {
        for name in addressed_to {
            push_unique_target(&mut targets, name);
        }
    } else if let Some(explicit_names) = explicit_recipient_names {
        for name in explicit_names {
            push_unique_target(&mut targets, name);
        }
        if let Some(target) = validated_talk_target {
            push_unique_target(&mut targets, target);
        }
        if targets.is_empty()
            && let Some(clause) = explicit_talk_recipient_clause(&raw)
        {
            push_unique_target(&mut targets, clause.to_string());
        }
    } else {
        for name in mentions.names {
            push_unique_target(&mut targets, name);
        }
        if let Some(target) = validated_talk_target {
            push_unique_target(&mut targets, target);
        }
    }

    handle_npc_conversation(ctx, mentions.remaining, targets, spawn_loading).await
}

fn push_unique_target(targets: &mut Vec<String>, target: String) {
    if !targets
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&target))
    {
        targets.push(target);
    }
}

fn explicit_talk_recipient_clause(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let prefix_len = if lower.starts_with("talk to ") {
        "talk to ".len()
    } else if lower.starts_with("speak to ") {
        "speak to ".len()
    } else {
        return None;
    };

    let lower_remainder = &lower[prefix_len..];
    let clause_end = [" about ", " regarding "]
        .iter()
        .filter_map(|delimiter| lower_remainder.find(delimiter))
        .min()
        .unwrap_or(lower_remainder.len());
    let clause = trimmed[prefix_len..prefix_len + clause_end].trim();
    (!clause.is_empty()).then_some(clause)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::game_loop::GameLoopContext;
    use crate::game_loop::npc_turn::tests::CapturingEmitter;
    use crate::ipc::{ConversationRuntimeState, EventEmitter, GameConfig};
    use crate::npc::manager::NpcManager;
    use crate::npc::reactions::ReactionTemplates;
    use crate::world::{WorldState, transport::TransportMode};

    fn make_transport() -> TransportMode {
        TransportMode {
            label: "on foot".to_string(),
            id: "walking".to_string(),
            speed_m_per_s: 1.2,
        }
    }

    #[test]
    fn explicit_talk_recipient_clause_stops_before_message_body() {
        assert_eq!(
            super::explicit_talk_recipient_clause(
                "talk to Seamus Gallagher about Where is Padraig Darcy?"
            ),
            Some("Seamus Gallagher")
        );
        assert_eq!(
            super::explicit_talk_recipient_clause("SPEAK TO Peig Hannigan REGARDING the road"),
            Some("Peig Hannigan")
        );
        assert_eq!(
            super::explicit_talk_recipient_clause("talk to Seamus Gallagher and Colm Gallagher",),
            Some("Seamus Gallagher and Colm Gallagher")
        );
        assert_eq!(
            super::explicit_talk_recipient_clause("Where is Padraig Darcy?"),
            None
        );
    }

    #[tokio::test]
    async fn handle_look_emits_text_log() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        super::handle_look(&ctx, &transport).await;

        let names = emitter.event_names();
        assert!(
            names.iter().any(|n| n == "text-log"),
            "expected text-log from handle_look; got {names:?}"
        );
    }

    #[tokio::test]
    async fn handle_game_input_echoes_non_dialogue_as_command() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();
        super::handle_game_input(
            &ctx,
            "look".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        let text_logs: Vec<&serde_json::Value> = events
            .iter()
            .filter(|(name, _)| name == "text-log")
            .map(|(_, payload)| payload)
            .collect();
        assert!(
            text_logs.len() >= 2,
            "expected command echo plus look narration, got {text_logs:?}"
        );
        assert_eq!(
            text_logs[0].get("source").and_then(|v| v.as_str()),
            Some("player")
        );
        assert_eq!(
            text_logs[0].get("subtype").and_then(|v| v.as_str()),
            Some("command")
        );
        assert_eq!(
            text_logs[0].get("content").and_then(|v| v.as_str()),
            Some("look")
        );
        assert!(
            text_logs
                .iter()
                .skip(1)
                .any(|p| p.get("source").and_then(|v| v.as_str()) == Some("system")),
            "look should still emit system narration after command echo: {text_logs:?}"
        );
    }

    #[tokio::test]
    async fn handle_game_input_does_not_command_echo_dialogue() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();
        super::handle_game_input(
            &ctx,
            "hello there".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let events = emitter.events.lock().unwrap();
        assert!(
            events.iter().all(|(name, payload)| {
                name != "text-log"
                    || payload.get("subtype").and_then(|v| v.as_str()) != Some("command")
            }),
            "dialogue must not be echoed as a command: {events:?}"
        );
    }

    // ── Examine routing (#1424) ───────────────────────────────────────────────

    /// AC-6 / AC-3: Examine with a target must NOT emit the room blurb.
    ///
    /// In no-LLM mode `parse_intent_local("examine the old well")` classifies as
    /// `Examine` with `target = Some("the old well")`.  `handle_game_input` must
    /// route this to `handle_examine`, which emits a target-specific message
    /// that is distinct from the room description.
    ///
    /// This test fails against pre-fix code (Examine falls through to
    /// `handle_npc_conversation` which emits an idle message, not the room
    /// blurb — but the intent is still wrong; the fix ensures `handle_examine`
    /// is called and its output contains the target name).
    #[tokio::test]
    async fn examine_with_target_routes_to_handle_examine_not_look() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // First run a bare `look` to capture the room blurb.
        super::handle_look(&ctx, &transport).await;
        let look_logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        let room_blurb = look_logs.first().cloned().unwrap_or_default();

        // Clear events.
        emitter.events.lock().unwrap().clear();

        // Now run handle_game_input with "examine the old well".
        // No LLM configured — parse_intent_local classifies as Examine.
        super::handle_game_input(
            &ctx,
            "examine the old well".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let examine_logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        // Must have emitted something.
        assert!(
            !examine_logs.is_empty(),
            "examine must emit a text-log; got none"
        );

        // The output must NOT be the verbatim room blurb (#1424).
        assert!(
            !examine_logs.iter().any(|l| l == &room_blurb),
            "examine must NOT reprint the room blurb; got: {examine_logs:?}"
        );

        // The output must reference the target name.
        assert!(
            examine_logs.iter().any(|l| l.contains("old well")),
            "examine response must reference the target 'old well'; got: {examine_logs:?}"
        );
    }

    /// AC-5: When the examine-intent flag is disabled, examine falls through to handle_look.
    #[tokio::test]
    async fn examine_with_flag_disabled_falls_through_to_look() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let mut cfg = GameConfig::default();
        cfg.flags.disable("examine-intent");
        let config = tokio::sync::Mutex::new(cfg);
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();

        // Capture the room blurb from a normal look.
        super::handle_look(&ctx, &transport).await;
        let look_logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        let room_blurb = look_logs.first().cloned().unwrap_or_default();

        emitter.events.lock().unwrap().clear();

        // With flag disabled, examine should fall through to look.
        super::handle_examine(&ctx, Some("the old well".to_string()), &transport).await;

        let examine_logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        assert!(
            examine_logs.iter().any(|l| l == &room_blurb),
            "with flag disabled, handle_examine must emit the room blurb (falls through to look)"
        );
    }

    #[tokio::test]
    async fn handle_game_input_no_llm_unknown_text_routes_to_npc_conversation() {
        // With no client configured, parse_intent_local tries to classify.
        // Generic text that doesn't match move/look → routed to NPC conversation.
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "hello there" → no NPC present → idle-message text-log
        super::handle_game_input(
            &ctx,
            "hello there".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let names = emitter.event_names();
        assert!(
            names.iter().any(|n| n == "text-log"),
            "expected text-log (idle message) when no NPC present; got {names:?}"
        );
    }

    // ── TODO #40/#56: Move-no-target dispatch ─────────────────────────────────

    #[tokio::test]
    async fn move_no_target_at_empty_location_emits_system_message() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        let outcome = super::try_handle_move(&ctx, None, &transport, &templates).await;
        assert!(
            matches!(outcome, super::MoveDispatch::Handled),
            "empty-location Move-no-target should be Handled (system message)"
        );
        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert!(
            logs.iter().any(|l| l.contains("where would ye be off to")),
            "expected 'where would ye be off to?' system message; got {logs:?}"
        );
    }

    #[tokio::test]
    async fn move_no_target_at_populated_location_falls_through_to_npc() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world_state = WorldState::new();
        let player_loc = world_state.player_location;
        let world = tokio::sync::Mutex::new(world_state);

        // One co-located NPC: the existing test fixture lives at LocationId(1)
        // which matches the default WorldState::new() player location.
        let mut npc = parish_npc::Npc::new_test_npc();
        npc.set_location(player_loc);
        let mut mgr = NpcManager::new();
        mgr.add_npc(npc);
        let npc_manager = tokio::sync::Mutex::new(mgr);

        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        let outcome = super::try_handle_move(&ctx, None, &transport, &templates).await;
        assert!(
            matches!(outcome, super::MoveDispatch::FallThroughToNpc),
            "populated-location Move-no-target should fall through to NPC routing"
        );
        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert!(
            !logs.iter().any(|l| l.contains("where would ye be off to")),
            "did not expect 'where would ye be off to?' at populated location; got {logs:?}"
        );
    }

    // ── validated_talk_target: non-NPC targets suppressed (#1220, #1227) ──────

    /// Regression (#1220): A player self-introduction ("My name is Aiden") must
    /// NOT produce an "Aiden is not here." message.  In headless no-LLM mode
    /// the local parser returns Talk with target=None for first-person input, so
    /// the validated_talk_target is always None and the path degenerates cleanly
    /// to an idle message (no NPC present) or ambient NPC conversation.
    ///
    /// This test proves the regression: even if the harness were to inject a
    /// talk_target of "Aiden" (a name that is not an NPC), no "is not here."
    /// message is emitted — the `handle_game_input` talk_target validation
    /// silently drops non-NPC names.
    #[tokio::test]
    async fn self_introduction_does_not_emit_not_here() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // First-person input: local parser classifies as Talk, target=None.
        // No NPC present → idle message (or nothing).  Must never emit "not here.".
        super::handle_game_input(
            &ctx,
            "My name is Aiden, I'm new to these parts".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert!(
            !logs.iter().any(|l| l.contains("is not here.")),
            "self-introduction must not emit 'X is not here.'; got {logs:?}"
        );
    }

    /// Regression (#1227): A noun/object mention ("I saw a boat on the stream")
    /// must NOT produce "a boat on the stream is not here." — the LLM-supplied
    /// talk_target is validated against present NPCs before use.
    ///
    /// In local-only mode this passes trivially (target=None).  The integration
    /// proof is the headless /stub fixture; this test guards the code path.
    #[tokio::test]
    async fn object_mention_does_not_emit_not_here() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        super::handle_game_input(
            &ctx,
            "I saw a boat on the stream near the mill pond this morning".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();
        assert!(
            !logs.iter().any(|l| l.contains("is not here.")),
            "object mention must not emit 'X is not here.'; got {logs:?}"
        );
    }

    // ── #1450: addressed_to pins dialogue intent over movement ────────────────

    /// AC-1 / AC-2 (#1450): a move-classified input with non-empty `addressed_to`
    /// must NOT trigger movement — it must fall through to NPC conversation routing.
    ///
    /// In no-LLM mode the local parser classifies "go to the pub" as `Move`.
    /// But when `addressed_to = ["Peig Hannigan"]`, the `is_move` guard must be
    /// skipped so the dialogue is routed to NPC conversation.  We assert that no
    /// movement system message is emitted (movement emits "And where would ye be
    /// off to?" or a travel log, neither of which would appear here since there
    /// is no connected location "the pub" in the default world; what we can assert
    /// is that the input does NOT emit a movement attempt and falls through
    /// to NPC conversation — in the empty-NPC case this produces an idle text-log,
    /// not a movement message).
    #[tokio::test]
    async fn addressed_to_non_empty_skips_move_branch() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "go to the pub" → local parser classifies as Move.
        // addressed_to = ["Peig Hannigan"] → must NOT route to movement.
        super::handle_game_input(
            &ctx,
            "go to the pub".to_string(),
            vec!["Peig Hannigan".to_string()],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        // The movement "where would ye be off to?" hint must NOT appear —
        // that would indicate the is_move branch fired despite addressed_to.
        assert!(
            !logs.iter().any(|l| l.contains("where would ye be off to")),
            "#1450: addressed_to must suppress move branch; got logs: {logs:?}"
        );
    }

    /// AC-3 (#1450): with empty `addressed_to`, movement classification still
    /// wins as before (regression guard).
    #[tokio::test]
    async fn empty_addressed_to_preserves_move_routing() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // Bare "go to the pub" with no NPC present and no addressed_to:
        // the move branch fires, no NPC present, no matching location →
        // the movement handler emits a "not found" system message (not the
        // idle message). We assert something was emitted (the move path ran).
        super::handle_game_input(
            &ctx,
            "go to the pub".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let event_names = emitter.event_names();
        // The move path always emits at least one event (travel or error).
        assert!(
            !event_names.is_empty(),
            "empty addressed_to with Move input must produce output; got none"
        );
    }

    // ── #1449: Interact intent produces narrated action ───────────────────────

    /// AC-5 / AC-6 (#1449): an `Interact`-classified input must NOT route to
    /// `handle_npc_conversation`; it must emit a narrated `text-log`.
    ///
    /// The local parser does not emit `Interact`; that intent comes from the LLM.
    /// In tests (no LLM configured), `parse_intent_local` returns `None` for
    /// physical action phrases, and the dispatch falls through to NPC conversation
    /// which emits an idle-message `text-log`.  To test the `is_interact` branch
    /// directly we call `handle_interact` (the extracted helper).
    ///
    /// We test the branch via `handle_interact` directly to avoid LLM dependency.
    #[tokio::test]
    async fn interact_intent_emits_narrated_action_not_npc_dialogue() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        // Call handle_interact directly (the interact-narration branch).
        super::handle_interact(&ctx, "tie a strip of cloth to the thorn bush").await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        assert!(
            !logs.is_empty(),
            "#1449: interact must emit a text-log narration; got none"
        );
        assert!(
            logs.iter()
                .any(|l| l.contains("tie a strip of cloth to the thorn bush")),
            "#1449: narration must reference the original input; got: {logs:?}"
        );
    }

    /// AC-5 / AC-6 (#1449) end-to-end via local parser: the local parser now
    /// classifies physical-action imperative verbs as `Interact` directly
    /// (no LLM required), so `handle_game_input` with flag enabled must
    /// emit a narrated `text-log` and NOT route to NPC conversation.
    #[tokio::test]
    async fn handle_game_input_interact_narrates_via_local_parser() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None); // no LLM — local parser only
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "tie a strip of cloth to the thorn bush" — primary #1449 repro.
        // The local parser classifies this as Interact; with flag enabled (default)
        // handle_game_input must emit a narrated action text-log.
        super::handle_game_input(
            &ctx,
            "tie a strip of cloth to the thorn bush".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;
        super::handle_game_input(
            &ctx,
            "I set to work in the potato patch, breaking clods and planting seed.".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        assert!(
            !logs.is_empty(),
            "#1449: interact must emit a text-log; got none"
        );
        assert!(
            logs.iter()
                .any(|l| l.contains("tie a strip of cloth to the thorn bush")),
            "#1449: narration must reference the original input; got: {logs:?}"
        );
        assert!(
            logs.iter().any(|l| {
                l == "You set to work in the potato patch, breaking clods and planting seed."
            }),
            "#1780: first-person task action must route to narration; got: {logs:?}"
        );
    }

    /// AC-7 (#1449): with `interact-narration` flag disabled, interact falls
    /// through to NPC conversation (legacy behaviour preserved as kill-switch).
    #[tokio::test]
    async fn interact_with_flag_disabled_falls_through() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let mut cfg = GameConfig::default();
        cfg.flags.disable("interact-narration");
        let config = tokio::sync::Mutex::new(cfg);
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "pick up the bellows" — the local parser now classifies this as
        // `Interact` (#1449 fix). With the `interact-narration` flag disabled,
        // the `is_interact` dispatch branch falls through to NPC conversation
        // (legacy kill-switch). No action narration should be emitted; an idle
        // text-log (no NPC present) is produced instead.
        super::handle_game_input(
            &ctx,
            "pick up the bellows".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        // With flag disabled, dispatch falls through to NPC conversation →
        // idle message text-log (no NPC present).
        let event_names = emitter.event_names();
        assert!(
            event_names.iter().any(|n| n == "text-log"),
            "interact fallthrough must still emit a text-log; got {event_names:?}"
        );
    }

    // ── Gemini review thread fixes ────────────────────────────────────────────

    /// Capitalized imperatives and first-person task actions must be normalized
    /// into grammatical second-person narration.
    ///
    /// "Tie a strip of cloth." → "You tie a strip of cloth."
    #[tokio::test]
    async fn handle_interact_normalizes_capitalized_trailing_period() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        // Capitalized input with trailing period — the Gemini repro case.
        super::handle_interact(&ctx, "Tie a strip of cloth.").await;
        super::handle_interact(
            &ctx,
            "I set to work in the potato patch, breaking clods and planting seed.",
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        assert!(
            !logs.is_empty(),
            "handle_interact must emit a text-log for capitalized input"
        );
        // Must normalize to lowercase first char, single trailing period.
        assert!(
            logs.iter().any(|l| l == "You tie a strip of cloth."),
            "expected 'You tie a strip of cloth.' (normalized); got: {logs:?}"
        );
        assert!(
            logs.iter().any(|l| {
                l == "You set to work in the potato patch, breaking clods and planting seed."
            }),
            "first-person task action must become second-person narration; got: {logs:?}"
        );
        assert!(
            !logs.iter().any(|l| l.to_lowercase().starts_with("you i ")),
            "narration must not retain the first-person pronoun; got: {logs:?}"
        );
        // Must NOT produce a double-period.
        assert!(
            !logs.iter().any(|l| l.contains("..")),
            "narration must not contain '..'; got: {logs:?}"
        );
    }

    /// Thread 2: an Interact-classified input WITH addressed_to non-empty must
    /// route to NPC conversation, NOT emit the generic narration.
    ///
    /// In no-LLM mode, "tie a strip of cloth to the thorn bush" is classified as
    /// Interact by the local parser.  With `addressed_to = ["Brigid"]`, the
    /// `is_interact && addressed_to.is_empty()` guard must be false, so the input
    /// falls through to handle_npc_conversation (here: idle text-log, no NPC).
    /// We assert that NO "You tie" narration is emitted.
    #[tokio::test]
    async fn interact_with_addressed_to_routes_to_npc_conversation_not_narration() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(NpcManager::new());
        let config = tokio::sync::Mutex::new(GameConfig::default());
        let conversation = tokio::sync::Mutex::new(ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
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

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // Interact-classified input but with addressed_to set — must NOT narrate.
        super::handle_game_input(
            &ctx,
            "tie a strip of cloth to the thorn bush".to_string(),
            vec!["Brigid".to_string()],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<String> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                p.get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
            })
            .collect();

        // Must NOT emit the interact narration "You tie ..." — it must fall
        // through to NPC conversation routing instead.
        assert!(
            !logs.iter().any(|l| l.starts_with("You tie")),
            "interact with addressed_to must NOT produce action narration; got: {logs:?}"
        );
        // Must still emit something (idle message from NPC conversation path,
        // no NPC present in test world).
        assert!(
            !logs.is_empty(),
            "interact with addressed_to must still produce a text-log (NPC path); got none"
        );
    }

    // ── #1461: broader Interact coverage + no-silent-drop ────────────────────

    /// AC-1 (#1461): "draw a bucket of water" is now caught by parse_intent_local
    /// (local parser broadened to include "draw ") and routes to narrated action.
    ///
    /// This is the primary repro: previously "draw" was not in interact_prefixes,
    /// so the input fell through to NPC conversation with no action narration.
    #[tokio::test]
    async fn draw_water_action_narrates_via_local_parser() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(crate::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(crate::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(crate::ipc::GameConfig::default());
        let conversation = tokio::sync::Mutex::new(crate::ipc::ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None); // no LLM — local parser only
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn crate::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // Primary #1461 repro: "draw a bucket of water from the well and take a
        // long drink" — must emit a narrated action text-log, NOT be silently
        // dropped (or routed to NPC dialogue).
        super::handle_game_input(
            &ctx,
            "draw a bucket of water from the well and take a long drink".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<(String, String)> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                let kind = p
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                let content = p
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                Some((kind, content))
            })
            .collect();

        // Must emit at least one text-log.
        assert!(!logs.is_empty(), "#1461: must emit a text-log; got none");

        // The action text-log must have source == "action" (not "system" idle or NPC).
        let action_logs: Vec<&str> = logs
            .iter()
            .filter(|(k, _)| k == "action")
            .map(|(_, c)| c.as_str())
            .collect();
        assert!(
            !action_logs.is_empty(),
            "#1461: expected kind=action narration for 'draw a bucket'; got: {logs:?}"
        );

        // The narration must mention the action.
        assert!(
            action_logs
                .iter()
                .any(|c| c.contains("draw a bucket") || c.contains("draw")),
            "#1461: narration must reference the action; got: {action_logs:?}"
        );
    }

    /// AC-2 (#1461): "kneel by the well and say a quiet prayer" routes to Interact
    /// narration, not NPC dialogue.  The local parser catches "kneel " prefix
    /// before the LLM sees the trailing "say a quiet prayer" clause.
    #[tokio::test]
    async fn kneel_and_pray_compound_action_narrates() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(crate::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(crate::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(crate::ipc::GameConfig::default());
        let conversation = tokio::sync::Mutex::new(crate::ipc::ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn crate::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        super::handle_game_input(
            &ctx,
            "kneel by the well and say a quiet prayer".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<(String, String)> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                let kind = p
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                let content = p
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                Some((kind, content))
            })
            .collect();

        assert!(!logs.is_empty(), "#1461: must emit a text-log; got none");

        // Must be kind="action", not "system" idle message or NPC dialogue.
        let action_logs: Vec<&str> = logs
            .iter()
            .filter(|(k, _)| k == "action")
            .map(|(_, c)| c.as_str())
            .collect();
        assert!(
            !action_logs.is_empty(),
            "#1461: 'kneel … say a prayer' must emit kind=action narration; got: {logs:?}"
        );
    }

    /// AC-4 (#1461) regression: greeting still routes to NPC conversation (idle
    /// message), not Interact narration.
    #[tokio::test]
    async fn greeting_routes_to_dialogue_not_interact() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(crate::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(crate::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(crate::ipc::GameConfig::default());
        let conversation = tokio::sync::Mutex::new(crate::ipc::ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn crate::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        super::handle_game_input(
            &ctx,
            "hello, good morning".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<(String, String)> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                let kind = p
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                let content = p
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                Some((kind, content))
            })
            .collect();

        // Must emit something (idle message — no NPC present).
        assert!(
            !logs.is_empty(),
            "greeting must produce a text-log; got none"
        );

        // Must NOT produce kind="action" (no interact narration for a greeting).
        assert!(
            logs.iter().all(|(k, _)| k != "action"),
            "greeting must NOT produce kind=action narration; got: {logs:?}"
        );
    }

    // ── #1461 / #1463 Thread 1: None-intent fallback ─────────────────────────

    /// Thread 1 (#1463): when `intent` is `None` (LLM call failed entirely) and
    /// the input is shaped like a physical action, the no-silent-drop fallback
    /// must still fire.
    ///
    /// Previously `unwrap_or(false)` meant a `None` intent → `is_unknown = false`
    /// → fallback skipped → action silently dropped.  After the fix (`unwrap_or(true)`)
    /// a `None` intent is treated as semantically equivalent to `Unknown`.
    ///
    /// In no-LLM mode, `parse_intent_local("push the door")` returns `None`
    /// (the verb "push" is intentionally excluded from `interact_prefixes` so
    /// the LLM would normally resolve it).  With the fix the fallback fires and
    /// emits a narrated action text-log.
    #[tokio::test]
    async fn none_intent_physical_action_narrates_not_dropped() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(crate::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(crate::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(crate::ipc::GameConfig::default());
        let conversation = tokio::sync::Mutex::new(crate::ipc::ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None); // no LLM → parse_intent_local → None
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn crate::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "push the door open" — "push" is NOT in interact_prefixes (intentionally
        // left to the LLM for ambiguity resolution, per the comment above those
        // prefixes).  With no LLM client, parse_intent_local returns None.
        // The no-silent-drop fallback (is_unknown with unwrap_or(true)) must catch
        // this and emit a narrated action text-log.
        super::handle_game_input(
            &ctx,
            "push the door open".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<(String, String)> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                let kind = p
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                let content = p
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                Some((kind, content))
            })
            .collect();

        // Must produce at least one text-log.
        assert!(
            !logs.is_empty(),
            "#1461/#1463 Thread 1: None-intent action must emit a text-log; got none"
        );

        // Must emit kind="action" (the interact-narration path), not "system"
        // (idle message) — which would indicate the input was silently dropped
        // to NPC conversation.
        let action_logs: Vec<&str> = logs
            .iter()
            .filter(|(k, _)| k == "action")
            .map(|(_, c)| c.as_str())
            .collect();
        assert!(
            !action_logs.is_empty(),
            "#1461/#1463 Thread 1: None-intent action must produce kind=action narration; \
             got: {logs:?}"
        );
    }

    /// AC-5 (#1461) regression: "go to the forge" routes as Move.
    #[tokio::test]
    async fn go_to_forge_routes_as_move_not_interact() {
        let emitter = Arc::new(CapturingEmitter::new());
        let world = tokio::sync::Mutex::new(crate::world::WorldState::new());
        let npc_manager = tokio::sync::Mutex::new(crate::npc::manager::NpcManager::new());
        let config = tokio::sync::Mutex::new(crate::ipc::GameConfig::default());
        let conversation = tokio::sync::Mutex::new(crate::ipc::ConversationRuntimeState::new());
        let inference_queue = tokio::sync::Mutex::new(None);
        let client = tokio::sync::Mutex::new(None);
        let cloud_client = tokio::sync::Mutex::new(None);
        let inference_config = crate::config::InferenceConfig::default();

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: Arc::clone(&emitter) as Arc<dyn crate::ipc::EventEmitter>,
            inference_config: &inference_config,
            pronunciations: &[],
            client: &client,
            cloud_client: &cloud_client,
            language: crate::npc::LanguageSettings::english_only(),
            inference_failure_messages: &[],
            idle_messages: &[],
        };

        let transport = make_transport();
        let templates = ReactionTemplates::default();

        // "go to the forge" — must route to movement, not Interact narration.
        // No location matches "the forge" in the default world, so movement
        // will emit a "not found" system message (not kind="action").
        super::handle_game_input(
            &ctx,
            "go to the forge".to_string(),
            vec![],
            &transport,
            &templates,
            || None,
        )
        .await;

        let logs: Vec<(String, String)> = emitter
            .events
            .lock()
            .unwrap()
            .iter()
            .filter(|(n, _)| n == "text-log")
            .filter_map(|(_, p)| {
                let kind = p
                    .get("source")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                let content = p
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)?;
                Some((kind, content))
            })
            .collect();

        assert!(
            !logs.is_empty(),
            "'go to the forge' must produce output; got none"
        );

        // Must NOT produce kind="action" — that would mean Interact fired instead of Move.
        assert!(
            logs.iter().all(|(k, _)| k != "action"),
            "'go to the forge' must NOT produce kind=action; should be movement; got: {logs:?}"
        );
    }
}
