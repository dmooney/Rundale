//! The durability seam of the request lifecycle.
//!
//! The turn engine journals every lifecycle step before acting on it:
//! acceptance before interpretation or inference, the attempt start, a
//! pending clarification, the terminal outcome, and — for a committed turn —
//! the request terminal, its transcript events, and its durable state in one
//! atomic write.
//!
//! Contract, for every implementation:
//! - each call is atomic: on error nothing it carried was written;
//! - an event id already journaled with an identical payload is a no-op that
//!   returns the stored event (idempotent redelivery); with a different
//!   payload the whole call fails;
//! - sequences are assigned at append and strictly increase.
//!
//! [`MemoryTurnJournal`] implements the contract in memory. The SQLite
//! implementation (request and transcript tables in the existing save
//! database, one transaction per commit) is convergence Stage 3; see
//! `docs/design/portable-turn-api.md` §6.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;

use limerick_types::PlayerTask;

use super::BoxFuture;
use super::ids::{EventSequence, LogicalRequestId, TranscriptEventId};
use super::lifecycle::RequestRecord;
use super::transcript::{PendingEvent, TranscriptEvent};

/// A committed turn: the terminal request record, its transcript events,
/// and the durable state the turn changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnCommit {
    /// The request, already transitioned to `Completed`.
    pub record: RequestRecord,
    /// Transcript events produced by the attempt.
    pub events: Vec<PendingEvent>,
    /// Player-task post-states the turn produced (today's durable per-turn
    /// state; Stage 3 adds the authoritative state delta).
    pub task_mutations: Vec<PlayerTask>,
}

/// A journal write failed; nothing was written.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// An event id was reused with different content.
    #[error("transcript event {0} already exists with different content")]
    ConflictingEvent(TranscriptEventId),
    /// `accept` for a request id that is already journaled.
    #[error("request {0} is already journaled")]
    DuplicateRequest(LogicalRequestId),
    /// An update for a request that was never accepted.
    #[error("request {0} was never accepted")]
    UnknownRequest(LogicalRequestId),
    /// The storage layer failed.
    #[error("journal storage failed: {0}")]
    Storage(String),
}

/// Durable storage for request records and transcript events.
pub trait TurnJournal: Send + Sync {
    /// Journals a newly accepted request and its command event.
    fn accept(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>>;

    /// Journals a lifecycle step of an accepted request that commits no
    /// gameplay: attempt start, pending clarification, or an uncommitted
    /// terminal outcome.
    fn update(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>>;

    /// Journals a committed turn atomically.
    fn commit(
        &self,
        commit: TurnCommit,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>>;

    /// Requests that are not terminal (for restart recovery).
    fn open_requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>>;
}

#[derive(Debug, Default)]
struct MemoryState {
    requests: BTreeMap<LogicalRequestId, RequestRecord>,
    events: Vec<TranscriptEvent>,
    event_index: HashMap<TranscriptEventId, usize>,
    task_batches: Vec<Vec<PlayerTask>>,
    fail_next_writes: usize,
}

/// The journal contract in memory, with fault injection for tests.
#[derive(Debug, Default)]
pub struct MemoryTurnJournal {
    state: Mutex<MemoryState>,
}

enum WriteKind {
    Accept,
    Update,
    Commit(Vec<PlayerTask>),
}

impl MemoryTurnJournal {
    /// An empty journal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next `count` writes fail with [`JournalError::Storage`]
    /// without writing anything.
    pub fn fail_next_writes(&self, count: usize) {
        self.state.lock().expect("journal lock").fail_next_writes = count;
    }

    /// Every journaled event, in sequence order.
    pub fn events(&self) -> Vec<TranscriptEvent> {
        self.state.lock().expect("journal lock").events.clone()
    }

    /// The journaled record of `id`.
    pub fn request(&self, id: &LogicalRequestId) -> Option<RequestRecord> {
        self.state
            .lock()
            .expect("journal lock")
            .requests
            .get(id)
            .cloned()
    }

    /// Task batches committed so far, one per committed turn.
    pub fn task_batches(&self) -> Vec<Vec<PlayerTask>> {
        self.state
            .lock()
            .expect("journal lock")
            .task_batches
            .clone()
    }

    fn write(
        &self,
        kind: WriteKind,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> Result<Vec<TranscriptEvent>, JournalError> {
        let mut state = self.state.lock().expect("journal lock");
        if state.fail_next_writes > 0 {
            state.fail_next_writes -= 1;
            return Err(JournalError::Storage("injected write failure".to_string()));
        }
        match kind {
            WriteKind::Accept if state.requests.contains_key(&record.id) => {
                return Err(JournalError::DuplicateRequest(record.id));
            }
            WriteKind::Update | WriteKind::Commit(_)
                if !state.requests.contains_key(&record.id) =>
            {
                return Err(JournalError::UnknownRequest(record.id));
            }
            _ => {}
        }
        // Validate every event before writing anything.
        for event in &events {
            if let Some(&index) = state.event_index.get(&event.id)
                && state.events[index].event != *event
            {
                return Err(JournalError::ConflictingEvent(event.id.clone()));
            }
        }
        let mut next = state.events.last().map_or(1, |last| last.sequence.0 + 1);
        let mut stored = Vec::with_capacity(events.len());
        for event in events {
            if let Some(&index) = state.event_index.get(&event.id) {
                stored.push(state.events[index].clone());
                continue;
            }
            let journaled = TranscriptEvent {
                sequence: EventSequence(next),
                event,
            };
            next += 1;
            let index = state.events.len();
            state.event_index.insert(journaled.event.id.clone(), index);
            state.events.push(journaled.clone());
            stored.push(journaled);
        }
        if let WriteKind::Commit(tasks) = kind {
            state.task_batches.push(tasks);
        }
        state.requests.insert(record.id.clone(), record);
        Ok(stored)
    }
}

impl TurnJournal for MemoryTurnJournal {
    fn accept(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        let result = self.write(WriteKind::Accept, record, events);
        Box::pin(async move { result })
    }

    fn update(
        &self,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        let result = self.write(WriteKind::Update, record, events);
        Box::pin(async move { result })
    }

    fn commit(
        &self,
        commit: TurnCommit,
    ) -> BoxFuture<'_, Result<Vec<TranscriptEvent>, JournalError>> {
        let result = self.write(
            WriteKind::Commit(commit.task_mutations),
            commit.record,
            commit.events,
        );
        Box::pin(async move { result })
    }

    fn open_requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        let open = self
            .state
            .lock()
            .expect("journal lock")
            .requests
            .values()
            .filter(|record| record.phase.is_open())
            .cloned()
            .collect();
        Box::pin(async move { Ok(open) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::ids::{ExecutionAttemptId, StateRevision};
    use crate::turn::lifecycle::{RequestPhase, TerminalOutcome};
    use crate::turn::transcript::{EventBuilder, TranscriptEventKind};

    fn accepted(id: &str) -> (RequestRecord, PendingEvent) {
        let record = RequestRecord::accept(LogicalRequestId::new(id), "hello", Vec::new(), None);
        let mut builder = EventBuilder::new(record.id.clone(), None, 0);
        let mut command = builder.event(TranscriptEventKind::PlayerCommand);
        command.content = Some("hello".to_string());
        command.item_id = Some(record.command_item());
        (record, command)
    }

    #[tokio::test]
    async fn sequences_strictly_increase_across_requests() {
        let journal = MemoryTurnJournal::new();
        let (first, first_command) = accepted("r1");
        let (second, second_command) = accepted("r2");
        let a = journal.accept(first, vec![first_command]).await.unwrap();
        let b = journal.accept(second, vec![second_command]).await.unwrap();
        assert!(a[0].sequence < b[0].sequence);
    }

    // Oracle: ios-port persistence `duplicate_events_are_idempotent_but_conflicting_payload_is_rejected`.
    #[tokio::test]
    async fn duplicate_events_are_idempotent_but_conflicts_reject_the_whole_write() {
        let journal = MemoryTurnJournal::new();
        let (mut record, command) = accepted("r1");
        let stored = journal
            .accept(record.clone(), vec![command.clone()])
            .await
            .unwrap();

        let again = journal
            .update(record.clone(), vec![command.clone()])
            .await
            .unwrap();
        assert_eq!(again, stored, "redelivery returns the stored event");
        assert_eq!(journal.events().len(), 1);

        let mut builder =
            EventBuilder::new(record.id.clone(), Some(ExecutionAttemptId::new("a1")), 0);
        let progress = builder.event(TranscriptEventKind::Progress);
        let mut conflicting = command.clone();
        conflicting.content = Some("goodbye".to_string());
        record
            .begin_attempt(ExecutionAttemptId::new("a1"), StateRevision(0))
            .unwrap();
        let error = journal
            .update(record.clone(), vec![progress, conflicting])
            .await
            .unwrap_err();
        assert_eq!(error, JournalError::ConflictingEvent(command.id.clone()));
        assert_eq!(journal.events().len(), 1, "no partial write");
        assert_eq!(
            journal.request(&record.id).unwrap().phase,
            RequestPhase::Accepted,
            "the record update is rolled back too"
        );
    }

    // Oracle: ios-port persistence `sqlite_failure_rolls_back_generation_state_request_and_event_together`.
    #[tokio::test]
    async fn a_failed_commit_writes_nothing_and_an_unchanged_retry_commits_once() {
        let journal = MemoryTurnJournal::new();
        let (mut record, command) = accepted("r1");
        journal.accept(record.clone(), vec![command]).await.unwrap();
        let attempt = ExecutionAttemptId::new("a1");
        record
            .begin_attempt(attempt.clone(), StateRevision(0))
            .unwrap();
        journal.update(record.clone(), Vec::new()).await.unwrap();
        record.complete(StateRevision(1)).unwrap();
        let mut builder = EventBuilder::new(record.id.clone(), Some(attempt), 0);
        let mut line = builder.event(TranscriptEventKind::NpcDialogue);
        line.content = Some("God bless ye.".to_string());
        let done = builder.response_completed(TerminalOutcome::Succeeded, Some(StateRevision(1)));
        let commit = TurnCommit {
            record: record.clone(),
            events: vec![line, done],
            task_mutations: Vec::new(),
        };

        journal.fail_next_writes(1);
        assert!(journal.commit(commit.clone()).await.is_err());
        assert_eq!(journal.events().len(), 1);
        assert_eq!(journal.task_batches().len(), 0);
        assert_eq!(
            journal.request(&record.id).unwrap().phase,
            RequestPhase::Executing
        );

        let stored = journal.commit(commit.clone()).await.unwrap();
        assert_eq!(stored.len(), 2);
        let replayed = journal.commit(commit).await.unwrap();
        assert_eq!(replayed, stored, "a replayed commit appends nothing new");
        assert_eq!(journal.events().len(), 3);
        assert!(journal.request(&record.id).unwrap().has_committed());
    }

    #[tokio::test]
    async fn accept_rejects_a_known_request_and_update_rejects_an_unknown_one() {
        let journal = MemoryTurnJournal::new();
        let (record, command) = accepted("r1");
        journal
            .accept(record.clone(), vec![command.clone()])
            .await
            .unwrap();
        assert_eq!(
            journal
                .accept(record.clone(), vec![command])
                .await
                .unwrap_err(),
            JournalError::DuplicateRequest(record.id.clone())
        );
        let (stranger, _) = accepted("r2");
        assert_eq!(
            journal
                .update(stranger.clone(), Vec::new())
                .await
                .unwrap_err(),
            JournalError::UnknownRequest(stranger.id)
        );
    }

    #[tokio::test]
    async fn open_requests_lists_only_non_terminal_records() {
        let journal = MemoryTurnJournal::new();
        let (mut open, open_command) = accepted("open");
        let (mut done, done_command) = accepted("done");
        journal
            .accept(open.clone(), vec![open_command])
            .await
            .unwrap();
        journal
            .accept(done.clone(), vec![done_command])
            .await
            .unwrap();
        open.begin_attempt(ExecutionAttemptId::new("a"), StateRevision(0))
            .unwrap();
        journal.update(open.clone(), Vec::new()).await.unwrap();
        done.begin_attempt(ExecutionAttemptId::new("b"), StateRevision(0))
            .unwrap();
        done.finish_uncommitted(TerminalOutcome::Failed).unwrap();
        journal.update(done, Vec::new()).await.unwrap();
        let listed = journal.open_requests().await.unwrap();
        assert_eq!(listed, vec![open]);
    }
}
