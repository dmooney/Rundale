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
        if let Some(target) = move_target {
            handle_movement(ctx, &target, transport, reaction_templates).await;
        } else {
            ctx.emitter.emit_event(
                "text-log",
                serde_json::to_value(text_log("system", "And where would ye be off to?"))
                    .unwrap_or(serde_json::Value::Null),
            );
        }
        return;
    }

    if is_look {
        handle_look(ctx, transport).await;
        return;
    }

    // `talk to <name>` / `speak to <name>` — bypass @mention parsing and
    // route directly to the multi-target dispatch loop with this single
    // addressee.  The chip-selection list still gets prepended below.
    //
    // Pass `raw` (the original input) rather than an empty string so that
    // dialogue like "Hello Brigid, good morning!" is not discarded when the
    // intent parser classifies it as Talk.  An empty `raw` still produces the
    // "say something first" prompt, which is correct for bare "talk to X".
    if is_talk && let Some(target) = talk_target {
        let mut targets: Vec<String> = Vec::with_capacity(addressed_to.len() + 1);
        for name in addressed_to {
            if !targets.iter().any(|t| t == &name) {
                targets.push(name);
            }
        }
        if !targets.iter().any(|t| t == &target) {
            targets.push(target);
        }
        handle_npc_conversation(ctx, raw, targets, spawn_loading).await;
        return;
    }

    // Resolve ordered NPC recipients from visible local names.
    let mentions = {
        let world = ctx.world.lock().await;
        let npc_manager = ctx.npc_manager.lock().await;
        extract_npc_mentions(&raw, &world, &npc_manager)
    };

    // Chip selections (real names from the frontend) come first, then any
    // inline @mentions that aren't already in the chip set.  Deduping happens
    // in `resolve_npc_targets` via `find_by_name`, which matches both real
    // and display names.
    let mut targets: Vec<String> = Vec::with_capacity(addressed_to.len() + mentions.names.len());
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
}
