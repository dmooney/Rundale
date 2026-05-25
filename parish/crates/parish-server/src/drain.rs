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
    let mut had_streaming = false;
    let mut stream_conversation_done = false;
    let mut timed_out = false;

    loop {
        // Silence window: 150 ms normally; wait for stream-end while streaming.
        let silence = if had_streaming && !stream_conversation_done {
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
                            &mut had_streaming,
                            &mut stream_conversation_done,
                        );
                        // Once stream-end arrives, give the bus one more
                        // short window to drain any trailing world-update.
                        if stream_conversation_done {
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
        had_streaming,
    }
}

fn process_event(
    event: parish_core::event_bus::ServerEvent,
    lines: &mut Vec<OutputLine>,
    npc_turns: &mut HashMap<u64, (String, String)>,
    world_update: &mut Option<serde_json::Value>,
    travel: &mut Option<TravelDetail>,
    had_streaming: &mut bool,
    stream_done: &mut bool,
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
                *had_streaming = true;
                let entry = npc_turns
                    .entry(p.turn_id)
                    .or_insert_with(|| (p.source.clone(), String::new()));
                entry.1.push_str(&p.token);
            }
        }
        "stream-turn-end" => {
            let payload: Option<parish_core::ipc::StreamTurnEndPayload> =
                serde_json::from_value(event.payload).ok();
            if let Some(p) = payload
                && let Some((source, text)) = npc_turns.remove(&p.turn_id)
            {
                let (role, speaker) = source_to_role(&source);
                lines.push(OutputLine {
                    id: format!("stream-{}", p.turn_id),
                    role,
                    speaker,
                    text,
                });
            }
        }
        "stream-end" => {
            *stream_done = true;
            flush_pending_turns(npc_turns, lines);
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
