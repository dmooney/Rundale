//! NPC reaction endpoints and background reaction emission.
//!
//! Covers:
//! - `POST /api/react-to-message` — player emoji reaction to an NPC message
//! - [`emit_npc_reactions`] — background task that generates NPC reactions to
//!   player input

use std::sync::Arc;

use axum::Json;
use axum::extract::Extension;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use parish_core::config::InferenceCategory;
use parish_core::ipc::ReactRequest;
use parish_core::npc::reactions;
use parish_core::world::LocationId;

use crate::state::AppState;

// ── Reaction endpoint ──────────────────────────────────────────────────────

/// `POST /api/react-to-message` — player reacts to an NPC message with an emoji.
pub async fn react_to_message(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<ReactRequest>,
) -> impl IntoResponse {
    // Validate emoji is in the palette
    if reactions::reaction_description(&body.emoji).is_none() {
        return StatusCode::BAD_REQUEST;
    }

    // Reject message_snippet values that could inject content into NPC system
    // prompts (#498).  Uses the shared validation from parish-core (#687)
    // so both runtimes are guaranteed identical behaviour.
    if body
        .message_snippet
        .chars()
        .any(parish_core::game_loop::is_snippet_injection_char)
    {
        return StatusCode::BAD_REQUEST;
    }

    // Store the reaction in the target NPC's reaction log
    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&body.npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log
            .add(&body.emoji, &body.message_snippet, now);
    }

    StatusCode::OK
}

/// Generates NPC reactions to a player message and emits events.
///
/// `location` must be the player's location **at the time the message was
/// sent**, captured before any `handle_game_input` call that might move the
/// player. This prevents a race where the player moves between spawn and
/// execution, causing reactions to be attributed to NPCs at the wrong location.
///
/// Delegates to [`parish_core::game_loop::emit_npc_reactions`] (#696 slice 5).
///
/// Pre-captures the NPC list at `location`, resolves the reaction client and
/// feature flags from the session config, constructs an `AppStateEmitter`, and
/// calls the shared implementation which spawns the background reaction task.
/// Resolution happens inside a short-lived spawned task because this function
/// is non-async (called from the request handler) but needs to async-lock the
/// tokio config Mutex.
pub fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    location: LocationId,
    state: &Arc<AppState>,
) {
    let state_clone = Arc::clone(state);
    let player_msg_id = player_msg_id.to_string();
    let player_input = player_input.to_string();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::emitter::AppStateEmitter::new(Arc::clone(state)));

    // Persist callback: closes over Arc<AppState> and locks npc_manager to
    // record each reaction in the NPC's reaction_log (#403). Uses
    // `tokio::spawn` to avoid blocking the reaction task while waiting for
    // the lock — locks are always released at statement-end in this style.
    let state_for_persist = Arc::clone(state);
    let persist: parish_core::game_loop::PersistReactionFn = std::sync::Arc::new(
        move |npc_name: String, emoji: String, player_input: String| {
            let state = Arc::clone(&state_for_persist);
            tokio::spawn(async move {
                let mut npc_manager = state.npc_manager.lock().await;
                if let Some(npc_mut) = npc_manager.find_by_name_mut(&npc_name) {
                    npc_mut.reaction_log.add_player_message_reaction(
                        &emoji,
                        &player_input,
                        chrono::Utc::now(),
                    );
                }
                // Feed the per-session diversity sensor (#995).
                npc_manager.record_reaction_emoji(&emoji);
            });
        },
    );

    tokio::spawn(async move {
        // Pre-capture the NPC list at the given location (the player may have
        // moved by the time the background task runs).
        let (npcs_here, reaction_client, reaction_model, llm_enabled) = {
            let npc_manager = state_clone.npc_manager.lock().await;
            let config = state_clone.config.lock().await;
            let base_client = state_clone.client.lock().await;
            let npcs = npc_manager
                .npcs_at(location)
                .iter()
                .map(|npc| (*npc).clone())
                .collect::<Vec<_>>();
            let (client, model) =
                config.resolve_category_client(InferenceCategory::Reaction, base_client.as_ref());
            let enabled = !config.flags.is_disabled("npc-llm-reactions");
            (npcs, client, model, enabled)
        };

        parish_core::game_loop::emit_npc_reactions(
            player_msg_id,
            player_input,
            npcs_here,
            reaction_client,
            reaction_model,
            llm_enabled,
            emitter,
            persist,
        );
    });
}
