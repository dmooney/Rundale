//! Movement and look handling — resolving player travel and location description.

use std::sync::Arc;

use parish_core::config::InferenceCategory;
use parish_core::debug_snapshot::DebugEvent;
use parish_core::ipc::{text_log, text_log_typed};
use tauri::Emitter;

use crate::AppState;
use crate::events::{
    EVENT_STREAM_END, EVENT_STREAM_TOKEN, EVENT_STREAM_TURN_END, EVENT_TEXT_LOG,
    EVENT_TRAVEL_START, StreamEndPayload, StreamTokenPayload, StreamTurnEndPayload, TextLogPayload,
};

/// Resolves movement to a named location using the shared movement pipeline.
///
/// Delegates all state mutation and message generation to
/// [`parish_core::game_session::apply_movement`], then emits the returned
/// effects to the frontend.
pub(super) async fn handle_movement(target: &str, state: &Arc<AppState>, app: &tauri::AppHandle) {
    use parish_core::game_session::{
        apply_movement, enrich_travel_encounter, roll_travel_encounter,
    };

    let transport = state.transport.default_mode().clone();

    // Apply all movement state changes within a single lock scope to prevent
    // TOCTOU races.
    let (effects, rolled_encounter) = {
        let mut world = state.world.lock().await;
        let mut npc_manager = state.npc_manager.lock().await;
        let effects = apply_movement(
            &mut world,
            &mut npc_manager,
            &state.reaction_templates,
            target,
            &transport,
        );
        let rolled = if effects.world_changed {
            let config = state.config.lock().await;
            if !config.flags.is_disabled("travel-encounters") {
                roll_travel_encounter(&world, &effects)
            } else {
                None
            }
        } else {
            None
        };
        (effects, rolled)
    };

    // Resolve encounter text — LLM-enriched when a reaction client exists
    // and the `travel-encounters-llm` flag is not explicitly disabled.
    let encounter_line: Option<String> = if let Some(rolled) = rolled_encounter.as_ref() {
        let llm_enabled = {
            let cfg = state.config.lock().await;
            !cfg.flags.is_disabled("travel-encounters-llm")
        };
        let (reaction_client, reaction_model) = if llm_enabled {
            let config = state.config.lock().await;
            let base_client = state.client.lock().await;
            config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref())
        } else {
            (None, String::new())
        };
        let text = if let Some(client) = reaction_client.as_ref() {
            enrich_travel_encounter(rolled, client, &reaction_model, 15).await
        } else {
            rolled.canned.text.clone()
        };
        let formatted = format!("  · {text}");
        {
            let mut world = state.world.lock().await;
            world.log(formatted.clone());
        }
        Some(formatted)
    } else {
        None
    };

    // Emit travel-start animation payload first
    if let Some(travel_payload) = &effects.travel_start {
        let _ = app.emit(EVENT_TRAVEL_START, travel_payload);
    }

    // Emit all player-visible messages in order
    for msg in &effects.messages {
        tracing::info!(source = %msg.source, text = %msg.text.trim(), "chat");
        let payload = match msg.subtype {
            Some(st) => text_log_typed(msg.source, &msg.text, st),
            None => text_log(msg.source, &msg.text),
        };
        let _ = app.emit(EVENT_TEXT_LOG, payload);
    }

    // Emit travel encounter line if one fired
    if let Some(line) = encounter_line {
        let _ = app.emit(EVENT_TEXT_LOG, text_log("system", &line));
    }

    // Emit NPC arrival reactions — stream gradually like normal NPC dialogue
    if !effects.arrival_reactions.is_empty() {
        stream_arrival_reactions(&effects.arrival_reactions, state, app).await;
    }

    // Record tier transitions in the debug event log
    if !effects.tier_transitions.is_empty() {
        record_tier_transitions_to_debug(&effects.tier_transitions, state).await;
    }

    // Emit updated world snapshot after a successful move
    if effects.world_changed {
        emit_world_snapshot_after_move(state, app).await;
    }
}

/// Streams NPC arrival reactions to the frontend, mirroring the gradual
/// token-by-token cadence of normal NPC dialogue. Extracted from
/// `handle_movement` (#1200 TD-012).
async fn stream_arrival_reactions(
    arrival_reactions: &[parish_core::npc::reactions::NpcReaction],
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    use parish_core::game_session::stream_reaction_texts;
    use parish_core::ipc::text_log_for_stream_turn;

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
        let world = state.world.lock().await;
        let npc_manager = state.npc_manager.lock().await;
        let config = state.config.lock().await;
        let base_client = state.client.lock().await;
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

    stream_reaction_texts(
        arrival_reactions,
        &all_npcs,
        current_location_id,
        &loc_name,
        tod,
        &weather,
        &introduced,
        reaction_client.as_ref(),
        &reaction_model,
        Some(&state.inference_log),
        &state.language_settings,
        |turn_id, npc_name| {
            // Use `text_log_for_stream_turn` so the UI's streaming-
            // placeholder guard recognises this entry and can finalise
            // (remove) it when the per-turn `stream-turn-end` fires with
            // no tokens — otherwise an empty bubble lingers in the chat
            // (#984 follow-up: "blank NPC reply" reported on the
            // `just demo 2 10` run).
            let _ = app.emit(
                EVENT_TEXT_LOG,
                text_log_for_stream_turn(npc_name.to_string(), String::new(), turn_id),
            );
        },
        |turn_id, source, batch| {
            let _ = app.emit(
                EVENT_STREAM_TOKEN,
                StreamTokenPayload {
                    token: batch.to_string(),
                    turn_id,
                    source: source.to_string(),
                    // Arrival reactions have no reconnect-resume contract
                    // (desktop transport never disconnects); `None` keeps
                    // the wire shape unchanged (#1164).
                    message_id: None,
                },
            );
        },
        |turn_id| {
            let _ = app.emit(EVENT_STREAM_TURN_END, StreamTurnEndPayload { turn_id });
        },
    )
    .await;

    // Finalise the streaming state so the frontend marks the last entry done.
    let _ = app.emit(EVENT_STREAM_END, StreamEndPayload { hints: vec![] });
}

/// Appends tier promotion/demotion transitions to the debug-event ring buffer.
/// Extracted from `handle_movement` (#1200 TD-012).
async fn record_tier_transitions_to_debug(
    tier_transitions: &[parish_core::npc::manager::TierTransition],
    state: &Arc<AppState>,
) {
    let ts = super::snapshot::debug_event_timestamp(state).await;
    let mut debug_events = state.debug_events.lock().await;
    for tt in tier_transitions {
        if debug_events.len() >= crate::DEBUG_EVENT_CAPACITY {
            debug_events.pop_front();
        }
        let direction = if tt.promoted { "promoted" } else { "demoted" };
        debug_events.push_back(DebugEvent {
            timestamp: ts.clone(),
            category: "tier".to_string(),
            message: format!(
                "{} {} {:?} → {:?}",
                tt.npc_name, direction, tt.old_tier, tt.new_tier,
            ),
        });
    }
}

/// Syncs conversation location and emits a fresh world snapshot after a
/// successful move. Extracted from `handle_movement` (#1200 TD-012).
async fn emit_world_snapshot_after_move(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let current_location = {
        let world = state.world.lock().await;
        world.player_location
    };
    let mut conversation = state.conversation.lock().await;
    conversation.sync_location(current_location);
    conversation.last_spoken_at = std::time::Instant::now();
    drop(conversation);

    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = super::snapshot::get_world_snapshot_inner(
        &world,
        Some(&npc_manager),
        &state.pronunciations,
    );
    let _ = app.emit(crate::events::EVENT_WORLD_UPDATE, snapshot);
}

/// Renders the current location description and exits.
pub(super) async fn handle_look(state: &Arc<AppState>, app: &tauri::AppHandle) {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let transport = state.transport.default_mode();
    let text = parish_core::ipc::render_look_text(
        &world,
        &npc_manager,
        transport.speed_m_per_s,
        &transport.label,
        false,
    );
    let _ = app.emit(
        EVENT_TEXT_LOG,
        TextLogPayload {
            id: String::new(),
            stream_turn_id: None,
            source: "system".into(),
            content: text,
            subtype: None,
        },
    );
}
