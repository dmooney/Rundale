//! The [`TurnJournal`] contract as one test suite every implementation runs.
//!
//! An implementation provides [`ContractJournal`] (fault injection and
//! read-back) and instantiates the suite with
//! [`journal_contract_tests!`]`(constructor)`.

use limerick_types::PlayerTask;

use super::ids::{ExecutionAttemptId, LogicalRequestId, StateRevision};
use super::journal::{JournalError, TurnCommit, TurnJournal};
use super::lifecycle::{RequestPhase, RequestRecord, TerminalOutcome};
use super::transcript::{EventBuilder, PendingEvent, TranscriptEvent, TranscriptEventKind};
use crate::persistence::GameSnapshot;

/// What the contract suite needs besides the trait itself.
pub(crate) trait ContractJournal: TurnJournal {
    /// Makes the next `count` writes fail after doing all their work, so
    /// the implementation must undo every part of them.
    fn fail_next_writes(&self, count: usize);
    /// Every stored event, in sequence order.
    fn stored_events(&self) -> Vec<TranscriptEvent>;
    /// The stored record of `id`.
    fn stored_request(&self, id: &LogicalRequestId) -> Option<RequestRecord>;
    /// Task post-states durably written by commits.
    fn durable_task_count(&self) -> usize;
    /// Authoritative states durably written by commits.
    fn durable_state_count(&self) -> usize;
}

/// A freshly accepted request and its command event.
pub(crate) fn accepted(id: &str) -> (RequestRecord, PendingEvent) {
    let record = RequestRecord::accept(LogicalRequestId::new(id), "hello", Vec::new(), None);
    let mut builder = EventBuilder::new(record.id.clone(), None, 0);
    let mut command = builder.event(TranscriptEventKind::PlayerCommand);
    command.content = Some("hello".to_string());
    command.item_id = Some(record.command_item());
    (record, command)
}

/// Starts `record`'s first attempt and returns the commit that ends it.
pub(crate) fn committed(record: &mut RequestRecord, tasks: Vec<PlayerTask>) -> TurnCommit {
    let attempt = ExecutionAttemptId::new("a1");
    record
        .begin_attempt(attempt.clone(), StateRevision(0))
        .unwrap();
    let mut started = record.clone();
    started.complete(StateRevision(1)).unwrap();
    let mut builder = EventBuilder::new(record.id.clone(), Some(attempt), 0);
    let mut line = builder.event(TranscriptEventKind::NpcDialogue);
    line.content = Some("God bless ye.".to_string());
    let done = builder.response_completed(TerminalOutcome::Succeeded, Some(StateRevision(1)));
    TurnCommit {
        record: started,
        events: vec![line, done],
        task_mutations: tasks,
        state: Some(state()),
    }
}

/// An authoritative state to commit.
pub(crate) fn state() -> GameSnapshot {
    GameSnapshot::capture(
        &crate::world::WorldState::new(),
        &crate::npc::manager::NpcManager::new(),
    )
}

/// One freshly assigned player task.
pub(crate) fn one_task() -> Vec<PlayerTask> {
    let mut progress = limerick_types::PlayerProgress::default();
    progress
        .assign_task(
            "Dig over the potato patch.",
            crate::npc::NpcId(7),
            crate::world::LocationId(1),
            chrono::Utc::now(),
        )
        .unwrap();
    progress.tasks().to_vec()
}

pub(crate) async fn sequences_strictly_increase_across_requests(journal: &impl ContractJournal) {
    let (first, first_command) = accepted("r1");
    let (mut second, second_command) = accepted("r2");
    let a = journal.accept(first, vec![first_command]).await.unwrap();
    let b = journal
        .accept(second.clone(), vec![second_command])
        .await
        .unwrap();
    let commit = committed(&mut second, Vec::new());
    journal.update(second, Vec::new()).await.unwrap();
    let c = journal.commit(commit).await.unwrap();
    assert!(a[0].sequence < b[0].sequence);
    assert!(b[0].sequence < c[0].sequence && c[0].sequence < c[1].sequence);
    let stored = journal.stored_events();
    assert!(
        stored
            .windows(2)
            .all(|pair| pair[0].sequence < pair[1].sequence),
        "stored in strictly increasing sequence order"
    );
}

// Oracle: ios-port persistence `duplicate_events_are_idempotent_but_conflicting_payload_is_rejected`.
pub(crate) async fn duplicate_events_are_idempotent_but_conflicts_reject_the_whole_write(
    journal: &impl ContractJournal,
) {
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
    assert_eq!(journal.stored_events().len(), 1);

    let mut builder = EventBuilder::new(record.id.clone(), Some(ExecutionAttemptId::new("a1")), 0);
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
    assert_eq!(journal.stored_events().len(), 1, "no partial write");
    assert_eq!(
        journal.stored_request(&record.id).unwrap().phase,
        RequestPhase::Accepted,
        "the record update is rolled back too"
    );
}

// Oracle: ios-port persistence `sqlite_failure_rolls_back_generation_state_request_and_event_together`.
pub(crate) async fn a_failed_commit_writes_nothing_and_an_unchanged_retry_commits_once(
    journal: &impl ContractJournal,
) {
    let (mut record, command) = accepted("r1");
    journal.accept(record.clone(), vec![command]).await.unwrap();
    let commit = committed(&mut record, one_task());
    journal.update(record.clone(), Vec::new()).await.unwrap();

    journal.fail_next_writes(1);
    assert!(journal.commit(commit.clone()).await.is_err());
    assert_eq!(journal.stored_events().len(), 1);
    assert_eq!(journal.durable_task_count(), 0);
    assert_eq!(journal.durable_state_count(), 0);
    assert_eq!(
        journal.stored_request(&record.id).unwrap().phase,
        RequestPhase::Executing
    );

    let stored = journal.commit(commit.clone()).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(journal.durable_task_count(), 1);
    assert_eq!(journal.durable_state_count(), 1);
    let replayed = journal.commit(commit).await.unwrap();
    assert_eq!(replayed, stored, "a replayed commit appends no new event");
    assert_eq!(journal.stored_events().len(), 3);
    assert!(journal.stored_request(&record.id).unwrap().has_committed());
}

pub(crate) async fn failed_accepts_and_updates_write_nothing(journal: &impl ContractJournal) {
    let (mut record, command) = accepted("r1");
    journal.fail_next_writes(1);
    assert!(
        journal
            .accept(record.clone(), vec![command.clone()])
            .await
            .is_err()
    );
    assert!(journal.stored_request(&record.id).is_none());
    assert!(journal.stored_events().is_empty());

    journal.accept(record.clone(), vec![command]).await.unwrap();
    record
        .begin_attempt(ExecutionAttemptId::new("a1"), StateRevision(0))
        .unwrap();
    let mut builder = EventBuilder::new(record.id.clone(), Some(ExecutionAttemptId::new("a1")), 0);
    let progress = builder.event(TranscriptEventKind::Progress);
    journal.fail_next_writes(1);
    assert!(
        journal
            .update(record.clone(), vec![progress.clone()])
            .await
            .is_err()
    );
    assert_eq!(journal.stored_events().len(), 1);
    assert_eq!(
        journal.stored_request(&record.id).unwrap().phase,
        RequestPhase::Accepted
    );
    journal
        .update(record.clone(), vec![progress])
        .await
        .unwrap();
    assert_eq!(journal.stored_events().len(), 2);
}

pub(crate) async fn accept_rejects_a_known_request_and_update_rejects_an_unknown_one(
    journal: &impl ContractJournal,
) {
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
        JournalError::UnknownRequest(stranger.id.clone())
    );
    let mut stranger = stranger;
    let commit = committed(&mut stranger, Vec::new());
    assert_eq!(
        journal.commit(commit).await.unwrap_err(),
        JournalError::UnknownRequest(stranger.id)
    );
}

pub(crate) async fn open_requests_lists_only_non_terminal_records(journal: &impl ContractJournal) {
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

pub(crate) async fn requests_lists_every_record_in_accepted_order(journal: &impl ContractJournal) {
    let (mut later, later_command) = accepted("b-later");
    let (earlier, earlier_command) = accepted("z-earlier");
    journal
        .accept(earlier.clone(), vec![earlier_command])
        .await
        .unwrap();
    journal
        .accept(later.clone(), vec![later_command])
        .await
        .unwrap();
    let commit = committed(&mut later, Vec::new());
    journal.update(later, Vec::new()).await.unwrap();
    let completed = commit.record.clone();
    journal.commit(commit).await.unwrap();
    assert_eq!(
        journal.requests().await.unwrap(),
        vec![earlier, completed],
        "every record, latest state, in the order first accepted"
    );
}

pub(crate) async fn events_without_a_request_are_journaled_in_sequence_and_idempotently(
    journal: &impl ContractJournal,
) {
    let mut opening = PendingEvent {
        id: super::ids::TranscriptEventId::new("opening"),
        request_id: None,
        attempt_id: None,
        item_id: None,
        kind: TranscriptEventKind::Narration,
        speaker: None,
        content: Some("Morning in the village.".to_string()),
        terminal_outcome: None,
        state_revision: None,
        clarification: None,
        metadata: Default::default(),
    };
    let stored = journal.record(vec![opening.clone()]).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(journal.record(vec![opening.clone()]).await.unwrap(), stored);

    let (record, command) = accepted("r1");
    let after = journal.accept(record, vec![command]).await.unwrap();
    assert!(stored[0].sequence < after[0].sequence);
    assert!(
        journal.requests().await.unwrap().len() == 1,
        "no request record"
    );

    opening.content = Some("Evening.".to_string());
    assert_eq!(
        journal.record(vec![opening.clone()]).await.unwrap_err(),
        JournalError::ConflictingEvent(opening.id.clone())
    );
    journal.fail_next_writes(1);
    let mut later = opening.clone();
    later.id = super::ids::TranscriptEventId::new("later");
    assert!(journal.record(vec![later.clone()]).await.is_err());
    assert_eq!(
        journal.stored_events().len(),
        2,
        "a failed record writes nothing"
    );

    let (owned, _) = accepted("r2");
    later.request_id = Some(owned.id);
    assert!(
        matches!(
            journal.record(vec![later]).await,
            Err(JournalError::Storage(_))
        ),
        "an event naming a request is journaled with that request"
    );
}

/// Instantiates the contract suite for the journal `$make` constructs.
macro_rules! journal_contract_tests {
    ($make:expr) => {
        mod contract {
            #[allow(unused_imports)]
            use super::*;
            use $crate::turn::journal_contract as suite;

            #[tokio::test]
            async fn sequences_strictly_increase_across_requests() {
                suite::sequences_strictly_increase_across_requests(&$make).await;
            }

            #[tokio::test]
            async fn duplicate_events_are_idempotent_but_conflicts_reject_the_whole_write() {
                suite::duplicate_events_are_idempotent_but_conflicts_reject_the_whole_write(&$make)
                    .await;
            }

            #[tokio::test]
            async fn a_failed_commit_writes_nothing_and_an_unchanged_retry_commits_once() {
                suite::a_failed_commit_writes_nothing_and_an_unchanged_retry_commits_once(&$make)
                    .await;
            }

            #[tokio::test]
            async fn failed_accepts_and_updates_write_nothing() {
                suite::failed_accepts_and_updates_write_nothing(&$make).await;
            }

            #[tokio::test]
            async fn accept_rejects_a_known_request_and_update_rejects_an_unknown_one() {
                suite::accept_rejects_a_known_request_and_update_rejects_an_unknown_one(&$make)
                    .await;
            }

            #[tokio::test]
            async fn open_requests_lists_only_non_terminal_records() {
                suite::open_requests_lists_only_non_terminal_records(&$make).await;
            }

            #[tokio::test]
            async fn events_without_a_request_are_journaled_in_sequence_and_idempotently() {
                suite::events_without_a_request_are_journaled_in_sequence_and_idempotently(&$make)
                    .await;
            }

            #[tokio::test]
            async fn requests_lists_every_record_in_accepted_order() {
                suite::requests_lists_every_record_in_accepted_order(&$make).await;
            }
        }
    };
}
pub(crate) use journal_contract_tests;
