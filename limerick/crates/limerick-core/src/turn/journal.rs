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
//! [`MemoryTurnJournal`] implements the contract in memory;
//! [`super::sqlite_journal::SqliteTurnJournal`] implements it in the save
//! database, with one SQLite transaction per call. See
//! `docs/design/portable-turn-api.md` §6.

use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::sync::Mutex;

use limerick_types::PlayerTask;

use super::BoxFuture;
use super::ids::{EventSequence, LogicalRequestId, TranscriptEventId};
use super::lifecycle::RequestRecord;
use super::transcript::{PendingEvent, TranscriptEvent};
use crate::persistence::GameSnapshot;

/// A committed turn: the terminal request record, its transcript events,
/// and the durable state the turn changed.
#[derive(Debug, Clone, PartialEq)]
pub struct TurnCommit {
    /// The request, already transitioned to `Completed`.
    pub record: RequestRecord,
    /// Transcript events produced by the attempt.
    pub events: Vec<PendingEvent>,
    /// Player-task post-states the turn produced.
    pub task_mutations: Vec<PlayerTask>,
    /// The authoritative state after the turn: a snapshot of the world and
    /// NPCs the turn installs. `None` when the turn completes at the current
    /// revision without changing state (a clarification answered for
    /// someone who has left).
    pub state: Option<GameSnapshot>,
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

    /// Every journaled request, in the order first accepted (for restoring
    /// an engine from the journal).
    fn requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>>;
}

#[derive(Debug, Default)]
struct MemoryState {
    requests: BTreeMap<LogicalRequestId, RequestRecord>,
    accepted_order: HashMap<LogicalRequestId, usize>,
    events: Vec<TranscriptEvent>,
    event_index: HashMap<TranscriptEventId, usize>,
    task_batches: Vec<Vec<PlayerTask>>,
    committed_states: usize,
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
    Commit { tasks: Vec<PlayerTask>, state: bool },
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

    /// How many committed turns carried an authoritative state.
    pub fn committed_states(&self) -> usize {
        self.state.lock().expect("journal lock").committed_states
    }

    /// Journals a committed turn after `durable` has stored its task batch.
    ///
    /// The commit is validated (and an injected failure consumed) before
    /// `durable` runs, so a commit this journal would reject never reaches
    /// durable storage, and a failed `durable` leaves the journal unchanged.
    /// Writers must be serialized (the turn engine is the only writer), so
    /// nothing can invalidate the commit while `durable` runs.
    pub async fn commit_with<F, Fut>(
        &self,
        commit: TurnCommit,
        durable: F,
    ) -> Result<Vec<TranscriptEvent>, JournalError>
    where
        F: FnOnce(Vec<PlayerTask>) -> Fut,
        Fut: Future<Output = Result<(), JournalError>>,
    {
        {
            let mut state = self.state.lock().expect("journal lock");
            Self::inject_failure(&mut state)?;
            Self::validate(&state, false, &commit.record, &commit.events)?;
        }
        durable(commit.task_mutations.clone()).await?;
        let mut state = self.state.lock().expect("journal lock");
        Self::validate(&state, false, &commit.record, &commit.events)?;
        Ok(Self::append(
            &mut state,
            Some(commit.task_mutations),
            commit.record,
            commit.events,
        ))
    }

    fn inject_failure(state: &mut MemoryState) -> Result<(), JournalError> {
        if state.fail_next_writes > 0 {
            state.fail_next_writes -= 1;
            return Err(JournalError::Storage("injected write failure".to_string()));
        }
        Ok(())
    }

    /// Checks a write against the stored state without changing it.
    fn validate(
        state: &MemoryState,
        accept: bool,
        record: &RequestRecord,
        events: &[PendingEvent],
    ) -> Result<(), JournalError> {
        let known = state.requests.contains_key(&record.id);
        if accept && known {
            return Err(JournalError::DuplicateRequest(record.id.clone()));
        }
        if !accept && !known {
            return Err(JournalError::UnknownRequest(record.id.clone()));
        }
        for event in events {
            if let Some(&index) = state.event_index.get(&event.id)
                && state.events[index].event != *event
            {
                return Err(JournalError::ConflictingEvent(event.id.clone()));
            }
        }
        Ok(())
    }

    /// Appends a validated write.
    fn append(
        state: &mut MemoryState,
        tasks: Option<Vec<PlayerTask>>,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> Vec<TranscriptEvent> {
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
        if let Some(tasks) = tasks {
            state.task_batches.push(tasks);
        }
        let order = state.accepted_order.len();
        state
            .accepted_order
            .entry(record.id.clone())
            .or_insert(order);
        state.requests.insert(record.id.clone(), record);
        stored
    }

    fn write(
        &self,
        kind: WriteKind,
        record: RequestRecord,
        events: Vec<PendingEvent>,
    ) -> Result<Vec<TranscriptEvent>, JournalError> {
        let mut state = self.state.lock().expect("journal lock");
        Self::inject_failure(&mut state)?;
        Self::validate(&state, matches!(kind, WriteKind::Accept), &record, &events)?;
        let tasks = match kind {
            WriteKind::Commit {
                tasks,
                state: has_state,
            } => {
                state.committed_states += usize::from(has_state);
                Some(tasks)
            }
            WriteKind::Accept | WriteKind::Update => None,
        };
        Ok(Self::append(&mut state, tasks, record, events))
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
            WriteKind::Commit {
                tasks: commit.task_mutations,
                state: commit.state.is_some(),
            },
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

    fn requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>> {
        let state = self.state.lock().expect("journal lock");
        let mut records: Vec<RequestRecord> = state.requests.values().cloned().collect();
        records.sort_by_key(|record| state.accepted_order.get(&record.id).copied());
        Box::pin(async move { Ok(records) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::turn::journal_contract::{self, ContractJournal, accepted, committed, one_task};

    impl ContractJournal for MemoryTurnJournal {
        fn fail_next_writes(&self, count: usize) {
            MemoryTurnJournal::fail_next_writes(self, count);
        }

        fn stored_events(&self) -> Vec<TranscriptEvent> {
            self.events()
        }

        fn stored_request(&self, id: &LogicalRequestId) -> Option<RequestRecord> {
            self.request(id)
        }

        fn durable_task_count(&self) -> usize {
            self.task_batches().iter().map(Vec::len).sum()
        }

        fn durable_state_count(&self) -> usize {
            self.committed_states()
        }
    }

    journal_contract::journal_contract_tests!(MemoryTurnJournal::new());

    #[tokio::test]
    async fn a_failed_durable_write_leaves_the_memory_journal_unchanged() {
        let journal = MemoryTurnJournal::new();
        let (mut record, command) = accepted("r1");
        journal.accept(record.clone(), vec![command]).await.unwrap();
        let commit = committed(&mut record, one_task());
        journal.update(record.clone(), Vec::new()).await.unwrap();

        let error = journal
            .commit_with(commit.clone(), |_| async {
                Err(JournalError::Storage("disk full".to_string()))
            })
            .await
            .unwrap_err();
        assert_eq!(error, JournalError::Storage("disk full".to_string()));
        assert_eq!(journal.events().len(), 1);
        assert!(journal.task_batches().is_empty());
        assert!(!journal.request(&record.id).unwrap().has_committed());

        // A commit the journal would reject never reaches durable storage.
        let unknown = TurnCommit {
            record: accepted("never-accepted").0,
            ..commit.clone()
        };
        let reached = std::sync::atomic::AtomicBool::new(false);
        let rejected = journal
            .commit_with(unknown, |_| async {
                reached.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            })
            .await;
        assert!(matches!(rejected, Err(JournalError::UnknownRequest(_))));
        assert!(!reached.load(std::sync::atomic::Ordering::SeqCst));

        let tasks = std::sync::Mutex::new(Vec::new());
        journal
            .commit_with(commit, |batch| async {
                tasks.lock().unwrap().extend(batch);
                Ok(())
            })
            .await
            .unwrap();
        assert_eq!(tasks.lock().unwrap().len(), 1);
        assert_eq!(journal.task_batches().len(), 1);
        assert!(journal.request(&record.id).unwrap().has_committed());
    }
}
