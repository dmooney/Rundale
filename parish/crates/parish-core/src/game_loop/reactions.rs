//! Shared NPC-reaction helpers extracted from all backends (#696 slice 4).
//!
//! # `is_snippet_injection_char`
//!
//! Shared validation for `react_to_message` end-points: rejects characters that
//! could escape out of NPC system-prompt templates (#498 / #687).  The function
//! was previously duplicated in `parish-server/src/routes.rs` and
//! `parish-tauri/src/commands.rs`; this canonical copy lives in `parish-core`
//! so both runtimes import the same definition.
//!
//! # `emit_npc_reactions`
//!
//! The async core of the NPC-reaction pipeline.  Callers must spawn this as a
//! background task and supply an `on_persist` callback that persists each
//! `(npc_name, emoji)` pair back to the NPC's reaction log.
//!
//! # Architecture gate
//!
//! This module must remain backend-agnostic.  It does **not** import `axum`,
//! `tauri`, or any crate in `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::inference::AnyClient;
use crate::ipc::{EventEmitter, NPC_REACTION_CONCURRENCY, NpcReactionPayload, capitalize_first};
use crate::npc::{Npc, reactions};

/// Boxed async future for persistence callbacks.
pub type PersistFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

// ── Injection-safety validation ───────────────────────────────────────────────

/// Returns `true` if `c` should be rejected from a reaction's
/// `message_snippet` because it could break out of the NPC system prompt
/// (#498).
///
/// Rejects:
/// - `"` and `\\` — escape out of surrounding JSON/string literals.
/// - Any Unicode control character (`is_control()`), which covers ASCII
///   C0 controls (`\n`, `\r`, `\t`, `\0`, etc.) and C1 controls including
///   U+0085 NEXT LINE.
/// - U+2028 LINE SEPARATOR and U+2029 PARAGRAPH SEPARATOR — not `control`
///   under Rust's definition but treated as line breaks by many LLMs.
pub fn is_snippet_injection_char(c: char) -> bool {
    c == '"' || c == '\\' || c == '\u{2028}' || c == '\u{2029}' || c.is_control()
}

// ── Background reaction task ──────────────────────────────────────────────────

/// Core of the NPC-reaction pipeline.  **Must be spawned** by the caller as a
/// background task; the function itself does not spawn.
///
/// # Parameters
///
/// - `npcs_here`: NPCs at the player's location **at message-send time** —
///   callers must capture this before any movement that might change the
///   location.  Prevents TOCTOU races where the player moves between dispatch
///   and execution (#406).
/// - `llm_enabled`: whether the `npc-llm-reactions` feature flag is on.
/// - `reaction_client`: optional LLM client for inference-backed reactions.
/// - `reaction_model`: model name to pass to the reaction inference call.
/// - `on_persist`: async callback called with `(npc_name, emoji)` for each
///   reaction so the caller can persist it to the NPC's reaction log.  Pass
///   a no-op closure (e.g. `|_, _| Box::pin(async {})`) to skip persistence.
/// - `emitter`: event emitter for `"npc-reaction"` events.
/// - `player_msg_id`: opaque message ID threaded through to the frontend.
/// - `player_input`: the original player message text.
#[allow(clippy::too_many_arguments)]
pub async fn emit_npc_reactions<F>(
    npcs_here: Vec<Npc>,
    llm_enabled: bool,
    reaction_client: Option<AnyClient>,
    reaction_model: String,
    on_persist: F,
    emitter: Arc<dyn EventEmitter>,
    player_msg_id: String,
    player_input: String,
) where
    F: Fn(String, String) -> PersistFuture + Send + 'static,
{
    if npcs_here.is_empty() {
        return;
    }

    // Run per-NPC inference concurrently, bounded to NPC_REACTION_CONCURRENCY
    // simultaneous calls so a busy location can't exhaust the LLM connection
    // pool (#406).
    let sem = Arc::new(Semaphore::new(NPC_REACTION_CONCURRENCY));
    let mut join_set = tokio::task::JoinSet::new();

    for npc in npcs_here {
        let sem = Arc::clone(&sem);
        let client = reaction_client.clone();
        let model = reaction_model.clone();
        let input = player_input.clone();

        join_set.spawn(async move {
            // Acquire a permit before starting the (potentially slow) LLM call.
            let _permit = sem.acquire().await.ok();

            // Try LLM path first; fall back to rule-based on any failure (#404).
            let emoji = if llm_enabled {
                if let Some(ref c) = client {
                    reactions::infer_player_message_reaction(
                        c,
                        &model,
                        &npc,
                        &input,
                        Duration::from_secs(2),
                    )
                    .await
                    .or_else(|| reactions::generate_rule_reaction(&input))
                } else {
                    reactions::generate_rule_reaction(&input)
                }
            } else {
                reactions::generate_rule_reaction(&input)
            };

            (npc.name.clone(), emoji)
        });
    }

    // Collect results as tasks finish, then persist + emit each reaction.
    while let Some(result) = join_set.join_next().await {
        let (npc_name, emoji) = match result {
            Ok((name, Some(emoji))) => (name, emoji),
            Ok((_, None)) => continue,
            Err(e) if e.is_panic() => {
                tracing::error!(error = %e, "npc reaction task panicked");
                continue;
            }
            Err(e) if e.is_cancelled() => {
                tracing::debug!("npc reaction task cancelled (shutdown)");
                continue;
            }
            Err(e) => {
                tracing::warn!(error = %e, "npc reaction task ended unexpectedly");
                continue;
            }
        };

        // Persist to reaction_log so NPC memory is maintained (#403).
        on_persist(npc_name.clone(), emoji.clone()).await;

        emitter.emit_event(
            "npc-reaction",
            serde_json::to_value(NpcReactionPayload {
                message_id: player_msg_id.clone(),
                emoji,
                source: capitalize_first(&npc_name),
            })
            .unwrap_or(serde_json::Value::Null),
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_snippet_injection_char ─────────────────────────────────────────────

    #[test]
    fn rejects_double_quote() {
        assert!(is_snippet_injection_char('"'));
    }

    #[test]
    fn rejects_backslash() {
        assert!(is_snippet_injection_char('\\'));
    }

    #[test]
    fn rejects_line_separator() {
        assert!(is_snippet_injection_char('\u{2028}'));
    }

    #[test]
    fn rejects_paragraph_separator() {
        assert!(is_snippet_injection_char('\u{2029}'));
    }

    #[test]
    fn rejects_control_chars() {
        // ASCII C0 controls.
        assert!(is_snippet_injection_char('\0'));
        assert!(is_snippet_injection_char('\n'));
        assert!(is_snippet_injection_char('\r'));
        assert!(is_snippet_injection_char('\t'));
        // U+0085 NEXT LINE (C1 control — covered by is_control()).
        assert!(is_snippet_injection_char('\u{0085}'));
    }

    #[test]
    fn allows_normal_ascii() {
        for c in ('a'..='z').chain('A'..='Z').chain('0'..='9') {
            assert!(
                !is_snippet_injection_char(c),
                "char {c:?} should be allowed"
            );
        }
        for c in [' ', ',', '.', '!', '?', '\'', '-'] {
            assert!(
                !is_snippet_injection_char(c),
                "char {c:?} should be allowed"
            );
        }
    }

    #[test]
    fn allows_safe_unicode() {
        // Typical Unicode characters used in Irish text.
        for c in ['á', 'é', 'í', 'ó', 'ú', 'Á', 'É', 'Í', 'Ó', 'Ú'] {
            assert!(
                !is_snippet_injection_char(c),
                "char {c:?} should be allowed"
            );
        }
    }

    #[test]
    fn clean_snippet_passes_filter() {
        let snippet = "He said hello to the priest.";
        assert!(!snippet.chars().any(is_snippet_injection_char));
    }

    #[test]
    fn injection_attempt_fails_filter() {
        let attack = "\" injection attempt";
        assert!(attack.chars().any(is_snippet_injection_char));
    }
}
