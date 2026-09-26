//! Durable transcript events.
//!
//! A transcript event is what a player-facing client renders: the accepted
//! command, narration, NPC dialogue, action results, scene changes,
//! clarification, progress, errors, and the terminal response marker. Every
//! event carries the request and attempt it belongs to, so a client can drop
//! events from an obsolete attempt, and a deterministic id, so redelivery is
//! idempotent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::ids::{
    EventSequence, ExecutionAttemptId, LogicalRequestId, StateRevision, TranscriptEventId,
    TranscriptItemId,
};
use super::lifecycle::{ClarificationPrompt, TerminalOutcome};

/// The kind of a transcript event.
///
/// Unknown kinds (written by a newer build) are preserved verbatim as
/// [`TranscriptEventKind::Unknown`] so an older build never rejects a save
/// because of them; clients show a neutral fallback line (ADR-025 §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TranscriptEventKind {
    /// The accepted player command.
    PlayerCommand,
    /// System narration (look, travel, idle lines).
    Narration,
    /// A committed NPC line.
    NpcDialogue,
    /// A narrated action (player or NPC).
    ActionResult,
    /// The player arrived somewhere.
    SceneChanged,
    /// The engine asked which person the player meant.
    ClarificationRequired,
    /// The player answered a clarification.
    ClarificationSelected,
    /// Non-committal progress (for example, a retry starting).
    Progress,
    /// A player-safe error line.
    Error,
    /// The terminal marker of an attempt.
    ResponseCompleted,
    /// A kind this build does not know, kept verbatim.
    Unknown(String),
}

impl TranscriptEventKind {
    /// The stable wire name.
    pub fn as_str(&self) -> &str {
        match self {
            Self::PlayerCommand => "player_command",
            Self::Narration => "narration",
            Self::NpcDialogue => "npc_dialogue",
            Self::ActionResult => "action_result",
            Self::SceneChanged => "scene_changed",
            Self::ClarificationRequired => "clarification_required",
            Self::ClarificationSelected => "clarification_selected",
            Self::Progress => "progress",
            Self::Error => "error",
            Self::ResponseCompleted => "response_completed",
            Self::Unknown(raw) => raw,
        }
    }

    /// Parses a wire name; unknown names are preserved.
    pub fn parse(raw: &str) -> Self {
        match raw {
            "player_command" => Self::PlayerCommand,
            "narration" => Self::Narration,
            "npc_dialogue" => Self::NpcDialogue,
            "action_result" => Self::ActionResult,
            "scene_changed" => Self::SceneChanged,
            "clarification_required" => Self::ClarificationRequired,
            "clarification_selected" => Self::ClarificationSelected,
            "progress" => Self::Progress,
            "error" => Self::Error,
            "response_completed" => Self::ResponseCompleted,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl Serialize for TranscriptEventKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TranscriptEventKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::parse(&raw))
    }
}

/// An event the engine produced, before the journal assigns its sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingEvent {
    /// Deterministic identity.
    pub id: TranscriptEventId,
    /// Owning request, when the event belongs to one.
    #[serde(default)]
    pub request_id: Option<LogicalRequestId>,
    /// Owning attempt, when the event belongs to one.
    #[serde(default)]
    pub attempt_id: Option<ExecutionAttemptId>,
    /// Transcript row the event creates or updates.
    #[serde(default)]
    pub item_id: Option<TranscriptItemId>,
    /// Kind.
    pub kind: TranscriptEventKind,
    /// Speaker display name, for dialogue and NPC actions.
    #[serde(default)]
    pub speaker: Option<String>,
    /// Text content.
    #[serde(default)]
    pub content: Option<String>,
    /// Outcome, on a `ResponseCompleted` event.
    #[serde(default)]
    pub terminal_outcome: Option<TerminalOutcome>,
    /// Committed revision; present only on a succeeded `ResponseCompleted`.
    #[serde(default)]
    pub state_revision: Option<StateRevision>,
    /// The question, on a `ClarificationRequired` event.
    #[serde(default)]
    pub clarification: Option<ClarificationPrompt>,
    /// Small string metadata (for example `retry`, `errorKind`).
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// A journaled event with its session sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptEvent {
    /// Session-monotonic position.
    pub sequence: EventSequence,
    /// The event.
    #[serde(flatten)]
    pub event: PendingEvent,
}

/// Builds the events of one request, numbering them deterministically.
#[derive(Debug, Clone)]
pub struct EventBuilder {
    request: LogicalRequestId,
    attempt: Option<ExecutionAttemptId>,
    next_ordinal: u32,
}

impl EventBuilder {
    /// Numbers events for `request` (and `attempt`, when set) from `ordinal`.
    pub fn new(
        request: LogicalRequestId,
        attempt: Option<ExecutionAttemptId>,
        ordinal: u32,
    ) -> Self {
        Self {
            request,
            attempt,
            next_ordinal: ordinal,
        }
    }

    /// The next ordinal this builder will use.
    pub fn next_ordinal(&self) -> u32 {
        self.next_ordinal
    }

    /// A new event of `kind` with the next deterministic id.
    pub fn event(&mut self, kind: TranscriptEventKind) -> PendingEvent {
        let id = TranscriptEventId::derive(&self.request, self.attempt.as_ref(), self.next_ordinal);
        self.next_ordinal += 1;
        PendingEvent {
            id,
            request_id: Some(self.request.clone()),
            attempt_id: self.attempt.clone(),
            item_id: None,
            kind,
            speaker: None,
            content: None,
            terminal_outcome: None,
            state_revision: None,
            clarification: None,
            metadata: BTreeMap::new(),
        }
    }

    /// The terminal marker for the builder's attempt.
    pub fn response_completed(
        &mut self,
        outcome: TerminalOutcome,
        revision: Option<StateRevision>,
    ) -> PendingEvent {
        let mut event = self.event(TranscriptEventKind::ResponseCompleted);
        event.terminal_outcome = Some(outcome);
        event.state_revision = revision.filter(|_| outcome == TerminalOutcome::Succeeded);
        event
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_kinds_round_trip_verbatim() {
        let json = r#"{"sequence":7,"id":"r:-:0","kind":"harvest_festival","content":"Bonfires"}"#;
        let event: TranscriptEvent = serde_json::from_str(json).unwrap();
        assert_eq!(
            event.event.kind,
            TranscriptEventKind::Unknown("harvest_festival".to_string())
        );
        let back = serde_json::to_value(&event).unwrap();
        assert_eq!(back["kind"], "harvest_festival");
        assert_eq!(back["sequence"], 7);
    }

    #[test]
    fn every_known_kind_round_trips() {
        for kind in [
            TranscriptEventKind::PlayerCommand,
            TranscriptEventKind::Narration,
            TranscriptEventKind::NpcDialogue,
            TranscriptEventKind::ActionResult,
            TranscriptEventKind::SceneChanged,
            TranscriptEventKind::ClarificationRequired,
            TranscriptEventKind::ClarificationSelected,
            TranscriptEventKind::Progress,
            TranscriptEventKind::Error,
            TranscriptEventKind::ResponseCompleted,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(
                serde_json::from_str::<TranscriptEventKind>(&json).unwrap(),
                kind
            );
        }
    }

    // RundaleKit `testFailedCompletionCannotAdvanceCommittedStateRevision`.
    #[test]
    fn only_a_succeeded_terminal_carries_a_committed_revision() {
        let mut builder = EventBuilder::new(
            LogicalRequestId::new("r"),
            Some(ExecutionAttemptId::new("a")),
            0,
        );
        let failed = builder.response_completed(TerminalOutcome::Failed, Some(StateRevision(9)));
        assert_eq!(failed.state_revision, None);
        let ok = builder.response_completed(TerminalOutcome::Succeeded, Some(StateRevision(9)));
        assert_eq!(ok.state_revision, Some(StateRevision(9)));
        assert_eq!(failed.id.as_str(), "r:a:0");
        assert_eq!(ok.id.as_str(), "r:a:1");
    }
}
