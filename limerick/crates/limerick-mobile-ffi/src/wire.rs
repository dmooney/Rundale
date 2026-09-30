//! JSON projections of engine values onto the Swift presentation contract.
//!
//! `mobile/RundaleKit` decodes these shapes (`SemanticEvent`,
//! `RequestRecord`, `ClarificationPrompt`, ...). They are presentation only:
//! the engine's own types stay the source of truth, and nothing here is read
//! back into game state.

use std::collections::HashMap;

use limerick_core::turn::{
    ClarificationPrompt, ExecutionAttemptId, FALLBACK_LINE, LogicalRequestId, RequestPhase,
    RequestRecord, TerminalOutcome, TranscriptEvent, TranscriptEventKind,
};
use serde_json::{Map, Value, json};

/// Presentation contract version the app accepts (`PresentationContractVersion`).
pub const CONTRACT_VERSION: (u16, u16) = (1, 0);

pub fn contract_version() -> Value {
    json!({"major": CONTRACT_VERSION.0, "minor": CONTRACT_VERSION.1})
}

/// A sequence, revision, or cursor: Swift decodes them as `{rawValue: n}`.
pub fn raw(value: u64) -> Value {
    json!({ "rawValue": value })
}

pub fn outcome(outcome: TerminalOutcome) -> &'static str {
    match outcome {
        TerminalOutcome::Succeeded => "succeeded",
        TerminalOutcome::Cancelled => "cancelled",
        TerminalOutcome::Failed => "failed",
        TerminalOutcome::Interrupted => "interrupted",
    }
}

fn phase(phase: RequestPhase) -> &'static str {
    match phase {
        RequestPhase::Accepted => "accepted",
        RequestPhase::Executing => "executing",
        RequestPhase::AwaitingClarification => "awaiting_clarification",
        RequestPhase::Completed => "completed",
        RequestPhase::Failed => "failed",
        RequestPhase::Cancelled => "cancelled",
        RequestPhase::Interrupted => "interrupted",
    }
}

pub fn clarification(prompt: &ClarificationPrompt) -> Value {
    json!({
        "question": prompt.question,
        "choices": prompt.choices.iter().map(|choice| json!({
            "id": choice.id,
            "label": choice.label,
            "entityID": choice.entity_id,
        })).collect::<Vec<_>>(),
    })
}

/// Where each request and attempt first appears in the transcript, for the
/// `startedAt` of the Swift request projection. Built from the events the
/// session has seen.
#[derive(Debug, Default)]
pub struct EventIndex {
    requests: HashMap<LogicalRequestId, u64>,
    attempts: HashMap<ExecutionAttemptId, u64>,
}

impl EventIndex {
    pub fn observe(&mut self, events: &[TranscriptEvent]) {
        for event in events {
            let sequence = event.sequence.0;
            if let Some(request) = &event.event.request_id {
                self.requests.entry(request.clone()).or_insert(sequence);
            }
            if let Some(attempt) = &event.event.attempt_id {
                self.attempts.entry(attempt.clone()).or_insert(sequence);
            }
        }
    }

    fn started_at(&self, record: &RequestRecord, attempt: &ExecutionAttemptId) -> u64 {
        let command = self.requests.get(&record.id).copied().unwrap_or_default();
        if record.attempts.first().map(|first| &first.id) == Some(attempt) {
            return command;
        }
        self.attempts.get(attempt).copied().unwrap_or(command)
    }
}

/// The Swift `RequestRecord` of an engine request.
pub fn request(record: &RequestRecord, index: &EventIndex) -> Value {
    let attempts: Vec<Value> = record
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "id": attempt.id.as_str(),
                "originalText": record.original_text,
                "phase": phase(attempt.phase),
                "terminalOutcome": attempt.terminal_outcome.map(outcome),
                "provisionalItemIDs": [],
                "startedAt": raw(index.started_at(record, &attempt.id)),
                "committedStateRevision": attempt.committed_revision.map(|r| raw(r.0)),
            })
        })
        .collect();
    json!({
        "id": record.id.as_str(),
        "originalText": record.original_text,
        "acceptedCommandItemID": record.command_item().as_str(),
        "attempts": attempts,
        "currentAttemptID": record.current_attempt().map(|attempt| attempt.id.as_str()),
        "phase": phase(record.phase),
        "terminalOutcome": record.terminal_outcome.map(outcome),
        "committedStateRevision": record.committed_revision.map(|r| raw(r.0)),
        "pendingClarification": record.pending_clarification.as_ref().map(clarification),
    })
}

/// The Swift `SemanticEvent` of a journaled transcript event.
///
/// The accepted command belongs to the request, not to one attempt, so the
/// engine journals it without an attempt; the projection names the
/// request's first attempt so the app's reducer can settle that attempt.
/// A kind this build does not know is shown as [`FALLBACK_LINE`].
pub fn event(session_id: &str, event: &TranscriptEvent, first_attempt: Option<&str>) -> Value {
    let pending = &event.event;
    let (kind, content) = match &pending.kind {
        TranscriptEventKind::Unknown(_) => ("narration", Some(FALLBACK_LINE.to_string())),
        known => (known.as_str(), pending.content.clone()),
    };
    let is_command = pending.kind == TranscriptEventKind::PlayerCommand;
    let attempt = pending
        .attempt_id
        .as_ref()
        .map(|id| id.as_str())
        .or(first_attempt.filter(|_| is_command));
    let mut body = Map::new();
    body.insert("contractVersion".into(), contract_version());
    body.insert("eventID".into(), json!(pending.id.as_str()));
    body.insert("sessionID".into(), json!(session_id));
    body.insert("sequence".into(), raw(event.sequence.0));
    body.insert("kind".into(), json!(kind));
    insert_some(&mut body, "content", content.map(Value::from));
    insert_some(
        &mut body,
        "speaker",
        pending.speaker.clone().map(Value::from),
    );
    insert_some(
        &mut body,
        "logicalRequestID",
        pending.request_id.as_ref().map(|id| json!(id.as_str())),
    );
    insert_some(&mut body, "attemptID", attempt.map(Value::from));
    insert_some(
        &mut body,
        "transcriptItemID",
        pending.item_id.as_ref().map(|id| json!(id.as_str())),
    );
    body.insert("provisional".into(), json!(false));
    body.insert("streamUpdate".into(), json!("replace"));
    insert_some(
        &mut body,
        "terminalOutcome",
        pending.terminal_outcome.map(|o| json!(outcome(o))),
    );
    body.insert("accepted".into(), json!(is_command));
    insert_some(
        &mut body,
        "sourceDraftID",
        pending.metadata.get("draftID").map(|id| json!(id)),
    );
    insert_some(
        &mut body,
        "stateRevision",
        pending.state_revision.map(|r| raw(r.0)),
    );
    insert_some(
        &mut body,
        "clarification",
        pending.clarification.as_ref().map(clarification),
    );
    body.insert("metadata".into(), json!(pending.metadata));
    Value::Object(body)
}

/// A provisional stream frame of a dialogue call: partial reply text shown
/// while it arrives and never journaled. It carries the latest durable
/// sequence (it does not advance the app's cursor) and orders itself by
/// `streamSequence`.
pub struct Provisional<'a> {
    pub session_id: &'a str,
    pub request_id: &'a str,
    pub attempt_id: &'a str,
    pub call_id: &'a str,
    pub speaker: Option<&'a str>,
    pub cursor: u64,
    pub stream_sequence: u64,
    pub text: &'a str,
}

impl Provisional<'_> {
    pub fn item_id(call_id: &str) -> String {
        format!("{call_id}:provisional")
    }

    pub fn to_json(&self) -> Value {
        json!({
            "contractVersion": contract_version(),
            "eventID": format!("{}:frame:{}", self.call_id, self.stream_sequence),
            "sessionID": self.session_id,
            "sequence": raw(self.cursor),
            "kind": "npc_dialogue",
            "content": self.text,
            "speaker": self.speaker,
            "logicalRequestID": self.request_id,
            "attemptID": self.attempt_id,
            "transcriptItemID": Self::item_id(self.call_id),
            "provisional": true,
            "streamSequence": self.stream_sequence,
            "streamUpdate": "append",
            "accepted": false,
            "metadata": {},
        })
    }
}

fn insert_some(body: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    if let Some(value) = value {
        body.insert(key.to_string(), value);
    }
}
