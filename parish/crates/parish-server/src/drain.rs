//! Event-drain helper for the synchronous `/api/command` endpoint.
//!
//! Subscribes to the session event bus *before* a command is dispatched, then
//! collects events until quiescent, returning a structured result the route
//! handler can embed in the HTTP response.

use std::collections::HashMap;
use std::time::Duration;

use parish_core::event_bus::EventStream;
use tokio::time::Instant;

use crate::sync_types::{OutputLine, Role, TravelDetail};

/// Accumulated output from draining a single command's event stream.
pub struct DrainResult {
    pub lines: Vec<OutputLine>,
    pub travel: Option<TravelDetail>,
    /// Most recent `world-update` payload received, if any.
    pub world_update: Option<serde_json::Value>,
    /// Whether the drain stopped because the deadline elapsed.
    pub timed_out: bool,
    /// Whether any `stream-token` events were observed (NPC conversation).
    pub had_streaming: bool,
    /// Production parser/guard measurements emitted by completed NPC turns.
    pub dialogue_quality: Vec<parish_core::ipc::DialogueQualityPayload>,
}

#[derive(Default)]
struct StreamState {
    had_streaming: bool,
    conversation_done: bool,
}

/// Drain the event stream until quiescent, returning all collected output.
///
/// The caller must subscribe to the event bus **before** dispatching the
/// command so no events are missed.
///
/// Quiescence rules:
/// - If streaming NPC tokens are observed, drain until `stream-end` arrives.
/// - Otherwise, stop after 150 ms of bus silence.
/// - In all cases, stop at `deadline`.
pub async fn drain_command(mut stream: EventStream, deadline: Instant) -> DrainResult {
    let mut lines: Vec<OutputLine> = Vec::new();
    // Accumulated NPC token text keyed by turn_id → (source, text)
    let mut npc_turns: HashMap<u64, (String, String)> = HashMap::new();
    let mut world_update: Option<serde_json::Value> = None;
    let mut travel: Option<TravelDetail> = None;
    let mut stream_state = StreamState::default();
    let mut timed_out = false;
    let mut dialogue_quality = Vec::new();

    loop {
        // Silence window: 150 ms normally; wait for stream-end while streaming.
        let silence = if stream_state.had_streaming && !stream_state.conversation_done {
            // NPC is still streaming — extend to the full deadline.
            deadline
        } else {
            let candidate = Instant::now() + Duration::from_millis(150);
            candidate.min(deadline)
        };

        tokio::select! {
            result = stream.recv() => {
                match result {
                    Ok(event) => {
                        process_event(
                            event,
                            &mut lines,
                            &mut npc_turns,
                            &mut world_update,
                            &mut travel,
                            &mut stream_state,
                            &mut dialogue_quality,
                        );
                        // Once stream-end arrives, give the bus one more
                        // short window to drain any trailing world-update.
                        if stream_state.conversation_done {
                            // Flush remaining npc_turns just in case.
                            flush_pending_turns(&mut npc_turns, &mut lines);
                            // Continue draining briefly for world-update.
                        }
                    }
                    Err(_) => break, // channel closed
                }
            }
            _ = tokio::time::sleep_until(silence) => {
                if silence >= deadline && Instant::now() >= deadline {
                    timed_out = true;
                }
                break;
            }
        }
    }

    flush_pending_turns(&mut npc_turns, &mut lines);

    DrainResult {
        lines,
        travel,
        world_update,
        timed_out,
        had_streaming: stream_state.had_streaming,
        dialogue_quality,
    }
}

fn process_event(
    event: parish_core::event_bus::ServerEvent,
    lines: &mut Vec<OutputLine>,
    npc_turns: &mut HashMap<u64, (String, String)>,
    world_update: &mut Option<serde_json::Value>,
    travel: &mut Option<TravelDetail>,
    stream_state: &mut StreamState,
    dialogue_quality: &mut Vec<parish_core::ipc::DialogueQualityPayload>,
) {
    match event.event.as_str() {
        "text-log" => {
            let payload: Option<parish_core::ipc::TextLogPayload> =
                serde_json::from_value(event.payload).ok();
            if let Some(p) = payload {
                // Skip placeholder lines for streaming NPC turns — we build
                // those from accumulated token buffers instead.
                if p.stream_turn_id.is_some() {
                    return;
                }
                let (role, speaker) = source_to_role(&p.source);
                lines.push(OutputLine {
                    id: p.id,
                    role,
                    speaker,
                    text: p.content,
                });
            }
        }
        "stream-token" => {
            let payload: Option<parish_core::ipc::StreamTokenPayload> =
                serde_json::from_value(event.payload).ok();
            if let Some(p) = payload {
                stream_state.had_streaming = true;
                let entry = npc_turns
                    .entry(p.turn_id)
                    .or_insert_with(|| (p.source.clone(), String::new()));
                entry.1.push_str(&p.token);
            }
        }
        "stream-turn-end" => {
            let payload: Option<parish_core::ipc::StreamTurnEndPayload> =
                serde_json::from_value(event.payload).ok();
            if let Some(p) = payload {
                stream_state.had_streaming = true;
                let buffered = npc_turns.remove(&p.turn_id);
                match p.status {
                    parish_core::ipc::StreamTurnStatus::Completed => {
                        let source = p
                            .source
                            .or_else(|| buffered.as_ref().map(|(source, _)| source.clone()));
                        let text = p
                            .final_text
                            .or_else(|| buffered.map(|(_, text)| text))
                            .unwrap_or_default();
                        if let Some(source) = source
                            && !text.is_empty()
                        {
                            let (role, speaker) = source_to_role(&source);
                            lines.push(OutputLine {
                                id: p
                                    .message_id
                                    .unwrap_or_else(|| format!("stream-{}", p.turn_id)),
                                role,
                                speaker,
                                text,
                            });
                        }
                    }
                    parish_core::ipc::StreamTurnStatus::Failed => {
                        // Any accumulated candidate is explicitly discarded. A
                        // non-success termination can never become player text.
                        if let Some(text) = p.recovery_message {
                            lines.push(OutputLine {
                                id: format!("stream-error-{}", p.turn_id),
                                role: Role::System,
                                speaker: "System".to_string(),
                                text,
                            });
                        }
                    }
                }
            }
        }
        "stream-end" => {
            stream_state.conversation_done = true;
            flush_pending_turns(npc_turns, lines);
        }
        "dialogue-corrected" => {
            let payload: Option<parish_core::ipc::DialogueCorrectedPayload> =
                serde_json::from_value(event.payload).ok();
            if let Some(p) = payload {
                if let Some((source, text)) = npc_turns.get_mut(&p.turn_id) {
                    let _ = source;
                    *text = p.corrected_text.clone();
                }
                if let Some(line) = lines
                    .iter_mut()
                    .find(|line| line.id == format!("stream-{}", p.turn_id))
                {
                    line.text = p.corrected_text;
                }
            }
        }
        "dialogue-quality" => {
            if let Ok(payload) =
                serde_json::from_value::<parish_core::ipc::DialogueQualityPayload>(event.payload)
            {
                dialogue_quality.push(payload);
            }
        }
        "world-update" => {
            *world_update = Some(event.payload);
        }
        "travel-start" => {
            let from = event.payload["from"].as_str().unwrap_or("").to_string();
            let to = event.payload["to"].as_str().unwrap_or("").to_string();
            let duration_minutes = event.payload["duration_minutes"].as_u64().unwrap_or(0);
            *travel = Some(TravelDetail {
                from,
                to,
                duration_minutes,
            });
        }
        // Cosmetic UI events — drop.
        _ => {}
    }
}

/// Moves any remaining buffered NPC turns (e.g. in case `stream-turn-end` was
/// missed) into `lines`.
fn flush_pending_turns(
    npc_turns: &mut HashMap<u64, (String, String)>,
    lines: &mut Vec<OutputLine>,
) {
    let mut remaining: Vec<(u64, (String, String))> = npc_turns.drain().collect();
    remaining.sort_by_key(|(id, _)| *id);
    for (turn_id, (source, text)) in remaining {
        if !text.is_empty() {
            let (role, speaker) = source_to_role(&source);
            lines.push(OutputLine {
                id: format!("stream-{turn_id}"),
                role,
                speaker,
                text,
            });
        }
    }
}

fn source_to_role(source: &str) -> (Role, String) {
    match source {
        "player" => (Role::Player, "You".to_string()),
        "system" | "narration" => (Role::System, "System".to_string()),
        name => (Role::Npc, name.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parish_core::event_bus::ServerEvent;

    #[test]
    fn quality_event_is_retained_and_correction_replaces_flushed_text() {
        let mut lines = vec![OutputLine {
            id: "stream-42".to_string(),
            role: Role::Npc,
            speaker: "Máire".to_string(),
            text: "raw model text".to_string(),
        }];
        let mut npc_turns = HashMap::new();
        let mut world_update = None;
        let mut travel = None;
        let mut stream_state = StreamState {
            had_streaming: true,
            conversation_done: false,
        };
        let mut quality = Vec::new();

        process_event(
            ServerEvent {
                event: "dialogue-corrected".to_string(),
                payload: serde_json::json!({
                    "turn_id": 42,
                    "corrected_text": "guarded text"
                }),
            },
            &mut lines,
            &mut npc_turns,
            &mut world_update,
            &mut travel,
            &mut stream_state,
            &mut quality,
        );
        process_event(
            ServerEvent {
                event: "dialogue-quality".to_string(),
                payload: serde_json::json!({
                    "turn_id": 42,
                    "parse_disposition": "full_json",
                    "contract_valid": true,
                    "guard_intervened": true,
                    "guard_reasons": ["display_cap"],
                    "model": "local-test",
                    "generation": {
                        "max_tokens": 768,
                        "temperature": 0.7,
                        "frequency_penalty": 0.5,
                        "json_mode": true,
                        "enable_thinking": false
                    }
                }),
            },
            &mut lines,
            &mut npc_turns,
            &mut world_update,
            &mut travel,
            &mut stream_state,
            &mut quality,
        );

        assert_eq!(lines[0].text, "guarded text");
        assert_eq!(quality.len(), 1);
        assert!(quality[0].contract_valid);
        assert!(quality[0].guard_intervened);
    }

    #[test]
    fn completed_terminal_uses_authoritative_full_text_over_buffered_prefix() {
        let mut lines = Vec::new();
        let mut npc_turns = HashMap::from([(1857, ("Brigid".to_string(), "Plainly,".to_string()))]);
        let mut world_update = None;
        let mut travel = None;
        let mut stream_state = StreamState::default();
        let mut quality = Vec::new();
        process_event(
            ServerEvent {
                event: "stream-turn-end".to_string(),
                payload: serde_json::to_value(parish_core::ipc::StreamTurnEndPayload::completed(
                    1857,
                    Some("msg-1857".to_string()),
                    "Brigid".to_string(),
                    "Plainly, the complete validated response is retained.".to_string(),
                ))
                .unwrap(),
            },
            &mut lines,
            &mut npc_turns,
            &mut world_update,
            &mut travel,
            &mut stream_state,
            &mut quality,
        );
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].id, "msg-1857");
        assert_eq!(
            lines[0].text,
            "Plainly, the complete validated response is retained."
        );
        assert!(npc_turns.is_empty());
    }

    #[test]
    fn failed_terminal_discards_partial_and_returns_recovery() {
        let mut lines = Vec::new();
        let mut npc_turns = HashMap::from([(
            1855,
            ("Brigid".to_string(), "forbidden partial".to_string()),
        )]);
        let mut world_update = None;
        let mut travel = None;
        let mut stream_state = StreamState::default();
        let mut quality = Vec::new();
        process_event(
            ServerEvent {
                event: "stream-turn-end".to_string(),
                payload: serde_json::to_value(parish_core::ipc::StreamTurnEndPayload::failed(
                    1855,
                    Some("msg-1855".to_string()),
                    Some("Nothing was added. Please try again.".to_string()),
                ))
                .unwrap(),
            },
            &mut lines,
            &mut npc_turns,
            &mut world_update,
            &mut travel,
            &mut stream_state,
            &mut quality,
        );
        assert_eq!(lines.len(), 1);
        assert!(lines[0].role == Role::System);
        assert_eq!(lines[0].text, "Nothing was added. Please try again.");
        assert!(lines.iter().all(|line| !line.text.contains("forbidden")));
        assert!(npc_turns.is_empty());
    }
}
