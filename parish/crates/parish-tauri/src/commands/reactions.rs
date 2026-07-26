//! NPC reaction commands — emoji reactions and background reaction emission.

use std::sync::Arc;

use parish_core::config::InferenceCategory;
use parish_core::npc::reactions;
use parish_core::world::LocationId;

use crate::AppState;

// Snippet injection validation is shared via parish_core::game_loop::is_snippet_injection_char
// (#687 security parity). Delegating here guarantees server and Tauri use identical logic.
pub use parish_core::game_loop::is_snippet_injection_char;

/// Player reacts to an NPC message with an emoji.
#[tauri::command]
pub async fn react_to_message(
    npc_name: String,
    message_snippet: String,
    emoji: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    do_react_to_message(&state, npc_name, message_snippet, emoji).await
}

pub(crate) async fn do_react_to_message(
    state: &Arc<AppState>,
    npc_name: String,
    message_snippet: String,
    emoji: String,
) -> Result<(), String> {
    // Validate emoji is in the palette
    if reactions::reaction_description(&emoji).is_none() {
        return Err("Unknown reaction emoji.".to_string());
    }

    // Reject snippets that could inject content into NPC system prompts (#687).
    if message_snippet.chars().any(is_snippet_injection_char) {
        return Err("Message snippet contains disallowed characters.".to_string());
    }

    // Staged turns install a cloned NPC manager. Serialize this mutation with
    // that install so an accepted reaction cannot be erased.
    let _persistence_guard = state.persistence_gate.lock().await;
    let mut npc_manager = state.npc_manager.lock().await;
    if let Some(npc) = npc_manager.find_by_name_mut(&npc_name) {
        let now = chrono::Utc::now();
        npc.reaction_log.add(&emoji, &message_snippet, now);
    }

    Ok(())
}

/// Delegates to [`parish_core::game_loop::emit_npc_reactions`] (#696 slice 5).
///
/// `location` must be the player's location **at the time the message was
/// sent**, captured before any `handle_game_input` call that might move the
/// player. This prevents a race where the player moves between spawn and
/// execution, causing reactions to be attributed to NPCs at the wrong location.
///
/// Pre-captures the NPC list, resolves the reaction client and feature flags,
/// constructs a `TauriEmitter`, and delegates to the shared implementation.
pub(super) fn emit_npc_reactions(
    player_msg_id: &str,
    player_input: &str,
    location: LocationId,
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) {
    let state_clone = Arc::clone(state);
    let player_msg_id = player_msg_id.to_string();
    let player_input = player_input.to_string();
    let emitter: std::sync::Arc<dyn parish_core::ipc::EventEmitter> =
        std::sync::Arc::new(crate::events::TauriEmitter::new(app.clone()));

    tokio::spawn(async move {
        // Pre-capture the NPC list at the given location (the player may have
        // moved by the time the background task runs).
        let (npcs_here, reaction_client, reaction_model, llm_enabled, context_bus, context_epoch) = {
            let world = state_clone.world.lock().await;
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
            (
                npcs,
                client,
                model,
                enabled,
                world.event_bus.clone(),
                world.event_bus.context_epoch(),
            )
        };
        let context_is_valid: parish_core::game_loop::ReactionContextValidFn = {
            let context_bus = context_bus.clone();
            std::sync::Arc::new(move || context_bus.context_epoch() == context_epoch)
        };
        let state_for_persist = Arc::clone(&state_clone);
        let persist_context = Arc::clone(&context_is_valid);
        let persist: parish_core::game_loop::PersistReactionFn = std::sync::Arc::new(
            move |npc_name: String, emoji: String, player_input: String| {
                let state = Arc::clone(&state_for_persist);
                let context_is_valid = Arc::clone(&persist_context);
                tokio::spawn(async move {
                    let _persistence_guard = state.persistence_gate.lock().await;
                    if !context_is_valid() {
                        return;
                    }
                    let mut npc_manager = state.npc_manager.lock().await;
                    if let Some(npc_mut) = npc_manager.find_by_name_mut(&npc_name) {
                        npc_mut.reaction_log.add_player_message_reaction(
                            &emoji,
                            &player_input,
                            chrono::Utc::now(),
                        );
                    }
                    npc_manager.record_reaction_emoji(&emoji);
                });
            },
        );

        parish_core::game_loop::emit_npc_reactions(
            player_msg_id,
            player_input,
            npcs_here,
            reaction_client,
            reaction_model,
            llm_enabled,
            emitter,
            persist,
            context_is_valid,
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cmd_tests::test_app_state;

    // ── is_snippet_injection_char ───────────────────────────────────────────

    #[test]
    fn snippet_injection_rejects_double_quote() {
        assert!(is_snippet_injection_char('"'));
    }

    #[test]
    fn snippet_injection_rejects_backslash() {
        assert!(is_snippet_injection_char('\\'));
    }

    #[test]
    fn snippet_injection_rejects_line_separator() {
        assert!(is_snippet_injection_char('\u{2028}'));
    }

    #[test]
    fn snippet_injection_rejects_paragraph_separator() {
        assert!(is_snippet_injection_char('\u{2029}'));
    }

    #[test]
    fn snippet_injection_rejects_null_byte() {
        assert!(is_snippet_injection_char('\0'));
    }

    #[test]
    fn snippet_injection_accepts_normal_chars() {
        for c in "abcdefghijklmnopqrstuvwxyz ÁÉÍÓÚ,.!?'".chars() {
            assert!(
                !is_snippet_injection_char(c),
                "char {:?} should be allowed",
                c
            );
        }
    }

    #[tokio::test]
    async fn player_reaction_waits_for_staged_turn_barrier() {
        let state = test_app_state();
        let location = state.world.lock().await.player_location;
        let mut molly = parish_core::npc::Npc::new_test_npc();
        molly.id = parish_core::npc::NpcId(77);
        molly.name = "Molly".to_string();
        molly.set_location(location);
        state.npc_manager.lock().await.add_npc(molly);

        let held = state.persistence_gate.lock().await;
        let state_for_reaction = Arc::clone(&state);
        let reaction = tokio::spawn(async move {
            do_react_to_message(
                &state_for_reaction,
                "Molly".to_string(),
                "Hello there".to_string(),
                "😊".to_string(),
            )
            .await
        });
        tokio::task::yield_now().await;
        assert!(
            !reaction.is_finished(),
            "reaction mutation must wait while a staged turn owns persistence_gate"
        );
        drop(held);
        tokio::time::timeout(std::time::Duration::from_secs(1), reaction)
            .await
            .expect("reaction should finish once candidate install is complete")
            .unwrap()
            .unwrap();

        let location = state.world.lock().await.player_location;
        assert_eq!(
            state
                .npc_manager
                .lock()
                .await
                .find_by_name("Molly", location)
                .unwrap()
                .reaction_log
                .len(),
            1
        );
    }
}
