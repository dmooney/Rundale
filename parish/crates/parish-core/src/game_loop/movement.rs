//! Shared movement handler extracted from all backends (#696 slice 4).
//!
//! [`handle_movement`] encapsulates everything the server and Tauri runtimes do
//! identically when the player moves: applying world-state changes, resolving
//! optional LLM-enriched travel encounters, streaming NPC arrival reactions, and
//! emitting the updated world snapshot.
//!
//! Backend-specific behaviour (Tauri debug-event logging for tier transitions,
//! server-side admin tracking) is left to the call site.  The function returns
//! the [`GameEffects`] so callers can inspect `tier_transitions` and any other
//! fields they care about.
//!
//! # Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::sync::Arc;

use crate::config::InferenceCategory;
use crate::game_loop::GameLoopContext;
use crate::game_session::{
    GameEffects, apply_movement, enrich_travel_encounter, roll_travel_encounter,
    stream_reaction_texts,
};
use crate::ipc::{
    StreamEndPayload, StreamTokenPayload, compute_name_hints, snapshot_from_world, text_log,
    text_log_typed,
};
use crate::npc::reactions::ReactionTemplates;
use crate::world::transport::TransportMode;

/// Resolves player movement to `target`, applies all game-state changes, and
/// emits all player-visible events through `ctx.emitter`.
///
/// Returns the [`GameEffects`] produced by the movement so that callers can
/// inspect runtime-specific fields (e.g. `tier_transitions` for Tauri debug
/// panels).  All text-log, stream-token, stream-end, travel-start, and
/// world-update events are emitted internally via `ctx.emitter`.
///
/// # Emitted events
///
/// | Event | When |
/// |---|---|
/// | `"travel-start"` | When the player actually moves (payload: [`TravelStartPayload`]) |
/// | `"text-log"` | For each movement message and optional encounter line |
/// | `"stream-token"` | For each NPC reaction text chunk |
/// | `"stream-end"` | After all arrival reactions complete |
/// | `"world-update"` | When `effects.world_changed` is true |
pub async fn handle_movement(
    ctx: &GameLoopContext<'_>,
    target: &str,
    transport: &TransportMode,
    reaction_templates: &ReactionTemplates,
) -> GameEffects {
    // Apply movement within a single lock scope to prevent TOCTOU races.
    let (effects, rolled_encounter) = {
        let mut world = ctx.world.lock().await;
        let mut npc_manager = ctx.npc_manager.lock().await;
        let effects = apply_movement(
            &mut world,
            &mut npc_manager,
            reaction_templates,
            target,
            transport,
        );
        // Travel encounter — default-on, kill-switchable via `travel-encounters` flag.
        let encounters_enabled = {
            let cfg = ctx.config.lock().await;
            !cfg.flags.is_disabled("travel-encounters")
        };
        let rolled = if effects.world_changed && encounters_enabled {
            roll_travel_encounter(&world, &effects)
        } else {
            None
        };
        (effects, rolled)
    };

    // Resolve the encounter text — LLM-enriched if a reaction client is
    // available and the `travel-encounters-llm` flag is not disabled.
    // Falls back to canned text on any error/timeout.
    let encounter_line: Option<String> = if let Some(rolled) = rolled_encounter.as_ref() {
        let llm_enabled = {
            let cfg = ctx.config.lock().await;
            !cfg.flags.is_disabled("travel-encounters-llm")
        };
        let (reaction_client, reaction_model) = if llm_enabled {
            let config = ctx.config.lock().await;
            let base_client = ctx.client.lock().await;
            config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref())
        } else {
            (None, String::new())
        };
        let text = if let Some(client) = reaction_client.as_ref() {
            enrich_travel_encounter(rolled, client, &reaction_model, 15).await
        } else {
            rolled.canned.text.clone()
        };
        // Log the (possibly enriched) line into the world text log so
        // persistence and debug panels see exactly one encounter line.
        let formatted = format!("  · {text}");
        {
            let mut world = ctx.world.lock().await;
            world.log(formatted.clone());
        }
        Some(formatted)
    } else {
        None
    };

    // Emit travel-start animation payload before text messages
    if let Some(ref travel_payload) = effects.travel_start {
        ctx.emitter.emit_event(
            "travel-start",
            serde_json::to_value(travel_payload).unwrap_or(serde_json::Value::Null),
        );
    }

    // Emit each player-visible message (honour the optional subtype for frontend styling)
    for msg in &effects.messages {
        let payload = match msg.subtype {
            Some(st) => text_log_typed(msg.source, &msg.text, st),
            None => text_log(msg.source, &msg.text),
        };
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(payload).unwrap_or(serde_json::Value::Null),
        );
    }

    // Emit travel encounter line if one fired
    if let Some(ref line) = encounter_line {
        ctx.emitter.emit_event(
            "text-log",
            serde_json::to_value(text_log("system", line.as_str()))
                .unwrap_or(serde_json::Value::Null),
        );
    }

    // Emit NPC arrival reactions — stream gradually like normal NPC dialogue
    if !effects.arrival_reactions.is_empty() {
        let (
            all_npcs,
            current_location_id,
            loc_name,
            tod,
            weather,
            introduced,
            reaction_client,
            reaction_model,
        ) = {
            let world = ctx.world.lock().await;
            let npc_manager = ctx.npc_manager.lock().await;
            let config = ctx.config.lock().await;
            let base_client = ctx.client.lock().await;
            let (rc, rm) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            (
                npc_manager.all_npcs().cloned().collect::<Vec<_>>(),
                world.player_location,
                world
                    .current_location_data()
                    .map(|d| d.name.clone())
                    .unwrap_or_default(),
                world.clock.time_of_day(),
                world.weather.to_string(),
                npc_manager.introduced_set(),
                rc,
                rm,
            )
        };

        let emitter_clone = Arc::clone(&ctx.emitter);
        let emitter_for_token = Arc::clone(&ctx.emitter);
        stream_reaction_texts(
            &effects.arrival_reactions,
            &all_npcs,
            current_location_id,
            &loc_name,
            tod,
            &weather,
            &introduced,
            reaction_client.as_ref(),
            &reaction_model,
            None, // inference_log: None — shared code doesn't hold runtime-specific logs
            &ctx.language,
            move |_turn_id, npc_name| {
                emitter_clone.emit_event(
                    "text-log",
                    serde_json::to_value(text_log(npc_name, String::new()))
                        .unwrap_or(serde_json::Value::Null),
                );
            },
            move |turn_id, source, batch| {
                emitter_for_token.emit_event(
                    "stream-token",
                    serde_json::to_value(StreamTokenPayload {
                        token: batch.to_string(),
                        turn_id,
                        source: source.to_string(),
                    })
                    .unwrap_or(serde_json::Value::Null),
                );
            },
        )
        .await;

        // Finalise the streaming state so the frontend marks the last entry done.
        ctx.emitter.emit_event(
            "stream-end",
            serde_json::to_value(StreamEndPayload { hints: vec![] })
                .unwrap_or(serde_json::Value::Null),
        );
    }

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
        let current_location = {
            let world = ctx.world.lock().await;
            world.player_location
        };
        {
            let mut conversation = ctx.conversation.lock().await;
            conversation.sync_location(current_location);
            conversation.last_spoken_at = std::time::Instant::now();
        }

        let snapshot = {
            let world = ctx.world.lock().await;
            let npc_manager = ctx.npc_manager.lock().await;
            let mut ws = snapshot_from_world(&world, transport);
            ws.name_hints = compute_name_hints(&world, &npc_manager, ctx.pronunciations);
            ws
        };
        ctx.emitter.emit_event(
            "world-update",
            serde_json::to_value(snapshot).unwrap_or(serde_json::Value::Null),
        );
    }

    effects
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
    async fn handle_movement_unknown_target_emits_text_log() {
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
        super::handle_movement(&ctx, "nowhere-land", &transport, &templates).await;

        let names = emitter.event_names();
        assert!(
            names.iter().any(|n| n == "text-log"),
            "expected text-log for unknown destination; got {names:?}"
        );
        // No world-update when movement fails.
        assert!(
            !names.iter().any(|n| n == "world-update"),
            "expected no world-update when destination unknown; got {names:?}"
        );
    }
}
