//! Projection of committed wire emissions onto transcript events.
//!
//! The shared game loop reports player-visible output as named wire events
//! (`text-log`, `stream-turn-end`, …) that the desktop clients already
//! consume. Transcript events are derived from exactly those emissions once
//! an attempt commits, so both presentations come from one source.
//!
//! Every event name the loop can emit is classified here, either as
//! transcript-bearing or as presentation-only; a fitness test fails if the
//! loop gains an emission this module does not classify.

use std::collections::HashMap;

use serde_json::Value;

use super::transcript::{EventBuilder, PendingEvent, TranscriptEventKind};

/// Emission names that carry transcript content.
pub const TRANSCRIPT_EMISSIONS: &[&str] = &[
    "text-log",
    "stream-token",
    "stream-turn-end",
    "dialogue-corrected",
    "world-update",
];

/// Emission names that are presentation only (animation, telemetry,
/// reactions) and never become transcript events.
pub const PRESENTATION_EMISSIONS: &[&str] = &[
    "travel-start",
    "stream-end",
    "dialogue-quality",
    "npc-reaction",
    "loading",
];

struct OpenStream {
    speaker: String,
    subtype: Option<String>,
    tokens: String,
    position: usize,
}

/// Projects one attempt's committed emissions, in order.
///
/// `location` is the player's location name before the attempt; a
/// `world-update` naming a different location becomes `SceneChanged`.
/// Player echo lines are skipped: the accepted command is its own event.
pub fn project_emissions(
    emissions: &[(String, Value)],
    builder: &mut EventBuilder,
    location: Option<&str>,
) -> Vec<PendingEvent> {
    let mut events: Vec<Option<PendingEvent>> = Vec::new();
    let mut streams: HashMap<u64, OpenStream> = HashMap::new();
    let mut dialogue_by_turn: HashMap<u64, usize> = HashMap::new();
    let mut location = location.map(str::to_string);

    let text =
        |payload: &Value, key: &str| payload.get(key).and_then(Value::as_str).map(str::to_string);

    for (name, payload) in emissions {
        match name.as_str() {
            "text-log" => {
                let source = text(payload, "source").unwrap_or_default();
                let content = text(payload, "content").unwrap_or_default();
                let subtype = text(payload, "subtype");
                if let Some(turn_id) = payload.get("stream_turn_id").and_then(Value::as_u64) {
                    // A streaming placeholder: its text arrives later.
                    events.push(None);
                    streams.insert(
                        turn_id,
                        OpenStream {
                            speaker: source,
                            subtype,
                            tokens: content,
                            position: events.len() - 1,
                        },
                    );
                    continue;
                }
                if source == "player" || content.trim().is_empty() {
                    continue;
                }
                let kind = if subtype.as_deref() == Some("action") {
                    TranscriptEventKind::ActionResult
                } else if source == "system" {
                    TranscriptEventKind::Narration
                } else if source == "You" {
                    // The player's own line, repeated beside an absent
                    // addressee (#1493); the command event already has it.
                    continue;
                } else {
                    TranscriptEventKind::NpcDialogue
                };
                let mut event = builder.event(kind);
                if source != "system" {
                    event.speaker = Some(source);
                }
                event.content = Some(content);
                events.push(Some(event));
            }
            "stream-token" => {
                if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_u64)
                    && let Some(stream) = streams.get_mut(&turn_id)
                {
                    stream
                        .tokens
                        .push_str(&text(payload, "token").unwrap_or_default());
                }
            }
            "stream-turn-end" => {
                let Some(turn_id) = payload.get("turn_id").and_then(Value::as_u64) else {
                    continue;
                };
                let stream = streams.remove(&turn_id);
                let status = text(payload, "status").unwrap_or_default();
                if status == "failed" {
                    if let Some(message) = text(payload, "recovery_message") {
                        let mut event = builder.event(TranscriptEventKind::Error);
                        event.content = Some(message);
                        events.push(Some(event));
                    }
                    continue;
                }
                let Some(stream) = stream else { continue };
                let content = text(payload, "final_text").unwrap_or(stream.tokens);
                if content.trim().is_empty() {
                    continue;
                }
                let kind = if stream.subtype.as_deref() == Some("action") {
                    TranscriptEventKind::ActionResult
                } else {
                    TranscriptEventKind::NpcDialogue
                };
                let mut event = builder.event(kind);
                event.speaker = text(payload, "source").or(Some(stream.speaker));
                event.content = Some(content);
                dialogue_by_turn.insert(turn_id, stream.position);
                events[stream.position] = Some(event);
            }
            "dialogue-corrected" => {
                if let Some(turn_id) = payload.get("turn_id").and_then(Value::as_u64)
                    && let Some(&position) = dialogue_by_turn.get(&turn_id)
                    && let Some(Some(event)) = events.get_mut(position)
                {
                    event.content = text(payload, "corrected_text");
                }
            }
            "world-update" => {
                let Some(name) = text(payload, "location_name") else {
                    continue;
                };
                if location.as_deref() != Some(name.as_str()) {
                    if location.is_some() {
                        let mut event = builder.event(TranscriptEventKind::SceneChanged);
                        event.content = Some(name.clone());
                        if let Some(id) = payload.get("location_id").and_then(Value::as_u64) {
                            event.metadata.insert("sceneID".to_string(), id.to_string());
                        }
                        events.push(Some(event));
                    }
                    location = Some(name);
                }
            }
            _ => {}
        }
    }
    events.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::ids::{ExecutionAttemptId, LogicalRequestId};
    use serde_json::json;

    fn builder() -> EventBuilder {
        EventBuilder::new(
            LogicalRequestId::new("r"),
            Some(ExecutionAttemptId::new("a")),
            1,
        )
    }

    #[test]
    fn a_dialogue_turn_projects_its_canonical_line_in_placeholder_order() {
        let emissions = vec![
            (
                "text-log".to_string(),
                json!({"source":"player","content":"> hello"}),
            ),
            (
                "text-log".to_string(),
                json!({"source":"Peig","content":"","stream_turn_id":7}),
            ),
            (
                "text-log".to_string(),
                json!({"source":"system","content":"A dog barks."}),
            ),
            (
                "stream-token".to_string(),
                json!({"token":"God bless","turn_id":7,"source":"Peig"}),
            ),
            (
                "stream-turn-end".to_string(),
                json!({"turn_id":7,"status":"completed","source":"Peig","final_text":"God bless ye."}),
            ),
            (
                "dialogue-corrected".to_string(),
                json!({"turn_id":7,"corrected_text":"God bless ye kindly."}),
            ),
            ("stream-end".to_string(), json!({"hints":[]})),
        ];
        let events = project_emissions(&emissions, &mut builder(), Some("Kilteevan"));
        let summary: Vec<_> = events
            .iter()
            .map(|event| {
                (
                    event.kind.clone(),
                    event.speaker.clone(),
                    event.content.clone(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    TranscriptEventKind::NpcDialogue,
                    Some("Peig".to_string()),
                    Some("God bless ye kindly.".to_string())
                ),
                (
                    TranscriptEventKind::Narration,
                    None,
                    Some("A dog barks.".to_string())
                ),
            ]
        );
        assert_eq!(events[0].id.as_str(), "r:a:2");
        assert_eq!(events[1].id.as_str(), "r:a:1");
    }

    #[test]
    fn failed_streams_become_player_safe_errors_and_reactions_use_their_tokens() {
        let emissions = vec![
            (
                "text-log".to_string(),
                json!({"source":"Peig","content":"","stream_turn_id":1}),
            ),
            (
                "stream-turn-end".to_string(),
                json!({"turn_id":1,"status":"failed","recovery_message":"Please try again."}),
            ),
            (
                "text-log".to_string(),
                json!({"source":"Mick","content":"","stream_turn_id":2,"subtype":"action"}),
            ),
            (
                "stream-token".to_string(),
                json!({"token":"nods","turn_id":2,"source":"Mick"}),
            ),
            (
                "stream-turn-end".to_string(),
                json!({"turn_id":2,"status":"completed"}),
            ),
        ];
        let events = project_emissions(&emissions, &mut builder(), None);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, TranscriptEventKind::Error);
        assert_eq!(events[0].content.as_deref(), Some("Please try again."));
        assert_eq!(events[1].kind, TranscriptEventKind::ActionResult);
        assert_eq!(events[1].content.as_deref(), Some("nods"));
    }

    #[test]
    fn a_world_update_to_a_new_location_is_a_scene_change() {
        let emissions = vec![
            (
                "travel-start".to_string(),
                json!({"from":"Kilteevan","to":"Crossroads"}),
            ),
            (
                "world-update".to_string(),
                json!({"location_id":13,"location_name":"The Crossroads"}),
            ),
            (
                "world-update".to_string(),
                json!({"location_id":13,"location_name":"The Crossroads"}),
            ),
        ];
        let events = project_emissions(&emissions, &mut builder(), Some("Kilteevan Village"));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, TranscriptEventKind::SceneChanged);
        assert_eq!(events[0].content.as_deref(), Some("The Crossroads"));
        assert_eq!(
            events[0].metadata.get("sceneID").map(String::as_str),
            Some("13")
        );
    }

    /// Fitness check: every wire event the shared game loop emits is
    /// classified by this module.
    #[test]
    fn every_game_loop_emission_is_classified() {
        let sources = [
            include_str!("../game_loop/input.rs"),
            include_str!("../game_loop/movement.rs"),
            include_str!("../game_loop/npc_turn.rs"),
            include_str!("../game_loop/reactions.rs"),
            include_str!("../game_loop/staged_turn.rs"),
            include_str!("../game_session.rs"),
        ];
        let pattern = regex::Regex::new(r#"emit_event\(\s*"([a-z-]+)""#).unwrap();
        let mut seen = Vec::new();
        for source in sources {
            for capture in pattern.captures_iter(source) {
                seen.push(capture[1].to_string());
            }
        }
        assert!(
            seen.contains(&"text-log".to_string()),
            "scan found emissions"
        );
        for name in seen {
            assert!(
                TRANSCRIPT_EMISSIONS.contains(&name.as_str())
                    || PRESENTATION_EMISSIONS.contains(&name.as_str()),
                "game-loop emission {name:?} is not classified in turn::projection"
            );
        }
    }
}
