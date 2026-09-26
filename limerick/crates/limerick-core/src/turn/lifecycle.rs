//! The request lifecycle state machine.
//!
//! A [`RequestRecord`] is the durable account of one player submission. Its
//! transitions are pure: the turn engine calls them, journals the updated
//! record, and only then acts. See `docs/design/portable-turn-api.md` §5.1.
//!
//! ```text
//! Accepted ──begin──▶ Executing ──complete──▶ Completed (Succeeded)
//!     │                 │  ▲ │
//!     │                 │  │ └─finish_uncommitted─▶ Failed | Cancelled | Interrupted
//!     │                 │  │                            │
//!     │   await_clarification  answer_clarification     └──begin (retry)──▶ Executing
//!     │                 ▼  │
//!     │         AwaitingClarification ──cancel_clarification──▶ Cancelled
//!     └──recover──▶ Interrupted            (survives recover)
//! ```

use serde::{Deserialize, Serialize};

use super::ids::{
    ExecutionAttemptId, InferenceCallId, LogicalRequestId, StateRevision, TranscriptItemId,
};

/// Where a request or attempt is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestPhase {
    /// Durably accepted; no attempt has started.
    Accepted,
    /// An attempt is running (possibly waiting for an inference result).
    Executing,
    /// Waiting for the player to choose between ambiguous addressees.
    AwaitingClarification,
    /// Committed successfully. Terminal and not retryable.
    Completed,
    /// Ended without committing because inference or validation failed.
    Failed,
    /// Ended without committing because the player stopped it.
    Cancelled,
    /// Ended without committing because the process stopped mid-attempt.
    Interrupted,
}

impl RequestPhase {
    /// Whether the phase is final for the current attempt.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Interrupted
        )
    }

    /// Whether a request in this phase blocks new submissions.
    pub fn is_open(self) -> bool {
        !self.is_terminal()
    }
}

/// How an attempt ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutcome {
    /// Committed.
    Succeeded,
    /// Stopped by the player.
    Cancelled,
    /// Inference or validation failed.
    Failed,
    /// The process stopped while the attempt was open.
    Interrupted,
}

impl TerminalOutcome {
    fn phase(self) -> RequestPhase {
        match self {
            Self::Succeeded => RequestPhase::Completed,
            Self::Cancelled => RequestPhase::Cancelled,
            Self::Failed => RequestPhase::Failed,
            Self::Interrupted => RequestPhase::Interrupted,
        }
    }
}

/// One authored choice offered when an addressee is ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationChoice {
    /// Stable choice id the host sends back.
    pub id: String,
    /// Player-facing label.
    pub label: String,
    /// The entity the choice selects (an NPC id), when there is one.
    #[serde(default)]
    pub entity_id: Option<String>,
}

/// A finite question put to the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationPrompt {
    /// The question.
    pub question: String,
    /// The allowed answers.
    pub choices: Vec<ClarificationChoice>,
}

/// One execution of a logical request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestAttempt {
    /// Attempt identity.
    pub id: ExecutionAttemptId,
    /// Attempt phase.
    pub phase: RequestPhase,
    /// How the attempt ended, once terminal.
    #[serde(default)]
    pub terminal_outcome: Option<TerminalOutcome>,
    /// Authoritative revision the attempt started from.
    pub base_revision: StateRevision,
    /// Inference calls issued so far.
    #[serde(default)]
    pub calls_issued: u32,
    /// Revision produced by this attempt's commit.
    #[serde(default)]
    pub committed_revision: Option<StateRevision>,
}

/// The durable record of one player submission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestRecord {
    /// Logical identity, stable across retries.
    pub id: LogicalRequestId,
    /// The submitted text.
    pub original_text: String,
    /// Explicit addressees chosen with the submission (UI chips).
    #[serde(default)]
    pub addressed_to: Vec<String>,
    /// Host draft the submission came from, echoed on the command event.
    #[serde(default)]
    pub draft_id: Option<String>,
    /// Attempts in order; the last one is current.
    #[serde(default)]
    pub attempts: Vec<RequestAttempt>,
    /// Request phase (mirrors the current attempt once one exists).
    pub phase: RequestPhase,
    /// Outcome of the latest terminal attempt.
    #[serde(default)]
    pub terminal_outcome: Option<TerminalOutcome>,
    /// Revision committed by the request, once it succeeded.
    #[serde(default)]
    pub committed_revision: Option<StateRevision>,
    /// The open question, while awaiting clarification.
    #[serde(default)]
    pub pending_clarification: Option<ClarificationPrompt>,
    /// Addressee the player selected in answer to a clarification.
    #[serde(default)]
    pub selected_addressee: Option<String>,
}

/// A transition that the current state does not allow.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    /// The request already committed; it can never run again.
    #[error("request {0} already committed")]
    AlreadyCommitted(LogicalRequestId),
    /// The request is open (running or awaiting clarification).
    #[error("request {0} is still open")]
    StillOpen(LogicalRequestId),
    /// The operation needs a running attempt.
    #[error("request {0} has no running attempt")]
    NotExecuting(LogicalRequestId),
    /// The operation needs an open clarification.
    #[error("request {0} is not awaiting clarification")]
    NotAwaitingClarification(LogicalRequestId),
    /// The clarification choice is not one of the offered choices.
    #[error("unknown clarification choice {0:?}")]
    UnknownChoice(String),
    /// A terminal outcome that cannot end an uncommitted attempt.
    #[error("{0:?} cannot end an uncommitted attempt")]
    InvalidOutcome(TerminalOutcome),
}

/// Why a callback for an attempt was ignored. Ignoring has no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IgnoredReason {
    /// No request is open.
    NoOpenRequest,
    /// The callback names an attempt that is not the current one.
    StaleAttempt,
    /// The attempt already ended (stopped, failed, committed).
    AttemptTerminal,
    /// The callback names a call that is not the one awaited.
    StaleCall,
    /// Authoritative state moved since the attempt started.
    StaleRevision,
}

impl RequestRecord {
    /// Records a newly accepted submission.
    pub fn accept(
        id: LogicalRequestId,
        original_text: impl Into<String>,
        addressed_to: Vec<String>,
        draft_id: Option<String>,
    ) -> Self {
        Self {
            id,
            original_text: original_text.into(),
            addressed_to,
            draft_id,
            attempts: Vec::new(),
            phase: RequestPhase::Accepted,
            terminal_outcome: None,
            committed_revision: None,
            pending_clarification: None,
            selected_addressee: None,
        }
    }

    /// The transcript item of the accepted command.
    pub fn command_item(&self) -> TranscriptItemId {
        self.id.command_item()
    }

    /// The current (latest) attempt.
    pub fn current_attempt(&self) -> Option<&RequestAttempt> {
        self.attempts.last()
    }

    fn current_attempt_mut(&mut self) -> Option<&mut RequestAttempt> {
        self.attempts.last_mut()
    }

    /// Whether the request committed.
    pub fn has_committed(&self) -> bool {
        self.terminal_outcome == Some(TerminalOutcome::Succeeded)
    }

    /// Starts an attempt: the first one after acceptance, or a retry after an
    /// uncommitted terminal outcome.
    pub fn begin_attempt(
        &mut self,
        attempt: ExecutionAttemptId,
        base_revision: StateRevision,
    ) -> Result<(), LifecycleError> {
        if self.has_committed() {
            return Err(LifecycleError::AlreadyCommitted(self.id.clone()));
        }
        let first = self.phase == RequestPhase::Accepted && self.attempts.is_empty();
        if !first && !self.phase.is_terminal() {
            return Err(LifecycleError::StillOpen(self.id.clone()));
        }
        self.attempts.push(RequestAttempt {
            id: attempt,
            phase: RequestPhase::Executing,
            terminal_outcome: None,
            base_revision,
            calls_issued: 0,
            committed_revision: None,
        });
        self.phase = RequestPhase::Executing;
        self.terminal_outcome = None;
        self.pending_clarification = None;
        Ok(())
    }

    /// Allocates the id of the next inference call in the running attempt.
    pub fn next_call(&mut self) -> Result<InferenceCallId, LifecycleError> {
        let id = self.id.clone();
        let attempt = self
            .current_attempt_mut()
            .filter(|attempt| attempt.phase == RequestPhase::Executing)
            .ok_or(LifecycleError::NotExecuting(id))?;
        attempt.calls_issued += 1;
        Ok(attempt.id.call(attempt.calls_issued))
    }

    /// Decides whether a host callback still belongs to the running attempt.
    /// `current` is the authoritative revision now.
    pub fn check_callback(
        &self,
        attempt: &ExecutionAttemptId,
        call: Option<&InferenceCallId>,
        base_revision: StateRevision,
        current: StateRevision,
    ) -> Result<(), IgnoredReason> {
        let Some(running) = self.current_attempt() else {
            return Err(IgnoredReason::NoOpenRequest);
        };
        if &running.id != attempt {
            return Err(IgnoredReason::StaleAttempt);
        }
        if running.phase != RequestPhase::Executing || self.has_committed() {
            return Err(IgnoredReason::AttemptTerminal);
        }
        if let Some(call) = call
            && call != &running.id.call(running.calls_issued)
        {
            return Err(IgnoredReason::StaleCall);
        }
        if running.base_revision != base_revision || current != base_revision {
            return Err(IgnoredReason::StaleRevision);
        }
        Ok(())
    }

    /// Parks the running attempt on a question for the player.
    pub fn await_clarification(
        &mut self,
        prompt: ClarificationPrompt,
    ) -> Result<(), LifecycleError> {
        let id = self.id.clone();
        let attempt = self
            .current_attempt_mut()
            .filter(|attempt| attempt.phase == RequestPhase::Executing)
            .ok_or(LifecycleError::NotExecuting(id))?;
        attempt.phase = RequestPhase::AwaitingClarification;
        self.phase = RequestPhase::AwaitingClarification;
        self.pending_clarification = Some(prompt);
        Ok(())
    }

    /// Resumes the same attempt with the player's choice. The attempt's base
    /// revision moves to `current`, because it re-runs against current state.
    pub fn answer_clarification(
        &mut self,
        choice_id: &str,
        current: StateRevision,
    ) -> Result<ClarificationChoice, LifecycleError> {
        if self.phase != RequestPhase::AwaitingClarification {
            return Err(LifecycleError::NotAwaitingClarification(self.id.clone()));
        }
        let choice = self
            .pending_clarification
            .as_ref()
            .and_then(|prompt| prompt.choices.iter().find(|choice| choice.id == choice_id))
            .cloned()
            .ok_or_else(|| LifecycleError::UnknownChoice(choice_id.to_string()))?;
        let attempt = self
            .current_attempt_mut()
            .expect("an awaiting request has an attempt");
        attempt.phase = RequestPhase::Executing;
        attempt.base_revision = current;
        attempt.calls_issued = 0;
        self.phase = RequestPhase::Executing;
        self.pending_clarification = None;
        self.selected_addressee = choice.entity_id.clone().or(Some(choice.label.clone()));
        Ok(choice)
    }

    /// Ends the pending clarification without running (new input superseded
    /// it, or the player dismissed it).
    pub fn cancel_clarification(&mut self) -> Result<(), LifecycleError> {
        if self.phase != RequestPhase::AwaitingClarification {
            return Err(LifecycleError::NotAwaitingClarification(self.id.clone()));
        }
        self.pending_clarification = None;
        self.end(TerminalOutcome::Cancelled);
        Ok(())
    }

    /// Commits the running attempt.
    pub fn complete(&mut self, revision: StateRevision) -> Result<(), LifecycleError> {
        let id = self.id.clone();
        let attempt = self
            .current_attempt_mut()
            .filter(|attempt| attempt.phase == RequestPhase::Executing)
            .ok_or(LifecycleError::NotExecuting(id))?;
        attempt.committed_revision = Some(revision);
        self.committed_revision = Some(revision);
        self.end(TerminalOutcome::Succeeded);
        Ok(())
    }

    /// Ends the open attempt without committing.
    pub fn finish_uncommitted(&mut self, outcome: TerminalOutcome) -> Result<(), LifecycleError> {
        if outcome == TerminalOutcome::Succeeded {
            return Err(LifecycleError::InvalidOutcome(outcome));
        }
        if self.has_committed() {
            return Err(LifecycleError::AlreadyCommitted(self.id.clone()));
        }
        if self.phase.is_terminal() {
            return Err(LifecycleError::NotExecuting(self.id.clone()));
        }
        self.pending_clarification = None;
        self.end(outcome);
        Ok(())
    }

    /// Applies restart recovery: an accepted or running request becomes
    /// `Interrupted` and is never re-run automatically; a pending
    /// clarification survives. Returns whether the record changed.
    pub fn recover(&mut self) -> bool {
        match self.phase {
            RequestPhase::Accepted | RequestPhase::Executing => {
                self.end(TerminalOutcome::Interrupted);
                true
            }
            _ => false,
        }
    }

    fn end(&mut self, outcome: TerminalOutcome) {
        let phase = outcome.phase();
        if let Some(attempt) = self.current_attempt_mut() {
            attempt.phase = phase;
            attempt.terminal_outcome = Some(outcome);
        }
        self.phase = phase;
        self.terminal_outcome = Some(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepted() -> RequestRecord {
        RequestRecord::accept(
            LogicalRequestId::new("r1"),
            "ask Peig about the wall",
            Vec::new(),
            None,
        )
    }

    fn running() -> (RequestRecord, ExecutionAttemptId) {
        let mut record = accepted();
        let attempt = ExecutionAttemptId::new("a1");
        record
            .begin_attempt(attempt.clone(), StateRevision(0))
            .unwrap();
        (record, attempt)
    }

    fn prompt() -> ClarificationPrompt {
        ClarificationPrompt {
            question: "Which person do you mean?".to_string(),
            choices: vec![
                ClarificationChoice {
                    id: "choose-1".to_string(),
                    label: "Mícheál Connolly".to_string(),
                    entity_id: Some("1".to_string()),
                },
                ClarificationChoice {
                    id: "choose-2".to_string(),
                    label: "Róisín Connolly".to_string(),
                    entity_id: Some("2".to_string()),
                },
            ],
        }
    }

    // Oracle: ios-port `successful_candidate_commits_one_exchange_and_retry_is_rejected`.
    #[test]
    fn a_committed_request_cannot_retry_and_late_callbacks_are_ignored() {
        let (mut record, attempt) = running();
        let call = record.next_call().unwrap();
        record.complete(StateRevision(1)).unwrap();
        assert_eq!(record.phase, RequestPhase::Completed);
        assert_eq!(record.committed_revision, Some(StateRevision(1)));
        assert_eq!(
            record.begin_attempt(ExecutionAttemptId::new("a2"), StateRevision(1)),
            Err(LifecycleError::AlreadyCommitted(record.id.clone()))
        );
        assert_eq!(
            record.check_callback(&attempt, Some(&call), StateRevision(0), StateRevision(1)),
            Err(IgnoredReason::AttemptTerminal)
        );
    }

    // Oracle: ios-port `stop_wins_and_late_candidate_cannot_commit`.
    #[test]
    fn stop_is_terminal_and_a_late_callback_is_ignored() {
        let (mut record, attempt) = running();
        let call = record.next_call().unwrap();
        record
            .finish_uncommitted(TerminalOutcome::Cancelled)
            .unwrap();
        assert_eq!(record.phase, RequestPhase::Cancelled);
        assert_eq!(
            record.check_callback(&attempt, Some(&call), StateRevision(0), StateRevision(0)),
            Err(IgnoredReason::AttemptTerminal)
        );
        assert!(record.complete(StateRevision(1)).is_err());
        assert_eq!(record.committed_revision, None);
    }

    // Oracle: ios-port `failed_attempt_can_retry_with_new_attempt_identity`.
    #[test]
    fn a_failed_attempt_retries_with_a_new_attempt_and_old_callbacks_go_stale() {
        let (mut record, first) = running();
        let first_call = record.next_call().unwrap();
        record.finish_uncommitted(TerminalOutcome::Failed).unwrap();
        let retry = ExecutionAttemptId::new("a2");
        record
            .begin_attempt(retry.clone(), StateRevision(0))
            .unwrap();
        assert_eq!(record.attempts.len(), 2);
        assert_eq!(record.phase, RequestPhase::Executing);
        assert_eq!(record.terminal_outcome, None);
        assert_eq!(
            record.check_callback(
                &first,
                Some(&first_call),
                StateRevision(0),
                StateRevision(0)
            ),
            Err(IgnoredReason::StaleAttempt)
        );
        let retry_call = record.next_call().unwrap();
        assert_eq!(retry_call.as_str(), "a2#1");
        assert_eq!(
            record.check_callback(
                &retry,
                Some(&retry_call),
                StateRevision(0),
                StateRevision(0)
            ),
            Ok(())
        );
    }

    // Oracle: ios-port `endpoint_failure_requires_the_invocation_base_revision`.
    #[test]
    fn callbacks_must_match_the_awaited_call_and_revision() {
        let (mut record, attempt) = running();
        let first = record.next_call().unwrap();
        let second = record.next_call().unwrap();
        assert_eq!(
            record.check_callback(&attempt, Some(&first), StateRevision(0), StateRevision(0)),
            Err(IgnoredReason::StaleCall)
        );
        assert_eq!(
            record.check_callback(&attempt, Some(&second), StateRevision(3), StateRevision(0)),
            Err(IgnoredReason::StaleRevision)
        );
        assert_eq!(
            record.check_callback(&attempt, Some(&second), StateRevision(0), StateRevision(1)),
            Err(IgnoredReason::StaleRevision)
        );
        assert_eq!(
            record.check_callback(&attempt, Some(&second), StateRevision(0), StateRevision(0)),
            Ok(())
        );
    }

    // Oracle: ios-port `accepted_restart_becomes_interrupted_and_does_not_rerun`.
    #[test]
    fn recovery_interrupts_open_work_but_keeps_a_pending_question() {
        let mut accepted_only = accepted();
        assert!(accepted_only.recover());
        assert_eq!(
            accepted_only.terminal_outcome,
            Some(TerminalOutcome::Interrupted)
        );

        let (mut executing, _) = running();
        assert!(executing.recover());
        assert_eq!(executing.phase, RequestPhase::Interrupted);
        assert_eq!(
            executing.current_attempt().unwrap().terminal_outcome,
            Some(TerminalOutcome::Interrupted)
        );

        let (mut asking, _) = running();
        asking.await_clarification(prompt()).unwrap();
        assert!(!asking.recover());
        assert_eq!(asking.phase, RequestPhase::AwaitingClarification);

        let mut done = running().0;
        done.complete(StateRevision(1)).unwrap();
        assert!(!done.recover());
    }

    // Oracle: ios-port `phase3_ambiguity_survives_resume_and_selection_continues_original_request`.
    #[test]
    fn a_clarification_answer_continues_the_same_attempt() {
        let (mut record, attempt) = running();
        record.await_clarification(prompt()).unwrap();
        let restored: RequestRecord =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        let mut record = restored;
        assert_eq!(
            record.answer_clarification("choose-9", StateRevision(0)),
            Err(LifecycleError::UnknownChoice("choose-9".to_string()))
        );
        let choice = record
            .answer_clarification("choose-2", StateRevision(2))
            .unwrap();
        assert_eq!(choice.label, "Róisín Connolly");
        assert_eq!(record.phase, RequestPhase::Executing);
        assert_eq!(record.attempts.len(), 1);
        assert_eq!(record.current_attempt().unwrap().id, attempt);
        assert_eq!(
            record.current_attempt().unwrap().base_revision,
            StateRevision(2)
        );
        assert_eq!(record.selected_addressee.as_deref(), Some("2"));
        assert_eq!(record.original_text, "ask Peig about the wall");
    }

    #[test]
    fn a_superseded_clarification_is_cancelled_and_retryable() {
        let (mut record, _) = running();
        record.await_clarification(prompt()).unwrap();
        assert_eq!(
            record.begin_attempt(ExecutionAttemptId::new("a2"), StateRevision(0)),
            Err(LifecycleError::StillOpen(record.id.clone()))
        );
        record.cancel_clarification().unwrap();
        assert_eq!(record.phase, RequestPhase::Cancelled);
        assert!(record.pending_clarification.is_none());
        record
            .begin_attempt(ExecutionAttemptId::new("a2"), StateRevision(0))
            .unwrap();
    }

    #[test]
    fn succeeded_cannot_end_an_uncommitted_attempt() {
        let (mut record, _) = running();
        assert_eq!(
            record.finish_uncommitted(TerminalOutcome::Succeeded),
            Err(LifecycleError::InvalidOutcome(TerminalOutcome::Succeeded))
        );
    }
}
