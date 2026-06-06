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
    // Validate emoji is in the palette
    if reactions::reaction_description(&emoji).is_none() {
        return Err("Unknown reaction emoji.".to_string());
    }

    // Reject snippets that could inject content into NPC system prompts (#687).
    if message_snippet.chars().any(is_snippet_injection_char) {
        return Err("Message snippet contains disallowed characters.".to_string());
    }

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

    // Persist callback: closes over Arc<AppState> and locks npc_manager to
    // record each reaction in the NPC's reaction_log (#403).
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
