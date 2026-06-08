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
use crate::game_loop::{GameLoopContext, handle_movement, handle_npc_conversation};
use crate::input::{parse_intent, parse_intent_local};
use crate::ipc::{extract_npc_mentions, render_look_text, text_log};
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
) {
    // Record the raw player input before any parsing so a bug report filed
    // mid-turn carries the exact action that triggered the failure (#1331).
    ctx.conversation.lock().await.record_player_input(&raw);

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
        let result = parse_intent(client, &raw, &model).await;
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
    let is_talk = intent
        .as_ref()
        .map(|i| matches!(i.intent, crate::input::IntentKind::Talk))
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
        match try_handle_move(ctx, move_target, transport, reaction_templates).await {
            MoveDispatch::Handled => return,
            // TODO #40/#56: Move-no-target at a populated location falls through
            // to the NPC conversation path below so the co-located NPC has a
            // chance to reply instead of the player getting a silent system
            // one-liner. Empty-location case is still handled inline above.
            MoveDispatch::FallThroughToNpc => {}
        }
    }

    if is_look {
        handle_look(ctx, transport).await;
        return;
    }

    // Resolve ordered NPC recipients from visible local names.
    // Also validate the LLM's talk_target — only accept it when it resolves
    // to an actual co-located NPC (by name or role vocative). Non-NPC nouns
    // such as "a boat" or the player's own name must not be pushed into the
    // target list: they will generate a spurious "X is not here." message
    // (#1220, #1227).
    let (mentions, validated_talk_target) = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        let mentions = extract_npc_mentions(&raw, &world, &npc_manager);
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
        (mentions, validated)
    };

    // Chip selections (real names from the frontend) come first, then names
    // detected in the player's text, then the LLM's single talk target when it
    // supplied one and resolved to a present NPC. Deduping happens in
    // `resolve_npc_targets` via `find_by_name`, which matches both real and
    // display names.
    let mut targets: Vec<String> =
        Vec::with_capacity(addressed_to.len() + mentions.names.len() + 1);
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
    if let Some(target) = validated_talk_target
        && !targets.iter().any(|t| t == &target)
    {
        targets.push(target);
    }

    handle_npc_conversation(ctx, mentions.remaining, targets, spawn_loading).await;
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
        npc.location = player_loc;
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
}
